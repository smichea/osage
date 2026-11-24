// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Discover an airborne joint tree's first inward crossing of a box supported by fixed geometry.

use super::World;
use crate::{Settings, free_pair::spine_box_points};
use rapier3d::parry::query::{NonlinearRigidMotion, ShapeCastStatus, cast_shapes_nonlinear};
use rapier3d::prelude::*;

#[derive(Clone, Copy)]
pub(super) struct SupportedImpact {
    local_normal: Vector,
    compressing: bool,
}

struct CrossingShapes<'a> {
    support: &'a NonlinearRigidMotion,
    limb: &'a NonlinearRigidMotion,
    extents: glam::DVec3,
    axis: Vector,
    radius: f64,
}

impl CrossingShapes<'_> {
    /// Signed distance and material arms. Both Osage colliders are centred at their mass centres.
    fn witness(&self, t: f32) -> (f32, Vector, Vector, Vector) {
        let b = self.support.position_at_time(t);
        let p = self.limb.position_at_time(t);
        let inverse = b.rotation.as_dquat().inverse();
        let center = inverse * (p.translation.as_dvec3() - b.translation.as_dvec3());
        let axis = inverse * (p.rotation * self.axis).as_dvec3();
        let (spine, surface) = spine_box_points(center, axis, self.extents);
        let delta = spine - surface;
        let normal = delta.normalize_or_zero();
        let limb_arm = b.rotation * (spine - normal * self.radius - center).as_vec3();
        ((delta.length() - self.radius) as f32, b.rotation * normal.as_vec3(), limb_arm, b.rotation * surface.as_vec3())
    }

    fn inward_crossing(
        &self,
        mut time: f32,
        dt: f32,
        slop: f32,
        gravity: Vector,
        body: &RigidBody,
        support: &RigidBody,
    ) -> Option<(f32, Vector)> {
        // Support mapping at near-parallel faces can report a spurious gap.
        // Check the cast against the exact segment-to-box geometry.
        let (gap, normal, limb_arm, box_arm) = self.witness(time);
        let limb_velocity = body.linvel() + gravity * time + body.angvel().cross(limb_arm);
        let box_velocity = support.linvel() + support.angvel().cross(box_arm);
        let closing = -(limb_velocity - box_velocity).dot(normal);
        if closing * dt <= slop {
            return None;
        }
        let scale = self.support.start.translation.abs().max(self.limb.start.translation.abs()).max_element().max(1.);
        let tolerance = (32. * f32::EPSILON * scale).min(slop);
        if gap.abs() > tolerance {
            // One Newton correction on the inward branch. A sign bracket below
            // rejects a miss, a tangent, or an unreliable angular forecast.
            time += gap / closing;
        }
        if time < 1.0e-6 || dt - time < 1.0e-6 {
            return None;
        }
        let guard = (2. * tolerance / closing).max(2e-6);
        if self.witness((time - guard).max(0.)).0 <= 0. || self.witness((time + guard).min(dt)).0 >= 0. {
            return None;
        }
        Some((time, normal))
    }
}

impl World {
    pub(super) fn carries_fixed_reaction(&self, collider: ColliderHandle) -> bool {
        self.sim.contact_pairs_with(collider).any(|pair| {
            let (other, upward) = if pair.collider2 == collider {
                (pair.collider1, -self.sim.gravity)
            } else {
                (pair.collider2, self.sim.gravity)
            };
            self.sim
                .colliders
                .get(other)
                .is_some_and(|c| c.parent().is_none_or(|body| self.sim.bodies[body].is_fixed()))
                && pair.total_impulse().dot(upward) > 0.
        })
    }

    // A rig falling past a finite edge can leave the box while folding through
    // the original impact plane. A new flight clears the slab along its old
    // material normal, with no reaction anywhere in the articulated tree.
    fn cleared_impact_slab(&self, shape: &Collider, extents: Vector, previous: Vector) -> bool {
        let n = shape.position().rotation * previous;
        let limit = extents.dot(previous.abs()) + self.sim.integration_parameters.prediction_distance();
        let mut above = true;
        let mut below = true;
        for &(_, co) in &self.parts {
            if self.sim.contact_pairs_with(co).any(|pair| pair.total_impulse_magnitude() > 0.) {
                return false;
            }
            let limb = &self.sim.colliders[co];
            let span = if let Some(c) = limb.shape().as_capsule() {
                c.radius + (limb.position().rotation * c.segment.b).dot(n).abs()
            } else {
                let Some(ball) = limb.shape().as_ball() else { return false };
                ball.radius
            };
            let distance = (limb.position().translation - shape.position().translation).dot(n);
            above &= distance - span > limit;
            below &= distance + span < -limit;
            if !above && !below {
                return false;
            }
        }
        true
    }

    /// During load transfer the foot may already be at rest against the face
    /// while the mass behind it is still falling into the joint chain.
    pub(crate) fn compression_substeps(&mut self, dt: f32) -> usize {
        let Some(SupportedImpact { local_normal, compressing: true }) = self.supported_impact else { return 1 };
        let [support] = self.boxes.as_slice() else { return 1 };
        if self.sim.bodies[self.parts[0].0].is_sleeping() {
            return 1;
        }
        let support = &self.sim.bodies[*support];
        let normal = support.rotation() * local_normal;
        let handle = self.box_collider(0);
        let loaded = self.sim.contact_pairs_with(handle).any(|pair| {
            // The native normal points from collider 1 to collider 2.
            let (other, direction) =
                if pair.collider1 == handle { (pair.collider2, normal) } else { (pair.collider1, -normal) };
            let limb = &self.sim.colliders[other];
            limb.parent().is_some_and(|h| self.sim.bodies[h].is_dynamic())
                && limb.restitution() == 0.
                && pair.total_impulse().dot(direction) > 0.
        });
        if !loaded || !self.carries_fixed_reaction(handle) {
            return 1;
        }

        let mut momentum = 0.;
        let mut mass = 0.;
        for &(id, _) in &self.parts {
            let part = &self.sim.bodies[id];
            let relative = support.velocity_at_point(part.center_of_mass()) - part.linvel();
            momentum += part.mass() * relative.dot(normal);
            mass += part.mass();
        }
        let travel = (momentum / mass) * dt;
        let parameters = &self.sim.integration_parameters;
        let tolerance = parameters.allowed_linear_error();
        let mut count = 1;
        // Bisect the native interval so every slice shares an exact binary
        // subdivision of dt. Use the existing articulated CCD work limit.
        while count <= parameters.max_ccd_substeps / 2 && travel > count as f32 * tolerance {
            count *= 2;
        }
        if count == 1 {
            // The load has slowed on the reacting face. Retain its normal for
            // flight detection, but leave subsequent rolling to the ordinary solver.
            self.supported_impact = Some(SupportedImpact { local_normal, compressing: false });
        }
        count
    }

    pub(crate) fn supported_impact_time(&mut self, cfg: Settings, dt: f32) -> Option<f32> {
        let [box_body] = self.boxes.as_slice() else { return None };
        let support = &self.sim.bodies[*box_body];
        if self.parts.len() < 2 || !support.is_dynamic() || self.sim.bodies[self.parts[0].0].is_sleeping() {
            return None;
        }
        let handle = self.box_collider(0);
        let shape = &self.sim.colliders[handle];
        if shape.restitution() > 0. {
            return None;
        }
        // Occupied supports retain native persistent contacts. Once the actor has
        // separated it can approach again, including from the other side.
        if self.sim.contact_pairs_with(handle).any(|pair| {
            let other = if pair.collider1 == handle { pair.collider2 } else { pair.collider1 };
            self.sim.colliders.get(other).and_then(|c| c.parent()).is_some_and(|p| self.sim.bodies[p].is_dynamic())
                && pair.has_any_active_contact()
        }) {
            return None;
        }
        let bounds = shape.compute_aabb();
        let extents = shape.shape().as_cuboid()?.half_extents;
        if let Some(previous) = self.supported_impact {
            if !self.cleared_impact_slab(shape, extents, previous.local_normal) {
                return None;
            }
            self.supported_impact = None;
        }
        let radius_of_support = shape.shape().compute_local_bounding_sphere().radius;
        let support_travel = dt * (support.linvel().length() + radius_of_support * support.angvel().length());
        let support_motion =
            NonlinearRigidMotion::new(*shape.position(), Vector::ZERO, support.linvel(), support.angvel());
        let mut checked_support = false;
        let mut earliest: Option<(f32, Vector)> = None;
        for &(id, co) in &self.parts {
            let body = &self.sim.bodies[id];
            let limb = &self.sim.colliders[co];
            let span = limb.shape().compute_local_bounding_sphere().radius;
            let travel =
                support_travel + dt * (body.linvel().length() + span * body.angvel().length() + cfg.gravity * dt);
            let aabb = limb.compute_aabb();
            if aabb.mins.cmpgt(bounds.maxs + Vector::splat(travel)).any()
                || aabb.maxs.cmplt(bounds.mins - Vector::splat(travel)).any()
            {
                continue;
            }
            if !checked_support {
                if !self.carries_fixed_reaction(handle) {
                    return None;
                }
                checked_support = true;
            }
            // The free-flight estimate uses midpoint velocity. Clamping vertical
            // speed or a shock from another surface invalidates that estimate.
            let motion = NonlinearRigidMotion::new(
                *limb.position(),
                Vector::ZERO,
                body.linvel() + self.sim.gravity * (0.5 * dt),
                body.angvel(),
            );
            let Ok(Some(hit)) =
                cast_shapes_nonlinear(&support_motion, shape.shape(), &motion, limb.shape(), 0., dt, true)
            else {
                continue;
            };
            let time = hit.time_of_impact;
            if !time.is_finite() || time < 1.0e-6 || dt - time < 1.0e-6 || limb.restitution() > 0. {
                return None;
            }
            let (axis, radius) = if let Some(capsule) = limb.shape().as_capsule() {
                (capsule.segment.b, capsule.radius)
            } else {
                (Vector::ZERO, limb.shape().as_ball()?.radius)
            };
            let shapes = CrossingShapes {
                support: &support_motion,
                limb: &motion,
                extents: extents.as_dvec3(),
                axis,
                radius: f64::from(radius),
            };
            let slop = self.sim.integration_parameters.allowed_linear_error();
            let spin_speed = span * body.angvel().length() + radius_of_support * support.angvel().length();
            if hit.status != ShapeCastStatus::Converged && spin_speed * dt > slop {
                continue;
            }
            let Some((time, normal)) = shapes.inward_crossing(time, dt, slop, self.sim.gravity, body, support) else {
                continue;
            };
            if earliest.is_none_or(|(previous, _)| time < previous) {
                earliest = Some((time, support_motion.position_at_time(time).rotation.inverse() * normal));
            }
        }
        let (time, normal) = earliest?;
        if self.parts.iter().any(|&(body, limb)| {
            let vy = self.sim.bodies[body].linvel().y;
            cfg.fall_limit(vy, dt) != vy || self.sim.contact_pairs_with(limb).any(|p| p.total_impulse_magnitude() > 0.)
        }) {
            return None;
        }
        self.supported_impact = Some(SupportedImpact { local_normal: normal, compressing: true });
        Some(time)
    }
}
