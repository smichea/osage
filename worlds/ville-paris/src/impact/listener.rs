// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The prototype's ear model applied to a direct impact at its reception pose.
use osage_paris_soundscape::{
    ears::{HEAD_RADIUS, Heard, Listening, PINNA},
    ways::SOUND,
};
use osage_paris_voices::RATE;

pub(super) enum Listener {
    Speakers([f32; 2]),
    Headphones(Box<Headphones>),
}

impl Listener {
    pub(super) fn new(mode: Listening, heard: Heard, gain: f64) -> Self {
        match mode {
            Listening::Speakers => Self::Speakers(heard.pan.map(|p| (p * gain) as f32)),
            Listening::Headphones => Self::Headphones(Box::new(Headphones::new(heard, gain as f32))),
        }
    }

    pub(super) fn add(&mut self, mono: &[f32], stereo: &mut [[f32; 2]]) {
        match self {
            Self::Speakers(gain) => {
                for (out, x) in stereo.iter_mut().zip(mono) {
                    out[0] += x * gain[0];
                    out[1] += x * gain[1];
                }
            }
            Self::Headphones(ears) => {
                for (out, &x) in stereo.iter_mut().zip(mono) {
                    ears.add(x, out);
                }
            }
        }
    }
}

// At 48 kHz the ear-to-ear path needs at most 32 samples, and the outer
// ear's longest echo needs 20. Each line also has a preceding sample for interpolation.
const HISTORY: usize = 64;

#[derive(Clone, Copy)]
struct Tap {
    whole: usize,
    fraction: f32,
}

impl Tap {
    fn new(seconds: f64) -> Self {
        let samples = seconds * f64::from(RATE);
        let whole = samples.floor() as usize;
        assert!(whole + 1 < HISTORY);
        Self { whole, fraction: (samples - whole as f64) as f32 }
    }

    fn read(self, history: &[f32; HISTORY], cursor: usize) -> f32 {
        let now = cursor.wrapping_sub(self.whole) % HISTORY;
        let before = now.wrapping_sub(1) % HISTORY;
        history[now] + (history[before] - history[now]) * self.fraction
    }
}

pub(super) struct Headphones {
    gain: f32,
    pinna: [Tap; 5],
    ears: [Tap; 2],
    b0: [f32; 2],
    b1: [f32; 2],
    feedback: f32,
    dry: [f32; HISTORY],
    reflected: [f32; HISTORY],
    previous_in: [f32; 2],
    previous_out: [f32; 2],
    cursor: usize,
}

impl Headphones {
    fn new(heard: Heard, gain: f32) -> Self {
        // Bilinear transform of the prototype's one-pole head-shadow filter.
        let beta = 2. * SOUND / HEAD_RADIUS;
        let rate2 = 2. * f64::from(RATE);
        Self {
            gain,
            pinna: heard.pinna.map(Tap::new),
            ears: heard.delay.map(Tap::new),
            b0: heard.shadow.map(|a| ((beta + rate2 * a) / (beta + rate2)) as f32),
            b1: heard.shadow.map(|a| ((beta - rate2 * a) / (beta + rate2)) as f32),
            feedback: ((rate2 - beta) / (beta + rate2)) as f32,
            dry: [0.; HISTORY],
            reflected: [0.; HISTORY],
            previous_in: [0.; 2],
            previous_out: [0.; 2],
            cursor: 0,
        }
    }

    fn add(&mut self, x: f32, out: &mut [f32; 2]) {
        self.dry[self.cursor] = x;
        let mut reflected = x;
        for (tap, (gain, ..)) in self.pinna.iter().zip(PINNA) {
            reflected += gain as f32 * tap.read(&self.dry, self.cursor);
        }
        self.reflected[self.cursor] = reflected;
        for (ear, y) in out.iter_mut().enumerate() {
            let input = self.ears[ear].read(&self.reflected, self.cursor);
            let filtered =
                self.b0[ear] * input + self.b1[ear] * self.previous_in[ear] + self.feedback * self.previous_out[ear];
            self.previous_in[ear] = input;
            self.previous_out[ear] = filtered;
            *y += filtered * self.gain;
        }
        self.cursor = (self.cursor + 1) % HISTORY;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_shadow_preserves_constant_pressure_and_matches_the_high_frequency_gain_of_each_ear() {
        // Coincident echoes have zero net gain. Removing the ear delays isolates the head filter.
        let heard = Heard { pan: [0., 1.], delay: [0.; 2], pinna: [0.; 5], shadow: [0.28, 2.] };
        let mut ears = Headphones::new(heard, 1.);
        let mut pressure = [0.; 2];
        for _ in 0..512 {
            pressure = [0.; 2];
            ears.add(1., &mut pressure);
        }
        assert!(pressure.iter().all(|p| (p - 1.).abs() < 1e-5));
        for _ in 0..512 {
            ears.add(-1., &mut [0.; 2]);
            pressure = [0.; 2];
            ears.add(1., &mut pressure);
        }
        for (amplitude, high_gain) in pressure.into_iter().zip(heard.shadow) {
            assert!((f64::from(amplitude) - high_gain).abs() < 1e-5);
        }
    }
}
