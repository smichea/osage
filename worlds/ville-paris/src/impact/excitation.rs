// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The attack of one material impact. Each positive reaction changes the gain at its measured
//! time. A later load on the same foot is part of the ongoing support of the body.
use osage_paris_voices::RATE;
use serde::Serialize;

/// A short material attack comparable to the few-millisecond push of the open impact voice.
pub(super) const WINDOW: u64 = 4 * RATE as u64 / 1000;
/// Artistic reference for the normalized renderer. The proxy is not measured acoustic energy.
const REFERENCE_WORK_J: f64 = 25.;
const SMOOTH_FRAMES: u32 = RATE / 2000;

#[derive(Clone, Serialize)]
pub(super) struct Sample {
    pub offset_frames: u64,
    impulse_ns: f32,
    cumulative_impulse_ns: f64,
    work_proxy_j: f64,
    pub amplitude: f32,
    attribution: &'static str,
    members: Vec<usize>,
    impulse_basis: &'static str,
}

#[derive(Clone, Serialize)]
pub(super) struct Excitation {
    window_frames: u64,
    reference_work_j: f64,
    incoming_speed_m_s: f32,
    pub samples: Vec<Sample>,
}

impl Excitation {
    pub fn new(speed: f32) -> Self {
        Self {
            window_frames: WINDOW,
            reference_work_j: REFERENCE_WORK_J,
            incoming_speed_m_s: speed,
            samples: Vec::new(),
        }
    }

    pub fn add(
        &mut self,
        offset: u64,
        impulse: f32,
        attribution: &'static str,
        members: Vec<usize>,
        impulse_basis: &'static str,
    ) -> f32 {
        let cumulative = self.samples.last().map_or(0., |s| s.cumulative_impulse_ns) + f64::from(impulse);
        let work = 0.5 * cumulative * f64::from(self.incoming_speed_m_s);
        let amplitude = (work / (work + REFERENCE_WORK_J)).sqrt() as f32;
        self.samples.push(Sample {
            offset_frames: offset,
            impulse_ns: impulse,
            cumulative_impulse_ns: cumulative,
            work_proxy_j: work,
            amplitude,
            attribution,
            members,
            impulse_basis,
        });
        amplitude
    }
}

/// A propagation path reads the same source envelope at its own delay. The first measured
/// sample is immediately available. Later measurements ramp over half a millisecond.
pub(super) struct Envelope {
    pub source: (usize, u64),
    changes: Vec<(u64, f32)>,
    next: usize,
    gain: f32,
    target: f32,
    increment: f32,
    remaining: u32,
}

impl Envelope {
    pub fn new(source: (usize, u64), gain: f32) -> Self {
        Self { source, changes: Vec::new(), next: 0, gain, target: gain, increment: 0., remaining: 0 }
    }

    pub fn add(&mut self, offset: u64, amplitude: f32) {
        self.changes.push((offset, amplitude));
    }

    pub fn apply(&mut self, from: u64, samples: &mut [f32]) {
        if self.next == self.changes.len() && self.remaining == 0 {
            for v in samples {
                *v *= self.gain;
            }
            return;
        }
        for (i, v) in samples.iter_mut().enumerate() {
            let offset = from + i as u64;
            while let Some(&(at, target)) = self.changes.get(self.next)
                && at <= offset
            {
                self.next += 1;
                self.target = target;
                self.remaining = SMOOTH_FRAMES;
                self.increment = (self.target - self.gain) / self.remaining as f32;
            }
            if self.remaining != 0 {
                self.gain += self.increment;
                self.remaining -= 1;
                if self.remaining == 0 {
                    self.gain = self.target;
                }
            }
            *v *= self.gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_charge_has_one_bounded_gain_and_future_samples_do_not_change_the_past() {
        let charge = |amounts: &[f32]| {
            let mut e = Excitation::new(6.);
            for (i, &p) in amounts.iter().enumerate() {
                e.add(i as u64, p, "resolved", vec![1], "normal impulse");
            }
            e.samples.last().unwrap().amplitude
        };
        assert_eq!(charge(&[16.]), charge(&[1., 3., 4., 8.]));
        assert!(charge(&[4.]) < charge(&[8.]) && charge(&[1e20]) <= 1.);
        let render = |cuts: &[usize]| {
            let mut e = Envelope::new((1, 0), 0.125);
            let mut out = [1.; 100];
            e.add(40, 0.75);
            let mut at = 0;
            for &end in cuts {
                e.apply(at as u64, &mut out[at..end]);
                at = end;
            }
            out
        };
        let out = render(&[100]);
        assert_eq!(out, render(&[1, 39, 40, 41, 55, 100]));
        assert_eq!(out[..40], [0.125; 40]);
        assert!(out[40] > 0.125 && out[40] < 0.75);
        assert_eq!(out[63..], [0.75; 37]);
    }
}
