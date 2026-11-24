// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! How the level of a sound moves over time, and what a passage looks like
//! in it: a train that comes, is there, and goes is a level that rises,
//! peaks and falls.

use crate::{
    bands::{Band, band_levels_over_time},
    level::{db, mean_square},
};

/// A level along time.
#[derive(Clone, Debug, PartialEq)]
pub struct Envelope {
    /// The middle of each step, seconds.
    pub times: Vec<f64>,
    /// The level of each step, dB.
    pub levels: Vec<f64>,
}

/// The level of a signal every `step_s` seconds (0.1, say): the mean square
/// of each step, in dB. Samples left after the last whole step are not
/// counted, unless they are all there is.
pub fn envelope(signal: &[f32], rate: u32, step_s: f64) -> Envelope {
    let step = ((step_s * f64::from(rate)).round() as usize).max(1);
    let steps = if signal.is_empty() { 0 } else { (signal.len() / step).max(1) };
    let mut found = Envelope { times: Vec::with_capacity(steps), levels: Vec::with_capacity(steps) };
    for (n, samples) in signal.chunks(step).take(steps).enumerate() {
        found.times.push((n * step) as f64 / f64::from(rate) + samples.len() as f64 / 2.0 / f64::from(rate));
        found.levels.push(db(mean_square(samples)));
    }
    found
}

/// The level of one band of a signal every `step_s` seconds
/// ([`band_levels_over_time`]: each value is seen through a window long
/// enough to tell the band from its neighbours, 0.17 s at least for steps
/// of 0.1 s).
pub fn band_envelope(signal: &[f32], rate: u32, band: &Band, step_s: f64) -> Envelope {
    let mut found = band_levels_over_time(signal, rate, &[*band], step_s);
    Envelope { times: found.times, levels: found.levels.swap_remove(0) }
}

/// What a level that rises then falls looks like.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Passage {
    /// When the level peaks, seconds.
    pub time: f64,
    /// The level at the peak, dB.
    pub peak_db: f64,
    /// The peak above the quietest second before it, dB: 0 when nothing
    /// comes before.
    pub rise_db: f64,
    /// The peak above the quietest second after it, dB.
    pub fall_db: f64,
}

impl Passage {
    /// Whether the level rose by `by_db` at least to its peak, and fell by
    /// as much after it: something came and went.
    pub fn rises_then_falls(&self, by_db: f64) -> bool {
        self.rise_db >= by_db && self.fall_db >= by_db
    }
}

/// The mean of levels in dB, as a level: the level of their mean power.
fn mean_level(levels: &[f64]) -> f64 {
    db(levels.iter().map(|level| 10f64.powf(level / 10.0)).sum::<f64>() / levels.len().max(1) as f64)
}

impl Envelope {
    /// Seconds between two values.
    pub fn step(&self) -> f64 {
        if self.times.len() > 1 {
            (self.times[self.times.len() - 1] - self.times[0]) / (self.times.len() - 1) as f64
        } else {
            0.0
        }
    }

    /// When the level is highest, and how high: (seconds, dB). `None` for an
    /// envelope of nothing.
    pub fn peak(&self) -> Option<(f64, f64)> {
        let at = (0..self.levels.len()).max_by(|&a, &b| self.levels[a].total_cmp(&self.levels[b]).then(b.cmp(&a)))?;
        Some((self.times[at], self.levels[at]))
    }

    /// The level between two instants, dB: that of the mean power of the
    /// steps whose middle falls between them. Silence when none does.
    pub fn level_between(&self, from_s: f64, to_s: f64) -> f64 {
        let inside: Vec<f64> = self
            .times
            .iter()
            .zip(&self.levels)
            .filter(|(time, _)| (from_s..to_s).contains(*time))
            .map(|(_, level)| *level)
            .collect();
        if inside.is_empty() { db(0.0) } else { mean_level(&inside) }
    }

    /// By how much the peak rises above the level between two instants, dB:
    /// how far a passage stands out of what was there without it.
    pub fn rise_above(&self, from_s: f64, to_s: f64) -> f64 {
        self.peak().map_or(0.0, |(_, peak)| peak - self.level_between(from_s, to_s))
    }

    /// The same envelope with each value replaced by the level of the mean
    /// power over `seconds` around it: what is left when clicks and gusts
    /// are averaged out.
    pub fn smoothed(&self, seconds: f64) -> Envelope {
        let step = self.step();
        let reach = if step > 0.0 { (seconds / step / 2.0).round() as usize } else { 0 };
        let levels = (0..self.levels.len())
            .map(|n| mean_level(&self.levels[n.saturating_sub(reach)..(n + reach + 1).min(self.levels.len())]))
            .collect();
        Envelope { times: self.times.clone(), levels }
    }

    /// The peak of the envelope, and how far it stands above the quietest
    /// second before it and the quietest second after it. `None` for an
    /// envelope of nothing.
    ///
    /// The quietest second, rather than the first and the last: a passage is
    /// told by the calm on each side of it, wherever in the file that calm
    /// is.
    pub fn passage(&self) -> Option<Passage> {
        let at = (0..self.levels.len()).max_by(|&a, &b| self.levels[a].total_cmp(&self.levels[b]).then(b.cmp(&a)))?;
        let step = self.step();
        let second = if step > 0.0 { ((1.0 / step).round() as usize).max(1) } else { 1 };
        let quietest = |side: &[f64]| {
            if side.is_empty() {
                return self.levels[at];
            }
            side.windows(second.min(side.len())).map(mean_level).fold(f64::INFINITY, f64::min)
        };
        Some(Passage {
            time: self.times[at],
            peak_db: self.levels[at],
            rise_db: self.levels[at] - quietest(&self.levels[..at]),
            fall_db: self.levels[at] - quietest(&self.levels[at + 1..]),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{sine, white};

    /// 20 s of noise that passes at 10 s: its level that of a source moving
    /// at 10 m/s on a line 5 m away, over a steady hum.
    fn passing() -> Vec<f32> {
        let mut signal = white(60, 48_000, 20.0, 0.5);
        for (n, s) in signal.iter_mut().enumerate() {
            let along = 10.0 * (n as f64 / 48_000.0 - 10.0);
            *s *= (5.0 / (along * along + 25.0).sqrt()) as f32;
        }
        for (s, hum) in signal.iter_mut().zip(sine(48_000, 100.0, 0.03, 20.0)) {
            *s += hum;
        }
        signal
    }

    #[test]
    fn the_envelope_of_a_steady_sine_is_its_level_at_every_step() {
        let found = envelope(&sine(48_000, 1_000.0, 0.1, 1.05), 48_000, 0.1);
        assert_eq!(found.levels.len(), 10);
        assert!((found.times[0] - 0.05).abs() < 1e-12 && (found.times[9] - 0.95).abs() < 1e-12);
        assert!((found.step() - 0.1).abs() < 1e-12);
        for level in &found.levels {
            assert!((level - db(0.005)).abs() < 1e-3);
        }
        // Less than a step is still measured; nothing is nothing.
        assert_eq!(envelope(&sine(48_000, 1_000.0, 0.1, 0.03), 48_000, 0.1).levels.len(), 1);
        assert!(envelope(&[], 48_000, 0.1).levels.is_empty());
        assert_eq!(envelope(&[], 48_000, 0.1).peak(), None);
        assert_eq!(envelope(&[], 48_000, 0.1).passage(), None);
    }

    #[test]
    fn a_passage_peaks_when_the_source_is_nearest_and_rises_then_falls() {
        let found = envelope(&passing(), 48_000, 0.1);
        let (time, level) = found.peak().unwrap();
        assert!((time - 10.0).abs() < 0.3, "peak at {time} s");
        // Uniform noise of ±0.5 at its nearest: a mean square of 1/12.
        assert!((level - db(1.0 / 12.0)).abs() < 0.5, "{level} dB");
        // At 100 m the noise is 26 dB down, and the hum is as strong as it is there.
        let passage = found.passage().unwrap();
        assert!((passage.time - time).abs() < 1e-12 && (passage.peak_db - level).abs() < 1e-12);
        assert!((passage.rise_db - 20.9).abs() < 1.5 && (passage.fall_db - 20.9).abs() < 1.5, "{passage:?}");
        assert!(passage.rises_then_falls(18.0) && !passage.rises_then_falls(26.0));
        assert!((found.rise_above(0.0, 2.0) - 20.7).abs() < 1.5, "{}", found.rise_above(0.0, 2.0));
        assert!((found.level_between(9.9, 10.1) - level).abs() < 0.5);
    }

    #[test]
    fn a_steady_sound_and_a_sound_that_only_arrives_are_no_passage() {
        let steady = envelope(&white(61, 48_000, 10.0, 0.3), 48_000, 0.1).passage().unwrap();
        assert!(!steady.rises_then_falls(3.0), "{steady:?}");
        // The first half of a passage: it rises, and is cut at its peak.
        let arriving = envelope(&passing()[..480_000], 48_000, 0.1).passage().unwrap();
        assert!(arriving.rise_db > 15.0 && arriving.fall_db < 3.0, "{arriving:?}");
        assert!(!arriving.rises_then_falls(6.0));
    }

    #[test]
    fn the_envelope_of_a_band_does_not_see_what_passes_in_another() {
        let signal = passing();
        let hum = band_envelope(&signal, 48_000, &Band::new(80.0, 125.0), 0.1);
        assert_eq!(hum.levels.len(), 200);
        // The hum's band holds a little of the passing noise (45 Hz of 24 kHz) at its nearest.
        let passage = hum.passage().unwrap();
        assert!(!passage.rises_then_falls(5.0), "{passage:?}");
        assert!((hum.level_between(0.0, 5.0) - db(0.000_45)).abs() < 0.5);
        let noise = band_envelope(&signal, 48_000, &Band::octave(2_000.0), 0.1);
        assert!(noise.passage().unwrap().rises_then_falls(20.0));
        assert!((noise.peak().unwrap().0 - 10.0).abs() < 0.5);
    }

    #[test]
    fn smoothing_keeps_the_level_and_takes_out_a_click() {
        let mut signal = white(62, 48_000, 6.0, 0.1);
        for s in &mut signal[144_000..144_096] {
            *s = 0.9;
        }
        let raw = envelope(&signal, 48_000, 0.1);
        let smooth = raw.smoothed(1.0);
        assert_eq!(smooth.times, raw.times);
        assert!(raw.rise_above(0.0, 2.0) > 6.0);
        assert!(smooth.rise_above(0.0, 2.0) < 3.0, "{}", smooth.rise_above(0.0, 2.0));
        assert!((smooth.level_between(0.0, 2.0) - raw.level_between(0.0, 2.0)).abs() < 0.2);
    }
}
