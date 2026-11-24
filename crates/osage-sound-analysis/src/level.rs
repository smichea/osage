// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! How strong a signal is, and whether it is safe to play: its peaks — at
//! the samples and between them —, its mean level, its offset, and the
//! samples a 16-bit file could not hold.
//!
//! Levels are in decibels relative to full scale, of mean squares: a square
//! wave between −1 and 1 reads 0 dB, a sine of the same peaks −3.01 dB.
//! Peaks are in decibels of the largest magnitude: both waves peak at 0 dB.

/// What silence reads, in decibels: a floor rather than minus infinity, so
/// that a difference of two levels is always a number.
pub const SILENCE_DB: f64 = -180.0;

/// The level in decibels of a mean square (or of any power).
pub fn db(power: f64) -> f64 {
    10.0 * power.max(1e-18).log10()
}

/// The level in decibels of a magnitude: a peak, an amplitude.
pub fn amplitude_db(amplitude: f64) -> f64 {
    db(amplitude * amplitude)
}

/// The largest magnitude among the samples.
pub fn sample_peak(signal: &[f32]) -> f64 {
    signal.iter().fold(0.0, |peak, &s| f64::max(peak, f64::from(s).abs()))
}

/// The mean of the squares of the samples.
pub fn mean_square(signal: &[f32]) -> f64 {
    if signal.is_empty() {
        return 0.0;
    }
    signal.iter().map(|&s| f64::from(s) * f64::from(s)).sum::<f64>() / signal.len() as f64
}

/// The root of the mean square.
pub fn rms(signal: &[f32]) -> f64 {
    mean_square(signal).sqrt()
}

/// The mean of the samples: the offset a loudspeaker would be held at.
pub fn dc(signal: &[f32]) -> f64 {
    if signal.is_empty() {
        return 0.0;
    }
    signal.iter().map(|&s| f64::from(s)).sum::<f64>() / signal.len() as f64
}

/// How alike two channels are at the same instant: 1 for the same signal,
/// −1 for a signal and its opposite, 0 for unrelated ones or for silence.
/// Offsets are not removed: this is what a correlation meter shows.
pub fn correlation(left: &[f32], right: &[f32]) -> f64 {
    let (mut both, mut l2, mut r2) = (0.0, 0.0, 0.0);
    for (&l, &r) in left.iter().zip(right) {
        let (l, r) = (f64::from(l), f64::from(r));
        both += l * r;
        l2 += l * l;
        r2 += r * r;
    }
    let scale = (l2 * r2).sqrt();
    if scale > 0.0 { both / scale } else { 0.0 }
}

/// The largest magnitude a 16-bit sample holds, 32 767 of 32 768: a sample
/// there or beyond was cut, or will be.
pub const PCM16_LIMIT: f64 = 32_767.0 / 32_768.0;

/// Samples at the limits of a 16-bit file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clipping {
    /// How many samples sit at a limit ([`PCM16_LIMIT`]) or beyond.
    pub samples: usize,
    /// The longest run of them in a row: one or two happen to a loud signal
    /// that was never cut, a long run is a flat top that is heard.
    pub longest_run: usize,
}

/// The samples of a signal at the limits of a 16-bit file.
pub fn clipping(signal: &[f32]) -> Clipping {
    let mut found = Clipping::default();
    let mut run = 0;
    for &s in signal {
        if f64::from(s).abs() >= PCM16_LIMIT {
            run += 1;
            found.samples += 1;
            found.longest_run = found.longest_run.max(run);
        } else {
            run = 0;
        }
    }
    found
}

/// The filter of ITU-R BS.1770-4 (annex 2) that finds three points between
/// each pair of samples: four phases of twelve taps, in the order of the
/// standard's table.
const TRUE_PEAK: [[f64; 12]; 4] = [
    [
        0.001_708_984_375,
        0.010_986_328_125,
        -0.019_653_320_312_5,
        0.033_203_125,
        -0.059_448_242_187_5,
        0.137_329_101_562_5,
        0.972_167_968_75,
        -0.102_294_921_875,
        0.047_607_421_875,
        -0.026_611_328_125,
        0.014_892_578_125,
        -0.008_300_781_25,
    ],
    [
        -0.029_174_804_687_5,
        0.029_296_875,
        -0.051_757_812_5,
        0.089_111_328_125,
        -0.166_503_906_25,
        0.465_087_890_625,
        0.779_785_156_25,
        -0.200_317_382_812_5,
        0.101_562_5,
        -0.058_227_539_062_5,
        0.033_081_054_687_5,
        -0.018_920_898_437_5,
    ],
    [
        -0.018_920_898_437_5,
        0.033_081_054_687_5,
        -0.058_227_539_062_5,
        0.101_562_5,
        -0.200_317_382_812_5,
        0.779_785_156_25,
        0.465_087_890_625,
        -0.166_503_906_25,
        0.089_111_328_125,
        -0.051_757_812_5,
        0.029_296_875,
        -0.029_174_804_687_5,
    ],
    [
        -0.008_300_781_25,
        0.014_892_578_125,
        -0.026_611_328_125,
        0.047_607_421_875,
        -0.102_294_921_875,
        0.972_167_968_75,
        0.137_329_101_562_5,
        -0.059_448_242_187_5,
        0.033_203_125,
        -0.019_653_320_312_5,
        0.010_986_328_125,
        0.001_708_984_375,
    ],
];

/// Outputs of the true-peak filter looked at together, so that quiet
/// stretches are stepped over a block at a time.
const BLOCK: usize = 16;

/// The largest magnitude of the wave the samples stand for, between them as
/// well as at them: the true peak of ITU-R BS.1770-4, by its 48-tap filter
/// that computes the wave four times as finely.
///
/// The filter is defined relative to the sampling rate — it rebuilds
/// everything under half of it — so it is applied as it is at any rate: a
/// file at 44.1 kHz is looked at 176 400 times a second, one at 48 kHz
/// 192 000 times. The samples themselves count too, which the standard does
/// not say: its filter passes beside them, and a peak cannot be lower than
/// a sample.
///
/// Four points a sample and a filter of 48 taps make this an estimate, good
/// to a tenth of a decibel or two: a sine of full scale at a quarter of the
/// rate, whose tops fall midway between samples, reads +0.08 dBTP.
pub fn true_peak(signal: &[f32]) -> f64 {
    // No output can exceed the largest input it sees times the sum of the
    // magnitudes of a phase's taps: a block whose inputs are that far under
    // the peak found so far need not be computed.
    let reach = TRUE_PEAK.iter().map(|phase| phase.iter().map(|tap| tap.abs()).sum::<f64>()).fold(0.0, f64::max);
    let tops: Vec<f64> = signal.chunks(BLOCK).map(sample_peak).collect();
    let top = |block: usize| tops.get(block).copied().unwrap_or(0.0);
    // Each phase backwards: output[n] = Σ phase[k]·x[n − k] is then a sum
    // along the samples as they come.
    let backwards = TRUE_PEAK.map(|mut phase| {
        phase.reverse();
        phase
    });
    let mut peak = sample_peak(signal);
    // The last eleven outputs are past the last sample.
    let blocks = (signal.len() + 11).div_ceil(BLOCK);
    let mut seen = [0.0f64; BLOCK + 11];
    for block in 0..blocks {
        let near = if block > 0 { top(block).max(top(block - 1)) } else { top(block) };
        if near * reach <= peak {
            continue;
        }
        // seen[i] is the sample at block·BLOCK − 11 + i, zero outside the signal.
        for (i, slot) in seen.iter_mut().enumerate() {
            *slot = (block * BLOCK + i).checked_sub(11).and_then(|at| signal.get(at)).map_or(0.0, |&s| f64::from(s));
        }
        for twelve in seen.windows(12) {
            for phase in &backwards {
                let value: f64 = phase.iter().zip(twelve).map(|(tap, sample)| tap * sample).sum();
                peak = peak.max(value.abs());
            }
        }
    }
    peak
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{sine, white};
    use std::f64::consts::{FRAC_PI_4, TAU};

    /// The same filter over every sample, without stepping over anything.
    fn true_peak_plainly(signal: &[f32]) -> f64 {
        let mut peak = sample_peak(signal);
        for n in 0..signal.len() + 11 {
            for phase in &TRUE_PEAK {
                let mut value = 0.0;
                for (k, tap) in phase.iter().enumerate() {
                    if n >= k && n - k < signal.len() {
                        value += tap * f64::from(signal[n - k]);
                    }
                }
                peak = peak.max(value.abs());
            }
        }
        peak
    }

    #[test]
    fn the_filter_s_table_is_the_standard_s() {
        // What the standard's table shows: each phase is another one backwards,
        // and each keeps a steady level nearly as it is — the outer two to
        // 0.2 %, the inner two 2.7 % low, as printed.
        for (phase, mirror) in [(0, 3), (1, 2)] {
            let mut backwards = TRUE_PEAK[mirror];
            backwards.reverse();
            assert_eq!(TRUE_PEAK[phase], backwards);
        }
        let sums = TRUE_PEAK.map(|phase| phase.iter().sum::<f64>());
        assert!((sums[0] - 1.001_6).abs() < 1e-4 && (sums[1] - 0.973_0).abs() < 1e-4, "{sums:?}");
    }

    #[test]
    fn decibels_of_powers_and_of_magnitudes() {
        assert!((db(0.5) + 3.0103).abs() < 1e-4);
        assert!((amplitude_db(0.5) + 6.0206).abs() < 1e-4);
        assert_eq!(db(0.0), SILENCE_DB);
        assert_eq!(db(0.0) - db(0.0), 0.0);
    }

    #[test]
    fn a_full_scale_sine_at_a_quarter_of_the_rate_peaks_3_db_between_its_samples() {
        // Samples fall at 45°, 135°, 225°, 315°: all at 0.707, the tops in between.
        let signal: Vec<f32> = (0..4_800).map(|n| (TAU * n as f64 / 4.0 + FRAC_PI_4).sin() as f32).collect();
        let (sample, between) = (amplitude_db(sample_peak(&signal)), amplitude_db(true_peak(&signal)));
        assert!((sample + 3.0103).abs() < 1e-3, "sample peak {sample}");
        // The nearest of the four points is 11.25° from the top (−0.17 dB),
        // and the filter is not quite flat there (+0.25 dB).
        assert!((between - sample - 3.0103).abs() < 0.1, "true peak {between}, sample peak {sample}");
        assert!(between.abs() < 0.1);
    }

    #[test]
    fn the_true_peak_of_a_slow_sine_is_its_amplitude() {
        for rate in [44_100, 48_000] {
            let signal = sine(rate, 997.0, 0.5, 0.2);
            let peak = amplitude_db(true_peak(&signal));
            assert!((peak - amplitude_db(0.5)).abs() < 0.01, "{peak} dBTP at {rate} Hz");
        }
    }

    #[test]
    fn the_true_peak_is_never_under_the_sample_peak() {
        let signal = white(21, 48_000, 0.5, 0.8);
        assert!(true_peak(&signal) >= sample_peak(&signal));
        assert_eq!(true_peak(&[]), 0.0);
        assert_eq!(true_peak(&[0.0; 100]), 0.0);
        // A lone sample rings on both sides, and none of it is higher.
        let mut click = vec![0.0f32; 64];
        click[20] = 0.9;
        assert!((true_peak(&click) - f64::from(0.9f32)).abs() < 1e-12);
    }

    #[test]
    fn stepping_over_quiet_blocks_changes_no_true_peak() {
        // Noise that swells and fades, so that most blocks are stepped over.
        let mut signal = white(8, 48_000, 0.4, 1.0);
        let count = signal.len();
        for (n, s) in signal.iter_mut().enumerate() {
            *s *= (0.02 + (TAU * 3.0 * n as f64 / count as f64).sin().abs()) as f32 * 0.9;
        }
        assert_eq!(true_peak(&signal), true_peak_plainly(&signal));
        // A peak in the last samples, whose ringing falls after the end.
        let mut ending = vec![0.0f32; 1_000];
        ending[998] = 0.5;
        ending[999] = -0.5;
        assert_eq!(true_peak(&ending), true_peak_plainly(&ending));
        assert!(true_peak(&ending) > 0.5);
    }

    #[test]
    fn samples_at_the_16_bit_limits_are_counted_and_their_longest_run() {
        let top = 32_767.0 / 32_768.0;
        let signal = [0.0, top, top, 0.5, -1.0, -1.0, -1.0, top, 0.999, 1.2, 0.0];
        assert_eq!(clipping(&signal), Clipping { samples: 7, longest_run: 4 });
        assert_eq!(clipping(&sine(48_000, 440.0, 0.99, 0.1)), Clipping { samples: 0, longest_run: 0 });
        // A sine driven 6 dB too hard and cut: flat tops a third of a turn long.
        let cut: Vec<f32> = sine(48_000, 480.0, 2.0, 0.1).iter().map(|s| s.clamp(-1.0, top)).collect();
        let found = clipping(&cut);
        assert!((32..=34).contains(&found.longest_run), "{found:?}");
        assert!(found.samples > 3_000);
    }

    #[test]
    fn the_offset_and_the_level_of_a_sine_riding_on_an_offset() {
        let signal: Vec<f32> = sine(48_000, 1_000.0, 0.5, 1.0).iter().map(|s| s + 0.1).collect();
        assert!((dc(&signal) - 0.1).abs() < 1e-6);
        assert!((mean_square(&signal) - (0.125 + 0.01)).abs() < 1e-6);
        assert!((rms(&signal) - 0.135f64.sqrt()).abs() < 1e-6);
        assert!((sample_peak(&signal) - 0.6).abs() < 1e-6);
        assert_eq!((dc(&[]), rms(&[]), sample_peak(&[])), (0.0, 0.0, 0.0));
    }

    #[test]
    fn a_signal_correlates_fully_with_itself_and_not_with_another() {
        let (a, b) = (white(1, 48_000, 1.0, 0.5), white(2, 48_000, 1.0, 0.5));
        let opposite: Vec<f32> = a.iter().map(|s| -s).collect();
        assert!((correlation(&a, &a) - 1.0).abs() < 1e-12);
        assert!((correlation(&a, &opposite) + 1.0).abs() < 1e-12);
        assert!(correlation(&a, &b).abs() < 0.02);
        assert_eq!(correlation(&a, &[0.0; 48_000]), 0.0);
    }
}
