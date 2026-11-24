// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Passive angular friction on unloaded branches of an articulated body.

use crate::Part;
use rapier3d::math::Pose as Isometry;
use rapier3d::prelude::*;

struct Link {
    parent: usize,
    child: usize,
    joint: ImpulseJointHandle,
    free_axes: JointAxesMask,
}

#[derive(Default, Clone, Copy)]
struct Segment {
    unloaded_s: f32,
    resting_on_contact: bool,
    hanging: bool,
}

#[derive(Default)]
pub(super) struct HangingBranches {
    links: Vec<Link>,
    segments: Vec<Segment>,
}

impl HangingBranches {
    pub(super) fn new(
        sim: &mut PhysicsWorld,
        bodies: &[(RigidBodyHandle, ColliderHandle)],
        rig: &[Part],
        joints: Vec<ImpulseJointHandle>,
    ) -> Self {
        if joints.is_empty() {
            return Self::default();
        }
        // Accumulate masses and inertias in the authored rig frame, independent of
        // the world's translation or initial animation. Children follow their parents.
        let mut masses: Vec<_> = rig
            .iter()
            .zip(bodies)
            .map(|(part, &(body, _))| {
                let pose = Isometry::from_parts(part.center, part.rotation);
                sim.bodies[body].mass_properties().local_mprops.transform_by(&pose)
            })
            .collect();
        let mut has_children = vec![false; rig.len()];
        let mut links = Vec::with_capacity(joints.len());
        let gravity = sim.gravity.length();
        for (child, handle) in (1..rig.len()).zip(joints).rev() {
            let part = &rig[child];
            let parent = part.parent.expect("validated joint tree");
            let joint = &mut sim.impulse_joints.get_mut(handle, false).unwrap().data;
            let free_axes = JointAxesMask::ANG_AXES & !joint.locked_axes;
            let mass = masses[child];
            if !free_axes.is_empty() {
                let (model, damping) = if has_children[child] {
                    // Approximate the hanging assembly as a compound pendulum in its
                    // bind pose. Use the parallel-axis moment about each joint axis.
                    let arm = mass.local_com - part.anchor;
                    let length = arm.length().max(part.radius + part.half_segment);
                    let gravity_torque = mass.mass() * gravity * length;
                    let inertia = mass.reconstruct_inertia_matrix();
                    let damping = [Vector::X, Vector::Y, Vector::Z].map(|axis| {
                        let axis = part.joint_rotation * axis;
                        let moment = axis.dot(inertia * axis) + mass.mass() * arm.cross(axis).length_squared();
                        2. * (moment.max(0.) * gravity_torque).sqrt()
                    });
                    (MotorModel::ForceBased, damping)
                } else {
                    let radius = sim.colliders[bodies[child].1].shape().compute_local_bounding_sphere().radius;
                    let length = joint.local_frame2.translation.length().max(radius);
                    (MotorModel::AccelerationBased, [(gravity / length).sqrt(); 3])
                };
                for (axis, damping) in [JointAxis::AngX, JointAxis::AngY, JointAxis::AngZ].into_iter().zip(damping) {
                    if free_axes.contains(axis.into()) {
                        joint.set_motor_model(axis, model).set_motor_velocity(axis, 0., damping);
                    }
                }
                joint.motor_axes = JointAxesMask::empty();
            }
            masses[parent] += mass;
            has_children[parent] = true;
            links.push(Link { parent, child, joint: handle, free_axes });
        }
        links.reverse();
        Self { links, segments: vec![Segment::default(); rig.len()] }
    }

    pub(super) fn update(&mut self, sim: &mut PhysicsWorld, bodies: &[(RigidBodyHandle, ColliderHandle)], dt: f32) {
        for (segment, &(body, collider)) in self.segments.iter_mut().zip(bodies) {
            let activation = sim.bodies[body].activation();
            let quiet = activation.is_eligible_for_sleep();
            let mut loaded = false;
            segment.resting_on_contact = false;
            for pair in sim.contact_pairs_with(collider) {
                if pair.total_impulse_magnitude() > 0. {
                    loaded = true;
                    if !quiet {
                        break;
                    }
                    let up = if pair.collider2 == collider { -sim.gravity } else { sim.gravity };
                    if pair.total_impulse().dot(up) > 0. {
                        segment.resting_on_contact = true;
                        break;
                    }
                }
            }
            segment.unloaded_s = if loaded { 0. } else { (segment.unloaded_s + dt).min(activation.time_until_sleep) };
        }
        // A hand or foot receiving an impulse also loads its upstream joints.
        for link in self.links.iter().rev() {
            let below = self.segments[link.child].unloaded_s;
            self.segments[link.parent].unloaded_s = self.segments[link.parent].unloaded_s.min(below);
        }
        // An unloaded branch inherits its supported attachment, even through a fixed joint.
        for link in &self.links {
            let parent = self.segments[link.parent];
            let child = &mut self.segments[link.child];
            let body = &sim.bodies[bodies[link.child].0];
            let hanging = (parent.resting_on_contact || parent.hanging)
                && child.unloaded_s >= body.activation().time_until_sleep
                && !body.is_sleeping();
            if child.hanging != hanging {
                child.hanging = hanging;
                let joint = &mut sim.impulse_joints.get_mut(link.joint, false).unwrap().data;
                joint.motor_axes = if hanging { link.free_axes } else { JointAxesMask::empty() };
            }
        }
    }
}
