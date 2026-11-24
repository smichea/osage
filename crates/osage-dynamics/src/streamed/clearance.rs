// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Sufficient separation certificates for an upright capsule during one native fixed-terrain step.
use super::TerrainSimulation;
use glam::{DVec2, DVec3};
use rapier3d::prelude::*;

/// Horizontal motion of a centred, upright, nonspinning capsule. A terrain step must
/// independently confirm that its actual constraints preserve this motion.
#[derive(Clone, Copy)]
pub struct UprightSweep {
    start: DVec3,
    end: DVec3,
    half_segment: f64,
    radius: f64,
    prediction: f64,
    max_travel: f64,
    padding: f64,
}

impl UprightSweep {
    /// Broad phase for a speculative contact at the actual current pose.
    pub fn excludes_start_bounds(&self, bounds: [[f32; 3]; 2]) -> bool {
        let [lo, hi] = bounds.map(|a| DVec3::from(a.map(f64::from)));
        let extent = DVec3::Y * self.half_segment + DVec3::splat(self.radius + self.prediction + self.padding);
        hi.cmplt(self.start - extent).any() || lo.cmpgt(self.start + extent).any()
    }

    /// Even a native contact reaction is subject to the per-iteration speed cap.
    /// The start region also encloses the full scalar speculative contact distance.
    pub fn excludes_bounds(&self, bounds: [[f32; 3]; 2]) -> bool {
        let [min, max] = bounds.map(|a| DVec3::from(a.map(f64::from)));
        let horizontal = self.radius + self.prediction + self.padding;
        let extent = DVec3::new(horizontal, self.half_segment + horizontal + self.max_travel, horizontal);
        let low = self.start.min(self.end) - extent;
        let high = self.start.max(self.end) + extent;
        max.cmplt(low).any() || min.cmpgt(high).any()
    }

    /// Exclude both physical contact throughout the step and a speculative point at
    /// its beginning. Equality, including a tangency, keeps the page required.
    pub fn excludes_triangle(&self, triangle: [[f32; 3]; 3]) -> bool {
        let (physical, speculative) = self.separation(triangle);
        physical && speculative
    }

    pub fn excludes_speculation(&self, triangle: [[f32; 3]; 3]) -> bool {
        self.separation(triangle).1
    }

    fn separation(&self, triangle: [[f32; 3]; 3]) -> (bool, bool) {
        let relative = triangle.map(|v| DVec3::from(v.map(f64::from)) - self.start);
        let xz = |p: DVec3| DVec2::new(p.x, p.z);
        let vertices = relative.map(xz);
        let travel = xz(self.end - self.start);
        let edges = [vertices[1] - vertices[0], vertices[2] - vertices[1], vertices[0] - vertices[2]];
        let scale = relative.iter().fold(self.max_travel.max(1.), |s, v| s.max(v.abs().max_element()));
        let tolerance = self.padding + 128. * f64::EPSILON * scale;
        let mut start_gap: f64 = 0.;
        let mut swept_gap: f64 = 0.;
        // Edge normals and vertex pairs supply separating directions for a projected segment
        // and triangle. Only strict projection gaps can certify separation, including for
        // degenerate vertical faces whose horizontal projection is itself a segment.
        for axis in edges
            .map(|e| e.perp())
            .into_iter()
            .chain([travel.perp()])
            .chain(vertices)
            .chain(vertices.map(|p| p - travel))
        {
            let Some(axis) = axis.try_normalize() else { continue };
            let p = vertices.map(|v| v.dot(axis));
            let low = p[0].min(p[1]).min(p[2]);
            let high = p[0].max(p[1]).max(p[2]);
            let shift = travel.dot(axis);
            start_gap = start_gap.max(low.max(-high));
            swept_gap = swept_gap.max((low - shift.max(0.)).max(shift.min(0.) - high));
        }
        let low_y = relative.iter().map(|v| v.y).fold(f64::INFINITY, f64::min);
        let high_y = relative.iter().map(|v| v.y).fold(f64::NEG_INFINITY, f64::max);
        let vertical_gap = (low_y - self.half_segment).max(-self.half_segment - high_y).max(0.);
        (swept_gap > self.radius + tolerance, start_gap.hypot(vertical_gap) > self.radius + self.prediction + tolerance)
    }
}

fn axial(v: Vector) -> bool {
    v.is_finite() && v.x == 0. && v.z == 0.
}

/// A nonzero transverse warm start can move the first solver iteration even if the
/// incoming body velocity vanishes. Raw and clustered manifolds may both seed the new solve.
fn axial_cache(sim: &PhysicsWorld, actor: ColliderHandle) -> bool {
    sim.contact_pairs_with(actor).all(|p| {
        p.manifolds().iter().chain(p.solver_manifolds()).all(|m| {
            m.points
                .iter()
                .all(|w| w.data.warmstart_tangent_world == Vector::ZERO && w.data.warmstart_twist_impulse == 0.)
        })
    })
}

/// Height of the last upward, centred native contact. A partial mesh can lose its
/// manifold for a tick even at a point well inside its only available floor triangle.
fn previous_floor(sim: &PhysicsWorld, actor: ColliderHandle, center: Vector, foot_y: f64, margin: f64) -> Option<f64> {
    let mut floor: Option<f64> = None;
    for pair in sim.contact_pairs_with(actor) {
        let first = pair.collider1 == actor;
        for m in pair.solver_manifolds() {
            let normal = if first { -m.data.normal } else { m.data.normal };
            if normal != Vector::Y {
                continue;
            }
            for c in &m.data.solver_contacts {
                let fixed = if first { c.anchor2 } else { c.anchor1 };
                let y = f64::from(fixed.y);
                if axial(fixed - center) && (foot_y - y).abs() <= margin {
                    floor = Some(floor.map_or(y, |old| old.max(y)));
                }
            }
        }
    }
    floor
}

fn central_constraints(sim: &PhysicsWorld, actor: ColliderHandle, center: Vector, original: Vector) -> bool {
    // With one CCD pass, Rapier leaves the exact manifolds used by the solver in
    // its narrow phase. CCD can only shorten the solved motion after that pass.
    sim.contact_pairs_with(actor).all(|pair| {
        let first = pair.collider1 == actor;
        pair.solver_manifolds().iter().all(|m| {
            m.data.solver_contacts.iter().all(|c| {
                let w = &m.points[c.contact_indices()[0] as usize].data;
                let (arm, anchor, fixed) =
                    if first { (w.solver_dp1, c.anchor1, c.anchor2) } else { (w.solver_dp2, c.anchor2, c.anchor1) };
                // Gravity and every normal impulse are vertical and pass through the
                // centre. The friction constraint sees zero lateral displacement at
                // both anchors on every solver iteration, including the warm start.
                axial(original)
                    && axial(m.data.normal)
                    && m.data.normal.y.abs() == 1.
                    && axial(arm)
                    && axial(anchor)
                    && axial(fixed - center)
                    && c.tangent_velocity == Vector::ZERO
                    && w.warmstart_tangent_world == Vector::ZERO
                    && w.warmstart_twist_impulse == 0.
            })
        })
    })
}

impl TerrainSimulation {
    pub fn upright_sweep(&self) -> Option<UprightSweep> {
        let native = self.rapier();
        let [(id, handle)] = native.parts.as_slice() else { return None };
        let sim = &native.sim;
        let parameters = sim.integration_parameters;
        let body = &sim.bodies[*id];
        let collider = &sim.colliders[*handle];
        let capsule = collider.shape().as_capsule()?;
        let start = body.translation().as_dvec3();
        let velocity = body.linvel();
        let dt = f64::from(self.simulation.settings.dt());
        let limit = f64::from(parameters.max_linear_velocity());
        if !self.simulation.box_poses.is_empty()
            || self.simulation.substeps != 1
            || parameters.max_ccd_substeps != 1
            || parameters.num_solver_iterations != 4
            || body.additional_solver_iterations() != 0
            || *body.rotation() != glam::Quat::IDENTITY
            || body.angvel() != Vector::ZERO
            || body.local_center_of_mass() != Vector::ZERO
            || body.user_force() != Vector::ZERO
            || body.user_torque() != Vector::ZERO
            || !axial(sim.gravity)
            || collider.contact_skin() != 0.
            || !axial(capsule.segment.a)
            || capsule.segment.b != -capsule.segment.a
            || !start.is_finite()
            || !velocity.is_finite()
            || !limit.is_finite()
            || limit <= 0.
            || limit == f64::from(f32::MAX)
        {
            return None;
        }
        let end = start + DVec3::new(f64::from(velocity.x), 0., f64::from(velocity.z)) * dt;
        let travel = limit * dt;
        // Four native f32 position integrations, their pose reconstruction, and the CCD
        // interpolation must all fit. Use world magnitude for f32 translations.
        let scale = start.abs().max(end.abs()).max_element().max(travel).max(1.);
        let padding = 0.001 + 64. * f64::from(f32::EPSILON) * scale;
        Some(UprightSweep {
            start,
            end,
            radius: f64::from(capsule.radius),
            half_segment: f64::from(capsule.segment.a.y.abs()),
            prediction: f64::from(parameters.prediction_distance())
                .max((velocity.as_dvec3().length() * dt).min(f64::from(body.soft_ccd_prediction()))),
            max_travel: travel,
            padding,
        })
    }

    /// Called only when actual missing single-face geometry would otherwise hold the actor.
    /// Preview the same persistent native state, sharing immutable mesh storage. No contact,
    /// force event or simulation clock in the live world is changed by the preview.
    pub fn confirms_upright_sweep(&self) -> bool {
        let Some(sweep) = self.upright_sweep() else {
            return false;
        };
        let native = self.rapier();
        let (id, actor) = native.parts[0];
        let sim = &native.sim;
        if !axial_cache(sim, actor) {
            return false;
        }
        let mut trial = PhysicsWorld {
            gravity: sim.gravity,
            integration_parameters: sim.integration_parameters,
            bodies: sim.bodies.clone(),
            colliders: sim.colliders.clone(),
            islands: sim.islands.clone(),
            broad_phase: sim.broad_phase.clone(),
            narrow_phase: sim.narrow_phase.clone(),
            ccd_solver: sim.ccd_solver.clone(),
            // This constructor's single freely moving body has no joints, box, or soft bodies.
            ..PhysicsWorld::default()
        };
        let cfg = self.simulation.settings;
        let dt = cfg.dt();
        trial.integration_parameters.dt = dt;
        let original = sim.bodies[id].linvel();
        let limited = cfg.fall_limit(original.y, dt);
        if limited > original.y {
            trial.bodies[id].set_linvel(Vector::new(original.x, limited, original.z), true);
        }
        trial.step();
        let body = &trial.bodies[id];
        if *body.rotation() != glam::Quat::IDENTITY || body.angvel() != Vector::ZERO || !body.translation().is_finite()
        {
            return false;
        }
        let bottom = |y: f64| y - sweep.half_segment - sweep.radius;
        let foot_y = bottom(sweep.start.y);
        let previous = previous_floor(sim, actor, sim.bodies[id].translation(), foot_y, sweep.padding);
        let arrived_y = bottom(f64::from(body.translation().y));
        if previous.is_some_and(|y| arrived_y < foot_y.min(y) - sweep.padding)
            || !central_constraints(&trial, actor, sim.bodies[id].translation(), original)
        {
            return false;
        }
        // On a new partial support, check that the native contact can persist. A point just
        // above the plane may otherwise drop through it on the following step; handing
        // its cached state to full terrain later cannot undo that first bad placement.
        if previous.is_none()
            && let Some(y) = previous_floor(&trial, actor, body.translation(), arrived_y, sweep.padding)
        {
            trial.step();
            let next_y = bottom(f64::from(trial.bodies[id].translation().y));
            if !next_y.is_finite() || next_y < arrived_y.min(y) - sweep.padding {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Capsule, RigStart, Settings};

    fn sweep(x: f64, z: f64, vx: f64, vz: f64) -> UprightSweep {
        let start = DVec3::new(x, 0.88, z);
        UprightSweep {
            start,
            end: start + DVec3::new(vx, 0., vz),
            half_segment: 0.6,
            radius: 0.28,
            prediction: 0.,
            max_travel: 7.,
            padding: 0.,
        }
    }

    #[test]
    fn confirms_closed_edge_vertex_and_interior_of_oblique_motion() {
        let triangle = [[0., 0., 0.], [0., 0., 3.], [3., 0., 0.]];
        let edge = 0.28 / 2f64.sqrt();
        assert!(!sweep(1.5 + edge, 1.5 + edge, 0., 0.).excludes_triangle(triangle));
        assert!(sweep(1.5 + edge + 1e-7, 1.5 + edge, 0., 0.).excludes_triangle(triangle));
        assert!(!sweep(-0.28, -1., 0., 2.).excludes_triangle(triangle));
        assert!(sweep(-0.281, -1., 0., 2.).excludes_triangle(triangle));
        assert!(!sweep(-1., -1., 5., 5.).excludes_triangle(triangle));
        let wall = [[0., -1., 0.], [0., 3., 0.], [0., 3., 3.]];
        assert!(!sweep(-1., 1.5, 2., 0.).excludes_triangle(wall));
        assert!(sweep(-0.281, 1.5, 0., 1.).excludes_triangle(wall));
        let mut approaching = sweep(-0.32, 0.5, 0.01, 0.1);
        approaching.prediction = 0.28;
        assert!(!approaching.excludes_triangle(triangle), "a speculative candidate is still in reach");
        approaching.start.y += 1.;
        approaching.end.y += 1.;
        assert!(approaching.excludes_triangle(triangle));
    }

    #[test]
    fn keeps_explicit_witnesses_on_intermediate_spine_positions() {
        // Plant a point of the triangle inside or on the true swept capsule at a
        // known time. A strict separating projection must never discard that witness.
        for direction in 0..64 {
            let (sn, cs) = (f64::from(direction) * std::f64::consts::TAU / 64.).sin_cos();
            let mut track = sweep(-0.37, 0.43, cs * 1.9, sn * 1.9);
            track.padding = 1e-6;
            for sample in 0..=12 {
                let at = track.start.lerp(track.end, f64::from(sample) / 12.);
                for height in [-0.6, 0., 0.6] {
                    for radius in [0., 0.14, 0.28] {
                        let p = at + DVec3::Y * height + DVec3::new(-sn, 0., cs) * radius;
                        let triangle = [p, p + DVec3::new(sn, 1., -cs), p + DVec3::new(cs, -0.3, sn)];
                        assert!(!track.excludes_triangle(triangle.map(|v| v.as_vec3().to_array())));
                    }
                }
            }
        }
    }

    fn world(velocity: [f32; 3]) -> TerrainSimulation {
        let settings = Settings {
            tick_hz: 60,
            gravity: 9.81,
            terminal_speed: 55.,
            capsule: Capsule { radius: 0.28, half_segment: 0.6, mass: 75., friction: 0.5 },
        };
        TerrainSimulation::new(
            settings,
            &[settings.capsule.part()],
            RigStart { origin_m: [0., 0.881, 0.], velocity_m_s: velocity, animation: None },
        )
        .unwrap()
    }

    #[test]
    fn previews_free_flight_and_axial_support_without_advancing_the_live_world() {
        let mut sim = world([2., -4., 1.]);
        assert!(sim.confirms_upright_sweep());
        assert_eq!(sim.state().pose.center, [0., 0.881, 0.]);
        let floor = || ColliderBuilder::cuboid(4., 0.5, 4.).translation(Vector::new(0., -0.5, 0.));
        sim.insert(floor(), None, "horizontal", [0, 0]);
        assert!(!sim.confirms_upright_sweep(), "oblique friction and spin are not certified");
        let mut sim = world([0., -3., 0.]);
        sim.insert(floor(), None, "horizontal", [0, 0]);
        assert!(sim.confirms_upright_sweep());
        let mut slope = world([0., -3., 0.]);
        slope.insert(floor().rotation(Vector::Z * 0.15), None, "slope", [0, 0]);
        assert!(!slope.confirms_upright_sweep());
        let mut spinning = world([0.; 3]);
        let id = spinning.rapier().parts[0].0;
        spinning.rapier_mut().sim.bodies[id].set_angvel(Vector::Y, true);
        assert!(spinning.upright_sweep().is_none());
        let mut unlimited = world([0.; 3]);
        unlimited.rapier_mut().sim.integration_parameters.normalized_max_linear_velocity = f32::MAX;
        assert!(unlimited.upright_sweep().is_none());
    }
}
