// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The small parts the voices are made of. The noise, the band-pass, the
//! one-pole low-pass and the soft limiter are those of `osage-kart-sound`
//! (private there), kept the same as there; the second-order sections are
//! Robert Bristow-Johnson's (Audio EQ Cookbook), the pink filter Paul
//! Kellett's.

use std::f32::consts::{PI, TAU};

/// Samples a second, as every osage voice.
pub const RATE: u32 = 48_000;
pub const DT: f32 = 1.0 / RATE as f32;

/// The level every voice sounds at when driven as its documentation says
/// (a car at 30 km/h, a bogie at 40): an RMS of 0.1, one metre away — the
/// loudness `tools/make-sounds.py` evens the kart's loops to.
pub const NOMINAL_RMS: f32 = 0.1;

/// A gain from decibels.
pub fn db(decibels: f32) -> f32 {
    10f32.powf(decibels / 20.0)
}

/// A soft limiter, cheaper than `tanh` and as smooth: odd, 1 at 3 and beyond.
pub fn soft(x: f32) -> f32 {
    let x = x.clamp(-3.0, 3.0);
    x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
}

/// A seed for the part `name` of something seeded `seed`: the same on every
/// machine, and unchanged when other parts are added (FNV-1a).
pub fn seed_for(seed: u64, name: &str) -> u32 {
    let mut h = 0xcbf2_9ce4_8422_2325u64 ^ seed.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    for byte in name.bytes() {
        h = (h ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    ((h >> 32) ^ h) as u32
}

/// A small, seeded noise: xorshift.
#[derive(Clone, Debug)]
pub struct Noise(u32);

impl Noise {
    pub fn new(seed: u32) -> Self {
        Self(seed.wrapping_mul(0x9e37_79b9) | 1)
    }

    fn step(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }

    /// White, uniform in −1…1 (an RMS of 0.577). Named as in
    /// `osage-kart-sound`, whose noise this is.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> f32 {
        (self.step() >> 8) as f32 / (1u32 << 23) as f32 - 1.0
    }

    /// Uniform in 0…1.
    pub fn unit(&mut self) -> f32 {
        (self.step() >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Uniform between `low` and `high`.
    pub fn between(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.unit()
    }

    /// Another noise, for another part of the same voice.
    pub fn fork(&mut self) -> Noise {
        Noise::new(self.step() ^ 0xa511_e9b3)
    }
}

/// White noise made pink (−3 dB an octave), within half a decibel from
/// 10 Hz up: three one-poles summed (Paul Kellett's economy filter).
#[derive(Clone, Debug, Default)]
pub struct Pink {
    b: [f32; 3],
}

impl Pink {
    pub fn pass(&mut self, white: f32) -> f32 {
        self.b[0] = 0.997_65 * self.b[0] + white * 0.099_046;
        self.b[1] = 0.963 * self.b[1] + white * 0.296_516_4;
        self.b[2] = 0.57 * self.b[2] + white * 1.052_691_3;
        self.b[0] + self.b[1] + self.b[2] + white * 0.184_8
    }
}

/// A one-pole low-pass; what it takes off is the matching high-pass.
#[derive(Clone, Debug, Default)]
pub struct Low {
    y: f32,
    a: f32,
}

impl Low {
    pub fn at(frequency: f32) -> Self {
        let mut low = Self::default();
        low.tune(frequency);
        low
    }

    pub fn tune(&mut self, frequency: f32) {
        self.a = 1.0 - (-TAU * frequency.max(0.0) * DT).exp();
    }

    /// A time constant instead of a frequency: 63 % of the way in `seconds`.
    pub fn ease(&mut self, seconds: f32) {
        self.a = 1.0 - (-DT / seconds.max(DT)).exp();
    }

    pub fn pass(&mut self, x: f32) -> f32 {
        self.y += (x - self.y) * self.a;
        self.y
    }

    /// The high-pass: `x` less what the low-pass keeps.
    pub fn above(&mut self, x: f32) -> f32 {
        x - self.pass(x)
    }

    pub fn value(&self) -> f32 {
        self.y
    }

    pub fn set(&mut self, y: f32) {
        self.y = y;
    }
}

/// A resonant state-variable filter (topology preserving): low, band and
/// high at once. The band's gain at its centre is `q`.
#[derive(Clone, Debug, Default)]
pub struct Band {
    ic1: f32,
    ic2: f32,
    g: f32,
    k: f32,
    a1: f32,
}

impl Band {
    pub fn at(frequency: f32, q: f32) -> Self {
        let mut band = Self::default();
        band.tune(frequency, q);
        band
    }

    pub fn tune(&mut self, frequency: f32, q: f32) {
        self.g = (PI * frequency.clamp(10.0, RATE as f32 * 0.45) * DT).tan();
        self.k = 1.0 / q.max(0.05);
        self.a1 = 1.0 / (1.0 + self.g * (self.g + self.k));
    }

    /// Low, band and high of `x`.
    pub fn split(&mut self, x: f32) -> (f32, f32, f32) {
        let v3 = x - self.ic2;
        let v1 = self.a1 * (self.ic1 + self.g * v3);
        let v2 = self.ic2 + self.g * v1;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v2, v1, x - self.k * v1 - v2)
    }

    /// The band, as `osage-kart-sound`'s: `q` times louder at its centre.
    pub fn pass(&mut self, x: f32) -> f32 {
        self.split(x).1
    }

    /// The band at the level it came in at its centre.
    pub fn level(&mut self, x: f32) -> f32 {
        self.split(x).1 * self.k
    }
}

/// A second-order section (Audio EQ Cookbook), transposed direct form II.
#[derive(Clone, Debug)]
pub struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    z: [f32; 2],
}

impl Default for Biquad {
    fn default() -> Self {
        Self { b: [1.0, 0.0, 0.0], a: [0.0, 0.0], z: [0.0, 0.0] }
    }
}

impl Biquad {
    fn set(&mut self, b: [f32; 3], a: [f32; 3]) {
        self.b = [b[0] / a[0], b[1] / a[0], b[2] / a[0]];
        self.a = [a[1] / a[0], a[2] / a[0]];
    }

    fn turn(frequency: f32, q: f32) -> (f32, f32) {
        let w = TAU * frequency.clamp(10.0, RATE as f32 * 0.45) * DT;
        (w.cos(), w.sin() / (2.0 * q.max(0.05)))
    }

    pub fn low_pass(&mut self, frequency: f32, q: f32) -> &mut Self {
        let (cos, alpha) = Self::turn(frequency, q);
        self.set([(1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0], [1.0 + alpha, -2.0 * cos, 1.0 - alpha]);
        self
    }

    pub fn high_pass(&mut self, frequency: f32, q: f32) -> &mut Self {
        let (cos, alpha) = Self::turn(frequency, q);
        self.set([(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0], [1.0 + alpha, -2.0 * cos, 1.0 - alpha]);
        self
    }

    /// A band at its own level at its centre.
    pub fn band_pass(&mut self, frequency: f32, q: f32) -> &mut Self {
        let (cos, alpha) = Self::turn(frequency, q);
        self.set([alpha, 0.0, -alpha], [1.0 + alpha, -2.0 * cos, 1.0 - alpha]);
        self
    }

    pub fn notch(&mut self, frequency: f32, q: f32) -> &mut Self {
        let (cos, alpha) = Self::turn(frequency, q);
        self.set([1.0, -2.0 * cos, 1.0], [1.0 + alpha, -2.0 * cos, 1.0 - alpha]);
        self
    }

    /// A bell of `gain` decibels.
    pub fn peak(&mut self, frequency: f32, q: f32, gain: f32) -> &mut Self {
        let (cos, alpha) = Self::turn(frequency, q);
        let a = 10f32.powf(gain / 40.0);
        self.set([1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a], [1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a]);
        self
    }

    /// Everything under `frequency` raised by `gain` decibels.
    pub fn low_shelf(&mut self, frequency: f32, gain: f32) -> &mut Self {
        let (cos, alpha) = Self::turn(frequency, std::f32::consts::FRAC_1_SQRT_2);
        let a = 10f32.powf(gain / 40.0);
        let r = 2.0 * a.sqrt() * alpha;
        self.set(
            [
                a * ((a + 1.0) - (a - 1.0) * cos + r),
                2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                a * ((a + 1.0) - (a - 1.0) * cos - r),
            ],
            [(a + 1.0) + (a - 1.0) * cos + r, -2.0 * ((a - 1.0) + (a + 1.0) * cos), (a + 1.0) + (a - 1.0) * cos - r],
        );
        self
    }

    /// Everything over `frequency` raised by `gain` decibels.
    pub fn high_shelf(&mut self, frequency: f32, gain: f32) -> &mut Self {
        let (cos, alpha) = Self::turn(frequency, std::f32::consts::FRAC_1_SQRT_2);
        let a = 10f32.powf(gain / 40.0);
        let r = 2.0 * a.sqrt() * alpha;
        self.set(
            [
                a * ((a + 1.0) + (a - 1.0) * cos + r),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                a * ((a + 1.0) + (a - 1.0) * cos - r),
            ],
            [(a + 1.0) - (a - 1.0) * cos + r, 2.0 * ((a - 1.0) - (a + 1.0) * cos), (a + 1.0) - (a - 1.0) * cos - r],
        );
        self
    }

    pub fn pass(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// A turning phase, 0…1: what a tone, a wheel or a slow sway counts on.
#[derive(Clone, Debug, Default)]
pub struct Phase(pub f32);

impl Phase {
    /// Moves on by `frequency` for one sample; true when it came round.
    pub fn turn(&mut self, frequency: f32) -> bool {
        self.0 += frequency * DT;
        if self.0 >= 1.0 {
            self.0 -= self.0.floor();
            return true;
        }
        false
    }

    pub fn sin(&self) -> f32 {
        (TAU * self.0).sin()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    /// The level of a sine at `frequency` after `pass`, once it has settled.
    fn gain(frequency: f32, mut pass: impl FnMut(f32) -> f32) -> f32 {
        let out: Vec<f32> = (0..RATE).map(|i| pass((TAU * frequency * i as f32 * DT).sin())).collect();
        rms(&out[RATE as usize / 2..]) * std::f32::consts::SQRT_2
    }

    #[test]
    fn the_noise_is_the_same_twice_and_fills_its_range() {
        let (mut a, mut b) = (Noise::new(7), Noise::new(7));
        let xs: Vec<f32> = (0..100_000).map(|_| a.next()).collect();
        assert!(xs.iter().zip((0..100_000).map(|_| b.next())).all(|(x, y)| *x == y));
        assert!(xs.iter().all(|x| (-1.0..1.0).contains(x)));
        assert!((rms(&xs) - 0.577).abs() < 0.01, "{}", rms(&xs));
        assert!(Noise::new(8).next() != Noise::new(7).next());
        let mut c = Noise::new(7);
        assert!((0..10_000).all(|_| (0.0..1.0).contains(&c.unit())));
    }

    #[test]
    fn a_part_keeps_its_seed_whatever_else_is_seeded() {
        assert_eq!(seed_for(6, "bridge/river"), seed_for(6, "bridge/river"));
        assert_ne!(seed_for(6, "bridge/river"), seed_for(7, "bridge/river"));
        assert_ne!(seed_for(6, "bridge/river"), seed_for(6, "bridge/road"));
    }

    #[test]
    fn filters_pass_what_they_say() {
        let mut low = Low::at(1_000.0);
        assert!((gain(100.0, |x| low.pass(x)) - 1.0).abs() < 0.02);
        let mut low = Low::at(1_000.0);
        assert!(gain(10_000.0, |x| low.pass(x)) < 0.15);

        let mut band = Band::at(1_000.0, 4.0);
        assert!((gain(1_000.0, |x| band.pass(x)) - 4.0).abs() < 0.05);
        let mut band = Band::at(1_000.0, 4.0);
        assert!((gain(1_000.0, |x| band.level(x)) - 1.0).abs() < 0.02);
        let mut band = Band::at(1_000.0, 4.0);
        assert!(gain(4_000.0, |x| band.level(x)) < 0.1);

        let mut bell = Biquad::default();
        bell.peak(2_000.0, 1.0, 6.0);
        assert!((gain(2_000.0, |x| bell.pass(x)) - db(6.0)).abs() < 0.03);
        let mut shelf = Biquad::default();
        shelf.high_shelf(1_000.0, -12.0);
        assert!((gain(12_000.0, |x| shelf.pass(x)) - db(-12.0)).abs() < 0.02);
        let mut shelf = Biquad::default();
        shelf.low_shelf(1_000.0, 6.0);
        assert!((gain(60.0, |x| shelf.pass(x)) - db(6.0)).abs() < 0.03);
        let mut cut = Biquad::default();
        cut.low_pass(500.0, 0.707);
        assert!(gain(4_000.0, |x| cut.pass(x)) < 0.02);
        let mut cut = Biquad::default();
        cut.high_pass(2_000.0, 0.707);
        assert!(gain(200.0, |x| cut.pass(x)) < 0.02);
        let mut hole = Biquad::default();
        hole.notch(3_000.0, 4.0);
        assert!(gain(3_000.0, |x| hole.pass(x)) < 0.02);
    }

    #[test]
    fn pink_falls_three_decibels_an_octave() {
        let level = |frequency: f32| {
            let (mut noise, mut pink, mut band) = (Noise::new(3), Pink::default(), Biquad::default());
            band.band_pass(frequency, 4.0);
            let out: Vec<f32> = (0..RATE * 8).map(|_| band.pass(pink.pass(noise.next()))).collect();
            20.0 * rms(&out).log10()
        };
        // A band of constant Q is twice as wide an octave up, so pink noise
        // keeps its level from band to band.
        let (low, high) = (level(250.0), level(4_000.0));
        assert!((low - high).abs() < 1.5, "{low} {high}");
    }

    #[test]
    fn the_limiter_is_odd_and_bounded() {
        assert_eq!(soft(0.0), 0.0);
        assert!((soft(3.0) - 1.0).abs() < 1e-6 && (soft(9.0) - 1.0).abs() < 1e-6);
        assert!((soft(-0.5) + soft(0.5)).abs() < 1e-7);
        assert!((soft(0.1) - 0.1).abs() < 0.001);
    }
}
