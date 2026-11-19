// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A mesh cut to a box: the rays of the traced light (T4) see the walker's
//! block alone, what reaches out of it cut at its surface, so that what lies
//! beyond comes in as the light its probes saw (D-137), not as Solari lights
//! it again. A building whose corner stood in the block was traced whole and
//! lit the floor from its own traced light instead of Cycles'.

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, MeshVertexAttribute, PrimitiveTopology, VertexAttributeValues},
    prelude::*,
};

/// What is left of a mesh cut to a box.
pub enum Cut {
    /// Its triangles inside the box, cut where they cross it.
    Part(Box<Mesh>),
    /// Nothing of it is inside.
    Nothing,
    /// It cannot be cut (not triangles, or an attribute not of floats): it
    /// stays whole.
    Whole,
}

/// The triangles of `mesh`, placed by `place`, inside the box `lo..hi` of
/// the world's frame; a triangle that crosses the box's surface is cut
/// there, its attributes interpolated along the cut (normals and tangents
/// made unit again).
pub fn clip(mesh: &Mesh, place: &Transform, lo: Vec3, hi: Vec3) -> Cut {
    if mesh.primitive_topology() != PrimitiveTopology::TriangleList {
        return Cut::Whole;
    }
    // The box's six planes in the mesh's own frame, n · x <= d inside: a
    // world plane m · y <= e, with y = A x + t, is (Aᵀ m) · x <= e - m · t.
    let affine = place.compute_affine();
    let (a, t) = (Mat3::from(affine.matrix3), Vec3::from(affine.translation));
    let planes: Vec<(Vec3, f32)> = [
        (Vec3::X, hi.x),
        (Vec3::NEG_X, -lo.x),
        (Vec3::Y, hi.y),
        (Vec3::NEG_Y, -lo.y),
        (Vec3::Z, hi.z),
        (Vec3::NEG_Z, -lo.z),
    ]
    .into_iter()
    .map(|(m, e)| (a.transpose() * m, e - m.dot(t)))
    .collect();

    // Every attribute as floats, a vertex's after another's; the position
    // first.
    let mut layout: Vec<(MeshVertexAttribute, usize)> = Vec::new();
    let mut columns: Vec<Vec<f32>> = Vec::new();
    let mut count = 0;
    let mut position = None;
    for (attribute, values) in mesh.attributes() {
        let (width, flat): (usize, Vec<f32>) = match values {
            VertexAttributeValues::Float32(v) => (1, v.clone()),
            VertexAttributeValues::Float32x2(v) => (2, v.iter().flatten().copied().collect()),
            VertexAttributeValues::Float32x3(v) => (3, v.iter().flatten().copied().collect()),
            VertexAttributeValues::Float32x4(v) => (4, v.iter().flatten().copied().collect()),
            _ => return Cut::Whole,
        };
        count = flat.len() / width;
        if attribute.id == Mesh::ATTRIBUTE_POSITION.id {
            position = Some(layout.len());
        }
        layout.push((*attribute, width));
        columns.push(flat);
    }
    let Some(position) = position else { return Cut::Whole };
    let stride: usize = layout.iter().map(|(_, w)| w).sum();
    let offset = |k: usize| layout[..k].iter().map(|(_, w)| w).sum::<usize>();
    let at = offset(position);
    let normal = layout.iter().position(|(a, _)| a.id == Mesh::ATTRIBUTE_NORMAL.id).map(offset);
    let tangent = layout.iter().position(|(a, _)| a.id == Mesh::ATTRIBUTE_TANGENT.id).map(offset);
    let vertex = |i: usize| -> Vec<f32> {
        let mut v = Vec::with_capacity(stride);
        for ((_, width), column) in layout.iter().zip(&columns) {
            v.extend_from_slice(&column[i * width..(i + 1) * width]);
        }
        v
    };
    let place_of = |v: &[f32]| Vec3::new(v[at], v[at + 1], v[at + 2]);
    let side = |p: Vec3, (n, d): (Vec3, f32)| n.dot(p) - d;

    let indices: Vec<usize> = match mesh.indices() {
        Some(indices) => indices.iter().collect(),
        None => (0..count).collect(),
    };
    let (mut out, mut out_indices): (Vec<Vec<f32>>, Vec<u32>) = (Vec::new(), Vec::new());
    // A vertex kept as it was, kept once.
    let mut kept = vec![u32::MAX; count];
    for &corners in indices.as_chunks::<3>().0 {
        if corners.iter().any(|&i| i >= count) {
            continue;
        }
        let points = corners.map(|i| place_of(&vertex(i)));
        if planes.iter().any(|&plane| points.iter().all(|&p| side(p, plane) > 0.0)) {
            continue;
        }
        if planes.iter().all(|&plane| points.iter().all(|&p| side(p, plane) <= 0.0)) {
            for i in corners {
                if kept[i] == u32::MAX {
                    kept[i] = out.len() as u32;
                    out.push(vertex(i));
                }
                out_indices.push(kept[i]);
            }
            continue;
        }
        // Cut by each plane in turn (Sutherland–Hodgman), then fanned.
        let mut polygon: Vec<Vec<f32>> = corners.iter().map(|&i| vertex(i)).collect();
        for &plane in &planes {
            let mut next = Vec::with_capacity(polygon.len() + 1);
            for k in 0..polygon.len() {
                let (a, b) = (&polygon[k], &polygon[(k + 1) % polygon.len()]);
                let (da, db) = (side(place_of(a), plane), side(place_of(b), plane));
                if da <= 0.0 {
                    next.push(a.clone());
                }
                if (da <= 0.0) != (db <= 0.0) {
                    let s = da / (da - db);
                    next.push(a.iter().zip(b).map(|(x, y)| x + (y - x) * s).collect());
                }
            }
            polygon = next;
            if polygon.len() < 3 {
                break;
            }
        }
        if polygon.len() < 3 {
            continue;
        }
        for v in &mut polygon {
            for (start, width) in [(normal, 3), (tangent, 3)] {
                if let Some(start) = start {
                    let n = Vec3::from_slice(&v[start..start + width]).normalize_or_zero();
                    v[start..start + width].copy_from_slice(&n.to_array());
                }
            }
        }
        let first = out.len() as u32;
        let sides = polygon.len() as u32;
        out.extend(polygon);
        for k in 1..sides - 1 {
            out_indices.extend([first, first + k, first + k + 1]);
        }
    }
    if out_indices.is_empty() {
        return Cut::Nothing;
    }
    let mut part = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    let mut start = 0;
    for (attribute, width) in layout {
        let values = match width {
            1 => VertexAttributeValues::Float32(out.iter().map(|v| v[start]).collect()),
            2 => VertexAttributeValues::Float32x2(out.iter().map(|v| [v[start], v[start + 1]]).collect()),
            3 => VertexAttributeValues::Float32x3(out.iter().map(|v| [v[start], v[start + 1], v[start + 2]]).collect()),
            _ => VertexAttributeValues::Float32x4(
                out.iter().map(|v| [v[start], v[start + 1], v[start + 2], v[start + 3]]).collect(),
            ),
        };
        part.insert_attribute(attribute, values);
        start += width;
    }
    part.insert_indices(Indices::U32(out_indices));
    Cut::Part(Box::new(part))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad() -> Mesh {
        // A 4 m square in the plane y = 0, from -2 to 2, facing up.
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(
                Mesh::ATTRIBUTE_POSITION,
                vec![[-2.0f32, 0.0, -2.0], [2.0, 0.0, -2.0], [2.0, 0.0, 2.0], [-2.0, 0.0, 2.0]],
            )
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; 4])
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
            .with_inserted_indices(Indices::U32(vec![0, 2, 1, 0, 3, 2]))
    }

    fn area(mesh: &Mesh) -> f32 {
        let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { return 0.0 };
        let indices: Vec<usize> = mesh.indices().map(|i| i.iter().collect()).unwrap_or_default();
        indices
            .chunks(3)
            .map(|t| {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(p[i]));
                (b - a).cross(c - a).length() / 2.0
            })
            .sum()
    }

    #[test]
    fn a_square_is_cut_to_the_box_its_texture_with_it() {
        let place = Transform::from_xyz(10.0, 1.0, 0.0);
        let Cut::Part(part) = clip(&quad(), &place, Vec3::new(9.0, 0.0, -5.0), Vec3::new(20.0, 2.0, 5.0)) else {
            panic!("a part")
        };
        // From x = 9 (local -1) to 2: 3 m by 4.
        assert!((area(&part) - 12.0).abs() < 1e-4, "{}", area(&part));
        let Some(VertexAttributeValues::Float32x3(p)) = part.attribute(Mesh::ATTRIBUTE_POSITION) else { panic!() };
        let Some(VertexAttributeValues::Float32x2(uv)) = part.attribute(Mesh::ATTRIBUTE_UV_0) else { panic!() };
        for (p, uv) in p.iter().zip(uv) {
            assert!(p[0] >= -1.0 - 1e-5);
            // u follows x: -2 → 0, 2 → 1.
            assert!((uv[0] - (p[0] + 2.0) / 4.0).abs() < 1e-5, "{p:?} {uv:?}");
        }
        // Still facing up, wound as before.
        let indices: Vec<usize> = part.indices().expect("indices").iter().collect();
        for t in indices.chunks(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(p[i]));
            assert!((b - a).cross(c - a).y > 0.0);
        }
    }

    #[test]
    fn a_turned_square_is_cut_in_the_world_s_frame() {
        // Turned a quarter about y and moved: local x is the world's -z.
        let place =
            Transform::from_xyz(0.0, 0.0, 10.0).with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
        let Cut::Part(part) = clip(&quad(), &place, Vec3::new(-5.0, -1.0, 9.0), Vec3::new(5.0, 1.0, 20.0)) else {
            panic!("a part")
        };
        assert!((area(&part) - 12.0).abs() < 1e-4, "{}", area(&part));
    }

    #[test]
    fn what_is_outside_or_inside_stays_out_or_whole() {
        let place = Transform::IDENTITY;
        assert!(matches!(clip(&quad(), &place, Vec3::splat(5.0), Vec3::splat(6.0)), Cut::Nothing));
        let Cut::Part(whole) = clip(&quad(), &place, Vec3::splat(-3.0), Vec3::splat(3.0)) else { panic!("whole") };
        assert!((area(&whole) - 16.0).abs() < 1e-4);
        assert_eq!(whole.count_vertices(), 4, "kept vertices kept once");
    }
}
