// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Sounds to measure with, not to listen to: a pure tone, whose pitch and
//! level say what the way from an emitter did to it, and a steady noise,
//! which costs next to nothing to make — the kinds "tone" and "noise".
//!
//! Parameters: `hz` (the tone's frequency, 1 000 by default), `level` (the
//! RMS of either, the nominal 0.1 by default).

use std::f64::consts::TAU;

use crate::{
    Drive, Params, Voice,
    dsp::{NOMINAL_RMS, Noise, RATE},
};

pub const TONE: &str = "A pure tone, to measure a way by: its pitch and level are known";
pub const TONE_PARAMS: &[(&str, f32)] = &[("hz", 1_000.0), ("level", NOMINAL_RMS)];
pub const NOISE: &str = "A steady white noise, to measure a way by: it costs nothing to make";
pub const NOISE_PARAMS: &[(&str, f32)] = &[("level", NOMINAL_RMS)];

/// A sine: its phase is kept in double precision, so that it is still a
/// sine after an hour.
pub struct Tone {
    phase: f64,
    step: f64,
    peak: f32,
}

impl Tone {
    pub fn new(_seed: u32, params: &Params) -> Self {
        Self {
            phase: 0.0,
            step: f64::from(params.get("hz", 1_000.0).clamp(1.0, 20_000.0)) / f64::from(RATE),
            peak: params.get("level", NOMINAL_RMS).clamp(0.0, 0.7) * std::f32::consts::SQRT_2,
        }
    }
}

impl Voice for Tone {
    fn render(&mut self, _drive: &Drive, out: &mut [f32]) {
        for sample in out {
            *sample = (TAU * self.phase).sin() as f32 * self.peak;
            self.phase = (self.phase + self.step).fract();
        }
    }
}

/// White noise at a level.
pub struct Hiss {
    noise: Noise,
    gain: f32,
}

impl Hiss {
    pub fn new(seed: u32, params: &Params) -> Self {
        // A uniform noise in −1…1 has an RMS of 1/√3.
        Self { noise: Noise::new(seed), gain: params.get("level", NOMINAL_RMS).clamp(0.0, 0.5) * 3f32.sqrt() }
    }
}

impl Voice for Hiss {
    fn render(&mut self, _drive: &Drive, out: &mut [f32]) {
        for sample in out {
            *sample = self.noise.next() * self.gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn made(voice: &mut dyn Voice, samples: usize) -> Vec<f32> {
        let mut out = vec![0.0; samples];
        for block in out.chunks_mut(128) {
            voice.render(&Drive::default(), block);
        }
        out
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn the_tone_has_its_pitch_and_both_have_their_level() {
        let out = made(&mut Tone::new(1, &Params::default().with("hz", 440.0)), RATE as usize);
        let rising = out.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        assert!((439..=441).contains(&rising), "{rising} turns in a second");
        assert!((rms(&out) - NOMINAL_RMS).abs() < 0.001);
        let out = made(&mut Hiss::new(1, &Params::default()), RATE as usize);
        assert!((rms(&out) - NOMINAL_RMS).abs() < 0.002, "{}", rms(&out));
        let quiet = made(&mut Hiss::new(1, &Params::default().with("level", 0.01)), RATE as usize);
        assert!((rms(&quiet) - 0.01).abs() < 0.0005);
    }
}
