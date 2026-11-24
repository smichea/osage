// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

use glam::{Quat, Vec3};

/// One centred bind-pose sphere or Y-axis capsule. Positions and joint frames use rig coordinates.
#[derive(Clone, Debug)]
pub struct Part {
    pub name: String,
    pub parent: Option<usize>,
    pub center: Vec3,
    pub rotation: Quat,
    pub radius: f32,
    pub half_segment: f32,
    pub mass: f32,
    pub inertia: Option<Vec3>,
    pub material: Option<osage_objects::CollisionMaterial>,
    pub joint_rotation: Quat,
    pub anchor: Vec3,
    pub limits_deg: [[f32; 2]; 3],
}

impl Part {
    /// A capsule fitted between two joint landmarks, with its local Y along the bone.
    pub fn between(
        name: &'static str,
        parent: Option<usize>,
        ends: [[f32; 3]; 2],
        radius: f32,
        mass: f32,
        limits_deg: [[f32; 2]; 3],
    ) -> Self {
        let [a, b] = ends.map(Vec3::from_array);
        let length = a.distance(b);
        Self {
            name: name.to_owned(),
            inertia: None,
            material: None,
            joint_rotation: Quat::IDENTITY,
            parent,
            center: (a + b) * 0.5,
            rotation: Quat::from_rotation_arc(Vec3::Y, (b - a) / length),
            radius,
            half_segment: (length * 0.5 - radius).max(0.0),
            mass,
            anchor: a,
            limits_deg,
        }
    }
}
