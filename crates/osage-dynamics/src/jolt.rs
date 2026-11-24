// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

use super::{
    Contact, ContactBodies, DynamicBox, ImpulseAttribution, ImpulseMeasurement, Lift, Observation, Part, Pose,
    Settings, StaticMesh,
};
use crate::static_instances::{Deferred, Reach};
use anyhow::{Context, Result, ensure};
use oxijolt::{
    Activation, BodyId, BodySettings, ContactEvent, EventSettings, MotionQuality, PhysicsWorld, Quat, RVec3,
    RagdollJoint, RagdollPart, RagdollSettings, Shape, SixDofAxis, SixDofConstraintAxis, SixDofConstraintSettings,
    Skeleton, SkeletonJoint, SwingType, Vec3, WorldSettings,
};
use std::collections::HashMap;

pub(super) struct World {
    sim: PhysicsWorld,
    deferred: Deferred<BodyId>,
    actor_radius: f32,
    parts: Vec<BodyId>,
    part_masses: Vec<f32>,
    observed_active: Vec<bool>,
    boxes: Vec<BodyId>,
    pair_group: Option<oxijolt::CollisionGroup>,
    pair_suppressed: bool,
    surfaces: HashMap<BodyId, usize>,
}

fn native_shape(mesh: &StaticMesh) -> Result<Shape> {
    let vertices = mesh.vertices.iter().copied().map(Vec3::from).collect::<Vec<_>>();
    let (shape, dropped) =
        Shape::new_mesh(&vertices, &mesh.triangles).with_context(|| format!("building triangle mesh {}", mesh.id))?;
    ensure!(dropped.is_empty(), "degenerate triangles on {}", mesh.id);
    Ok(shape)
}
fn reach(center: [f32; 3], velocity: [f32; 3], radius: f32, dt: f32, gravity: f32) -> Reach {
    // Native default maximum linear speed, speculative distance and six configured position passes.
    Reach::new(center, velocity, radius, dt, gravity, 500., 0.02 + 6. * 0.2)
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
        let bodies = u32::try_from(
            meshes.len().checked_add(rig.len()).and_then(|n| n.checked_add(moving.len())).context("too many bodies")?,
        )?;
        // The native world has a fixed body capacity. An initially static scene can acquire a lift.
        let spare = u32::from(moving.is_empty() && bodies < WorldSettings::MAX_BODIES);
        let capacity = (bodies + spare).max(1024);
        let mut sim = PhysicsWorld::new(
            WorldSettings::default()
                .single_threaded()
                .penetration_slop(0.002)
                .position_steps(6)
                .max_bodies(capacity)
                .gravity(Vec3::new(0., -cfg.gravity, 0.)),
        )?;
        sim.set_event_settings(EventSettings::default().persisted_contacts(true));
        let mut surfaces = HashMap::new();
        // The resident loader historically checks the native shape-coordinate limit on the world mesh.
        if let Some(invalid) =
            meshes.iter().find(|m| m.vertices.iter().flatten().any(|v| v.abs() > oxijolt::limits::MAX_SHAPE_EXTENT))
        {
            native_shape(invalid)?;
        }
        let actor_radius = rig[0].radius + rig[0].half_segment;
        let admission = (rig.len() == 1 && moving.is_empty()).then(|| {
            reach(
                (glam::Vec3::from_array(origin) + rig[0].center).to_array(),
                velocity,
                actor_radius,
                cfg.dt(),
                cfg.gravity,
            )
        });
        let deferred = super::static_instances::insert_shapes(
            meshes,
            admission.as_ref(),
            native_shape,
            |i, mesh, shape, offset| {
                let id = sim.create_body(
                    shape,
                    &BodySettings::new_static()
                        .position(offset.into())
                        .friction(mesh.material.map_or(cfg.capsule.friction, |m| m.friction))
                        .restitution(mesh.material.map_or(0., |m| m.restitution)),
                )?;
                surfaces.insert(id, i);
                Ok(id)
            },
        )?;
        let mut boxes = Vec::with_capacity(moving.len());
        let pair_group = if rig.len() == 1 && moving.len() == 1 && rig[0].half_segment > 0. {
            Some(oxijolt::CollisionGroup::new(&oxijolt::GroupFilterTableBuilder::new(1)?.build(), 0, 0)?)
        } else {
            None
        };
        for (index, b) in moving.iter().enumerate() {
            let shape = Shape::new_box_with_convex_radius(b.half_extents_m.into(), 0.)?;
            let body = BodySettings::new_dynamic()
                .position(b.center_m.into())
                .rotation(b.rotation_xyzw.into())
                .linear_velocity(b.velocity_m_s.into())
                .mass(b.mass_kg)
                .friction(b.friction)
                .restitution(0.)
                .linear_damping(0.)
                .angular_damping(0.)
                .motion_quality(MotionQuality::LinearCast);
            let id = sim.create_body(&shape, &body)?;
            if let Some(group) = &pair_group {
                sim.body_mut(id)?.set_collision_group(Some(group), oxijolt::Activation::Activate)?;
            }
            surfaces.insert(id, meshes.len() + index);
            boxes.push(id);
        }
        let origin = glam::Vec3::from_array(origin);
        let shapes = rig
            .iter()
            .map(|p| {
                if p.half_segment == 0. {
                    Shape::new_sphere(p.radius)
                } else {
                    Shape::new_capsule(p.half_segment, p.radius)
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut descriptors = Vec::with_capacity(rig.len());
        for (p, shape) in rig.iter().zip(&shapes) {
            let material = p
                .material
                .unwrap_or(osage_objects::CollisionMaterial { friction: cfg.capsule.friction, restitution: 0. });
            let body = BodySettings::new_dynamic()
                .position(RVec3::from((origin + p.center).to_array()))
                .rotation(Quat::from(p.rotation.to_array()))
                .linear_velocity(velocity.into())
                .motion_quality(MotionQuality::LinearCast)
                .friction(material.friction)
                .restitution(material.restitution)
                .linear_damping(0.)
                .angular_damping(0.);
            let body = match p.inertia {
                Some(moments) => body.mass_and_inertia(p.mass, oxijolt::Inertia::Principal(moments.to_array().into())),
                None => body.mass(p.mass),
            };
            let joint = p.parent.map(|_| joint_settings(p, (origin + p.anchor).to_array().into()));
            descriptors.push(RagdollPart { shape, body, joint });
        }
        let parts = if rig.len() == 1 {
            vec![sim.create_body(&shapes[0], &descriptors[0].body)?]
        } else {
            let bones: Vec<_> = rig
                .iter()
                .map(|p| SkeletonJoint { name: p.name.as_str(), parent: p.parent.map(|i| i as u32) })
                .collect();
            let skeleton = Skeleton::new(&bones)?;
            let settings = RagdollSettings::new(&skeleton, &descriptors)?;
            let id = sim.create_ragdoll(&settings, None, Activation::Activate)?;
            sim.ragdoll_mut(id)?.set_linear_and_angular_velocity(velocity.into(), Vec3::ZERO)?;
            sim.ragdoll(id)?.body_ids().to_vec()
        };
        Ok(Self {
            sim,
            deferred,
            actor_radius,
            parts,
            part_masses: rig.iter().map(|p| p.mass).collect(),
            observed_active: Vec::new(),
            surfaces,
            boxes,
            pair_group,
            pair_suppressed: false,
        })
    }

    pub(super) fn add_lift(&mut self, lift: Lift, pose: Pose) -> Result<()> {
        ensure!(
            lift.peak_speed() <= f64::from(oxijolt::limits::MAX_LINEAR_VELOCITY),
            "lift speed exceeds Jolt's kinematic velocity limit"
        );
        let top = lift.center_at(f64::from(lift.period_s) * 0.5);
        ensure!(
            lift.center_m
                .into_iter()
                .chain(top)
                .all(|x| f64::from(x.abs()) <= f64::from(oxijolt::limits::MAX_POSITION)),
            "lift stroke exceeds Jolt's world coordinate limit"
        );
        let shape = Shape::new_box_with_convex_radius(lift.half_extents_m.into(), 0.)?;
        let body = BodySettings::new_kinematic()
            .position(pose.center.into())
            .linear_velocity(pose.linear_velocity.into())
            .friction(lift.friction)
            .restitution(0.);
        let id = self.sim.create_body(&shape, &body).context("creating kinematic lift")?;
        self.surfaces.insert(id, self.surfaces.len());
        self.boxes.push(id);
        Ok(())
    }

    pub(super) fn first_box_surface(&self) -> usize {
        self.surfaces[&self.boxes[0]]
    }

    pub(super) fn move_lift(&mut self, target: [f32; 3], dt: f32) -> Result<()> {
        self.sim
            .body_mut(self.boxes[0])?
            .move_kinematic(target.into(), Quat::IDENTITY, dt)
            .context("driving kinematic lift")
    }

    pub(super) fn prepare_static(&mut self, cfg: Settings, dt: f32) -> Result<()> {
        if self.deferred.is_empty() {
            return Ok(());
        }
        let body = self.sim.body(self.parts[0])?;
        let admission =
            reach(body.position().into(), body.linear_velocity().into(), self.actor_radius, dt, cfg.gravity);
        self.deferred.admit(&admission, |id, mesh| {
            let shape = native_shape(mesh)?;
            let mut body = self.sim.body_mut(id)?;
            body.set_position(RVec3::ZERO, Activation::DontActivate)?;
            body.set_shape(&shape, None, Activation::DontActivate)?;
            Ok(())
        })
    }
    pub(super) fn starting_poses(&mut self, poses: &[Pose]) -> Result<()> {
        for (&id, pose) in self.parts.iter().zip(poses) {
            let mut body = self.sim.body_mut(id)?;
            body.set_position_and_rotation(pose.center.into(), pose.rotation_xyzw.into(), Activation::Activate)?;
            body.set_linear_velocity(pose.linear_velocity.into())?;
            body.set_angular_velocity(pose.angular_velocity.into())?;
        }
        Ok(())
    }

    pub(super) fn suppress_free_pair(&mut self, separated: bool) -> Result<()> {
        if self.pair_suppressed == separated {
            return Ok(());
        }
        let group = if separated { self.pair_group.as_ref() } else { None };
        self.sim.body_mut(self.parts[0])?.set_collision_group(group, oxijolt::Activation::Activate)?;
        self.pair_suppressed = separated;
        Ok(())
    }
    pub(super) fn step(
        &mut self,
        cfg: Settings,
        dt: f32,
        observation: Observation<'_>,
        contacts: &mut Vec<Contact>,
        poses: &mut [Pose],
        box_poses: &mut [Pose],
    ) -> Result<()> {
        let observing = observation.output.is_some();
        let substep = observation.time.substep;
        let first_contact = contacts.len();
        self.observed_active.clear();
        let mut momentum = glam::DVec3::ZERO;
        for (&id, &mass) in self.parts.iter().zip(&self.part_masses) {
            let mut v = self.sim.body(id)?.linear_velocity();
            let limited = cfg.fall_limit(v.y, dt);
            if v.y < limited {
                v.y = limited;
                self.sim.body_mut(id)?.set_linear_velocity(v)?;
            }
            if observing {
                self.observed_active.push(self.sim.body(id)?.is_active());
                momentum -= glam::Vec3::from_array(v.into()).as_dvec3() * f64::from(mass);
            }
        }
        let report = self.sim.step(dt)?;
        ensure!(report.is_complete(), "Jolt contact capacity exceeded: {report:?}");
        for event in self.sim.take_events().contacts {
            let m = match event {
                ContactEvent::Added { manifold, .. } | ContactEvent::Persisted { manifold, .. } => manifold,
                ContactEvent::Removed(_) => continue,
            };
            let (surface, dynamic, sign, first) = if let Some(&surface) = self.surfaces.get(&m.pair.body1) {
                (surface, m.pair.body2, 1., true)
            } else if let Some(&surface) = self.surfaces.get(&m.pair.body2) {
                (surface, m.pair.body1, -1., false)
            } else {
                continue;
            };
            let Some(part) = self.parts.iter().position(|id| *id == dynamic) else {
                continue;
            };
            let n = m.normal;
            for p in &m.points {
                let (a, b) = (p.point_on1, p.point_on2);
                let separation = (b.x - a.x) * n.x + (b.y - a.y) * n.y + (b.z - a.z) * n.z;
                contacts.push(Contact {
                    substep,
                    surface,
                    // Jolt's packed subshape ID is not an input triangle index.
                    surface_triangle: None,
                    part,
                    point: if first { a.into() } else { b.into() },
                    normal: [sign * n.x, sign * n.y, sign * n.z],
                    separation,
                    applied_offset_s: None,
                });
            }
        }
        let mut sleep_transition = false;
        for (index, (dest, id)) in
            poses.iter_mut().chain(box_poses).zip(self.parts.iter().chain(&self.boxes)).enumerate()
        {
            let body = self.sim.body(*id)?;
            *dest = Pose {
                center: body.position().into(),
                rotation_xyzw: body.rotation().into(),
                linear_velocity: body.linear_velocity().into(),
                angular_velocity: body.angular_velocity().into(),
            };
            if let Some(&was_active) = self.observed_active.get(index) {
                let active = body.is_active();
                let mut v = glam::Vec3::from_array(dest.linear_velocity).as_dvec3();
                if was_active || active {
                    v.y += f64::from(cfg.gravity) * f64::from(dt);
                }
                sleep_transition |= active != was_active;
                momentum += v * f64::from(self.part_masses[index]);
            }
        }
        if let Some(output) = observation.output {
            let witnesses = &contacts[first_contact..];
            if !witnesses.is_empty() || momentum != glam::DVec3::ZERO {
                let mut candidates: Vec<_> =
                    witnesses.iter().map(|c| ContactBodies { part: c.part, surface: c.surface }).collect();
                candidates.sort_unstable();
                candidates.dedup();
                let (attribution, point, normal) = if let [pair] = candidates.as_slice()
                    && !sleep_transition
                {
                    let first = witnesses[0];
                    let point = witnesses.iter().all(|c| c.point == first.point).then_some(first.point);
                    let normal = witnesses.iter().all(|c| c.normal == first.normal).then_some(first.normal);
                    (ImpulseAttribution::Pair(*pair), point, normal)
                } else {
                    (ImpulseAttribution::Unresolved(candidates), None, None)
                };
                output.push(observation.time.record(
                    attribution,
                    point,
                    normal,
                    ImpulseMeasurement::MomentumBalance(momentum.as_vec3().to_array()),
                ));
            }
        }
        Ok(())
    }
}

/// A one-axis joint uses a native hinge. Multi-axis joints use a constrained six-DOF joint.
fn joint_settings(p: &Part, anchor: RVec3) -> RagdollJoint {
    use SixDofConstraintAxis::{RotationX, RotationY, RotationZ, TranslationX, TranslationY, TranslationZ};
    let axes = [glam::Vec3::X, glam::Vec3::Y, glam::Vec3::Z].map(|v| Vec3::from((p.joint_rotation * v).to_array()));
    let mut free = p.limits_deg.iter().enumerate().filter(|(_, range)| range[0] != range[1]);
    if let Some((axis, &[lo, hi])) = free.next()
        && free.next().is_none()
    {
        let hinge = oxijolt::HingeConstraintSettings::new(anchor, axes[axis], axes[(axis + 1) % 3]);
        return RagdollJoint::Hinge(hinge.limits(lo.to_radians(), hi.to_radians()));
    }
    let mut j = SixDofConstraintSettings::new(anchor, axes[0], axes[1])
        .swing_type(if p.swing_envelope.is_independent() { SwingType::Pyramid } else { SwingType::Cone });

    for axis in [TranslationX, TranslationY, TranslationZ] {
        j = j.axis(axis, SixDofAxis::Fixed);
    }
    for (axis, [lo, hi]) in [RotationX, RotationY, RotationZ].into_iter().zip(p.limits_deg) {
        j = j.axis(
            axis,
            if lo == hi {
                SixDofAxis::Fixed
            } else {
                SixDofAxis::Limited { min: lo.to_radians(), max: hi.to_radians() }
            },
        );
    }
    RagdollJoint::SixDof(j)
}
