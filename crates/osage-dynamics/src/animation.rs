// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! A pair of sampled actor-space poses at the animation-to-dynamics transition.
use super::{Part, Pose};
use anyhow::{Result, ensure};
use glam::{Quat, Vec3};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Deserialize)]
/// Centre of mass and orientation of one collision body in actor coordinates.
pub struct BodyTransform {
    pub center_m: [f32; 3],
    pub rotation_xyzw: [f32; 4],
}

#[derive(Debug, Deserialize)]
/// Two samples of the same body, ordered by time.
pub struct AnimatedBody {
    pub name: String,
    pub previous: BodyTransform,
    pub current: BodyTransform,
}

#[derive(Debug, Deserialize)]
/// Samples at activation and one positive interval earlier, identified by rig body name.
pub struct AnimationHandoff {
    pub sample_interval_s: f32,
    pub parts: Vec<AnimatedBody>,
}

/// Each sample uses actor coordinates. A shared origin and velocity place the character in the world.
pub struct RigStart<'a> {
    pub origin_m: [f32; 3],
    pub velocity_m_s: [f32; 3],
    pub animation: Option<&'a AnimationHandoff>,
}

impl AnimationHandoff {
    pub(super) fn initial_poses(&self, rig: &[Part], origin: Vec3, velocity: Vec3) -> Result<Vec<Pose>> {
        let dt = self.sample_interval_s;
        ensure!(dt.is_finite() && dt > 0., "invalid animation sample interval");
        let mut samples: HashMap<_, _> = self.parts.iter().map(|b| (b.name.as_str(), b)).collect();
        ensure!(samples.len() == self.parts.len() && samples.len() == rig.len(), "animation/body count mismatch");
        let mut poses: Vec<Pose> = Vec::with_capacity(rig.len());
        for part in rig {
            let sample = samples
                .remove(part.name.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing animation sample for {}", part.name))?;
            let [a, b] = [sample.previous, sample.current]
                .map(|p| (Vec3::from_array(p.center_m), Quat::from_array(p.rotation_xyzw)));
            ensure!(
                a.0.is_finite() && b.0.is_finite() && a.1.is_normalized() && b.1.is_normalized(),
                "invalid animated transform for {}",
                part.name
            );
            let mut delta = (b.1 * a.1.conjugate()).normalize();
            if delta.w < 0. {
                delta = -delta;
            }
            let angular = delta.to_scaled_axis() / dt;
            let linear = if let Some(parent) = part.parent {
                // Both ends of a rigid joint must start with the same point velocity.
                // The two lever arms use their current orientations and their bind anchors.
                let parent_pose = poses[parent];
                let parent_bind = &rig[parent];
                let parent_arm = (Quat::from_array(parent_pose.rotation_xyzw) * parent_bind.rotation.conjugate())
                    * (part.anchor - parent_bind.center);
                let child_arm = (b.1 * part.rotation.conjugate()) * (part.anchor - part.center);
                Vec3::from_array(parent_pose.linear_velocity)
                    + Vec3::from_array(parent_pose.angular_velocity).cross(parent_arm)
                    - angular.cross(child_arm)
            } else {
                velocity + (b.0 - a.0) / dt
            };
            let center = origin + b.0;
            ensure!(
                angular.is_finite() && linear.is_finite() && center.is_finite(),
                "invalid animated motion for {}",
                part.name
            );
            poses.push(Pose {
                center: center.to_array(),
                rotation_xyzw: b.1.to_array(),
                linear_velocity: linear.to_array(),
                angular_velocity: angular.to_array(),
            });
        }
        Ok(poses)
    }
}
