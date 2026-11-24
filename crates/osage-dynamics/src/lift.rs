// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

use crate::Pose;
use serde::{Deserialize, Serialize};

/// A level kinematic box. Its centre starts at the bottom of the stroke and
/// travels vertically at y(t) = center_y + travel_m × (1 − cos(2πt / period_s)) / 2.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Lift {
    pub center_m: [f32; 3],
    pub half_extents_m: [f32; 3],
    pub travel_m: f32,
    pub period_s: f32,
    pub friction: f32,
}

impl Lift {
    pub fn valid(&self) -> bool {
        self.center_m.into_iter().all(f32::is_finite)
            && self.half_extents_m.into_iter().all(|half| half.is_finite() && half > 0.)
            && self.travel_m.is_finite()
            && self.travel_m > 0.
            && self.period_s.is_finite()
            && self.period_s > 0.
            && (self.center_m[1] + self.travel_m).is_finite()
            && (self.peak_speed() as f32).is_finite()
            && self.friction.is_finite()
            && self.friction >= 0.
    }

    fn phase(&self, time_s: f64) -> f64 {
        let period = f64::from(self.period_s);
        std::f64::consts::TAU * (time_s.rem_euclid(period) / period)
    }

    pub(super) fn peak_speed(&self) -> f64 {
        std::f64::consts::PI * f64::from(self.travel_m) / f64::from(self.period_s)
    }

    /// Prescribed world centre in metres at absolute simulation time in seconds.
    pub fn center_at(&self, time_s: f64) -> [f32; 3] {
        let [x, y, z] = self.center_m;
        let height = f64::from(y) + 0.5 * f64::from(self.travel_m) * (1. - self.phase(time_s).cos());
        [x, height as f32, z]
    }

    pub(super) fn pose_at(&self, time_s: f64) -> Pose {
        Pose {
            center: self.center_at(time_s),
            rotation_xyzw: [0., 0., 0., 1.],
            linear_velocity: [0., (self.peak_speed() * self.phase(time_s).sin()) as f32, 0.],
            angular_velocity: [0.; 3],
        }
    }
}
