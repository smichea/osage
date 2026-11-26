// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Voices played from recordings, which a world gives as content, and the
//! levels of detail they are played at from afar.
//!
//! - **A loop that follows a state** ([`Loop`]): clips each recorded at a
//!   known value of what the voice follows — its speed, or its load. The two
//!   on either side of the state are crossfaded, and each is played faster
//!   or slower so that its note follows the state, as
//!   `osage-kart-sound` plays its engine.
//! - **One-shots** ([`Shot`]): a set of clips for what was struck, one
//!   drawn for each blow from the voice's seed (no two steps alike),
//!   played from the sample of the blow, as loud as it was hard.
//! - **Layers**: either may sound over a synthesised voice, for what was
//!   not recorded.
//!
//! **Levels of detail.** A clip is kept at its own rate and at a half, a
//! quarter and an eighth of it, each made from the one above by a
//! low-pass and every other sample: a sound's mipmaps. A clip made at a
//! lower rate has no finer level, and holds as many fewer bytes. The engine
//! tells a voice the highest frequency still worth making
//! ([`crate::Drive::top`]): from afar, the air has taken the top. The voice
//! then plays the coarsest level that still holds that frequency with a
//! fifth to spare, and crossfades over 50 ms when it changes level. A voice
//! told it is worth little detail ([`crate::Drive::detail`] under [`FEW`])
//! also plays one loop where the other is under [`SOLO`], draws no
//! variation among its shots, and drops its layer. Every change is eased:
//! nothing steps, and a loop goes on where it is whether it is heard or
//! not.
//!
//! A level of rate saves no processor time — the voice makes 48 000
//! samples a second whatever it reads them from — but bytes, for a clip
//! shipped at a lower rate for what is heard only from afar; one loop
//! instead of two halves what a loop voice costs.
//!
//! Deterministic like every voice: the same clips, seed and drive give the
//! same samples.

use std::sync::Arc;

use crate::{Drive, NOMINAL_RMS, RATE, Voice, dsp::seed_for};

/// Under this much detail, a voice plays one loop where the other is
/// quieter than [`SOLO`], one shot of its set and no layer.
pub const FEW: f32 = 0.35;

/// With little detail, of two loops the quieter is left out under this
/// share of the pressure (−25 dB): two recordings differ in colour, and
/// left out at −16 dB the other changed a third of an octave by up to 10 dB
/// (`paris-soundscape lod`); at −25 dB, by 2.5 dB at most.
pub const SOLO: f32 = 0.056;

/// The levels kept of a clip: its own rate, then each half of the one
/// above.
pub const LEVELS: usize = 4;

/// The share of a level's half rate it holds without loss: its low-pass
/// is flat to there.
const HOLDS: f32 = 0.45;

/// How long a change of level, of loops or of layers takes, samples (50 ms).
const EASE: u32 = RATE / 20;

/// How long a shot making room for a newer one takes to fade out (5 ms).
const FADE: u32 = RATE / 200;

/// A recorded sound, mono, and its levels of detail: level `k` at
/// `RATE / 2^k`. A clip given at a lower rate has no level finer than its
/// own: one made at 12 kHz starts at level 2, and holds a quarter of the
/// bytes of one made at 48 kHz — what a world ships for what is only heard
/// from afar.
#[derive(Debug, PartialEq)]
pub struct Clip {
    /// Whether it is played round and round (a loop) or once (a shot).
    pub looped: bool,
    /// Its finest level.
    first: usize,
    /// Its levels from the finest.
    levels: Vec<Arc<[f32]>>,
    /// Its RMS at its finest level.
    rms: f32,
}

impl Clip {
    /// A clip from `samples` at `rate` Hz: kept at the finest level whose
    /// rate is at least its own (read between its samples to that rate if
    /// it is not one), and each coarser level made from it. A loop is taken
    /// as seamless: what follows its last sample is its first.
    /// A clip at a rate above [`RATE`] is low-passed under the engine's
    /// half rate before it is read at it; one at no rate (0) is silence.
    pub fn new(samples: &[f32], rate: u32, looped: bool) -> Clip {
        let first = (0..LEVELS).rev().find(|&k| RATE >> k >= rate).unwrap_or(0);
        let own: Vec<f32> = if rate == 0 {
            Vec::new()
        } else if rate == RATE >> first {
            samples.to_vec()
        } else if rate > RATE {
            let below = filter(samples, &taps(f64::from(HOLDS) * f64::from(RATE) / f64::from(rate)), looped);
            resample(&below, rate, RATE, looped)
        } else {
            resample(samples, rate, RATE >> first, looped)
        };
        let rms = (own.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / own.len().max(1) as f64).sqrt() as f32;
        let mut levels: Vec<Arc<[f32]>> = vec![own.into()];
        for _ in first + 1..LEVELS {
            let above = levels.last().expect("one level at least");
            levels.push(halve(above, looped).into());
        }
        Clip { looped, first, levels, rms }
    }

    /// The level played for level `k`: `k`, or the clip's finest if it has
    /// none so fine.
    pub fn played(&self, k: usize) -> usize {
        k.clamp(self.first, LEVELS - 1)
    }

    /// Its samples at the level played for `k`, at `RATE / 2^played(k)`.
    pub fn level(&self, k: usize) -> &[f32] {
        &self.levels[self.played(k) - self.first]
    }

    /// How long it is, in samples at [`RATE`].
    pub fn length(&self) -> usize {
        self.levels[0].len() << self.first
    }

    /// The bytes its levels hold: its finest, and all of them.
    pub fn bytes(&self) -> (usize, usize) {
        (self.levels[0].len() * 4, self.levels.iter().map(|level| level.len() * 4).sum())
    }

    /// How long it lasts, s.
    pub fn seconds(&self) -> f64 {
        self.length() as f64 / f64::from(RATE)
    }
}

/// The highest frequency level `k` holds, Hz.
pub fn holds(k: usize) -> f32 {
    HOLDS * RATE as f32 / (1u32 << k) as f32
}

/// A level is played only where it holds this much more than the top asked:
/// its low-pass has begun to fall just under what it holds.
const MARGIN: f32 = 1.2;

/// The coarsest level that still holds `top` Hz with [`MARGIN`] to spare,
/// for a clip played `ratio` times as fast as it was recorded (which
/// raises its top as much).
pub fn level_for(top: f32, ratio: f32) -> usize {
    (0..LEVELS).rev().find(|&k| holds(k) * ratio >= top * MARGIN).unwrap_or(0)
}

/// A low-pass of 63 taps, a windowed sinc (Blackman) cut at `cut` of the
/// rate. A level is made with a cut at a quarter of the rate above it, so
/// flat to [`HOLDS`] of the new half rate and 70 dB down past it.
fn taps(cut: f64) -> [f32; 63] {
    let mut taps = [0.0f32; 63];
    let centre = 31.0f64;
    for (n, tap) in taps.iter_mut().enumerate() {
        let x = n as f64 - centre;
        let sinc =
            if x == 0.0 { 2.0 * cut } else { (std::f64::consts::TAU * cut * x).sin() / (std::f64::consts::PI * x) };
        let w = std::f64::consts::TAU * n as f64 / 62.0;
        let window = 0.42 - 0.5 * w.cos() + 0.08 * (2.0 * w).cos();
        *tap = (sinc * window) as f32;
    }
    let sum: f32 = taps.iter().sum();
    taps.map(|tap| tap / sum)
}

/// The clip at half its rate: low-passed, every other sample. A loop wraps
/// round; a shot is silent outside itself.
fn halve(samples: &[f32], looped: bool) -> Vec<f32> {
    let taps = taps(0.25);
    let n = samples.len() as isize;
    let at = |i: isize| -> f32 {
        if looped {
            samples[i.rem_euclid(n.max(1)) as usize]
        } else if (0..n).contains(&i) {
            samples[i as usize]
        } else {
            0.0
        }
    };
    (0..samples.len().div_ceil(2))
        .map(|half| {
            let centre = 2 * half as isize;
            taps.iter().enumerate().map(|(t, tap)| tap * at(centre + t as isize - 31)).sum()
        })
        .collect()
}

/// `samples` through the low-pass `taps`, as many as they are.
fn filter(samples: &[f32], taps: &[f32; 63], looped: bool) -> Vec<f32> {
    let n = samples.len() as isize;
    let at = |i: isize| -> f32 {
        if looped {
            samples[i.rem_euclid(n.max(1)) as usize]
        } else if (0..n).contains(&i) {
            samples[i as usize]
        } else {
            0.0
        }
    };
    (0..n).map(|i| taps.iter().enumerate().map(|(t, tap)| tap * at(i + t as isize - 31)).sum()).collect()
}

/// A clip at `rate` brought to `to`, read between its
/// samples by four-point Lagrange interpolation (a recording is given at
/// the rate it was made at, 44.1 kHz often).
fn resample(samples: &[f32], rate: u32, to: u32, looped: bool) -> Vec<f32> {
    let step = f64::from(rate) / f64::from(to);
    let length = (samples.len() as f64 / step).floor() as usize;
    (0..length).map(|n| read(samples, n as f64 * step, looped)).collect()
}

/// A clip read at `position` (in its own samples), four-point Lagrange.
fn read(samples: &[f32], position: f64, looped: bool) -> f32 {
    let n = samples.len();
    if n == 0 {
        return 0.0;
    }
    let whole = position.floor();
    let f = (position - whole) as f32;
    let at = |i: i64| -> f32 {
        if looped {
            samples[i.rem_euclid(n as i64) as usize]
        } else if (0..n as i64).contains(&i) {
            samples[i as usize]
        } else {
            0.0
        }
    };
    let i = whole as i64;
    let (p, q, r, s) = (at(i - 1), at(i), at(i + 1), at(i + 2));
    // Lagrange through −1, 0, 1, 2.
    let c0 = -f * (f - 1.0) * (f - 2.0) / 6.0;
    let c1 = (f + 1.0) * (f - 1.0) * (f - 2.0) / 2.0;
    let c2 = -(f + 1.0) * f * (f - 2.0) / 2.0;
    let c3 = (f + 1.0) * f * (f - 1.0) / 6.0;
    c0 * p + c1 * q + c2 * r + c3 * s
}

/// One clip being played at a level, and the level it is leaving.
#[derive(Clone, Copy, Debug)]
struct Head {
    /// Where it is in the clip, in samples at [`RATE`] of its first level.
    at: f64,
    level: usize,
    /// The level it is leaving, and how far through that it is (0 to
    /// [`EASE`]); none when it is not changing.
    leaving: Option<(usize, u32)>,
}

impl Head {
    fn new() -> Head {
        Head { at: 0.0, level: 0, leaving: None }
    }

    /// Asks for level `wanted`: a change starts unless one is under way.
    fn ask(&mut self, wanted: usize) {
        if wanted != self.level && self.leaving.is_none() {
            self.leaving = Some((self.level, 0));
            self.level = wanted;
        }
    }

    /// The next sample of `clip`, gone `ratio` samples on: the level it is
    /// at, crossfaded from the one it is leaving.
    fn next(&mut self, clip: &Clip, ratio: f64) -> f32 {
        let sample = |k: usize| {
            let k = clip.played(k);
            read(clip.level(k), self.at / f64::from(1u32 << k), clip.looped)
        };
        let mut out = sample(self.level);
        if let Some((from, done)) = self.leaving {
            // Two levels of one clip are the same sound below the lower's
            // top: crossfaded by amplitude, not by power.
            let share = done as f32 / EASE as f32;
            out = out * share + sample(from) * (1.0 - share);
            self.leaving = (done + 1 < EASE).then_some((from, done + 1));
        }
        self.at += ratio;
        if clip.looped {
            self.at %= clip.length().max(1) as f64;
        }
        out
    }

    fn done(&self, clip: &Clip) -> bool {
        !clip.looped && self.at >= clip.length() as f64 + 4.0
    }
}

/// The RMS of the loudest 50 ms of `samples` at `rate` (all of them, if
/// shorter), looked at every 5 ms.
fn loudest(samples: &[f32], rate: u32) -> f32 {
    let window = (rate / 20) as usize;
    if samples.len() <= window {
        return (samples.iter().map(|s| s * s).sum::<f32>() / window as f32).sqrt();
    }
    samples
        .windows(window)
        .step_by(window / 10)
        .map(|w| (w.iter().map(|s| s * s).sum::<f32>() / window as f32).sqrt())
        .fold(0.0, f32::max)
}

/// What a loop follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Follows {
    /// The speed of what carries it, m/s.
    Speed,
    /// Its load, 0 to 1.
    Load,
}

impl Follows {
    fn of(self, drive: &Drive) -> f32 {
        let x = match self {
            Follows::Speed => drive.speed.abs(),
            Follows::Load => drive.load,
        };
        // What is not a number is taken for rest.
        if x.is_finite() { x } else { 0.0 }
    }
}

/// A synthesised voice sounding under a recorded one, at a gain.
pub struct Layer {
    pub voice: Box<dyn Voice>,
    pub gain: f32,
}

/// A layer, faded in and out as the detail asks.
struct Layered {
    layer: Layer,
    /// How much of it is heard now, 0 to 1.
    share: f32,
    scratch: Vec<f32>,
}

impl Layered {
    fn new(layer: Option<Layer>) -> Option<Layered> {
        layer.map(|layer| Layered { layer, share: 1.0, scratch: Vec::new() })
    }

    /// Adds the layer to `out`, unless it has faded out for little detail.
    fn add(&mut self, drive: &Drive, out: &mut [f32]) {
        let wanted = if drive.detail < FEW { 0.0 } else { 1.0 };
        let step = 1.0 / EASE as f32;
        if self.share == 0.0 && wanted == 0.0 {
            return;
        }
        self.scratch.resize(out.len(), 0.0);
        self.layer.voice.render(drive, &mut self.scratch);
        for (o, s) in out.iter_mut().zip(&self.scratch) {
            self.share += (wanted - self.share).clamp(-step, step);
            *o += s * self.layer.gain * self.share;
        }
    }
}

pub const LOOP: &str = "Clips each recorded at a known speed or load, the two either side crossfaded, each played faster or slower to follow the state";

/// A loop that follows a state.
pub struct Loop {
    /// The clips, each with the state it was recorded at, lowest first.
    clips: Vec<(Arc<Clip>, f32)>,
    follows: Follows,
    /// Each clip's gain to [`NOMINAL_RMS`], and its own playing head.
    gains: Vec<f32>,
    heads: Vec<Head>,
    /// How much of each clip is heard now, eased.
    shares: Vec<f32>,
    layer: Option<Layered>,
}

impl Loop {
    /// `clips`: the loops, each with the value of what it follows it was
    /// recorded at (it plays at its own speed there). Each is brought to
    /// [`NOMINAL_RMS`], so that the level holds between two.
    pub fn new(seed: u32, mut clips: Vec<(Arc<Clip>, f32)>, follows: Follows, layer: Option<Layer>) -> Loop {
        clips.sort_by(|a, b| a.1.total_cmp(&b.1));
        let gains = clips.iter().map(|(clip, _)| NOMINAL_RMS / clip.rms.max(1e-9)).collect();
        // Each loop starts at a place of its own, drawn from the seed: two
        // emitters of the same clips are not in step.
        let heads = clips
            .iter()
            .enumerate()
            .map(|(k, (clip, _))| {
                let draw = seed_for(u64::from(seed), &format!("loop {k}"));
                Head { at: f64::from(draw) / f64::from(u32::MAX) * clip.length() as f64, ..Head::new() }
            })
            .collect();
        let n = clips.len();
        Loop { clips, follows, gains, heads, shares: vec![0.0; n], layer: Layered::new(layer) }
    }

    /// For the state `x`: each clip's share (the two either side, at equal
    /// power on the scale of the state; with little detail, the nearest
    /// alone when the other is quieter than [`SOLO`]) and how fast it is
    /// played.
    fn mix(&self, x: f32, few: bool) -> (Vec<f32>, Vec<f32>) {
        let n = self.clips.len();
        let mut shares = vec![0.0; n];
        let at: Vec<f32> = self.clips.iter().map(|(_, at)| *at).collect();
        let above = at.iter().position(|&a| a > x).unwrap_or(n);
        if above == 0 || above == n || n == 1 {
            shares[above.min(n - 1).saturating_sub(usize::from(above == n))] = 1.0;
        } else {
            let (lo, hi) = (above - 1, above);
            let t = ((x - at[lo]) / (at[hi] - at[lo]).max(1e-9)).clamp(0.0, 1.0);
            let angle = t * std::f32::consts::FRAC_PI_2;
            let (cos, sin) = (angle.cos(), angle.sin());
            if few && sin.min(cos) < SOLO {
                shares[if t < 0.5 { lo } else { hi }] = 1.0;
            } else {
                shares[lo] = cos;
                shares[hi] = sin;
            }
        }
        // A clip recorded at a state plays at its own speed there, and
        // follows the state from there, from half to a quarter more; one
        // recorded at rest (0) plays at its own speed.
        let ratios = at.iter().map(|&a| if a > 0.0 { (x / a).clamp(0.5, 1.25) } else { 1.0 }).collect();
        (shares, ratios)
    }
}

impl Voice for Loop {
    fn render(&mut self, drive: &Drive, out: &mut [f32]) {
        out.fill(0.0);
        if self.clips.is_empty() {
            if let Some(layer) = &mut self.layer {
                layer.add(drive, out);
            }
            return;
        }
        let x = self.follows.of(drive);
        let (wanted, ratios) = self.mix(x, drive.detail < FEW);
        let step = 1.0 / EASE as f32;
        for (k, (clip, _)) in self.clips.iter().enumerate() {
            let head = &mut self.heads[k];
            if wanted[k] == 0.0 && self.shares[k] == 0.0 {
                // Unheard, it goes on all the same: where a loop is does not
                // depend on whether it was heard, and a level of detail
                // changes nothing of it but what it makes.
                head.at = (head.at + f64::from(ratios[k]) * out.len() as f64) % clip.length().max(1) as f64;
                continue;
            }
            head.ask(level_for(drive.top, ratios[k]));
            for o in out.iter_mut() {
                self.shares[k] += (wanted[k] - self.shares[k]).clamp(-step, step);
                *o += head.next(clip, f64::from(ratios[k])) * self.gains[k] * self.shares[k];
            }
        }
        if let Some(layer) = &mut self.layer {
            layer.add(drive, out);
        }
        for o in out.iter_mut() {
            *o = o.clamp(-1.0, 1.0);
        }
    }
}

pub const SHOT: &str =
    "Clips played once from the sample of a blow, as loud as it was hard, one drawn for each blow from the seed";

/// One-shots for what is struck.
pub struct Shot {
    clips: Vec<Arc<Clip>>,
    /// Each clip's gain: its loudest 50 ms brought to [`NOMINAL_RMS`].
    gains: Vec<f32>,
    seed: u32,
    /// How many blows it has played: what draws the next clip.
    blows: u64,
    /// The shots sounding: which clip, how loud, its head, how many samples
    /// before it starts in this block, and, for one making room for a newer,
    /// how many samples it has left to fade out.
    playing: Vec<(usize, f32, Head, usize, Option<u32>)>,
    layer: Option<Layered>,
}

impl Shot {
    /// Each clip is brought to where its loudest 50 ms have an RMS of
    /// [`NOMINAL_RMS`] at full strength — as loud as the synthesised blow
    /// of `impact` at its loudest, which is 0.07 to 0.12 — so that the
    /// clips of a set are as loud as one another.
    pub fn new(seed: u32, clips: Vec<Arc<Clip>>, layer: Option<Layer>) -> Shot {
        let gains =
            clips.iter().map(|clip| NOMINAL_RMS / loudest(clip.level(0), RATE >> clip.played(0)).max(1e-9)).collect();
        Shot { clips, gains, seed, blows: 0, playing: Vec::new(), layer: Layered::new(layer) }
    }
}

impl Voice for Shot {
    fn render(&mut self, drive: &Drive, out: &mut [f32]) {
        out.fill(0.0);
        if let Some(hit) = drive.hit
            && !self.clips.is_empty()
        {
            // From afar, one clip of the set for every blow.
            let pick = if drive.detail < FEW {
                0
            } else {
                seed_for(u64::from(self.seed), &format!("blow {}", self.blows)) as usize % self.clips.len()
            };
            self.blows += 1;
            // Two at most at once: a third makes the oldest fade out over
            // 5 ms, and one already fading makes way for it.
            if self.playing.len() == 3 {
                let fading = self.playing.iter().position(|shot| shot.4.is_some()).unwrap_or(0);
                self.playing.remove(fading);
            }
            if self.playing.iter().filter(|shot| shot.4.is_none()).count() == 2
                && let Some(oldest) = self.playing.iter_mut().find(|shot| shot.4.is_none())
            {
                oldest.4 = Some(FADE);
            }
            // (Not a number is no blow.)
            if hit.strength > 0.0 {
                let at = hit.at.min(out.len().saturating_sub(1));
                self.playing.push((pick, hit.strength.min(1.0), Head::new(), at, None));
            }
        }
        for (pick, strength, head, wait, fading) in &mut self.playing {
            let (clip, gain) = (&self.clips[*pick], self.gains[*pick]);
            head.ask(level_for(drive.top, 1.0));
            for o in out.iter_mut().skip(*wait) {
                let fade = match fading {
                    Some(left) => {
                        *left = left.saturating_sub(1);
                        *left as f32 / FADE as f32
                    }
                    None => 1.0,
                };
                *o += head.next(clip, 1.0) * *strength * gain * fade;
            }
            *wait = 0;
        }
        let clips = &self.clips;
        self.playing.retain(|(pick, _, head, _, fading)| !head.done(&clips[*pick]) && *fading != Some(0));
        if let Some(layer) = &mut self.layer {
            layer.add(drive, out);
        }
        for o in out.iter_mut() {
            *o = o.clamp(-1.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Hit;

    /// A tone of `hz` for `seconds`, a whole number of periods: a seamless loop.
    fn tone(hz: f64, seconds: f64) -> Vec<f32> {
        let periods = (hz * seconds).round();
        let n = (periods / hz * f64::from(RATE)).round() as usize;
        (0..n).map(|i| (0.3 * (std::f64::consts::TAU * periods * i as f64 / n as f64).sin()) as f32).collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|s| s * s).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    #[test]
    fn each_level_holds_the_clip_below_its_top_and_nothing_above() {
        // A tone under every level's top passes them all at its level; one above level 2's top is gone from it.
        for (hz, kept_to) in [(1_000.0, 3), (8_000.0, 1)] {
            let clip = Clip::new(&tone(hz, 1.0), RATE, true);
            let full = rms(clip.level(0));
            for k in 1..LEVELS {
                let level = rms(clip.level(k));
                if k <= kept_to {
                    assert!((level / full - 1.0).abs() < 0.01, "{hz} Hz at level {k}: {level} for {full}");
                } else {
                    assert!(level < 0.001 * full, "{hz} Hz at level {k}: {level}");
                }
            }
        }
        assert_eq!(level_for(20_000.0, 1.0), 0);
        assert_eq!(level_for(2_000.0, 1.0), 3);
        assert_eq!(level_for(4_000.0, 1.0), 2);
        assert_eq!(level_for(4_000.0, 0.5), 1);
        assert_eq!(level_for(5_000.0, 1.0), 1);
    }

    #[test]
    fn a_state_that_is_not_a_number_is_rest_for_a_block_and_no_more() {
        // Speed not a number in one block of 750: every sample a number, and after it the loop as before.
        let mut voice = Loop::new(4, engine(), Follows::Speed, None);
        let mut out = vec![0.0; 750 * 128];
        for (b, block) in out.chunks_mut(128).enumerate() {
            let speed = if b == 10 { f32::NAN } else { 7.0 };
            voice.render(&Drive { speed, ..Drive::default() }, block);
        }
        assert!(out.iter().all(|s| s.is_finite()));
        assert!((rms(&out[RATE as usize..]) / NOMINAL_RMS - 1.0).abs() < 0.08);
    }

    #[test]
    fn a_third_shot_fades_the_oldest_out_instead_of_cutting_it() {
        // A 200 Hz shot a second long, struck three times 10 blocks apart: no step at the third blow larger than
        // what two shots make from one sample to the next.
        let samples: Vec<f32> =
            (0..48_000).map(|i| 0.5 * (std::f32::consts::TAU * 200.0 * i as f32 / 48_000.0).sin()).collect();
        let mut voice = Shot::new(1, vec![Arc::new(Clip::new(&samples, RATE, false))], None);
        let mut out = vec![0.0; 40 * 128];
        for (b, block) in out.chunks_mut(128).enumerate() {
            let hit = (b % 10 == 0 && b < 30).then_some(Hit { at: 0, strength: 1.0 });
            voice.render(&Drive { hit, ..Drive::default() }, block);
        }
        let step =
            |from: usize, to: usize| out[from..to].windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        let (before, at) = (step(1_400, 2_550), step(2_550, 2_900));
        assert!(at < 1.5 * before, "{at} at the third blow, {before} before it");
        // A blow not a number, or past the block's end, plays at its last sample at most.
        let mut late = Shot::new(1, vec![Arc::new(Clip::new(&samples, RATE, false))], None);
        let mut block = [0.0f32; 128];
        late.render(&Drive { hit: Some(Hit { at: 0, strength: f32::NAN }), ..Drive::default() }, &mut block);
        assert!(block.iter().all(|s| *s == 0.0));
        late.render(&Drive { hit: Some(Hit { at: 100_000, strength: 1.0 }), ..Drive::default() }, &mut block);
        assert!(block[..127].iter().all(|s| *s == 0.0));
    }

    #[test]
    fn a_clip_at_no_rate_is_silence_and_one_above_the_engine_s_does_not_fold_down() {
        assert_eq!(Clip::new(&[0.5; 100], 0, true).length(), 0);
        // 30 kHz at 96 kHz is above what 48 kHz holds: nothing of it is left.
        let high: Vec<f32> = (0..96_000)
            .map(|i| (std::f64::consts::TAU * 30_000.0 * f64::from(i) / 96_000.0).sin() as f32 * 0.5)
            .collect();
        assert!(rms(Clip::new(&high, 96_000, true).level(0)) < 0.001);
        // 1 kHz at 96 kHz is kept.
        let low: Vec<f32> = (0..96_000)
            .map(|i| (std::f64::consts::TAU * 1_000.0 * f64::from(i) / 96_000.0).sin() as f32 * 0.5)
            .collect();
        let kept = rms(Clip::new(&low, 96_000, true).level(0));
        assert!((kept / (0.5 / 2f32.sqrt()) - 1.0).abs() < 0.01, "{kept}");
    }

    #[test]
    fn a_clip_at_another_rate_is_brought_to_the_engine_s() {
        let at = 44_100u32;
        let periods = 441.0;
        let samples: Vec<f32> =
            (0..44_100).map(|i| (std::f64::consts::TAU * periods * f64::from(i) / 44_100.0).sin() as f32).collect();
        let clip = Clip::new(&samples, at, true);
        assert_eq!(clip.level(0).len(), 48_000);
        let rises = clip.level(0).windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        assert!((rises as f64 - periods).abs() <= 1.0, "{rises}");
    }

    #[test]
    fn a_clip_made_at_a_quarter_of_the_rate_holds_a_quarter_and_plays_as_the_whole_one_s_level() {
        // A 1 kHz tone made at 12 kHz: it starts at level 2, with a quarter of the bytes; finer levels play
        // it; and it sounds as the same tone made at 48 kHz played at level 2.
        let at = |rate: u32| -> Vec<f32> {
            (0..rate / 2)
                .map(|i| (0.3 * (std::f64::consts::TAU * 1_000.0 * f64::from(i) / f64::from(rate)).sin()) as f32)
                .collect()
        };
        let (low, whole) = (Clip::new(&at(12_000), 12_000, true), Clip::new(&at(RATE), RATE, true));
        assert_eq!((low.played(0), low.played(3)), (2, 3));
        assert_eq!(low.bytes().0 * 4, whole.bytes().0);
        assert_eq!(low.length(), whole.length());
        let play = |clip: Clip, top: f32| {
            let mut voice = Loop::new(1, vec![(Arc::new(clip), 0.0)], Follows::Speed, None);
            let mut out = vec![0.0; 128 * 100];
            for block in out.chunks_mut(128) {
                voice.render(&Drive { top, ..Drive::default() }, block);
            }
            out
        };
        let (a, b) = (play(low, 24_000.0), play(whole, 4_000.0));
        assert!((rms(&a) / rms(&b) - 1.0).abs() < 0.01, "{} against {}", rms(&a), rms(&b));
    }

    /// `seconds` of a loop voice driven at speeds from `from` to `to`, m/s.
    fn sweep(voice: &mut Loop, from: f32, to: f32, seconds: f32, detail: f32, top: f32) -> Vec<f32> {
        let blocks = (seconds * RATE as f32 / 128.0) as usize;
        let mut out = vec![0.0; blocks * 128];
        for (b, block) in out.chunks_mut(128).enumerate() {
            let speed = from + (to - from) * b as f32 / blocks as f32;
            voice.render(&Drive { speed, detail, top, ..Drive::default() }, block);
        }
        out
    }

    fn engine() -> Vec<(Arc<Clip>, f32)> {
        vec![
            (Arc::new(Clip::new(&tone(100.0, 0.5), RATE, true)), 0.0),
            (Arc::new(Clip::new(&tone(150.0, 0.5), RATE, true)), 6.0),
            (Arc::new(Clip::new(&tone(240.0, 0.5), RATE, true)), 12.0),
        ]
    }

    #[test]
    fn a_loop_follows_its_state_without_a_click_and_twice_the_same() {
        let mut one = Loop::new(4, engine(), Follows::Speed, None);
        let heard = sweep(&mut one, 0.0, 14.0, 6.0, 1.0, 24_000.0);
        let mut two = Loop::new(4, engine(), Follows::Speed, None);
        assert_eq!(heard, sweep(&mut two, 0.0, 14.0, 6.0, 1.0, 24_000.0));
        // No step: the largest change between two samples is what the highest note makes at its loudest
        // (2π·300 Hz·0.3·gain / 48 kHz), and a little.
        let gain = NOMINAL_RMS / (0.3 / 2f32.sqrt());
        let most = std::f32::consts::TAU * 300.0 * 0.3 * gain / RATE as f32;
        let step = heard.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        assert!(step < 1.2 * most, "{step} for {most}");
        // The level holds between two loops, at equal power.
        for second in heard.chunks(RATE as usize) {
            assert!((rms(second) / NOMINAL_RMS - 1.0).abs() < 0.08, "{}", rms(second));
        }
        // The note follows the speed: at 9 m/s, the 150 Hz loop at a quarter more and the 240 Hz one at 3/4.
        let mut held = Loop::new(4, engine(), Follows::Speed, None);
        let steady = sweep(&mut held, 7.5, 7.5, 1.0, 1.0, 24_000.0);
        let rises = steady.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count() as f32;
        assert!(rises > 150.0 && rises < 240.0 * 0.75 + 10.0, "{rises} rises a second");
    }

    #[test]
    fn from_afar_a_loop_plays_a_lower_level_and_one_loop_with_nothing_heard_at_the_change() {
        // Held at 0.15 m/s, where the second loop is 28 dB under the first, the top falling from 24 kHz to
        // 1 kHz: levels change, and the detail falls under FEW, the second loop fading out. The 100 Hz note is
        // held by every level.
        let mut voice = Loop::new(9, engine(), Follows::Speed, None);
        let blocks = 3 * RATE as usize / 128;
        let mut out = vec![0.0; blocks * 128];
        for (b, block) in out.chunks_mut(128).enumerate() {
            let t = b as f32 / blocks as f32;
            let top = 24_000.0 * (1_000.0f32 / 24_000.0).powf(t);
            voice.render(&Drive { speed: 0.15, detail: 1.0 - 0.9 * t, top, ..Drive::default() }, block);
        }
        // The loop still heard, at the lowest level.
        let heard: Vec<usize> = (0..3).filter(|&k| voice.shares[k] > 0.0).collect();
        assert_eq!(heard.len(), 1);
        assert_eq!(voice.heads[heard[0]].level, LEVELS - 1);
        let step = out.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        let gain = NOMINAL_RMS / (0.3 / 2f32.sqrt());
        assert!(step < 1.2 * std::f32::consts::TAU * 150.0 * 0.3 * gain / RATE as f32, "{step}");
        // At the end, the nearest loop alone, at its level.
        let last = &out[out.len() - RATE as usize / 4..];
        assert!((rms(last) / NOMINAL_RMS - 1.0).abs() < 0.05, "{}", rms(last));
    }

    #[test]
    fn a_shot_plays_from_its_blow_as_loud_as_it_was_hard_and_draws_its_clip() {
        // Three bursts of a tone, 25, 50 and 75 ms long: told apart by their length.
        let bursts: Vec<Arc<Clip>> = (1..=3)
            .map(|k| {
                let samples: Vec<f32> =
                    (0..4_800).map(|i| if i < 1_200 * k { 0.3 * (i as f32 * 0.13).sin() } else { 0.0 }).collect();
                Arc::new(Clip::new(&samples, RATE, false))
            })
            .collect();
        let play = |seed: u32, detail: f32| {
            let mut voice = Shot::new(seed, bursts.clone(), None);
            let mut out = vec![0.0; 128 * 40 * 4];
            for (b, block) in out.chunks_mut(128).enumerate() {
                let hit = (b % 40 == 0).then_some(Hit { at: 17, strength: 0.5 });
                voice.render(&Drive { hit, detail, ..Drive::default() }, block);
            }
            out
        };
        let heard = play(3, 1.0);
        assert_eq!(heard, play(3, 1.0));
        // Silent before the blow's sample; its loudest 50 ms at half the nominal RMS.
        assert!(heard[..17].iter().all(|s| s.abs() < 1e-6));
        let blow = 128 * 40;
        let level = loudest(&heard[..blow], RATE);
        assert!((level / (0.5 * NOMINAL_RMS) - 1.0).abs() < 0.03, "{level}");
        // Four blows, not all the same clip; from afar, all the first.
        let lengths = |out: &[f32]| -> Vec<usize> {
            (0..4).map(|k| out[k * blow..(k + 1) * blow].iter().filter(|s| s.abs() > 0.01).count() / 600).collect()
        };
        let near = lengths(&heard);
        assert!(near.iter().any(|&l| l != near[0]), "{near:?}");
        let far = lengths(&play(3, 0.1));
        assert!(far.iter().all(|&l| l == far[0]), "{far:?}");
    }
}
