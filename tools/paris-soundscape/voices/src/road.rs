// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The road: what a car and a scooter radiate as they pass — the kinds
//! "car" and "scooter".
//!
//! # Car
//!
//! A passenger car after the CNOSSOS-EU road source (Directive (EU)
//! 2015/996, category 1): the sound power of its rolling noise and of its
//! propulsion noise in each octave from 63 Hz to 8 kHz, as laws of its
//! speed. An RMS of [`NOMINAL_RMS`] stands for the 97.0 dB the model gives
//! at 30 km/h; at 50 it gives 2 dB more; under 20 km/h it keeps what it
//! gives at 20.
//!
//! - **tyres on asphalt:** seeded noise through four bands, at the levels of
//!   the rolling noise at the speed: one each for the octaves of its hump
//!   (500 Hz, 1 kHz, 2 kHz), one for the rumble under (63 to 250 Hz) and
//!   the hiss over (4 and 8 kHz) taken with the 2 kHz one. The whole sways
//!   by a decibel or so and tilts a little, as slowed noise does: no stretch
//!   of road is the same. Silent at rest;
//! - **engine:** a four-cylinder four-stroke fires twice a turn. Each firing
//!   is a push — a smooth bump of 1.8 ms, shorter and so brighter under
//!   load — never quite as strong or as punctual as the last, each cylinder
//!   a little unequal; and the engine's breath (intake, valves, injectors),
//!   noise over 600 Hz that swells with each firing. Both go through four
//!   bands at the levels of the propulsion noise: one for the firing's own
//!   note, which follows the revs and carries the model's lowest octave,
//!   three for the rest (125 and 250 Hz, 500 Hz and 1 kHz, 2 to 8 kHz).
//!   Pulling (`accel` > 0) it is louder, up to 5.5 dB at 2 m/s², and
//!   rougher; lifting off, quieter, down to 6 dB at −1.5 m/s²; at rest it
//!   idles at 800 turns a minute, 6 dB under;
//! - **gearbox:** the revs follow the speed through six gears; the driver
//!   changes up a little over the `rpm` parameter (later when pulling),
//!   down well under it, the foot off for a third of a second; the revs
//!   wander by half a percent;
//! - **electric motor** (`engine` under 1): no engine, and a faint whine of
//!   two partials that rises with the speed (1.9 kHz and 1 kHz at 30 km/h),
//!   stronger when the motor pulls or brakes;
//! - **from afar** (`detail` under a half): the same level and colour from
//!   four bands instead of eight — the tyres' rumble and hump, the firing
//!   and the rest of the engine — faded to over 24 ms;
//! - **mix:** nothing up to a half of full scale; beyond, peaks round off
//!   (a car over about 70 km/h).
//!
//! Parameters: `engine` (1 a combustion car, 0 an electric one, between the
//! two a mix), `rpm` (engine speed at 30 km/h in the gear held there,
//! 1 800), `size` (1 the model's car; a bigger one is lower and louder),
//! `tone` (−1…1: the tyres' colour, duller or brighter by 3 dB an octave
//! about 1 kHz, at the same level). The seed gives each car its own: a
//! decibel and a half either way in each octave, 4 % on the revs, unequal
//! cylinders, 15 % on the whine's pitch.
//!
//! The drive: `speed`, `accel` and `detail`; `load` and `hit` are not read.
//! Level: an RMS of [`NOMINAL_RMS`] at a steady 30 km/h with the default
//! parameters.
//!
//! # Scooter
//!
//! A 50 cc two-stroke on its variator: the kart's voice (`osage-kart-sound`,
//! "any small single-cylinder engine") told it stands still on asphalt — so
//! without the wind in a driver's ears, the sway and the riding of its own
//! level — at a scooter's revs and throttle:
//!
//! - **revs:** the variator holds the engine where it pulls, 6 500 turns a
//!   minute at 30 km/h off the throttle to 7 500 flat out (the `rpm`
//!   parameter and 1 000 times the throttle), 20 more for each km/h; under
//!   12 km/h the clutch lets it down to the kart's idle; it wanders by a
//!   percent;
//! - **throttle:** `load` when the drive gives one, the `throttle` parameter
//!   (0.6) when it gives none (0); shut at rest;
//! - what its bursts leave standing is taken off (a high-pass at 25 Hz), and
//!   the level set.
//!
//! Parameters: `rpm` (6 500), `throttle` (0.6). The seed gives 3 % on the
//! revs, and the kart's own noise. The drive: `speed` and `load`; `detail`
//! is not read, the kart's voice having no cheaper version. Level: an RMS
//! of [`NOMINAL_RMS`] at 30 km/h at the default throttle; about 2.5 dB less
//! at a throttle of 0.3, 1.5 dB more flat out.
//!
//! # Cost
//!
//! The test `cost` measures it. On one core of a laptop held to 1.3 GHz, a
//! car costs 0.4 times the kart's voice.
//!
//! # Assumptions
//!
//! The model gives levels in octaves, averaged over a fleet. All that makes
//! a sound of them is ours:
//!
//! - the 2015 coefficients, not those amended since; the model's reference
//!   road and temperature;
//! - which octaves share a band, and that the top octave of the tyres comes
//!   out 6 dB under the model (it is 26 dB under their hump); from afar,
//!   octaves are within 5 dB of the model;
//! - that the firing's own note carries the level of the 63 Hz octave
//!   wherever the gear puts it between 40 and 125 Hz;
//! - the engine's make-up: the length of a push, the share of breath (most
//!   of the level from 1 kHz up), cylinders unequal by up to 7 %, firings by
//!   4 % (more at idle and under load), revs wandering by 0.6 %;
//! - the gearbox: its ratios, when the driver changes, how long for, the
//!   clutch slipping when pulling away;
//! - at rest: 800 turns a minute and 6 dB under the level at 20 km/h (the
//!   model is of traffic that flows), tyres fading out under 5 km/h;
//! - what the foot does: +5.5 dB is the model's correction for traffic
//!   leaving a light, recalled and not checked, laid here on 2 m/s²; −6 dB
//!   lifting off is ours;
//! - the tyres' sway (0.8 dB about once a second) and tilt;
//! - what the seed, `size` and `tone` do;
//! - the electric whine: its two pitches (a motor's slots and a gear's
//!   teeth, 64 and 33 Hz for each km/h), its level, 24 dB under the nominal
//!   one; no warning sound under 20 km/h, which the law asks of such cars;
//! - `detail`: one step at a half, not a slope;
//! - the scooter: its revs and how the throttle and the speed move them, an
//!   idle at the kart's 2 800 turns a minute where a scooter's is lower, the
//!   kart's misfires, pipe and bark kept as they are.

use std::f32::consts::TAU;

use osage_kart_sound::{Drive as KartDrive, IDLE_RPM, MAX_RPM, Voice as Kart};

use crate::{
    Drive, Params, Voice,
    dsp::{DT, Low, NOMINAL_RMS, Noise, RATE, db, soft},
};

pub const CAR: &str = "A car as it radiates passing by, after the CNOSSOS-EU road source: tyres on asphalt, and a \
                       four-cylinder engine or an electric motor's whine";
pub const CAR_PARAMS: &[(&str, f32)] = &[("engine", 1.0), ("rpm", 1_800.0), ("size", 1.0), ("tone", 0.0)];

pub const SCOOTER: &str = "A 50 cc two-stroke scooter on its variator: the kart's engine, without the kart's wind";
pub const SCOOTER_PARAMS: &[(&str, f32)] = &[("rpm", 6_500.0), ("throttle", 0.6)];

/// Samples between two updates of what changes slowly, as the kart's voice.
const BLOCK: usize = 32;

/// CNOSSOS-EU, category 1 (passenger cars), Directive (EU) 2015/996: the
/// sound power of the rolling noise in each octave from 63 Hz to 8 kHz is
/// `A + B·lg(v/70)`, of the propulsion noise `A + B·(v − 70)/70`, dB re
/// 1 pW, `v` in km/h.
const ROLLING_A: [f32; 8] = [79.7, 85.7, 84.5, 90.2, 97.3, 93.9, 84.1, 74.3];
const ROLLING_B: [f32; 8] = [30.0, 41.5, 38.9, 25.7, 32.5, 37.2, 39.0, 40.0];
const PROPULSION_A: [f32; 8] = [94.5, 89.2, 88.0, 85.9, 84.2, 86.9, 83.3, 76.1];
const PROPULSION_B: [f32; 8] = [-1.3, 7.2, 7.7, 8.0, 8.0, 8.0, 8.0, 8.0];
/// Under this speed, km/h, the model keeps the level it has at it.
const SLOWEST: f32 = 20.0;
/// What the model gives at 30 km/h, all octaves and both parts together:
/// the sound power that [`NOMINAL_RMS`] stands for.
const NOMINAL_DB: f32 = 97.0;

/// Sound power in each octave at `kmh`, dB: rolling, propulsion.
fn model(kmh: f32) -> [[f32; 8]; 2] {
    let v = kmh.max(SLOWEST);
    let (lg, over) = ((v / 70.0).log10(), (v - 70.0) / 70.0);
    [
        std::array::from_fn(|i| ROLLING_A[i] + ROLLING_B[i] * lg),
        std::array::from_fn(|i| PROPULSION_A[i] + PROPULSION_B[i] * over),
    ]
}

/// A band of a car's sound: its centre, Hz, its width, octaves, and the
/// octaves of the model whose level it carries, from the first up to but
/// not the last.
type Lane = (f32, f32, (usize, usize));

/// A car heard close, tyres then engine. The tyres keep a band for each
/// octave of their hump; the engine one for its firing, which follows the
/// revs. What is either side is taken together.
const NEAR: [[Lane; 4]; 2] = [
    [(90.0, 2.5, (0, 3)), (500.0, 1.0, (3, 4)), (1_000.0, 1.0, (4, 5)), (2_200.0, 1.25, (5, 8))],
    [(FIRING, 1.0, (0, 1)), (177.0, 2.0, (1, 3)), (707.0, 2.0, (3, 5)), (2_828.0, 2.0, (5, 8))],
];
/// A car heard from afar or under louder sounds, in four bands: the tyres'
/// rumble and their hump, the engine's firing and all the rest of it.
const FAR: [[Lane; 2]; 2] =
    [[(90.0, 2.5, (0, 3)), (1_000.0, 1.6, (3, 8))], [(FIRING, 1.0, (0, 1)), (400.0, 4.0, (1, 8))]];

/// How much louder than its level each band is made for that level to come
/// out of its octaves: what the noise and the bands' widths and overlaps
/// take. Measured (the test `calibrate`).
#[derive(Clone, Copy, Debug)]
struct Trims {
    near: [[f32; 4]; 2],
    far: [[f32; 2]; 2],
}

const TRIMS: Trims =
    Trims { near: [[19.93, 12.62, 11.32, 4.01], [12.44, 4.54, 3.76, 1.74]], far: [[21.70, 8.79], [12.45, 2.98]] };

/// Gearbox: engine speed in each gear, times that of the gear held at
/// 30 km/h.
const GEARS: [f32; 6] = [2.2, 1.35, 1.0, 0.74, 0.58, 0.47];
/// The driver changes up over this many times the `rpm` parameter (more
/// when pulling), down under that many.
const UP: f32 = 1.11;
const DOWN: f32 = 0.64;
/// A gear change, blocks: the foot off for a third of a second.
const SHIFT: u32 = (0.3 * RATE as f32) as u32 / BLOCK as u32;
/// Turns a minute at rest.
const IDLE: f32 = 800.0;
/// An idling engine, against one pulling the car at a steady 20 km/h.
const IDLE_LEVEL: f32 = 0.5;
/// The firing rate the trims were measured at, Hz (1 800 turns a minute),
/// and how long a push lasts, s.
const FIRING: f32 = 60.0;
const PUSH: f32 = 0.001_8;
/// The engine's breath against its pushes.
const BREATH: f32 = 2.4;
/// An electric motor's whine: Hz for each km/h, and its level.
const WHINE: f32 = 64.0;
const WHINE_LEVEL: f32 = 0.006;
/// Blocks to go from the near sound to the far one (24 ms).
const FADE: f32 = 36.0;

/// Keeps a sample within −1…1: itself up to a half, rounded off beyond.
fn bound(x: f32) -> f32 {
    if x.abs() <= 0.5 { x } else { (0.5 + 0.5 * soft((x.abs() - 0.5) * 2.0)).copysign(x) }
}

/// Four bands side by side, each two band-passes one after the other, the
/// four computed together: written lane by lane, so that the compiler makes
/// one vector operation of four.
#[derive(Clone, Copy, Debug, Default)]
struct Quad {
    b: [f32; 4],
    a1: [f32; 4],
    a2: [f32; 4],
    /// What each section heard and gave, one and two samples ago.
    z: [[f32; 4]; 6],
    /// How loud each band comes out, and by how much that moves a sample.
    gain: [f32; 4],
    ramp: [f32; 4],
}

impl Quad {
    fn tuned(bands: [(f32, f32); 4]) -> Self {
        let mut quad = Self::default();
        for (lane, (centre, octaves)) in bands.into_iter().enumerate() {
            quad.tune(lane, centre, octaves);
        }
        quad
    }

    /// Puts the band `lane` at `centre`, `octaves` wide: each of its two
    /// sections 3 dB down half its width away, the band 6 dB down there and
    /// a half turn from a neighbour that begins where it ends. With every
    /// other band turned over, neighbours then add up to what one band
    /// passes at its centre, and the whole is smooth.
    fn tune(&mut self, lane: usize, centre: f32, octaves: f32) {
        let q = 1.0 / (2f32.powf(octaves / 2.0) - 2f32.powf(-octaves / 2.0));
        let w = TAU * centre.clamp(20.0, RATE as f32 * 0.45) * DT;
        let alpha = w.sin() / (2.0 * q);
        self.b[lane] = alpha / (1.0 + alpha);
        self.a1[lane] = -2.0 * w.cos() / (1.0 + alpha);
        self.a2[lane] = (1.0 - alpha) / (1.0 + alpha);
    }

    /// Where the gains are to be a block from now.
    fn aim(&mut self, gains: [f32; 4]) {
        self.ramp = std::array::from_fn(|lane| (gains[lane] - self.gain[lane]) / BLOCK as f32);
    }

    /// One sample: the first two bands hear `first`, the other two `second`.
    #[inline(always)]
    #[allow(clippy::needless_range_loop)] // Lane by lane, every array alike: what the compiler makes vectors of.
    fn pass(&mut self, first: f32, second: f32) -> f32 {
        let x = [first, first, second, second];
        let [x1, x2, y1, y2, w1, w2] = &mut self.z;
        let mut lanes = [0.0f32; 4];
        for i in 0..4 {
            // Each section in the form whose own past comes back through
            // one multiplication only: waiting for it is what a sample
            // costs.
            let y = (self.b[i] * (x[i] - x2[i]) - self.a2[i] * y2[i]) - self.a1[i] * y1[i];
            let w = (self.b[i] * (y - y2[i]) - self.a2[i] * w2[i]) - self.a1[i] * w1[i];
            (x2[i], x1[i]) = (x1[i], x[i]);
            (y2[i], y1[i]) = (y1[i], y);
            (w2[i], w1[i]) = (w1[i], w);
            self.gain[i] += self.ramp[i];
            lanes[i] = w * self.gain[i];
        }
        (lanes[0] + lanes[1]) + (lanes[2] + lanes[3])
    }

    /// Adds to `out` what the bands make of `first` and `second`.
    fn run(&mut self, first: &[f32], second: &[f32], out: &mut [f32]) {
        // In hand for the length of the block: the compiler then keeps the
        // lanes in registers.
        let mut quad = *self;
        for ((first, second), out) in first.iter().zip(second).zip(out) {
            *out += quad.pass(*first, *second);
        }
        *self = quad;
    }

    /// Adds to `out` what the bands make of `mine` and those of `other` of
    /// `theirs`. Both in one loop: neither waits for the other.
    fn run_beside(&mut self, other: &mut Quad, mine: &[f32], theirs: &[f32], out: &mut [f32]) {
        let (mut a, mut b) = (*self, *other);
        for ((mine, theirs), out) in mine.iter().zip(theirs).zip(out) {
            *out += a.pass(*mine, *mine) + b.pass(*theirs, *theirs);
        }
        (*self, *other) = (a, b);
    }
}

/// Something that never holds still: noise slowed to a wander of about
/// `frequency`, an RMS of 1, a new value every block.
#[derive(Clone, Debug)]
struct Drift {
    y: f32,
    a: f32,
    scale: f32,
}

impl Drift {
    fn at(frequency: f32) -> Self {
        let a = 1.0 - (-TAU * frequency * BLOCK as f32 * DT).exp();
        // What a one-pole leaves of a uniform noise (a variance of a third).
        Self { y: 0.0, a, scale: (3.0 * (2.0 - a) / a).sqrt() }
    }

    fn next(&mut self, noise: &mut Noise) -> f32 {
        self.y += (noise.next() - self.y) * self.a;
        self.y * self.scale
    }
}

/// A pure tone as a point turning round a circle, moved on by four
/// multiplications a sample; its pitch is set every block.
#[derive(Clone, Copy, Debug)]
struct Tone {
    re: f32,
    im: f32,
    cos: f32,
    sin: f32,
    gain: f32,
    ramp: f32,
}

impl Tone {
    fn new(turn: f32) -> Self {
        let (im, re) = (TAU * turn).sin_cos();
        Self { re, im, cos: 1.0, sin: 0.0, gain: 0.0, ramp: 0.0 }
    }

    fn set(&mut self, frequency: f32, gain: f32) {
        (self.sin, self.cos) = (TAU * frequency.min(RATE as f32 * 0.45) * DT).sin_cos();
        // Rounding would let the point leave its circle: a step back to it.
        let keep = 1.5 - 0.5 * (self.re * self.re + self.im * self.im);
        (self.re, self.im) = (self.re * keep, self.im * keep);
        self.ramp = (gain - self.gain) / BLOCK as f32;
    }

    #[inline(always)]
    fn next(&mut self) -> f32 {
        (self.re, self.im) = (self.re * self.cos - self.im * self.sin, self.im * self.cos + self.re * self.sin);
        self.gain += self.ramp;
        self.im * self.gain
    }
}

/// What the cylinders give the engine's bands: a push at each firing, and
/// the breath of the engine swelling with it.
#[derive(Clone, Debug)]
struct Firing {
    /// Through the time from one firing to the next, 0…1.
    phase: f32,
    cylinder: usize,
    /// How strong each cylinder is: never quite the same.
    unequal: [f32; 4],
    /// This firing's strength and pace.
    fire: f32,
    pace: f32,
    noise: Noise,
    /// The breath's low end, taken off it: it would blur the firing's own
    /// partials. How much of it stays from a sample to the next.
    low: f32,
    keep: f32,
    // Set every block.
    step: f32,
    sharp: f32,
    push: f32,
    rough: f32,
    depth: f32,
    breath: f32,
}

impl Firing {
    #[inline(always)]
    fn next(&mut self) -> f32 {
        self.phase += self.step * self.pace;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            self.cylinder = (self.cylinder + 1) & 3;
            self.fire = self.unequal[self.cylinder] * (1.0 + self.rough * self.noise.next());
            self.pace = 1.0 + 0.2 * self.rough * self.noise.next();
        }
        // The push: a smooth bump, over before the next firing. Smooth at
        // both ends, it has next to nothing left where partials would fold.
        let u = self.phase * self.sharp;
        let push = if u < 1.0 {
            let bump = u * (1.0 - u);
            16.0 * bump * bump * self.push
        } else {
            0.0
        };
        let swell = 1.0 + self.depth * (1.0 - 2.0 * self.phase);
        let air = self.noise.next();
        self.low = self.low * self.keep + air * (1.0 - self.keep);
        self.fire * (push + (air - self.low) * swell * self.breath)
    }
}

/// A car passing.
pub struct Car {
    // What the scene chose, and what the seed made of it.
    engine: f32,
    /// 1; a test silences the tyres to hear the engine alone.
    tyres: f32,
    rpm: f32,
    whine: f32,
    /// Its own colour: each octave a little up or down, rolling then
    /// propulsion, and all of it louder for a bigger car.
    colour: [[f32; 8]; 2],
    trims: Trims,

    road: Noise,
    slow: Noise,
    firing: Firing,
    /// Near: the tyres' four bands, the engine's four. Far: two and two.
    bank: [Quad; 3],
    tones: [Tone; 2],

    /// The drive it follows, and where it has eased to.
    wanted: Drive,
    started: bool,
    speed: f32,
    accel: f32,
    revs: f32,
    gear: usize,
    shift: u32,
    /// 0 the near sound, 1 the far one; where it goes, and where it was a
    /// block ago.
    far: f32,
    far_wanted: f32,
    far_before: f32,
    sway: Drift,
    tilt: Drift,
    wander: Drift,
    /// The speed the model was last worked out at, and the level it gave
    /// each band, near and far.
    modelled: f32,
    near: [[f32; 4]; 2],
    wide: [[f32; 2]; 2],
    /// Where the band of the firing was last put, Hz.
    note: f32,
    /// Samples left before the next update.
    left: usize,
}

impl Car {
    pub fn new(seed: u32, params: &Params) -> Self {
        Self::build(seed, params, TRIMS, 1.0)
    }

    /// A car; `own` is how much of its own the seed gives it (none: the
    /// model's mean car).
    fn build(seed: u32, params: &Params, trims: Trims, own: f32) -> Self {
        let get = |name: &str| params.get(name, CAR_PARAMS.iter().find(|(n, _)| *n == name).map_or(0.0, |(_, d)| *d));
        let mut noise = Noise::new(seed);
        let size = get("size").clamp(0.5, 2.0);
        let tone = get("tone").clamp(-1.0, 1.0);

        // Its own colour, at the level of the model at 30 km/h: a decibel
        // and a half either way in each octave, and the tilt asked for.
        let at_30 = model(30.0);
        let mut colour = [[1.0f32; 8]; 2];
        for (part, colour) in colour.iter_mut().enumerate() {
            let (mut plain, mut coloured) = (0.0, 0.0);
            for (i, c) in colour.iter_mut().enumerate() {
                let tilt = if part == 0 { 3.0 * tone * (i as f32 - 4.0) } else { 0.0 };
                *c = db(1.5 * own * noise.next() + tilt);
                let power = 10f32.powf(at_30[part][i] / 10.0);
                plain += power;
                coloured += power * *c * *c;
            }
            // A bigger car is a louder one.
            colour.iter_mut().for_each(|c| *c *= (plain / coloured).sqrt() * size.sqrt());
        }

        // And a lower one: every band down, and as much narrower, the
        // tyres' taking that much less of the noise.
        let down = 1.0 / size.sqrt();
        colour[0].iter_mut().for_each(|c| *c /= down.sqrt());
        let near = |part: usize| NEAR[part].map(|(centre, octaves, _)| (centre * down, octaves));
        let far = |part: usize, lane: usize| (FAR[part][lane].0 * down, FAR[part][lane].1);

        Self {
            engine: get("engine").clamp(0.0, 1.0),
            tyres: 1.0,
            // No two cars turn at quite the same speed: two engines a few
            // thousandths apart would otherwise sound as one.
            rpm: get("rpm").clamp(900.0, 4_000.0) * (1.0 + 0.07 * own * noise.next()),
            whine: 1.0 + 0.15 * own * noise.next(),
            colour,
            trims,
            road: noise.fork(),
            slow: noise.fork(),
            firing: Firing {
                phase: noise.unit(),
                cylinder: 0,
                unequal: std::array::from_fn(|_| 1.0 + 0.07 * noise.next()),
                fire: 1.0,
                pace: 1.0,
                noise: noise.fork(),
                low: 0.0,
                keep: (-TAU * 600.0 * DT).exp(),
                step: 0.0,
                sharp: 1.0,
                push: 0.0,
                rough: 0.0,
                depth: 0.0,
                breath: 0.0,
            },
            bank: [
                Quad::tuned(near(0)),
                Quad::tuned(near(1)),
                Quad::tuned([far(0, 0), far(0, 1), far(1, 0), far(1, 1)]),
            ],
            tones: [Tone::new(noise.unit()), Tone::new(noise.unit())],
            wanted: Drive::default(),
            started: false,
            speed: 0.0,
            accel: 0.0,
            revs: IDLE,
            gear: 0,
            shift: 0,
            far: 0.0,
            far_wanted: 0.0,
            far_before: 0.0,
            sway: Drift::at(1.5),
            tilt: Drift::at(0.8),
            wander: Drift::at(0.6),
            modelled: 0.0,
            near: [[0.0; 4]; 2],
            wide: [[0.0; 2]; 2],
            note: 0.0,
            left: 0,
        }
    }

    /// Engine speed in `gear` at `kmh`, turns a minute.
    fn geared(&self, gear: usize, kmh: f32) -> f32 {
        kmh * self.rpm / 30.0 * GEARS[gear]
    }

    /// The level the model gives each band at `kmh`, as this car colours it.
    fn remodel(&mut self, kmh: f32) {
        self.modelled = kmh;
        let levels = model(kmh);
        for part in 0..2 {
            let octaves: [f32; 8] =
                std::array::from_fn(|i| NOMINAL_RMS * db(levels[part][i] - NOMINAL_DB) * self.colour[part][i]);
            let together = |(from, to): (usize, usize)| octaves[from..to].iter().map(|l| l * l).sum::<f32>().sqrt();
            // Every other band turned over ([`Quad::tune`]).
            let turned = |lane: usize| if lane.is_multiple_of(2) { 1.0 } else { -1.0 };
            for (lane, band) in NEAR[part].iter().enumerate() {
                self.near[part][lane] = together(band.2) * self.trims.near[part][lane] * turned(lane);
            }
            for (lane, band) in FAR[part].iter().enumerate() {
                self.wide[part][lane] = together(band.2) * self.trims.far[part][lane] * turned(lane);
            }
        }
    }

    /// Eases towards the drive and sets what changes slowly, once a block.
    fn control(&mut self) {
        let d = self.wanted;
        // Whatever is asked, a speed and an acceleration a car can have.
        let speed = d.speed.abs().min(100.0);
        let accel = if d.accel.is_finite() { d.accel.clamp(-12.0, 12.0) } else { 0.0 };
        let block = BLOCK as f32 * DT;
        if !self.started {
            // It was already under way: in the gear a steady driver holds.
            self.started = true;
            (self.speed, self.accel) = (speed, accel);
            while self.gear < 5 && self.geared(self.gear, speed * 3.6) > UP * self.rpm {
                self.gear += 1;
            }
            self.revs = self.geared(self.gear, speed * 3.6).max(IDLE);
            self.far_wanted = if d.detail < 0.5 { 1.0 } else { 0.0 };
            (self.far, self.far_before) = (self.far_wanted, self.far_wanted);
        }
        // The speed over 10 ms; the engine answers the foot more slowly.
        // Arrived is arrived: a speed forever nearing nought would cost.
        self.speed += (speed - self.speed) * (block / 0.01).min(1.0);
        self.accel += (accel - self.accel) * (block / 0.08).min(1.0);
        if (speed - self.speed).abs() < 1e-5 {
            self.speed = speed;
        }
        if (accel - self.accel).abs() < 1e-5 {
            self.accel = accel;
        }
        let kmh = self.speed * 3.6;
        let moving = (kmh / 5.0).min(1.0);

        if (kmh.max(SLOWEST) - self.modelled).abs() > 0.001 * self.modelled {
            self.remodel(kmh.max(SLOWEST));
        }

        // The gearbox.
        let mut clutch = 0.0;
        if self.shift > 0 {
            self.shift -= 1;
            let through = self.shift as f32 / SHIFT as f32;
            clutch = 4.0 * through * (1.0 - through);
        } else {
            let turning = self.geared(self.gear, kmh);
            if turning > (UP + 0.33 * self.accel.clamp(0.0, 2.5)) * self.rpm && self.gear < 5 {
                (self.gear, self.shift) = (self.gear + 1, SHIFT);
            } else if turning < DOWN * self.rpm && self.gear > 0 {
                (self.gear, self.shift) = (self.gear - 1, SHIFT);
            }
        }
        // The foot: down to pull, up to slow, and off through a gear change.
        let foot = if self.accel > 0.0 { (self.accel / 2.0).min(1.0) } else { (self.accel / 1.5).max(-1.0) };
        let foot = foot * (1.0 - clutch) - 0.8 * clutch * moving;
        let (pull, lift) = (foot.max(0.0), (-foot).max(0.0));
        // Pulling away, the clutch slips and the engine is held above idle.
        let turning = self.geared(self.gear, kmh).max(IDLE * (1.0 + 0.8 * pull));
        self.revs += (turning - self.revs) * (block / 0.12).min(1.0);

        // The firing's own band goes where the gear puts the firing: its
        // note has the level of the model's lowest octave wherever it is.
        let note = (self.revs / 30.0).clamp(40.0, 125.0);
        if (note - self.note).abs() > 0.002 * self.note {
            self.note = note;
            self.bank[1].tune(0, note, 1.0);
            self.bank[2].tune(2, note, 1.0);
        }

        // The engine: twice a turn, never quite steady.
        let firing = self.revs / 30.0 * (1.0 + 0.006 * self.wander.next(&mut self.slow));
        let width = ((PUSH * (1.0 - 0.4 * pull)) * firing).min(0.9);
        let f = &mut self.firing;
        f.step = firing * DT;
        f.sharp = 1.0 / width;
        // A shorter push is a higher one: the firing's own note keeps its
        // level, and more of its partials come with it.
        f.push = PUSH * FIRING / width;
        f.rough = 0.04 + 0.05 * pull + 0.06 * (1.0 - moving);
        f.depth = 0.5 + 0.3 * pull;
        f.breath = BREATH * (1.0 + 0.3 * pull);
        let engine = self.engine
            * (IDLE_LEVEL + (1.0 - IDLE_LEVEL) * moving)
            * (1.0 + (db(5.5) - 1.0) * pull)
            * (1.0 - 0.5 * lift * moving);

        // The tyres: silent at rest, and never the same stretch of road.
        let tyres = self.tyres * moving * (1.0 + 0.09 * self.sway.next(&mut self.slow)).max(0.0);
        let tilt = 0.06 * self.tilt.next(&mut self.slow);

        // Near or far, with some room so as not to go to and fro.
        if d.detail < 0.45 {
            self.far_wanted = 1.0;
        } else if d.detail > 0.55 {
            self.far_wanted = 0.0;
        }
        self.far_before = self.far;
        self.far += (self.far_wanted - self.far).clamp(-1.0 / FADE, 1.0 / FADE);
        let (near, far) = (1.0 - self.far, self.far);
        let [tyres_near, engine_near, both_far] = &mut self.bank;
        tyres_near.aim(std::array::from_fn(|i| self.near[0][i] * tyres * near * (1.0 + tilt * (i as f32 - 1.5))));
        engine_near.aim(std::array::from_fn(|i| self.near[1][i] * engine * near));
        both_far.aim([
            self.wide[0][0] * tyres * far * (1.0 - tilt),
            self.wide[0][1] * tyres * far * (1.0 + tilt),
            self.wide[1][0] * engine * far,
            self.wide[1][1] * engine * far,
        ]);

        // An electric motor: a whine that rises with the speed, stronger
        // when it pulls or brakes; gone before it could fold.
        if self.engine < 1.0 {
            let pitch = WHINE * kmh * self.whine;
            let level = (1.0 - self.engine)
                * WHINE_LEVEL
                * (pitch / 640.0).min(1.0)
                * (1.0 + 1.5 * foot.abs())
                * ((16_000.0 - pitch) / 4_000.0).clamp(0.0, 1.0);
            self.tones[0].set(pitch, level);
            self.tones[1].set(pitch * 0.52, level * 0.6);
        }
    }
}

impl Voice for Car {
    fn render(&mut self, drive: &Drive, out: &mut [f32]) {
        self.wanted = *drive;
        let mut out = out;
        while !out.is_empty() {
            if self.left == 0 {
                self.control();
                self.left = BLOCK;
            }
            let (now, rest) = out.split_at_mut(self.left.min(out.len()));
            self.left -= now.len();
            out = rest;

            let (mut road, mut fired) = ([0.0f32; BLOCK], [0.0f32; BLOCK]);
            let (road, fired) = (&mut road[..now.len()], &mut fired[..now.len()]);
            if self.engine > 0.0 {
                // Both in one loop: neither waits for the other.
                for (road, fired) in road.iter_mut().zip(fired.iter_mut()) {
                    (*road, *fired) = (self.road.next(), self.firing.next());
                }
            } else {
                road.iter_mut().for_each(|x| *x = self.road.next());
            }
            now.fill(0.0);
            // A sound faded out is not made.
            let [tyres_near, engine_near, both_far] = &mut self.bank;
            if self.far < 1.0 || self.far_before < 1.0 {
                if self.engine > 0.0 {
                    tyres_near.run_beside(engine_near, road, fired, now);
                } else {
                    tyres_near.run(road, road, now);
                }
            }
            if self.far > 0.0 || self.far_before > 0.0 {
                both_far.run(road, fired, now);
            }
            if self.engine < 1.0 {
                let [high, low] = &mut self.tones;
                now.iter_mut().for_each(|sample| *sample += high.next() + low.next());
            }
            now.iter_mut().for_each(|sample| *sample = bound(*sample));
        }
    }
}

/// A scooter passing.
pub struct Scooter {
    kart: Kart,
    wanted: Drive,
    throttle: f32,
    rpm: f32,
    noise: Noise,
    wander: Drift,
    /// Takes off what the kart's engine leaves standing: its bursts all push
    /// the same way.
    standing: Low,
    left: usize,
}

/// The nominal level against the kart's voice at a scooter's revs and
/// throttle. Measured.
const SCOOTER_LEVEL: f32 = 0.449;

impl Scooter {
    pub fn new(seed: u32, params: &Params) -> Self {
        let get =
            |name: &str| params.get(name, SCOOTER_PARAMS.iter().find(|(n, _)| *n == name).map_or(0.0, |(_, d)| *d));
        let mut noise = Noise::new(seed ^ 0x5c00_7e12);
        Self {
            kart: Kart::new(seed),
            wanted: Drive::default(),
            throttle: get("throttle").clamp(0.0, 1.0),
            rpm: get("rpm").clamp(4_000.0, 8_000.0) * (1.0 + 0.03 * noise.next()),
            noise,
            wander: Drift::at(0.7),
            standing: Low::at(25.0),
            left: 0,
        }
    }

    /// The kart's drive for this crate's.
    fn kart_drive(&mut self) -> KartDrive {
        let d = self.wanted;
        let kmh = d.speed.abs().min(100.0) * 3.6;
        // The clutch bites as it pulls away; at rest it idles.
        let under_way = (kmh / 12.0).min(1.0);
        let throttle = if d.load > 0.0 { d.load.min(1.0) } else { self.throttle } * under_way;
        // The variator holds the engine where it pulls: higher with the
        // throttle, a little higher with the speed.
        let held = (self.rpm + 1_000.0 * throttle + 20.0 * (kmh - 30.0)).clamp(4_000.0, 9_000.0);
        let rpm = IDLE_RPM + (held - IDLE_RPM) * under_way.sqrt();
        let rpm = rpm * (1.0 + 0.008 * self.wander.next(&mut self.noise));
        KartDrive {
            // Under where the kart's mix would turn its engine down.
            revs: ((rpm - IDLE_RPM) / (MAX_RPM - IDLE_RPM)).clamp(0.0, 0.54),
            throttle,
            slip: 0.0,
            ground: 0.0,
            // Standing still, for the kart: no wind in a driver's ears, no
            // sway of its own, no riding of its level.
            speed: 0.0,
        }
    }
}

impl Voice for Scooter {
    fn render(&mut self, drive: &Drive, out: &mut [f32]) {
        self.wanted = *drive;
        let mut out = out;
        while !out.is_empty() {
            if self.left == 0 {
                let drive = self.kart_drive();
                self.kart.set(drive);
                self.left = BLOCK;
            }
            let (now, rest) = out.split_at_mut(self.left.min(out.len()));
            self.left -= now.len();
            out = rest;
            self.kart.render(now);
            for sample in now {
                *sample = self.standing.above(*sample) * SCOOTER_LEVEL;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::Biquad;

    /// The octaves of the model, Hz.
    const OCTAVES: [f32; 8] = [63.0, 125.0, 250.0, 500.0, 1_000.0, 2_000.0, 4_000.0, 8_000.0];

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    fn decibels(x: f32) -> f32 {
        20.0 * x.max(1e-9).log10()
    }

    /// The level against the nominal one, dB.
    fn level(samples: &[f32]) -> f32 {
        decibels(rms(samples) / NOMINAL_RMS)
    }

    fn run(voice: &mut dyn Voice, drive: Drive, seconds: f32) -> Vec<f32> {
        let mut out = vec![0.0; (seconds * RATE as f32) as usize];
        for block in out.chunks_mut(128) {
            voice.render(&drive, block);
        }
        out
    }

    fn at(kmh: f32) -> Drive {
        Drive { speed: kmh / 3.6, ..Drive::default() }
    }

    fn car(seed: u32) -> Car {
        Car::new(seed, &Params::default())
    }

    fn electric(seed: u32) -> Car {
        Car::new(seed, &Params::default().with("engine", 0.0))
    }

    fn scooter(seed: u32) -> Scooter {
        Scooter::new(seed, &Params::default())
    }

    /// The model's mean car: nothing of its own from the seed.
    fn mean_car(params: &Params, trims: Trims) -> Car {
        Car::build(6, params, trims, 0.0)
    }

    /// The power in the octave at `centre`: a band with steep sides (eighth
    /// order each), the first twentieth left out for it to settle.
    fn octave(samples: &[f32], centre: f32) -> f32 {
        let mut filters: [Biquad; 8] = std::array::from_fn(|_| Biquad::default());
        for (i, q) in [0.509_8, 0.601_3, 0.9, 2.562_9].into_iter().enumerate() {
            filters[i].high_pass(centre / std::f32::consts::SQRT_2, q);
            filters[4 + i].low_pass(centre * std::f32::consts::SQRT_2, q);
        }
        let through: Vec<f32> = samples.iter().map(|s| filters.iter_mut().fold(*s, |x, f| f.pass(x))).collect();
        rms(&through[through.len() / 20..]).powi(2)
    }

    /// The power the model gives each octave at `kmh`, at the nominal level:
    /// `tyres` times rolling and `engine` times propulsion.
    fn wanted(kmh: f32, tyres: f32, engine: f32) -> [f32; 8] {
        let levels = model(kmh);
        let power = |level: f32| (NOMINAL_RMS * db(level - NOMINAL_DB)).powi(2);
        std::array::from_fn(|i| tyres * power(levels[0][i]) + engine * power(levels[1][i]))
    }

    /// How alike two sounds are, −1…1.
    fn alike(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
        dot / (a.iter().map(|x| x * x).sum::<f32>() * b.iter().map(|y| y * y).sum::<f32>()).sqrt().max(1e-12)
    }

    /// The strongest period between `low` and `high` samples, by
    /// autocorrelation (as `osage-kart-sound`'s tests).
    fn period(s: &[f32], low: usize, high: usize) -> usize {
        let c = |lag: usize| s.iter().zip(&s[lag..]).map(|(x, y)| x * y).sum::<f32>();
        (low..high).max_by(|&a, &b| c(a).total_cmp(&c(b))).unwrap()
    }

    /// The largest step from a sample to the next.
    fn step(s: &[f32]) -> f32 {
        s.windows(2).fold(0.0, |m, w| m.max((w[1] - w[0]).abs()))
    }

    /// A second driven one way, then a second another: the largest step
    /// about the change (10 ms before to 100 ms after), and elsewhere.
    fn changed(voice: &mut dyn Voice, before: Drive, after: Drive) -> (f32, f32) {
        let mut s = run(voice, before, 1.0);
        s.extend(run(voice, after, 1.0));
        let (from, to) = (RATE as usize - 480, RATE as usize + 4_800);
        (step(&s[from..to]), step(&s[..from]).max(step(&s[to..])))
    }

    #[test]
    fn the_model_gives_97_decibels_at_30_and_two_more_at_50() {
        let total = |kmh: f32| 10.0 * model(kmh).iter().flatten().map(|l| 10f32.powf(l / 10.0)).sum::<f32>().log10();
        assert!((total(30.0) - NOMINAL_DB).abs() < 0.05, "{}", total(30.0));
        assert!((total(50.0) - 99.0).abs() < 0.05, "{}", total(50.0));
        assert!((total(70.0) - 101.9).abs() < 0.05, "{}", total(70.0));
        assert_eq!(model(5.0), model(20.0), "under 20 km/h, what it gives at 20");
    }

    #[test]
    fn a_car_at_a_steady_30_sounds_at_the_nominal_level_whatever_its_seed() {
        for seed in 1..=8 {
            let found = level(&run(&mut car(seed), at(30.0), 10.0));
            assert!(found.abs() < 1.0, "seed {seed}: {found} dB");
        }
    }

    #[test]
    fn the_level_follows_the_model_with_the_speed_and_holds_under_20() {
        let found = |kmh: f32| level(&run(&mut car(3), at(kmh), 10.0));
        let (at_10, at_20, at_30, at_50, at_70) = (found(10.0), found(20.0), found(30.0), found(50.0), found(70.0));
        assert!((at_50 - at_30 - 2.0).abs() < 0.7, "50 km/h: {} dB over 30", at_50 - at_30);
        assert!((at_70 - at_30 - 4.9).abs() < 1.0, "70 km/h: {} dB over 30", at_70 - at_30);
        assert!((at_20 - at_30 + 0.5).abs() < 1.0, "20 km/h: {} dB over 30", at_20 - at_30);
        assert!((at_10 - at_20).abs() < 1.0, "10 km/h: {} dB over 20", at_10 - at_20);
    }

    #[test]
    fn each_octave_is_at_the_level_of_the_model() {
        for kmh in [30.0, 50.0] {
            let s = run(&mut mean_car(&Params::default(), TRIMS), at(kmh), 20.0);
            for (centre, want) in OCTAVES.into_iter().zip(wanted(kmh, 1.0, 1.0)) {
                let miss = 10.0 * (octave(&s, centre) / want).log10();
                assert!(miss.abs() < 2.0, "{kmh} km/h, {centre} Hz: {miss} dB");
            }
            // The tyres alone, but for their top octave, far under the rest.
            let mut tyres = mean_car(&Params::default().with("engine", 0.0), TRIMS);
            tyres.whine = 0.0;
            let s = run(&mut tyres, at(kmh), 20.0);
            for (centre, want) in OCTAVES.into_iter().zip(wanted(kmh, 1.0, 0.0)).take(7) {
                let miss = 10.0 * (octave(&s, centre) / want).log10();
                assert!(miss.abs() < 2.0, "tyres, {kmh} km/h, {centre} Hz: {miss} dB");
            }
        }
    }

    #[test]
    fn the_tyres_are_loudest_about_1_khz() {
        let s = run(&mut electric(2), at(30.0), 10.0);
        let levels = OCTAVES.map(|centre| octave(&s, centre));
        assert!(levels.iter().all(|l| *l <= levels[4]), "{levels:?}");
    }

    #[test]
    fn the_engine_fires_twice_a_turn() {
        for rpm in [1_800.0, 2_400.0] {
            let mut car = Car::new(4, &Params::default().with("rpm", rpm));
            let s = run(&mut car, at(30.0), 1.0);
            // The seed moves the revs by a few percent: ask the car.
            let expected = RATE as f32 / (car.revs / 30.0);
            assert!((car.revs / rpm - 1.0).abs() < 0.05, "{} turns a minute for {rpm}", car.revs);
            let found = period(&s[RATE as usize / 4..], (expected * 0.7) as usize, (expected * 1.3) as usize);
            assert!((found as f32 - expected).abs() / expected < 0.03, "{rpm}: {found} samples for {expected}");
        }
    }

    #[test]
    fn the_driver_changes_up_as_the_car_gains_speed() {
        let mut car = car(5);
        let (mut gears, mut highest) = (vec![0], 0.0f32);
        let mut block = [0.0; 128];
        // From rest to 50 km/h in ten seconds.
        for i in 0..3_750 {
            let speed = 50.0 / 3.6 * i as f32 / 3_750.0;
            car.render(&Drive { speed, accel: 1.4, ..Drive::default() }, &mut block);
            highest = highest.max(car.revs);
            if *gears.last().unwrap() != car.gear {
                gears.push(car.gear);
            }
        }
        assert_eq!(gears, [0, 1, 2, 3], "one gear at a time");
        assert!(highest < 3_200.0, "{highest} turns a minute");
        // And at a steady 30 km/h it holds the gear of the `rpm` parameter.
        let mut steady = Car::new(5, &Params::default());
        steady.render(&at(30.0), &mut block);
        assert_eq!(steady.gear, 2);
    }

    #[test]
    fn pulling_is_louder_and_rougher_and_lifting_off_quieter() {
        let found = |accel: f32| run(&mut car(2), Drive { accel, ..at(30.0) }, 5.0);
        let (lifting, steady, pulling) = (found(-1.5), found(0.0), found(2.0));
        assert!(level(&pulling) > level(&steady) + 4.0, "{} over {}", level(&pulling), level(&steady));
        assert!(level(&lifting) < level(&steady) - 3.0, "{} under {}", level(&lifting), level(&steady));
        // Rougher: the top gains more than the firing's own note.
        let gain = |centre: f32| 10.0 * (octave(&pulling, centre) / octave(&steady, centre)).log10();
        assert!(gain(4_000.0) > gain(63.0) + 1.0, "{} at 4 kHz, {} at 63 Hz", gain(4_000.0), gain(63.0));
    }

    #[test]
    fn at_rest_an_engine_idles_and_an_electric_car_is_silent() {
        let idle = run(&mut car(1), at(0.0), 4.0);
        assert!(level(&idle) < -5.0 && level(&idle) > -14.0, "{} dB", level(&idle));
        // 800 turns a minute: a firing every 37.5 ms.
        let found = period(&idle[RATE as usize..], 1_300, 2_300);
        assert!((found as f32 - 1_800.0).abs() < 110.0, "{found} samples between firings");
        let still = run(&mut electric(1), at(0.0), 1.0);
        assert!(rms(&still) < 1e-6, "{}", rms(&still));
    }

    #[test]
    fn an_electric_car_is_its_tyres_and_a_faint_whine() {
        let mut quiet = electric(3);
        let s = run(&mut quiet, at(30.0), 10.0);
        // The rolling noise alone is 9.2 dB under the whole at 30 km/h.
        assert!((level(&s) + 9.2).abs() < 1.0, "{} dB", level(&s));
        let engine = run(&mut car(3), at(30.0), 10.0);
        assert!(octave(&s, 63.0) < octave(&engine, 63.0) / 100.0, "no firing");
        // How much of a tone there is at `frequency`, as an amplitude.
        let tone = |frequency: f32| {
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for (i, x) in s[RATE as usize..2 * RATE as usize].iter().enumerate() {
                let turn = std::f64::consts::TAU * frequency as f64 * i as f64 / RATE as f64;
                re += *x as f64 * turn.cos();
                im += *x as f64 * turn.sin();
            }
            (2.0 * (re * re + im * im).sqrt() / RATE as f64) as f32
        };
        let pitch = WHINE * 30.0 * quiet.whine;
        assert!((tone(pitch) / WHINE_LEVEL - 1.0).abs() < 0.3, "the whine: {} at {pitch} Hz", tone(pitch));
        assert!(tone(pitch * 1.07) < WHINE_LEVEL / 3.0, "and nothing beside it: {}", tone(pitch * 1.07));
        assert!(tone(pitch) < rms(&s) / 4.0, "faint");
    }

    #[test]
    fn the_same_seed_is_the_same_car_and_two_seeds_are_not_copies() {
        let found = |seed: u32| run(&mut car(seed), at(30.0), 10.0);
        assert!(found(6) == found(6), "the same seed, another sound");
        for (one, other) in [(6, 7), (1, 2), (3, 40)] {
            // Their tyres share nothing; their engines, at the same speed in the same gear, turn within a
            // few hundredths of each other, as two real cars' do, and part of their firing lines up.
            let likeness = alike(&found(one), &found(other));
            assert!(likeness.abs() < 0.25, "seeds {one} and {other}: {likeness}");
            let electric = |seed: u32| run(&mut electric(seed), at(30.0), 10.0);
            let likeness = alike(&electric(one), &electric(other));
            assert!(likeness.abs() < 0.1, "electric, seeds {one} and {other}: {likeness}");
        }
        // Nor the same engine or the same colour.
        let (mut one, mut other) = (car(6), car(7));
        run(&mut one, at(30.0), 0.1);
        run(&mut other, at(30.0), 0.1);
        assert!((one.revs - other.revs).abs() > 1.0 && one.colour != other.colour);
    }

    #[test]
    fn the_tyres_never_repeat_and_never_hold_still() {
        let s = run(&mut electric(4), at(30.0), 12.0);
        let second = RATE as usize;
        for lag in [second / 2, second, 2 * second] {
            let likeness = alike(&s[second..5 * second], &s[second + lag..5 * second + lag]);
            assert!(likeness.abs() < 0.05, "{lag} samples later: {likeness}");
        }
        // The level of each quarter second, dB: it moves, by a decibel or so.
        let levels: Vec<f32> = s[second..].chunks(second / 4).map(|quarter| decibels(rms(quarter))).collect();
        let mean = levels.iter().sum::<f32>() / levels.len() as f32;
        let spread = (levels.iter().map(|l| (l - mean).powi(2)).sum::<f32>() / levels.len() as f32).sqrt();
        assert!(spread > 0.3 && spread < 1.5, "{spread} dB");
    }

    #[test]
    fn a_car_leaves_nothing_standing_and_stays_within_bounds() {
        for (name, params) in [("car", Params::default()), ("electric", Params::default().with("engine", 0.0))] {
            let s = run(&mut Car::new(8, &params), at(30.0), 10.0);
            let mean = s.iter().sum::<f32>() / s.len() as f32;
            assert!(mean.abs() < 0.001, "{name}: a mean of {mean}");
            // Nothing a scene can ask takes it out of range.
            for drive in [
                Drive { accel: 12.0, ..at(250.0) },
                Drive { accel: -12.0, detail: 0.0, ..at(130.0) },
                Drive { speed: -20.0, accel: f32::NAN, ..Drive::default() },
                Drive { speed: f32::NAN, ..Drive::default() },
                Drive { speed: f32::INFINITY, load: 7.0, ..Drive::default() },
            ] {
                let s = run(&mut Car::new(8, &params), drive, 2.0);
                assert!(s.iter().all(|x| x.is_finite() && x.abs() <= 1.0), "{name}: {drive:?}");
            }
        }
        // At the nominal level the mix does nothing.
        let peak = run(&mut car(8), at(30.0), 10.0).iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak < 0.5, "{peak}");
    }

    #[test]
    fn the_length_of_a_block_changes_nothing() {
        let found = |voice: &mut dyn Voice, length: usize| {
            let mut out = vec![0.0; RATE as usize / 2];
            out.chunks_mut(length).for_each(|block| voice.render(&Drive { accel: 0.5, ..at(40.0) }, block));
            out
        };
        let whole = found(&mut car(9), 128);
        assert!(whole == found(&mut car(9), 7) && whole == found(&mut car(9), 4_800));
        let whole = found(&mut electric(9), 128);
        assert!(whole == found(&mut electric(9), 7) && whole == found(&mut electric(9), 4_800));
        let whole = found(&mut scooter(9), 128);
        assert!(whole == found(&mut scooter(9), 7) && whole == found(&mut scooter(9), 4_800));
    }

    #[test]
    fn no_change_of_drive_or_of_detail_makes_a_step_larger_than_the_sounds_own() {
        let far = Drive { detail: 0.0, ..at(30.0) };
        for (before, after) in [
            (at(30.0), at(50.0)),
            (at(50.0), at(0.0)),
            (at(0.0), at(30.0)),
            (at(30.0), Drive { accel: 3.0, ..at(30.0) }),
            (Drive { accel: 3.0, ..at(30.0) }, Drive { accel: -3.0, ..at(30.0) }),
            (at(30.0), far),
            (far, at(30.0)),
        ] {
            for engine in [1.0, 0.0] {
                let mut voice = Car::new(5, &Params::default().with("engine", engine));
                let (about, elsewhere) = changed(&mut voice, before, after);
                // Within a tenth: the largest step of a noise is itself a matter of chance.
                assert!(about <= elsewhere * 1.1, "engine {engine}, {before:?} to {after:?}: {about} over {elsewhere}");
            }
        }
        for (before, after) in [
            (at(30.0), at(45.0)),
            (at(30.0), at(0.0)),
            (at(0.0), at(30.0)),
            (at(30.0), Drive { load: 1.0, ..at(30.0) }),
            (Drive { load: 1.0, ..at(30.0) }, Drive { load: 0.1, detail: 0.0, ..at(30.0) }),
        ] {
            let (about, elsewhere) = changed(&mut scooter(5), before, after);
            assert!(about <= elsewhere, "scooter, {before:?} to {after:?}: {about} over {elsewhere}");
        }
    }

    #[test]
    fn heard_from_afar_a_car_keeps_its_level_and_its_colour() {
        for engine in [1.0, 0.0] {
            for kmh in [30.0, 50.0] {
                let mut voice = mean_car(&Params::default().with("engine", engine), TRIMS);
                voice.whine = 0.0;
                let s = run(&mut voice, Drive { detail: 0.2, ..at(kmh) }, 20.0);
                let want = wanted(kmh, 1.0, engine);
                let whole = 10.0 * (rms(&s[s.len() / 20..]).powi(2) / want.iter().sum::<f32>()).log10();
                assert!(whole.abs() < 1.0, "engine {engine}, {kmh} km/h: {whole} dB");
                for (centre, want) in OCTAVES.into_iter().zip(want) {
                    let miss = 10.0 * (octave(&s, centre) / want).log10();
                    assert!(miss.abs() < 5.0, "engine {engine}, {kmh} km/h, {centre} Hz: {miss} dB");
                }
            }
        }
        // Fewer bands are made: the near ones are left where they were.
        let mut voice = car(2);
        run(&mut voice, Drive { detail: 0.0, ..at(30.0) }, 0.5);
        let left = voice.bank[0];
        run(&mut voice, Drive { detail: 0.0, ..at(30.0) }, 0.5);
        assert!(voice.bank[0].z == left.z);
    }

    #[test]
    fn a_bigger_car_is_lower_and_louder_and_tone_tilts_the_tyres() {
        let found =
            |name: &str, value: f32| run(&mut mean_car(&Params::default().with(name, value), TRIMS), at(30.0), 10.0);
        let (small, big) = (found("size", 0.7), found("size", 1.5));
        assert!(level(&big) > level(&small) + 2.0, "{} and {}", level(&big), level(&small));
        // Lower: more of the tyres under their hump than over it.
        let lean = |s: &[f32]| 10.0 * (octave(s, 500.0) / octave(s, 2_000.0)).log10();
        assert!(lean(&big) > lean(&small) + 2.0, "{} and {}", lean(&big), lean(&small));
        let (dull, bright) = (found("tone", -1.0), found("tone", 1.0));
        assert!((level(&dull) - level(&bright)).abs() < 0.5, "at the same level");
        assert!(lean(&dull) > lean(&bright) + 2.0, "{} and {}", lean(&dull), lean(&bright));
    }

    #[test]
    fn a_scooter_at_30_sounds_at_the_nominal_level_whatever_its_seed() {
        for seed in 1..=8 {
            let found = level(&run(&mut scooter(seed), at(30.0), 10.0));
            assert!(found.abs() < 1.0, "seed {seed}: {found} dB");
        }
        // No load given is the default throttle.
        let said = run(&mut scooter(3), Drive { load: 0.6, ..at(30.0) }, 2.0);
        assert!(run(&mut scooter(3), at(30.0), 2.0) == said);
    }

    #[test]
    fn a_scooter_holds_its_revs_on_the_variator_a_little_higher_when_faster() {
        // Once a turn: the period gives the revs.
        let revs = |kmh: f32| {
            let s = run(&mut scooter(2), at(kmh), 1.5);
            60.0 * RATE as f32 / period(&s[RATE as usize / 2..], 300, 520) as f32
        };
        let (slower, faster) = (revs(30.0), revs(45.0));
        assert!((6_500.0..7_500.0).contains(&slower), "{slower} turns a minute at 30 km/h");
        assert!(faster > slower + 100.0 && faster < slower + 600.0, "{faster} at 45 km/h, {slower} at 30");
    }

    #[test]
    fn the_throttle_makes_a_scooter_louder_and_at_rest_it_idles() {
        let found = |drive: Drive| level(&run(&mut scooter(4), drive, 5.0));
        let (shut, open) = (found(Drive { load: 0.2, ..at(30.0) }), found(Drive { load: 1.0, ..at(30.0) }));
        assert!(open > shut + 2.0, "{open} dB flat out, {shut} dB nearly shut");
        let idle = found(at(0.0));
        assert!(idle < -3.0 && idle > -14.0, "{idle} dB at rest");
    }

    #[test]
    fn the_kart_is_told_it_stands_still_on_asphalt() {
        let mut voice = scooter(1);
        for kmh in [0.0, 30.0, 45.0, 120.0, f32::NAN] {
            voice.wanted = Drive { load: 1.0, ..at(kmh) };
            let drive = voice.kart_drive();
            // No wind, no slide, no kerb; under the revs the kart's mix rides.
            assert_eq!((drive.speed, drive.slip, drive.ground), (0.0, 0.0, 0.0));
            assert!((0.0..0.55).contains(&drive.revs) && (0.0..=1.0).contains(&drive.throttle), "{drive:?}");
        }
        // And so a scooter is no louder for going faster than its revs make it.
        let found = |kmh: f32| level(&run(&mut scooter(1), at(kmh), 5.0));
        assert!((found(90.0) - found(30.0)).abs() < 1.5, "{} at 90 km/h, {} at 30", found(90.0), found(30.0));
    }

    #[test]
    fn the_same_seed_is_the_same_scooter_and_two_seeds_are_not_copies() {
        let found = |seed: u32| run(&mut scooter(seed), at(30.0), 10.0);
        assert!(found(6) == found(6), "the same seed, another sound");
        for (one, other) in [(6, 7), (1, 2), (3, 40)] {
            // Their tyres share nothing; their engines, at the same speed in the same gear, turn within a
            // few hundredths of each other, as two real cars' do, and part of their firing lines up.
            let likeness = alike(&found(one), &found(other));
            assert!(likeness.abs() < 0.25, "seeds {one} and {other}: {likeness}");
            let electric = |seed: u32| run(&mut electric(seed), at(30.0), 10.0);
            let likeness = alike(&electric(one), &electric(other));
            assert!(likeness.abs() < 0.1, "electric, seeds {one} and {other}: {likeness}");
        }
    }

    #[test]
    fn a_scooter_leaves_nothing_standing_and_stays_within_bounds() {
        let s = run(&mut scooter(8), at(30.0), 10.0);
        let mean = s.iter().sum::<f32>() / s.len() as f32;
        assert!(mean.abs() < 0.001, "a mean of {mean}");
        for drive in [
            Drive { load: 1.0, ..at(250.0) },
            Drive { load: 9.0, accel: 9.0, ..at(60.0) },
            Drive { speed: f32::NAN, load: f32::NAN, ..Drive::default() },
            Drive { speed: f32::NEG_INFINITY, ..Drive::default() },
        ] {
            let s = run(&mut scooter(8), drive, 2.0);
            assert!(s.iter().all(|x| x.is_finite() && x.abs() <= 1.0), "{drive:?}");
        }
    }

    /// Works out [`TRIMS`]: makes each band as loud as it takes for the
    /// model's level to come out of its octaves at 30 km/h, and prints them.
    #[test]
    #[ignore = "a measurement: run it to work the trims out again"]
    fn calibrate() {
        let mut trims = TRIMS;
        for round in 0..10 {
            let mut worst = 0.0f32;
            for part in 0..2 {
                for far in [false, true] {
                    // One part alone, and no whine.
                    let mut car = mean_car(&Params::default(), trims);
                    (car.engine, car.tyres, car.whine) = (part as f32, 1.0 - part as f32, 0.0);
                    let detail = if far { 0.0 } else { 1.0 };
                    let s = run(&mut car, Drive { detail, ..at(30.0) }, 40.0);
                    let got = OCTAVES.map(|centre| octave(&s, centre));
                    let want = wanted(30.0, 1.0 - part as f32, part as f32);
                    let mut fit = |trim: &mut f32, (from, to): (usize, usize)| {
                        let miss =
                            10.0 * (want[from..to].iter().sum::<f32>() / got[from..to].iter().sum::<f32>()).log10();
                        worst = worst.max(miss.abs());
                        *trim *= db(miss.clamp(-6.0, 6.0) * 0.8);
                    };
                    if far {
                        (0..2).for_each(|lane| fit(&mut trims.far[part][lane], FAR[part][lane].2));
                    } else {
                        (0..4).for_each(|lane| fit(&mut trims.near[part][lane], NEAR[part][lane].2));
                    }
                }
            }
            println!("round {round}: off by {worst:.2} dB at most before\n{trims:.2?}");
        }
    }

    /// Ten seconds of each, timed: `cargo test road -- --ignored cost
    /// --nocapture --test-threads 1`. The fastest quarter second is the one
    /// nothing else on the machine got in the way of.
    #[test]
    #[ignore = "a measurement: nanoseconds a sample"]
    fn cost() {
        type Render = Box<dyn FnMut(&mut [f32])>;
        let time = |name: &str, mut render: Render| {
            let mut out = vec![0.0f32; RATE as usize / 4];
            let (mut all, mut fastest) = (0.0f32, f32::MAX);
            for _ in 0..40 {
                let start = std::time::Instant::now();
                out.chunks_mut(128).for_each(&mut render);
                let ns = start.elapsed().as_nanos() as f32 / out.len() as f32;
                (all, fastest) = (all + ns / 40.0, fastest.min(ns));
            }
            println!("{name:<24} {all:6.1} ns a sample over 10 s, {fastest:6.1} at the fastest");
        };
        let voice = |kind: &str, params: Params, drive: Drive| -> Render {
            let mut voice = crate::make(kind, 6, &params).unwrap();
            Box::new(move |block| voice.render(&drive, block))
        };
        let (plain, electric) = (Params::default(), Params::default().with("engine", 0.0));
        let far = Drive { detail: 0.0, ..at(30.0) };
        time("car", voice("car", plain.clone(), at(30.0)));
        time("car, from afar", voice("car", plain.clone(), far));
        time("car, pulling", voice("car", plain.clone(), Drive { accel: 1.5, ..at(30.0) }));
        time("electric car", voice("car", electric.clone(), at(30.0)));
        time("electric car, from afar", voice("car", electric, far));
        time("scooter", voice("scooter", plain, at(30.0)));
        let mut kart = Kart::new(6);
        kart.set(KartDrive { revs: 0.36, throttle: 0.6, ..KartDrive::default() });
        time("the kart's voice itself", Box::new(move |block| kart.render(block)));
    }
}
