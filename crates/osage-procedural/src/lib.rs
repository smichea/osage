// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Static scenery, sculpted from simple shapes and gathered into a few
//! meshes. A tree made of four objects, a thousand times, costs a draw and a
//! visibility test per object, in the main view and again in every shadow
//! cascade. Here each shape is placed, painted through its vertex colours and
//! appended to the mesh of the patch of land it stands on: the same triangles
//! in a few dozen draws, still culled patch by patch.
//!
//! A [`Shape`] is a mesh in its own space: a Bevy primitive, a solid of
//! revolution ([`Shape::lathe`]), a tube along a path ([`Shape::sweep`]) or
//! flat faces ([`Shape::faces`]). A [`Paint`] colours it, plainly or from its
//! foot to its top. A [`Batch`] gathers placed shapes that share a material.
//! A [`Sculpture`] blends pieces into one organic surface: a figure, cloth.

use std::collections::BTreeMap;

pub mod sculpt;
pub use sculpt::Sculpture;

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, VertexAttributeValues},
    prelude::*,
    render::render_resource::PrimitiveTopology,
};

/// Side of a patch of land, metres: what is gathered into one mesh.
const PATCH: f32 = 90.0;

/// A mesh in its own space, with front faces counter-clockwise.
#[derive(Clone)]
pub struct Shape {
    positions: Vec<Vec3>,
    normals: Vec<Vec3>,
    indices: Vec<u32>,
    /// Its lowest and highest points, for paints that change with height.
    foot: f32,
    top: f32,
}

impl Shape {
    fn from_parts(positions: Vec<Vec3>, normals: Vec<Vec3>, indices: Vec<u32>) -> Self {
        let (foot, top) = positions.iter().fold((f32::MAX, f32::MIN), |(low, high), p| (low.min(p.y), high.max(p.y)));
        Self { positions, normals, indices, foot, top }
    }

    /// A Bevy mesh as a shape: a primitive, or anything with positions and normals.
    pub fn new(mesh: impl Into<Mesh>) -> Self {
        let mesh: Mesh = mesh.into();
        let read = |attribute| match mesh.attribute(attribute) {
            Some(VertexAttributeValues::Float32x3(values)) => values.iter().map(|v| Vec3::from(*v)).collect(),
            _ => Vec::new(),
        };
        let positions: Vec<Vec3> = read(Mesh::ATTRIBUTE_POSITION);
        let mut normals: Vec<Vec3> = read(Mesh::ATTRIBUTE_NORMAL);
        normals.resize(positions.len(), Vec3::Y);
        let indices = match mesh.indices() {
            Some(indices) => indices.iter().map(|i| i as u32).collect(),
            None => (0..positions.len() as u32).collect(),
        };
        Self::from_parts(positions, normals, indices)
    }

    /// A solid of revolution around the Y axis: `profile` lists (radius,
    /// height) from the foot upwards, the outside on the side of larger radii.
    pub fn lathe(profile: &[(f32, f32)], sides: usize) -> Self {
        Self::gore(profile, sides, 0.0, std::f32::consts::TAU)
    }

    /// A slice of a solid of revolution: from angle `from`, over `turn`
    /// radians, in `sides` facets. Slices of alternate colours make stripes.
    pub fn gore(profile: &[(f32, f32)], sides: usize, from: f32, turn: f32) -> Self {
        let (mut positions, mut normals, mut indices) = (Vec::new(), Vec::new(), Vec::new());
        let ring = sides as u32 + 1;
        for (i, &(radius, height)) in profile.iter().enumerate() {
            let (before, after) = (profile[i.saturating_sub(1)], profile[(i + 1).min(profile.len() - 1)]);
            // Across the profile's direction, turned outwards.
            let across = Vec2::new(after.1 - before.1, before.0 - after.0).normalize_or(Vec2::X);
            for j in 0..=sides {
                let (sin, cos) = (from + j as f32 / sides as f32 * turn).sin_cos();
                positions.push(Vec3::new(radius * cos, height, radius * sin));
                normals.push(Vec3::new(across.x * cos, across.y, across.x * sin));
            }
            if i + 1 < profile.len() {
                for j in 0..sides as u32 {
                    let (a, b) = (i as u32 * ring + j, i as u32 * ring + j + 1);
                    let (d, c) = (a + ring, b + ring);
                    indices.extend([a, d, b, b, d, c]);
                }
            }
        }
        Self::from_parts(positions, normals, indices)
    }

    /// A round tube of `radius` along `path`. `beside` is any direction not
    /// along the path: the tube's seam lies on that side.
    pub fn sweep(path: &[Vec3], radius: f32, sides: usize, beside: Vec3) -> Self {
        let (mut positions, mut normals, mut indices) = (Vec::new(), Vec::new(), Vec::new());
        let ring = sides as u32 + 1;
        for (k, &at) in path.iter().enumerate() {
            let along = (path[(k + 1).min(path.len() - 1)] - path[k.saturating_sub(1)]).normalize_or(Vec3::Y);
            let out = beside.cross(along).normalize_or(Vec3::X);
            let up = along.cross(out);
            for j in 0..=sides {
                let (sin, cos) = (j as f32 / sides as f32 * std::f32::consts::TAU).sin_cos();
                let normal = out * cos + up * sin;
                positions.push(at + normal * radius);
                normals.push(normal);
            }
            if k + 1 < path.len() {
                for j in 0..sides as u32 {
                    let (a, b) = (k as u32 * ring + j, k as u32 * ring + j + 1);
                    let (d, c) = (a + ring, b + ring);
                    indices.extend([a, b, d, b, c, d]);
                }
            }
        }
        Self::from_parts(positions, normals, indices)
    }

    /// Flat faces: each a convex polygon whose corners turn counter-clockwise
    /// seen from its front. With `both_sides`, each face is also seen from behind.
    pub fn faces(faces: &[&[Vec3]], both_sides: bool) -> Self {
        let (mut positions, mut normals, mut indices) = (Vec::new(), Vec::new(), Vec::new());
        for face in faces {
            let normal = (face[1] - face[0]).cross(face[2] - face[0]).normalize_or(Vec3::Y);
            for (side, turn) in [(1.0, [1, 2]), (-1.0, [2, 1])].into_iter().take(if both_sides { 2 } else { 1 }) {
                let first = positions.len() as u32;
                positions.extend_from_slice(face);
                normals.extend(std::iter::repeat_n(normal * side, face.len()));
                for corner in 1..face.len() as u32 - 1 {
                    indices.extend([first, first + corner + turn[0] - 1, first + corner + turn[1] - 1]);
                }
            }
        }
        Self::from_parts(positions, normals, indices)
    }

    /// A prism: the convex `outline` in the XY plane, drawn from `near` to
    /// `far` along Z, with flat faces and crisp edges, as moulded plastic
    /// or sheet metal. The outline may turn either way.
    pub fn extrude(outline: &[Vec2], near: f32, far: f32) -> Self {
        let mut outline = outline.to_vec();
        // Counter-clockwise, seen from +Z.
        let area: f32 = (0..outline.len()).map(|i| outline[i].perp_dot(outline[(i + 1) % outline.len()])).sum();
        if area < 0.0 {
            outline.reverse();
        }
        let at = |p: Vec2, z: f32| Vec3::new(p.x, p.y, z);
        let front: Vec<Vec3> = outline.iter().map(|&p| at(p, far)).collect();
        let back: Vec<Vec3> = outline.iter().rev().map(|&p| at(p, near)).collect();
        let sides: Vec<[Vec3; 4]> = (0..outline.len())
            .map(|i| {
                let (a, b) = (outline[i], outline[(i + 1) % outline.len()]);
                [at(a, near), at(b, near), at(b, far), at(a, far)]
            })
            .collect();
        let mut faces: Vec<&[Vec3]> = vec![&front, &back];
        faces.extend(sides.iter().map(|side| side.as_slice()));
        Self::faces(&faces, false)
    }

    /// A prism like [`Shape::extrude`], its outline's corners rounded with
    /// `radius` in `steps` facets and its sides shaded smooth across them,
    /// as moulded plastic is; the two caps stay flat.
    pub fn extrude_rounded(outline: &[Vec2], near: f32, far: f32, radius: f32, steps: usize) -> Self {
        let mut outline = outline.to_vec();
        let n = outline.len();
        let area: f32 = (0..n).map(|i| outline[i].perp_dot(outline[(i + 1) % n])).sum();
        if area < 0.0 {
            outline.reverse();
        }
        // Each corner becomes an arc between the points `radius` along its two edges.
        let mut round = Vec::new();
        for i in 0..n {
            let (before, corner, after) = (outline[(i + n - 1) % n], outline[i], outline[(i + 1) % n]);
            let (into, out) = ((corner - before).normalize_or_zero(), (after - corner).normalize_or_zero());
            let reach = radius.min(corner.distance(before) / 2.0).min(corner.distance(after) / 2.0);
            let (start, end) = (corner - into * reach, corner + out * reach);
            for k in 0..=steps.max(1) {
                let t = k as f32 / steps.max(1) as f32;
                // A quadratic curve with the corner as its control point.
                round.push(start.lerp(corner, t).lerp(corner.lerp(end, t), t));
            }
        }
        let m = round.len();
        // Outward normal at each point: across the mean of its two edges.
        let normal = |j: usize| {
            let (a, b, c) = (round[(j + m - 1) % m], round[j], round[(j + 1) % m]);
            let along = ((b - a).normalize_or_zero() + (c - b).normalize_or_zero()).normalize_or(Vec2::X);
            Vec2::new(along.y, -along.x)
        };
        let (mut positions, mut normals, mut indices) = (Vec::new(), Vec::new(), Vec::new());
        for (j, point) in round.iter().enumerate() {
            let nrm = normal(j).extend(0.0);
            positions.extend([point.extend(near), point.extend(far)]);
            normals.extend([nrm, nrm]);
        }
        for j in 0..m as u32 {
            let (a, b) = (2 * j, 2 * ((j + 1) % m as u32));
            // Near and far of this point, then of the next: counter-clockwise from outside.
            indices.extend([a, b, a + 1, b, b + 1, a + 1]);
        }
        // The caps, flat, as fans.
        for (z, facing) in [(far, 1.0f32), (near, -1.0)] {
            let first = positions.len() as u32;
            for p in &round {
                positions.push(p.extend(z));
                normals.push(Vec3::Z * facing);
            }
            for j in 1..m as u32 - 1 {
                if facing > 0.0 {
                    indices.extend([first, first + j, first + j + 1]);
                } else {
                    indices.extend([first, first + j + 1, first + j]);
                }
            }
        }
        Self::from_parts(positions, normals, indices)
    }

    /// A box between two corners.
    pub fn block(low: Vec3, high: Vec3) -> Self {
        let size = high - low;
        let mesh = Cuboid::new(size.x, size.y, size.z).mesh().build().translated_by((low + high) / 2.0);
        Self::new(mesh)
    }

    #[cfg(test)]
    fn triangles(&self) -> impl Iterator<Item = [usize; 3]> + '_ {
        self.indices.as_chunks::<3>().0.iter().map(|t| t.map(|i| i as usize))
    }
}

/// The matrix of a placement, and the one that carries normals through it.
fn matrices(place: Transform) -> (bevy::math::Affine3A, Mat3) {
    let matrix = place.compute_affine();
    (matrix, Mat3::from(matrix.matrix3).inverse().transpose())
}

/// How a shape is coloured: one colour at its foot, one at its top, blended
/// between. A darker foot is the cheapest shading there is.
#[derive(Clone, Copy)]
pub struct Paint {
    foot: [f32; 4],
    top: [f32; 4],
}

impl Paint {
    /// One colour all over, in sRGB.
    pub fn plain(r: f32, g: f32, b: f32) -> Self {
        Self::blend((r, g, b), (r, g, b))
    }

    /// From one sRGB colour at the foot to another at the top.
    pub fn blend(foot: (f32, f32, f32), top: (f32, f32, f32)) -> Self {
        let linear = |(r, g, b): (f32, f32, f32)| Color::srgb(r, g, b).to_linear().to_f32_array();
        Self { foot: linear(foot), top: linear(top) }
    }

    /// The colour, darker by `depth` (0 to 1) at the foot.
    pub fn shaded(r: f32, g: f32, b: f32, depth: f32) -> Self {
        let keep = 1.0 - depth;
        Self::blend((r * keep, g * keep, b * keep), (r, g, b))
    }

    fn at(&self, height: f32) -> [f32; 4] {
        std::array::from_fn(|i| self.foot[i] + (self.top[i] - self.foot[i]) * height)
    }
}

/// The triangles of one patch.
#[derive(Default)]
struct Patch {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colours: Vec<[f32; 4]>,
    /// Texture coordinates in metres, projected from the side each face
    /// faces most: a grain laid on the batch keeps its size on every object.
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

/// Where a point lies on a texture projected onto the three planes of its
/// box, chosen by the direction it faces most: (x, z) from above or below,
/// (z, y) from the sides, (x, y) from the front or back. In metres.
pub fn box_uv(position: Vec3, normal: Vec3) -> [f32; 2] {
    let a = normal.abs();
    if a.y >= a.x && a.y >= a.z {
        [position.x, position.z]
    } else if a.x >= a.z {
        [position.z, -position.y]
    } else {
        [position.x, -position.y]
    }
}

impl Patch {
    fn add(&mut self, shape: &Shape, place: Transform, paint: Paint) {
        let (matrix, for_normals) = matrices(place);
        let first = self.positions.len() as u32;
        let tall = (shape.top - shape.foot).max(1e-6);
        for (position, normal) in shape.positions.iter().zip(&shape.normals) {
            let (placed, facing) = (matrix.transform_point3(*position), (for_normals * *normal).normalize_or(Vec3::Y));
            self.positions.push(placed.to_array());
            self.normals.push(facing.to_array());
            self.colours.push(paint.at((position.y - shape.foot) / tall));
            self.uvs.push(box_uv(placed, facing));
        }
        // A mirrored placement turns every triangle over: turn it back.
        let mirrored = matrix.matrix3.determinant() < 0.0;
        for triangle in shape.indices.as_chunks::<3>().0 {
            let [a, b, c] = triangle.map(|i| i + first);
            self.indices.extend(if mirrored { [a, c, b] } else { [a, b, c] });
        }
    }

    fn mesh(self) -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colours)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
            .with_inserted_indices(Indices::U32(self.indices))
    }
}

/// Placed shapes that share a material, gathered by patch of land.
#[derive(Default)]
pub struct Batch {
    patches: BTreeMap<(i32, i32), Patch>,
}

impl Batch {
    /// Adds `shape` at `place`; it joins the patch its origin stands on.
    pub fn add(&mut self, shape: &Shape, place: Transform, paint: Paint) {
        self.add_within(place.translation, shape, place, paint);
    }

    /// Adds `shape` at `place`, to the patch of `anchor`: the pieces of one
    /// object share its anchor, and are drawn or culled together.
    pub fn add_within(&mut self, anchor: Vec3, shape: &Shape, place: Transform, paint: Paint) {
        let patch = ((anchor.x / PATCH).floor() as i32, (anchor.z / PATCH).floor() as i32);
        self.patches.entry(patch).or_default().add(shape, place, paint);
    }

    #[cfg(test)]
    pub fn triangles(&self) -> usize {
        self.patches.values().map(|patch| patch.indices.len() / 3).sum()
    }

    /// Adds everything of another batch.
    pub fn append(&mut self, other: Batch) {
        for (key, patch) in other.patches {
            let mine = self.patches.entry(key).or_default();
            let first = mine.positions.len() as u32;
            mine.positions.extend(patch.positions);
            mine.normals.extend(patch.normals);
            mine.colours.extend(patch.colours);
            mine.uvs.extend(patch.uvs);
            mine.indices.extend(patch.indices.into_iter().map(|index| index + first));
        }
    }

    /// Everything as one mesh, whatever the patches: for an object that moves.
    pub fn merged(self) -> Mesh {
        let mut whole = Patch::default();
        for patch in self.patches.into_values() {
            let first = whole.positions.len() as u32;
            whole.positions.extend(patch.positions);
            whole.normals.extend(patch.normals);
            whole.colours.extend(patch.colours);
            whole.uvs.extend(patch.uvs);
            whole.indices.extend(patch.indices.into_iter().map(|index| index + first));
        }
        whole.mesh()
    }

    /// One mesh per patch that has something.
    pub fn meshes(self) -> impl Iterator<Item = Mesh> {
        self.patches.into_values().map(Patch::mesh)
    }
}

/// A small repeatable generator: what it scatters is the same on
/// every machine and at every run.
#[derive(Clone, Debug)]
pub struct Dice(pub u64);

impl Dice {
    pub fn roll(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 40) & 0xff_ffff) as f32 / 16_777_216.0
    }

    pub fn between(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.roll()
    }

    pub fn pick<T: Clone>(&mut self, choices: &[T]) -> T {
        choices[(self.roll() * choices.len() as f32) as usize % choices.len()].clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rounded_extrusion_faces_outwards_and_keeps_its_size() {
        let outline = [Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(1.0, 0.6), Vec2::new(0.0, 0.6)];
        let shape = Shape::extrude_rounded(&outline, -0.2, 0.2, 0.1, 3);
        let centre = Vec3::new(0.5, 0.3, 0.0);
        for t in shape.indices.as_chunks::<3>().0 {
            let [a, b, c] = t.map(|i| shape.positions[i as usize]);
            let normal = (b - a).cross(c - a);
            assert!(normal.dot((a + b + c) / 3.0 - centre) > 0.0, "a face looks inwards");
        }
        let (low, high) = shape.positions.iter().fold((Vec3::MAX, Vec3::MIN), |(l, h), p| (l.min(*p), h.max(*p)));
        assert!(low.x >= -1e-5 && high.x <= 1.0 + 1e-5 && (high.y - 0.6).abs() < 1e-5, "within the outline");
        // The corner itself is cut off.
        assert!(shape.positions.iter().all(|p| p.xy().distance(Vec2::new(1.0, 0.6)) > 0.02));
    }

    #[test]
    fn an_extruded_outline_faces_outwards_whichever_way_it_turns() {
        let outline =
            [Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(1.2, 0.5), Vec2::new(0.5, 0.9), Vec2::new(-0.1, 0.4)];
        let reversed: Vec<Vec2> = outline.iter().rev().copied().collect();
        for outline in [outline.to_vec(), reversed] {
            let shape = Shape::extrude(&outline, -0.3, 0.3);
            let centre = shape.positions.iter().sum::<Vec3>() / shape.positions.len() as f32;
            for t in shape.indices.as_chunks::<3>().0 {
                let [a, b, c] = t.map(|i| shape.positions[i as usize]);
                let normal = (b - a).cross(c - a);
                assert!(normal.dot((a + b + c) / 3.0 - centre) > 0.0, "a face looks inwards");
                assert!(normal.dot(shape.normals[t[0] as usize]) > 0.0, "a normal disagrees with its winding");
            }
            assert_eq!(shape.indices.len() / 3, 2 * 3 + 5 * 2, "two caps of three triangles, five sides of two");
        }
    }

    /// Every triangle of a closed shape around the origin's axis must face
    /// away from it, and agree with its vertices' normals.
    fn faces_outwards(shape: &Shape, centre: impl Fn(Vec3) -> Vec3) {
        for [a, b, c] in shape.triangles() {
            let (pa, pb, pc) = (shape.positions[a], shape.positions[b], shape.positions[c]);
            let face = (pb - pa).cross(pc - pa);
            if face.length() < 1e-7 {
                continue; // a triangle closed to a point, at a pole
            }
            let middle = (pa + pb + pc) / 3.0;
            assert!(face.dot(middle - centre(middle)) > 0.0, "a triangle faces inwards");
            let normal = shape.normals[a] + shape.normals[b] + shape.normals[c];
            assert!(face.dot(normal) > 0.0, "a triangle disagrees with its normals");
        }
    }

    #[test]
    fn a_lathe_faces_outwards() {
        let vase = Shape::lathe(&[(0.0, 0.0), (1.0, 0.2), (1.4, 1.0), (0.6, 2.0), (0.0, 2.2)], 9);
        assert_eq!(vase.positions.len(), 5 * 10);
        assert_eq!((vase.foot, vase.top), (0.0, 2.2));
        faces_outwards(&vase, |_| Vec3::new(0.0, 1.1, 0.0));
        assert!(vase.normals.iter().all(|n| (n.length() - 1.0).abs() < 1e-4));
        // A slice of it has the same profile over a part of the turn.
        let slice = Shape::gore(&[(0.0, 0.0), (1.0, 0.2), (1.4, 1.0), (0.6, 2.0), (0.0, 2.2)], 2, 1.0, 0.5);
        assert_eq!(slice.positions.len(), 5 * 3);
        faces_outwards(&slice, |_| Vec3::new(0.0, 1.1, 0.0));
    }

    #[test]
    fn a_sweep_faces_outwards() {
        let path: Vec<Vec3> = (0..=8).map(|k| Vec3::new(k as f32, (k as f32 * 0.4).sin(), 0.0)).collect();
        let tube = Shape::sweep(&path, 0.3, 7, Vec3::Z);
        // The nearest point of the path's line at the same x is inside the tube.
        faces_outwards(&tube, |p| Vec3::new(p.x, (p.x * 0.4).sin(), 0.0));
    }

    #[test]
    fn primitives_and_faces_face_outwards() {
        faces_outwards(&Shape::new(Sphere::new(1.0).mesh().ico(1).unwrap()), |_| Vec3::ZERO);
        faces_outwards(&Shape::block(Vec3::new(1.0, 1.0, 1.0), Vec3::new(2.0, 3.0, 4.0)), |_| Vec3::new(1.5, 2.0, 2.5));
        let square = [Vec3::ZERO, Vec3::X, Vec3::new(1.0, 0.0, -1.0), Vec3::NEG_Z];
        let floor = Shape::faces(&[&square], false);
        assert_eq!(floor.indices.len(), 6);
        assert!(floor.normals.iter().all(|n| n.distance(Vec3::Y) < 1e-5), "counter-clockwise from above faces up");
        assert_eq!(Shape::faces(&[&square], true).indices.len(), 12);
    }

    #[test]
    fn a_mirrored_placement_still_faces_outwards() {
        let ball = Shape::new(Sphere::new(1.0).mesh().ico(1).unwrap());
        let mirrored = Transform::from_xyz(5.0, 0.0, 0.0).with_scale(Vec3::new(-1.0, 2.0, 1.0));
        let mut patch = Patch::default();
        patch.add(&ball, mirrored, Paint::plain(1.0, 1.0, 1.0));
        let placed = Shape::from_parts(
            patch.positions.iter().map(|p| Vec3::from(*p)).collect(),
            patch.normals.iter().map(|n| Vec3::from(*n)).collect(),
            patch.indices.clone(),
        );
        faces_outwards(&placed, |_| Vec3::new(5.0, 0.0, 0.0));
    }

    #[test]
    fn texture_coordinates_are_metres_on_the_side_a_face_faces() {
        let mut batch = Batch::default();
        let place = Transform::from_xyz(10.0, 0.0, 20.0);
        batch.add(&Shape::block(Vec3::ZERO, Vec3::new(4.0, 2.0, 1.0)), place, Paint::plain(1.0, 1.0, 1.0));
        let mesh = batch.merged();
        let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
            panic!("positions")
        };
        let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL) else {
            panic!("normals")
        };
        let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else { panic!("uvs") };
        for ((p, n), uv) in positions.iter().zip(normals).zip(uvs) {
            let (p, n) = (Vec3::from(*p), Vec3::from(*n));
            let expected = if n.y.abs() > 0.9 {
                [p.x, p.z]
            } else if n.x.abs() > 0.9 {
                [p.z, -p.y]
            } else {
                [p.x, -p.y]
            };
            assert_eq!(*uv, expected, "at {p} facing {n}");
        }
        // The top face spans its 4 by 1 metres in texture coordinates, wherever it stands.
        let top: Vec<[f32; 2]> = normals.iter().zip(uvs).filter(|(n, _)| n[1] > 0.9).map(|(_, uv)| *uv).collect();
        let span = |k: usize| {
            top.iter().map(|uv| uv[k]).fold(f32::MIN, f32::max) - top.iter().map(|uv| uv[k]).fold(f32::MAX, f32::min)
        };
        assert!((span(0) - 4.0).abs() < 1e-4 && (span(1) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn shapes_are_gathered_by_patch_and_painted_by_height() {
        let post = Shape::block(Vec3::ZERO, Vec3::new(1.0, 4.0, 1.0));
        let mut batch = Batch::default();
        batch.add(&post, Transform::from_xyz(10.0, 0.0, 10.0), Paint::shaded(1.0, 1.0, 1.0, 0.5));
        batch.add(&post, Transform::from_xyz(20.0, 0.0, 10.0), Paint::plain(1.0, 0.0, 0.0));
        batch.add(&post, Transform::from_xyz(-10.0, 0.0, 10.0), Paint::plain(0.0, 1.0, 0.0));
        assert_eq!(batch.triangles(), 3 * 12);
        assert_eq!(batch.patches.len(), 2, "two posts share a patch, the third stands on its neighbour");
        let mut again = Batch::default();
        again.add(&post, Transform::from_xyz(10.0, 0.0, 10.0), Paint::plain(1.0, 1.0, 1.0));
        again.add(&post, Transform::from_xyz(-10.0, 0.0, 10.0), Paint::plain(1.0, 1.0, 1.0));
        assert_eq!(again.merged().indices().map(|indices| indices.len()), Some(2 * 36), "merged into one mesh");
        let shared = &batch.patches[&(0, 0)];
        assert_eq!(shared.positions.len(), 2 * 24);
        let (foot, top) = shared.positions[..24].iter().zip(&shared.colours[..24]).fold(
            (f32::MAX, f32::MIN),
            |(foot, top), (p, c)| if p[1] < 2.0 { (foot.min(c[0]), top) } else { (foot, top.max(c[0])) },
        );
        assert!(foot < 0.3 && top > 0.99, "darker at the foot than at the top: {foot} {top}");
    }
}
