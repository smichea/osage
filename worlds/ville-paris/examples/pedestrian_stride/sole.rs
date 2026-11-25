// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use anyhow::{Context, Result, ensure};
use bevy::{
    mesh::{
        VertexAttributeValues,
        skinning::{SkinnedMesh, SkinnedMeshInverseBindposes},
    },
    prelude::{Assets, Children, Entity, GlobalTransform, Mat4, Mesh, Mesh3d, Vec3, World},
};
use std::collections::HashSet;

struct Vertex {
    joints: [usize; 4],
    weights: [f32; 4],
    bind_position: Vec3,
}

pub(super) struct Sole {
    palette: Vec<(Entity, Mat4)>,
    vertices: Vec<Vertex>,
}

impl Sole {
    /// Match rendered vertices whose skin weight belongs predominantly to this foot subtree.
    pub(super) fn bind(world: &World, actor: Entity, foot: Entity) -> Result<Self> {
        let mut subtree = HashSet::new();
        let mut stack = vec![foot];
        while let Some(id) = stack.pop() {
            subtree.insert(id);
            if let Some(children) = world.get::<Children>(id) {
                stack.extend(children.iter());
            }
        }
        let mut palette = Vec::new();
        let mut vertices = Vec::new();
        for &part in world.get::<Children>(actor).context("missing body parts")?.iter() {
            let Some(skin) = world.get::<SkinnedMesh>(part) else { continue };
            let mesh_handle = world.get::<Mesh3d>(part).context("skinned part without mesh")?;
            let mesh = world.resource::<Assets<Mesh>>().get(&mesh_handle.0).context("unloaded mesh")?;
            let bind = world
                .resource::<Assets<SkinnedMeshInverseBindposes>>()
                .get(&skin.inverse_bindposes)
                .context("missing inverse bind poses")?;
            ensure!(bind.len() == skin.joints.len(), "joint palette size");
            let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
                anyhow::bail!("positions")
            };
            let Some(VertexAttributeValues::Uint16x4(joints)) = mesh.attribute(Mesh::ATTRIBUTE_JOINT_INDEX) else {
                anyhow::bail!("joint indices")
            };
            let Some(VertexAttributeValues::Float32x4(weights)) = mesh.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT) else {
                anyhow::bail!("joint weights")
            };
            let mut used = vec![mesh.indices().is_none(); positions.len()];
            if let Some(indices) = mesh.indices() {
                for i in indices.iter() {
                    used[i] = true;
                }
            }
            let base = palette.len();
            for (i, rendered) in used.into_iter().enumerate() {
                if !rendered {
                    continue;
                }
                let indices = joints[i].map(usize::from);
                let weight: f32 =
                    (0..4).filter(|&j| subtree.contains(&skin.joints[indices[j]])).map(|j| weights[i][j]).sum();
                if weight > 0.5 {
                    vertices.push(Vertex {
                        joints: indices.map(|j| base + j),
                        weights: weights[i],
                        bind_position: positions[i].into(),
                    });
                }
            }
            palette.extend(skin.joints.iter().copied().zip(bind.iter().copied()));
        }
        ensure!(!vertices.is_empty(), "foot has no rendered vertices");
        Ok(Self { palette, vertices })
    }

    /// Minimum signed vertical distance from a skinned vertex to the surface at its own X/Z.
    pub(super) fn clearance(&self, world: &World, ground: impl Fn(Vec3) -> Option<f32>) -> Result<f32> {
        let skin: Vec<Mat4> = self
            .palette
            .iter()
            .map(|&(id, inverse)| {
                Ok(world.get::<GlobalTransform>(id).context("missing joint transform")?.to_matrix() * inverse)
            })
            .collect::<Result<_>>()?;
        let mut lowest = f32::INFINITY;
        for v in &self.vertices {
            let mut p = Vec3::ZERO;
            for (i, weight) in v.joints.into_iter().zip(v.weights) {
                if weight > 0. {
                    p += skin[i].transform_point3(v.bind_position) * weight;
                }
            }
            lowest = lowest.min(p.y - ground(p).context("foot outside reference surface")?);
        }
        Ok(lowest)
    }
}
