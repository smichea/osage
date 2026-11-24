// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

use glam::Vec3;
use serde::Serialize;

/// Actor part and touched surface, using the input order of [`crate::Simulation`].
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ContactBodies {
    pub part: usize,
    pub surface: usize,
}

/// A solver point or a unique observed pair can identify both participants. A momentum balance
/// can also cover several simultaneous candidates, or a step without a reported witness.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImpulseAttribution {
    Pair(ContactBodies),
    Unresolved(Vec<ContactBodies>),
}

/// Impulse in N·s on the actor, in world coordinates. A solver normal omits tangential friction.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImpulseMeasurement {
    /// Accumulated normal impulse of one active Rapier solver point in a native solve.
    SolverNormal(f32),
    /// Full impulse applied at the witness by Osage's isolated sticking-contact solve.
    AppliedVector([f32; 3]),
    /// Change of the actor's total linear momentum, after subtracting applied gravity.
    /// Includes friction and numerical residuals of the native integration and its constraints.
    MomentumBalance([f32; 3]),
}

/// Post-solve physical observation from [`crate::Simulation::step_with_impulses`].
/// The times bound the integration interval inside the public tick; a directly applied impulse
/// has equal bounds. `point` is on the surface. A multi-point balance can have no unique witness.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ContactImpulse {
    pub substep: u8,
    pub start_offset_s: f32,
    pub end_offset_s: f32,
    pub attribution: ImpulseAttribution,
    pub point: Option<[f32; 3]>,
    /// Unit direction from the surface towards the actor when there is a common contact normal.
    pub normal: Option<[f32; 3]>,
    pub measurement: ImpulseMeasurement,
}

impl ContactImpulse {
    pub fn pair(&self) -> Option<ContactBodies> {
        match self.attribution {
            ImpulseAttribution::Pair(pair) => Some(pair),
            ImpulseAttribution::Unresolved(_) => None,
        }
    }

    /// Signed projection on the unique contact normal. Callers choose their own detection threshold.
    pub fn normal_impulse_ns(&self) -> Option<f32> {
        let normal = Vec3::from_array(self.normal?);
        match self.measurement {
            ImpulseMeasurement::SolverNormal(value) => Some(value),
            ImpulseMeasurement::AppliedVector(vector) | ImpulseMeasurement::MomentumBalance(vector) => {
                Some(normal.dot(Vec3::from_array(vector)))
            }
        }
    }
}

/// Reborrowed for each native advance, including intervals introduced by relative casts.
#[derive(Clone, Copy)]
pub(super) struct Interval {
    pub substep: u8,
    pub start: f32,
    pub end: f32,
}

impl Interval {
    pub(super) fn record(
        self,
        attribution: ImpulseAttribution,
        point: Option<[f32; 3]>,
        normal: Option<[f32; 3]>,
        measurement: ImpulseMeasurement,
    ) -> ContactImpulse {
        ContactImpulse {
            substep: self.substep,
            start_offset_s: self.start,
            end_offset_s: self.end,
            attribution,
            point,
            normal,
            measurement,
        }
    }
}

pub(super) struct Observation<'a> {
    pub time: Interval,
    pub output: Option<&'a mut Vec<ContactImpulse>>,
}
