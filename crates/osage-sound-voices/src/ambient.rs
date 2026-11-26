// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! What is around the walker: their own steps, and the city far off.
//!
//! # Footsteps ([`Footsteps`], the kind `footsteps`)
//!
//! The steps of somebody walking on an asphalt pavement, the emitter at
//! their feet. In order:
//!
//! - **the pace:** a foot lands every step length walked, `Drive::speed`
//!   over `step-length` steps a second (1.87 at 1.4 m/s), each step up to
//!   8 % early or late; nothing under 0.2 m/s, and no more than 4.5 steps a
//!   second however fast the emitter is carried;
//! - **the heel strike:** a low thud — the push of the heel, a bump of force
//!   5 ms long, through a heavily damped band near 110 Hz — and a scuff of
//!   noise between 1 and 4 kHz, 20 to 40 ms until it is a tenth of its
//!   loudest (30 to 55 ms in all), part of it sparse grains: the grit of
//!   asphalt under a sole;
//! - **the roll onto the sole,** 100 to 150 ms later (sooner at a run): the
//!   same two parts, softer; the thud higher (near 165 Hz) and shorter, the
//!   scuff duller (0.8 to 3 kHz), rising slowly and lasting 50 to 75 ms, as
//!   the sole comes down from the heel to the toes;
//! - **the two feet:** each has its own pitch, strength, colour and time to
//!   roll, drawn once from the seed and a few percent apart; each step then
//!   varies around its foot's;
//! - **the strength** follows the speed: a faster walker treads harder.
//!
//! This is a sparse sound, so its level is set by its peak and not by its
//! RMS: a step peaks at about 0.5 at 1.4 m/s, one metre away (four steps in
//! five between 0.42 and 0.6, none under 0.33 or over 0.8: the scuff is
//! noise, and its own peaks come where they will), and the long-term RMS is
//! then about 0.035, some 9 dB under [`NOMINAL_RMS`]. Between two steps the
//! samples are exactly zero, and cost next to nothing.
//!
//! What it reads of the [`Drive`]: `speed` (pace and strength) and `detail`.
//! The pace follows the speed at once; strength and detail are read when a
//! heel lands and held for that step, so a change never clicks and a step
//! under way is finished as it began. Under a detail of 0.5 the sole's thud
//! and the grains are left out: the heel strike, which sets the peak, stays,
//! and so do the level and the colour of the scuffs.
//!
//! Parameters:
//!
//! - `step-length`, metres, 0.75: the ground covered by one step;
//! - `scuff`, 1: how much the sole scuffs, times the default (less for soft
//!   rubber, more for leather on grit).
//!
//! # The city far off ([`CityBed`], the kind `city-bed`)
//!
//! The hum of traffic on the quays and beyond, with nothing in it to point
//! at. In order:
//!
//! - **the body:** pink noise through a one-pole low-pass at 300 Hz and two
//!   one-pole high-passes at 45 Hz. Octave by octave it is level from 80 to
//!   300 Hz, falls 6 dB an octave above, and ever faster under 45 Hz
//!   (7 dB down at 40 Hz, 10 at 31.5, then 12 dB an octave);
//! - **the air:** a band of white noise two octaves wide round 1 kHz, 20 dB
//!   under the body: tyres hissing far away;
//! - **the drift:** the level of the whole moves within 2 dB, from louder
//!   to quieter and back, each move taking 5 to 20 s; the air has a drift
//!   of its own of the same size, so that the colour moves a little too.
//!   Lengths and levels are drawn from the seed: nothing comes round again;
//! - **half the rate:** so little of it is above a few kilohertz that it is
//!   made at 24 kHz, and every other sample is the mean of its two
//!   neighbours. What this mirrors above 12 kHz is noise like the rest,
//!   some 38 dB under the body: no more than the fall of 6 dB an octave
//!   would have left there.
//!
//! Its RMS is [`NOMINAL_RMS`] within 0.4 dB over a minute, and within 1 dB
//! over ten seconds, wherever the drift is. Two beds of different seeds
//! share nothing: one in each ear they are heard as a diffuse sound, and
//! their separate drifts move it a little from side to side.
//!
//! What it reads of the [`Drive`]: `detail` only. From a detail of 0.5 down
//! to 0.25 the air goes, eased over some 30 ms; under 0.25 it is no longer
//! made, and the body takes its share of the level.
//!
//! Parameters:
//!
//! - `drift`, decibels, 2: how far the level moves, from its lowest to its
//!   highest;
//! - `hiss`, decibels, −20: the level of the air against the body.
//!
//! # Assumptions
//!
//! No measurement of a footstep on asphalt, nor of the spectrum of Paris
//! heard from this bridge, was at hand. The shapes below follow the brief
//! and were checked on spectrograms, not by ear:
//!
//! - the brief gives a step's parts (a thud of 80 to 200 Hz, a scuff of 20
//!   to 40 ms at 1 to 4 kHz, the sole 100 to 150 ms later, 8 % in time);
//!   their pitches (110 and 165 Hz), dampings, the bump's length, the sole's
//!   scuff (band, 50 to 75 ms) and the levels of the parts (the heel's thud
//!   0.35 of a peak of 0.5, the sole at a quarter of the heel) are choices;
//! - "8 % in time" is taken as each step up to 8 % early or late, evenly;
//!   measured gaits are steadier;
//! - a step's strength varies by 15 %, its pitch by 6 %, its colour by 10 %;
//!   the feet are 4 to 10 % apart in pitch and strength, 6 to 16 % in
//!   colour, 8 to 20 % in the time they take to roll;
//! - strength grows as the speed to the power 0.7, between 0.5 and 1.4
//!   times that of 1.4 m/s; the step length does not grow with the speed;
//!   the roll onto the sole shortens only above walking pace; nobody takes
//!   more than 4.5 steps a second;
//! - a walker who has stood for a quarter of a second puts a foot down
//!   within half a step of setting out again;
//! - the grains of the scuff (one sample in 64, a third to a half of its
//!   power) stand for grit: no source;
//! - the city's body follows the brief (80 to 400 Hz, 6 dB an octave above
//!   300 Hz, rolled off under 40 Hz), the slopes read as levels in octave
//!   bands; this agrees with the research note's "63 to 500 Hz dominant",
//!   itself recalled, not read. The corner of 45 Hz is a choice;
//! - the drift crosses from louder to quieter at each move (0.4 to 1 dB
//!   either side), so that ten seconds stay within 1 dB of the nominal
//!   level: the brief asks for both;
//! - the air stands where the rolling noise of cars is loudest (the 1 kHz
//!   octave of the CNOSSOS-EU tables, Directive (EU) 2015/996); its width,
//!   and its own drift, are choices;
//! - which detail leaves out what (0.5, 0.25) is a choice.

use std::f32::consts::{FRAC_1_SQRT_2, PI};

use crate::{
    Drive, Params, Voice,
    dsp::{Band, Biquad, Low, NOMINAL_RMS, Noise, Pink, RATE, db},
};

/// Under this a sample is left as it is; over it, it is bent towards 1.
/// Neither voice reaches it at its nominal level (a step peaks under 0.8,
/// the bed stays under six times its RMS).
const KNEE: f32 = 0.85;

/// Keeps a sample within −1…1 without touching it under [`KNEE`]: unlike
/// `dsp::soft`, which bends everything a little, it is not at work at all
/// at the nominal level.
fn bound(x: f32) -> f32 {
    let over = x.abs() - KNEE;
    if over <= 0.0 {
        return x;
    }
    let room = 1.0 - KNEE;
    (KNEE + room * over / (room + over)).copysign(x)
}

pub const FOOTSTEPS: &str =
    "The steps of a walker on asphalt, heel then sole, at the pace of their speed; a step peaks at 0.5 at 1.4 m/s";
pub const FOOTSTEPS_PARAMS: &[(&str, f32)] = &[("step-length", 0.75), ("scuff", 1.0)];

/// Nobody steps under this speed, m/s: they stand, or shift their weight.
const SLOWEST: f32 = 0.2;
/// The speed the level is set at, m/s: an unhurried walk.
const WALK: f32 = 1.4;
/// Steps a second nobody exceeds, a sprinter's: an emitter carried faster
/// than legs go must not become a buzz.
const FASTEST_PACE: f32 = 4.5;
/// A step is early or late by up to this much of its length.
const SWING: f32 = 0.08;
/// One step, in the units the pace is counted in. Whole numbers, so that
/// many samples counted at once land where they would one by one.
const STEP: u64 = 1 << 32;
/// A step's peak at [`WALK`], one metre away.
const STEP_PEAK: f32 = 0.5;
/// Seconds from the heel to the sole at a walk, about: between 100 and
/// 150 ms once the foot and the step have had their say.
const ROLL: f32 = 0.125;
/// Samples a walker stands before the next foot is made ready: a pause
/// shorter than this is a hesitation, and the step goes on where it was.
const PAUSE: usize = RATE as usize / 4;
/// The push of a foot lasts this many periods of its thud: a thud is then
/// the same shape at every pitch.
const BUMP: f32 = 0.55;
/// One sample of noise in this many is a grain of grit.
const GRAINS: f32 = 64.0;

/// One of the two parts of a step.
struct Part {
    /// Centre of the thud, Hz, and how little it is damped.
    thud: f32,
    q: f32,
    /// The thud's peak, of a step's.
    weight: f32,
    /// What the band answers a push of 1 with, at its peak (measured).
    ring: f32,
    /// The scuff's band, Hz.
    band: (f32, f32),
    /// The scuff's RMS at its loudest, of a step's peak, and that of the
    /// noise through the band before any gain (measured).
    hiss: f32,
    through: f32,
    /// The scuff rises in this many seconds, and lasts between these.
    rise: f32,
    lasts: (f32, f32),
    /// How much of the scuff's power is grains.
    grit: f32,
}

const HEEL: Part = Part {
    thud: 110.0,
    q: 1.4,
    weight: 0.7,
    ring: 0.591,
    band: (1_000.0, 4_000.0),
    hiss: 0.17,
    through: 0.232,
    rise: 0.001,
    lasts: (0.030, 0.055),
    grit: 0.25,
};

const SOLE: Part = Part {
    thud: 165.0,
    q: 1.0,
    weight: 0.22,
    ring: 0.501,
    band: (800.0, 3_000.0),
    hiss: 0.10,
    through: 0.201,
    rise: 0.012,
    lasts: (0.050, 0.075),
    grit: 0.5,
};

/// The push of a foot on the ground, `u` of the way through: a smooth bump,
/// zero and level at both ends.
fn push(u: f32) -> f32 {
    if u < 1.0 { 16.0 * u * u * (1.0 - u) * (1.0 - u) } else { 0.0 }
}

/// What a scuff is made of: noise through a band, some of it grains.
struct Grit {
    noise: Noise,
    floor: Biquad,
    top: Biquad,
    smooth: f32,
    grain: f32,
}

impl Grit {
    fn new(noise: Noise) -> Self {
        Self { noise, floor: Biquad::default(), top: Biquad::default(), smooth: 1.0, grain: 0.0 }
    }

    /// Between `band.0` and `band.1` Hz, `grit` of its power in grains;
    /// nothing is left in the filters of what they passed before.
    fn tune(&mut self, band: (f32, f32), grit: f32) {
        (self.floor, self.top) = (Biquad::default(), Biquad::default());
        self.floor.high_pass(band.0, FRAC_1_SQRT_2);
        self.top.low_pass(band.1, FRAC_1_SQRT_2);
        // Grains and smooth noise share the power: a grain is one sample in
        // `GRAINS`, so that much stronger (white noise between −1 and 1 has
        // a power of a third).
        self.smooth = (1.0 - grit).sqrt();
        self.grain = (grit * GRAINS / 3.0).sqrt();
    }

    fn next(&mut self) -> f32 {
        let white = self.noise.next();
        // The largest samples of the noise are the grains: no second draw.
        let grain = if white.abs() > 1.0 - 1.0 / GRAINS { self.grain.copysign(white) } else { 0.0 };
        self.top.pass(self.floor.pass(white * self.smooth + grain))
    }
}

/// What one foot keeps from step to step, each about 1.
#[derive(Clone, Copy)]
struct Foot {
    pitch: f32,
    strength: f32,
    colour: f32,
    /// Seconds from the heel to the sole.
    roll: f32,
}

/// How a foot comes down this once.
#[derive(Clone, Copy)]
struct Tread {
    /// The step's peak.
    strength: f32,
    /// What the thud's pitch and the scuff's band are moved by, about 1.
    pitch: f32,
    colour: f32,
    /// With a thud, and with grains in the scuff.
    thud: bool,
    grains: bool,
}

/// A heel or a sole on the ground: a thud and a scuff, asleep between two
/// steps.
struct Contact {
    thud: Band,
    thud_gain: f32,
    /// One over the push's length in samples.
    bump: f32,
    grit: Grit,
    scuff_gain: f32,
    /// The scuff's length in samples, and one over its rise and its length.
    scuff: usize,
    rise: f32,
    fall: f32,
    age: usize,
    life: usize,
}

impl Contact {
    fn new(noise: Noise) -> Self {
        Self {
            thud: Band::default(),
            thud_gain: 0.0,
            bump: 0.0,
            grit: Grit::new(noise),
            scuff_gain: 0.0,
            scuff: 0,
            rise: 0.0,
            fall: 0.0,
            age: 0,
            life: 0,
        }
    }

    fn awake(&self) -> bool {
        self.age < self.life
    }

    /// Lands; `scuff` is how much the sole scuffs, times the part's own.
    fn strike(&mut self, part: &Part, tread: Tread, scuff: f32) {
        // The pace and the lengths below keep a foot from landing on its
        // own sound: cut short, it would click.
        debug_assert!(!self.awake(), "a foot landed while its last step still sounded");
        let rate = RATE as f32;
        let centre = part.thud * tread.pitch;
        let bump = (BUMP / centre * rate).max(2.0);
        // The band rings on after the push, falling by e every q/(π f)
        // seconds: ten of those, and what is cut is 87 dB under the thud.
        let ring = 10.0 * part.q / (PI * centre) * rate;
        self.thud = Band::at(centre, part.q);
        self.thud_gain = if tread.thud { tread.strength * part.weight / part.ring } else { 0.0 };
        self.bump = 1.0 / bump;

        let length = self.grit.noise.between(part.lasts.0, part.lasts.1) * rate;
        self.grit
            .tune((part.band.0 * tread.colour, part.band.1 * tread.colour), if tread.grains { part.grit } else { 0.0 });
        // A band moved up is as much wider: the same level needs less gain.
        self.scuff_gain = tread.strength * part.hiss * scuff / (part.through * tread.colour.sqrt());
        self.scuff = length as usize;
        self.rise = 1.0 / (part.rise * rate);
        self.fall = 1.0 / length;

        self.age = 0;
        self.life = if tread.thud { self.scuff.max((bump + ring) as usize) } else { self.scuff };
    }

    /// Adds its next samples to `out`, until it has died away.
    fn render(&mut self, out: &mut [f32]) {
        let n = out.len().min(self.life - self.age);
        for sample in &mut out[..n] {
            let t = self.age as f32;
            let mut y = 0.0;
            if self.thud_gain > 0.0 {
                y += self.thud.pass(push(t * self.bump)) * self.thud_gain;
            }
            if self.age < self.scuff {
                // Up in a straight line, then down ever more gently to
                // nothing at its end: nothing is cut.
                let fall = 1.0 - t * self.fall;
                y += self.grit.next() * (t * self.rise).min(1.0) * fall * fall * self.scuff_gain;
            }
            *sample += y;
            self.age += 1;
        }
    }
}

/// The sole that follows a heel already down.
struct Roll {
    /// Samples until it lands.
    after: usize,
    foot: Foot,
    strength: f32,
    full: bool,
}

/// The steps of somebody walking on asphalt (the kind `footsteps`): see the
/// module's documentation.
pub struct Footsteps {
    step_length: f32,
    scuff: f32,
    feet: [Foot; 2],
    side: usize,
    chance: Noise,
    /// How far into the step, in [`STEP`]s, and where the next foot lands.
    phase: u64,
    next: u64,
    /// Samples stood still.
    stood: usize,
    heel: Contact,
    sole: Contact,
    roll: Option<Roll>,
}

impl Footsteps {
    pub fn new(seed: u32, params: &Params) -> Self {
        let get = |name: &str| params.get(name, FOOTSTEPS_PARAMS.iter().find(|(n, _)| *n == name).map_or(0.0, |p| p.1));
        let mut chance = Noise::new(seed);
        let (heel, sole) = (Contact::new(chance.fork()), Contact::new(chance.fork()));
        // The two feet stand either side of the middle, never on it: they
        // always differ, by as little as `low` and as much as `high`.
        let mut apart = |low: f32, high: f32| {
            let half = chance.between(low, high) * if chance.next() < 0.0 { -0.5 } else { 0.5 };
            [1.0 - half, 1.0 + half]
        };
        let (pitch, strength, colour, roll) =
            (apart(0.04, 0.10), apart(0.04, 0.10), apart(0.06, 0.16), apart(0.08, 0.20));
        let feet =
            [0, 1].map(|i| Foot { pitch: pitch[i], strength: strength[i], colour: colour[i], roll: ROLL * roll[i] });
        // A walker met on their way is anywhere in the second half of a
        // step: two of them do not set off together, and the first foot
        // lands soon.
        let phase = (STEP as f64 * f64::from(chance.between(0.5, 1.0))) as u64;
        Self {
            step_length: get("step-length").max(0.05),
            scuff: get("scuff").max(0.0),
            feet,
            side: 0,
            chance,
            phase,
            next: STEP,
            stood: 0,
            heel,
            sole,
            roll: None,
        }
    }

    /// Steps a second at `speed`.
    fn pace(&self, speed: f32) -> f32 {
        let speed = speed.abs();
        if speed.is_nan() || speed < SLOWEST { 0.0 } else { (speed / self.step_length).min(FASTEST_PACE) }
    }

    fn strike_heel(&mut self, drive: &Drive, pace: f32) {
        self.phase -= self.next;
        self.next = (STEP as f64 * f64::from(1.0 + SWING * self.chance.next())) as u64;
        let foot = self.feet[self.side];
        self.side ^= 1;
        let harder = (drive.speed.abs() / WALK).powf(0.7).clamp(0.5, 1.4);
        let strength = STEP_PEAK * harder * foot.strength * self.chance.between(0.85, 1.15);
        let full = drive.detail >= 0.5;
        let pitch = foot.pitch * self.chance.between(0.94, 1.06);
        let colour = foot.colour * self.chance.between(0.9, 1.1);
        self.heel.strike(&HEEL, Tread { strength, pitch, colour, thud: true, grains: full }, self.scuff);
        // The foot rolls in 100 to 150 ms at a walk, and in less of the
        // step's time at a run.
        let walk = WALK / 0.75;
        let seconds = (foot.roll * self.chance.between(0.92, 1.08)).clamp(0.1, 0.15) * (walk / pace).clamp(0.4, 1.0);
        self.roll = Some(Roll { after: ((seconds * RATE as f32) as usize).max(1), foot, strength, full });
    }

    fn strike_sole(&mut self, roll: &Roll) {
        let strength = roll.strength * self.chance.between(0.85, 1.15);
        let pitch = roll.foot.pitch * self.chance.between(0.94, 1.06);
        let colour = roll.foot.colour * self.chance.between(0.9, 1.1);
        self.sole.strike(&SOLE, Tread { strength, pitch, colour, thud: roll.full, grains: roll.full }, self.scuff);
    }
}

impl Voice for Footsteps {
    fn render(&mut self, drive: &Drive, out: &mut [f32]) {
        out.fill(0.0);
        let pace = self.pace(drive.speed);
        let advance = (f64::from(pace) * STEP as f64 / f64::from(RATE)) as u64;
        if advance > 0 {
            self.stood = 0;
        } else {
            // Whoever has stood, and sets out again, puts a foot down
            // within half a step.
            self.stood = self.stood.saturating_add(out.len());
            if self.stood >= PAUSE {
                self.phase = self.phase.max(self.next / 2);
            }
        }
        let mut at = 0;
        while at < out.len() {
            if advance > 0 && self.phase >= self.next {
                self.strike_heel(drive, pace);
            }
            if let Some(roll) = self.roll.take_if(|roll| roll.after == 0) {
                self.strike_sole(&roll);
            }
            // As many samples as nothing lands in: to the next foot down,
            // or to the end of what was asked.
            let mut n = out.len() - at;
            if advance > 0 {
                n = n.min(usize::try_from((self.next - self.phase).div_ceil(advance)).unwrap_or(usize::MAX));
            }
            if let Some(roll) = &mut self.roll {
                n = n.min(roll.after);
                roll.after -= n;
            }
            if self.heel.awake() || self.sole.awake() {
                let now = &mut out[at..at + n];
                self.heel.render(now);
                self.sole.render(now);
                now.iter_mut().for_each(|sample| *sample = bound(*sample));
            }
            self.phase += advance * n as u64;
            at += n;
        }
    }
}

pub const CITY: &str =
    "The city far off and all around: a steady hum of distant traffic, a faint hiss of tyres, drifting slowly";
pub const CITY_PARAMS: &[(&str, f32)] = &[("drift", 2.0), ("hiss", -20.0)];

/// Samples between two updates of what moves slowly, as `osage-kart-sound`.
const TICK: usize = 32;
/// The bed is made at half of [`RATE`]. The filters of `dsp` count in
/// samples of [`RATE`], so a corner is given them this many times higher.
const HALF: f32 = 2.0;
/// Where the body starts to fall, and under what it is cut, Hz.
const BODY_TOP: f32 = 300.0;
const BODY_FLOOR: f32 = 45.0;
/// The air's band, Hz: one pole under it, two over.
const AIR: (f32, f32) = (600.0, 1_200.0);
/// The RMS of the body and of the air before any gain (measured).
const BODY_RMS: f32 = 0.699;
const AIR_RMS: f32 = 0.1014;
/// From this detail down the air goes, and under this one it is not made.
const AIR_DETAIL: (f32, f32) = (0.5, 0.25);
/// What the air comes or goes by at each [`TICK`]: all of it in 30 ms.
const AIR_EASE: f32 = TICK as f32 / (0.03 * RATE as f32);

/// The hum: pink noise between two corners, at half the rate.
struct Body {
    noise: Noise,
    pink: Pink,
    top: Low,
    floor: [Low; 2],
}

impl Body {
    fn new(noise: Noise) -> Self {
        let floor = Low::at(BODY_FLOOR * HALF);
        Self { noise, pink: Pink::default(), top: Low::at(BODY_TOP * HALF), floor: [floor.clone(), floor] }
    }

    fn next(&mut self) -> f32 {
        let low = self.top.pass(self.pink.pass(self.noise.next()));
        let once = self.floor[0].above(low);
        self.floor[1].above(once)
    }
}

/// The hiss of tyres: a wide band of white noise, at half the rate.
struct Air {
    noise: Noise,
    floor: Low,
    top: [Low; 2],
}

impl Air {
    fn new(noise: Noise) -> Self {
        let top = Low::at(AIR.1 * HALF);
        Self { noise, floor: Low::at(AIR.0 * HALF), top: [top.clone(), top] }
    }

    fn next(&mut self) -> f32 {
        let high = self.floor.above(self.noise.next());
        let once = self.top[0].pass(high);
        self.top[1].pass(once)
    }

    /// Nothing left in the filters: where it starts again from.
    fn hush(&mut self) {
        self.floor.set(0.0);
        self.top.iter_mut().for_each(|low| low.set(0.0));
    }
}

/// A gain that wanders about 1: from a level on one side of it to a level
/// on the other in 5 to 20 s, then back to another, slowing at each so that
/// no corner is heard. Crossing over each time keeps it from staying loud,
/// or quiet, for long: ten seconds of it are never far from 1.
struct Drift {
    noise: Noise,
    /// Decibels from the lowest to the highest.
    span: f32,
    /// The side it is going to: 1 louder, −1 quieter.
    side: f32,
    from: f32,
    to: f32,
    at: u32,
    length: u32,
}

impl Drift {
    fn new(mut noise: Noise, span: f32) -> Self {
        let side = if noise.next() < 0.0 { -1.0 } else { 1.0 };
        Self { noise, span, side, from: 1.0, to: 1.0, at: 0, length: 0 }
    }

    /// The gain for the next [`TICK`].
    fn next(&mut self) -> f32 {
        if self.at >= self.length {
            self.from = self.to;
            self.side = -self.side;
            self.to = db(self.side * self.noise.between(0.2, 0.5) * self.span);
            self.length = (self.noise.between(5.0, 20.0) * RATE as f32 / TICK as f32) as u32;
            self.at = 0;
        }
        let u = self.at as f32 / self.length as f32;
        self.at += 1;
        self.from + (self.to - self.from) * u * u * (3.0 - 2.0 * u)
    }
}

/// The city far off (the kind `city-bed`): see the module's documentation.
pub struct CityBed {
    body: Body,
    air: Air,
    level: Drift,
    breath: Drift,
    /// The air's share of the power, when all of it is there.
    share: f32,
    /// How much of the air is there, 0…1, and how much is asked for.
    airy: f32,
    wanted: f32,
    /// The gains of the body and of the air: now, what they move by at each
    /// sample made, and where that takes them by the next [`TICK`].
    gains: (f32, f32),
    steps: (f32, f32),
    aims: (f32, f32),
    /// Samples to make until the next update, and whether there was one.
    until: usize,
    started: bool,
    /// The last sample made, and whether it is still to be given: every
    /// other sample given is half-way between two made.
    last: f32,
    owed: bool,
}

impl CityBed {
    pub fn new(seed: u32, params: &Params) -> Self {
        let get = |name: &str| params.get(name, CITY_PARAMS.iter().find(|(n, _)| *n == name).map_or(0.0, |p| p.1));
        let mut noise = Noise::new(seed);
        let span = get("drift").clamp(0.0, 12.0);
        let hiss = db(get("hiss").min(0.0)).powi(2);
        Self {
            body: Body::new(noise.fork()),
            air: Air::new(noise.fork()),
            level: Drift::new(noise.fork(), span),
            breath: Drift::new(noise.fork(), span),
            share: hiss / (1.0 + hiss),
            airy: 0.0,
            wanted: 1.0,
            gains: (0.0, 0.0),
            steps: (0.0, 0.0),
            aims: (0.0, 0.0),
            until: 0,
            started: false,
            last: 0.0,
            owed: false,
        }
    }

    /// Once a [`TICK`]: the drifts move on, the air comes or goes.
    fn control(&mut self) {
        if !self.started {
            // A bed that starts far off starts without its air.
            (self.airy, self.started) = (self.wanted, true);
        } else if self.airy == 0.0 && self.wanted > 0.0 {
            self.air.hush();
        }
        self.airy += (self.wanted - self.airy).clamp(-AIR_EASE, AIR_EASE);
        let (level, breath) = (self.level.next(), self.breath.next());
        // The air keeps its share of the power whatever its own drift does
        // to it; without it the body takes the whole.
        let air = (self.share * (self.airy * breath).powi(2)).min(1.0);
        // Where the gains were going is where they are, but for what many
        // small sums are out by.
        self.gains = self.aims;
        self.aims = (level * NOMINAL_RMS * (1.0 - air).sqrt() / BODY_RMS, level * NOMINAL_RMS * air.sqrt() / AIR_RMS);
        let per = 2.0 / TICK as f32;
        self.steps = ((self.aims.0 - self.gains.0) * per, (self.aims.1 - self.gains.1) * per);
    }

    /// The next sample at half the rate, with the air or without.
    #[inline(always)]
    fn make(&mut self, air: bool) -> f32 {
        let mut y = self.body.next() * self.gains.0;
        self.gains.0 += self.steps.0;
        if air {
            y += self.air.next() * self.gains.1;
            self.gains.1 += self.steps.1;
        }
        bound(y)
    }
}

impl Voice for CityBed {
    fn render(&mut self, drive: &Drive, out: &mut [f32]) {
        // A detail that is not a number asks for none.
        let detail = if drive.detail.is_nan() { 0.0 } else { drive.detail };
        self.wanted = ((detail - AIR_DETAIL.1) / (AIR_DETAIL.0 - AIR_DETAIL.1)).clamp(0.0, 1.0);
        let mut rest = out;
        if self.owed && !rest.is_empty() {
            let (first, later) = rest.split_at_mut(1);
            first[0] = self.last;
            self.owed = false;
            rest = later;
        }
        while !rest.is_empty() {
            if self.until == 0 {
                self.control();
                self.until = TICK / 2;
            }
            let making = rest.len().div_ceil(2).min(self.until);
            let (now, later) = rest.split_at_mut((making * 2).min(rest.len()));
            self.until -= making;
            rest = later;
            // Without its air the bed is the body alone, and costs less.
            let air = self.gains.1 != 0.0 || self.steps.1 != 0.0;
            let (pairs, odd) = now.as_chunks_mut::<2>();
            for pair in pairs {
                let made = self.make(air);
                *pair = [0.5 * (self.last + made), made];
                self.last = made;
            }
            if let [odd] = odd {
                let made = self.make(air);
                *odd = 0.5 * (self.last + made);
                self.last = made;
                self.owed = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::make;

    const WALKING: Drive = Drive { speed: WALK, accel: 0.0, load: 0.0, hit: None, detail: 1.0, top: 24_000.0 };
    const STILL: Drive = Drive { speed: 0.0, accel: 0.0, load: 0.0, hit: None, detail: 1.0, top: 24_000.0 };

    fn steps(seed: u32) -> Box<dyn Voice> {
        make("footsteps", seed, &Params::default()).unwrap()
    }

    fn city(seed: u32) -> Box<dyn Voice> {
        make("city-bed", seed, &Params::default()).unwrap()
    }

    /// `seconds` more of a voice, asked for `block` samples at a time.
    fn play(voice: &mut dyn Voice, drive: Drive, seconds: f32, block: usize) -> Vec<f32> {
        let mut out = vec![0.0; (seconds * RATE as f32) as usize];
        out.chunks_mut(block).for_each(|chunk| voice.render(&drive, chunk));
        out
    }

    fn rms(s: &[f32]) -> f32 {
        (s.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>() / s.len() as f64).sqrt() as f32
    }

    fn decibels(x: f32) -> f32 {
        20.0 * x.max(1e-9).log10()
    }

    fn peak(s: &[f32]) -> f32 {
        s.iter().fold(0.0, |most, x| most.max(x.abs()))
    }

    fn mean(xs: &[f32]) -> f32 {
        (xs.iter().map(|x| f64::from(*x)).sum::<f64>() / xs.len() as f64) as f32
    }

    /// How far `xs` spread about their mean, as a share of it.
    fn spread(xs: &[f32]) -> f32 {
        let m = mean(xs);
        (xs.iter().map(|x| (x - m).powi(2)).sum::<f32>() / xs.len() as f32).sqrt() / m
    }

    /// The level of `s` round `centre`, dB: three octave bands one after
    /// the other, so that a loud band two octaves away does not leak in.
    fn octave(s: &[f32], centre: f32) -> f32 {
        let mut bands = [Biquad::default(), Biquad::default(), Biquad::default()];
        bands.iter_mut().for_each(|band| _ = band.band_pass(centre, std::f32::consts::SQRT_2));
        // A whisper under everything keeps the filters, in the silence
        // between two steps, out of numbers too small to compute fast.
        let mut whisper = 1e-9f32;
        let through = s.iter().map(|x| {
            whisper = -whisper;
            bands.iter_mut().fold(*x + whisper, |y, band| band.pass(y))
        });
        decibels(rms(&through.collect::<Vec<f32>>()))
    }

    /// How alike two sounds are, −1…1, the second `lag` samples late.
    fn alike(a: &[f32], b: &[f32], lag: usize) -> f32 {
        let dot = |x: &[f32], y: &[f32]| x.iter().zip(y).map(|(p, q)| f64::from(*p) * f64::from(*q)).sum::<f64>();
        (dot(a, &b[lag..]) / (dot(a, a) * dot(b, b)).sqrt()) as f32
    }

    /// The largest move from one sample to the next: what a click would be.
    fn largest_step(s: &[f32]) -> f32 {
        s.windows(2).fold(0.0, |most, pair| most.max((pair[1] - pair[0]).abs()))
    }

    /// Where sound starts and ends: runs of samples with no more than 5 ms
    /// of exact silence inside. At a walk a step is two of them, heel then
    /// sole.
    fn bursts(s: &[f32]) -> Vec<(usize, usize)> {
        let (mut found, mut start, mut last) = (Vec::new(), None, 0);
        for (i, x) in s.iter().enumerate() {
            if *x != 0.0 {
                start.get_or_insert(i);
                last = i;
            } else if start.is_some() && i - last > 240 {
                found.push((start.take().unwrap(), last + 1));
            }
        }
        found
    }

    /// The steps of `s`, each as its heel and its sole; a step cut by the
    /// end is left out.
    fn heels_and_soles(s: &[f32]) -> Vec<[(usize, usize); 2]> {
        bursts(s).as_chunks::<2>().0.to_vec()
    }

    #[test]
    fn a_walker_steps_once_every_step_length() {
        let s = play(&mut *steps(1), WALKING, 60.0, 128);
        let count = heels_and_soles(&s).len() as f32;
        assert!((count - 60.0 * 1.4 / 0.75).abs() <= 2.0, "{count} steps in a minute at 1.4 m/s");

        let short = Params::default().with("step-length", 0.5);
        let s = play(&mut *make("footsteps", 1, &short).unwrap(), WALKING, 60.0, 128);
        let count = heels_and_soles(&s).len() as f32;
        assert!((count - 60.0 * 1.4 / 0.5).abs() <= 2.0, "{count} steps of half a metre in a minute");
    }

    #[test]
    fn nobody_steps_under_a_fifth_of_a_metre_a_second() {
        for speed in [0.0, 0.19, -0.1, f32::NAN] {
            let s = play(&mut *steps(2), Drive { speed, ..STILL }, 20.0, 128);
            assert!(s.iter().all(|x| *x == 0.0), "a step at {speed} m/s");
        }
        let s = play(&mut *steps(2), Drive { speed: 0.25, ..STILL }, 20.0, 128);
        let count = heels_and_soles(&s).len();
        assert!((5..=8).contains(&count), "{count} steps in 20 s at 0.25 m/s");
    }

    #[test]
    fn a_step_peaks_at_about_a_half_at_walking_pace_and_the_long_term_level_is_well_under_nominal() {
        for seed in [3, 4, 5] {
            let s = play(&mut *steps(seed), WALKING, 120.0, 128);
            let mut peaks: Vec<f32> = heels_and_soles(&s).iter().map(|step| peak(&s[step[0].0..step[1].1])).collect();
            peaks.sort_by(f32::total_cmp);
            let part = |share: f32| peaks[((peaks.len() - 1) as f32 * share) as usize];
            assert!((0.46..0.54).contains(&part(0.5)), "seed {seed}: the middle step peaks at {}", part(0.5));
            // Four steps in five within 2 dB of it, and none far off: the
            // scuff is noise, and its own peaks come where they will.
            assert!(part(0.1) > 0.39 && part(0.9) < 0.63, "seed {seed}: most from {} to {}", part(0.1), part(0.9));
            assert!(part(0.0) > 0.32 && part(1.0) < 0.82, "seed {seed}: all from {} to {}", part(0.0), part(1.0));
            // Under the knee: the limiter was not at work.
            assert!(part(1.0) < KNEE);
            let level = decibels(rms(&s) / NOMINAL_RMS);
            assert!((-11.0..-7.0).contains(&level), "seed {seed}: {level} dB against the nominal level");
        }
    }

    #[test]
    fn a_step_is_a_heel_strike_then_the_softer_roll_onto_the_sole() {
        let s = play(&mut *steps(4), WALKING, 60.0, 128);
        for [heel, sole] in heels_and_soles(&s) {
            let apart = (sole.0 - heel.0) as f32 / RATE as f32;
            assert!((0.099..0.151).contains(&apart), "the sole {apart} s after the heel");
            let (heel, sole) = (&s[heel.0..heel.1], &s[sole.0..sole.1]);
            assert!(peak(sole) < 0.6 * peak(heel), "sole {} heel {}", peak(sole), peak(heel));
            assert!(rms(sole) * (sole.len() as f32).sqrt() < 0.6 * rms(heel) * (heel.len() as f32).sqrt());
            // The heel is struck, the sole rolls: it takes longer.
            assert!(heel.len() < RATE as usize * 60 / 1_000, "a heel of {} samples", heel.len());
            assert!((RATE as usize * 45 / 1_000..RATE as usize * 76 / 1_000).contains(&sole.len()));
        }
    }

    #[test]
    fn a_heel_strike_is_a_low_thud_and_a_scuff_of_one_to_four_kilohertz() {
        let s = play(&mut *steps(5), WALKING, 60.0, 128);
        // Only the heels: the soles silenced.
        let mut heels = vec![0.0; s.len()];
        for [heel, _] in heels_and_soles(&s) {
            heels[heel.0..heel.1].copy_from_slice(&s[heel.0..heel.1]);
        }
        let level = |centre: f32| octave(&heels, centre);
        // The thud is the heaviest part, in the octave of 125 Hz; between
        // it and the scuff there is a gap, and little above the scuff.
        assert!(level(125.0) > level(63.0) && level(125.0) > level(250.0) + 3.0);
        assert!(level(2_000.0) > level(500.0) + 8.0, "scuff {} gap {}", level(2_000.0), level(500.0));
        assert!(level(2_000.0) > level(8_000.0) + 5.0, "scuff {} top {}", level(2_000.0), level(8_000.0));
        // White noise between two corners: as much again an octave up, as
        // far as the upper corner.
        assert!((level(4_000.0) - level(2_000.0)).abs() < 3.0, "{} {}", level(2_000.0), level(4_000.0));
        assert!((2.0..7.0).contains(&(level(2_000.0) - level(1_000.0))), "{} {}", level(1_000.0), level(2_000.0));
        // The scuff lasts 20 to 40 ms: what is over 500 Hz is by then a
        // tenth of its loudest, whatever the thud still does.
        for [heel, _] in heels_and_soles(&s).into_iter().take(20) {
            let mut low = Low::at(500.0);
            let high: Vec<f32> = s[heel.0..heel.1].iter().map(|x| low.above(*x)).collect();
            let loudest = peak(&high);
            let ends = high.iter().rposition(|x| x.abs() > 0.1 * loudest).unwrap() as f32 / RATE as f32;
            assert!((0.015..0.042).contains(&ends), "a scuff of {ends} s");
        }
    }

    #[test]
    fn steps_vary_in_time_by_up_to_eight_percent_and_in_strength() {
        let s = play(&mut *steps(6), WALKING, 120.0, 128);
        let found = heels_and_soles(&s);
        let gaps: Vec<f32> = found.windows(2).map(|pair| (pair[1][0].0 - pair[0][0].0) as f32 / RATE as f32).collect();
        let even = 0.75 / WALK;
        assert!((mean(&gaps) - even).abs() < 0.01 * even, "a step every {} s", mean(&gaps));
        assert!(gaps.iter().all(|gap| (gap / even - 1.0).abs() < 0.081), "a step more than 8 % early or late");
        assert!((0.035..0.06).contains(&spread(&gaps)), "steps {} apart in time", spread(&gaps));
        let peaks: Vec<f32> = found.iter().map(|step| peak(&s[step[0].0..step[0].1])).collect();
        assert!((0.05..0.2).contains(&spread(&peaks)), "steps {} apart in strength", spread(&peaks));
    }

    #[test]
    fn left_and_right_feet_differ_a_little() {
        for seed in [7, 8, 9] {
            let s = play(&mut *steps(seed), WALKING, 120.0, 128);
            let found = heels_and_soles(&s);
            // The time a foot takes to roll is its own: every other step.
            let rolls = |foot: usize| -> Vec<f32> {
                found.iter().skip(foot).step_by(2).map(|step| (step[1].0 - step[0].0) as f32 / RATE as f32).collect()
            };
            let (left, right) = (mean(&rolls(0)), mean(&rolls(1)));
            let apart = (left - right).abs();
            assert!((0.004..0.03).contains(&apart), "seed {seed}: feet roll in {left} and {right} s");
            // And so are its pitch, its strength and its colour.
            let feet = Footsteps::new(seed, &Params::default()).feet;
            for (a, b) in
                [(feet[0].pitch, feet[1].pitch), (feet[0].strength, feet[1].strength), (feet[0].colour, feet[1].colour)]
            {
                assert!((0.039..0.161).contains(&(a - b).abs()), "seed {seed}: feet at {a} and {b}");
            }
        }
    }

    #[test]
    fn a_faster_walker_treads_harder_and_more_often_and_stays_in_range() {
        let stroll = play(&mut *steps(10), Drive { speed: 0.8, ..STILL }, 30.0, 128);
        let walk = play(&mut *steps(10), WALKING, 30.0, 128);
        let run = play(&mut *steps(10), Drive { speed: 3.5, ..STILL }, 30.0, 128);
        assert!(peak(&stroll) < peak(&walk) && peak(&walk) < peak(&run));
        assert!(rms(&stroll) < 0.7 * rms(&walk) && rms(&walk) < 0.7 * rms(&run));
        // Faster than legs go: no more than 4.5 steps a second, and bounded.
        let car = play(&mut *steps(10), Drive { speed: 14.0, ..STILL }, 30.0, 128);
        assert_eq!(Footsteps::new(10, &Params::default()).pace(14.0), FASTEST_PACE);
        // At that pace a sole may land before its heel has died away, and
        // the two are then one burst.
        let found = bursts(&car).len() as f32 / 30.0;
        assert!((4.3..9.2).contains(&found), "{found} heels and soles a second");
        for s in [&stroll, &walk, &run, &car] {
            assert!(s.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
        }
    }

    #[test]
    fn more_scuff_is_more_of_the_top_and_none_of_the_thud() {
        let with = |scuff: f32| {
            let params = Params::default().with("scuff", scuff);
            play(&mut *make("footsteps", 11, &params).unwrap(), WALKING, 30.0, 128)
        };
        let (none, usual, much) = (with(0.0), with(1.0), with(2.0));
        assert!((octave(&much, 2_000.0) - octave(&usual, 2_000.0) - 6.0).abs() < 1.0);
        assert!(octave(&none, 2_000.0) < octave(&usual, 2_000.0) - 20.0);
        assert!((octave(&none, 125.0) - octave(&much, 125.0)).abs() < 0.5);
    }

    #[test]
    fn the_same_seed_gives_the_same_steps_and_another_seed_others() {
        let (a, b) = (play(&mut *steps(12), WALKING, 30.0, 128), play(&mut *steps(12), WALKING, 30.0, 128));
        assert!(a == b);
        for other in [13, 14, 15] {
            let other = play(&mut *steps(other), WALKING, 30.0, 128);
            assert!(other != a);
            // Whatever the few milliseconds between them.
            let most = (0..2_400).step_by(48).map(|lag| alike(&a, &other, lag).abs()).fold(0.0, f32::max);
            assert!(most < 0.1, "two walkers alike by {most}");
        }
    }

    #[test]
    fn steps_leave_no_offset() {
        let s = play(&mut *steps(14), WALKING, 60.0, 128);
        assert!(mean(&s).abs() < 1e-4, "{}", mean(&s));
        // Nor does one step: a thud pushes the air out and lets it back.
        for [heel, sole] in heels_and_soles(&s) {
            let sum: f32 = s[heel.0..sole.1].iter().sum();
            assert!(sum.abs() < 0.2, "a step sums to {sum}");
        }
    }

    #[test]
    fn steps_are_the_same_whatever_the_length_of_the_blocks() {
        let whole = play(&mut *steps(15), WALKING, 10.0, 128);
        for block in [1, 77, 4_096] {
            assert!(play(&mut *steps(15), WALKING, 10.0, block) == whole, "blocks of {block}");
        }
    }

    #[test]
    fn a_step_under_way_is_finished_as_it_began_whatever_changes() {
        let steady = play(&mut *steps(16), WALKING, 6.0, 128);
        let found = heels_and_soles(&steady);
        // Ten milliseconds into the third heel strike, the thud at its
        // loudest: the worst moment for anything to change.
        let ([heel, sole], next) = (found[2], found[3][0].0);
        let change = (heel.0 + 480).next_multiple_of(128);
        assert!(change < heel.1);
        for after in [STILL, Drive { speed: 3.0, ..STILL }, Drive { detail: 0.0, ..WALKING }] {
            let mut voice = steps(16);
            let mut s = play(&mut *voice, WALKING, change as f32 / RATE as f32, 128);
            assert_eq!(s.len(), change);
            s.extend(play(&mut *voice, after, 3.0, 128));
            // The step is the one it would have been.
            assert!(s[..sole.1] == steady[..sole.1], "the step changed under way");
            // So no move from a sample to the next is larger than the
            // steady walk's own.
            assert!(largest_step(&s[change - 240..sole.1]) <= largest_step(&steady));
            if after.speed == 0.0 {
                assert!(s[sole.1..].iter().all(|x| *x == 0.0), "a step after stopping");
            } else if after.speed > WALK {
                let first = bursts(&s[sole.1..])[0].0 + sole.1;
                assert!(first < next, "the pace did not quicken");
            }
        }
    }

    #[test]
    fn whatever_the_drive_does_no_foot_lands_on_its_own_sound() {
        // The speed and the detail change at random every few blocks, from
        // standing to faster than legs go: a foot cut short by the next
        // would trip the check in `Contact::strike`, and would click.
        let sprint = play(&mut *steps(18), Drive { speed: 14.0, ..STILL }, 30.0, 128);
        for seed in [18, 19] {
            let (mut voice, mut chance) = (steps(seed), Noise::new(seed));
            let mut s = vec![0.0; RATE as usize * 60];
            let mut drive = WALKING;
            for block in s.chunks_mut(128) {
                if chance.unit() < 0.05 {
                    drive.speed = [0.0, 0.1, 0.5, WALK, 3.0, 14.0, -14.0][(chance.unit() * 7.0) as usize];
                    drive.detail = chance.unit();
                }
                voice.render(&drive, block);
            }
            assert!(s.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
            assert!(bursts(&s).len() > 60, "seed {seed}: {} heels and soles", bursts(&s).len());
            assert!(largest_step(&s) <= largest_step(&sprint), "seed {seed}: {}", largest_step(&s));
        }
    }

    #[test]
    fn a_walker_who_sets_out_puts_a_foot_down_within_half_a_step() {
        for seed in 20..30 {
            let mut voice = steps(seed);
            play(&mut *voice, WALKING, 5.3, 128);
            play(&mut *voice, STILL, 2.0, 128);
            let s = play(&mut *voice, WALKING, 2.0, 128);
            let first = bursts(&s)[0].0 as f32 / RATE as f32;
            assert!(first <= 0.55 * 0.75 / WALK, "seed {seed}: the first foot down after {first} s");
        }
    }

    #[test]
    fn less_detail_keeps_the_heel_strike_and_its_level() {
        let full = play(&mut *steps(17), WALKING, 60.0, 128);
        let less = play(&mut *steps(17), Drive { detail: 0.2, ..WALKING }, 60.0, 128);
        let (a, b) = (heels_and_soles(&full), heels_and_soles(&less));
        assert_eq!(a.len(), b.len());
        let peaks = |s: &[f32], found: &[[(usize, usize); 2]]| -> Vec<f32> {
            found.iter().map(|step| peak(&s[step[0].0..step[0].1])).collect()
        };
        let (loud, cheap) = (mean(&peaks(&full, &a)), mean(&peaks(&less, &b)));
        assert!((decibels(cheap / loud)).abs() < 1.0, "peaks of {loud} and {cheap}");
        assert!(decibels(rms(&less) / rms(&full)).abs() < 1.0);
        for centre in [125.0, 2_000.0] {
            assert!((octave(&less, centre) - octave(&full, centre)).abs() < 1.5);
        }
    }

    #[test]
    fn the_city_sounds_at_the_nominal_level() {
        for seed in [1, 2, 3, 4] {
            let s = play(&mut *city(seed), Drive { load: 0.5, ..STILL }, 120.0, 128);
            let level = decibels(rms(&s) / NOMINAL_RMS);
            assert!(level.abs() < 0.5, "seed {seed}: {level} dB over two minutes");
            for (i, window) in s.chunks(RATE as usize * 10).enumerate() {
                let level = decibels(rms(window) / NOMINAL_RMS);
                assert!(level.abs() < 1.0, "seed {seed}: {level} dB over the ten seconds from {}", i * 10);
            }
            // Under the knee: the limiter was not at work.
            assert!(peak(&s) < KNEE && s.iter().all(|x| x.is_finite()), "seed {seed}: a peak of {}", peak(&s));
        }
    }

    #[test]
    fn the_city_has_the_weight_of_distant_traffic() {
        let s = play(&mut *city(5), STILL, 120.0, 128);
        let level = |centre: f32| octave(&s, centre);
        // A body level from 80 to 300 Hz, octave by octave.
        let body = [level(90.0), level(125.0), level(250.0)];
        assert!(body.iter().all(|band| (band - body[1]).abs() < 2.0), "{body:?}");
        // Rolled off under 40 Hz.
        assert!(level(31.5) < body[1] - 6.0 && level(16.0) < body[1] - 16.0, "{} {}", level(31.5), level(16.0));
        // Falling above 300 Hz: 500 Hz is under the body, 1 kHz further,
        // and so on up, 6 dB an octave once well above the corner.
        let fall = [level(500.0), level(1_000.0), level(2_000.0), level(4_000.0), level(8_000.0)];
        assert!((2.0..6.0).contains(&(body[2] - fall[0])), "{} {}", body[2], fall[0]);
        for pair in fall.windows(2) {
            assert!((3.5..11.0).contains(&(pair[0] - pair[1])), "{fall:?}");
        }
        assert!(fall[4] < body[1] - 25.0, "{fall:?}");
    }

    #[test]
    fn the_air_of_the_city_is_twenty_decibels_under_and_near_one_kilohertz() {
        let with = |hiss: f32| {
            let params = Params::default().with("hiss", hiss).with("drift", 0.0);
            play(&mut *make("city-bed", 6, &params).unwrap(), STILL, 60.0, 128)
        };
        let (none, usual) = (with(-120.0), with(-20.0));
        // What the air adds is the difference: same seed, same body.
        let air: Vec<f32> = usual.iter().zip(&none).map(|(a, b)| a - b).collect();
        let under = decibels(rms(&air) / rms(&none));
        assert!((under + 20.0).abs() < 0.5, "the air {under} dB against the body");
        let bands = [250.0, 500.0, 1_000.0, 2_000.0, 4_000.0];
        let loudest = bands.into_iter().max_by(|a, b| octave(&air, *a).total_cmp(&octave(&air, *b)));
        assert_eq!(loudest, Some(1_000.0));
        // And the level is the same with it and without.
        assert!(decibels(rms(&usual) / rms(&none)).abs() < 0.1);
    }

    #[test]
    fn the_city_made_at_half_the_rate_keeps_falling_to_the_top() {
        let s = play(&mut *city(17), STILL, 30.0, 128);
        let level = |centre: f32| octave(&s, centre) - octave(&s, 125.0);
        println!("4 kHz {}, 8 kHz {}, 16 kHz {}", level(4_000.0), level(8_000.0), level(16_000.0));
        // What is mirrored above 12 kHz is noise as the rest, and no more
        // of it than the fall of 6 dB an octave would have left there.
        assert!(level(16_000.0) < level(8_000.0) - 4.0, "{} {}", level(16_000.0), level(8_000.0));
        assert!(level(16_000.0) < -34.0, "{}", level(16_000.0));
    }

    #[test]
    fn the_city_drifts_by_two_decibels_slowly_and_has_no_event() {
        let levels = |drift: f32| -> Vec<f32> {
            let params = Params::default().with("drift", drift);
            let s = play(&mut *make("city-bed", 7, &params).unwrap(), STILL, 240.0, 128);
            // No event: nothing stands out of the noise.
            assert!(peak(&s) < 6.0 * rms(&s), "a peak {} times the level", peak(&s) / rms(&s));
            s.chunks(RATE as usize * 4).map(|window| decibels(rms(window))).collect()
        };
        let range = |levels: &[f32]| {
            levels.iter().fold(f32::MIN, |m, x| m.max(*x)) - levels.iter().fold(f32::MAX, |m, x| m.min(*x))
        };
        let (steady, usual, wide) = (range(&levels(0.0)), range(&levels(2.0)), range(&levels(6.0)));
        assert!(steady < 1.0, "a steady city moves by {steady} dB");
        assert!((1.2..2.8).contains(&usual), "the city moves by {usual} dB");
        assert!(wide > 3.5, "a city asked to move by 6 dB moves by {wide}");
        // Slowly: from four seconds to the next, a fraction of a decibel.
        let usual = levels(2.0);
        let quickest = usual.windows(2).fold(0.0f32, |m, pair| m.max((pair[1] - pair[0]).abs()));
        assert!(quickest < 1.2, "{quickest} dB in four seconds");
    }

    #[test]
    fn the_city_never_comes_round_again() {
        let s = play(&mut *city(8), STILL, 40.0, 128);
        // From a tenth of a second to ten seconds late, it is never like
        // itself: no loop for the ear to find.
        let lags = (4_800..480_000).step_by(4_801);
        let most = lags.map(|lag| alike(&s[..s.len() - lag], &s, lag).abs()).fold(0.0, f32::max);
        assert!(most < 0.05, "like itself by {most}");
    }

    #[test]
    fn two_cities_are_incoherent_and_one_seed_is_one_city() {
        let (a, b) = (play(&mut *city(9), STILL, 10.0, 128), play(&mut *city(9), STILL, 10.0, 128));
        assert!(a == b);
        for other in [10, 11, 12] {
            let other = play(&mut *city(other), STILL, 10.0, 128);
            // Whatever the ears' few samples of delay between them.
            let both = |lag: usize| alike(&a, &other, lag).abs().max(alike(&other, &a, lag).abs());
            let most = (0..96).map(both).fold(0.0, f32::max);
            assert!(most < 0.1, "two cities alike by {most}");
        }
    }

    #[test]
    fn the_city_leaves_no_offset() {
        let s = play(&mut *city(13), STILL, 60.0, 128);
        assert!(mean(&s).abs() < 1e-4, "{}", mean(&s));
    }

    #[test]
    fn the_city_is_the_same_whatever_the_length_of_the_blocks() {
        let whole = play(&mut *city(14), STILL, 10.0, 128);
        for block in [1, 77, 4_096] {
            assert!(play(&mut *city(14), STILL, 10.0, block) == whole, "blocks of {block}");
        }
    }

    #[test]
    fn the_city_follows_nothing_but_the_detail() {
        let whole = play(&mut *city(14), STILL, 5.0, 128);
        let hit = Some(crate::Hit { at: 3, strength: 1.0 });
        let driven = play(
            &mut *city(14),
            Drive { speed: 14.0, accel: -1.0, load: 1.0, hit, detail: 1.0, top: 24_000.0 },
            5.0,
            128,
        );
        assert!(driven == whole);
    }

    #[test]
    fn less_detail_keeps_the_city_at_its_level_and_its_colour_and_does_not_click() {
        let far = Drive { detail: 0.0, ..STILL };
        let full = play(&mut *city(15), STILL, 60.0, 128);
        let less = play(&mut *city(15), far, 60.0, 128);
        assert!(decibels(rms(&less) / rms(&full)).abs() < 0.1);
        for centre in [63.0, 125.0, 250.0, 500.0] {
            assert!((octave(&less, centre) - octave(&full, centre)).abs() < 0.5);
        }
        assert!((octave(&less, 1_000.0) - octave(&full, 1_000.0)).abs() < 3.0);
        // A detail that is not a number is no detail.
        assert!(play(&mut *city(15), Drive { detail: f32::NAN, ..STILL }, 1.0, 128) == less[..RATE as usize]);

        // Detail taken away and given back while it sounds.
        let mut voice = city(15);
        let mut s = play(&mut *voice, STILL, 2.0, 128);
        s.extend(play(&mut *voice, far, 2.0, 128));
        s.extend(play(&mut *voice, STILL, 2.0, 128));
        // In the fifth of a second after each change, no move from a sample
        // to the next larger than the city's own largest.
        let (away, back) = (RATE as usize * 2, RATE as usize * 4);
        let around = largest_step(&s[away - 1..away + 9_600]).max(largest_step(&s[back - 1..back + 9_600]));
        assert!(around <= largest_step(&full), "{around} against {}", largest_step(&full));
        // What changed is the air: going, it moves no faster than it did
        // itself; back, it is another stretch of the same hiss, and the two
        // together move no faster than two airs.
        let changed: Vec<f32> = s.iter().zip(&full).map(|(a, b)| a - b).collect();
        let air: Vec<f32> = full.iter().zip(&less).map(|(a, b)| a - b).collect();
        assert!(largest_step(&changed[..back]) <= 1.05 * largest_step(&air), "{}", largest_step(&changed[..back]));
        assert!(largest_step(&changed) <= 2.0 * largest_step(&air), "{}", largest_step(&changed));
        // Once away it is the city without its air, and once back the city
        // with it: the same body, 20 dB over the two airs.
        let (gone, settled) = (away + 4_800..back, back + 4_800..s.len());
        assert!(s[gone.clone()] == less[gone]);
        let apart = decibels(rms(&changed[settled.clone()]) / rms(&full[settled]));
        assert!((-19.0..-15.0).contains(&apart), "{apart} dB between the city and itself");
    }

    #[test]
    fn the_city_stays_in_range_however_far_it_is_asked_to_drift() {
        let params = Params::default().with("drift", 40.0).with("hiss", 20.0);
        let s = play(&mut *make("city-bed", 16, &params).unwrap(), STILL, 120.0, 128);
        assert!(s.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
    }

    #[test]
    fn the_limiter_leaves_alone_what_is_under_its_knee_and_bounds_the_rest() {
        assert_eq!(bound(0.5), 0.5);
        assert_eq!(bound(-KNEE), -KNEE);
        let mut last = KNEE;
        for i in 1..1_000 {
            let x = KNEE + i as f32 * 0.01;
            assert!(bound(x) > last && bound(x) < 1.0 && bound(-x) == -bound(x));
            last = bound(x);
        }
        assert!(bound(1e9) <= 1.0);
    }

    #[test]
    fn the_measured_constants_are_what_the_filters_give() {
        let n = RATE as usize * 60;
        let level = |next: &mut dyn FnMut() -> f32| {
            ((0..n).map(|_| f64::from(next()).powi(2)).sum::<f64>() / n as f64).sqrt() as f32
        };
        let mut body = Body::new(Noise::new(1));
        let through = level(&mut || body.next());
        assert!((through / BODY_RMS - 1.0).abs() < 0.02, "the body: {through}");
        let mut air = Air::new(Noise::new(1));
        let through = level(&mut || air.next());
        assert!((through / AIR_RMS - 1.0).abs() < 0.02, "the air: {through}");

        for part in [&HEEL, &SOLE] {
            let mut band = Band::at(part.thud, part.q);
            let length = BUMP / part.thud * RATE as f32;
            let ring = (0..4_000).fold(0.0f32, |most, i| most.max(band.pass(push(i as f32 / length)).abs()));
            assert!((ring / part.ring - 1.0).abs() < 0.02, "a thud rings to {ring}");
            let mut grit = Grit::new(Noise::new(1));
            grit.tune(part.band, part.grit);
            let through = level(&mut || grit.next());
            assert!((through / part.through - 1.0).abs() < 0.02, "a scuff comes through at {through}");
        }
    }

    /// Not a test of the sound: what each voice costs, to report. Beside
    /// them, the least a voice that plays a recording does (read, between
    /// two samples, times a gain).
    /// `cargo test --offline --release cost -- --ignored --nocapture`
    #[test]
    #[ignore = "a measurement: prints nanoseconds a sample"]
    fn cost() {
        let blocks = RATE as usize * 10 / 128;
        let time = |name: &str, block: &mut dyn FnMut(&mut [f32; 128])| {
            let mut out = [0.0f32; 128];
            let mut best = f64::MAX;
            for _ in 0..20 {
                let start = std::time::Instant::now();
                for _ in 0..blocks {
                    block(&mut out);
                    std::hint::black_box(&out);
                }
                best = best.min(start.elapsed().as_nanos() as f64 / (blocks * 128) as f64);
            }
            println!("{name:<44} {best:6.2} ns a sample");
        };
        for (name, drive) in [
            ("footsteps", WALKING),
            ("footsteps", Drive { detail: 0.0, ..WALKING }),
            ("footsteps", Drive { speed: 14.0, ..STILL }),
            ("footsteps", STILL),
            ("city-bed", STILL),
            ("city-bed", Drive { detail: 0.0, ..STILL }),
        ] {
            let mut voice = make(name, 6, &Params::default()).unwrap();
            let label = format!("{name}, speed {} m/s, detail {}", drive.speed, drive.detail);
            time(&label, &mut |out| voice.render(&drive, out));
        }
        let mut noise = Noise::new(6);
        let recording: Vec<f32> = (0..RATE * 4).map(|_| noise.next()).collect();
        let (mut at, mut gain) = (0.0f32, 0.5f32);
        time("a recording played 3 % fast", &mut |out| {
            for sample in out.iter_mut() {
                let (i, f) = (at as usize, at.fract());
                *sample = (recording[i] + (recording[i + 1] - recording[i]) * f) * gain;
                at += 1.03;
                if at >= (recording.len() - 1) as f32 {
                    at -= (recording.len() - 1) as f32;
                }
                gain += (0.5 - gain) * 0.001;
            }
        });
    }
}
