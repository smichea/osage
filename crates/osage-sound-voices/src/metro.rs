// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! A rubber-tyred metro and the steel viaduct under it: the kinds
//! "metro-bogie", "metro-motor", "metro-joint" and "metro-span".
//!
//! The train is an MP 89 CC of Paris line 6: five cars, ten bogies, tyres on
//! steel runways, guide wheels on side bars, asynchronous motors fed by GTO
//! thyristor inverters. It has no steel wheel on a rail in plain track, so
//! none of these voices squeals, sings or clatters.
//!
//! **A bogie** ([`Bogie`]), one emitter each, in order:
//!
//! - **the tyres:** white noise through a resonance at 670 Hz and two gentle
//!   low-passes, which together have the octave shape of the rolling noise of
//!   a heavy vehicle's tyres (CNOSSOS-EU, category 3) at 40 km/h; three gains
//!   follow the speed — the noise as it is (32.5 dB a decade), the resonance
//!   (23.75) and what is above 1.5 kHz (10 more) — so the shape and the
//!   level stay the table's from 20 to 80 km/h, within 2 dB;
//! - **the body:** the same noise through a band near 100 Hz, swelling a
//!   little as each of the two axles turns;
//! - **the guide wheels:** short dry ticks of noise near 1.2 kHz, about one a
//!   metre, faint.
//!
//! An RMS of [`NOMINAL_RMS`](crate::NOMINAL_RMS) at 40 km/h; fades out under
//! 3 km/h and is silent at rest. Reads `Drive::speed` and `Drive::detail`
//! (under a half, no guide wheels). Parameters: `wheel` (m, a tyre's
//! diameter), `body` and `rattle` (0…2, how much of each).
//!
//! **A motor car's traction** ([`Motor`]), one emitter each, in order:
//!
//! - **two inverters,** a little apart in pitch as two bogies' wheels are in
//!   size. Each is a tone of four harmonics. Under 13 Hz of motor current the
//!   tone is a fixed carrier near 400 Hz; above, it is a whole number of
//!   pulses in a turn of the current — 27, 15, 9, 5, 3, then 1 — so it rises
//!   with the speed and drops at each change, which lasts 45 ms. The current
//!   beats in it at three times its own frequency: the sidebands;
//! - **the gear:** the mesh tone, 40 Hz for each m/s, and its second
//!   harmonic, swaying with the wheel's turn;
//! - **the casing,** a resonance near 950 Hz that both ring;
//! - **the fan:** a breath of noise that grows with the square of the speed.
//!
//! The inverters are as loud as the effort, `max(load, |accel| / 1 m/s²)`
//! within 0…1; braking is traction backwards and sounds the same, the ladder
//! walked down. Coasting leaves a tenth of the gear and the fan. An RMS of
//! [`NOMINAL_RMS`](crate::NOMINAL_RMS) at 40 km/h with an effort of 0.5.
//! Reads `speed`, `accel`, `load` and `detail` (under a half, one inverter
//! and no fan). Parameters: `carrier` (Hz), `electrical` (Hz of motor
//! current for each m/s), `mesh` (Hz of gear mesh for each m/s), `gear`
//! (0…2, how much of it).
//!
//! **A joint of the runway crossed** ([`Joint`]): each `Drive::hit` is one
//! axle's tyres. A push of 3 ms sets ringing a thump (60 to 90 Hz, 100 to
//! 135 ms) and two muted knocks (200 to 400 Hz, 45 to 70 ms), a little
//! different at each blow; the knocks are roughened by noise during the
//! push. Peaks about 0.5 for a strength of 1, is over within 150 ms, and
//! gives exact silence without a hit. Up to eight sound at once. Reads `hit`
//! and `detail` (under a half, one knock). Parameters: `thump` and `knock`
//! (Hz, the middle of each), `ring` (s, the thump's length).
//!
//! **A span of the viaduct** ([`Span`]), one emitter each, in order:
//!
//! - **the modes:** white noise rings six resonators the seed tunes within
//!   6 % of 47, 63, 88, 121, 164 and 215 Hz, with a Q of 12 to 20. They are
//!   fed, never held: when the train leaves they ring down by themselves, the
//!   lowest over 0.5 to 0.9 s;
//! - **the floor:** the same noise between 40 and 250 Hz, unresonant;
//! - **the rattle:** noise at 300 to 600 Hz let through only while the modes
//!   swing past a threshold, as a loose plate buzzes on the peaks.
//!
//! As loud as the square root of `load` (bogies add as powers) times the
//! speed. An RMS of [`NOMINAL_RMS`](crate::NOMINAL_RMS) at a load of 1 and
//! 40 km/h, averaged over tens of seconds: a few narrow modes swell and fade
//! by a decibel or so from one ten seconds to the next, as the real ones do.
//! Exact silence once the modes have rung down. Reads `load`, `speed` and
//! `detail` (under a half, four modes and no rattle). Parameters: `tone`
//! (times the modes' frequencies: a shorter span rings higher), `ring`
//! (times their Q), `rattle` (0…2).
//!
//! What changes slowly is set every 32 samples, counted across calls, so a
//! voice gives the same samples whatever the length of the blocks.
//!
//! # Assumptions
//!
//! No measured spectrum, level or inverter frequency of an MP 89 was found.
//! What follows is not backed by a source for this train or this bridge.
//!
//! - The tyres' spectrum and its law with speed are those of a lorry's tyres
//!   on asphalt (CNOSSOS-EU category 3 rolling noise, Directive (EU)
//!   2015/996), not of a metro's on steel.
//!   The table stops at 20 km/h; the law is carried on below it.
//! - The body near 100 Hz, its level and its swell with the wheels are
//!   guesses, as are the guide wheels' ticks: their pitch, rate and level.
//! - The carrier, the ladder of pulse numbers, the motor current at which
//!   each change happens (13, 17, 29, 48, 70 and 96 Hz), 5.5 Hz of current
//!   for each m/s, the slip (1.2 Hz at full effort), the harmonics of the
//!   tone, the sidebands at three times the current and the casing's
//!   resonance describe GTO drives in general, tuned by ear to nothing.
//! - 40 Hz of gear mesh for each m/s, its level and the fan are guesses.
//! - Loudness proportional to effort, and the same for braking as for
//!   traction, are guesses.
//! - The joint's two parts, their pitches and lengths are guesses; a tyre is
//!   soft, so no click was given to it.
//! - The span's modes are not those of the Viaduc de Passy: the six
//!   frequencies, their Q, the floor, the rattle and its threshold are
//!   guesses, as are loudness proportional to speed and to the square root
//!   of the load.
//! - Tyres wear: a train's wheels are taken to differ by up to 1.5 % in
//!   size, which sets how far two motors are apart in pitch.

use std::f32::consts::{PI, TAU};

use crate::{
    Drive, Params, Voice,
    dsp::{DT, Low, Noise, soft},
};

/// Samples between two updates of what changes slowly: eased controls,
/// gains, frequencies (as `osage-kart-sound`).
const CONTROL: usize = 32;
/// The speed the bogie's, the motor's and the span's levels are set at, m/s.
const NOMINAL_SPEED: f32 = 40.0 / 3.6;
/// No voice follows a speed beyond this, m/s: the train's own limit is 70
/// km/h, and a level that grows with speed has to stop somewhere.
const TOP_SPEED: f32 = 25.0;

/// Four values worked on side by side. Written as loops of four, which the
/// compiler turns into single operations on four numbers: four tones then
/// cost little more than one.
type Four = [f32; 4];

/// The value a scene gave `name`, or the kind's default.
fn param(params: &Params, defaults: &[(&str, f32)], name: &str) -> f32 {
    params.get(name, defaults.iter().find(|(n, _)| *n == name).map_or(0.0, |(_, value)| *value))
}

/// `x` within `low`…`high`; what is not a number counts as nothing.
fn within(x: f32, low: f32, high: f32) -> f32 {
    if x.is_finite() { x.clamp(low, high) } else { 0f32.clamp(low, high) }
}

/// Keeps a sample within −1…1 and leaves it as it is under 0.7: a voice at
/// its nominal level never reaches the bend.
fn guard(x: f32) -> f32 {
    const KNEE: f32 = 0.7;
    let size = x.abs();
    if size <= KNEE { x } else { (KNEE + (1.0 - KNEE) * soft((size - KNEE) / (1.0 - KNEE))).copysign(x) }
}

/// A turn of a wave close to a sine, for a phase in 0…1: two parabolas, 0
/// at 0 and at a half, −1 at a quarter. Its third harmonic is 29 dB down
/// and the rest falls as the cube of the order, so nothing of it reaches
/// half the rate. A true sine cost most of a tone's time.
fn arch(phase: f32) -> f32 {
    let half = phase - 0.5;
    16.0 * half * (0.5 - half.abs())
}

/// 0 at 0, 1 at 1, level at both.
fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// How much of the power of a tone at `hertz` a one-pole low-pass at
/// `corner` keeps.
fn kept(corner: f32, hertz: f32) -> f64 {
    let a = 1.0 - f64::from(-TAU * corner * DT).exp();
    a * a / (1.0 + (1.0 - a).powi(2) - 2.0 * (1.0 - a) * f64::from(TAU * hertz * DT).cos())
}

/// A control that follows what it is given without a step, moved once every
/// [`CONTROL`] samples.
#[derive(Clone, Debug)]
struct Eased {
    now: f32,
    rate: f32,
}

impl Eased {
    /// Starting at `now`, 63 % of the way in `seconds`.
    fn new(now: f32, seconds: f32) -> Self {
        Self { now, rate: 1.0 - (-(CONTROL as f32) * DT / seconds).exp() }
    }

    fn follow(&mut self, target: f32) -> f32 {
        self.now += (target - self.now) * self.rate;
        // It arrives, so that a voice can tell silence and go to sleep.
        if (target - self.now).abs() < 1e-5 {
            self.now = target;
        }
        self.now
    }
}

/// Gains that go in straight lines from one control's values to the next:
/// a tone's level must not move in steps.
#[derive(Clone, Debug, Default)]
struct Ramps {
    now: Four,
    goal: Four,
    step: Four,
}

impl Ramps {
    fn aim(&mut self, goal: Four) {
        for (i, wanted) in goal.into_iter().enumerate() {
            self.now[i] = self.goal[i];
            self.goal[i] = wanted;
            self.step[i] = (wanted - self.now[i]) / CONTROL as f32;
        }
    }

    /// Neither sounding nor about to.
    fn idle(&self) -> bool {
        self.now == [0.0; 4] && self.goal == [0.0; 4]
    }
}

/// Something that never holds still: noise slowed to `hertz`, of unit RMS,
/// moved once every [`CONTROL`] samples.
#[derive(Clone, Debug)]
struct Wander {
    noise: Noise,
    y: f32,
    a: f32,
    scale: f32,
}

impl Wander {
    fn new(noise: Noise, hertz: f32) -> Self {
        let a = 1.0 - (-TAU * hertz * CONTROL as f32 * DT).exp();
        // The noise has a variance of a third; the low-pass keeps a / (2 − a)
        // of it.
        Self { noise, y: 0.0, a, scale: (3.0 * (2.0 - a) / a).sqrt() }
    }

    fn next(&mut self) -> f32 {
        self.y += (self.noise.next() - self.y) * self.a;
        self.y * self.scale
    }
}

// ---------------------------------------------------------------- the bogie

pub const BOGIE: &str = "A bogie of a rubber-tyred metro rolling: the roar of its tyres on the steel runways, a low body, the dry rattle of its guide wheels";
pub const BOGIE_PARAMS: &[(&str, f32)] = &[("wheel", 1.0), ("body", 1.0), ("rattle", 1.0)];

/// The tyres' resonance: with the two low-passes below, the cheapest filter
/// found whose octave levels are the table's at 40 km/h within 1.1 dB. Hz,
/// Q, and how much of it is added to the noise.
const HUMP: (f32, f32, f32) = (672.0, 2.55, 2.4);
/// The two low-passes after it, Hz.
const ROUND: [f32; 2] = [721.0, 2_790.0];
/// What is above this rises faster with speed than the rest, Hz.
const BRIGHT: f32 = 1_500.0;
/// Sets the bogie's level at 40 km/h; the test of the nominal level holds
/// it.
const BOGIE_LEVEL: f32 = 0.369;
/// The body's level beside the tyres'.
const BODY: f32 = 3.0;
/// Guide-wheel ticks for each metre run.
const TICKS_A_METRE: f32 = 1.0;
/// A tick's swing at its loudest, at 40 km/h.
const TICK: f32 = 0.35;

/// A resonance: a band-pass at its own level at its centre (Audio EQ
/// Cookbook). Its zeros are at nothing and at half the rate, which is why
/// three multiplies do where a second-order section takes five.
#[derive(Clone, Debug, Default)]
struct Hump {
    b: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Hump {
    fn at(hertz: f32, q: f32) -> Self {
        let turn = TAU * hertz * DT;
        let alpha = turn.sin() / (2.0 * q);
        Self {
            b: alpha / (1.0 + alpha),
            a1: -2.0 * turn.cos() / (1.0 + alpha),
            a2: (1.0 - alpha) / (1.0 + alpha),
            ..Self::default()
        }
    }

    fn pass(&mut self, x: f32) -> f32 {
        let y = self.b * (x - self.x2) - self.a1 * self.y1 - self.a2 * self.y2;
        (self.x2, self.x1) = (self.x1, x);
        (self.y2, self.y1) = (self.y1, y);
        y
    }
}

/// A bogie rolling.
#[derive(Clone, Debug)]
pub struct Bogie {
    noise: Noise,
    hump: Hump,
    round: [Low; 2],
    bright: Low,
    /// The body: what the first low-pass keeps less what the second does.
    body: [Low; 2],
    /// The guide wheels: when one ticks, the band it rings, how much of the
    /// last tick is left and how much stays after a sample.
    ticks: Noise,
    rattle: Hump,
    tick: f32,
    tick_fade: f32,
    drift: Wander,
    speed: Eased,
    detail: Eased,
    /// Where each axle is in its turn, and turns for each metre.
    axles: [f32; 2],
    turns: [f32; 2],
    body_share: f32,
    rattle_share: f32,
    /// Samples until the controls are next set.
    until: usize,
    awake: bool,
    // Set every `CONTROL` samples.
    plain: f32,
    peak: f32,
    tilt: f32,
    low: f32,
    clack: f32,
}

impl Bogie {
    pub fn new(seed: u32, params: &Params) -> Self {
        let mut noise = Noise::new(seed);
        let mut shape = noise.fork();
        let wheel = within(param(params, BOGIE_PARAMS, "wheel"), 0.3, 3.0);
        // Two axles' tyres are never worn alike: they drift in and out of
        // step.
        let turns = [1.0 / (PI * wheel * shape.between(0.99, 1.0)), 1.0 / (PI * wheel * shape.between(1.0, 1.01))];
        Self {
            hump: Hump::at(HUMP.0, HUMP.1),
            round: [Low::at(ROUND[0]), Low::at(ROUND[1])],
            bright: Low::at(BRIGHT),
            body: [Low::at(shape.between(150.0, 170.0)), Low::at(shape.between(56.0, 64.0))],
            ticks: noise.fork(),
            rattle: Hump::at(shape.between(1_000.0, 1_400.0), 3.5),
            tick: 0.0,
            tick_fade: (-DT / 0.003).exp(),
            drift: Wander::new(noise.fork(), 1.2),
            speed: Eased::new(0.0, 0.04),
            detail: Eased::new(1.0, 0.03),
            axles: [shape.unit(), shape.unit()],
            turns,
            body_share: within(param(params, BOGIE_PARAMS, "body"), 0.0, 2.0),
            rattle_share: within(param(params, BOGIE_PARAMS, "rattle"), 0.0, 2.0),
            until: 0,
            awake: false,
            plain: 0.0,
            peak: 0.0,
            tilt: 0.0,
            low: 0.0,
            clack: 0.0,
            noise,
        }
    }

    fn control(&mut self, drive: &Drive) {
        let speed = self.speed.follow(within(drive.speed.abs(), 0.0, TOP_SPEED));
        let detail = self.detail.follow(within(drive.detail, 0.0, 1.0));
        if speed == 0.0 {
            // At rest nothing rolls. The filters keep what they held: it is
            // noise like any other, under a gain that comes back from nothing.
            self.awake = false;
            self.tick = 0.0;
            return;
        }
        self.awake = true;
        // Powers of the speed by square roots: x^1.625, x^1.1875 and x^0.5
        // are the three laws, and a root costs less than a power.
        let x = speed / NOMINAL_SPEED;
        let r2 = x.sqrt();
        let r8 = r2.sqrt().sqrt();
        let r16 = r8.sqrt();
        // The runway is never even: the roar wanders by a decibel.
        let level = BOGIE_LEVEL * smooth(speed * 3.6 / 3.0) * (1.0 + 0.1 * self.drift.next().clamp(-3.0, 3.0));
        self.plain = level * x * r2 * r8;
        self.peak = level * HUMP.2 * x * r8 * r16;
        self.tilt = r2 - 1.0;
        let mut swell = 0.0;
        for (axle, turns) in self.axles.iter_mut().zip(self.turns) {
            *axle += speed * turns * CONTROL as f32 * DT;
            if *axle >= 1.0 {
                *axle -= 1.0;
            }
            swell += arch(*axle);
        }
        self.low = level * BODY * self.body_share * x * r2 * r8 * (1.0 + 0.1 * swell);
        // The guide wheels: fewer than a tick every control, so one draw
        // says whether one starts now.
        let rattle = self.rattle_share * smooth((detail - 0.35) / 0.3);
        self.clack = TICK * rattle * x;
        if rattle > 0.0 && self.ticks.unit() < speed * TICKS_A_METRE * CONTROL as f32 * DT {
            let strength = self.ticks.unit();
            self.tick = self.tick.max(0.3 + 0.7 * strength * strength);
        }
    }

    fn run(&mut self, out: &mut [f32]) {
        if !self.awake {
            out.fill(0.0);
            return;
        }
        let (plain, peak, tilt, low, clack) = (self.plain, self.peak, self.tilt, self.low, self.clack);
        for sample in out {
            let noise = self.noise.next();
            let rounded = self.round[0].pass(plain * noise + peak * self.hump.pass(noise));
            let tyres = self.round[1].pass(rounded);
            let body = self.body[0].pass(noise) - self.body[1].pass(noise);
            let mut sum = tyres + tilt * self.bright.above(tyres) + low * body;
            if self.tick > 1e-4 {
                sum += self.rattle.pass(self.ticks.next()) * self.tick * clack;
                self.tick *= self.tick_fade;
            }
            *sample = guard(sum);
        }
    }
}

impl Voice for Bogie {
    fn render(&mut self, drive: &Drive, out: &mut [f32]) {
        if !self.awake && drive.speed == 0.0 {
            // At rest and asked for nothing: a bogie costs nothing.
            out.fill(0.0);
            self.until = 0;
            return;
        }
        let mut at = 0;
        while at < out.len() {
            if self.until == 0 {
                self.control(drive);
                self.until = CONTROL;
            }
            let n = self.until.min(out.len() - at);
            self.run(&mut out[at..at + n]);
            self.until -= n;
            at += n;
        }
    }
}

// ---------------------------------------------------------------- the motor

pub const MOTOR: &str =
    "A motor car's traction: the stepped whine of two GTO inverters, loud with the effort, and the whine of the gear";
pub const MOTOR_PARAMS: &[(&str, f32)] = &[("carrier", 400.0), ("electrical", 5.5), ("mesh", 40.0), ("gear", 1.0)];

/// The acceleration at which the motors give all they have, m/s².
const FULL_EFFORT: f32 = 1.0;
/// How long a change of pulse number lasts, s.
const CHANGE: f32 = 0.045;
/// A drive changes down at a little less current than it changed up at, so
/// that a train holding the speed of a change does not go back and forth.
const HYSTERESIS: f32 = 0.94;
/// The second inverter beside the first.
const SECOND: f32 = 0.5;
/// A loaded motor's current turns faster than its rotor: Hz at full effort.
const SLIP: f32 = 1.2;
/// Sets the motor's level at 40 km/h with an effort of 0.5; the test of the
/// nominal level holds it.
const MOTOR_LEVEL: f32 = 0.0873;
/// The share of the way to the output a follower of its slowest part goes
/// in a sample: 5 Hz.
const STILL: f32 = std::f32::consts::TAU * 5.0 / crate::RATE as f32;
/// The gear, at 40 km/h under full effort, beside an inverter.
const GEAR: f32 = 0.3;
/// The casing's resonance: how much it adds to a harmonic that falls on
/// it, and how sharp it is.
const CASING: f32 = 1.2;
const CASING_Q: f32 = 1.5;

/// One way an inverter chops its voltage.
#[derive(Clone, Copy, Debug)]
struct Pulses {
    /// Pulses in a turn of the motor current; 0 for the carrier, which keeps
    /// its own time.
    count: f32,
    /// The motor current, Hz, at which the drive changes to the next way.
    until: f32,
    /// How deep the current beats in the tone.
    growl: f32,
    /// The tone's first four harmonics, together of unit RMS.
    harmonics: Four,
}

impl Pulses {
    fn new(count: f32, until: f32, growl: f32, harmonics: Four) -> Self {
        let rms = (harmonics.iter().map(|a| a * a).sum::<f32>() / 2.0).sqrt();
        Self { count, until, growl, harmonics: harmonics.map(|a| a / rms) }
    }

    /// The tone at `hertz` as a polynomial of a sine: Chebyshev's
    /// polynomials each turn a sine into one harmonic and nothing else,
    /// where a wave drawn as a shape (a saw, a pulse) would fold its upper
    /// harmonics back under half the rate. Each harmonic is as strong as the
    /// casing, ringing at `casing`, makes it. [`arch`] is not quite a sine:
    /// what the polynomial would keep of it on average (its squares average
    /// 8/15 and its fourth powers 128/315, not 1/2 and 3/8) is taken off,
    /// so that the tone pushes no way for long.
    fn shape(&self, hertz: f32, casing: f32) -> [f32; 5] {
        let mut h = self.harmonics;
        for (k, harmonic) in h.iter_mut().enumerate() {
            // 1 + CASING / (1 + Q²·(x − 1/x)²), with one division.
            let x2 = (hertz * (k + 1) as f32 / casing).powi(2);
            let off = CASING_Q * (x2 - 1.0);
            *harmonic *= 1.0 + CASING * x2 / (x2 + off * off + 1e-9);
        }
        // (For a true sine the constant would be −h₂ + h₄.)
        let c = [0.0, h[0] - 3.0 * h[2], 2.0 * h[1] - 8.0 * h[3], 4.0 * h[2], 8.0 * h[3]];
        [-c[2] * 8.0 / 15.0 - c[4] * 128.0 / 315.0, c[1], c[2], c[3], c[4]]
    }
}

/// The ways an inverter goes through from rest to full speed.
fn ladder() -> [Pulses; 7] {
    // A whine while the tone is high; a buzz, then a hum, once it is low and
    // mostly its harmonics reach the ear.
    const WHINE: Four = [1.0, 0.55, 0.35, 0.22];
    const BUZZ: Four = [1.0, 0.7, 0.5, 0.4];
    const HUM: Four = [0.5, 0.4, 0.35, 0.4];
    [
        Pulses::new(0.0, 13.0, 0.5, WHINE),
        Pulses::new(27.0, 17.0, 0.5, WHINE),
        Pulses::new(15.0, 29.0, 0.5, WHINE),
        Pulses::new(9.0, 48.0, 0.45, WHINE),
        Pulses::new(5.0, 70.0, 0.3, WHINE),
        // Three times the current is the tone itself here, and a harmonic of
        // it below: beating would only add a steady push.
        Pulses::new(3.0, 96.0, 0.0, BUZZ),
        Pulses::new(1.0, f32::INFINITY, 0.0, HUM),
    ]
}

/// A motor car's traction.
#[derive(Clone, Debug)]
pub struct Motor {
    ladder: [Pulses; 7],
    /// The way of chopping now, the one before, and how far the change from
    /// one to the other has gone, 0…1.
    mode: usize,
    from: usize,
    change: f32,
    /// The four tones — each inverter's now, then each inverter's before
    /// the change: where each is in its turn, how far it moves in a sample,
    /// its wave, how deep the current beats in it, and how loud it is.
    turn: Four,
    step: Four,
    shape: [Four; 5],
    growl: Four,
    weight: Ramps,
    /// Three times the motor current: where it is in its turn.
    beat: f32,
    beat_step: f32,
    /// Where the gear's mesh is in its turn, and the wheel in its own.
    mesh: f32,
    mesh_step: f32,
    wheel: f32,
    gear: f32,
    speed: Eased,
    /// The effort, negative when braking.
    pull: Eased,
    detail: Eased,
    drift: [Wander; 2],
    /// Each inverter's wheels beside the nominal (worn tyres turn faster)
    /// and its carrier, Hz.
    wear: [f32; 2],
    carrier: [f32; 2],
    casing: f32,
    per_speed: f32,
    mesh_per_speed: f32,
    gear_share: f32,
    until: usize,
    awake: bool,
    /// What the output holds under 5 Hz, followed: taken off it. Beating
    /// at three times the current, the tone makes a little of it whenever a
    /// harmonic of the beat falls on one of the tone's.
    still: f32,
}

impl Motor {
    pub fn new(seed: u32, params: &Params) -> Self {
        let mut noise = Noise::new(seed);
        let mut shape = noise.fork();
        let carrier = within(param(params, MOTOR_PARAMS, "carrier"), 100.0, 1_500.0);
        let wear = shape.between(0.985, 1.015);
        // The second bogie's wheels are a few thousandths larger or smaller,
        // its carrier a few hertz off: the two beat.
        let apart = shape.between(0.003, 0.008) * if shape.unit() < 0.5 { -1.0 } else { 1.0 };
        Self {
            ladder: ladder(),
            mode: 0,
            from: 0,
            change: 1.0,
            turn: std::array::from_fn(|_| shape.unit()),
            step: [0.0; 4],
            shape: [[0.0; 4]; 5],
            growl: [0.0; 4],
            weight: Ramps::default(),
            beat: shape.unit(),
            beat_step: 0.0,
            mesh: shape.unit(),
            mesh_step: 0.0,
            wheel: shape.unit(),
            gear: 0.0,
            speed: Eased::new(0.0, 0.04),
            pull: Eased::new(0.0, 0.06),
            detail: Eased::new(1.0, 0.03),
            drift: [Wander::new(noise.fork(), 0.7), Wander::new(noise.fork(), 0.7)],
            wear: [wear, wear * (1.0 + apart)],
            carrier: [carrier * shape.between(0.99, 1.01), carrier * shape.between(0.99, 1.01)],
            casing: shape.between(910.0, 990.0),
            per_speed: within(param(params, MOTOR_PARAMS, "electrical"), 1.0, 12.0),
            mesh_per_speed: within(param(params, MOTOR_PARAMS, "mesh"), 5.0, 150.0),
            gear_share: within(param(params, MOTOR_PARAMS, "gear"), 0.0, 2.0),
            until: 0,
            awake: false,
            still: 0.0,
        }
    }

    fn control(&mut self, drive: &Drive) {
        let speed = self.speed.follow(within(drive.speed.abs(), 0.0, TOP_SPEED));
        let detail = self.detail.follow(within(drive.detail, 0.0, 1.0));
        // Braking is traction backwards: the motors feed the line, through
        // the same inverters.
        let accel = within(drive.accel / FULL_EFFORT, -1.0, 1.0);
        let load = within(drive.load, 0.0, 1.0);
        let pull = self.pull.follow(if accel.abs() >= load { accel } else { load });
        let effort = pull.abs();

        // The way of chopping follows the current the speed asks for; one
        // change ends before the next begins.
        let current = self.per_speed * speed;
        if self.change >= 1.0 {
            let mut mode = self.mode;
            while self.ladder[mode].until < current {
                mode += 1;
            }
            while mode > 0 && current < self.ladder[mode - 1].until * HYSTERESIS {
                mode -= 1;
            }
            if mode != self.mode {
                // What sounded goes on where tones fade; the new tone comes
                // from nothing.
                for k in 0..2 {
                    self.turn[k + 2] = self.turn[k];
                    self.weight.goal[k + 2] = self.weight.goal[k];
                    self.weight.goal[k] = 0.0;
                }
                (self.from, self.mode, self.change) = (self.mode, mode, 0.0);
            }
        }
        // Two tones apart in pitch: crossed at equal power.
        let (mut coming, mut going) = (1.0, 0.0);
        if self.change < 1.0 {
            self.change = (self.change + CONTROL as f32 * DT / CHANGE).min(1.0);
            let (t, left) = (self.change, 1.0 - self.change);
            (coming, going) = (t * (1.5 - 0.5 * t * t), left * (1.5 - 0.5 * left * left));
        }

        // The chopping is there whenever the inverter works, whatever it
        // gives: the whine comes up fast with the first of the effort. Far
        // off or under louder sounds one inverter does, made as loud as the
        // two were together.
        let fine = smooth((detail - 0.35) / 0.3);
        let second = SECOND * fine;
        let whine = MOTOR_LEVEL * effort * (2.0 - effort) * ((1.0 + SECOND * SECOND) / (1.0 + second * second)).sqrt();
        let levels = [whine, whine * second];
        // Wheels creep and the drive hunts: the pitch is never quite still,
        // which also keeps two motors from holding a phase.
        let mut currents = [0.0; 2];
        for (k, hertz) in currents.iter_mut().enumerate() {
            let unsteady = 1.0 + 0.002 * self.drift[k].next().clamp(-3.0, 3.0);
            *hertz = (current * self.wear[k] * unsteady + SLIP * pull).max(0.0);
        }
        let mut goal = [0.0; 4];
        for (first, mode, share) in [(0, self.mode, coming), (2, self.from, going)] {
            if share == 0.0 {
                continue;
            }
            let pulses = self.ladder[mode];
            // The pulses are counted on the current's own turn: the tone
            // follows it, as a synchronous drive's does.
            let tone = |k: usize| if pulses.count > 0.0 { pulses.count * currents[k] } else { self.carrier[k] };
            let shape = pulses.shape(tone(0), self.casing);
            for k in 0..2 {
                self.step[first + k] = tone(k) * DT;
                self.growl[first + k] = pulses.growl;
                for (lane, coefficient) in self.shape.iter_mut().zip(shape) {
                    lane[first + k] = coefficient;
                }
                goal[first + k] = levels[k] * share;
            }
        }
        self.weight.aim(goal);
        self.beat_step = 3.0 * currents[0] * DT;

        // The gear whines with the wheels, louder the harder its teeth are
        // pressed; the turning wheel sways its pitch.
        self.wheel += speed / PI * CONTROL as f32 * DT;
        if self.wheel >= 1.0 {
            self.wheel -= 1.0;
        }
        let sway = 1.0 + 0.004 * arch(self.wheel);
        self.mesh_step = self.mesh_per_speed * speed * self.wear[0] * sway * DT;
        let ratio = speed / NOMINAL_SPEED;
        self.gear = MOTOR_LEVEL * GEAR * self.gear_share * (0.1 + 0.9 * effort) * ratio.min(1.5);
        self.awake = !(self.weight.idle() && self.gear == 0.0);
    }

    fn run(&mut self, out: &mut [f32]) {
        if !self.awake {
            out.fill(0.0);
            self.still = 0.0;
            return;
        }
        let whining = !self.weight.idle();
        let (step, shape, growl, weight_step) = (self.step, self.shape, self.growl, self.weight.step);
        let (mut turn, mut weight) = (self.turn, self.weight.now);
        let gear = self.gear;
        for sample in out {
            let mut sum = 0.0;
            if whining {
                self.beat += self.beat_step;
                if self.beat >= 1.0 {
                    self.beat -= 1.0;
                }
                // A triangle: the beat needs no better.
                let beat = 1.0 - 4.0 * (self.beat - 0.5).abs();
                let mut tones = [0.0; 4];
                for i in 0..4 {
                    turn[i] += step[i];
                    if turn[i] >= 1.0 {
                        turn[i] -= 1.0;
                    }
                    weight[i] += weight_step[i];
                    let s = arch(turn[i]);
                    let wave =
                        shape[0][i] + s * (shape[1][i] + s * (shape[2][i] + s * (shape[3][i] + s * shape[4][i])));
                    tones[i] = wave * (1.0 + growl[i] * beat) * weight[i];
                }
                sum = (tones[0] + tones[1]) + (tones[2] + tones[3]);
            }
            self.mesh += self.mesh_step;
            if self.mesh >= 1.0 {
                self.mesh -= 1.0;
            }
            let mesh = arch(self.mesh);
            // The mesh and its second harmonic; the square of an arch
            // averages 8/15, which is taken off.
            let made = sum + (mesh + 0.5 * (16.0 / 15.0 - 2.0 * mesh * mesh)) * gear;
            self.still += (made - self.still) * STILL;
            *sample = guard(made - self.still);
        }
        (self.turn, self.weight.now) = (turn, weight);
    }
}

impl Voice for Motor {
    fn render(&mut self, drive: &Drive, out: &mut [f32]) {
        if !self.awake && drive.speed == 0.0 && drive.load == 0.0 && drive.accel == 0.0 {
            // At rest and asked for nothing: a motor costs nothing.
            out.fill(0.0);
            self.until = 0;
            return;
        }
        let mut at = 0;
        while at < out.len() {
            if self.until == 0 {
                self.control(drive);
                self.until = CONTROL;
            }
            let n = self.until.min(out.len() - at);
            self.run(&mut out[at..at + n]);
            self.until -= n;
            at += n;
        }
    }
}

// ---------------------------------------------------------------- the joint

pub const JOINT: &str =
    "An expansion joint of the runway crossed by an axle's tyres: a soft low thump and a muted knock";
pub const JOINT_PARAMS: &[(&str, f32)] = &[("thump", 75.0), ("knock", 300.0), ("ring", 0.12)];

/// Crossings sounding at once.
const BLOWS: usize = 8;
/// A crossing is over this long after it began, s.
const GONE: f32 = 0.15;
/// Its last samples are faded, so that it ends on nothing.
const RELEASE: u32 = 480;
/// Sets a crossing's peak for a strength of 1; the test of the peak holds
/// it.
const JOINT_LEVEL: f32 = 0.603;
/// How much of the push reaches the knocks as noise: no two crossings alike.
const ROUGH: f32 = 0.15;

/// How much of a tone a raised cosine lasting `periods` of it lets through:
/// its spectrum. A push much longer than a period hardly rings it.
fn pushed(periods: f32) -> f32 {
    if (1.0 - periods).abs() < 1e-3 {
        return 0.5;
    }
    ((PI * periods).sin() / (PI * periods * (1.0 - periods * periods))).abs().max(0.05)
}

/// One crossing under way. Its parts — the thump, two knocks, and a place
/// left empty — each ring as `y[n] = c1·y[n−1] − c2·y[n−2]`, fed by the
/// push.
#[derive(Clone, Debug)]
struct Blow {
    noise: Noise,
    y1: Four,
    y2: Four,
    c1: Four,
    c2: Four,
    gain: Four,
    rough: Four,
    /// Samples until it begins, since it began, that the push lasts, and
    /// until it is over.
    wait: u32,
    age: u32,
    push: u32,
    left: u32,
    /// The push a sample ago.
    last: f32,
}

/// A joint of the runway.
#[derive(Clone, Debug)]
pub struct Joint {
    noise: Noise,
    blows: Vec<Blow>,
    thump: f32,
    knock: f32,
    ring: f32,
}

impl Joint {
    pub fn new(seed: u32, params: &Params) -> Self {
        Self {
            noise: Noise::new(seed),
            blows: Vec::with_capacity(BLOWS),
            thump: within(param(params, JOINT_PARAMS, "thump"), 30.0, 200.0),
            knock: within(param(params, JOINT_PARAMS, "knock"), 100.0, 600.0),
            ring: within(param(params, JOINT_PARAMS, "ring"), 0.03, 0.125),
        }
    }

    fn strike(&mut self, wait: usize, strength: f32, detail: f32) {
        let strength = within(strength, 0.0, 1.5);
        // A ninth crossing while eight sound is lost among them.
        if strength <= 0.0 || self.blows.len() == BLOWS {
            return;
        }
        let mut noise = self.noise.fork();
        // A tyre is soft: it leans on the gap for some 3 ms.
        let seconds = noise.between(0.002_6, 0.003_4);
        let push = (seconds / DT) as u32;
        // Hz, seconds to −60 dB, swing, roughness: no two crossings alike.
        let thump = (self.thump * noise.between(0.8, 1.2), self.ring * noise.between(0.85, 1.1), 0.8, 0.0);
        let low = (self.knock * noise.between(0.67, 0.97), noise.between(0.045, 0.07), 0.34, ROUGH);
        let high = (self.knock * noise.between(0.97, 1.33), noise.between(0.045, 0.07), 0.22, ROUGH);
        // Far off, one knock as strong as the two.
        let (count, low) = if detail < 0.5 { (2, (low.0, low.1, 0.405, ROUGH)) } else { (3, low) };
        let left = (push + (thump.1 * 1.1 / DT) as u32).min((GONE / DT) as u32);
        let mut blow = Blow {
            noise,
            y1: [0.0; 4],
            y2: [0.0; 4],
            c1: [0.0; 4],
            c2: [0.0; 4],
            gain: [0.0; 4],
            rough: [0.0; 4],
            wait: wait as u32,
            age: 0,
            push,
            left,
            last: 0.0,
        };
        for (i, (hertz, lasts, swing, rough)) in [thump, low, high].into_iter().take(count).enumerate() {
            let r = (-6.91 * DT / lasts).exp();
            (blow.c1[i], blow.c2[i], blow.rough[i]) = (2.0 * r * (TAU * hertz * DT).cos(), r * r, rough);
            // Fed the push's slope, a part swings to what the push lets
            // through of its tone: `gain` undoes that, and `swing` is left.
            blow.gain[i] = strength * JOINT_LEVEL * swing / pushed(hertz * seconds);
        }
        self.blows.push(blow);
    }
}

impl Voice for Joint {
    fn render(&mut self, drive: &Drive, out: &mut [f32]) {
        if let Some(hit) = drive.hit {
            // When in the block: one told for later is struck at its end.
            self.strike(hit.at.min(out.len().saturating_sub(1)), hit.strength, within(drive.detail, 0.0, 1.0));
        }
        if self.blows.is_empty() {
            out.fill(0.0);
            return;
        }
        for sample in out.iter_mut() {
            let mut sum = 0.0;
            for blow in &mut self.blows {
                if blow.wait > 0 {
                    blow.wait -= 1;
                    continue;
                }
                if blow.left == 0 {
                    continue;
                }
                let mut parts = [0.0; 4];
                if blow.age <= blow.push {
                    // The push is a raised cosine of unit area. The parts are
                    // fed its slope, which has no steady part: neither have
                    // they.
                    let push = if blow.age < blow.push {
                        (1.0 + arch(((blow.age as f32 + 0.5) / blow.push as f32 + 0.25).fract())) / blow.push as f32
                    } else {
                        0.0
                    };
                    let (slope, grit) = (push - blow.last, push * blow.noise.next());
                    blow.last = push;
                    for (i, part) in parts.iter_mut().enumerate() {
                        let y = blow.c1[i] * blow.y1[i] - blow.c2[i] * blow.y2[i] + slope + blow.rough[i] * grit;
                        blow.y2[i] = blow.y1[i];
                        blow.y1[i] = y;
                        *part = y * blow.gain[i];
                    }
                } else {
                    // Then they ring by themselves: most of a crossing.
                    for (i, part) in parts.iter_mut().enumerate() {
                        let y = blow.c1[i] * blow.y1[i] - blow.c2[i] * blow.y2[i];
                        blow.y2[i] = blow.y1[i];
                        blow.y1[i] = y;
                        *part = y * blow.gain[i];
                    }
                }
                let fade = blow.left.min(RELEASE) as f32 * (1.0 / RELEASE as f32);
                sum += ((parts[0] + parts[1]) + (parts[2] + parts[3])) * fade;
                blow.age += 1;
                blow.left -= 1;
            }
            *sample = guard(sum);
        }
        self.blows.retain(|blow| blow.left > 0);
    }
}

// ---------------------------------------------------------------- the span

pub const SPAN: &str =
    "A span of the riveted steel viaduct rumbling under the train: a few low modes that ring on after it, and a rattle";
pub const SPAN_PARAMS: &[(&str, f32)] = &[("tone", 1.0), ("ring", 1.0), ("rattle", 1.0)];

/// The modes, Hz, before the seed moves each by up to 6 %.
const MODES: [f32; 6] = [47.0, 63.0, 88.0, 121.0, 164.0, 215.0];
/// How far each swings beside the lowest.
const SWING: [f32; 6] = [1.0, 1.0, 0.9, 0.75, 0.6, 0.5];
/// Where each mode is among the eight resonators: the first four run
/// whatever the detail, so the three modes a far listener hears best are
/// there, with the floor.
const PLACE: [usize; 6] = [4, 0, 1, 2, 5, 6];
/// The floor: a resonator too wide to ring, its place, Hz, Q and swing
/// beside the lowest mode.
const FLOOR: (usize, f32, f32, f32) = (3, 105.0, 0.6, 1.6);
/// Sets the span's level at a load of 1 and 40 km/h; the test of the
/// nominal level holds it.
const SPAN_LEVEL: f32 = 0.0762;
/// The swing of the modes past which loose things rattle, and how loud.
const LOOSE: f32 = 0.07;
const RATTLE: f32 = 6.0;
/// The noise is rounded twice at this before the resonators are fed it, Hz:
/// a span has no hiss.
const ROUNDED: f32 = 200.0;

/// A span of the viaduct.
#[derive(Clone, Debug)]
pub struct Span {
    noise: Noise,
    rounded: [Low; 2],
    /// The rounded noise one and two samples ago: the resonators are fed
    /// the difference, which has nothing steady in it.
    fed: [f32; 2],
    /// The resonators — six modes, the floor, and a place left empty —
    /// each ringing as `y[n] = c1·y[n−1] − c2·y[n−2]`.
    y1: [f32; 8],
    y2: [f32; 8],
    c1: [f32; 8],
    c2: [f32; 8],
    /// What makes each one's RMS its share when it is fed noise of unit
    /// gain, and that times how much of it the detail keeps.
    weight: [f32; 8],
    heard: [f32; 8],
    /// Whether the last four run.
    all: bool,
    /// The rattle: what the first rounding took off, under 600 Hz.
    under: [Low; 2],
    load: Eased,
    speed: Eased,
    detail: Eased,
    /// The detail the shares were last set for.
    shared: f32,
    /// The gain now, where it goes in a straight line, and by how much a
    /// sample.
    gain: f32,
    gain_goal: f32,
    gain_step: f32,
    buzz: f32,
    rattle_share: f32,
    until: usize,
    awake: bool,
}

impl Span {
    pub fn new(seed: u32, params: &Params) -> Self {
        let mut noise = Noise::new(seed);
        let mut shape = noise.fork();
        let tone = within(param(params, SPAN_PARAMS, "tone"), 0.5, 2.0);
        let ring = within(param(params, SPAN_PARAMS, "ring"), 0.25, 2.0);
        let together = SWING.iter().map(|s| s * s).sum::<f32>().sqrt();
        let (mut c1, mut c2, mut weight) = ([0.0; 8], [0.0; 8], [0.0; 8]);
        let mut tune = |place: usize, hertz: f32, q: f32, swing: f32| {
            let (turn, r) = (f64::from(TAU * hertz * DT), f64::from((-PI * hertz * DT / q).exp()));
            let (a, b) = (2.0 * r * turn.cos(), r * r);
            // Its variance fed white noise of unit gain (a variance of a
            // third) less itself two samples before, by its first two
            // autocorrelations. Narrow as it is, a mode takes the rounding
            // of the noise as that at its pitch.
            let r0 = (1.0 + b) / ((1.0 - b) * ((1.0 + b).powi(2) - a * a));
            let rho2 = a * a / (1.0 + b) - b;
            let variance = 2.0 * r0 * (1.0 - rho2) / 3.0 * kept(ROUNDED, hertz).powi(2);
            (c1[place], c2[place], weight[place]) = (a as f32, b as f32, swing / together / variance.sqrt() as f32);
        };
        for i in 0..MODES.len() {
            let hertz = MODES[i] * tone * shape.between(0.94, 1.06);
            // The lowest two carry the ring-down: 0.4 s at least.
            let q = ring * shape.between(if i < 2 { 12.0 } else { 8.0 }, 20.0);
            tune(PLACE[i], hertz, q, SWING[i]);
        }
        tune(FLOOR.0, FLOOR.1 * tone * shape.between(0.95, 1.05), FLOOR.2, FLOOR.3);
        Self {
            rounded: [Low::at(ROUNDED), Low::at(ROUNDED)],
            fed: [0.0; 2],
            y1: [0.0; 8],
            y2: [0.0; 8],
            c1,
            c2,
            weight,
            heard: weight,
            all: true,
            under: [Low::at(600.0), Low::at(600.0)],
            load: Eased::new(0.0, 0.04),
            speed: Eased::new(0.0, 0.04),
            detail: Eased::new(1.0, 0.03),
            shared: 1.0,
            gain: 0.0,
            gain_goal: 0.0,
            gain_step: 0.0,
            buzz: 0.0,
            rattle_share: within(param(params, SPAN_PARAMS, "rattle"), 0.0, 2.0),
            until: 0,
            awake: false,
            noise,
        }
    }

    fn control(&mut self, drive: &Drive) {
        let load = self.load.follow(within(drive.load, 0.0, 1.0));
        let speed = self.speed.follow(within(drive.speed.abs(), 0.0, TOP_SPEED));
        let detail = self.detail.follow(within(drive.detail, 0.0, 1.0));
        // Bogies on the span add as powers; each shakes it as hard as it is
        // fast.
        self.gain = self.gain_goal;
        self.gain_goal = SPAN_LEVEL * load.sqrt() * speed / NOMINAL_SPEED;
        self.gain_step = (self.gain_goal - self.gain) / CONTROL as f32;
        let fine = smooth((detail - 0.35) / 0.3);
        self.buzz = RATTLE * self.rattle_share * fine;
        if detail != self.shared {
            // Fewer modes, the rest louder by what the missing ones gave.
            self.shared = detail;
            let part = |place: usize| if place < 4 { 1.0 } else { fine };
            let power = |part: &dyn Fn(usize) -> f32| {
                (0..MODES.len()).map(|i| (SWING[i] * part(PLACE[i])).powi(2)).sum::<f32>() + FLOOR.3 * FLOOR.3
            };
            let more = (power(&|_| 1.0) / power(&part)).sqrt();
            for place in 0..8 {
                self.heard[place] = self.weight[place] * part(place) * more;
            }
            self.all = fine > 0.0;
        }
        self.awake = true;
        if self.gain == 0.0 && self.gain_goal == 0.0 {
            // Nothing feeds the resonators: each rings down, is put to rest
            // once it swings by less than a hundred-thousandth, and when
            // all are at rest the span sleeps.
            self.awake = false;
            for i in 0..8 {
                let (y1, y2, c1) = (self.y1[i], self.y2[i], self.c1[i]);
                // What a swing of 1 leaves in two samples one after the
                // other.
                let least = 1e-10 * (1.0 - 0.25 * c1 * c1).max(1e-6);
                if y1 * y1 + y2 * y2 - c1 * y1 * y2 < least {
                    (self.y1[i], self.y2[i]) = (0.0, 0.0);
                } else {
                    self.awake = true;
                }
            }
            if !self.awake {
                self.fed = [0.0; 2];
            }
        }
    }

    fn run(&mut self, out: &mut [f32]) {
        if !self.awake {
            out.fill(0.0);
            return;
        }
        let (buzz, places) = (self.buzz, if self.all { 8 } else { 4 });
        let (c1, c2, heard) = (self.c1, self.c2, self.heard);
        let (mut y1, mut y2) = (self.y1, self.y2);
        for sample in out {
            self.gain += self.gain_step;
            let white = self.noise.next();
            let once = self.rounded[0].pass(white);
            let push = self.rounded[1].pass(once) * self.gain;
            let slope = push - self.fed[1];
            self.fed = [push, self.fed[0]];
            let mut sum = 0.0;
            for four in (0..places).step_by(4) {
                let mut parts = [0.0; 4];
                for (k, part) in parts.iter_mut().enumerate() {
                    let i = four + k;
                    let y = c1[i] * y1[i] - c2[i] * y2[i] + slope;
                    y2[i] = y1[i];
                    y1[i] = y;
                    *part = y * heard[i];
                }
                sum += (parts[0] + parts[1]) + (parts[2] + parts[3]);
            }
            if buzz > 0.0 {
                // Loose plates and rivets buzz on the peaks of the swing.
                let grit = self.under[0].pass(white - once);
                sum += buzz * (sum.abs() - LOOSE).max(0.0) * self.under[1].pass(grit);
            }
            *sample = guard(sum);
        }
        (self.y1, self.y2) = (y1, y2);
    }
}

impl Voice for Span {
    fn render(&mut self, drive: &Drive, out: &mut [f32]) {
        if !self.awake && (drive.load == 0.0 || drive.speed == 0.0) {
            // Rung down and asked for nothing: a span costs nothing.
            out.fill(0.0);
            self.until = 0;
            return;
        }
        let mut at = 0;
        while at < out.len() {
            if self.until == 0 {
                self.control(drive);
                self.until = CONTROL;
            }
            let n = self.until.min(out.len() - at);
            self.run(&mut out[at..at + n]);
            self.until -= n;
            at += n;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Hit, NOMINAL_RMS, RATE};

    const SECOND: usize = RATE as usize;

    /// A drive at `kmh` with `load`.
    fn at(kmh: f32, load: f32) -> Drive {
        Drive { speed: kmh / 3.6, load, ..Drive::default() }
    }

    /// `seconds` of a voice following `drive(t)`, in blocks of `block`.
    fn follow(voice: &mut dyn Voice, seconds: f32, block: usize, mut drive: impl FnMut(f32) -> Drive) -> Vec<f32> {
        let mut out = vec![0.0; (seconds * RATE as f32) as usize];
        let mut done = 0;
        for chunk in out.chunks_mut(block) {
            voice.render(&drive(done as f32 * DT), chunk);
            done += chunk.len();
        }
        out
    }

    /// `seconds` of a voice under one drive, once it has settled for a
    /// second.
    fn steady(voice: &mut dyn Voice, seconds: f32, drive: Drive) -> Vec<f32> {
        follow(voice, seconds + 1.0, 128, |_| drive).split_off(SECOND)
    }

    fn bogie(seed: u32) -> Bogie {
        Bogie::new(seed, &Params::default())
    }

    fn motor(seed: u32) -> Motor {
        Motor::new(seed, &Params::default())
    }

    fn span(seed: u32) -> Span {
        Span::new(seed, &Params::default())
    }

    /// A joint crossed `rate` times a second for `seconds`.
    fn crossed(seed: u32, seconds: f32, rate: f32, strength: f32) -> Vec<f32> {
        let mut voice = Joint::new(seed, &Params::default());
        let every = (RATE as f32 / rate / 128.0) as usize;
        let mut block = 0;
        follow(&mut voice, seconds, 128, |_| {
            block += 1;
            Drive { hit: (block % every == 1).then_some(Hit { at: 5, strength }), ..Drive::default() }
        })
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / samples.len().max(1) as f64).sqrt() as f32
    }

    fn mean(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| f64::from(*s)).sum::<f64>() / samples.len().max(1) as f64) as f32
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0, |m, s| m.max(s.abs()))
    }

    /// Decibels of `a` over `b`.
    fn over(a: f32, b: f32) -> f32 {
        20.0 * (a.max(1e-12) / b.max(1e-12)).log10()
    }

    /// The transform of `re + i·im`, in place; their length a power of two.
    fn transform(re: &mut [f64], im: &mut [f64]) {
        let n = re.len();
        let mut j = 0;
        for i in 1..n {
            let mut bit = n >> 1;
            while j & bit != 0 {
                j ^= bit;
                bit >>= 1;
            }
            j ^= bit;
            if i < j {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= n {
            let turn = -std::f64::consts::TAU / len as f64;
            for start in (0..n).step_by(len) {
                for k in 0..len / 2 {
                    let (c, s) = ((turn * k as f64).cos(), (turn * k as f64).sin());
                    let (a, b) = (start + k, start + k + len / 2);
                    let (x, y) = (re[b] * c - im[b] * s, re[b] * s + im[b] * c);
                    (re[b], im[b]) = (re[a] - x, im[a] - y);
                    (re[a], im[a]) = (re[a] + x, im[a] + y);
                }
            }
            len *= 2;
        }
    }

    /// The power in each of `size / 2` bands of `RATE / size` Hz, averaged
    /// over windows half overlapping (Welch); the whole adds up to the mean
    /// square.
    fn spectrum(samples: &[f32], size: usize) -> Vec<f64> {
        let window: Vec<f64> =
            (0..size).map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / size as f64).cos()).collect();
        let scale = 2.0 / (window.iter().map(|w| w * w).sum::<f64>() * size as f64);
        let mut power = vec![0.0; size / 2];
        let mut windows = 0;
        for piece in samples.windows(size).step_by(size / 2) {
            let mut re: Vec<f64> = piece.iter().zip(&window).map(|(s, w)| f64::from(*s) * w).collect();
            let mut im = vec![0.0; size];
            transform(&mut re, &mut im);
            for (k, p) in power.iter_mut().enumerate() {
                *p += (re[k] * re[k] + im[k] * im[k]) * scale;
            }
            windows += 1;
        }
        power.iter().map(|p| p / f64::from(windows.max(1))).collect()
    }

    /// The power between `low` and `high` Hz of a spectrum.
    fn band(power: &[f64], low: f32, high: f32) -> f64 {
        let width = f64::from(RATE) / (2 * power.len()) as f64;
        power
            .iter()
            .enumerate()
            .filter(|(k, _)| (f64::from(low)..f64::from(high)).contains(&(*k as f64 * width)))
            .map(|(_, p)| *p)
            .sum()
    }

    /// The level of each octave from 63 Hz to 8 kHz, dB.
    fn octaves(samples: &[f32]) -> [f32; 8] {
        let power = spectrum(samples, 8_192);
        let mut centre = 62.5;
        std::array::from_fn(|_| {
            let level = 10.0 * band(&power, centre / 2f32.sqrt(), centre * 2f32.sqrt()).max(1e-30).log10();
            centre *= 2.0;
            level as f32
        })
    }

    /// The strongest tone between `low` and `high`, Hz.
    fn tone(samples: &[f32], low: f32, high: f32) -> f32 {
        let size = 32_768;
        let power = spectrum(samples, size);
        let width = RATE as f32 / size as f32;
        let (from, to) = ((low / width) as usize, (high / width) as usize);
        let k = (from..to).max_by(|a, b| power[*a].total_cmp(&power[*b])).unwrap();
        // Between the bands, by the powers either side.
        let (below, here, above) = (power[k - 1].ln(), power[k].ln(), power[k + 1].ln());
        (k as f32 + (0.5 * (below - above) / (below - 2.0 * here + above)) as f32) * width
    }

    /// CNOSSOS-EU 2015, rolling noise of category 3 (heavy vehicles): the
    /// level of each octave from 63 Hz to 8 kHz at `kmh`, dB.
    fn rolling(kmh: f32) -> [f32; 8] {
        const A: [f32; 8] = [87.0, 91.7, 94.1, 100.7, 100.8, 94.3, 87.1, 82.5];
        const B: [f32; 8] = [30.0, 33.5, 31.3, 25.4, 31.8, 37.1, 38.6, 40.6];
        std::array::from_fn(|i| A[i] + B[i] * (kmh / 70.0).log10())
    }

    // --------------------------------------------------------------- tests

    /// The mean over seeds 1 to 6 of what `make(seed)` gives under `drive`, dB over the nominal RMS.
    fn level(make: impl Fn(u32) -> Box<dyn Voice>, drive: Drive) -> f32 {
        let levels: Vec<f32> =
            (1..=6).map(|seed| over(rms(&steady(&mut *make(seed), 10.0, drive)), NOMINAL_RMS)).collect();
        levels.iter().sum::<f32>() / 6.0
    }

    #[test]
    fn each_voice_is_at_the_nominal_level_where_its_documentation_says() {
        let bogie = level(|seed| Box::new(bogie(seed)), at(40.0, 0.0));
        let motor = level(|seed| Box::new(motor(seed)), at(40.0, 0.5));
        let span = level(|seed| Box::new(span(seed)), at(40.0, 1.0));
        eprintln!("bogie {bogie:.2} dB, motor {motor:.2} dB, span {span:.2} dB");
        for (name, db) in [("bogie", bogie), ("motor", motor), ("span", span)] {
            assert!(db.abs() < 0.5, "{name}: {db:.2} dB from the nominal level");
        }
        // A joint crossed at full strength peaks at about a half.
        let peaks: Vec<f32> = (0..40).map(|seed| peak(&crossed(seed, 0.3, 1.0, 1.0))).collect();
        let mean_peak = peaks.iter().sum::<f32>() / 40.0;
        assert!((mean_peak - 0.5).abs() < 0.05, "{mean_peak}");
    }

    #[test]
    fn the_motor_pushes_no_way_for_long() {
        // Its mean against its RMS, at speeds and efforts across its range.
        for (kmh, effort) in [(5.0, 0.5), (30.0, 0.5), (40.0, 0.5), (70.0, 0.5), (30.0, 1.0), (0.0, 0.7)] {
            let out = steady(&mut motor(1), 20.0, at(kmh, effort));
            assert!(
                mean(&out).abs() < 0.005 * rms(&out).max(1e-6),
                "{kmh} km/h, effort {effort}: mean {} rms {}",
                mean(&out),
                rms(&out)
            );
        }
    }

    #[test]
    fn at_rest_each_falls_silent_and_the_length_of_the_blocks_changes_nothing() {
        // Running, then stopped: the bogie and the span go silent, to the sample.
        let mut going = bogie(2);
        let out = follow(&mut going, 8.0, 128, |t| if t < 4.0 { at(40.0, 0.0) } else { at(0.0, 0.0) });
        assert!(out[7 * SECOND..].iter().all(|s| *s == 0.0));
        let mut loaded = span(2);
        let out = follow(&mut loaded, 12.0, 128, |t| if t < 4.0 { at(40.0, 1.0) } else { at(40.0, 0.0) });
        assert!(out[11 * SECOND..].iter().all(|s| *s == 0.0));
        let mut idle = motor(2);
        assert!(steady(&mut idle, 2.0, at(0.0, 0.0)).iter().all(|s| *s == 0.0));
        // Blocks of 7 or of 4 096 give the samples of blocks of 128.
        for (name, make, drive) in [
            (
                "bogie",
                Box::new(|| Box::new(bogie(3)) as Box<dyn Voice>) as Box<dyn Fn() -> Box<dyn Voice>>,
                at(40.0, 0.0),
            ),
            ("motor", Box::new(|| Box::new(motor(3)) as Box<dyn Voice>), at(30.0, 0.6)),
            ("span", Box::new(|| Box::new(span(3)) as Box<dyn Voice>), at(40.0, 1.0)),
        ] {
            let base = follow(&mut *make(), 2.0, 128, |_| drive);
            for block in [7, 4_096] {
                assert!(follow(&mut *make(), 2.0, block, |_| drive) == base, "{name} in blocks of {block}");
            }
        }
    }

    #[test]
    fn a_joint_told_of_a_blow_past_its_block_strikes_at_its_end_and_never_locks() {
        // Eight blows told for far past the block, then one a second: each of those is heard.
        let mut voice = Joint::new(1, &Params::default());
        let mut block = 0;
        let out = follow(&mut voice, 3.0, 128, |_| {
            block += 1;
            let hit = match block {
                1..=8 => Some(Hit { at: 96_000, strength: 1.0 }),
                b if b % 375 == 0 => Some(Hit { at: 5, strength: 1.0 }),
                _ => None,
            };
            Drive { hit, ..Drive::default() }
        });
        assert!(peak(&out[..1_024 + 128]) > 0.1, "the eight sound at once");
        for second in 1..3 {
            let from = second * SECOND;
            assert!(peak(&out[from..from + 4_800]) > 0.1, "the blow at {second} s");
        }
    }

    // ------------------------------------------------------------ measures

    /// Prints what the constants above are set from; not a test.
    #[test]
    #[ignore = "prints measurements"]
    fn measures() {
        for kmh in [20.0, 40.0, 70.0] {
            let tyres = steady(
                &mut Bogie::new(3, &Params::default().with("body", 0.0).with("rattle", 0.0)),
                30.0,
                at(kmh, 0.0),
            );
            let all = steady(&mut bogie(3), 30.0, at(kmh, 0.0));
            let (got, full, want) = (octaves(&tyres), octaves(&all), rolling(kmh));
            let rel = |o: &[f32; 8]| o.map(|x| ((x - o[3]) * 10.0).round() / 10.0);
            println!("bogie {kmh}: rms tyres {:.4} all {:.4} peak {:.3}", rms(&tyres), rms(&all), peak(&all));
            println!("  tyres {:?}\n  all   {:?}\n  table {:?}", rel(&got), rel(&full), rel(&want));
        }
        for seed in 1..=6 {
            let b = steady(&mut bogie(seed), 20.0, at(40.0, 0.0));
            let m = steady(&mut motor(seed), 20.0, at(40.0, 0.5));
            let s = steady(&mut span(seed), 30.0, at(40.0, 1.0));
            println!(
                "seed {seed}: bogie {:.4} motor {:.4} (peak {:.3}) span {:.4} (peak {:.3})",
                rms(&b),
                rms(&m),
                peak(&m),
                rms(&s),
                peak(&s)
            );
        }
        let peaks: Vec<f32> = (0..40).map(|seed| peak(&crossed(seed, 0.3, 1.0, 1.0))).collect();
        println!("joint peaks: mean {:.3} {:?}", peaks.iter().sum::<f32>() / 40.0, peaks);
        let s = steady(&mut span(2), 60.0, at(40.0, 1.0));
        let p = spectrum(&s, 8_192);
        let total = band(&p, 0.0, 24_000.0);
        for (low, high) in [(0.0, 35.0), (35.0, 280.0), (280.0, 650.0), (650.0, 1_000.0), (1_000.0, 24_000.0)] {
            println!("span {low}-{high}: {:.2} %", 100.0 * band(&p, low, high) / total);
        }
        println!("span octaves {:?}", octaves(&s));
        let quiet = steady(&mut Span::new(2, &Params::default().with("rattle", 0.0)), 60.0, at(40.0, 1.0));
        println!("span without rattle {:.4} with {:.4}", rms(&quiet), rms(&s));
        let m = steady(&mut motor(1), 20.0, at(40.0, 0.5));
        println!("motor octaves {:?}", octaves(&m));
        println!(
            "motor coasting {:.1} dB, tone {:.1}",
            over(rms(&steady(&mut motor(1), 10.0, at(40.0, 0.0))), rms(&m)),
            tone(&m, 150.0, 600.0)
        );
        let j = crossed(1, 10.0, 2.0, 1.0);
        let p = spectrum(&j, 8_192);
        let total = band(&p, 0.0, 24_000.0);
        for (low, high) in
            [(0.0, 40.0), (40.0, 120.0), (120.0, 180.0), (180.0, 450.0), (450.0, 1_000.0), (1_000.0, 24_000.0)]
        {
            println!("joint {low}-{high}: {:.2} %", 100.0 * band(&p, low, high) / total);
        }
        println!("joint mean {:.5} rms {:.4}", mean(&j), rms(&j));
    }

    /// Prints nanoseconds a sample of each voice; not a test.
    #[test]
    #[ignore = "prints timings"]
    fn costs() {
        // Ten seconds of sound, timed as a whole and a tenth of a second at
        // a time: on a machine busy with other work the quietest tenth says
        // what the voice costs, the whole what it cost this time.
        let time = |name: &str, voice: &mut dyn Voice, drive: &dyn Fn(usize) -> Drive| {
            let mut out = vec![0.0f32; SECOND * 10];
            let (mut whole, mut best, mut block) = (0.0f64, f64::MAX, 0);
            for tenth in out.chunks_mut(4_800) {
                let start = std::time::Instant::now();
                for piece in tenth.chunks_mut(128) {
                    voice.render(&drive(block), piece);
                    block += 1;
                }
                let took = start.elapsed().as_secs_f64();
                whole += took;
                best = best.min(took / tenth.len() as f64);
            }
            println!(
                "{name}: {:.1} ns a sample over 10 s, {:.1} in its quietest tenth of a second (rms {:.3})",
                whole * 1e9 / out.len() as f64,
                best * 1e9,
                rms(&out)
            );
        };
        let low = |drive: Drive| Drive { detail: 0.0, ..drive };
        time("bogie, 40 km/h", &mut bogie(1), &|_| at(40.0, 0.0));
        time("bogie, 40 km/h, no detail", &mut bogie(1), &|_| low(at(40.0, 0.0)));
        time("bogie, at rest", &mut bogie(1), &|_| at(0.0, 0.0));
        time("motor, 40 km/h, effort 0.5", &mut motor(1), &|_| at(40.0, 0.5));
        time("motor, 40 km/h, effort 0.5, no detail", &mut motor(1), &|_| low(at(40.0, 0.5)));
        time("motor, speeding up and slowing down, effort 1", &mut motor(1), &|i| {
            at(35.0 * (1.0 + (i as f32 * 0.002).sin()), 1.0)
        });
        time("motor, at rest", &mut motor(1), &|_| at(0.0, 0.0));
        let hit = |every: usize, i: usize| i.is_multiple_of(every).then_some(Hit { at: 3, strength: 1.0 });
        time("joint, 8 crossings a second", &mut Joint::new(1, &Params::default()), &|i| Drive {
            hit: hit(47, i),
            ..Drive::default()
        });
        time("joint, 8 sounding at all times", &mut Joint::new(1, &Params::default()), &|i| Drive {
            hit: hit(2, i),
            ..Drive::default()
        });
        time("joint, none", &mut Joint::new(1, &Params::default()), &|_| Drive::default());
        time("span, 40 km/h, load 1", &mut span(1), &|_| at(40.0, 1.0));
        time("span, 40 km/h, load 1, no detail", &mut span(1), &|_| low(at(40.0, 1.0)));
        time("span, no load", &mut span(1), &|_| at(40.0, 0.0));
    }
}
