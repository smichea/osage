// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! A scene rendered to a file's samples, offline: the engine run faster than
//! the clock, from a few seconds before the first sample — sound that left
//! its emitter before then is already on its way — then faded at both ends,
//! brought to a level by one gain for the whole file (nothing that follows
//! the sound: a limiter riding the train's passage would falsify it; and
//! the level is that of the file as it will be heard, ends faded),
//! dithered and rounded to 16 bits.

use osage_sound_voices::dsp::{Noise, seed_for};

use crate::{
    engine::{BLOCK, Engine, FieldEars, Options, Pose, RATE, Struck, Tally},
    field::MOST,
    scene::Scene,
};

/// What makes a listener's blocks: one engine for all the scene's sound —
/// or, when the options ask for far sound as a field, an engine for the
/// near part, an engine that makes the far part's field at the cube's
/// centre, and that field heard by the listener's head.
pub struct Hearing {
    near: Engine,
    far: Option<FarPart>,
}

/// The engine that makes the far field, the listener's ears for it, and
/// the field's block.
type FarPart = (Engine, FieldEars, Box<[[f32; BLOCK]; MOST]>);

impl Hearing {
    pub fn new(scene: Scene, options: Options) -> Hearing {
        let far = options.far.map(|far| {
            let ears = FieldEars::new(far.order, options.quality, options.listening);
            (Engine::field(scene.clone(), options.clone()), ears, Box::new([[0.0; BLOCK]; MOST]))
        });
        Hearing { near: Engine::new(scene, options), far }
    }

    /// The instant of the next sample, s.
    pub fn instant(&self) -> f64 {
        self.near.instant()
    }

    /// Places a node in both parts ([`Engine::drive_at`]).
    pub fn drive_at(&mut self, node: usize, pose: Pose, at: f64) {
        self.near.drive_at(node, pose, at);
        if let Some((far, ..)) = &mut self.far {
            far.drive_at(node, pose, at);
        }
    }

    pub fn drive(&mut self, node: usize, pose: Pose) {
        let at = self.instant();
        self.drive_at(node, pose, at);
    }

    /// A contact a solver reports, in both parts ([`Engine::strike_told`]).
    pub fn strike_told(&mut self, struck: Struck) {
        if let Some((far, ..)) = &mut self.far {
            far.strike_told(struck.clone());
        }
        self.near.strike_told(struck);
    }

    /// The next block for each ear: the near part, and the far field heard
    /// by the head as the near part placed it.
    pub fn render(&mut self, left: &mut [f32; BLOCK], right: &mut [f32; BLOCK]) {
        self.near.render(left, right);
        if let Some((far, ears, field)) = &mut self.far {
            far.render_field(field);
            ears.hear(field, &self.near.head(), left, right);
        }
    }

    /// What the parts cost, summed.
    pub fn tally(&self) -> Tally {
        let mut tally = self.near.tally;
        if let Some((far, ..)) = &self.far {
            let f = far.tally;
            tally.ways += f.ways;
            tally.most_ways += f.most_ways;
            tally.voices += f.voices;
            tally.contacts += f.contacts;
            tally.ring_bytes += f.ring_bytes;
            tally.touching += f.touching;
            tally.finding += f.finding;
            tally.voicing += f.voicing;
            tally.carrying += f.carrying;
            tally.tailing += f.tailing;
        }
        tally
    }
}

/// A pose told at an instant: a node placed from outside
/// ([`Engine::drive`]), as a recorded replay holds it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Told {
    /// The instant it was told at, s, on the scene's clock.
    pub at: f64,
    pub node: usize,
    pub pose: Pose,
}

/// Poses told in advance, given to an engine as its blocks come: each
/// before the first block that starts at or after its instant, those of the
/// same instant in the order they were told.
#[derive(Clone, Debug, Default)]
pub struct Replay {
    told: Vec<Told>,
    next: usize,
}

impl Replay {
    pub fn new(told: &[Told]) -> Replay {
        let mut told = told.to_vec();
        told.sort_by(|a, b| a.at.total_cmp(&b.at));
        Replay { told, next: 0 }
    }

    /// Gives `hearing` every pose due by the instant of its next block.
    pub fn tell(&mut self, hearing: &mut Hearing) {
        let now = hearing.instant() + 1e-9;
        while let Some(told) = self.told.get(self.next).filter(|told| told.at <= now) {
            hearing.drive_at(told.node, told.pose, told.at);
            self.next += 1;
        }
    }
}

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
    render_told(scene, options, seconds, &[])
}

/// [`render`], with the poses a game or a server told, each from the first
/// block that starts at or after its instant ([`Told`]): what a recorded
/// replay gives, and what [`crate::live`] makes of the same replay, block
/// for block.
pub fn render_told(scene: &Scene, options: &Options, seconds: f64, told: &[Told]) -> Rendered {
    let mut engine = Hearing::new(scene.clone(), Options { start: -LEAD, ..options.clone() });
    let mut replay = Replay::new(told);
    let lead = (LEAD * RATE as f64 / BLOCK as f64).round() as usize;
    let frames = (seconds * RATE as f64).round() as usize;
    let (mut left, mut right) = (Vec::with_capacity(frames + BLOCK), Vec::with_capacity(frames + BLOCK));
    let (mut l, mut r) = ([0.0f32; BLOCK], [0.0f32; BLOCK]);
    for _ in 0..lead {
        replay.tell(&mut engine);
        engine.render(&mut l, &mut r);
    }
    // What the lead cost is not the render's.
    let before = engine.tally();
    while left.len() < frames {
        replay.tell(&mut engine);
        engine.render(&mut l, &mut r);
        left.extend_from_slice(&l);
        right.extend_from_slice(&r);
    }
    left.truncate(frames);
    right.truncate(frames);
    let tally = Tally {
        blocks: engine.tally().blocks - before.blocks,
        ways: engine.tally().ways - before.ways,
        voices: engine.tally().voices - before.voices,
        contacts: engine.tally().contacts - before.contacts,
        touching: engine.tally().touching - before.touching,
        finding: engine.tally().finding - before.finding,
        voicing: engine.tally().voicing - before.voicing,
        carrying: engine.tally().carrying - before.carrying,
        tailing: engine.tally().tailing - before.tailing,
        ..engine.tally()
    };
    Rendered { left, right, tally }
}

/// The ends of a file: `rendered` rises from silence over `fade_in` seconds
/// and falls to it over `fade_out`, a raised cosine each — over less in a
/// render too short for them: the two together never take more than half of
/// it.
pub fn fade(rendered: &mut Rendered, fade_in: f64, fade_out: f64) {
    let frames = rendered.left.len();
    let asked = (fade_in + fade_out).max(0.0) * RATE as f64;
    let fits = if asked > frames as f64 / 2.0 { frames as f64 / 2.0 / asked } else { 1.0 };
    let (rise, fall) = (fade_in * RATE as f64 * fits, fade_out * RATE as f64 * fits);
    let ease = |x: f64| 0.5 - 0.5 * (std::f64::consts::PI * x.clamp(0.0, 1.0)).cos();
    for n in 0..frames {
        let mut level = 1.0;
        if rise > 0.0 {
            level *= ease(n as f64 / rise);
        }
        if fall > 0.0 {
            level *= ease((frames - 1 - n) as f64 / fall);
        }
        if level < 1.0 {
            rendered.left[n] = (f64::from(rendered.left[n]) * level) as f32;
            rendered.right[n] = (f64::from(rendered.right[n]) * level) as f32;
        }
    }
}

/// The gain that brings a render to `loudness` (LUFS, ITU-R BS.1770-4) — or
/// to less, if at that loudness its peaks between samples would pass
/// `ceiling` dB of full scale. Returns the gain, and the loudness reached.
/// Asked of a render whose ends are already faded ([`fade`]): the loudness
/// is then the file's.
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

/// The render at `gain`, dithered (two uniform noises a sample, a triangle
/// of two steps: the rounding error is then a steady hiss, not a distortion
/// of the sound) and rounded to 16 bits.
pub fn master(rendered: &Rendered, gain: f64, seed: u64) -> Mastered {
    let frames = rendered.left.len();
    let mut dither = [Noise::new(seed_for(seed, "dither left")), Noise::new(seed_for(seed, "dither right"))];
    let mut samples = Vec::with_capacity(frames * 2);
    let mut held = 0;
    for n in 0..frames {
        for (ear, channel) in [&rendered.left, &rendered.right].into_iter().enumerate() {
            let noise = f64::from(dither[ear].unit()) - 0.5 + f64::from(dither[ear].unit()) - 0.5;
            let value = (f64::from(channel[n]) * gain * 32_767.0 + noise).round();
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
        let whole = Rendered { left: tone.clone(), right: tone.iter().map(|s| -s).collect(), tally: Tally::default() };
        let mut rendered = whole.clone();
        fade(&mut rendered, 0.1, 0.2);
        let mastered = master(&rendered, 1.0, 6);
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
        assert_eq!(master(&rendered, 1.0, 6), mastered);
        assert_ne!(master(&rendered, 1.0, 7).samples, mastered.samples);
        // Too loud is held and told.
        assert!(master(&whole, 3.0, 6).held > 0);
    }

    #[test]
    fn a_short_file_is_as_loud_as_asked_once_its_ends_are_faded() {
        // A second of a tone, asked at −30 LUFS with ends that would take 1.9 s: they take half of it, and
        // the file as it is written reads what was asked.
        let tone: Vec<f32> = (0..RATE).map(|i| 0.05 * (i as f32 * 0.13).sin()).collect();
        let mut rendered = Rendered { left: tone.clone(), right: tone, tally: Tally::default() };
        fade(&mut rendered, 0.4, 1.5);
        // The fades take half of the file between them, in the proportion asked; the rest is untouched.
        let (rise, fall) = (0.5 * 0.4 / 1.9 * RATE as f64, 0.5 * 1.5 / 1.9 * RATE as f64);
        let untouched = &rendered.left[rise.ceil() as usize..RATE as usize - fall.ceil() as usize - 1];
        assert!(
            untouched.iter().enumerate().all(|(n, s)| *s == 0.05 * ((n + rise.ceil() as usize) as f32 * 0.13).sin())
        );
        assert!(rendered.left[0] == 0.0 && rendered.left[RATE as usize - 1] == 0.0);
        let (gain, reached) = level(&rendered, -30.0, -1.0);
        assert!((reached + 30.0).abs() < 1e-9, "{reached}");
        let mastered = master(&rendered, gain, 6);
        let file: Vec<Vec<f32>> = (0..2)
            .map(|ear| mastered.samples.iter().skip(ear).step_by(2).map(|s| f32::from(*s) / 32_768.0).collect())
            .collect();
        let read = osage_sound_analysis::loudness(&[&file[0], &file[1]], RATE).unwrap();
        assert!((read + 30.0).abs() < 0.05, "{read} LUFS");
    }
}
