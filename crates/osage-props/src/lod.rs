// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Levels of detail by vertex clustering: the mesh's box is cut into cells;
//! the vertices of a cell become one, at their mean; triangles whose corners
//! fall in fewer than three cells vanish. Coarse next to an error-driven
//! simplification, but simple, fast and enough for what is seen from afar —
//! a tree, a bush, a rock — where only the silhouette and the colour remain.

use std::collections::HashMap;

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    prelude::*,
};

/// `mesh` with its vertices merged in cells of `cell` metres; `None` if it
/// is not a triangle list with positions, or nothing would be left.
pub fn simplify(mesh: &Mesh, cell: f32) -> Option<Mesh> {
    if mesh.primitive_topology() != PrimitiveTopology::TriangleList || cell <= 0.0 {
        return None;
    }
    let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
        return None;
    };
    let normals = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
        Some(VertexAttributeValues::Float32x3(normals)) => Some(normals),
        _ => None,
    };
    let uvs = match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
        Some(VertexAttributeValues::Float32x2(uvs)) => Some(uvs),
        _ => None,
    };
    let indices: Vec<u32> = match mesh.indices() {
        Some(indices) => indices.iter().map(|i| i as u32).collect(),
        None => (0..positions.len() as u32).collect(),
    };

    // Each vertex's cell; each cell's sums, to average.
    #[derive(Default)]
    struct Sum {
        position: Vec3,
        normal: Vec3,
        uv: Vec2,
        count: f32,
    }
    let mut cells: HashMap<IVec3, u32> = HashMap::new();
    let mut sums: Vec<Sum> = Vec::new();
    let mut remap = Vec::with_capacity(positions.len());
    // Cells start at the mesh's lowest corner: a mesh smaller than a cell is one cell.
    let low = positions.iter().fold(Vec3::MAX, |low, p| low.min(Vec3::from(*p)));
    for (i, p) in positions.iter().enumerate() {
        let p = Vec3::from(*p);
        let key = ((p - low) / cell).floor().as_ivec3();
        let next = sums.len() as u32;
        let slot = *cells.entry(key).or_insert(next);
        if slot == next {
            sums.push(Sum::default());
        }
        let sum = &mut sums[slot as usize];
        sum.position += p;
        if let Some(normals) = normals {
            sum.normal += Vec3::from(normals[i]);
        }
        if let Some(uvs) = uvs {
            sum.uv += Vec2::from(uvs[i]);
        }
        sum.count += 1.0;
        remap.push(slot);
    }

    let mut kept = Vec::with_capacity(indices.len());
    let mut seen = std::collections::HashSet::new();
    for triangle in indices.as_chunks::<3>().0 {
        let [a, b, c] = triangle.map(|i| remap[i as usize]);
        if a == b || b == c || a == c {
            continue;
        }
        // The same triangle twice, from both sides of a leaf, is kept once each way.
        let key = [a, b, c];
        let canonical = {
            let start = (0..3).min_by_key(|&k| key[k]).unwrap_or(0);
            [key[start], key[(start + 1) % 3], key[(start + 2) % 3]]
        };
        if seen.insert(canonical) {
            kept.extend([a, b, c]);
        }
    }
    if kept.is_empty() {
        return None;
    }

    let mut out = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default()).with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        sums.iter().map(|s| (s.position / s.count).to_array()).collect::<Vec<_>>(),
    );
    if normals.is_some() {
        let normals: Vec<[f32; 3]> = sums.iter().map(|s| s.normal.normalize_or(Vec3::Y).to_array()).collect();
        out.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    }
    if uvs.is_some() {
        out.insert_attribute(
            Mesh::ATTRIBUTE_UV_0,
            sums.iter().map(|s| (s.uv / s.count).to_array()).collect::<Vec<_>>(),
        );
    }
    out.insert_indices(Indices::U32(kept));
    // Normal maps need tangents; without UVs there are none to make.
    if mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some() && uvs.is_some() {
        let _ = out.generate_tangents();
    }
    Some(out)
}

/// The mesh's size: the longest side of its box.
pub fn extent(mesh: &Mesh) -> f32 {
    let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
        return 0.0;
    };
    let (low, high) = positions
        .iter()
        .fold((Vec3::MAX, Vec3::MIN), |(low, high), p| (low.min(Vec3::from(*p)), high.max(Vec3::from(*p))));
    (high - low).max_element().max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangles(mesh: &Mesh) -> usize {
        mesh.indices().map_or(0, |i| i.len() / 3)
    }

    #[test]
    fn a_dense_mesh_keeps_its_shape_with_fewer_triangles() {
        let sphere: Mesh = Sphere::new(1.0).mesh().uv(96, 48);
        let coarse = simplify(&sphere, 0.2).expect("something is left");
        assert!(triangles(&coarse) * 8 < triangles(&sphere), "{} from {}", triangles(&coarse), triangles(&sphere));
        assert!((extent(&coarse) - extent(&sphere)).abs() < 0.3, "about the same size: {}", extent(&coarse));
        if let Some(VertexAttributeValues::Float32x3(normals)) = coarse.attribute(Mesh::ATTRIBUTE_NORMAL) {
            assert!(normals.iter().all(|n| (Vec3::from(*n).length() - 1.0).abs() < 1e-3), "unit normals");
        }
    }

    #[test]
    fn no_triangle_collapses_to_a_line_or_a_point() {
        let sphere: Mesh = Sphere::new(1.0).mesh().ico(3).expect("an icosphere");
        let coarse = simplify(&sphere, 0.4).expect("something is left");
        let indices: Vec<u32> = coarse.indices().unwrap().iter().map(|i| i as u32).collect();
        for t in indices.as_chunks::<3>().0 {
            assert!(t[0] != t[1] && t[1] != t[2] && t[0] != t[2], "{t:?}");
        }
    }

    #[test]
    fn a_cell_larger_than_the_mesh_leaves_nothing() {
        let cube: Mesh = Cuboid::new(1.0, 1.0, 1.0).mesh().build();
        assert!(simplify(&cube, 10.0).is_none());
        assert!(simplify(&cube, 0.0).is_none(), "a cell must have a size");
        // A cell smaller than the details changes nothing that matters.
        let fine = simplify(&cube, 0.01).expect("the cube");
        assert_eq!(triangles(&fine), triangles(&cube));
    }
}
