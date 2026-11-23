// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! A signal sampled at another rate: what lets a measure defined at 48 kHz
//! (loudness) read a file made at 44.1 kHz.
//!
//! Each output sample is a sum of the input samples around its instant,
//! weighed by the sinc that rebuilds a band-limited wave, cut short by a
//! Kaiser window. The band kept is that of the lower of the two rates.

use std::f64::consts::PI;

/// Zero crossings of the sinc kept on each side, at the lower of the two
/// rates. With [`BETA`], what cannot be carried is 90 dB down, and the wave
/// is kept to within 0.01 dB up to nine tenths of the lower rate's half.
const CROSSINGS: usize = 32;
/// The shape of the Kaiser window.
const BETA: f64 = 9.0;

/// The modified Bessel function of the first kind and order zero, by its
/// series.
fn bessel_i0(x: f64) -> f64 {
    let (mut sum, mut term, mut k) = (1.0, 1.0, 1.0);
    while term > 1e-17 * sum {
        term *= (x / (2.0 * k)).powi(2);
        sum += term;
        k += 1.0;
    }
    sum
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// `signal`, sampled `from` times a second, as it would be sampled `to`
/// times a second: as long in seconds, and without what the new rate cannot
/// carry. The sound before the first sample and after the last is taken as
/// silence.
pub fn resample(signal: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || from == 0 || to == 0 || signal.is_empty() {
        return signal.to_vec();
    }
    let divisor = gcd(from, to);
    // Output n falls at input position n·down/up: a whole sample and one of
    // `up` fractions, each fraction with its own weights.
    let (up, down) = ((to / divisor) as usize, (from / divisor) as usize);
    // The band kept, as a part of the input's: all of it when going up.
    let band = (f64::from(to) / f64::from(from)).min(1.0);
    let half = (CROSSINGS as f64 / band).ceil() as usize;
    let taps = 2 * half;
    let mut weights = vec![0.0f64; up * taps];
    for (phase, row) in weights.chunks_exact_mut(taps).enumerate() {
        let fraction = phase as f64 / up as f64;
        for (t, weight) in row.iter_mut().enumerate() {
            // From the output's instant to input sample (whole − half + 1 + t).
            let distance = t as f64 - (half as f64 - 1.0) - fraction;
            let across = distance / half as f64;
            if across.abs() >= 1.0 {
                continue;
            }
            let x = PI * band * distance;
            let sinc = if x.abs() < 1e-9 { 1.0 } else { x.sin() / x };
            *weight = sinc * bessel_i0(BETA * (1.0 - across * across).sqrt());
        }
        // A steady level stays exactly what it was.
        let sum: f64 = row.iter().sum();
        for weight in row.iter_mut() {
            *weight /= sum;
        }
    }
    let count = (signal.len() as u64 * up as u64 / down as u64) as usize;
    let mut out = Vec::with_capacity(count);
    for n in 0..count {
        let position = n as u64 * down as u64;
        let (whole, phase) = ((position / up as u64) as usize, (position % up as u64) as usize);
        let row = &weights[phase * taps..(phase + 1) * taps];
        let value: f64 = if whole + 1 >= half && whole + half < signal.len() {
            let around = &signal[whole + 1 - half..whole + 1 + half];
            row.iter().zip(around).map(|(weight, &s)| weight * f64::from(s)).sum()
        } else {
            row.iter()
                .enumerate()
                .filter_map(|(t, weight)| {
                    let at = (whole + 1 + t).checked_sub(half)?;
                    Some(weight * f64::from(*signal.get(at)?))
                })
                .sum()
        };
        out.push(value as f32);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        level::{db, mean_square},
        testing::sine,
    };
    use std::f64::consts::TAU;

    /// The largest gap between `signal` and a sine of `hz` and `amplitude`,
    /// away from the ends.
    fn gap(signal: &[f32], rate: u32, hz: f64, amplitude: f64) -> f64 {
        let inner = 200..signal.len() - 200;
        inner
            .map(|n| (f64::from(signal[n]) - amplitude * (TAU * hz * n as f64 / f64::from(rate)).sin()).abs())
            .fold(0.0, f64::max)
    }

    #[test]
    fn a_sine_keeps_its_level_its_pitch_and_its_phase_from_44_1_to_48_khz() {
        for hz in [50.0, 997.0, 10_000.0, 19_000.0] {
            let out = resample(&sine(44_100, hz, 0.5, 0.5), 44_100, 48_000);
            assert_eq!(out.len(), 24_000);
            let error = gap(&out, 48_000, hz, 0.5);
            assert!(error < 1e-4, "{hz} Hz is off by {error}");
        }
    }

    #[test]
    fn the_same_rate_gives_the_same_samples() {
        let signal = sine(48_000, 440.0, 0.3, 0.01);
        assert_eq!(resample(&signal, 48_000, 48_000), signal);
        assert!(resample(&[], 44_100, 48_000).is_empty());
    }

    #[test]
    fn a_steady_level_stays_exactly_where_it_was() {
        let out = resample(&[0.25; 4_000], 44_100, 48_000);
        for value in &out[100..out.len() - 100] {
            assert!((value - 0.25).abs() < 1e-7);
        }
    }

    #[test]
    fn going_down_removes_what_the_lower_rate_cannot_carry() {
        let kept = resample(&sine(96_000, 1_000.0, 0.5, 0.25), 96_000, 48_000);
        assert_eq!(kept.len(), 12_000);
        assert!(gap(&kept, 48_000, 1_000.0, 0.5) < 1e-4);
        // 30 kHz would come back as 18 kHz at 48 kHz, were it let through.
        let removed = resample(&sine(96_000, 30_000.0, 0.5, 0.25), 96_000, 48_000);
        let level = db(mean_square(&removed[200..11_800]));
        assert!(level < -90.0, "what should be gone reads {level} dB");
    }
}
