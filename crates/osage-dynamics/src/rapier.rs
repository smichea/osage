// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

use super::{
    Contact, ContactBodies, ContactImpulse, DynamicBox, ImpulseAttribution, ImpulseMeasurement, Interval, Lift,
    Observation, Part, Pose, Settings, StaticMesh,
};
use crate::mesh_contacts;
use crate::static_instances::{Deferred, Reach};
use anyhow::{Context, Result};
use rapier3d::math::Pose as Isometry;
use rapier3d::prelude::*;
use std::sync::Mutex;

mod hanging;
mod supported_impact;
use hanging::HangingBranches;

/// Raw native witnesses after a full sleeping step. The stiffness decision for the
/// refreshed graph becomes reusable when the next step begins at the same poses.
struct RestingContacts {
    witnesses: Vec<Contact>,
    stiffness_dt: Option<f32>,
}

pub(super) struct World {
    pub(super) sim: PhysicsWorld,
    boxes: Vec<RigidBodyHandle>,
    hanging: HangingBranches,
    supported_impact: Option<supported_impact::SupportedImpact>,
    resting_contacts: Option<RestingContacts>,
    deferred: Deferred<ColliderHandle>,
    actor_radius: f32,
    pair_suppressed: bool,
    force_events: bool,
    pub(super) parts: Vec<(RigidBodyHandle, ColliderHandle)>,
}

fn native_shape(mesh: &StaticMesh) -> Result<SharedShape> {
    let points = mesh.vertices.iter().map(|p| Vector::from_array(*p)).collect();
    mesh_contacts::collider(points, mesh.triangles.clone())
        .map(|builder| builder.shape)
        .with_context(|| format!("building triangle mesh {}", mesh.id))
}
fn reach(sim: &PhysicsWorld, center: [f32; 3], velocity: [f32; 3], radius: f32, dt: f32, gravity: f32) -> Reach {
    let params = &sim.integration_parameters;
    Reach::new(
        center,
        velocity,
        radius,
        dt,
        gravity,
        params.max_linear_velocity(),
        radius + params.prediction_distance() + params.max_corrective_velocity() * dt,
    )
}
fn insert_terrain(
    sim: &mut PhysicsWorld,
    meshes: &[StaticMesh],
    default_friction: f32,
    admission: Option<&Reach>,
) -> Result<Deferred<ColliderHandle>> {
    // Both solvers use the geometric mean of the two material friction values.
    super::static_instances::insert_shapes(meshes, admission, native_shape, |i, mesh, shape, offset| {
        let h = sim.insert_collider(
            ColliderBuilder::new(shape.clone())
                .translation(Vector::from_array(offset))
                .user_data(i as u128)
                .friction(mesh.material.map_or(default_friction, |m| m.friction).sqrt())
                .friction_combine_rule(CoefficientCombineRule::Multiply)
                .restitution(mesh.material.map_or(0., |m| m.restitution))
                .restitution_combine_rule(CoefficientCombineRule::Max),
            None,
        );
        Ok(h)
    })
}

impl World {
    pub(super) fn new(
        meshes: &[StaticMesh],
        moving: &[DynamicBox],
        cfg: Settings,
        origin: [f32; 3],
        velocity: [f32; 3],
        rig: &[Part],
    ) -> Result<Self> {
        let mut sim = PhysicsWorld::new();
        sim.narrow_phase.set_query_dispatcher(mesh_contacts::MeshQueries::default());
        sim.gravity = Vector::NEG_Y * cfg.gravity;
        sim.integration_parameters.dt = cfg.dt();
        if rig.len() > 1 {
            sim.integration_parameters.num_solver_iterations = 8;
            // After CCD stops one limb, advance the time left in the step for the rest of its joint chain.
            sim.integration_parameters.max_ccd_substeps = 4;
        }
        // Streaming publishes static geometry after the actor exists. Rapier uses pair
        // insertion order when constructing manifolds. The articulated solver can amplify
        // the resulting round-off during a fall, even with byte-identical fixed triangles.
        let articulated_terrain = rig.len() > 1 && moving.is_empty();
        let actor_radius = rig[0].radius + rig[0].half_segment;
        let admission = (rig.len() == 1 && moving.is_empty()).then(|| {
            reach(
                &sim,
                (Vector::from_array(origin) + rig[0].center).to_array(),
                velocity,
                actor_radius,
                cfg.dt(),
                cfg.gravity,
            )
        });
        let deferred = if articulated_terrain {
            None
        } else {
            Some(insert_terrain(&mut sim, meshes, cfg.capsule.friction, admission.as_ref())?)
        };
        let mut boxes = Vec::with_capacity(moving.len());
        let isolated = rig.len() == 1 && moving.len() == 1 && rig[0].half_segment > 0.;
        for (index, b) in moving.iter().enumerate() {
            let [x, y, z] = b.half_extents_m;
            let pose = Isometry::from_parts(b.center_m.into(), glam::Quat::from_array(b.rotation_xyzw));
            let (handle, _) = sim.insert(
                RigidBodyBuilder::dynamic().pose(pose).linvel(b.velocity_m_s.into()).ccd_enabled(true),
                ColliderBuilder::cuboid(x, y, z)
                    .collision_groups(if isolated {
                        InteractionGroups::new(Group::GROUP_3, Group::ALL, InteractionTestMode::And)
                    } else {
                        InteractionGroups::all()
                    })
                    .mass(b.mass_kg)
                    .friction(b.friction.sqrt())
                    .friction_combine_rule(CoefficientCombineRule::Multiply)
                    .restitution(0.)
                    .user_data((meshes.len() + index) as u128),
            );
            boxes.push(handle);
        }
        // One tested actor in this world. Its parts hit the scene, while its own limbs are joined.
        let groups = InteractionGroups::new(Group::GROUP_2, !Group::GROUP_2, InteractionTestMode::And);
        let mut parts: Vec<(RigidBodyHandle, ColliderHandle)> = Vec::with_capacity(rig.len());
        let origin = Vector::from_array(origin);
        let mut joints = Vec::with_capacity(rig.len() - 1);
        for p in rig {
            let frame = Isometry::from_parts(origin + p.center, p.rotation);
            let collider = if p.half_segment == 0. {
                ColliderBuilder::ball(p.radius)
            } else {
                ColliderBuilder::capsule_y(p.half_segment, p.radius)
            };
            let material = p
                .material
                .unwrap_or(osage_objects::CollisionMaterial { friction: cfg.capsule.friction, restitution: 0. });
            let collider = match p.inertia {
                Some(inertia) => collider.mass_properties(MassProperties::new(Vector::ZERO, p.mass, inertia)),
                None => collider.mass(p.mass),
            };
            let mut builder = RigidBodyBuilder::dynamic().pose(frame).linvel(velocity.into()).ccd_enabled(true);
            // A moderate descent can move by less than one radius per step,
            // below the hard CCD trigger. Let a lone capsule anticipate those contacts.
            if rig.len() == 1 && p.half_segment > 0. {
                builder = builder.soft_ccd_prediction(p.radius);
            }
            let (body, collider) = sim.insert(
                builder,
                collider
                    .friction(material.friction.sqrt())
                    .friction_combine_rule(CoefficientCombineRule::Multiply)
                    .restitution(material.restitution)
                    .restitution_combine_rule(CoefficientCombineRule::Max)
                    .collision_groups(groups),
            );
            if let Some(parent) = p.parent {
                let anchor = Isometry::from_parts(origin + p.anchor, p.joint_rotation);
                let parent_body = parts[parent].0;
                let parent_frame = sim.bodies[parent_body].position();
                let mut locked = JointAxesMask::LOCKED_SPHERICAL_AXES;
                let axes = [JointAxis::AngX, JointAxis::AngY, JointAxis::AngZ];
                for (axis, range) in axes.into_iter().zip(p.limits_deg) {
                    if range == [0., 0.] {
                        locked |= JointAxesMask::from(axis);
                    }
                }
                let mut joint = GenericJointBuilder::new(locked)
                    .local_frame1(parent_frame.inverse() * anchor)
                    .local_frame2(frame.inverse() * anchor)
                    .contacts_enabled(false);
                for (axis, [lo, hi]) in axes.into_iter().zip(p.limits_deg) {
                    if lo != hi {
                        joint = joint.limits(axis, [lo.to_radians(), hi.to_radians()]);
                    }
                }
                joints.push(sim.insert_impulse_joint(parent_body, body, joint));
            }
            parts.push((body, collider));
        }
        let hanging = HangingBranches::new(&mut sim, &parts, rig, joints);
        let deferred = match deferred {
            Some(d) => d,
            None => insert_terrain(&mut sim, meshes, cfg.capsule.friction, None)?,
        };
        Ok(Self {
            sim,
            parts,
            boxes,
            hanging,
            deferred,
            actor_radius,
            pair_suppressed: false,
            force_events: false,
            supported_impact: None,
            resting_contacts: None,
        })
    }

    pub(super) fn add_lift(&mut self, lift: Lift, pose: Pose) {
        let [x, y, z] = lift.half_extents_m;
        let surface = self.sim.colliders.len() - self.parts.len();
        let (body, _) = self.sim.insert(
            RigidBodyBuilder::kinematic_position_based()
                .translation(pose.center.into())
                .linvel(pose.linear_velocity.into()),
            ColliderBuilder::cuboid(x, y, z)
                .friction(lift.friction.sqrt())
                .friction_combine_rule(CoefficientCombineRule::Multiply)
                .restitution(0.)
                .restitution_combine_rule(CoefficientCombineRule::Max)
                .user_data(surface as u128),
        );
        self.boxes.push(body);
    }

    pub(super) fn move_lift(&mut self, target: [f32; 3]) {
        self.sim.bodies[self.boxes[0]].set_next_kinematic_translation(target.into());
    }

    pub(super) fn prepare_static(&mut self, cfg: Settings, dt: f32) -> Result<()> {
        if self.deferred.is_empty() {
            return Ok(());
        }
        let body = &self.sim.bodies[self.parts[0].0];
        let admission = reach(
            &self.sim,
            body.translation().to_array(),
            body.linvel().to_array(),
            self.actor_radius,
            dt,
            cfg.gravity,
        );
        self.deferred.admit(&admission, |h, mesh| {
            let shape = native_shape(mesh)?;
            let collider = &mut self.sim.colliders[h];
            collider.set_translation(Vector::ZERO);
            collider.set_shape(shape);
            Ok(())
        })
    }
    pub(super) fn starting_poses(&mut self, poses: &[Pose]) {
        for (&(id, _), pose) in self.parts.iter().zip(poses) {
            let body = &mut self.sim.bodies[id];
            body.set_position(
                Isometry::from_parts(pose.center.into(), glam::Quat::from_array(pose.rotation_xyzw)),
                true,
            );
            body.set_linvel(pose.linear_velocity.into(), true);
            body.set_angvel(pose.angular_velocity.into(), true);
        }
    }

    pub(super) fn box_collider(&self, index: usize) -> ColliderHandle {
        self.sim.bodies[self.boxes[index]].colliders()[0]
    }

    /// Match the resident insertion order of active terrain, actor and moving support.
    /// The free box comes before the actor; the driven lift is added after the actor.
    /// No solver step or contact cache may precede this operation.
    pub(super) fn finish_moving_bootstrap(&mut self, lift: bool) {
        let moving_count = self.boxes.len() + self.parts.len();
        if self.sim.colliders.len() == moving_count {
            return;
        }
        // In the resident articulated world without a free box, limbs already precede terrain.
        // Move only the lift in that case. A single capsule follows the resident terrain.
        let reinsert_parts = !lift || self.parts.len() == 1;
        let mut bodies: Vec<_> = self.boxes.iter().map(|&b| (b, self.sim.bodies[b].colliders()[0], None)).collect();
        if reinsert_parts {
            bodies.extend(self.parts.iter().enumerate().map(|(i, &(b, c))| (b, c, Some(i))));
            if lift {
                bodies.rotate_left(self.boxes.len());
            }
        }
        // Keep all old handles occupied until every replacement exists. Static
        // page handles, rigid bodies, joint anchors and starting poses stay valid.
        let mut previous = Vec::with_capacity(moving_count);
        let mut last_slot = self.sim.colliders.iter().map(|(h, _)| h.into_raw_parts().0).max().expect("live bodies");
        for (body, old, part) in bodies {
            previous.push(old);
            let handle = loop {
                let handle = self.sim.insert_collider(self.sim.colliders[old].clone(), Some(body));
                let slot = handle.into_raw_parts().0;
                if slot > last_slot {
                    last_slot = slot;
                    break handle;
                }
                // A page may leave while the first tick is held. Keep its vacant
                // slot occupied until the whole moving group has been appended.
                previous.push(handle);
            };
            match part {
                Some(i) => self.parts[i].1 = handle,
                None => self.sim.colliders[handle].user_data = u128::from(handle.into_raw_parts().0),
            }
        }
        for handle in previous {
            self.sim.remove_collider(handle);
        }
    }

    pub(super) fn first_box_surface(&self) -> usize {
        let collider = self.box_collider(0);
        self.sim.colliders[collider].user_data as usize
    }

    pub(super) fn suppress_free_pair(&mut self, separated: bool) {
        if self.pair_suppressed == separated {
            return;
        }
        let handle = self.parts[0].1;
        let mut groups = self.sim.colliders[handle].collision_groups();
        groups.filter.set(Group::GROUP_3, !separated);
        self.sim.colliders[handle].set_collision_groups(groups);
        self.pair_suppressed = separated;
    }
    // Match the two surface-point velocities in all three dimensions for an isolated
    // inelastic contact. Reject the impulse when it lies outside the Coulomb cone, which
    // leaves sliding/restitution under the engine's ordinary solver.
    pub(super) fn resolve_sticking(
        &mut self,
        contact: super::free_pair::StickingCandidate,
        capsule: &mut Pose,
        cuboid: &mut Pose,
    ) -> Option<Vector> {
        let handles = [self.parts[0].0, self.boxes[0]];
        let arms = contact.arms;
        let bodies = handles.map(|h| &self.sim.bodies[h]);
        let relative = bodies[0].linvel() + bodies[0].angvel().cross(arms[0])
            - bodies[1].linvel()
            - bodies[1].angvel().cross(arms[1]);
        let response = |impulse: Vector| {
            let mut velocity = Vector::ZERO;
            for (body, arm) in bodies.into_iter().zip(arms) {
                let mass = body.mass_properties();
                let spin = mass.effective_world_inv_inertia * arm.cross(impulse);
                velocity += mass.effective_inv_mass * impulse + spin.cross(arm);
            }
            velocity
        };
        let compliance = glam::Mat3::from_cols(response(Vector::X), response(Vector::Y), response(Vector::Z));
        let inverse = compliance.try_inverse()?;
        let impulse = inverse * (-relative);
        let normal_impulse = impulse.dot(contact.normal);
        if !impulse.is_finite()
            || normal_impulse <= 0.
            || (impulse - normal_impulse * contact.normal).length() > contact.friction * normal_impulse
        {
            return None;
        }
        for (((id, arm), j), state) in handles.into_iter().zip(arms).zip([impulse, -impulse]).zip([capsule, cuboid]) {
            let body = &mut self.sim.bodies[id];
            body.apply_impulse(j, true);
            body.apply_torque_impulse(arm.cross(j), true);
            state.linear_velocity = body.linvel().to_array();
            state.angular_velocity = body.angvel().to_array();
        }
        Some(impulse)
    }

    fn deepening_limb_contact(&self, body: RigidBodyHandle, collider: ColliderHandle, dt: f32) -> bool {
        let box_shape = &self.sim.colliders[collider];
        let parameters = &self.sim.integration_parameters;
        let slop = parameters.allowed_linear_error();
        let threshold = parameters.prediction_distance() - slop;
        let box_bounds = box_shape.compute_aabb();
        let box_scale = box_bounds.mins.abs().max(box_bounds.maxs.abs()).max_element().max(1.);
        self.sim.contact_pairs_with(collider).any(|pair| {
            let limb = if pair.collider1 == collider { pair.collider2 } else { pair.collider1 };
            let Some(limb) = self.sim.colliders.get(limb) else { return false };
            let Some(limb_body) = limb.parent().and_then(|h| self.sim.bodies.get(h)).filter(|b| b.is_dynamic()) else {
                return false;
            };
            // Translating by the overlap along any world axis separates the two AABBs.
            // This bounds the true shape penetration from above, including for a tilted box.
            let bounds = limb.compute_aabb();
            let overlap = (bounds.maxs - box_bounds.mins).min(box_bounds.maxs - bounds.mins).min_element();
            let scale = bounds.mins.abs().max(bounds.maxs.abs()).max_element().max(box_scale);
            if overlap + 32. * f32::EPSILON * scale < slop {
                return false;
            }
            let Ok(Some(contact)) = rapier3d::parry::query::contact(
                box_shape.position(),
                box_shape.shape(),
                limb.position(),
                limb.shape(),
                0.,
            ) else {
                return false;
            };
            if contact.dist >= -slop {
                return false;
            }
            let box_velocity = self.sim.bodies[body].velocity_at_point(contact.point1);
            let limb_velocity = limb_body.velocity_at_point(contact.point2);
            let closing = (limb_velocity - box_velocity).dot(contact.normal1).min(0.);
            contact.dist + dt * closing < -threshold
        })
    }

    /// A rigid box carrying its load on fixed terrain has two contacts in series.
    /// Near the prediction distance use the terrain's stiffness for the limb and
    /// box. Shallow contacts retain their ordinary compliance so the rig can settle.
    fn update_supported_contact_stiffness(&mut self, dt: f32) {
        let [body] = self.boxes.as_slice() else { return };
        if self.parts.len() < 2 || !self.sim.bodies[*body].is_dynamic() {
            return;
        }
        let collider = self.box_collider(0);
        let supported = self.deepening_limb_contact(*body, collider, dt) && self.carries_fixed_reaction(collider);
        self.sim.integration_parameters.contact_softness = if supported {
            self.sim.integration_parameters.static_contact_softness
        } else {
            SpringCoefficients::contact_defaults()
        };
    }

    pub(super) fn invalidate_resting_contacts(&mut self) {
        self.resting_contacts = None;
    }

    fn articulated_scene_is_asleep(&self) -> bool {
        // Single bodies can use deferred static geometry or an isolated-pair plan.
        // A kinematic support can move on the next step with the actor still asleep.
        self.parts.len() > 1
            && self.parts.iter().all(|&(body, _)| self.sim.bodies[body].is_sleeping())
            && self.boxes.iter().all(|&body| self.sim.bodies[body].is_dynamic() && self.sim.bodies[body].is_sleeping())
    }

    pub(super) fn step(
        &mut self,
        cfg: Settings,
        dt: f32,
        observation: Observation<'_>,
        contacts: &mut Vec<Contact>,
        poses: &mut [Pose],
        box_poses: &mut [Pose],
    ) {
        for (id, _) in &self.parts {
            // Mutable indexing registers a user change with Rapier.
            let mut v = self.sim.bodies[*id].linvel();
            let limited = cfg.fall_limit(v.y, dt);
            if v.y < limited {
                v.y = limited;
                self.sim.bodies[*id].set_linvel(v, true);
            }
        }
        self.hanging.update(&mut self.sim, &self.parts, dt);
        let observe = observation.output.is_some();
        let asleep = self.articulated_scene_is_asleep();
        if !asleep || observe != self.force_events {
            self.invalidate_resting_contacts();
        }
        if self.resting_contacts.as_ref().and_then(|rest| rest.stiffness_dt) != Some(dt) {
            self.update_supported_contact_stiffness(dt);
            if let Some(rest) = &mut self.resting_contacts {
                rest.stiffness_dt = Some(dt);
            }
        }
        self.sim.integration_parameters.dt = dt;
        if self.force_events != observe {
            self.force_events = observe;
            for &(_, handle) in &self.parts {
                let collider = &mut self.sim.colliders[handle];
                let mut flags = collider.active_events();
                flags.set(ActiveEvents::CONTACT_FORCE_EVENTS, observe);
                collider.set_active_events(flags);
            }
        }
        if let Some(output) = observation.output {
            let events = ForceEvents { parts: &self.parts, time: observation.time, output: Mutex::new(output) };
            self.sim.step_with_events(&(), &events);
        } else {
            self.sim.step();
        }
        let substep = observation.time.substep;
        // The native step always runs, including force events, island updates and wakes.
        // Sleeping manifolds keep their world witnesses while all moving bodies stay asleep.
        let still_asleep = asleep && self.articulated_scene_is_asleep();
        if still_asleep && let Some(rest) = &self.resting_contacts {
            contacts.extend(rest.witnesses.iter().map(|&contact| Contact { substep, ..contact }));
        } else {
            let start = contacts.len();
            self.record_contacts(substep, contacts);
            self.resting_contacts =
                still_asleep.then(|| RestingContacts { witnesses: contacts[start..].to_vec(), stiffness_dt: None });
        }
        let handles = self.parts.iter().map(|p| p.0).chain(self.boxes.iter().copied());
        for (id, dest) in handles.zip(poses.iter_mut().chain(box_poses)) {
            let body = &self.sim.bodies[id];
            *dest = Pose {
                center: body.translation().to_array(),
                rotation_xyzw: body.rotation().to_array(),
                linear_velocity: body.linvel().to_array(),
                angular_velocity: body.angvel().to_array(),
            };
        }
    }

    fn record_contacts(&self, substep: u8, contacts: &mut Vec<Contact>) {
        for (index, &(_, collider)) in self.parts.iter().enumerate() {
            for pair in self.sim.contact_pairs_with(collider) {
                let first = pair.collider1 != collider;
                let surface = if first { pair.collider1 } else { pair.collider2 };
                let touched = &self.sim.colliders[surface];
                for m in pair.solver_manifolds() {
                    let surface_triangle =
                        touched.shape().as_trimesh().map(|_| if first { m.subshape1 } else { m.subshape2 });
                    for contact in &m.data.solver_contacts {
                        let (a, b) = m.data.solver_contact_world_points(contact, &self.sim.bodies);
                        let normal = if first { m.data.normal } else { -m.data.normal };
                        let point = if first { a } else { b };
                        contacts.push(Contact {
                            substep,
                            surface: touched.user_data as usize,
                            surface_triangle,
                            part: index,
                            point: point.to_array(),
                            normal: normal.to_array(),
                            separation: (b - a).dot(m.data.normal),
                            applied_offset_s: None,
                        });
                    }
                }
            }
        }
    }
}

/// Rapier calls this after writing back the solved impulses, once for each force-producing
/// pair in each CCD solve. A later CCD slice may already have replaced its manifolds.
struct ForceEvents<'a> {
    parts: &'a [(RigidBodyHandle, ColliderHandle)],
    time: Interval,
    output: Mutex<&'a mut Vec<ContactImpulse>>,
}

impl EventHandler for ForceEvents<'_> {
    fn handle_soft_body_tear_event(&self, _: &SoftBodySet, _: &SoftBodyTearEvent) {}

    fn handle_collision_event(&self, _: &RigidBodySet, _: &ColliderSet, _: CollisionEvent, _: Option<&ContactPair>) {}

    fn handle_contact_force_event(
        &self,
        _: Real,
        bodies: &RigidBodySet,
        colliders: &ColliderSet,
        pair: &ContactPair,
        _: Real,
    ) {
        let Some(part) = self.parts.iter().position(|&(_, h)| h == pair.collider1 || h == pair.collider2) else {
            return;
        };
        let surface_first = self.parts[part].1 == pair.collider2;
        let surface = if surface_first { pair.collider1 } else { pair.collider2 };
        let pair_id = ContactBodies { part, surface: colliders[surface].user_data as usize };
        let mut out = self.output.lock().expect("physics impulse buffer poisoned");
        for manifold in pair.solver_manifolds() {
            let normal = if surface_first { manifold.data.normal } else { -manifold.data.normal };
            for contact in &manifold.data.solver_contacts {
                let impulse = manifold.points[contact.contact_indices()[0] as usize].data.impulse;
                if impulse > 0. {
                    let (a, b) = manifold.data.solver_contact_world_points(contact, bodies);
                    let point = if surface_first { a } else { b };
                    out.push(self.time.record(
                        ImpulseAttribution::Pair(pair_id),
                        Some(point.to_array()),
                        Some(normal.to_array()),
                        ImpulseMeasurement::SolverNormal(impulse),
                    ));
                }
            }
        }
    }
}
