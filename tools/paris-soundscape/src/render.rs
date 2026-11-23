// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! A scene rendered to a file's samples, offline: the engine run faster than
//! the clock, from a few seconds before the first sample — sound that left
//! its emitter before then is already on its way — then brought to a level
//! by one gain for the whole file (nothing that follows the sound: a
//! limiter riding the train's passage would falsify it), faded at both
//! ends, dithered and rounded to 16 bits.

use osage_paris_voices::dsp::{Noise, seed_for};

use crate::{
    engine::{BLOCK, Engine, Options, RATE, Tally},
    scene::Scene,
};

/// How long before its first sample a render starts, s: longer than sound
/// takes from the farthest emitter, and a whole number of blocks.
pub const LEAD: f64 = 3.0;

/// A render before it is brought to a file's level.
#[derive(Clone, Debug, PartialEq)]
pub struct Rendered {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
    pub tally: Tally,
}

impl Rendered {
    /// The largest sample of either ear.
    pub fn peak(&self) -> f32 {
        self.left.iter().chain(&self.right).fold(0.0f32, |m, s| m.max(s.abs()))
    }

    pub fn seconds(&self) -> f64 {
        self.left.len() as f64 / RATE as f64
    }
}

/// `seconds` of the scene from its instant 0, as `options` ask (their own
/// start is replaced by the lead).
pub fn render(scene: &Scene, options: &Options, seconds: f64) -> Rendered {
    let mut engine = Engine::new(scene.clone(), Options { start: -LEAD, ..options.clone() });
    let lead = (LEAD * RATE as f64 / BLOCK as f64).round() as usize;
    let frames = (seconds * RATE as f64).round() as usize;
    let (mut left, mut right) = (Vec::with_capacity(frames + BLOCK), Vec::with_capacity(frames + BLOCK));
    let (mut l, mut r) = ([0.0f32; BLOCK], [0.0f32; BLOCK]);
    for _ in 0..lead {
        engine.render(&mut l, &mut r);
    }
    // What the lead cost is not the render's.
    let before = engine.tally;
    while left.len() < frames {
        engine.render(&mut l, &mut r);
        left.extend_from_slice(&l);
        right.extend_from_slice(&r);
    }
    left.truncate(frames);
    right.truncate(frames);
    let tally = Tally {
        blocks: engine.tally.blocks - before.blocks,
        ways: engine.tally.ways - before.ways,
        voices: engine.tally.voices - before.voices,
        contacts: engine.tally.contacts - before.contacts,
        touching: engine.tally.touching - before.touching,
        finding: engine.tally.finding - before.finding,
        voicing: engine.tally.voicing - before.voicing,
        carrying: engine.tally.carrying - before.carrying,
        tailing: engine.tally.tailing - before.tailing,
        ..engine.tally
    };
    Rendered { left, right, tally }
}

/// The gain that brings a render to `loudness` (LUFS, ITU-R BS.1770-4) — or
/// to less, if at that loudness its peaks between samples would pass
/// `ceiling` dB of full scale. Returns the gain, and the loudness reached.
pub fn level(rendered: &Rendered, loudness: f64, ceiling: f64) -> (f64, f64) {
    let Some(found) = osage_sound_analysis::loudness(&[&rendered.left, &rendered.right], RATE) else {
        // Silence, or less than the 0.4 s a loudness is measured over.
        return (1.0, f64::NEG_INFINITY);
    };
    let peak = osage_sound_analysis::true_peak(&rendered.left).max(osage_sound_analysis::true_peak(&rendered.right));
    let wanted = 10f64.powf((loudness - found) / 20.0);
    let allowed = 10f64.powf(ceiling / 20.0) / peak.max(1e-12);
    let gain = wanted.min(allowed);
    (gain, found + 20.0 * gain.log10())
}

/// A render as a file's samples.
#[derive(Clone, Debug, PartialEq)]
pub struct Mastered {
    /// Left and right in turn, 16 bits.
    pub samples: Vec<i16>,
    /// Samples that had to be held at the limit: none, in a render at its
    /// level.
    pub held: usize,
}

/// The render at `gain`, faded in over `fade_in` seconds and out over
/// `fade_out`, dithered (two uniform noises a sample, a triangle of two
/// steps: the rounding error is then a steady hiss, not a distortion of the
/// sound) and rounded to 16 bits.
pub fn master(rendered: &Rendered, gain: f64, seed: u64, fade_in: f64, fade_out: f64) -> Mastered {
    let frames = rendered.left.len();
    let mut dither = [Noise::new(seed_for(seed, "dither left")), Noise::new(seed_for(seed, "dither right"))];
    let (rise, fall) = (fade_in * RATE as f64, fade_out * RATE as f64);
    let mut samples = Vec::with_capacity(frames * 2);
    let mut held = 0;
    for n in 0..frames {
        // A raised cosine at each end.
        let ease = |x: f64| 0.5 - 0.5 * (std::f64::consts::PI * x.clamp(0.0, 1.0)).cos();
        let mut level = gain;
        if rise > 0.0 {
            level *= ease(n as f64 / rise);
        }
        if fall > 0.0 {
            level *= ease((frames - 1 - n) as f64 / fall);
        }
        for (ear, channel) in [&rendered.left, &rendered.right].into_iter().enumerate() {
            let noise = f64::from(dither[ear].unit()) - 0.5 + f64::from(dither[ear].unit()) - 0.5;
            let value = (f64::from(channel[n]) * level * 32_767.0 + noise).round();
            if !(-32_768.0..=32_767.0).contains(&value) {
                held += 1;
            }
            samples.push(value.clamp(-32_768.0, 32_767.0) as i16);
        }
    }
    Mastered { samples, held }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mastering_keeps_the_level_fades_the_ends_and_holds_nothing() {
        let tone: Vec<f32> = (0..RATE).map(|i| 0.5 * (i as f32 * 0.05).sin()).collect();
        let rendered =
            Rendered { left: tone.clone(), right: tone.iter().map(|s| -s).collect(), tally: Tally::default() };
        let mastered = master(&rendered, 1.0, 6, 0.1, 0.2);
        assert_eq!((mastered.samples.len(), mastered.held), (2 * RATE as usize, 0));
        // The ends are silent to within the dither; the middle is at half of full scale.
        assert!(
            mastered.samples[..4].iter().all(|s| s.abs() <= 2)
                && mastered.samples[2 * RATE as usize - 4..].iter().all(|s| s.abs() <= 2)
        );
        let middle = &mastered.samples[RATE as usize - 2_000..RATE as usize + 2_000];
        let peak = middle.iter().map(|s| i32::from(*s).abs()).max().unwrap();
        assert!((peak - 16_383).abs() < 60, "{peak}");
        // The two ears were each other's opposite: so are their samples, to within the dither.
        assert!(middle.chunks(2).all(|pair| (i32::from(pair[0]) + i32::from(pair[1])).abs() <= 3));
        // The same seed, the same bytes; another seed, another dither.
        assert_eq!(master(&rendered, 1.0, 6, 0.1, 0.2), mastered);
        assert_ne!(master(&rendered, 1.0, 7, 0.1, 0.2).samples, mastered.samples);
        // Too loud is held and told.
        assert!(master(&rendered, 3.0, 6, 0.0, 0.0).held > 0);
    }
}
