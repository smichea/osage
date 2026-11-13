// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A kart's sound, synthesised from its state ([`Drive`]) sample by sample:
//!
//! - **engine:** a single-cylinder two-stroke fires once a turn; each firing
//!   is a burst that rings the exhaust's resonances, rougher and brighter
//!   under throttle; off the throttle it misfires now and then, as these
//!   engines do;
//! - **tyres:** squeal, a narrow band of noise whose pitch rises with the
//!   slide;
//! - **ground:** kerbs drum at the rate their blocks pass, grass rumbles;
//! - **wind,** rising with the square of the speed;
//! - **knocks:** a thump and the crack of plastic, as strong as the impulse;
//! - **cues** of paddock objects ([`Cue`], D-052): a bumper's air bag, a
//!   pit-crew push;
//! - and, apart from any kart, a drone's rotors ([`Buzz`]).
//!
//! Used where no recording is at hand (D-044's rule for sound: recordings
//! when the content is there, this otherwise), and for what recordings
//! cannot follow, the revs. Deterministic: a seeded noise, no clock.

use std::{
    f32::consts::{PI, TAU},
    sync::Arc,
};

/// Samples a second.
pub const RATE: u32 = 48_000;
const DT: f32 = 1.0 / RATE as f32;

/// Engine speeds of a 125 cc two-stroke kart engine, turns a minute.
pub const IDLE_RPM: f32 = 2_800.0;
pub const MAX_RPM: f32 = 14_500.0;
/// A kart's centrifugal clutch bites at about this engine speed.
const CLUTCH_RPM: f32 = 4_500.0;
/// The engine speed at the kart's top speed: below the limiter, where a
/// two-stroke still sounds full rather than strained. A boost takes it on
/// towards [`MAX_RPM`].
pub const CRUISE_RPM: f32 = 11_800.0;
/// The fastest a recorded loop is played, times its own speed: faster, a
/// recording no longer sounds like an engine but a tape sped up.
const FASTEST_TAPE: f32 = 1.25;
/// Kerb blocks pass the wheel every this many metres.
const KERB_BLOCK: f32 = 1.0;

/// What a kart's sound follows.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Drive {
    /// Engine speed, 0 at idle to 1 at the limiter ([`revs`]).
    pub revs: f32,
    /// Throttle, 0…1.
    pub throttle: f32,
    /// How much the tyres slide, 0…1.
    pub slip: f32,
    /// What the wheels run on: 0 asphalt, 0.6 kerbs, 1 grass.
    pub ground: f32,
    /// Speed, m/s.
    pub speed: f32,
}

/// Engine speed, 0…1, of a kart at `speed` whose top speed is `top`: no
/// gearbox, so the engine turns with the wheels above the clutch's speed —
/// [`CRUISE_RPM`] at top speed, on towards the limiter in a boost; below
/// the clutch, it revs with the throttle.
pub fn revs(speed: f32, top: f32, throttle: f32) -> f32 {
    let ratio = speed.abs() / top.max(1.0);
    let geared = if ratio <= 1.0 {
        CLUTCH_RPM + (CRUISE_RPM - CLUTCH_RPM) * ratio
    } else {
        CRUISE_RPM + (MAX_RPM - CRUISE_RPM) * ((ratio - 1.0) / 0.3).min(1.0)
    };
    let free = IDLE_RPM + (CLUTCH_RPM + 1_500.0 - IDLE_RPM) * throttle.clamp(0.0, 1.0);
    let rpm = if speed.abs() < 3.0 { free.max(geared * speed.abs() / 3.0) } else { geared };
    ((rpm - IDLE_RPM) / (MAX_RPM - IDLE_RPM)).clamp(0.0, 1.0)
}

/// A small, seeded noise: xorshift.
#[derive(Clone, Debug)]
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / (1u32 << 23) as f32 - 1.0
    }
}

/// Samples between two updates of what changes slowly: eased controls,
/// filter coefficients, gains. Computing them every sample cost most of a
/// voice's time, in the audio callback.
const BLOCK: u32 = 32;

/// A soft limiter, cheaper than `tanh` and as smooth: odd, 1 at 3 and beyond.
fn soft(x: f32) -> f32 {
    let x = x.clamp(-3.0, 3.0);
    x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
}

/// A resonant band-pass: a state-variable filter (topology preserving).
#[derive(Clone, Debug, Default)]
struct Band {
    ic1: f32,
    ic2: f32,
    g: f32,
    a1: f32,
}

impl Band {
    fn tune(&mut self, frequency: f32, q: f32) {
        self.g = (PI * frequency.clamp(20.0, RATE as f32 * 0.45) * DT).tan();
        self.a1 = 1.0 / (1.0 + self.g * (self.g + 1.0 / q));
    }

    fn pass(&mut self, x: f32) -> f32 {
        let v3 = x - self.ic2;
        let v1 = self.a1 * (self.ic1 + self.g * v3);
        let v2 = self.ic2 + self.g * v1;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        v1
    }
}

/// A one-pole low-pass.
#[derive(Clone, Debug, Default)]
struct Low {
    y: f32,
    a: f32,
}

impl Low {
    fn tune(&mut self, frequency: f32) {
        self.a = 1.0 - (-TAU * frequency * DT).exp();
    }

    fn pass(&mut self, x: f32) -> f32 {
        self.y += (x - self.y) * self.a;
        self.y
    }
}

/// Recorded sounds a voice plays instead of synthesising them, mono at
/// [`RATE`], loops seamless. Whatever is missing is synthesised.
#[derive(Clone, Default)]
pub struct Recordings {
    /// Engine loops and the engine speed each was recorded at, turns a
    /// minute, slowest first: the voice crossfades between the two nearest
    /// and plays them faster or slower to follow the revs.
    pub engine: Vec<(Arc<[f32]>, f32)>,
    /// Tyres sliding, a loop.
    pub squeal: Option<Arc<[f32]>>,
    /// Wheels on grass and gravel, a loop.
    pub ground: Option<Arc<[f32]>>,
    /// Knocks, each played once; a knock picks one.
    pub knocks: Vec<Arc<[f32]>>,
}

impl std::fmt::Debug for Recordings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rpm: Vec<f32> = self.engine.iter().map(|(_, rpm)| *rpm).collect();
        f.debug_struct("Recordings")
            .field("engine_rpm", &rpm)
            .field("squeal", &self.squeal.is_some())
            .field("ground", &self.ground.is_some())
            .field("knocks", &self.knocks.len())
            .finish()
    }
}

/// Grains of a loop, each this many samples (120 ms), two at a time.
const GRAIN: f32 = 0.12 * RATE as f32;

/// A loop played as grains (D-070): short pieces taken from anywhere in
/// it, each faded in and out, overlapping by half at equal power. A loop
/// of an engine a second long, played as it is, repeats every second —
/// a pattern the ear finds and will not let go of; played as grains, it
/// keeps its sound and loses its period.
#[derive(Clone, Debug)]
struct Grains {
    /// Where each grain is in the loop, and how far through itself.
    grains: [(f32, f32); 2],
    noise: Noise,
}

impl Grains {
    fn new(seed: u32, clip: &[f32]) -> Self {
        let mut noise = Noise(seed | 1);
        let mut at = || ((noise.next() * 0.5 + 0.5) * clip.len() as f32).floor();
        let (a, b) = (at(), at());
        Self { grains: [(a, 0.0), (b, GRAIN / 2.0)], noise }
    }

    /// The next sample of `clip` at `rate` times its speed.
    fn play(&mut self, clip: &[f32], rate: f32) -> f32 {
        let n = clip.len();
        if n < 2 {
            return 0.0;
        }
        let mut s = 0.0;
        for (at, age) in &mut self.grains {
            if *age >= GRAIN {
                // A new grain, from anywhere in the loop.
                // On a sample: interpolating a noise at a fixed fraction
                // would take some of its power.
                *at = ((self.noise.next() * 0.5 + 0.5) * n as f32).floor();
                *age -= GRAIN;
            }
            let i = *at as usize;
            let f = *at - i as f32;
            let x = clip[i % n] * (1.0 - f) + clip[(i + 1) % n] * f;
            // A half sine: two grains half a grain apart keep the power.
            s += x * (PI * *age / GRAIN).sin();
            *at = (*at + rate) % n as f32;
            *age += 1.0;
        }
        s
    }
}

/// A loop played at a varying speed.
#[derive(Clone, Debug, Default)]
struct Tape {
    at: f32,
}

impl Tape {
    /// The next sample of `clip` at `rate` times its speed, interpolated.
    fn play(&mut self, clip: &[f32], rate: f32) -> f32 {
        let n = clip.len();
        if n < 2 {
            return 0.0;
        }
        let i = self.at as usize;
        let f = self.at - i as f32;
        let s = clip[i % n] * (1.0 - f) + clip[(i + 1) % n] * f;
        self.at += rate;
        if self.at >= n as f32 {
            self.at -= n as f32;
        }
        s
    }
}

/// Loudness of the recorded layers, to match the synthesised ones (measured):
/// the clips are made at one loudness, loops at an RMS of 0.1, knocks at a
/// peak of 0.8 (tools/make-sounds.py).
const TAPE_ENGINE: f32 = 3.0;
const TAPE_SQUEAL: f32 = 5.0;
const TAPE_GROUND: f32 = 0.7;
const TAPE_KNOCK: f32 = 0.9;

/// The engine.
#[derive(Clone, Debug)]
struct Engine {
    phase: f32,
    jitter: f32,
    /// What is left of the last firing.
    fire: f32,
    /// How much of a firing is left after a sample.
    decay: f32,
    noise: Noise,
    pipe: Band,
    bark: Band,
    body: Low,
    // Set every block.
    step: f32,
    load: f32,
    misfire: f32,
    /// Recorded: the loops' tapes, and the two playing — which, how loud,
    /// how fast.
    tapes: Vec<Grains>,
    playing: [(usize, f32, f32); 2],
    /// Takes off the brightness a loop gains when played faster, so that
    /// its timbre stays an engine's.
    tone: Low,
    /// A slow sway of the revs, as a real engine never holds a speed
    /// exactly: its phase, turns.
    sway: f32,
}

impl Engine {
    fn control(&mut self, d: &Drive, recorded: &[(Arc<[f32]>, f32)]) {
        // Under way, the revs sway by up to 1 % (two slow waves that never
        // line up): a held note is what tires the ear.
        self.sway = (self.sway + BLOCK as f32 * DT).fract();
        let under_way = (d.speed / 10.0).clamp(0.0, 1.0);
        let wave = (TAU * 0.7 * self.sway).sin() * 0.6 + (TAU * 1.9 * self.sway + 1.3).sin() * 0.4;
        let rpm = (IDLE_RPM + (MAX_RPM - IDLE_RPM) * d.revs) * (1.0 + 0.01 * under_way * wave);
        self.step = rpm / 60.0 * DT;
        self.load = 0.35 + 0.65 * d.throttle;
        self.misfire = (1.0 - d.throttle) * 0.3 * (0.3 + d.revs);
        self.pipe.tune(380.0 + 260.0 * d.revs, 3.5);
        self.bark.tune(1_400.0 + 900.0 * d.revs, 2.5);
        if recorded.is_empty() {
            return;
        }
        // The two loops either side of the revs, crossfaded at equal power
        // on a scale of ratios.
        let i = recorded.iter().rposition(|(_, at)| *at <= rpm).unwrap_or(0);
        let j = (i + 1).min(recorded.len() - 1);
        let t = if i == j || recorded[i].1 > rpm {
            0.0
        } else {
            ((rpm / recorded[i].1).ln() / (recorded[j].1 / recorded[i].1).ln()).clamp(0.0, 1.0)
        };
        let rate = |k: usize| (rpm / recorded[k].1).clamp(0.5, FASTEST_TAPE);
        let (a, b) = ((t * PI / 2.0).cos(), (t * PI / 2.0).sin());
        self.playing = [(i, a, rate(i)), (j, if i == j { 0.0 } else { b }, rate(j))];
        // Played faster, a loop's spectrum moves up with it: a low-pass that
        // moves down as much keeps it from turning into a whine.
        let fastest = rate(i).max(if i == j { 0.0 } else { rate(j) });
        self.tone.tune((7_000.0 / fastest.max(1.0).powi(2)).clamp(2_500.0, 12_000.0));
    }

    fn sample(&mut self, recorded: &[(Arc<[f32]>, f32)]) -> f32 {
        if !recorded.is_empty() {
            let mut s = 0.0;
            let [(i, a, ra), (j, b, rb)] = self.playing;
            // Keep both tapes turning, so that a loop fades in where it
            // should be — each once, when both are the same.
            s += self.tapes[i].play(&recorded[i].0, ra) * a;
            if j != i {
                s += self.tapes[j].play(&recorded[j].0, rb) * b;
            }
            return self.tone.pass(s) * TAPE_ENGINE * (0.4 + 0.6 * self.load);
        }
        self.phase += self.step * (1.0 + self.jitter);
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            self.jitter = self.noise.next() * 0.025;
            // Off the throttle a two-stroke skips firings.
            if self.noise.next() * 0.5 + 0.5 >= self.misfire {
                self.fire = 1.0;
            }
        }
        self.fire *= self.decay;
        let load = self.load;
        let burst = self.fire * (0.7 + 0.3 * load) + self.noise.next() * self.fire * (0.2 + 0.5 * load);
        let pipe = self.pipe.pass(burst);
        let bark = self.bark.pass(burst) * (0.25 + 0.5 * load);
        let body = self.body.pass(burst);
        soft((pipe * 1.6 + bark + body * 1.2) * (1.2 + 1.8 * load)) * (0.32 + 0.2 * load)
    }
}

/// A paddock object's sound, from the kart that uses it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    /// A bumper's air bag: a deep thump and a rush of air, 0.4 s.
    Bumper,
    /// A pit-crew push: air rising in pitch as the kart is shoved, 0.7 s.
    Push,
}

impl Cue {
    /// How long it sounds, s.
    pub fn length(self) -> f32 {
        match self {
            Cue::Bumper => 0.4,
            Cue::Push => 0.7,
        }
    }
}

/// A drone's four rotors, each its own pitch, and the air they move: a
/// buzz that rises with `gain` (0 silent, 1 close and flying).
#[derive(Clone, Debug)]
pub struct Buzz {
    phases: [f32; 4],
    noise: Noise,
    air: Band,
    wobble: f32,
}

impl Buzz {
    pub fn new(seed: u32) -> Self {
        let mut air = Band::default();
        air.tune(1_400.0, 1.2);
        Self { phases: [0.0, 0.25, 0.5, 0.75], noise: Noise(seed.wrapping_mul(0x9e37_79b9) | 1), air, wobble: 0.0 }
    }

    /// The next sample, about −1…1, at `gain`.
    pub fn sample(&mut self, gain: f32) -> f32 {
        // Blade passes: two blades a rotor, a little apart in speed.
        const RATES: [f32; 4] = [172.0, 178.5, 184.0, 191.5];
        self.wobble = (self.wobble + DT * 3.1).fract();
        let sway = 1.0 + 0.015 * (TAU * self.wobble).sin();
        let mut hum = 0.0;
        for (phase, rate) in self.phases.iter_mut().zip(RATES) {
            *phase = (*phase + rate * sway * DT).fract();
            // A saw with its edge softened: the blade's pulse.
            hum += (2.0 * *phase - 1.0) * 0.6 + (TAU * 2.0 * *phase).sin() * 0.25;
        }
        let air = self.air.pass(self.noise.next());
        soft((hum * 0.12 + air * 0.5) * gain.clamp(0.0, 1.0))
    }
}

/// A knock under way.
#[derive(Clone, Debug)]
struct Knock {
    age: f32,
    strength: f32,
    /// Recorded: which clip, and how far into it.
    clip: usize,
    at: usize,
}

/// One kart's sound.
#[derive(Clone, Debug)]
pub struct Voice {
    engine: Engine,
    /// The drive it follows, and the one it has eased to (no clicks).
    wanted: Drive,
    now: Drive,
    noise: Noise,
    squeal: Band,
    drum: Low,
    grass: Low,
    wind: Low,
    crack: Band,
    kerb: f32,
    knocks: Vec<Knock>,
    /// Cues sounding, with their age, s.
    cues: Vec<(Cue, f32)>,
    rush: Band,
    count: u32,
    /// Easing over a block.
    ease: f32,
    // Set every block.
    gains: Gains,
    recordings: Arc<Recordings>,
    squeal_tape: Tape,
    ground_tape: Tape,
    knocked: u32,
    /// The revs, slowly followed: what the engine has held.
    held_revs: f32,
    /// How long it has held them, 0…1.
    calm: f32,
}

/// Gains and rates, set every block.
#[derive(Clone, Debug, Default)]
struct Gains {
    /// The engine's level: a little lower when its note has held a while.
    engine: f32,
    squeal: f32,
    squeal_rate: f32,
    drum: f32,
    grass: f32,
    ground_rate: f32,
    wind: f32,
    kerb_step: f32,
}

impl Voice {
    /// A voice; `seed` keeps karts from sounding alike.
    pub fn new(seed: u32) -> Self {
        Self::with(seed, Arc::default())
    }

    /// A voice that plays `recordings` where it has them.
    pub fn with(seed: u32, recordings: Arc<Recordings>) -> Self {
        let seed = seed.wrapping_mul(0x9e37_79b9) | 1;
        let mut body = Low::default();
        body.tune(900.0);
        let mut crack = Band::default();
        crack.tune(1_100.0, 2.0);
        // Karts start their loops at different places.
        let start = |clip: &[f32], salt: u32| Tape { at: ((seed ^ salt) % clip.len().max(1) as u32) as f32 };
        let tapes = recordings
            .engine
            .iter()
            .enumerate()
            .map(|(k, (clip, _))| Grains::new(seed ^ (k as u32 * 0x2545), clip))
            .collect();
        Self {
            engine: Engine {
                phase: 0.0,
                jitter: 0.0,
                fire: 0.0,
                decay: (-DT * 700.0).exp(),
                noise: Noise(seed),
                pipe: Band::default(),
                bark: Band::default(),
                body,
                step: 0.0,
                load: 0.0,
                misfire: 0.0,
                tapes,
                playing: [(0, 0.0, 1.0); 2],
                tone: Low { y: 0.0, a: 1.0 },
                sway: (seed % 97) as f32 / 97.0,
            },
            wanted: Drive::default(),
            now: Drive::default(),
            noise: Noise(seed ^ 0x5bd1_e995),
            squeal: Band::default(),
            drum: Low::default(),
            grass: Low::default(),
            wind: Low::default(),
            crack,
            kerb: 0.0,
            knocks: Vec::new(),
            cues: Vec::new(),
            rush: Band::default(),
            count: 0,
            ease: 1.0 - (-(BLOCK as f32) * DT / 0.015).exp(),
            gains: Gains::default(),
            squeal_tape: recordings.squeal.as_deref().map_or(Tape::default(), |c| start(c, 0x51)),
            ground_tape: recordings.ground.as_deref().map_or(Tape::default(), |c| start(c, 0x9a)),
            recordings,
            knocked: seed,
            held_revs: 0.0,
            calm: 0.0,
        }
    }

    /// What to follow from now on.
    pub fn set(&mut self, drive: Drive) {
        self.wanted = drive;
    }

    /// A knock, `strength` 0…1.
    pub fn knock(&mut self, strength: f32) {
        if self.knocks.len() < 8 {
            self.knocked = self.knocked.wrapping_mul(0x2c1b_3c6d).wrapping_add(0x297a_2d39);
            let clip = (self.knocked >> 16) as usize % self.recordings.knocks.len().max(1);
            self.knocks.push(Knock { age: 0.0, strength: strength.clamp(0.0, 1.0), clip, at: 0 });
        }
    }

    /// A paddock object's cue, from this kart.
    pub fn cue(&mut self, cue: Cue) {
        if self.cues.len() < 4 {
            if self.cues.is_empty() {
                self.rush.tune(if cue == Cue::Push { 500.0 } else { 900.0 }, 1.5);
            }
            self.cues.push((cue, 0.0));
        }
    }

    /// The cues sounding now, one sample.
    fn cues(&mut self) -> f32 {
        let mut out = 0.0;
        let noise = self.noise.next();
        let mut rush = 0.0;
        for (cue, age) in &mut self.cues {
            let t = *age;
            match cue {
                // A thump whose pitch falls, and air let out.
                Cue::Bumper => {
                    let pitch = 48.0 + 60.0 * (-t * 18.0).exp();
                    out += (TAU * pitch * t).sin() * (-t * 9.0).exp() * 0.9;
                    rush += noise * (-t * 12.0).exp() * 0.8;
                }
                // Air rising in pitch as the kart is shoved, then a fade.
                Cue::Push => {
                    let swell = (t / 0.12).min(1.0) * (-(t - 0.12).max(0.0) * 4.5).exp();
                    rush += noise * swell * 1.8;
                }
            }
            *age += DT;
        }
        self.cues.retain(|(cue, age)| *age < cue.length());
        if let Some((cue, age)) = self.cues.first() {
            let rising = if *cue == Cue::Push { 500.0 + 2_500.0 * (age / cue.length()) } else { 900.0 };
            if self.count.is_multiple_of(BLOCK) {
                self.rush.tune(rising, 1.5);
            }
        }
        out + self.rush.pass(rush)
    }

    /// Eases towards what is wanted (over about 15 ms) and sets what
    /// changes slowly, once a block.
    fn control(&mut self) {
        let (w, n, a) = (self.wanted, &mut self.now, self.ease);
        n.revs += (w.revs - n.revs) * a;
        n.throttle += (w.throttle - n.throttle) * a;
        n.slip += (w.slip - n.slip) * a;
        n.ground += (w.ground - n.ground) * a;
        n.speed += (w.speed - n.speed) * a;
        let d = self.now;
        self.engine.control(&d, &self.recordings.engine);
        let moving = (d.speed / 8.0).min(1.0);
        self.squeal.tune(850.0 + 700.0 * d.slip + 10.0 * d.speed, 14.0);
        let on_kerb = (1.0 - (d.ground - 0.6).abs() / 0.4).clamp(0.0, 1.0);
        let on_grass = ((d.ground - 0.6) / 0.4).clamp(0.0, 1.0);
        self.drum.tune(140.0);
        self.grass.tune(180.0);
        self.wind.tune(300.0 + 25.0 * d.speed);
        // The ear tires of a held note, and so the engine, flat out and
        // steady for a few seconds, eases down by a quarter; any change of
        // revs brings it back at once (habituation, as a mix engineer
        // rides a fader).
        let block = BLOCK as f32 * DT;
        self.held_revs += (d.revs - self.held_revs) * (block / 0.5).min(1.0);
        let steady = (d.revs - self.held_revs).abs() < 0.03 && d.throttle > 0.5 && d.speed > 8.0;
        let (target, time) = if steady { (1.0, 3.0) } else { (0.0, 0.25) };
        self.calm += (target - self.calm) * (block / time).min(1.0);
        self.gains = Gains {
            // High in the revs, the engine is mixed lower: flat out, it
            // would cover the tyres, the others and the music.
            engine: (1.0 - 0.3 * ((d.revs - 0.55) / 0.25).clamp(0.0, 1.0)) * (1.0 - 0.25 * self.calm),
            squeal: d.slip.powf(1.5) * moving * 0.55,
            squeal_rate: 0.9 + 0.25 * d.slip,
            drum: on_kerb * moving * 0.35,
            grass: on_grass * moving * 0.9,
            ground_rate: 0.6 + d.speed / 30.0,
            wind: (d.speed / 25.0).powi(2) * 0.5,
            kerb_step: d.speed / KERB_BLOCK * DT,
        };
    }

    /// The next sample, about −1…1.
    pub fn sample(&mut self) -> f32 {
        if self.count.is_multiple_of(BLOCK) {
            self.control();
        }
        self.count = self.count.wrapping_add(1);
        let g = &self.gains;
        let tapes = &*self.recordings;

        let engine = self.engine.sample(&tapes.engine) * g.engine;

        let squeal = match &tapes.squeal {
            Some(clip) if g.squeal > 0.0 => self.squeal_tape.play(clip, g.squeal_rate) * g.squeal * TAPE_SQUEAL,
            Some(_) => 0.0,
            None => self.squeal.pass(self.noise.next()) * g.squeal,
        };

        // Kerbs drum as their blocks pass; grass rumbles.
        self.kerb = (self.kerb + g.kerb_step).fract();
        let block = if self.kerb < 0.5 { 1.0 } else { -1.0 };
        let drum = self.drum.pass(block) * g.drum;
        let grass = match &tapes.ground {
            Some(clip) if g.grass > 0.0 => self.ground_tape.play(clip, g.ground_rate) * g.grass * TAPE_GROUND,
            Some(_) => 0.0,
            None => self.grass.pass(self.noise.next()) * g.grass,
        };

        let wind = self.wind.pass(self.noise.next()) * g.wind;

        let mut knocks = 0.0;
        let noise = self.noise.next();
        let mut crack = 0.0;
        let mut taped = 0.0;
        for k in &mut self.knocks {
            let t = k.age;
            knocks += k.strength
                * ((TAU * 70.0 * t).sin() * (-t * 22.0).exp() + (TAU * 330.0 * t).sin() * (-t * 30.0).exp() * 0.5);
            match tapes.knocks.get(k.clip) {
                Some(clip) => {
                    taped += clip.get(k.at).copied().unwrap_or(0.0) * k.strength;
                    k.at += 1;
                }
                None => crack += k.strength * noise * (-t * 45.0).exp(),
            }
            k.age += DT;
        }
        let long = tapes.knocks.iter().map(|c| c.len()).max().unwrap_or(0);
        self.knocks.retain(|k| k.age < 0.4 || k.at < long);
        let crack = self.crack.pass(crack) * 1.5 + taped * TAPE_KNOCK;

        let cues = if self.cues.is_empty() { 0.0 } else { self.cues() };

        // A soft limiter: loud moments round off instead of clipping.
        soft(engine + squeal + drum + grass + wind + knocks * 0.7 + crack + cues)
    }

    /// Fills `out` with the next samples.
    pub fn render(&mut self, out: &mut [f32]) {
        for s in out {
            *s = self.sample();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(voice: &mut Voice, drive: Drive, seconds: f32) -> Vec<f32> {
        voice.set(drive);
        let mut out = vec![0.0; (seconds * RATE as f32) as usize];
        voice.render(&mut out);
        out
    }

    fn rms(s: &[f32]) -> f32 {
        (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt()
    }

    /// The strongest period between `low` and `high` samples, by autocorrelation.
    fn period(s: &[f32], low: usize, high: usize) -> usize {
        (low..high)
            .max_by(|&a, &b| {
                let c = |lag: usize| s.iter().zip(&s[lag..]).map(|(x, y)| x * y).sum::<f32>();
                c(a).total_cmp(&c(b))
            })
            .unwrap()
    }

    #[test]
    fn the_engine_fires_once_a_turn() {
        for revs in [0.2f32, 0.6] {
            let mut voice = Voice::new(1);
            let drive = Drive { revs, throttle: 1.0, ..Drive::default() };
            let s = run(&mut voice, drive, 0.5);
            let rpm = IDLE_RPM + (MAX_RPM - IDLE_RPM) * revs;
            let expected = RATE as f32 / (rpm / 60.0);
            let found = period(&s[RATE as usize / 10..], (expected * 0.7) as usize, (expected * 1.3) as usize);
            assert!((found as f32 - expected).abs() / expected < 0.06, "revs {revs}: {found} for {expected}");
        }
    }

    #[test]
    fn throttle_makes_the_engine_louder_and_everything_stays_in_range() {
        let mut open = Voice::new(2);
        let mut shut = Voice::new(2);
        let loud = run(&mut open, Drive { revs: 0.7, throttle: 1.0, speed: 20.0, ..Drive::default() }, 0.5);
        let soft = run(&mut shut, Drive { revs: 0.7, throttle: 0.0, speed: 20.0, ..Drive::default() }, 0.5);
        assert!(rms(&loud) > 1.3 * rms(&soft), "{} {}", rms(&loud), rms(&soft));
        assert!(loud.iter().chain(&soft).all(|x| x.is_finite() && x.abs() <= 1.0));
    }

    #[test]
    fn tyres_squeal_only_when_they_slide() {
        let base = Drive { revs: 0.0, throttle: 0.0, speed: 15.0, ..Drive::default() };
        let quiet = rms(&run(&mut Voice::new(3), base, 0.3));
        let sliding = rms(&run(&mut Voice::new(3), Drive { slip: 0.8, ..base }, 0.3));
        assert!(sliding > quiet + 0.05, "{quiet} {sliding}");
    }

    #[test]
    fn kerbs_drum_at_the_rate_their_blocks_pass() {
        // Engine off and no wind: only the kerb, at 10 m/s, 10 blocks a second.
        let drive = Drive { revs: 0.0, throttle: 0.0, ground: 0.6, speed: 10.0, slip: 0.0 };
        let mut voice = Voice::new(4);
        voice.engine.fire = 0.0;
        let s = run(&mut voice, drive, 1.0);
        let found = period(&s[RATE as usize / 5..], 3_000, 7_000);
        assert!((found as f32 - 4_800.0).abs() < 300.0, "{found}");
    }

    #[test]
    fn a_knock_is_heard_then_dies() {
        let mut voice = Voice::new(5);
        let still = Drive::default();
        let before = rms(&run(&mut voice, still, 0.1));
        voice.knock(1.0);
        let during = rms(&run(&mut voice, still, 0.1));
        let after = rms(&run(&mut voice, still, 0.5)[RATE as usize / 5..]);
        // The idling engine stays: after the knock, back to it.
        assert!(during > before + 0.1 && after < before * 1.3, "{before} {during} {after}");
    }

    #[test]
    fn the_engine_turns_with_the_wheels_above_the_clutch() {
        assert!(revs(0.0, 22.0, 0.0) < 0.01, "idling");
        assert!(revs(0.0, 22.0, 1.0) > 0.1, "revving on the spot");
        let cruise = (CRUISE_RPM - IDLE_RPM) / (MAX_RPM - IDLE_RPM);
        assert!((revs(22.0, 22.0, 1.0) - cruise).abs() < 1e-3, "top speed is cruising, not the limiter");
        assert!((revs(22.0 * 1.3, 22.0, 1.0) - 1.0).abs() < 1e-3, "a boost takes it to the limiter");
        assert!(revs(11.0, 22.0, 0.0) > revs(5.0, 22.0, 0.0), "faster, higher");
    }

    #[test]
    fn the_same_voice_twice_is_the_same() {
        let drive = Drive { revs: 0.5, throttle: 0.7, slip: 0.3, ground: 0.6, speed: 12.0 };
        assert_eq!(run(&mut Voice::new(9), drive, 0.2), run(&mut Voice::new(9), drive, 0.2));
    }

    /// A loop of a sine at `hz`.
    fn tone(hz: f32) -> Arc<[f32]> {
        // A whole number of periods: seamless.
        let n = (RATE as f32 / hz * 20.0).round() as usize;
        (0..n).map(|i| (TAU * 20.0 * i as f32 / n as f32).sin() * 0.14).collect()
    }

    /// A loop of noise, as an engine's recording mostly is: `seconds` long,
    /// seamless (a noise has no seam to hear).
    fn rumble(seconds: f32) -> Arc<[f32]> {
        // Low: an engine's sound is mostly below a few kHz.
        let (mut noise, mut low) = (Noise(77), Low::default());
        low.tune(1_500.0);
        (0..(seconds * RATE as f32) as usize).map(|_| low.pass(noise.next()) * 0.6).collect()
    }

    /// D-070: played as grains, a loop a second long no longer repeats
    /// every second.
    #[test]
    fn a_short_loop_is_played_without_its_period() {
        let loop_ = rumble(1.0);
        let n = loop_.len();
        let tapes = Arc::new(Recordings { engine: vec![(loop_, 9_000.0)], ..Recordings::default() });
        let revs = (9_000.0 - IDLE_RPM) / (MAX_RPM - IDLE_RPM);
        let s = run(&mut Voice::with(5, tapes), Drive { revs, throttle: 1.0, speed: 20.0, ..Drive::default() }, 3.2);
        let (a, b) = (&s[RATE as usize / 5..][..n], &s[RATE as usize / 5 + n..][..n]);
        let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
        let norm = (a.iter().map(|x| x * x).sum::<f32>() * b.iter().map(|y| y * y).sum::<f32>()).sqrt();
        assert!(dot / norm < 0.2, "one second like the next: {}", dot / norm);
        assert!(rms(a) > 0.05, "and heard: {}", rms(a));
    }

    #[test]
    fn a_recorded_engine_follows_the_revs() {
        // One loop at 100 Hz, recorded at 6 000 turns a minute: at 7 200 it
        // plays a fifth faster.
        let tapes = Arc::new(Recordings { engine: vec![(tone(100.0), 6_000.0)], ..Recordings::default() });
        let mut voice = Voice::with(1, tapes);
        let revs = (7_200.0 - IDLE_RPM) / (MAX_RPM - IDLE_RPM);
        let s = run(&mut voice, Drive { revs, throttle: 1.0, ..Drive::default() }, 0.5);
        let found = period(&s[RATE as usize / 10..], 250, 550);
        assert!((found as f32 - 400.0).abs() < 5.0, "{found}");
    }

    #[test]
    fn recorded_engine_loops_crossfade_between_their_speeds() {
        let tapes = Arc::new(Recordings {
            engine: vec![(rumble(1.3), 4_000.0), (rumble(1.1), 8_000.0)],
            ..Recordings::default()
        });
        let level = |rpm: f32| {
            let mut voice = Voice::with(2, tapes.clone());
            let revs = (rpm - IDLE_RPM) / (MAX_RPM - IDLE_RPM);
            rms(&run(&mut voice, Drive { revs, throttle: 1.0, ..Drive::default() }, 0.3)[RATE as usize / 10..])
        };
        // At equal power the level holds through the crossfade, within the
        // beating of the two loops.
        let (low, middle, high) = (level(4_000.0), level(5_657.0), level(8_000.0));
        assert!(
            low > 0.1 && (middle / low - 1.0).abs() < 0.5 && (high / low - 1.0).abs() < 0.1,
            "{low} {middle} {high}"
        );
    }

    #[test]
    fn recorded_tyres_and_knocks_play_only_when_they_should() {
        let tapes = Arc::new(Recordings {
            squeal: Some(tone(1_000.0)),
            knocks: vec![(0..4_800).map(|i| if i < 2_400 { 0.5 } else { 0.0 }).collect()],
            ..Recordings::default()
        });
        let still = Drive { speed: 15.0, ..Drive::default() };
        let mut voice = Voice::with(3, tapes.clone());
        let rolling = rms(&run(&mut voice, still, 0.3));
        let sliding = rms(&run(&mut voice, Drive { slip: 0.8, ..still }, 0.3));
        assert!(sliding > rolling + 0.1, "{rolling} {sliding}");
        let mut voice = Voice::with(3, tapes);
        let before = rms(&run(&mut voice, Drive::default(), 0.05));
        voice.knock(1.0);
        let during = rms(&run(&mut voice, Drive::default(), 0.04));
        assert!(during > before + 0.2, "{before} {during}");
    }

    #[test]
    fn cues_sound_and_end() {
        let mut quiet = Voice::new(3);
        let mut cued = Voice::new(3);
        cued.cue(Cue::Bumper);
        let energy = |voice: &mut Voice, n: usize| (0..n).map(|_| voice.sample().powi(2)).sum::<f32>() / n as f32;
        let n = (0.3 * RATE as f32) as usize;
        let (base, loud) = (energy(&mut quiet, n), energy(&mut cued, n));
        assert!(loud > base * 4.0 + 1e-4, "a bumper is heard: {loud} over {base}");
        // Over by its length: the same as an uncued voice afterwards.
        let rest = (0.5 * RATE as f32) as usize;
        energy(&mut quiet, rest);
        energy(&mut cued, rest);
        assert!(cued.cues.is_empty());
        let mut pushed = Voice::new(3);
        pushed.cue(Cue::Push);
        assert!(energy(&mut pushed, n) > base * 2.0 + 1e-5);
    }

    #[test]
    fn a_drone_buzzes_with_its_gain_and_is_silent_without() {
        let mut buzz = Buzz::new(1);
        let silent: f32 = (0..4800).map(|_| buzz.sample(0.0).abs()).sum();
        assert_eq!(silent, 0.0);
        let loud: f32 = (0..4800).map(|_| buzz.sample(1.0).powi(2)).sum::<f32>() / 4800.0;
        assert!(loud > 0.005 && loud < 1.0, "{loud}");
    }

    #[test]
    fn a_loop_is_never_played_much_faster_than_it_was_recorded() {
        // Recorded at 6 000: at the limiter it would play 2.4 times as fast.
        let tapes = Arc::new(Recordings { engine: vec![(tone(100.0), 6_000.0)], ..Recordings::default() });
        let mut voice = Voice::with(3, tapes);
        let s = run(&mut voice, Drive { revs: 1.0, throttle: 1.0, ..Drive::default() }, 0.5);
        let found = period(&s[RATE as usize / 10..], 250, 550);
        assert!((found as f32 - 480.0 / FASTEST_TAPE).abs() < 6.0, "{found}");
    }

    #[test]
    fn flat_out_and_steady_the_engine_eases_down_and_comes_back_on_a_change() {
        let tapes = Arc::new(Recordings { engine: vec![(rumble(1.1), 9_000.0)], ..Recordings::default() });
        let flat_out = Drive { revs: 0.77, throttle: 1.0, speed: 22.0, ..Drive::default() };
        let mut voice = Voice::with(4, tapes.clone());
        let first = rms(&run(&mut voice, flat_out, 1.0)[RATE as usize / 2..]);
        let later = rms(&run(&mut voice, flat_out, 8.0)[RATE as usize * 7..]);
        assert!(later < 0.85 * first && later > 0.6 * first, "held: {first} then {later}");
        let lifted = rms(&run(&mut voice, Drive { revs: 0.6, ..flat_out }, 1.0)[RATE as usize / 2..]);
        assert!(lifted > later, "a change of revs brings it back: {later} then {lifted}");
        // High in the revs it is mixed lower than in the middle.
        let middle = rms(&run(&mut Voice::with(4, tapes), Drive { revs: 0.45, ..flat_out }, 1.0)[RATE as usize / 2..]);
        assert!(first < middle, "{first} {middle}");
    }
}
