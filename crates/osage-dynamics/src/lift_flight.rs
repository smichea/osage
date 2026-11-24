// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! First inward contact of an airborne capsule with a prescribed vertical lift.
use crate::{Lift, Part, Pose, free_pair::spine_box_points};
use anyhow::{Result, ensure};
use glam::{DQuat, DVec3};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct LiftHit {
    /// Elapsed seconds from the supplied pose.
    pub seconds: f64,
    pub point_m: [f64; 3],
    pub normal: [f64; 3],
    pub closing_m_s: f64,
}

/// Gravity acts along -Y until the downward terminal speed. Angular velocity is
/// constant in world coordinates until an interaction changes the motion.
#[derive(Clone, Copy)]
pub struct FlightClock {
    pub start_s: f64,
    pub duration_s: f64,
    pub gravity_m_s2: f64,
    pub terminal_m_s: f64,
}

struct Sweep {
    lift: Lift,
    clock: FlightClock,
    center: DVec3,
    velocity: DVec3,
    arm: DVec3,
    spin: DVec3,
    radius: f64,
    acceleration_bound: f64,
}

struct Witness {
    gap: f64,
    normal: DVec3,
    point: DVec3,
    translation_speed: f64,
    rotation_speed_bound: f64,
    closing: f64,
}

impl Sweep {
    fn at(&self, t: f64) -> Witness {
        let g = self.clock.gravity_m_s2;
        let terminal = self.clock.terminal_m_s;
        let accelerating = if g > 0. { ((self.velocity.y + terminal) / g).min(t) } else { t };
        let mut center = self.center + self.velocity * t;
        center.y = self.center.y + self.velocity.y * accelerating
            - 0.5 * g * accelerating * accelerating
            - terminal * (t - accelerating);
        let mut velocity = self.velocity;
        velocity.y -= g * accelerating;
        let mut platform = DVec3::from(self.lift.center_m.map(f64::from));
        let phase = self.lift.phase(self.clock.start_s + t);
        platform.y += 0.5 * f64::from(self.lift.travel_m) * (1. - phase.cos());
        velocity.y -= self.lift.peak_speed() * phase.sin();
        let axis = DQuat::from_scaled_axis(self.spin * t) * self.arm;
        let (spine, surface) =
            spine_box_points(center - platform, axis, DVec3::from(self.lift.half_extents_m.map(f64::from)));
        let delta = spine - surface;
        let distance = delta.length();
        let normal = delta.normalize_or_zero();
        let translation_speed = velocity.dot(normal);
        let closing = -(translation_speed + self.spin.cross(spine - (center - platform)).dot(normal));
        Witness {
            gap: distance - self.radius,
            point: platform + surface,
            normal,
            translation_speed,
            rotation_speed_bound: self.spin.cross(self.arm).length()
                * normal.cross(self.spin.normalize_or_zero()).length(),
            closing,
        }
    }

    fn interval(&self, lo: f64, hi: f64) -> Option<LiftHit> {
        let mid = lo + (hi - lo) * 0.5;
        let w = self.at(mid);
        let half = (hi - lo) * 0.5;
        // Project the entire interval on the separating plane at its midpoint.
        // Translation has bounded vertical acceleration. Each spine endpoint's
        // rotational travel is bounded by the arc it follows in the interval.
        let shift = (w.translation_speed.abs() + w.rotation_speed_bound) * half
            + 0.5 * self.acceleration_bound * w.normal.y.abs() * half * half;
        if w.gap > shift || (shift == 0. && w.gap >= 0.) {
            return None;
        }
        if hi - lo <= 1e-8 || mid == lo || mid == hi {
            for t in [lo, mid, hi] {
                let w = self.at(t);
                if w.gap <= 0. && w.closing > 0. {
                    return Some(LiftHit {
                        seconds: t,
                        point_m: w.point.to_array(),
                        normal: w.normal.to_array(),
                        closing_m_s: w.closing,
                    });
                }
            }
            return None;
        }
        self.interval(lo, mid).or_else(|| self.interval(mid, hi))
    }
}

impl Lift {
    /// Forecast the first inward crossing of an initially separated sphere or capsule.
    /// Start time and duration are in seconds on the prescribed lift's clock.
    pub fn first_impact(&self, part: &Part, pose: Pose, clock: FlightClock) -> Result<Option<LiftHit>> {
        ensure!(
            self.valid()
                && clock.start_s.is_finite()
                && clock.duration_s.is_finite()
                && clock.duration_s >= 0.
                && (clock.start_s + clock.duration_s).is_finite()
                && clock.gravity_m_s2.is_finite()
                && clock.gravity_m_s2 >= 0.
                && clock.terminal_m_s.is_finite()
                && clock.terminal_m_s > 0.,
            "invalid lift flight clock or platform"
        );
        let rotation = DQuat::from_array(pose.rotation_xyzw.map(f64::from));
        let center = DVec3::from(pose.center.map(f64::from));
        let mut velocity = DVec3::from(pose.linear_velocity.map(f64::from));
        let spin = DVec3::from(pose.angular_velocity.map(f64::from));
        ensure!(
            center.is_finite()
                && velocity.is_finite()
                && spin.is_finite()
                && rotation.is_finite()
                && (rotation.length_squared() - 1.).abs() < 1e-4
                && part.radius.is_finite()
                && part.radius > 0.
                && part.half_segment.is_finite()
                && part.half_segment >= 0.,
            "invalid lift flight body"
        );
        velocity.y = velocity.y.max(-clock.terminal_m_s);
        let acceleration_bound =
            clock.gravity_m_s2 + self.peak_speed() * std::f64::consts::TAU / f64::from(self.period_s);
        let sweep = Sweep {
            lift: *self,
            clock,
            center,
            velocity,
            spin,
            arm: rotation.normalize() * DVec3::Y * f64::from(part.half_segment),
            radius: f64::from(part.radius),
            acceleration_bound,
        };
        let end = center + velocity * clock.duration_s;
        ensure!(
            end.is_finite()
                && (spin.length() * clock.duration_s).is_finite()
                && (acceleration_bound * clock.duration_s * clock.duration_s).is_finite(),
            "lift flight outside numerical range"
        );
        // The lift only moves on Y. This rejects entire flights that pass beside it.
        for axis in [0, 2] {
            let reach = f64::from(self.half_extents_m[axis]) + sweep.radius + sweep.arm.length();
            let origin = f64::from(self.center_m[axis]);
            if center[axis].min(end[axis]) > origin + reach || center[axis].max(end[axis]) < origin - reach {
                return Ok(None);
            }
        }
        let initial = sweep.at(0.);
        if initial.gap <= 0. {
            return Ok(None);
        }
        Ok(sweep.interval(0., clock.duration_s))
    }
}

/// Clock and measurements of the first predicted approach. Times in this report are
/// absolute simulation seconds, except for the per-body flight hit's relative time.
#[derive(Debug, Serialize)]
pub struct LiftApproach {
    pub start_s: f64,
    pub contact_hz: u32,
    pub predicted_contacts: Vec<PredictedContact>,
    pub first_manifold: Option<ManifoldTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<crate::lift_response::LiftResponse>,
    pub interrupted_at_s: Option<f64>,
    pub extra_intervals: u64,
}

#[derive(Debug, Serialize)]
pub struct PredictedContact {
    pub part: usize,
    #[serde(flatten)]
    pub hit: LiftHit,
}

#[derive(Debug, Serialize)]
pub struct ManifoldTime {
    pub start_s: f64,
    pub end_s: f64,
    pub part: usize,
    pub point_m: [f32; 3],
    pub separation_m: f32,
}

impl LiftApproach {
    pub(super) fn intervals(&self, start: f64, base_hz: u32) -> u32 {
        let Some(contact) = self.predicted_contacts.first() else { return 1 };
        let dt = 1. / f64::from(base_hz);
        let impact = self.start_s + contact.hit.seconds;
        if self.first_manifold.is_none()
            && self.interrupted_at_s.is_none()
            && start <= impact + 2. * dt
            && start + dt >= impact - 2. * dt
        {
            self.contact_hz.div_ceil(base_hz).max(1)
        } else {
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn platform() -> Lift {
        Lift { center_m: [0.; 3], half_extents_m: [0.5, 0.125, 0.5], travel_m: 1., period_s: 1., friction: 0.5 }
    }
    fn sphere() -> Part {
        Part::between("body", None, [[0., 0., 0.], [0., 0.125, 0.]], 0.125, 1., [[0.; 2]; 3])
    }
    fn start(center: [f32; 3]) -> Pose {
        Pose { center, rotation_xyzw: [0., 0., 0., 1.], linear_velocity: [0.; 3], angular_velocity: [0.; 3] }
    }
    fn clock() -> FlightClock {
        FlightClock { start_s: 0., duration_s: 1., gravity_m_s2: 0., terminal_m_s: 10. }
    }

    #[test]
    fn finds_a_lift_that_passes_through_and_returns_within_one_interval() {
        let lift = platform();
        let p = start([0., 0.75, 0.]);
        let hit = lift.first_impact(&sphere(), p, clock()).unwrap().unwrap();
        assert!((hit.seconds - 0.25).abs() < 1e-8);
        assert!(DVec3::from(hit.normal).distance(DVec3::Y) < 1e-12);
        assert!((hit.closing_m_s - std::f64::consts::PI).abs() < 1e-8);
        let shifted = FlightClock { start_s: 0.125, ..clock() };
        assert!((lift.first_impact(&sphere(), p, shifted).unwrap().unwrap().seconds - 0.125).abs() < 1e-8);
        assert!(lift.first_impact(&sphere(), start([1., 0.75, 0.]), clock()).unwrap().is_none());
    }

    #[test]
    fn terminal_flight_and_a_rounded_edge_return_their_common_witness() {
        let lift = platform();
        let pose = Pose { linear_velocity: [0., -50., 0.], ..start([0.575, 3., 0.]) };
        let cfg = FlightClock { terminal_m_s: 4., gravity_m_s2: 9.81, ..clock() };
        let hit = lift.first_impact(&sphere(), pose, cfg).unwrap().unwrap();
        assert!((hit.normal[0] - 0.6).abs() < 1e-5 && (hit.normal[1] - 0.8).abs() < 1e-5);
        let mut center = DVec3::from(pose.center.map(f64::from));
        center.y -= 4. * hit.seconds;
        assert!((center.distance(DVec3::from(hit.point_m)) - 0.125).abs() < 1e-7);
        assert!((hit.point_m[0] - 0.5).abs() < 1e-12);
    }

    #[test]
    fn spinning_capsule_can_hit_with_both_endpoint_poses_clear() {
        let lift = Lift { travel_m: 0.001, ..platform() };
        let part = Part { half_segment: 0.5, ..sphere() };
        let pose = Pose { angular_velocity: [0., 0., std::f32::consts::TAU], ..start([1., 0., 0.]) };
        let hit = lift.first_impact(&part, pose, clock()).unwrap().unwrap();
        assert!(hit.seconds > 0. && hit.seconds < 0.25 && hit.normal[0] > 0.);
        assert!(lift.first_impact(&part, start(pose.center), clock()).unwrap().is_none());
        assert!(lift.first_impact(&part, pose, FlightClock { duration_s: -1., ..clock() }).is_err());
    }
}
