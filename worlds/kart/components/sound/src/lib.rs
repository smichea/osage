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
//! - **knocks:** a thump and the crack of plastic, as strong as the impulse.
//!
//! Used where no recording is at hand (D-044's rule for sound: recordings
//! when the content is there, this otherwise), and for what recordings
//! cannot follow, the revs. Deterministic: a seeded noise, no clock.

use std::f32::consts::{PI, TAU};

/// Samples a second.
pub const RATE: u32 = 48_000;
const DT: f32 = 1.0 / RATE as f32;

/// Engine speeds of a 125 cc two-stroke kart engine, turns a minute.
pub const IDLE_RPM: f32 = 2_800.0;
pub const MAX_RPM: f32 = 14_500.0;
/// A kart's centrifugal clutch bites at about this engine speed.
const CLUTCH_RPM: f32 = 4_500.0;
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
/// gearbox, so the engine turns with the wheels above the clutch's speed;
/// below, it revs with the throttle.
pub fn revs(speed: f32, top: f32, throttle: f32) -> f32 {
    let geared = CLUTCH_RPM + (MAX_RPM - CLUTCH_RPM) * (speed.abs() / top.max(1.0)).min(1.05);
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

/// A resonant band-pass: a state-variable filter (topology preserving).
#[derive(Clone, Debug, Default)]
struct Band {
    ic1: f32,
    ic2: f32,
}

impl Band {
    fn pass(&mut self, x: f32, frequency: f32, q: f32) -> f32 {
        let g = (PI * frequency.clamp(20.0, RATE as f32 * 0.45) * DT).tan();
        let k = 1.0 / q;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + g * a1 * v3;
        let v2 = self.ic2 + g * v1;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        v1
    }
}

/// A one-pole low-pass.
#[derive(Clone, Debug, Default)]
struct Low(f32);

impl Low {
    fn pass(&mut self, x: f32, frequency: f32) -> f32 {
        let a = 1.0 - (-TAU * frequency * DT).exp();
        self.0 += (x - self.0) * a;
        self.0
    }
}

/// The engine.
#[derive(Clone, Debug)]
struct Engine {
    phase: f32,
    jitter: f32,
    /// What is left of the last firing.
    fire: f32,
    noise: Noise,
    pipe: Band,
    bark: Band,
    body: Low,
}

impl Engine {
    fn sample(&mut self, d: &Drive) -> f32 {
        let rpm = IDLE_RPM + (MAX_RPM - IDLE_RPM) * d.revs;
        self.phase += rpm / 60.0 * DT * (1.0 + self.jitter);
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            self.jitter = self.noise.next() * 0.025;
            // Off the throttle a two-stroke skips firings.
            let misfire = self.noise.next() * 0.5 + 0.5 < (1.0 - d.throttle) * 0.3 * (0.3 + d.revs);
            if !misfire {
                self.fire = 1.0;
            }
        }
        self.fire *= (-DT * 700.0).exp();
        let load = 0.35 + 0.65 * d.throttle;
        let burst = self.fire * (0.7 + 0.3 * load) + self.noise.next() * self.fire * (0.2 + 0.5 * load);
        let pipe = self.pipe.pass(burst, 380.0 + 260.0 * d.revs, 3.5);
        let bark = self.bark.pass(burst, 1_400.0 + 900.0 * d.revs, 2.5) * (0.25 + 0.5 * load);
        let body = self.body.pass(burst, 900.0);
        ((pipe * 1.6 + bark + body * 1.2) * (1.2 + 1.8 * load)).tanh() * (0.32 + 0.2 * load)
    }
}

/// A knock under way.
#[derive(Clone, Debug)]
struct Knock {
    age: f32,
    strength: f32,
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
}

impl Voice {
    /// A voice; `seed` keeps karts from sounding alike.
    pub fn new(seed: u32) -> Self {
        let seed = seed.wrapping_mul(0x9e37_79b9) | 1;
        Self {
            engine: Engine {
                phase: 0.0,
                jitter: 0.0,
                fire: 0.0,
                noise: Noise(seed),
                pipe: Band::default(),
                bark: Band::default(),
                body: Low::default(),
            },
            wanted: Drive::default(),
            now: Drive::default(),
            noise: Noise(seed ^ 0x5bd1_e995),
            squeal: Band::default(),
            drum: Low::default(),
            grass: Low::default(),
            wind: Low::default(),
            crack: Band::default(),
            kerb: 0.0,
            knocks: Vec::new(),
        }
    }

    /// What to follow from now on.
    pub fn set(&mut self, drive: Drive) {
        self.wanted = drive;
    }

    /// A knock, `strength` 0…1.
    pub fn knock(&mut self, strength: f32) {
        if self.knocks.len() < 8 {
            self.knocks.push(Knock { age: 0.0, strength: strength.clamp(0.0, 1.0) });
        }
    }

    /// The next sample, about −1…1.
    pub fn sample(&mut self) -> f32 {
        // Ease towards what is wanted over about 15 ms.
        let a = 1.0 - (-DT / 0.015f32).exp();
        let (w, n) = (self.wanted, &mut self.now);
        n.revs += (w.revs - n.revs) * a;
        n.throttle += (w.throttle - n.throttle) * a;
        n.slip += (w.slip - n.slip) * a;
        n.ground += (w.ground - n.ground) * a;
        n.speed += (w.speed - n.speed) * a;
        let d = self.now;

        let engine = self.engine.sample(&d);

        let moving = (d.speed / 8.0).min(1.0);
        let squeal = self.squeal.pass(self.noise.next(), 850.0 + 700.0 * d.slip + 10.0 * d.speed, 14.0)
            * d.slip.powf(1.5)
            * moving
            * 0.55;

        // Kerbs drum as their blocks pass; grass rumbles.
        self.kerb = (self.kerb + d.speed / KERB_BLOCK * DT).fract();
        let on_kerb = (1.0 - (d.ground - 0.6).abs() / 0.4).clamp(0.0, 1.0);
        let on_grass = ((d.ground - 0.6) / 0.4).clamp(0.0, 1.0);
        let block = if self.kerb < 0.5 { 1.0 } else { -1.0 };
        let drum = self.drum.pass(block, 140.0) * on_kerb * moving * 0.35;
        let grass = self.grass.pass(self.noise.next(), 180.0) * on_grass * moving * 0.9;

        let wind = self.wind.pass(self.noise.next(), 300.0 + 25.0 * d.speed) * (d.speed / 25.0).powi(2) * 0.5;

        let mut knocks = 0.0;
        let noise = self.noise.next();
        let mut crack = 0.0;
        for k in &mut self.knocks {
            let t = k.age;
            knocks += k.strength
                * ((TAU * 70.0 * t).sin() * (-t * 22.0).exp() + (TAU * 330.0 * t).sin() * (-t * 30.0).exp() * 0.5);
            crack += k.strength * noise * (-t * 45.0).exp();
            k.age += DT;
        }
        self.knocks.retain(|k| k.age < 0.4);
        let crack = self.crack.pass(crack, 1_100.0, 2.0) * 1.5;

        // A soft limiter: loud moments round off instead of clipping.
        (engine + squeal + drum + grass + wind + knocks * 0.7 + crack).tanh()
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
        assert!((revs(22.0, 22.0, 1.0) - 1.0).abs() < 1e-3);
        assert!(revs(11.0, 22.0, 0.0) > revs(5.0, 22.0, 0.0), "faster, higher");
    }

    #[test]
    fn the_same_voice_twice_is_the_same() {
        let drive = Drive { revs: 0.5, throttle: 0.7, slip: 0.3, ground: 0.6, speed: 12.0 };
        assert_eq!(run(&mut Voice::new(9), drive, 0.2), run(&mut Voice::new(9), drive, 0.2));
    }
}
