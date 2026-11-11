// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Organic solids: round cones, ellipsoids and boxes blended into one
//! smooth surface by a signed distance field, meshed by surface nets. A
//! figure built of capsules and spheres shows a seam and a crease at every
//! joint and reads as a mannequin; blended, a shoulder flows into an arm
//! and a thigh into a hip, as under cloth.
//!
//! Each piece belongs to a group (a colour, a material): a point of the
//! surface takes the group of the solid piece nearest to it, unless a paint
//! volume — a piece that adds nothing to the solid — contains it, as a
//! panel is sewn onto a suit. Folds can ripple a piece's surface, a few
//! millimetres deep, as cloth gathers at a joint.

use bevy::prelude::*;

use crate::Shape;

/// The form of a piece.
#[derive(Clone, Copy, Debug)]
enum Form {
    /// From `a`, of radius `ra`, to `b`, of radius `rb`, rounded at both ends.
    Cone { a: Vec3, b: Vec3, ra: f32, rb: f32 },
    /// Centred at `centre`, turned by `turn`, with these radii.
    Ellipsoid { centre: Vec3, radii: Vec3, turn: Quat },
    /// Centred at `centre`, turned by `turn`, with these half sides, its
    /// edges rounded by `radius`.
    Cuboid { centre: Vec3, half: Vec3, turn: Quat, radius: f32 },
}

#[derive(Clone, Copy, Debug)]
struct Piece {
    form: Form,
    group: usize,
    /// Only paints the surface: adds nothing to the solid.
    paint: bool,
    /// Depth of the folds rippling its surface, metres.
    folds: f32,
    /// The bone that moves it, for a skinned figure ([`Sculpture::skinned`]).
    bone: u16,
}

/// The triangles of one group as they are gathered: positions, normals,
/// indices, and where each vertex of the whole surface went.
type Gathered = (Vec<Vec3>, Vec<Vec3>, Vec<u32>, Vec<u32>);

/// A solid made of pieces blended smoothly, and the paint volumes on it.
#[derive(Clone, Debug)]
pub struct Sculpture {
    pieces: Vec<Piece>,
    /// How far, in metres, pieces blend into each other.
    blend: f32,
}

/// Distance to a round cone (Quilez).
fn round_cone(p: Vec3, a: Vec3, b: Vec3, ra: f32, rb: f32) -> f32 {
    let ba = b - a;
    let l2 = ba.length_squared();
    if l2 < 1e-12 {
        return p.distance(a) - ra.max(rb);
    }
    let rr = ra - rb;
    let a2 = l2 - rr * rr;
    let il2 = 1.0 / l2;
    let pa = p - a;
    let y = pa.dot(ba);
    let z = y - l2;
    let x2 = (pa * l2 - ba * y).length_squared();
    let y2 = y * y * l2;
    let z2 = z * z * l2;
    let k = rr.signum() * rr * rr * x2;
    if z.signum() * a2 * z2 > k {
        return (x2 + z2).sqrt() * il2 - rb;
    }
    if y.signum() * a2 * y2 < k {
        return (x2 + y2).sqrt() * il2 - ra;
    }
    ((x2 * a2 * il2).sqrt() + y * rr) * il2 - ra
}

/// Distance to an ellipsoid, approximate but exact on its surface (Quilez).
fn ellipsoid(p: Vec3, radii: Vec3) -> f32 {
    let k0 = (p / radii).length();
    let k1 = (p / (radii * radii)).length();
    if k1 < 1e-9 {
        return -radii.min_element();
    }
    k0 * (k0 - 1.0) / k1
}

fn cuboid(p: Vec3, half: Vec3) -> f32 {
    let q = p.abs() - half;
    q.max(Vec3::ZERO).length() + q.max_element().min(0.0)
}

/// The smooth minimum of two distances, blending over `k` metres.
fn smooth_min(a: f32, b: f32, k: f32) -> f32 {
    if k <= 0.0 {
        return a.min(b);
    }
    let h = (k - (a - b).abs()).max(0.0) / k;
    a.min(b) - h * h * k * 0.25
}

impl Piece {
    fn distance(&self, p: Vec3) -> f32 {
        let d = match self.form {
            Form::Cone { a, b, ra, rb } => round_cone(p, a, b, ra, rb),
            Form::Ellipsoid { centre, radii, turn } => ellipsoid(turn.inverse() * (p - centre), radii),
            Form::Cuboid { centre, half, turn, radius } => {
                let r = radius.min(half.min_element());
                cuboid(turn.inverse() * (p - centre), half - Vec3::splat(r)) - r
            }
        };
        if self.folds <= 0.0 {
            return d;
        }
        // Folds: ridges around the piece's long axis, wavering so that no
        // two are alike, as cloth gathers.
        let (axis, origin) = match self.form {
            Form::Cone { a, b, .. } => ((b - a).normalize_or(Vec3::Y), a),
            Form::Ellipsoid { centre, turn, .. } | Form::Cuboid { centre, turn, .. } => (turn * Vec3::Y, centre),
        };
        let along = (p - origin).dot(axis);
        let side = (p - origin) - axis * along;
        let around = side.x.atan2(side.z) + side.y;
        d - self.folds * (along * 105.0 + 1.7 * (around * 2.0 + along * 31.0).sin()).sin()
    }

    fn reach(&self) -> (Vec3, Vec3) {
        let margin = Vec3::splat(self.folds);
        match self.form {
            Form::Cone { a, b, ra, rb } => {
                let r = Vec3::splat(ra.max(rb));
                (a.min(b) - r - margin, a.max(b) + r + margin)
            }
            Form::Ellipsoid { centre, radii, .. } => {
                let r = Vec3::splat(radii.max_element());
                (centre - r - margin, centre + r + margin)
            }
            Form::Cuboid { centre, half, .. } => {
                let r = Vec3::splat(half.length());
                (centre - r - margin, centre + r + margin)
            }
        }
    }
}

impl Sculpture {
    /// An empty sculpture whose pieces blend over `blend` metres.
    pub fn new(blend: f32) -> Self {
        Self { pieces: Vec::new(), blend }
    }

    fn push(&mut self, form: Form, group: usize, paint: bool) -> &mut Self {
        self.pieces.push(Piece { form, group, paint, folds: 0.0, bone: 0 });
        self
    }

    /// A round cone from `a` (radius `ra`) to `b` (radius `rb`).
    pub fn cone(&mut self, group: usize, a: Vec3, b: Vec3, ra: f32, rb: f32) -> &mut Self {
        self.push(Form::Cone { a, b, ra, rb }, group, false)
    }

    /// An ellipsoid with `radii` along its own axes, turned by `turn`.
    pub fn ellipsoid(&mut self, group: usize, centre: Vec3, radii: Vec3, turn: Quat) -> &mut Self {
        self.push(Form::Ellipsoid { centre, radii, turn }, group, false)
    }

    /// A box with sharp edges (the blend rounds where it meets others).
    pub fn cuboid(&mut self, group: usize, centre: Vec3, half: Vec3, turn: Quat) -> &mut Self {
        self.push(Form::Cuboid { centre, half, turn, radius: 0.0 }, group, false)
    }

    /// A box with its edges rounded by `radius`, as moulded plastic is.
    pub fn rounded(&mut self, group: usize, centre: Vec3, half: Vec3, radius: f32, turn: Quat) -> &mut Self {
        self.push(Form::Cuboid { centre, half, turn, radius }, group, false)
    }

    /// Paints `group` on the surface inside a round cone; adds no matter.
    pub fn paint_cone(&mut self, group: usize, a: Vec3, b: Vec3, ra: f32, rb: f32) -> &mut Self {
        self.push(Form::Cone { a, b, ra, rb }, group, true)
    }

    /// Paints `group` on the surface inside a turned box; adds no matter.
    pub fn paint_box(&mut self, group: usize, centre: Vec3, half: Vec3, turn: Quat) -> &mut Self {
        self.push(Form::Cuboid { centre, half, turn, radius: 0.0 }, group, true)
    }

    /// Folds `depth` metres deep on the last piece added.
    pub fn folded(&mut self, depth: f32) -> &mut Self {
        if let Some(last) = self.pieces.last_mut() {
            last.folds = depth;
        }
        self
    }

    /// The last piece added moves with `bone` (0 unless said).
    pub fn bone(&mut self, bone: u16) -> &mut Self {
        if let Some(last) = self.pieces.last_mut() {
            last.bone = bone;
        }
        self
    }

    /// The two bones nearest a point of the surface and how much each
    /// moves it: the nearest solid piece's bone, and, within the blend of
    /// it, the next bone's, weighted so that the surface bends smoothly at
    /// a joint.
    pub fn weights(&self, p: Vec3) -> ([u16; 4], [f32; 4]) {
        let mut nearest: Vec<(u16, f32)> = Vec::new();
        for piece in self.pieces.iter().filter(|piece| !piece.paint) {
            let d = piece.distance(p).max(0.0);
            match nearest.iter_mut().find(|(bone, _)| *bone == piece.bone) {
                Some(entry) => entry.1 = entry.1.min(d),
                None => nearest.push((piece.bone, d)),
            }
        }
        nearest.sort_by(|a, b| a.1.total_cmp(&b.1));
        match nearest.as_slice() {
            [] => ([0; 4], [1.0, 0.0, 0.0, 0.0]),
            [only] => ([only.0, 0, 0, 0], [1.0, 0.0, 0.0, 0.0]),
            [first, second, ..] => {
                let span = self.blend.max(0.01) * 2.0;
                let other = (0.5 - (second.1 - first.1) / span).clamp(0.0, 0.5);
                ([first.0, second.0, 0, 0], [1.0 - other, other, 0.0, 0.0])
            }
        }
    }

    /// The surface as skinned meshes, one per group: positions, normals,
    /// the colour `paint` gives each group, and for each vertex the bones
    /// that move it and their weights ([`Sculpture::weights`]), for a
    /// renderer's skinning.
    pub fn skinned(&self, cell: f32, paint: impl Fn(usize) -> [f32; 4]) -> Vec<(usize, Mesh)> {
        use bevy::{
            asset::RenderAssetUsages,
            mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
        };
        self.shapes(cell)
            .into_iter()
            .map(|(group, shape)| {
                let (joints, weights): (Vec<[u16; 4]>, Vec<[f32; 4]>) =
                    shape.positions.iter().map(|p| self.weights(*p)).unzip();
                let colour = paint(group);
                let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
                    .with_inserted_attribute(
                        Mesh::ATTRIBUTE_POSITION,
                        shape.positions.iter().map(|p| p.to_array()).collect::<Vec<_>>(),
                    )
                    .with_inserted_attribute(
                        Mesh::ATTRIBUTE_NORMAL,
                        shape.normals.iter().map(|n| n.to_array()).collect::<Vec<_>>(),
                    )
                    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![colour; shape.positions.len()])
                    .with_inserted_attribute(Mesh::ATTRIBUTE_JOINT_INDEX, VertexAttributeValues::Uint16x4(joints))
                    .with_inserted_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, weights)
                    .with_inserted_indices(Indices::U32(shape.indices.clone()));
                (group, mesh)
            })
            .collect()
    }

    /// Signed distance to the surface: negative inside.
    pub fn distance(&self, p: Vec3) -> f32 {
        self.pieces.iter().filter(|piece| !piece.paint).fold(f32::MAX, |d, piece| {
            if d == f32::MAX { piece.distance(p) } else { smooth_min(d, piece.distance(p), self.blend) }
        })
    }

    /// The group of the surface at `p`: the last paint volume holding it,
    /// or the solid piece nearest to it.
    pub fn group_at(&self, p: Vec3) -> usize {
        if let Some(paint) = self.pieces.iter().rev().find(|piece| piece.paint && piece.distance(p) <= 0.0) {
            return paint.group;
        }
        self.pieces
            .iter()
            .filter(|piece| !piece.paint)
            .map(|piece| (piece.distance(p), piece.group))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map_or(0, |(_, group)| group)
    }

    /// The outward direction of the surface at `p`.
    fn normal(&self, p: Vec3, step: f32) -> Vec3 {
        let d = |offset: Vec3| self.distance(p + offset) - self.distance(p - offset);
        Vec3::new(d(Vec3::X * step), d(Vec3::Y * step), d(Vec3::Z * step)).normalize_or(Vec3::Y)
    }

    /// The surface, meshed by surface nets in cells of `cell` metres, as one
    /// shape per group, in the order of the groups.
    pub fn shapes(&self, cell: f32) -> Vec<(usize, Shape)> {
        let solid: Vec<&Piece> = self.pieces.iter().filter(|piece| !piece.paint).collect();
        if solid.is_empty() || cell <= 0.0 {
            return Vec::new();
        }
        let (low, high) = solid.iter().fold((Vec3::MAX, Vec3::MIN), |(low, high), piece| {
            let (l, h) = piece.reach();
            (low.min(l), high.max(h))
        });
        let low = low - Vec3::splat(self.blend + 2.0 * cell);
        let high = high + Vec3::splat(self.blend + 2.0 * cell);
        let n = ((high - low) / cell).ceil().as_uvec3() + UVec3::ONE;
        let (nx, ny, nz) = (n.x as usize, n.y as usize, n.z as usize);
        let point = |i: usize, j: usize, k: usize| low + Vec3::new(i as f32, j as f32, k as f32) * cell;
        let at = |i: usize, j: usize, k: usize| (k * ny + j) * nx + i;

        // The field at every point of the grid.
        let mut field = vec![0.0f32; nx * ny * nz];
        for k in 0..nz {
            for j in 0..ny {
                for i in 0..nx {
                    field[at(i, j, k)] = self.distance(point(i, j, k));
                }
            }
        }

        // A vertex in each cell the surface crosses: the mean of where it
        // crosses the cell's edges.
        const CORNERS: [[usize; 3]; 8] =
            [[0, 0, 0], [1, 0, 0], [0, 1, 0], [1, 1, 0], [0, 0, 1], [1, 0, 1], [0, 1, 1], [1, 1, 1]];
        const EDGES: [[usize; 2]; 12] =
            [[0, 1], [2, 3], [4, 5], [6, 7], [0, 2], [1, 3], [4, 6], [5, 7], [0, 4], [1, 5], [2, 6], [3, 7]];
        let cells = |i: usize, j: usize, k: usize| (k * (ny - 1) + j) * (nx - 1) + i;
        let mut vertex = vec![u32::MAX; (nx - 1) * (ny - 1) * (nz - 1)];
        let mut positions = Vec::new();
        for k in 0..nz - 1 {
            for j in 0..ny - 1 {
                for i in 0..nx - 1 {
                    let values = CORNERS.map(|[a, b, c]| field[at(i + a, j + b, k + c)]);
                    let inside = values.iter().filter(|v| **v < 0.0).count();
                    if inside == 0 || inside == 8 {
                        continue;
                    }
                    let (mut sum, mut count) = (Vec3::ZERO, 0.0);
                    for [e0, e1] in EDGES {
                        let (v0, v1) = (values[e0], values[e1]);
                        if (v0 < 0.0) != (v1 < 0.0) {
                            let t = v0 / (v0 - v1);
                            let [a0, b0, c0] = CORNERS[e0];
                            let [a1, b1, c1] = CORNERS[e1];
                            let p0 = Vec3::new(a0 as f32, b0 as f32, c0 as f32);
                            let p1 = Vec3::new(a1 as f32, b1 as f32, c1 as f32);
                            sum += p0.lerp(p1, t);
                            count += 1.0;
                        }
                    }
                    vertex[cells(i, j, k)] = positions.len() as u32;
                    positions.push(point(i, j, k) + sum / count * cell);
                }
            }
        }

        // A quad across each grid edge the surface crosses, joining the
        // vertices of the four cells around it, facing out of the solid.
        let mut quads: Vec<[u32; 4]> = Vec::new();
        for k in 0..nz {
            for j in 0..ny {
                for i in 0..nx {
                    let here = field[at(i, j, k)];
                    let base = [i, j, k];
                    for axis in 0..3 {
                        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
                        let mut next = base;
                        next[axis] += 1;
                        if next[axis] >= [nx, ny, nz][axis] || base[u] == 0 || base[v] == 0 {
                            continue;
                        }
                        let there = field[at(next[0], next[1], next[2])];
                        if (here < 0.0) == (there < 0.0) {
                            continue;
                        }
                        let cell_of = |du: usize, dv: usize| {
                            let mut c = base;
                            c[u] -= du;
                            c[v] -= dv;
                            if c[0] >= nx - 1 || c[1] >= ny - 1 || c[2] >= nz - 1 {
                                return u32::MAX;
                            }
                            vertex[cells(c[0], c[1], c[2])]
                        };
                        let ring = [cell_of(1, 1), cell_of(0, 1), cell_of(0, 0), cell_of(1, 0)];
                        if ring.contains(&u32::MAX) {
                            continue;
                        }
                        // Counter-clockwise seen from +axis; outward is
                        // +axis where the solid is at this end.
                        quads.push(if here < 0.0 { ring } else { [ring[0], ring[3], ring[2], ring[1]] });
                    }
                }
            }
        }

        // Normals from the field; each triangle to its group's shape.
        let normals: Vec<Vec3> = positions.iter().map(|p| self.normal(*p, cell * 0.5)).collect();
        let mut groups: std::collections::BTreeMap<usize, Gathered> = std::collections::BTreeMap::new();
        for [a, b, c, d] in quads {
            // Split along the shorter diagonal.
            let (pa, pb, pc, pd) =
                (positions[a as usize], positions[b as usize], positions[c as usize], positions[d as usize]);
            let triangles = if pa.distance_squared(pc) <= pb.distance_squared(pd) {
                [[a, b, c], [a, c, d]]
            } else {
                [[a, b, d], [b, c, d]]
            };
            for triangle in triangles {
                let centre = triangle.iter().map(|&i| positions[i as usize]).sum::<Vec3>() / 3.0;
                let (ps, ns, indices, remap) = groups.entry(self.group_at(centre)).or_default();
                if remap.is_empty() {
                    remap.resize(positions.len(), u32::MAX);
                }
                for i in triangle {
                    if remap[i as usize] == u32::MAX {
                        remap[i as usize] = ps.len() as u32;
                        ps.push(positions[i as usize]);
                        ns.push(normals[i as usize]);
                    }
                    indices.push(remap[i as usize]);
                }
            }
        }
        groups
            .into_iter()
            .map(|(group, (positions, normals, indices, _))| (group, Shape::from_parts(positions, normals, indices)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(shapes: &[(usize, Shape)]) -> (Vec<Vec3>, Vec<Vec3>, usize) {
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut triangles = 0;
        for (_, shape) in shapes {
            positions.extend(&shape.positions);
            normals.extend(&shape.normals);
            triangles += shape.indices.len() / 3;
        }
        (positions, normals, triangles)
    }

    #[test]
    fn a_ball_is_meshed_on_its_surface_facing_out() {
        let mut ball = Sculpture::new(0.0);
        ball.cone(0, Vec3::ZERO, Vec3::ZERO, 0.2, 0.2);
        let shapes = ball.shapes(0.02);
        let (positions, normals, triangles) = all(&shapes);
        assert!(triangles > 200, "{triangles}");
        for (p, n) in positions.iter().zip(&normals) {
            assert!((p.length() - 0.2).abs() < 0.01, "on the surface: {}", p.length());
            assert!(n.dot(p.normalize()) > 0.95, "facing out");
        }
        // Every triangle's winding agrees with its normals: front faces out.
        let shape = &shapes[0].1;
        for t in shape.indices.as_chunks::<3>().0 {
            let [a, b, c] = t.map(|i| shape.positions[i as usize]);
            let face = (b - a).cross(c - a);
            assert!(face.dot((a + b + c) / 3.0) > 0.0, "counter-clockwise from outside");
        }
    }

    #[test]
    fn blended_pieces_join_without_a_crease() {
        // Two capsules meeting at a right angle: blended, the inner corner fills.
        let corner = Vec3::new(0.06, 0.06, 0.0);
        let mut sharp = Sculpture::new(0.0);
        sharp.cone(0, Vec3::ZERO, Vec3::X * 0.4, 0.05, 0.05).cone(0, Vec3::ZERO, Vec3::Y * 0.4, 0.05, 0.05);
        let mut smooth = sharp.clone();
        smooth.blend = 0.06;
        assert!(sharp.distance(corner) > 0.0, "outside both capsules");
        assert!(smooth.distance(corner) < sharp.distance(corner) - 0.005, "filled by the blend");
        // Far from the joint, the blend changes nothing.
        let far = Vec3::new(0.35, 0.05, 0.0);
        assert!((smooth.distance(far) - sharp.distance(far)).abs() < 1e-4);
    }

    #[test]
    fn groups_follow_pieces_and_paint() {
        let mut figure = Sculpture::new(0.03);
        figure.cone(0, Vec3::ZERO, Vec3::Y * 0.5, 0.1, 0.1).cone(
            1,
            Vec3::new(0.0, 0.5, 0.0),
            Vec3::X * 0.4,
            0.05,
            0.05,
        );
        figure.paint_box(2, Vec3::new(0.0, 0.25, 0.1), Vec3::new(0.2, 0.05, 0.05), Quat::IDENTITY);
        assert_eq!(figure.group_at(Vec3::new(-0.1, 0.1, 0.0)), 0);
        assert_eq!(figure.group_at(Vec3::new(0.3, 0.12, 0.0)), 1);
        assert_eq!(figure.group_at(Vec3::new(0.0, 0.25, 0.1)), 2, "painted");
        let shapes = figure.shapes(0.02);
        let groups: Vec<usize> = shapes.iter().map(|(g, _)| *g).collect();
        assert_eq!(groups, vec![0, 1, 2]);
        // Painting adds no matter.
        let mut bare = figure.clone();
        bare.pieces.pop();
        let p = Vec3::new(0.0, 0.25, 0.12);
        assert_eq!(bare.distance(p), figure.distance(p));
    }

    #[test]
    fn a_rounded_box_keeps_its_size_and_rounds_its_corners() {
        let mut moulded = Sculpture::new(0.0);
        moulded.rounded(0, Vec3::ZERO, Vec3::new(0.3, 0.1, 0.2), 0.05, Quat::IDENTITY);
        // On its faces it is the box.
        assert!(moulded.distance(Vec3::new(0.3, 0.0, 0.0)).abs() < 1e-5);
        assert!(moulded.distance(Vec3::new(0.0, 0.1, 0.0)).abs() < 1e-5);
        // At a corner it is rounded off: the box's corner is outside.
        assert!(moulded.distance(Vec3::new(0.3, 0.1, 0.2)) > 0.02);
    }

    #[test]
    fn a_joint_blends_two_bones_and_the_rest_follows_one() {
        let mut arm = Sculpture::new(0.04);
        arm.cone(0, Vec3::ZERO, Vec3::X * 0.3, 0.05, 0.05)
            .bone(1)
            .cone(0, Vec3::X * 0.3, Vec3::X * 0.6, 0.05, 0.04)
            .bone(2);
        let (bones, weights) = arm.weights(Vec3::new(0.05, 0.05, 0.0));
        assert_eq!((bones[0], weights[0]), (1, 1.0), "far from the elbow: the upper arm alone");
        let (bones, weights) = arm.weights(Vec3::new(0.3, 0.05, 0.0));
        assert!(
            (weights[0] - 0.5).abs() < 0.1 && bones[..2].contains(&1) && bones[..2].contains(&2),
            "{bones:?} {weights:?}"
        );
        let (bones, weights) = arm.weights(Vec3::new(0.55, 0.04, 0.0));
        assert_eq!((bones[0], weights[0]), (2, 1.0), "the forearm alone");
        let meshes = arm.skinned(0.02, |_| [1.0; 4]);
        assert_eq!(meshes.len(), 1);
        assert!(meshes[0].1.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT).is_some());
    }

    #[test]
    fn folds_ripple_the_surface_a_little() {
        let mut sleeve = Sculpture::new(0.0);
        sleeve.cone(0, Vec3::ZERO, Vec3::Y * 0.4, 0.05, 0.05).folded(0.004);
        let plain = round_cone(Vec3::new(0.05, 0.2, 0.0), Vec3::ZERO, Vec3::Y * 0.4, 0.05, 0.05);
        let samples: Vec<f32> =
            (0..40).map(|k| sleeve.distance(Vec3::new(0.05, 0.01 * k as f32, 0.0)) - plain).collect();
        assert!(samples.iter().all(|d| d.abs() <= 0.0041), "no deeper than the folds");
        assert!(samples.iter().any(|d| *d > 0.002) && samples.iter().any(|d| *d < -0.002), "ridges and hollows");
    }
}
