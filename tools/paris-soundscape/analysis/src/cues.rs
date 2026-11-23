// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Where a stereo recording places its sounds: the cues a listener's two
//! ears compare, read from the two channels after Faller and Merimaa (2004).
//!
//! In a band of frequencies and over a stretch of time, three figures:
//!
//! - the **level difference** (ILD), dB: positive when the RIGHT channel is
//!   the louder;
//! - the **time difference** (ITD), microseconds, within ±1 ms: positive
//!   when the sound reaches the RIGHT ear first — that is, when the left
//!   channel is the late one;
//! - the **coherence**, 0…1: how alike the two channels are once the time
//!   difference is taken out. 1 for one source heard clearly, less when
//!   several sources or reflections mix, near 0 for a sound that comes from
//!   everywhere.
//!
//! So a source on the listener's right reads positive on both differences.
//!
//! The paper correlates the two ears' signals, band by band, over every lag
//! within ±1 ms: the lag where the correlation peaks is the time difference,
//! the height of the peak the coherence, the ratio of the two powers the
//! level difference. Here the same correlation is computed from the spectra
//! of frames of about 50 ms: the product of the left spectrum by the
//! conjugate of the right one, kept to the band's bins, is the transform of
//! that correlation, and can be evaluated at any lag, between samples too.
//! The paper's model of the inner ear (envelopes, compression) is left out:
//! what is measured is the signals, not what a nerve would do with them.

use crate::{
    bands::Band,
    fft::{Complex, Stft, frame_size},
    level::db,
};
use std::f64::consts::TAU;

/// The length of a frame, seconds: 2 048 samples at 44.1 and at 48 kHz.
/// Frames overlap by half.
pub const CUE_FRAME_S: f64 = 0.05;
/// The longest time difference looked for, seconds: more than a head makes
/// (0.66 ms), as in the paper.
pub const LONGEST_ITD_S: f64 = 0.001;
/// The band the ear reads time differences in from the wave itself, Hz:
/// above 1.5 kHz a wave is shorter than a head, and its lag is ambiguous.
pub const FINE_STRUCTURE: Band = Band::new(100.0, 1_500.0);
/// The band of the broadband time difference ([`gcc_phat`]), Hz.
pub const PHAT_BAND: Band = Band::new(100.0, 8_000.0);

/// The octave bands the cues are usually read in: 125 Hz to 8 kHz.
pub fn cue_bands() -> Vec<Band> {
    (-3..=3).map(|n| Band::octave(1_000.0 * 10f64.powf(0.3 * f64::from(n)))).collect()
}

/// What the two channels say of a band over a stretch of time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cue {
    /// The middle of the stretch, seconds.
    pub time: f64,
    /// Level difference, dB: positive when the right channel is louder.
    pub ild_db: f64,
    /// Time difference, microseconds: positive when the right channel leads.
    /// Only meaningful when [`Cue::coherence`] is high.
    pub itd_us: f64,
    /// Coherence, 0…1.
    pub coherence: f64,
    /// The level of the band, dB: the two channels' mean squares, summed.
    /// A quiet stretch has cues of no meaning: this is what tells it.
    pub level_db: f64,
}

/// A broadband time difference.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimeDifference {
    /// The middle of the stretch, seconds.
    pub time: f64,
    /// Microseconds: positive when the right channel leads.
    pub itd_us: f64,
    /// The height of the correlation's peak, 0…1: 1 when one channel is a
    /// late copy of the other at every frequency.
    pub peak: f64,
}

/// The correlation of the two channels at a lag of `lag` samples, from the
/// cross spectrum `cross` of bins `first`, `first + 1`… of frames of `size`
/// samples: `Σ Re(cross[k]·e^(2πi·k·lag/size))`.
fn correlation(cross: &[Complex], first: usize, size: usize, lag: f64) -> f64 {
    let step = Complex::turn(TAU * lag / size as f64);
    let mut turn = Complex::turn(TAU * lag * first as f64 / size as f64);
    let mut sum = 0.0;
    for value in cross {
        sum += value.re * turn.re - value.im * turn.im;
        turn = turn * step;
    }
    sum
}

/// The same at a lag of a whole number of samples, where every turn is one
/// of the `size` points of `circle` (`e^(2πi·n/size)`): most of the lags
/// looked at are whole, and reading a table is quicker than turning.
fn correlation_at_sample(cross: &[Complex], first: usize, circle: &[Complex], lag: isize) -> f64 {
    let mask = circle.len() - 1;
    let step = lag.rem_euclid(circle.len() as isize) as usize;
    let mut at = (first * step) & mask;
    let mut sum = 0.0;
    for value in cross {
        let turn = circle[at];
        sum += value.re * turn.re - value.im * turn.im;
        at = (at + step) & mask;
    }
    sum
}

/// What is left of the correlation of a signal with a copy `lag` samples
/// late when both are cut by the same Hann window: the copy is windowed
/// `lag` samples off, and reads less alike than it is. The loss is the same
/// at every frequency, so it lowers the peak without moving it.
fn window_loss(size: usize, lag: f64) -> f64 {
    (2.0 + (TAU * lag / size as f64).cos()) / 3.0
}

/// How near the highest a peak must be to be taken for as high: within one
/// part in a million.
const AS_HIGH: f64 = 1e-6;

/// The lag, in samples within ±`limit`, where the correlation peaks, and
/// its height there. A pure tone has a peak every period, and nothing tells
/// them apart: among peaks as high as each other, the nearest to zero.
fn peak(cross: &[Complex], first: usize, circle: &[Complex], limit: f64) -> (f64, f64) {
    let at = |lag: f64| correlation(cross, first, circle.len(), lag);
    // Whole samples first…
    let reach = limit.floor() as isize;
    let whole: Vec<f64> = (-reach..=reach).map(|lag| correlation_at_sample(cross, first, circle, lag)).collect();
    let top = whole.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mut best = (0.0, top);
    let mut nearest = isize::MAX;
    for (n, &height) in whole.iter().enumerate() {
        let lag = n as isize - reach;
        let is_peak = (n == 0 || height >= whole[n - 1]) && (n + 1 == whole.len() || height >= whole[n + 1]);
        if is_peak && height >= top - AS_HIGH * top.abs() && lag.abs() < nearest {
            nearest = lag.abs();
            best = (lag as f64, height);
        }
    }
    // …then quarters of a sample around the best, where a parabola through
    // three points is true to the curve even at the top of the band.
    let around = best.0;
    for quarter in [-3.0f64, -2.0, -1.0, 1.0, 2.0, 3.0] {
        let lag = (around + quarter * 0.25).clamp(-limit, limit);
        let height = at(lag);
        if height > best.1 {
            best = (lag, height);
        }
    }
    let (before, after) = (at(best.0 - 0.25), at(best.0 + 0.25));
    let bend = before - 2.0 * best.1 + after;
    if bend < 0.0 {
        let lag = (best.0 + (0.125 * (before - after) / bend).clamp(-0.25, 0.25)).clamp(-limit, limit);
        let height = at(lag);
        if height > best.1 {
            best = (lag, height);
        }
    }
    best
}

/// What is summed over the frames of a stretch, bin by bin.
struct Sums {
    /// Left times the conjugate of right.
    cross: Vec<Complex>,
    left: Vec<f64>,
    right: Vec<f64>,
    /// The same product with each bin brought to a length of one (PHAT).
    white: Vec<Complex>,
    frames: usize,
    white_frames: usize,
    /// The sum of the centres of the frames, samples.
    centres: f64,
}

impl Sums {
    fn new(bins: usize) -> Sums {
        Sums {
            cross: vec![Complex::ZERO; bins],
            left: vec![0.0; bins],
            right: vec![0.0; bins],
            white: vec![Complex::ZERO; bins],
            frames: 0,
            white_frames: 0,
            centres: 0.0,
        }
    }

    fn clear(&mut self) {
        *self = Sums::new(self.cross.len());
    }
}

/// The bins of frames of `size` samples whose frequency lies in `band`:
/// never the bin of 0 Hz nor that of half the rate, which have no phase.
fn bins_of(band: &Band, rate: u32, size: usize) -> (usize, usize) {
    let hz_a_bin = f64::from(rate) / size as f64;
    let first = ((band.low / hz_a_bin).ceil().max(1.0)) as usize;
    let end = ((band.high / hz_a_bin).ceil().max(1.0) as usize).min(size / 2);
    (first.min(end), end)
}

/// The cues of each band and, when asked, the broadband time difference,
/// for each stretch of `step_s` seconds: both from one pass over the signal.
pub(crate) fn analyse(
    left: &[f32],
    right: &[f32],
    rate: u32,
    bands: &[Band],
    step_s: f64,
    phat: bool,
) -> (Vec<Vec<Cue>>, Vec<TimeDifference>) {
    let size = frame_size(rate, CUE_FRAME_S);
    let (mut for_left, mut for_right) = (Stft::new(size, size / 2), Stft::new(size, size / 2));
    let length = left.len().min(right.len());
    let (left, right) = (&left[..length], &right[..length]);
    let rate_hz = f64::from(rate);
    let limit = LONGEST_ITD_S * rate_hz;
    let step = step_s.max(for_left.hop() as f64 / rate_hz);
    let ranges: Vec<(usize, usize)> = bands.iter().map(|band| bins_of(band, rate, size)).collect();
    let (white_first, white_end) = bins_of(&PHAT_BAND, rate, size);
    let circle: Vec<Complex> = (0..size).map(|n| Complex::turn(TAU * n as f64 / size as f64)).collect();
    let mut sums = Sums::new(for_left.bins());
    let mut cues = vec![Vec::new(); bands.len()];
    let mut differences = Vec::new();

    let mut close = |sums: &mut Sums| {
        if sums.frames == 0 {
            return;
        }
        let time = sums.centres / sums.frames as f64 / rate_hz;
        for (&(first, end), cues) in ranges.iter().zip(&mut cues) {
            let (l, r): (f64, f64) = (sums.left[first..end].iter().sum(), sums.right[first..end].iter().sum());
            let scale = (l * r).sqrt();
            let (lag, height) =
                if scale > 0.0 { peak(&sums.cross[first..end], first, &circle, limit) } else { (0.0, 0.0) };
            cues.push(Cue {
                time,
                ild_db: db(r / sums.frames as f64) - db(l / sums.frames as f64),
                itd_us: lag / rate_hz * 1e6,
                coherence: if scale > 0.0 { (height / window_loss(size, lag) / scale).clamp(0.0, 1.0) } else { 0.0 },
                level_db: db((l + r) / sums.frames as f64),
            });
        }
        if phat {
            let count = (white_end - white_first) * sums.white_frames;
            let (lag, height) = if count > 0 {
                peak(&sums.white[white_first..white_end], white_first, &circle, limit)
            } else {
                (0.0, 0.0)
            };
            let peak = if count > 0 { (height / count as f64).clamp(0.0, 1.0) } else { 0.0 };
            differences.push(TimeDifference { time, itd_us: lag / rate_hz * 1e6, peak });
        }
        sums.clear();
    };

    let mut stretch = 0;
    for frame in 0..for_left.steps(length) {
        let centre = for_left.centre(frame) as f64;
        let of = (centre / rate_hz / step).floor() as usize;
        if of != stretch {
            close(&mut sums);
            stretch = of;
        }
        let (l, weight) = for_left.spectrum(left, for_left.start(frame));
        let (r, _) = for_right.spectrum(right, for_right.start(frame));
        // Mean squares, as in `Stft::power`: frames at the ends of the
        // signal weigh as much as the others.
        let scale = if weight.squares > 0.0 { 2.0 / (size as f64 * weight.squares) } else { 0.0 };
        for (k, (l, r)) in l.iter().zip(r).enumerate() {
            sums.cross[k] = sums.cross[k] + (*l * r.conj()).scale(scale);
            sums.left[k] += l.power() * scale;
            sums.right[k] += r.power() * scale;
        }
        if phat {
            // Bins far under the frame's mean hold no sound whose phase
            // could be trusted: the floor leaves them out of the sum.
            let lengths: Vec<f64> = (white_first..white_end).map(|k| (l[k] * r[k].conj()).abs()).collect();
            let floor = 1e-3 * lengths.iter().sum::<f64>() / lengths.len().max(1) as f64;
            if floor > 0.0 {
                for (k, length) in (white_first..white_end).zip(lengths) {
                    sums.white[k] = sums.white[k] + (l[k] * r[k].conj()).scale(1.0 / (length + floor));
                }
                sums.white_frames += 1;
            }
        }
        sums.frames += 1;
        sums.centres += centre;
    }
    close(&mut sums);
    (cues, differences)
}

/// The cues of each of `bands` along a stereo signal: `cues[band][n]`, one
/// every `step_s` seconds.
///
/// Each value sums the frames (of [`CUE_FRAME_S`], half overlapped) whose
/// centre falls in its step; a step shorter than the 21 ms between frames
/// gives one value a frame. One frame holds few independent values in a
/// narrow band, so that two unrelated noises still read a coherence of 0.15
/// to 0.35 frame by frame: coherence near 0 shows over many frames — steps
/// of a second, or [`cue`] over a stretch. A source that moves during a
/// step blurs its own peak, and reads less coherent than it is.
pub fn cues(left: &[f32], right: &[f32], rate: u32, bands: &[Band], step_s: f64) -> Vec<Vec<Cue>> {
    analyse(left, right, rate, bands, step_s, false).0
}

/// The samples of a signal between two instants.
fn between(signal: &[f32], rate: u32, from_s: f64, to_s: f64) -> &[f32] {
    let at = |seconds: f64| ((seconds.max(0.0) * f64::from(rate)).round() as usize).min(signal.len());
    let (from, to) = (at(from_s), at(to_s));
    &signal[from.min(to)..to]
}

/// The cues of one band between two instants, all frames summed: what to
/// assert on in a test — "from 10 s to 12 s, between 500 Hz and 2 kHz, the
/// right is 3 dB louder".
pub fn cue(left: &[f32], right: &[f32], rate: u32, band: &Band, from_s: f64, to_s: f64) -> Cue {
    let (l, r) = (between(left, rate, from_s, to_s), between(right, rate, from_s, to_s));
    let silence = Cue { time: 0.0, ild_db: 0.0, itd_us: 0.0, coherence: 0.0, level_db: db(0.0) };
    let mut found = analyse(l, r, rate, &[*band], f64::INFINITY, false).0[0].first().copied().unwrap_or(silence);
    found.time += from_s.max(0.0);
    found
}

/// The broadband time difference along a stereo signal, one value every
/// `step_s` seconds, by the generalised cross-correlation with phase
/// transform (Knapp and Carter, 1976): every frequency of [`PHAT_BAND`]
/// counts for as much, however little sound it carries, so that the peak is
/// sharp even for a sound as coloured as tyres on a road. Same sign as
/// [`Cue::itd_us`].
pub fn gcc_phat(left: &[f32], right: &[f32], rate: u32, step_s: f64) -> Vec<TimeDifference> {
    analyse(left, right, rate, &[], step_s, true).1
}

/// The broadband time difference between two instants, all frames summed.
pub fn gcc_phat_between(left: &[f32], right: &[f32], rate: u32, from_s: f64, to_s: f64) -> TimeDifference {
    let (l, r) = (between(left, rate, from_s, to_s), between(right, rate, from_s, to_s));
    let mut found = analyse(l, r, rate, &[], f64::INFINITY, true).1.first().copied().unwrap_or(TimeDifference {
        time: 0.0,
        itd_us: 0.0,
        peak: 0.0,
    });
    found.time += from_s.max(0.0);
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{Chord, late, sine, white};

    #[test]
    fn a_copy_half_a_millisecond_late_and_6_db_lower_on_the_right_reads_minus_500_us_and_minus_6_db() {
        // The sound reaches the left ear first, and louder: a source on the left.
        let left = white(31, 48_000, 2.0, 0.5);
        let right = late(&left, 24, 0.5);
        for band in cue_bands() {
            let found = cue(&left, &right, 48_000, &band, 0.0, 2.0);
            assert!((found.itd_us + 500.0).abs() < 1.0, "{} µs at {} Hz", found.itd_us, band.centre());
            assert!((found.ild_db + 6.02).abs() < 0.05, "{} dB at {} Hz", found.ild_db, band.centre());
            assert!(found.coherence > 0.995, "coherence {} at {} Hz", found.coherence, band.centre());
            assert!((found.time - 1.0).abs() < 0.02);
        }
        let broad = gcc_phat_between(&left, &right, 48_000, 0.0, 2.0);
        assert!((broad.itd_us + 500.0).abs() < 1.0, "{} µs", broad.itd_us);
        assert!(broad.peak > 0.9, "peak {}", broad.peak);
    }

    #[test]
    fn the_same_copy_on_the_left_reads_plus_500_us_and_plus_6_db() {
        // The left channel lags and is the weaker: a source on the right.
        let right = white(32, 48_000, 2.0, 0.5);
        let left = late(&right, 24, 0.5);
        for band in cue_bands() {
            let found = cue(&left, &right, 48_000, &band, 0.0, 2.0);
            assert!((found.itd_us - 500.0).abs() < 1.0, "{} µs at {} Hz", found.itd_us, band.centre());
            assert!((found.ild_db - 6.02).abs() < 0.05, "{} dB at {} Hz", found.ild_db, band.centre());
        }
        let broad = gcc_phat_between(&left, &right, 48_000, 0.0, 2.0);
        assert!((broad.itd_us - 500.0).abs() < 1.0, "{} µs", broad.itd_us);
    }

    #[test]
    fn identical_channels_are_coherent_and_differ_in_nothing() {
        let signal = white(33, 48_000, 1.0, 0.5);
        for band in cue_bands() {
            for found in &cues(&signal, &signal, 48_000, &[band], 0.0)[0] {
                assert_eq!((found.ild_db, found.itd_us), (0.0, 0.0));
                assert!((found.coherence - 1.0).abs() < 1e-9);
            }
        }
        // A pure tone correlates as well one period away: zero is still what is read.
        let tone = sine(48_000, 4_000.0, 0.3, 0.5);
        let found = cue(&tone, &tone, 48_000, &Band::octave(4_000.0), 0.0, 0.5);
        assert_eq!((found.ild_db, found.itd_us), (0.0, 0.0));
        let broad = gcc_phat_between(&signal, &signal, 48_000, 0.0, 1.0);
        assert_eq!(broad.itd_us, 0.0);
        assert!((broad.peak - 1.0).abs() < 0.01);
    }

    #[test]
    fn unrelated_noises_have_a_coherence_near_zero_over_a_few_seconds() {
        let (left, right) = (white(34, 48_000, 4.0, 0.5), white(35, 48_000, 4.0, 0.5));
        for band in cue_bands() {
            let found = cue(&left, &right, 48_000, &band, 0.0, 4.0);
            assert!(found.coherence < 0.2, "coherence {} at {} Hz", found.coherence, band.centre());
            assert!(found.ild_db.abs() < 1.0, "{} dB at {} Hz", found.ild_db, band.centre());
        }
        let wide = cue(&left, &right, 48_000, &Band::new(500.0, 8_000.0), 0.0, 4.0);
        assert!(wide.coherence < 0.03, "coherence {}", wide.coherence);
        assert!(gcc_phat_between(&left, &right, 48_000, 0.0, 4.0).peak < 0.05);
    }

    #[test]
    fn a_lag_of_whole_samples_reads_the_same_from_the_table_as_by_turning() {
        let mut noise = crate::testing::Noise::new(41);
        let cross: Vec<Complex> =
            (0..37).map(|_| Complex::new(f64::from(noise.next()), f64::from(noise.next()))).collect();
        let circle: Vec<Complex> = (0..256).map(|n| Complex::turn(TAU * n as f64 / 256.0)).collect();
        for lag in [-48isize, -7, -1, 0, 1, 2, 33, 48] {
            let (table, turned) =
                (correlation_at_sample(&cross, 5, &circle, lag), correlation(&cross, 5, 256, lag as f64));
            assert!((table - turned).abs() < 1e-11, "lag {lag}: {table} against {turned}");
        }
    }

    #[test]
    fn a_delay_between_two_samples_is_read_to_a_microsecond_at_both_rates() {
        for rate in [44_100, 48_000] {
            for (low, high) in [(100.0, 1_500.0), (1_500.0, 6_000.0)] {
                let chord = Chord::new(40, low, high, 60);
                // 260 µs: 12.48 samples at 48 kHz, 11.47 at 44.1 kHz.
                let (left, right) = (chord.render(rate, 1.0, 260e-6, 0.3), chord.render(rate, 1.0, 0.0, 0.2));
                let found = cue(&left, &right, rate, &Band::new(low, high), 0.0, 1.0);
                assert!((found.itd_us - 260.0).abs() < 1.0, "{} µs in {low}…{high} Hz at {rate} Hz", found.itd_us);
                assert!((found.ild_db - 20.0 * (0.2f64 / 0.3).log10()).abs() < 0.05, "{} dB", found.ild_db);
                assert!(found.coherence > 0.995, "coherence {}", found.coherence);
                let broad = gcc_phat_between(&left, &right, rate, 0.0, 1.0);
                assert!((broad.itd_us - 260.0).abs() < 3.0, "{} µs by PHAT at {rate} Hz", broad.itd_us);
            }
        }
    }

    #[test]
    fn a_delay_longer_than_a_head_makes_is_held_at_the_limit() {
        let left = white(36, 48_000, 1.0, 0.5);
        let right = late(&left, 96, 1.0);
        let found = cue(&left, &right, 48_000, &Band::new(500.0, 4_000.0), 0.0, 1.0);
        assert!(found.itd_us.abs() <= 1_000.0 + 1e-9);
        assert!(found.coherence < 0.3, "coherence {}", found.coherence);
    }

    #[test]
    fn a_source_that_crosses_changes_the_sign_of_both_differences_on_the_way() {
        // Right and early for a second, then left and early for a second.
        let source = white(37, 48_000, 2.0, 0.5);
        let (mut left, mut right) = (late(&source, 20, 0.6), source.clone());
        for n in 48_000..96_000 {
            left[n] = source[n];
            right[n] = source[n - 20] * 0.6;
        }
        let found = &cues(&left, &right, 48_000, &[Band::new(200.0, 1_500.0)], 0.25)[0];
        assert_eq!(found.len(), 8);
        for value in found {
            // The quarter of a second that holds the crossing is neither.
            if (value.time - 1.0).abs() > 0.15 {
                let side = if value.time < 1.0 { 1.0 } else { -1.0 };
                assert!((value.itd_us - side * 416.7).abs() < 2.0, "{} µs at {} s", value.itd_us, value.time);
                assert!((value.ild_db - side * 4.44).abs() < 0.1, "{} dB at {} s", value.ild_db, value.time);
            }
        }
    }

    #[test]
    fn a_frame_tells_by_its_level_that_it_holds_nothing() {
        let mut left = vec![0.0f32; 24_000];
        left.extend(white(38, 48_000, 0.5, 0.5));
        let right = left.clone();
        let band = Band::octave(1_000.0);
        let found = &cues(&left, &right, 48_000, &[band], 0.0)[0];
        // A value a frame: frames are 1 024 samples apart.
        assert_eq!(found.len(), 48_000 / 1_024);
        assert!((found[1].time - found[0].time - 1_024.0 / 48_000.0).abs() < 1e-12);
        for value in found {
            if value.time < 0.45 {
                assert!(value.level_db < -150.0 && value.coherence == 0.0 && value.itd_us == 0.0);
            } else if value.time > 0.55 {
                // Each channel reads −26 dB in this octave.
                assert!(value.level_db > -40.0, "{} dB at {} s", value.level_db, value.time);
            }
        }
        let empty = cue(&left, &right, 48_000, &band, 0.0, 0.4);
        assert!(empty.level_db < -150.0 && empty.ild_db == 0.0);
        assert_eq!(cue(&[], &[], 48_000, &band, 0.0, 1.0).coherence, 0.0);
        assert!(cues(&[], &[], 48_000, &[band], 0.05)[0].is_empty());
        assert!(gcc_phat(&[], &[], 48_000, 0.05).is_empty());
    }

    #[test]
    fn steps_of_a_second_start_on_the_second() {
        let signal = white(39, 48_000, 3.0, 0.5);
        let found = &cues(&signal, &signal, 48_000, &[Band::octave(1_000.0)], 1.0)[0];
        assert_eq!(found.len(), 3);
        for (second, value) in found.iter().enumerate() {
            assert!((value.time - (second as f64 + 0.5)).abs() < 0.011, "{} s", value.time);
        }
        assert_eq!(gcc_phat(&signal, &signal, 48_000, 1.0).len(), 3);
    }
}
