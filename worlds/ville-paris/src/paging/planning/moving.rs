// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Reserve acoustic geometry along a possible contact between two moving shapes.
use super::Motion;
use crate::{impact::paged::Contact, paging::solver::World, replay::Replay};
use osage_dynamics::{DynamicBox, Pose};
use rapier3d::parry::{
    math::{Pose as Transform, Rotation, Vector},
    query::{NonlinearRigidMotion, cast_shapes_nonlinear},
    shape::{Capsule, Cuboid},
};

struct Trajectory {
    motion: Motion,
    rotation: Rotation,
    angular_velocity: Vector,
}

impl Trajectory {
    fn new(motion: Motion, pose: &Pose) -> Self {
        Self {
            motion,
            rotation: Rotation::from_array(pose.rotation_xyzw),
            angular_velocity: Vector::from_array(pose.angular_velocity),
        }
    }

    fn center(&self, time: f64) -> Vector {
        let mut center = self.motion.center + self.motion.velocity * time;
        center.y = self.motion.height(time);
        Vector::from_array(center.to_array().map(|v| v as f32))
    }

    fn interval(&self, from: f64, to: f64) -> NonlinearRigidMotion {
        let center = self.center(from);
        let rotation = Rotation::from_scaled_axis(self.angular_velocity * from as f32) * self.rotation;
        NonlinearRigidMotion::new(
            Transform::from_parts(center, rotation),
            Vector::ZERO,
            (self.center(to) - center) / (to - from) as f32,
            self.angular_velocity,
        )
    }

    fn point_velocity(&self, time: f64, point: Vector) -> Vector {
        let mut vel = self.motion.velocity;
        vel.y = (vel.y - self.motion.gravity * time).max(-self.motion.terminal);
        Vector::from_array(vel.to_array().map(|v| v as f32)) + self.angular_velocity.cross(point - self.center(time))
    }
}

fn first_contact(
    actor: &Trajectory,
    capsule: &Capsule,
    other: &Trajectory,
    cuboid: &Cuboid,
    seconds: f64,
) -> Option<(f64, Vector, f32)> {
    // At normal replay horizons each gravity chord deviates by at most 1 cm.
    // Very long reservations have a bounded cost and are refined on the next tick.
    let gravity = actor.motion.gravity.max(other.motion.gravity);
    let steps = (seconds * (gravity / 0.08).sqrt()).ceil().clamp(1., 32.) as u32;
    let duration = seconds / f64::from(steps);
    let radius = actor.motion.extent + other.motion.extent + gravity * duration * duration / 4.;
    for i in 0..steps {
        let start = f64::from(i) * duration;
        let end = f64::from(i + 1) * duration;
        let offset = actor.center(start) - other.center(start);
        let travel = actor.center(end) - other.center(end) - offset;
        let length = travel.length_squared();
        let u = if length > 0. { (-offset.dot(travel) / length).clamp(0., 1.) } else { 0. };
        if (offset + travel * u).length_squared() > (radius * radius) as f32 {
            continue;
        }
        let a = actor.interval(start, end);
        let b = other.interval(start, end);
        if let Ok(Some(hit)) = cast_shapes_nonlinear(&a, capsule, &b, cuboid, 0., duration as f32, false) {
            let time = start + f64::from(hit.time_of_impact);
            let pose = b.position_at_time(hit.time_of_impact);
            let point = pose * hit.witness2;
            let normal = pose.rotation * hit.normal2;
            let closing = (other.point_velocity(time, point) - actor.point_velocity(time, point)).dot(normal);
            if closing > 0.5 {
                return Some((time, point, closing));
            }
            // The shapes already meet here. Further motion depends on the unsolved reaction.
            return None;
        }
    }
    None
}

pub(in crate::paging) fn predict(
    world: &World,
    replay: &Replay,
    bonnet: DynamicBox,
    motions: &[Motion],
    tick: u32,
    horizon: f64,
    forecasts: &mut Vec<(usize, Contact)>,
) {
    let Some(mut motion) = world.box_motion(replay) else { return };
    let pose = &world.box_poses()[0];
    // A box resting on a road and a freely falling box need different extrapolations.
    if let Some(before) = world.prior_box_poses().first().filter(|_| tick > 0) {
        let acceleration = f64::from(before.linear_velocity[1] - pose.linear_velocity[1]) * f64::from(replay.tick_hz);
        motion.gravity = acceleration.clamp(0., motion.gravity);
    }
    let body = Trajectory::new(motion, pose);
    let shape = Cuboid::new(Vector::from_array(bonnet.half_extents_m));
    let static_predictions = forecasts.len();
    let mut earliest = u32::MAX;
    for (i, ((part, pose), &motion)) in world.parts().iter().zip(world.part_poses()).zip(motions).enumerate() {
        let capsule = Capsule::new_y(part.half_segment, part.radius);
        if let Some((time, point, closing)) =
            first_contact(&Trajectory::new(motion, pose), &capsule, &body, &shape, horizon)
        {
            let at = tick.saturating_add((time * f64::from(replay.tick_hz)).ceil() as u32);
            if at < earliest {
                earliest = at;
                forecasts.truncate(static_predictions);
            }
            if at == earliest {
                let contact = Contact {
                    tick: at,
                    point: point.to_array(),
                    speed: closing,
                    surface: "bonnet".into(),
                    owner: None,
                    part: Some(i),
                    substep: None,
                    voice: None,
                    measured: None,
                };
                forecasts.push((world.parts().len() * 3 + i, contact));
            }
        }
    }
}
