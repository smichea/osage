// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Signals whose measures are known before they are measured: what the
//! tests of this crate are made of.

use std::f64::consts::TAU;

/// A small, seeded noise: xorshift, as the voices use.
#[derive(Clone, Debug)]
pub struct Noise(u32);

impl Noise {
    pub fn new(seed: u32) -> Noise {
        Noise(seed.max(1))
    }

    /// The next value, −1…1.
    pub fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / (1u32 << 23) as f32 - 1.0
    }
}

/// `seconds` of white noise, −`amplitude`…`amplitude`.
pub fn white(seed: u32, rate: u32, seconds: f64, amplitude: f32) -> Vec<f32> {
    let mut noise = Noise::new(seed);
    (0..(f64::from(rate) * seconds) as usize).map(|_| noise.next() * amplitude).collect()
}

/// `seconds` of a sine of `hz` and of peak `amplitude`, starting at zero.
pub fn sine(rate: u32, hz: f64, amplitude: f64, seconds: f64) -> Vec<f32> {
    (0..(f64::from(rate) * seconds) as usize)
        .map(|n| (amplitude * (TAU * hz * n as f64 / f64::from(rate)).sin()) as f32)
        .collect()
}

/// A sound made of many sines of seeded frequencies (between `low` and
/// `high` Hz) and phases, which can be computed at any instant: so a copy
/// can be late by a time that is not a whole number of samples.
#[derive(Clone, Debug)]
pub struct Chord {
    /// (turns a second, phase in radians)
    parts: Vec<(f64, f64)>,
}

impl Chord {
    pub fn new(seed: u32, low: f64, high: f64, count: usize) -> Chord {
        let mut noise = Noise::new(seed);
        let mut unit = move || f64::from(noise.next()) * 0.5 + 0.5;
        Chord { parts: (0..count).map(|_| (low + (high - low) * unit(), TAU * unit())).collect() }
    }

    /// `seconds` of it at `rate`, heard `late` seconds late, of mean square
    /// `gain²/2`.
    pub fn render(&self, rate: u32, seconds: f64, late: f64, gain: f64) -> Vec<f32> {
        let scale = gain / (self.parts.len() as f64).sqrt();
        (0..(f64::from(rate) * seconds) as usize)
            .map(|n| {
                let t = n as f64 / f64::from(rate) - late;
                (scale * self.parts.iter().map(|(hz, phase)| (TAU * hz * t + phase).sin()).sum::<f64>()) as f32
            })
            .collect()
    }
}

/// `signal`, `samples` samples late and `gain` times as strong.
pub fn late(signal: &[f32], samples: usize, gain: f32) -> Vec<f32> {
    (0..signal.len()).map(|n| if n >= samples { signal[n - samples] * gain } else { 0.0 }).collect()
}
