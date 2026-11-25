// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::{FootChain, FootGround, solve_two_bone};
use bevy::mesh::{
    VertexAttributeValues,
    skinning::{SkinnedMesh, SkinnedMeshInverseBindposes},
};
use bevy::prelude::*;
use std::collections::HashMap;

/// Ground samples skinned by the upper link, lower link, ankle and contact node.
/// Each input lane stores a point in that joint's bind-local frame (XYZ) and its weight (W).
#[derive(Component, Clone)]
#[component(immutable)]
pub struct SoleSkin {
    points: std::sync::Arc<[[Vec4; 4]]>,
    triangles: std::sync::Arc<[[usize; 3]]>,
}

impl SoleSkin {
    pub fn new(points: Vec<[Vec4; 4]>) -> Option<Self> {
        Self::from_triangles(points, Vec::new())
    }

    /// Indices refer to the input samples before duplicate skin bindings are merged.
    pub fn from_triangles(mut points: Vec<[Vec4; 4]>, mut triangles: Vec<[usize; 3]>) -> Option<Self> {
        if points.is_empty() {
            return None;
        }
        for row in &mut points {
            let total: f32 = row.iter().map(|p| p.w).sum();
            if !row.iter().all(|p| p.is_finite() && p.w >= 0.) || (total - 1.).abs() > 1e-4 {
                return None;
            }
            for lane in row {
                *lane = lane.truncate().extend(1.) * (lane.w / total);
            }
        }
        let mut seen = HashMap::new();
        let mut unique = Vec::new();
        let mut slots = Vec::with_capacity(points.len());
        for row in points {
            let slot = *seen.entry(row.map(|v| v.to_array().map(f32::to_bits))).or_insert_with(|| {
                unique.push(row);
                unique.len() - 1
            });
            slots.push(slot);
        }
        for id in triangles.iter_mut().flatten() {
            *id = *slots.get(*id)?;
        }
        Some(Self { points: unique.into(), triangles: triangles.into() })
    }

    /// Bind drawn base-mesh vertices with majority ankle/contact influence. The four limb
    /// nodes must account for all influences of each selected vertex. Meshes must be loaded.
    pub fn from_actor(world: &World, actor: Entity, chain: FootChain) -> Result<Self, &'static str> {
        let meshes = world.get_resource::<Assets<Mesh>>().ok_or("mesh assets unavailable")?;
        let poses = world.get_resource::<Assets<SkinnedMeshInverseBindposes>>().ok_or("bind poses unavailable")?;
        let mut stack = vec![actor];
        let mut samples = Vec::new();
        let mut faces = Vec::new();
        while let Some(id) = stack.pop() {
            if let Some(children) = world.get::<Children>(id) {
                stack.extend(children.iter());
            }
            let Some(skin) = world.get::<SkinnedMesh>(id) else { continue };
            if !skin.joints.contains(&chain.ankle) && !skin.joints.contains(&chain.contact) {
                continue;
            }
            let mesh =
                world.get::<Mesh3d>(id).and_then(|handle| meshes.get(&handle.0)).ok_or("foot mesh unavailable")?;
            let bind = poses.get(&skin.inverse_bindposes).ok_or("foot inverse bind poses unavailable")?;
            if bind.len() != skin.joints.len() {
                return Err("invalid skin palette");
            }
            let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
                return Err("missing skin positions");
            };
            let Some(VertexAttributeValues::Float32x4(weights)) = mesh.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT) else {
                return Err("missing skin weights");
            };
            let indices = mesh.attribute(Mesh::ATTRIBUTE_JOINT_INDEX).ok_or("missing skin joints")?;
            let slots: Vec<_> =
                skin.joints.iter().map(|joint| chain.nodes().iter().position(|id| id == joint)).collect();
            let mut used = vec![mesh.indices().is_none(); positions.len()];
            if let Some(triangles) = mesh.indices() {
                for i in triangles.iter() {
                    *used.get_mut(i).ok_or("mesh index outside positions")? = true;
                }
            }
            let mut vertex_slots = vec![None; positions.len()];
            for (i, drawn) in used.into_iter().enumerate() {
                if !drawn {
                    continue;
                }
                let indices = match indices {
                    VertexAttributeValues::Uint16x4(v) => v.get(i).map(|row| row.map(usize::from)),
                    VertexAttributeValues::Uint8x4(v) => v.get(i).map(|row| row.map(usize::from)),
                    _ => None,
                }
                .ok_or("invalid skin joint indices")?;
                let weights = weights.get(i).ok_or("missing vertex weights")?;
                let mut foot_weight = 0.;
                let mut outside = false;
                let mut row = [Vec4::ZERO; 4];
                for (joint, &w) in indices.into_iter().zip(weights) {
                    if w == 0. {
                        continue;
                    }
                    match *slots.get(joint).ok_or("joint outside palette")? {
                        Some(slot) => {
                            if slot >= 2 {
                                foot_weight += w;
                            }
                            let point = bind[joint].transform_point3(positions[i].into());
                            row[slot] = point.extend(row[slot].w + w);
                        }
                        None => outside = true,
                    }
                }
                if foot_weight > 0.5 {
                    if outside {
                        return Err("foot vertex depends on a joint outside its limb");
                    }
                    vertex_slots[i] = Some(samples.len());
                    samples.push(row);
                }
            }
            if mesh.primitive_topology() == bevy::mesh::PrimitiveTopology::TriangleList {
                let mut collect_face = |a, b, c| {
                    if let [Some(x), Some(y), Some(z)] = [vertex_slots[a], vertex_slots[b], vertex_slots[c]] {
                        faces.push([x, y, z]);
                    }
                };
                if let Some(indices) = mesh.indices() {
                    let mut iter = indices.iter();
                    while let (Some(a), Some(b), Some(c)) = (iter.next(), iter.next(), iter.next()) {
                        collect_face(a, b, c);
                    }
                } else {
                    for i in 0..positions.len() / 3 {
                        collect_face(3 * i, 3 * i + 1, 3 * i + 2);
                    }
                }
            }
        }
        Self::from_triangles(samples, faces).ok_or("empty or invalid foot skin")
    }

    pub(super) fn lift(
        &self,
        joints: [GlobalTransform; 4],
        offset: Vec3,
        ground: Option<&FootGround>,
        root_y: f32,
        positions: &mut Vec<Vec3>,
    ) -> Option<f32> {
        let matrices = joints.map(|j| j.to_matrix());
        let posed = self.points.iter().map(|point| {
            let mut position = offset;
            for (joint, &lane) in matrices.iter().zip(point) {
                if lane.w > 0. {
                    position += (*joint * lane).truncate();
                }
            }
            position
        });
        let vertex_lift = |p: Vec3| {
            let height = match ground {
                Some(g) => g.support(p, root_y)?,
                None => root_y,
            };
            Some(height - p.y)
        };
        let Some((ground, query)) =
            ground.and_then(|g| Some((g, g.surface.as_ref()?))).filter(|_| !self.triangles.is_empty())
        else {
            return posed.filter_map(vertex_lift).reduce(f32::max);
        };
        positions.clear();
        positions.extend(posed);
        let range = root_y - ground.max_step_m..=root_y + ground.max_step_m;
        positions
            .iter()
            .copied()
            .filter_map(vertex_lift)
            .chain(
                self.triangles
                    .iter()
                    .filter_map(|face| query(face.map(|i| positions[i]), range.clone()).filter(|v| v.is_finite())),
            )
            .reduce(f32::max)
    }
}

pub(super) struct SkinFrame<'a> {
    pub parent: GlobalTransform,
    pub joints: [Transform; 4],
    pub ground: Option<&'a FootGround>,
    pub root_y: f32,
    pub scale: f32,
}

impl SkinFrame<'_> {
    /// Re-evaluate the blended vertices after IK. Keep the least-penetrating reachable pose.
    pub fn correct(
        &self,
        skin: &SoleSkin,
        mut target: Vec3,
        initial: [Quat; 3],
        positions: &mut Vec<Vec3>,
    ) -> [Quat; 3] {
        let [hip, knee, ankle, toe] = self.joints;
        let up = self.parent.affine().inverse().transform_vector3(Vec3::Y);
        let mut current = initial;
        let mut best = initial;
        let mut best_depth = f32::INFINITY;
        // Blended skin and an edge crossing can lift more slowly than the ankle target.
        const MAX_REFINEMENTS: usize = 8;
        for pass in 0..=MAX_REFINEMENTS {
            let [h, k, a] = current;
            let thigh = self.parent.mul_transform(hip.with_rotation(h));
            let shin = thigh.mul_transform(knee.with_rotation(k));
            let shoe = shin.mul_transform(ankle.with_rotation(a));
            let depth = skin
                .lift([thigh, shin, shoe, shoe.mul_transform(toe)], Vec3::ZERO, self.ground, self.root_y, positions)
                .unwrap_or(0.);
            if depth >= best_depth {
                break;
            }
            best = current;
            best_depth = depth;
            if depth <= 1e-5 * self.scale || pass == MAX_REFINEMENTS {
                break;
            }
            target += up * depth;
            let Some(next) = solve_two_bone(hip, knee, ankle, target) else { break };
            current = next;
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shared_vertex_follows_both_bones_and_tests_its_own_side_of_the_edge() {
        let zero = Vec4::ZERO;
        let row = [zero, Vec4::new(0., 0., 0., 0.3), Vec4::new(0., 0., 0., 0.7), zero];
        let skin = SoleSkin::new(vec![row, row]).unwrap();
        assert_eq!(skin.points.len(), 1);
        let mut joints = [GlobalTransform::IDENTITY; 4];
        joints[1] = GlobalTransform::from_translation(Vec3::new(0., 0.04, -0.04));
        joints[2] = GlobalTransform::from_translation(Vec3::new(0., 0.02, 0.01));
        let ground = FootGround::new(0.1, |p| Some(if p.z < 0. { 0.08 } else { 0. }));
        let lift = skin.lift(joints, Vec3::ZERO, Some(&ground), 0., &mut Vec::new()).unwrap();
        assert!((lift - 0.054).abs() < 1e-6);
        assert!(skin.lift(joints, Vec3::Z * 0.01, Some(&ground), 0., &mut Vec::new()).unwrap() < 0.);
        assert!(SoleSkin::new(vec![[zero; 4]]).is_none());
    }
}
