// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::{Primitive, StaticMesh};
use anyhow::{Result, ensure};
use bevy::math::{Mat4, Vec3};
use rapier3d::parry::{bounding_volume::Aabb, partitioning::Bvh};

pub(super) enum Source<'a> {
    Visual { parts: &'a [Primitive], ends: Vec<usize> },
    World(&'a StaticMesh),
}

impl<'a> Source<'a> {
    pub(super) fn visual(parts: &'a [Primitive]) -> Self {
        let mut total = 0;
        let ends = parts
            .iter()
            .map(|p| {
                total += p.indices.len() / 3;
                total
            })
            .collect();
        Self::Visual { parts, ends }
    }
    fn len(&self) -> usize {
        match self {
            Self::Visual { ends, .. } => ends.last().copied().unwrap_or(0),
            Self::World(mesh) => mesh.triangles.len(),
        }
    }
    pub(super) fn face(&self, triangle: usize) -> [[f32; 3]; 3] {
        match self {
            Self::Visual { parts, ends } => {
                let i = ends.partition_point(|&end| end <= triangle);
                let local = triangle - if i == 0 { 0 } else { ends[i - 1] };
                let part = &parts[i];
                part.indices.as_chunks::<3>().0[local].map(|id| part.positions[id as usize])
            }
            Self::World(mesh) => mesh.triangles[triangle].map(|id| mesh.vertices[id as usize]),
        }
    }
    /// Equal source IDs alone do not prove equal world rounding or orientation after a baked reflection.
    pub(super) fn matches(&self, matrix: Mat4, other: &StaticMesh) -> bool {
        let Self::Visual { parts, .. } = self else { return false };
        if self.len() != other.triangles.len()
            || parts.iter().map(|p| p.positions.len()).sum::<usize>() != other.vertices.len()
        {
            return false;
        }
        let mut base = 0usize;
        let mut faces = other.triangles.iter();
        for part in *parts {
            for face in part.indices.as_chunks::<3>().0 {
                if Some(face.map(|i| base + i as usize)) != faces.next().map(|f| f.map(|i| i as usize)) {
                    return false;
                }
            }
            for (p, world) in part.positions.iter().zip(&other.vertices[base..]) {
                let exact = matrix.transform_point3(Vec3::from(*p)).to_array().map(f32::to_bits);
                if exact != world.map(f32::to_bits) {
                    return false;
                }
            }
            base += part.positions.len();
        }
        true
    }
}

pub(super) fn equal_world_mesh(a: &StaticMesh, b: &StaticMesh) -> bool {
    a.vertices.len() == b.vertices.len()
        && a.triangles == b.triangles
        && a.vertices.iter().zip(&b.vertices).all(|(u, v)| u.map(f32::to_bits) == v.map(f32::to_bits))
}

pub(super) struct Mesh<'a> {
    pub source: Source<'a>,
    pub tree: Bvh,
}
impl<'a> Mesh<'a> {
    pub fn new(source: Source<'a>) -> Result<Self> {
        ensure!(u32::try_from(source.len()).is_ok(), "too many mesh faces for the query index");
        let bounds = (0..source.len())
            .map(|i| {
                let [a, b, c] = source.face(i).map(Vec3::from_array);
                Aabb::new(a.min(b).min(c).to_array().into(), a.max(b).max(c).to_array().into())
            })
            .collect::<Vec<_>>();
        Ok(Self { source, tree: Bvh::from_leaves(Default::default(), &bounds) })
    }
}

pub(super) enum Frame {
    World,
    Visual { matrix: Mat4, translation_only: bool },
}

impl Frame {
    pub fn visual(matrix: Mat4) -> Self {
        let translation_only = matrix.x_axis == Mat4::IDENTITY.x_axis
            && matrix.y_axis == Mat4::IDENTITY.y_axis
            && matrix.z_axis == Mat4::IDENTITY.z_axis;
        Self::Visual { matrix, translation_only }
    }
    pub fn matrix(&self) -> Mat4 {
        match self {
            Self::Visual { matrix, .. } => *matrix,
            Self::World => Mat4::IDENTITY,
        }
    }
    pub fn points(&self, face: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
        match self {
            Self::Visual { matrix, .. } => face.map(|p| matrix.transform_point3(Vec3::from(p)).to_array()),
            Self::World => face,
        }
    }

    /// Transform the local box with the same binary32 routine as its triangles. Each affine component
    /// is monotone on each axis, so its extrema occur at corners even under reflection or shear.
    /// A nonfinite projected corner leaves the node eligible for the exact world-space test.
    pub fn bounds(&self, lo: [f32; 3], hi: [f32; 3]) -> Option<Aabb> {
        let (lo, hi) = (Vec3::from(lo), Vec3::from(hi));
        let (lo, hi) = match self {
            Self::World => (lo, hi),
            Self::Visual { matrix, translation_only: true } => {
                let t = matrix.w_axis.truncate();
                (lo + t, hi + t)
            }
            Self::Visual { matrix, .. } => {
                let mut low = Vec3::splat(f32::INFINITY);
                let mut high = Vec3::splat(f32::NEG_INFINITY);
                for mask in 0..8 {
                    let corner = Vec3::new(
                        if mask & 1 == 0 { lo.x } else { hi.x },
                        if mask & 2 == 0 { lo.y } else { hi.y },
                        if mask & 4 == 0 { lo.z } else { hi.z },
                    );
                    let point = matrix.transform_point3(corner);
                    if !point.is_finite() {
                        return None;
                    }
                    low = low.min(point);
                    high = high.max(point);
                }
                (low, high)
            }
        };
        if !lo.is_finite() || !hi.is_finite() {
            return None;
        }
        let pad = lo.abs().max(hi.abs()).max(Vec3::ONE) * (4. * f32::EPSILON);
        let min = (lo - pad).max(Vec3::splat(-f32::MAX));
        let max = (hi + pad).min(Vec3::splat(f32::MAX));
        Some(Aabb::new(min.to_array().into(), max.to_array().into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projected_node_encloses_f32_faces_for_reflections_shear_and_collapsed_axes() {
        let lo = [-8., -2., 1.];
        let hi = [11., 7., 13.];
        let shear = Mat4::from_cols_array_2d(&[
            [-1., 0.25, 0., 0.],
            [0.5, 1., 0., 0.],
            [0., 0.125, 2., 0.],
            [1048576., 1024., -131072., 1.],
        ]);
        for matrix in [shear, Mat4::from_scale(Vec3::new(1., 0., 1.)), Mat4::IDENTITY] {
            let f = Frame::visual(matrix);
            let bounds = f.bounds(lo, hi).unwrap();
            for x in [lo[0], 1., hi[0]] {
                for y in [lo[1], 3., hi[1]] {
                    for z in [lo[2], 5., hi[2]] {
                        let p = matrix.transform_point3(Vec3::new(x, y, z));
                        assert!(p.cmpge(Vec3::from_array(bounds.mins.to_array())).all());
                        assert!(p.cmple(Vec3::from_array(bounds.maxs.to_array())).all());
                    }
                }
            }
        }
        let overflowing = Frame::visual(Mat4::from_scale(Vec3::splat(f32::MAX)));
        assert!(overflowing.bounds(lo, hi).is_none());
    }
}
