// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use bevy::prelude::*;

/// Phase of the single clip currently driving this rig, in cycles.
#[derive(Component)]
pub struct FootCyclePhase(pub f32);

/// Horizontal corrections for one foot of a looping, in-place clip at its nominal travel speed.
#[derive(Component, Clone)]
#[component(immutable)]
pub struct FootCycle {
    offsets: std::sync::Arc<[Vec2]>,
}

impl FootCycle {
    /// Equally spaced actor-local contact positions, excluding the repeated endpoint.
    /// Travel is the actor-local X/Z displacement over one complete cycle, in metres.
    pub fn from_samples(samples: &[Vec3], travel: Vec2, band: f32) -> Option<Self> {
        let n = samples.len();
        if n < 3 || !band.is_finite() || band < 0. || !travel.is_finite() || !samples.iter().all(|p| p.is_finite()) {
            return None;
        }
        // Start at a landing so a stance crossing phase zero stays in one unwrapped interval.
        let first = (0..n).find(|&i| samples[i].y <= band && samples[(i + n - 1) % n].y > band)?;
        let slot = |step: usize| (first + step) % n;
        let position = |step: usize| samples[slot(step)].xz() + travel * (step as f32 / n as f32);
        let mut offsets = vec![Vec2::ZERO; n];
        let mut swings = Vec::new();
        let mut step = 0;
        while step < n {
            let start = step;
            let mut lo = position(step);
            let mut hi = lo;
            while step < n && samples[slot(step)].y <= band {
                lo = lo.min(position(step));
                hi = hi.max(position(step));
                step += 1;
            }
            let anchor = lo.midpoint(hi);
            for i in start..step {
                offsets[slot(i)] = anchor - position(i);
            }
            let lift = step - 1;
            while step < n && samples[slot(step)].y > band {
                step += 1;
            }
            swings.push((lift, step));
        }
        for (lift, land) in swings {
            let from = offsets[slot(lift)];
            let to = offsets[slot(land)];
            for i in lift + 1..land {
                let t = (i - lift) as f32 / (land - lift) as f32;
                offsets[slot(i)] = from.lerp(to, t * t * (3. - 2. * t));
            }
        }
        offsets.iter().all(|v| v.is_finite()).then(|| Self { offsets: offsets.into() })
    }

    /// Actor-local X/Z displacement, interpolated across the clip seam.
    pub fn offset(&self, phase: f32) -> Option<Vec2> {
        if !phase.is_finite() {
            return None;
        }
        let n = self.offsets.len();
        let u = phase.rem_euclid(1.) * n as f32;
        let i = u as usize % n;
        Some(self.offsets[i].lerp(self.offsets[(i + 1) % n], u.fract()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holds_a_stance_across_phase_zero_and_prepares_the_next_landing() {
        let samples = [Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Y, Vec3::ZERO];
        let travel = Vec2::X * 2.;
        let path = FootCycle::from_samples(&samples, travel, 0.).unwrap();
        let anchor = |i: usize| {
            let phase = i as f32 / 5.;
            samples[i % 5].xz() + phase * travel + path.offset(phase).unwrap()
        };
        assert!((anchor(4) - anchor(5)).length() < 1e-6);
        assert!((anchor(4) - anchor(6)).length() < 1e-6);
        let low = path.offset(0.2).unwrap().x;
        let high = path.offset(-0.2).unwrap().x;
        assert!(low < path.offset(0.4).unwrap().x && path.offset(0.6).unwrap().x < high);
        assert!(FootCycle::from_samples(&[Vec3::ZERO; 4], travel, 0.).is_none());
        assert!(path.offset(f32::NAN).is_none());
    }
}
