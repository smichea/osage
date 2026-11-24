// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! How loud a programme is to a listener, by ITU-R BS.1770-4: the level of
//! the signal weighed as the ear weighs frequencies (K-weighting), averaged
//! over the stretches where there is something to hear (two gates).
//!
//! The figure is what broadcasters align programmes on (LKFS, the same as
//! LUFS), so a render of this prototype and a recording of anything else can
//! be set to the same loudness before they are compared.

use crate::resample::resample;

/// The rate the standard gives its filters for.
const RATE: u32 = 48_000;
/// A fifth of a gating block: blocks are 400 ms long and start every 100 ms.
const STEP: usize = 4_800;
/// The gate under which nothing counts, LKFS.
const ABSOLUTE_GATE: f64 = -70.0;
/// The second gate, under the level of what passed the first, LU.
const RELATIVE_GATE: f64 = -10.0;
/// What the standard adds so that a 997 Hz sine reads its own level: the
/// weighting's gain there, taken out.
const OFFSET: f64 = -0.691;

/// A second-order filter: `y[n] = b0·x[n] + b1·x[n−1] + b2·x[n−2] − a1·y[n−1] − a2·y[n−2]`.
#[derive(Clone, Copy, Debug)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    s1: f64,
    s2: f64,
}

impl Biquad {
    const fn new(b: [f64; 3], a: [f64; 2]) -> Biquad {
        Biquad { b, a, s1: 0.0, s2: 0.0 }
    }

    fn pass(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.s1;
        self.s1 = self.b[1] * x - self.a[0] * y + self.s2;
        self.s2 = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// The first stage of the K-weighting at 48 kHz (table 1 of the standard): a
/// shelf that lifts the highs by 4 dB, as a head does.
const SHELF: Biquad = Biquad::new(
    [1.535_124_859_586_97, -2.691_696_189_406_38, 1.198_392_810_852_85],
    [-1.690_659_293_182_41, 0.732_480_774_215_85],
);
/// Its second stage (table 2): a high-pass, since the ear hardly hears lows.
const HIGH_PASS: Biquad = Biquad::new([1.0, -2.0, 1.0], [-1.990_047_454_833_98, 0.990_072_250_366_21]);

/// The integrated loudness of a programme, in LKFS, by ITU-R BS.1770-4:
/// K-weighting, blocks of 400 ms every 100 ms, a gate at −70 LKFS and one
/// 10 LU under the level of what passed the first.
///
/// `None` when no block passes the gates: silence, or less than 400 ms of
/// sound.
///
/// Every channel counts with a weight of 1: this reads mono and stereo, and
/// does not know the surround channels the standard weighs otherwise. The
/// standard gives its filters at 48 kHz only: a signal at another rate is
/// first resampled to 48 kHz ([`resample`]), rather than filtered with
/// coefficients the standard does not give.
///
/// A sine of 997 Hz and of peak `a` in one channel reads
/// `20·lg(a) − 3.01` LKFS (its mean square is `a²/2`, and the standard's
/// constant cancels the weighting's gain at 997 Hz). In both channels of a
/// stereo file the mean squares add, and it reads `20·lg(a)`: −20.0 LKFS at
/// −20 dBFS, −23.0 LKFS at −23 dBFS.
pub fn loudness(channels: &[&[f32]], rate: u32) -> Option<f64> {
    let resampled: Vec<Vec<f32>>;
    let at_48: Vec<&[f32]> = if rate == RATE {
        channels.to_vec()
    } else {
        resampled = channels.iter().map(|channel| resample(channel, rate, RATE)).collect();
        resampled.iter().map(Vec::as_slice).collect()
    };
    let length = at_48.iter().map(|channel| channel.len()).min().unwrap_or(0);
    // The weighed squares of all channels, summed over each 100 ms.
    let mut steps = vec![0.0f64; length / STEP];
    for channel in &at_48 {
        let (mut shelf, mut high_pass) = (SHELF, HIGH_PASS);
        for (sum, step) in steps.iter_mut().zip(channel.as_chunks::<STEP>().0) {
            for &sample in step {
                let weighed = high_pass.pass(shelf.pass(f64::from(sample)));
                *sum += weighed * weighed;
            }
        }
    }
    // The mean square of each block of four steps.
    let blocks: Vec<f64> = steps.windows(4).map(|four| four.iter().sum::<f64>() / (4 * STEP) as f64).collect();
    let level = |mean_square: f64| OFFSET + 10.0 * mean_square.log10();
    let mean = |over: &dyn Fn(f64) -> bool| {
        let passed: Vec<f64> = blocks.iter().copied().filter(|&block| over(level(block))).collect();
        if passed.is_empty() { None } else { Some(passed.iter().sum::<f64>() / passed.len() as f64) }
    };
    let gate = level(mean(&|block| block > ABSOLUTE_GATE)?) + RELATIVE_GATE;
    Some(level(mean(&|block| block > ABSOLUTE_GATE && block > gate)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{sine, white};

    #[test]
    fn a_997_hz_sine_at_minus_20_dbfs_in_both_channels_reads_minus_20_lkfs() {
        let tone = sine(48_000, 997.0, 0.1, 5.0);
        let read = loudness(&[&tone, &tone], 48_000).unwrap();
        assert!((read + 20.0).abs() < 0.01, "{read} LKFS");
        // The alignment level of broadcast: −23 dBFS reads −23 LKFS.
        let tone = sine(48_000, 997.0, 10f64.powf(-23.0 / 20.0), 5.0);
        let read = loudness(&[&tone, &tone], 48_000).unwrap();
        assert!((read + 23.0).abs() < 0.01, "{read} LKFS");
    }

    #[test]
    fn the_same_sine_in_one_channel_only_reads_3_db_less() {
        let tone = sine(48_000, 997.0, 0.1, 5.0);
        let silence = vec![0.0f32; tone.len()];
        let one = loudness(&[&tone, &silence], 48_000).unwrap();
        assert!((one + 23.01).abs() < 0.01, "{one} LKFS");
        // What the standard states: full scale in one channel reads −3.01 LKFS.
        let full = sine(48_000, 997.0, 1.0, 5.0);
        let read = loudness(&[&full], 48_000).unwrap();
        assert!((read + 3.01).abs() < 0.01, "{read} LKFS");
    }

    #[test]
    fn loudness_follows_the_gain_decibel_for_decibel() {
        let noise = white(4, 48_000, 4.0, 0.5);
        let quieter: Vec<f32> = noise.iter().map(|s| s * 0.25).collect();
        let (loud, quiet) = (loudness(&[&noise, &noise], 48_000).unwrap(), loudness(&[&quieter, &quieter], 48_000));
        assert!((loud - quiet.unwrap() - 12.0412).abs() < 1e-3);
    }

    #[test]
    fn the_ear_s_weighting_lifts_the_highs_and_drops_the_lows() {
        let read = |hz: f64| {
            let tone = sine(48_000, hz, 0.1, 3.0);
            loudness(&[&tone, &tone], 48_000).unwrap()
        };
        // The shelf lifts the top by 4 dB, of which 0.7 are already there at
        // 1 kHz; the high-pass is 3 dB down near 38 Hz.
        assert!((read(10_000.0) + 20.0 - 3.3).abs() < 0.1, "{}", read(10_000.0));
        assert!(read(30.0) < -24.0, "{}", read(30.0));
        assert!((read(1_000.0) + 20.0).abs() < 0.02);
    }

    #[test]
    fn a_quiet_stretch_does_not_lower_the_loudness_of_the_rest() {
        // 10 s at −20 dBFS, then 10 s 30 dB lower: the mean of the whole is
        // 3 dB down, but the quiet half is under the second gate.
        let mut tone = sine(48_000, 997.0, 0.1, 10.0);
        tone.extend(sine(48_000, 997.0, 0.1 * 10f64.powf(-1.5), 10.0));
        let read = loudness(&[&tone, &tone], 48_000).unwrap();
        assert!((read + 20.0).abs() < 0.1, "{read} LKFS");
    }

    #[test]
    fn silence_around_a_sound_does_not_change_its_loudness() {
        // But for the three blocks at each end that hold both: 0.07 dB of 20 s.
        let tone = sine(48_000, 997.0, 0.1, 20.0);
        let mut padded = vec![0.0f32; 96_000];
        padded.extend(&tone);
        padded.extend(vec![0.0f32; 96_000]);
        let (alone, padded) = (loudness(&[&tone], 48_000).unwrap(), loudness(&[&padded], 48_000).unwrap());
        assert!((alone - padded).abs() < 0.1, "{alone} alone, {padded} in silence");
    }

    #[test]
    fn silence_and_what_is_shorter_than_a_block_have_no_loudness() {
        assert_eq!(loudness(&[&[0.0; 96_000], &[0.0; 96_000]], 48_000), None);
        assert_eq!(loudness(&[&sine(48_000, 997.0, 0.5, 0.3)], 48_000), None);
        assert_eq!(loudness(&[], 48_000), None);
        // Under the first gate: −80 dBFS.
        assert_eq!(loudness(&[&sine(48_000, 997.0, 1e-4, 2.0)], 48_000), None);
    }

    #[test]
    fn a_file_at_44_1_khz_reads_what_it_reads_at_48_khz() {
        let at_48 = loudness(&[&sine(48_000, 997.0, 0.1, 3.0)], 48_000).unwrap();
        let at_44 = loudness(&[&sine(44_100, 997.0, 0.1, 3.0)], 44_100).unwrap();
        assert!((at_48 - at_44).abs() < 0.01, "{at_48} at 48 kHz, {at_44} at 44.1 kHz");
        let high = loudness(&[&sine(44_100, 12_000.0, 0.1, 3.0)], 44_100).unwrap();
        let high_48 = loudness(&[&sine(48_000, 12_000.0, 0.1, 3.0)], 48_000).unwrap();
        assert!((high - high_48).abs() < 0.01, "{high_48} at 48 kHz, {high} at 44.1 kHz");
    }
}
