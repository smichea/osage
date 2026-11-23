// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Where in frequency a sound's energy lies: its level in octave and
//! third-octave bands, over a whole signal and over time, and the centre of
//! its spectrum.
//!
//! A band's level is the sum of the bins of a spectrum that fall in it, a
//! bin that straddles an edge being shared by what lies on each side. Bands
//! that touch therefore share a signal without losing or counting twice any
//! of it: together they hold its whole mean square.

use crate::{
    fft::{Stft, frame_size},
    level::db,
};

/// A band of frequencies, Hz: `low` included, `high` not.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub low: f64,
    pub high: f64,
}

impl Band {
    pub const fn new(low: f64, high: f64) -> Band {
        Band { low, high }
    }

    /// The octave around `centre`: from `centre/√2` to `centre·√2`, taking 2
    /// as 10^0.3 as the standard for filters does (IEC 61260-1).
    pub fn octave(centre: f64) -> Band {
        Band { low: centre * 10f64.powf(-0.15), high: centre * 10f64.powf(0.15) }
    }

    /// The third of an octave around `centre`.
    pub fn third_octave(centre: f64) -> Band {
        Band { low: centre * 10f64.powf(-0.05), high: centre * 10f64.powf(0.05) }
    }

    /// The frequency in the middle of the band, as the ear hears a middle:
    /// as many octaves from each edge.
    pub fn centre(&self) -> f64 {
        (self.low * self.high).sqrt()
    }

    pub fn width(&self) -> f64 {
        self.high - self.low
    }
}

/// The names of the ten octave bands, 31.5 Hz to 16 kHz, in the order of
/// [`octaves`].
pub const OCTAVE_NAMES: [f64; 10] = [31.5, 63.0, 125.0, 250.0, 500.0, 1_000.0, 2_000.0, 4_000.0, 8_000.0, 16_000.0];

/// The names of the thirty third-octave bands, 25 Hz to 20 kHz, in the order
/// of [`third_octaves`].
pub const THIRD_OCTAVE_NAMES: [f64; 30] = [
    25.0, 31.5, 40.0, 50.0, 63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0, 315.0, 400.0, 500.0, 630.0, 800.0, 1_000.0,
    1_250.0, 1_600.0, 2_000.0, 2_500.0, 3_150.0, 4_000.0, 5_000.0, 6_300.0, 8_000.0, 10_000.0, 12_500.0, 16_000.0,
    20_000.0,
];

/// The ten octave bands, 31.5 Hz to 16 kHz. Their exact centres are
/// `1000·10^(0.3·n)` Hz: the names ([`OCTAVE_NAMES`]) are round figures.
/// The last one reaches 22.4 kHz: a file at 44.1 kHz holds all of it but its
/// last 340 Hz.
pub fn octaves() -> Vec<Band> {
    (-5..=4).map(|n| Band::octave(1_000.0 * 10f64.powf(0.3 * f64::from(n)))).collect()
}

/// The thirty third-octave bands, 25 Hz to 20 kHz, three to each octave of
/// [`octaves`]: centres `1000·10^(0.1·n)` Hz.
pub fn third_octaves() -> Vec<Band> {
    (-16..=13).map(|n| Band::third_octave(1_000.0 * 10f64.powf(0.1 * f64::from(n)))).collect()
}

/// A band must be this many bins wide to be read cleanly: a Hann window
/// spreads a pure tone over four bins.
const BINS_A_BAND: f64 = 4.0;
/// The longest frame, seconds: 32 768 samples. Bands narrower than
/// [`BINS_A_BAND`] bins of it (the third octaves under 40 Hz) are read with
/// some of their neighbours in them.
const LONGEST_FRAME_S: f64 = 0.68;
/// The frame of what needs no fine frequency (the centre of a spectrum),
/// seconds: 4 096 samples.
const SHORT_FRAME_S: f64 = 0.085;

/// The part of `power` (mean squares at bins `hz_a_bin` apart) that lies in
/// `band`.
pub(crate) fn band_power(power: &[f64], hz_a_bin: f64, band: &Band) -> f64 {
    if power.is_empty() || band.high <= band.low {
        return 0.0;
    }
    // Bin k stands for the frequencies half a bin on each side of k·hz_a_bin.
    let first = ((band.low / hz_a_bin - 0.5).ceil().max(0.0)) as usize;
    let last = ((band.high / hz_a_bin + 0.5).floor().max(0.0) as usize).min(power.len() - 1);
    let mut sum = 0.0;
    for (k, value) in power.iter().enumerate().take(last + 1).skip(first) {
        let (from, to) = (((k as f64 - 0.5) * hz_a_bin).max(0.0), (k as f64 + 0.5) * hz_a_bin);
        let shared = (to.min(band.high) - from.max(band.low)).max(0.0);
        sum += value * shared / (to - from);
    }
    sum
}

/// The frame that reads `band` cleanly at `rate`: long enough for the band
/// to be [`BINS_A_BAND`] bins wide, between `shortest` and the longest.
fn frame_for(band: &Band, rate: u32, shortest: usize) -> usize {
    let longest = frame_size(rate, LONGEST_FRAME_S).max(shortest);
    let wanted = BINS_A_BAND * f64::from(rate) / band.width().max(1e-3);
    (wanted.min(longest as f64).ceil() as usize).next_power_of_two().clamp(shortest, longest)
}

/// The mean spectrum of a signal: its mean square at each bin of frames of
/// `size` samples, averaged over whole frames half a frame apart. A signal
/// shorter than a frame is seen through the longest frame it fills.
fn mean_spectrum(signal: &[f32], size: usize) -> (Vec<f64>, usize) {
    let mut size = size;
    while size > 2 && size > signal.len() {
        size /= 2;
    }
    let mut stft = Stft::new(size, size / 2);
    let mut mean = vec![0.0; stft.bins()];
    let mut power = Vec::new();
    let mut frames = 0.0;
    // The only frame of a signal shorter than two samples reaches outside it.
    let starts: Vec<isize> = stft.whole_frames(signal.len()).collect();
    for start in if starts.is_empty() { vec![0] } else { starts } {
        stft.power(signal, start, &mut power);
        for (sum, value) in mean.iter_mut().zip(&power) {
            *sum += value;
        }
        frames += 1.0;
    }
    for value in &mut mean {
        *value /= frames;
    }
    (mean, size)
}

/// The level of a signal in each of `bands`, dB, over the whole of it.
pub fn band_levels(signal: &[f32], rate: u32, bands: &[Band]) -> Vec<f64> {
    if signal.is_empty() {
        return bands.iter().map(|_| db(0.0)).collect();
    }
    let shortest = frame_size(rate, SHORT_FRAME_S);
    let size = bands.iter().map(|band| frame_for(band, rate, shortest)).max().unwrap_or(shortest);
    let (mean, size) = mean_spectrum(signal, size);
    let hz_a_bin = f64::from(rate) / size as f64;
    bands.iter().map(|band| db(band_power(&mean, hz_a_bin, band))).collect()
}

/// The levels of bands along a signal.
#[derive(Clone, Debug, PartialEq)]
pub struct BandLevels {
    /// The instant each value is centred on, seconds.
    pub times: Vec<f64>,
    /// `levels[band][step]`, dB.
    pub levels: Vec<Vec<f64>>,
}

/// The level of a signal in each of `bands`, every `step_s` seconds
/// (0.05, say).
///
/// A level cannot be both sharp in frequency and sharp in time: a band 22 Hz
/// wide needs a third of a second to be told from its neighbours. So each
/// band is read through the shortest window that resolves it — a frame of
/// 0.17 s for the octaves from 63 Hz up, 0.34 s for the one at 31.5 Hz, when
/// the step is 50 ms — and its values, though given every step, each speak
/// of that much time around their instant.
pub fn band_levels_over_time(signal: &[f32], rate: u32, bands: &[Band], step_s: f64) -> BandLevels {
    let hop = ((step_s * f64::from(rate)).round() as usize).max(1);
    // Windows must overlap by half at least, for every sample to count.
    let shortest = (2 * hop).next_power_of_two();
    let sizes: Vec<usize> = bands.iter().map(|band| frame_for(band, rate, shortest)).collect();
    let steps = Stft::new(shortest, hop).steps(signal.len());
    let times = (0..steps).map(|step| (step * hop + hop / 2) as f64 / f64::from(rate)).collect();
    let mut levels = vec![vec![0.0; steps]; bands.len()];
    let mut distinct = sizes.clone();
    distinct.sort_unstable();
    distinct.dedup();
    let mut power = Vec::new();
    for size in distinct {
        let mut stft = Stft::new(size, hop);
        let hz_a_bin = f64::from(rate) / size as f64;
        for step in 0..steps {
            stft.power(signal, stft.start(step), &mut power);
            for ((band, levels), _) in bands.iter().zip(&mut levels).zip(&sizes).filter(|(_, of)| **of == size) {
                levels[step] = db(band_power(&power, hz_a_bin, band));
            }
        }
    }
    BandLevels { times, levels }
}

/// The frequency a spectrum balances at, Hz, and its mean square.
fn balance(power: &[f64], hz_a_bin: f64) -> (f64, f64) {
    let total: f64 = power.iter().sum();
    let weighed: f64 = power.iter().enumerate().map(|(k, value)| k as f64 * hz_a_bin * value).sum();
    (if total > 0.0 { weighed / total } else { 0.0 }, total)
}

/// The centre of a signal's spectrum, Hz: the mean of its frequencies, each
/// weighed by the energy there. A brighter sound has a higher centre; white
/// noise has it at a quarter of the rate. 0 for silence.
pub fn spectral_centroid(signal: &[f32], rate: u32) -> f64 {
    if signal.is_empty() {
        return 0.0;
    }
    let (mean, size) = mean_spectrum(signal, frame_size(rate, SHORT_FRAME_S));
    balance(&mean, f64::from(rate) / size as f64).0
}

/// The centre of a signal's spectrum every `step_s` seconds: (seconds, Hz,
/// level in dB) — the level, so that the centre of near silence can be left
/// out.
pub fn spectral_centroid_over_time(signal: &[f32], rate: u32, step_s: f64) -> Vec<(f64, f64, f64)> {
    let hop = ((step_s * f64::from(rate)).round() as usize).max(1);
    let size = frame_size(rate, SHORT_FRAME_S).max((2 * hop).next_power_of_two());
    let mut stft = Stft::new(size, hop);
    let mut power = Vec::new();
    (0..stft.steps(signal.len()))
        .map(|step| {
            stft.power(signal, stft.start(step), &mut power);
            let (hz, total) = balance(&power, f64::from(rate) / size as f64);
            (stft.centre(step) as f64 / f64::from(rate), hz, db(total))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        level::{SILENCE_DB, mean_square},
        testing::{sine, white},
    };

    #[test]
    fn octaves_and_thirds_are_named_by_their_centres_and_touch() {
        let (octaves, thirds) = (octaves(), third_octaves());
        assert_eq!((octaves.len(), thirds.len()), (10, 30));
        for (band, name) in octaves.iter().zip(OCTAVE_NAMES) {
            assert!((band.centre() / name - 1.0).abs() < 0.01, "{name}: {}", band.centre());
            assert!((band.high / band.low - 2.0).abs() < 0.01);
        }
        for (band, name) in thirds.iter().zip(THIRD_OCTAVE_NAMES) {
            assert!((band.centre() / name - 1.0).abs() < 0.03, "{name}: {}", band.centre());
        }
        for pair in octaves.windows(2).chain(thirds.windows(2)) {
            assert!((pair[0].high / pair[1].low - 1.0).abs() < 1e-12);
        }
        // Three thirds make an octave, edge for edge.
        for (n, octave) in octaves.iter().enumerate() {
            assert!((thirds[3 * n].low / octave.low - 1.0).abs() < 1e-12);
            assert!((thirds[3 * n + 2].high / octave.high - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn a_sine_reads_its_level_in_its_own_band_and_60_db_less_elsewhere() {
        for rate in [44_100, 48_000] {
            let levels = band_levels(&sine(rate, 1_000.0, 0.1, 3.0), rate, &octaves());
            for (n, level) in levels.iter().enumerate() {
                if n == 5 {
                    // A sine of peak 0.1: mean square 0.005, −23.01 dB.
                    assert!((level + 23.01).abs() < 0.02, "{level} dB at {rate} Hz");
                } else {
                    assert!(*level < -83.0, "{} Hz reads {level} dB", OCTAVE_NAMES[n]);
                }
            }
            let thirds = band_levels(&sine(rate, 63.0, 0.1, 3.0), rate, &third_octaves());
            assert!((thirds[4] + 23.01).abs() < 0.02, "{}", thirds[4]);
        }
    }

    #[test]
    fn white_noise_gains_3_db_an_octave() {
        let levels = band_levels(&white(9, 48_000, 20.0, 0.5), 48_000, &octaves());
        for (n, pair) in levels.windows(2).enumerate() {
            assert!((pair[1] - pair[0] - 3.01).abs() < 0.8, "from {} Hz: {pair:?}", OCTAVE_NAMES[n]);
        }
        // Uniform noise of ±0.5 has a mean square of 1/12 spread evenly up
        // to 24 kHz: the octave at 1 kHz is 705 Hz of it.
        let expected = db(Band::octave(1_000.0).width() / 24_000.0 / 12.0);
        assert!((levels[5] - expected).abs() < 0.3, "{} against {expected}", levels[5]);
    }

    #[test]
    fn bands_that_touch_hold_together_the_whole_signal() {
        let signal = white(12, 48_000, 4.0, 0.5);
        let whole = db(mean_square(&signal));
        for bands in [vec![Band::new(0.0, 24_000.0)], vec![Band::new(0.0, 517.3), Band::new(517.3, 24_000.0)]] {
            let sum: f64 = band_levels(&signal, 48_000, &bands).iter().map(|level| 10f64.powf(level / 10.0)).sum();
            assert!((db(sum) - whole).abs() < 0.1, "{} against {whole}", db(sum));
        }
    }

    #[test]
    fn silence_and_a_band_out_of_reach_read_the_floor() {
        assert_eq!(band_levels(&[], 48_000, &octaves()), vec![SILENCE_DB; 10]);
        assert_eq!(band_levels(&[0.0; 10_000], 48_000, &[Band::new(100.0, 200.0)]), vec![SILENCE_DB]);
        let out_of_reach = band_levels(&white(1, 48_000, 1.0, 0.5), 48_000, &[Band::new(30_000.0, 40_000.0)]);
        assert_eq!(out_of_reach, vec![SILENCE_DB]);
        // A signal shorter than any frame is still read.
        let short = band_levels(&sine(48_000, 4_000.0, 0.1, 0.01), 48_000, &[Band::octave(4_000.0)]);
        assert!((short[0] + 23.0).abs() < 0.5, "{short:?}");
    }

    #[test]
    fn a_band_over_time_follows_a_sound_that_starts_half_way() {
        // One second of silence, then one of a 2 kHz sine; a 100 Hz hum all along.
        let mut signal = vec![0.0f32; 48_000];
        signal.extend(sine(48_000, 2_000.0, 0.2, 1.0));
        for (s, hum) in signal.iter_mut().zip(sine(48_000, 100.0, 0.05, 2.0)) {
            *s += hum;
        }
        let bands = [Band::octave(2_000.0), Band::octave(100.0)];
        let found = band_levels_over_time(&signal, 48_000, &bands, 0.05);
        assert_eq!((found.times.len(), found.levels.len(), found.levels[0].len()), (40, 2, 40));
        assert!((found.times[0] - 0.025).abs() < 1e-9 && (found.times[39] - 1.975).abs() < 1e-9);
        for (step, &time) in found.times.iter().enumerate() {
            let (high, low) = (found.levels[0][step], found.levels[1][step]);
            if time < 0.85 {
                assert!(high < -70.0, "{high} dB at {time} s");
            } else if time > 1.15 {
                assert!((high - db(0.02)).abs() < 0.1, "{high} dB at {time} s");
            }
            // The hum is as strong at the ends, where the window reaches outside.
            assert!((low - db(0.001_25)).abs() < 0.5, "the hum reads {low} dB at {time} s");
        }
    }

    #[test]
    fn low_bands_are_read_through_longer_windows_than_high_ones() {
        let shortest = (2usize * 2_400).next_power_of_two();
        assert_eq!(frame_for(&Band::octave(31.5), 48_000, shortest), 16_384);
        assert_eq!(frame_for(&Band::octave(63.0), 48_000, shortest), 8_192);
        assert_eq!(frame_for(&Band::octave(8_000.0), 48_000, shortest), 8_192);
        assert_eq!(frame_for(&Band::third_octave(25.0), 48_000, shortest), 32_768);
        assert_eq!(frame_for(&Band::octave(8_000.0), 48_000, 1_024), 1_024);
    }

    #[test]
    fn the_centre_of_a_sine_is_its_frequency_and_that_of_white_noise_a_quarter_of_the_rate() {
        let centre = spectral_centroid(&sine(48_000, 3_000.0, 0.3, 1.0), 48_000);
        assert!((centre - 3_000.0).abs() < 1.0, "{centre} Hz");
        let centre = spectral_centroid(&white(3, 48_000, 5.0, 0.5), 48_000);
        assert!((centre - 12_000.0).abs() < 150.0, "{centre} Hz");
        assert_eq!(spectral_centroid(&[0.0; 5_000], 48_000), 0.0);
        assert_eq!(spectral_centroid(&[], 48_000), 0.0);
    }

    #[test]
    fn the_centre_over_time_rises_when_the_sound_brightens() {
        let mut signal = sine(48_000, 500.0, 0.3, 1.0);
        signal.extend(sine(48_000, 5_000.0, 0.3, 1.0));
        let found = spectral_centroid_over_time(&signal, 48_000, 0.1);
        assert_eq!(found.len(), 20);
        for (time, hz, level) in found {
            if time < 0.9 {
                assert!((hz - 500.0).abs() < 5.0, "{hz} Hz at {time} s");
            } else if time > 1.1 {
                assert!((hz - 5_000.0).abs() < 5.0, "{hz} Hz at {time} s");
            }
            assert!((level - db(0.045)).abs() < 0.2);
        }
    }
}
