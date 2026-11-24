// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Spend additional native solver intervals while a rig is loading a lift.
use crate::{Contact, Part, Pose};
use anyhow::{Result, ensure};
use glam::Vec3;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ResponseSpan {
    /// Zero-based on the native substep clock, before local refinement.
    pub start_substep: u64,
    pub substeps: u64,
    pub divisions: u32,
}

#[derive(Debug, Serialize)]
pub struct LiftResponse {
    pub target_travel_m: f32,
    pub extra_intervals: u64,
    pub refined_spans: Vec<ResponseSpan>,
    #[serde(skip)]
    masses: Vec<f64>,
    #[serde(skip)]
    total_mass: f64,
    #[serde(skip)]
    last_loading_step: Option<u64>,
    #[serde(skip)]
    witnesses: Vec<Contact>,
}

impl LiftResponse {
    pub(super) fn new(parts: &[Part], target_travel_m: f32) -> Result<Self> {
        ensure!(target_travel_m.is_finite() && target_travel_m > 0., "invalid lift response travel");
        ensure!(
            !parts.is_empty() && parts.iter().all(|p| p.mass.is_finite() && p.mass > 0.),
            "invalid lift response masses"
        );
        Ok(Self {
            target_travel_m,
            extra_intervals: 0,
            refined_spans: Vec::new(),
            masses: parts.iter().map(|p| f64::from(p.mass)).collect(),
            total_mass: parts.iter().map(|p| f64::from(p.mass)).sum(),
            last_loading_step: None,
            witnesses: Vec::new(),
        })
    }

    pub(super) fn start(&mut self, step: u64, substeps_per_tick: u8) {
        self.last_loading_step = Some(step + u64::from(substeps_per_tick));
    }

    pub(super) fn is_loading(&self, step: u64) -> bool {
        self.last_loading_step.is_some_and(|end| step <= end)
    }

    pub(super) fn observe(&mut self, contacts: &[Contact], surface: usize) {
        self.witnesses.clear();
        self.witnesses.extend(contacts.iter().filter(|c| c.surface == surface && c.separation <= 0.).copied());
    }

    pub(super) fn divisions(&self, poses: &[Pose], platform: Pose, dt: f32, limit: u32) -> u32 {
        if self.witnesses.is_empty() || limit <= 1 {
            return 1;
        }
        let momentum = poses
            .iter()
            .zip(&self.masses)
            .map(|(p, m)| Vec3::from(p.linear_velocity).as_dvec3() * *m)
            .sum::<glam::DVec3>();
        let centre_velocity = (momentum / self.total_mass).as_vec3();
        let mut closing = 0_f32;
        for c in &self.witnesses {
            let at = Vec3::from(c.point);
            let n = Vec3::from(c.normal).normalize_or_zero();
            let limb = &poses[c.part];
            let velocity_at = |p: &Pose| {
                Vec3::from(p.linear_velocity) + Vec3::from(p.angular_velocity).cross(at - Vec3::from(p.center))
            };
            let surface_v = velocity_at(&platform);
            // The paw may already be nearly motionless while the trunk is still falling.
            closing = closing.max((surface_v - velocity_at(limb)).dot(n)).max((surface_v - centre_velocity).dot(n));
        }
        ((closing * dt / self.target_travel_m).ceil() as u32).clamp(1, limit)
    }

    pub(super) fn record(&mut self, step: u64, divisions: u32) {
        if divisions <= 1 {
            return;
        }
        self.extra_intervals += u64::from(divisions - 1);
        if let Some(last) = self.refined_spans.last_mut()
            && last.start_substep + last.substeps == step
            && last.divisions == divisions
        {
            last.substeps += 1;
        } else {
            self.refined_spans.push(ResponseSpan { start_substep: step, substeps: 1, divisions });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stationary_paw_can_still_receive_load_from_the_trunk() {
        let paw = Part::between("paw", None, [[0.; 3], [0., 1., 0.]], 0.1, 1., [[0.; 2]; 3]);
        let mut trunk = paw.clone();
        trunk.mass = 3.;
        let mut response = LiftResponse::new(&[paw, trunk], 1. / 128.).unwrap();
        let still = Pose {
            center: [0.; 3],
            rotation_xyzw: [0., 0., 0., 1.],
            linear_velocity: [0.; 3],
            angular_velocity: [0.; 3],
        };
        let falling = Pose { linear_velocity: [0., -4., 0.], ..still };
        let contact = Contact {
            part: 0,
            surface: 2,
            surface_triangle: None,
            substep: 1,
            separation: 0.,
            point: [1., 0., 0.],
            normal: [0., 1., 0.],
            applied_offset_s: None,
        };
        response.observe(&[contact], 2);
        let dt = 1. / 64.;
        assert_eq!(response.divisions(&[still, falling], still, dt, 8), 6);
        assert_eq!(response.divisions(&[still, falling], still, dt, 4), 4);
        let spinning = Pose { angular_velocity: [0., 0., -2.], ..still };
        assert_eq!(response.divisions(&[spinning, still], still, dt, 8), 4);
        assert_eq!(response.divisions(&[still, still], falling, dt, 8), 1);
        response.observe(&[Contact { surface: 1, ..contact }, Contact { separation: 0.001, ..contact }], 2);
        assert_eq!(response.divisions(&[still, falling], still, dt, 8), 1);
        for (step, count) in [2, 2, 1, 2, 4].into_iter().enumerate() {
            response.record(step as u64, count);
        }
        assert_eq!(response.extra_intervals, 6);
        let spans: Vec<_> = response.refined_spans.iter().map(|s| (s.start_substep, s.substeps, s.divisions)).collect();
        assert_eq!(spans, [(0, 2, 2), (3, 1, 2), (4, 1, 4)]);
    }
}
