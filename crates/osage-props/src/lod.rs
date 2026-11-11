// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Levels of detail, two ways.
//!
//! - **Vertex clustering**, for solid things (a rock, a trunk): the mesh's
//!   box is cut into cells; the vertices of a cell become one, at their mean;
//!   triangles whose corners fall in fewer than three cells vanish. Coarse
//!   next to an error-driven simplification, but simple, fast and enough for
//!   what is seen from afar, where only the silhouette and the colour remain.
//! - **Thinning**, for aggregates (needles, leaves, blades: many small pieces
//!   that touch no other): keep a share of the pieces, chosen at random, and
//!   grow each kept one about its centre so that the total area stays the
//!   same — stochastic simplification of aggregate detail (Cook, Halstead,
//!   Planck and Ryu, SIGGRAPH 2007). Clustering would merge each needle into
//!   nothing and leave a bare crown; thinning keeps the crown's coverage.

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

/// A piece is small, and may be thinned, below this share of the mesh's size.
const SMALL: f32 = 0.1;
/// A mesh is an aggregate with at least this many small pieces.
const MANY: usize = 64;

/// The pieces of a mesh that touch no other — a needle, a leaf, a blade —
/// each as the list of its vertices; corners at the same place are one
/// corner, so a piece split by a texture seam stays whole.
fn pieces(positions: &[[f32; 3]], indices: &[u32]) -> Vec<Vec<u32>> {
    // Corners welded by position, then joined through their triangles.
    let mut welded: HashMap<[u32; 3], u32> = HashMap::new();
    let corner: Vec<u32> =
        positions.iter().enumerate().map(|(i, p)| *welded.entry(p.map(f32::to_bits)).or_insert(i as u32)).collect();
    let mut parent: Vec<u32> = (0..positions.len() as u32).collect();
    fn root(parent: &mut [u32], mut i: u32) -> u32 {
        while parent[i as usize] != i {
            parent[i as usize] = parent[parent[i as usize] as usize];
            i = parent[i as usize];
        }
        i
    }
    for t in indices.as_chunks::<3>().0 {
        let [a, b, c] = t.map(|i| root(&mut parent, corner[i as usize]));
        parent[b as usize] = a;
        let c = root(&mut parent, c);
        parent[c as usize] = root(&mut parent, a);
    }
    let mut slot: HashMap<u32, usize> = HashMap::new();
    let mut pieces: Vec<Vec<u32>> = Vec::new();
    for i in 0..positions.len() as u32 {
        let r = root(&mut parent, corner[i as usize]);
        let next = pieces.len();
        let s = *slot.entry(r).or_insert(next);
        if s == next {
            pieces.push(Vec::new());
        }
        pieces[s].push(i);
    }
    pieces
}

/// A repeatable number in 0..1 for a piece.
fn chance(piece: usize, seed: u32) -> f32 {
    let mut h = (piece as u32).wrapping_mul(0x9E37_79B1) ^ seed.wrapping_mul(0x85EB_CA77);
    h = (h ^ (h >> 15)).wrapping_mul(0x2C1B_3C6D);
    h = (h ^ (h >> 12)).wrapping_mul(0x297A_2D39);
    ((h ^ (h >> 15)) & 0xff_ffff) as f32 / 16_777_216.0
}

/// The values of an attribute for the vertices `pick`, in that order.
fn gather(values: &VertexAttributeValues, pick: &[u32]) -> Option<VertexAttributeValues> {
    use VertexAttributeValues as V;
    fn take<T: Copy>(from: &[T], pick: &[u32]) -> Vec<T> {
        pick.iter().map(|&i| from[i as usize]).collect()
    }
    Some(match values {
        V::Float32(v) => V::Float32(take(v, pick)),
        V::Float32x2(v) => V::Float32x2(take(v, pick)),
        V::Float32x3(v) => V::Float32x3(take(v, pick)),
        V::Float32x4(v) => V::Float32x4(take(v, pick)),
        V::Uint16x4(v) => V::Uint16x4(take(v, pick)),
        V::Unorm16x4(v) => V::Unorm16x4(take(v, pick)),
        V::Unorm8x4(v) => V::Unorm8x4(take(v, pick)),
        V::Uint32(v) => V::Uint32(take(v, pick)),
        _ => return None,
    })
}

/// `mesh` with `keep` of its small pieces, chosen at random by `seed`, each
/// grown about its centre by `1/√keep` so that their total area stays the
/// same; large pieces (a trunk) are kept as they are. `None` if the mesh is
/// not a triangle list with positions, or not an aggregate of many small
/// pieces — a solid thing, for [`simplify`].
pub fn thin(mesh: &Mesh, keep: f32, seed: u32) -> Option<Mesh> {
    if mesh.primitive_topology() != PrimitiveTopology::TriangleList || !(0.0..1.0).contains(&keep) || keep <= 0.0 {
        return None;
    }
    let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
        return None;
    };
    let indices: Vec<u32> = match mesh.indices() {
        Some(indices) => indices.iter().map(|i| i as u32).collect(),
        None => (0..positions.len() as u32).collect(),
    };
    let size = extent(mesh);
    let pieces = pieces(positions, &indices);
    let bounds = |piece: &[u32]| {
        piece.iter().fold((Vec3::MAX, Vec3::MIN), |(low, high), &i| {
            let p = Vec3::from(positions[i as usize]);
            (low.min(p), high.max(p))
        })
    };
    // For each vertex: dropped, or kept and grown about a centre by a factor.
    let grow = 1.0 / keep.sqrt();
    let mut fate: Vec<Option<(Vec3, f32)>> = vec![None; positions.len()];
    let mut small = 0;
    for (n, piece) in pieces.iter().enumerate() {
        let (low, high) = bounds(piece);
        let fated = if (high - low).max_element() >= size * SMALL {
            Some((low, 1.0))
        } else {
            small += 1;
            (chance(n, seed) < keep).then_some(((low + high) / 2.0, grow))
        };
        for &i in piece {
            fate[i as usize] = fated;
        }
    }
    if small < MANY {
        return None;
    }

    // The kept vertices, renumbered in order; the triangles all of whose corners are kept.
    let mut renumber = vec![u32::MAX; positions.len()];
    let mut pick = Vec::new();
    for (i, f) in fate.iter().enumerate() {
        if f.is_some() {
            renumber[i] = pick.len() as u32;
            pick.push(i as u32);
        }
    }
    let kept: Vec<u32> = indices
        .as_chunks::<3>()
        .0
        .iter()
        .filter(|t| t.iter().all(|&i| fate[i as usize].is_some()))
        .flat_map(|t| t.map(|i| renumber[i as usize]))
        .collect();
    if kept.is_empty() {
        return None;
    }
    let mut out = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    for (attribute, values) in mesh.attributes() {
        if let Some(values) = gather(values, &pick) {
            out.insert_attribute(*attribute, values);
        }
    }
    let grown: Vec<[f32; 3]> = pick
        .iter()
        .map(|&i| {
            let (centre, factor) = fate[i as usize].unwrap_or((Vec3::ZERO, 1.0));
            (centre + (Vec3::from(positions[i as usize]) - centre) * factor).to_array()
        })
        .collect();
    out.insert_attribute(Mesh::ATTRIBUTE_POSITION, grown);
    out.insert_indices(Indices::U32(kept));
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

    /// `count` small separate squares of `side` metres, on a grid in a box of 10 m.
    fn needles(count: usize, side: f32) -> Mesh {
        let (mut positions, mut indices) = (Vec::new(), Vec::new());
        let row = (count as f32).sqrt().ceil() as usize;
        for n in 0..count {
            let at = Vec3::new((n % row) as f32, 0.0, (n / row) as f32) * (10.0 / row as f32);
            let base = positions.len() as u32;
            for corner in [Vec3::ZERO, Vec3::X, Vec3::new(1.0, 0.0, 1.0), Vec3::Z] {
                positions.push((at + corner * side).to_array());
            }
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
            .with_inserted_indices(Indices::U32(indices))
    }

    fn area(mesh: &Mesh) -> f32 {
        let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { return 0.0 };
        let indices: Vec<u32> = mesh.indices().unwrap().iter().map(|i| i as u32).collect();
        indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| Vec3::from(p[i as usize]));
                (b - a).cross(c - a).length() / 2.0
            })
            .sum()
    }

    #[test]
    fn thinning_keeps_a_share_of_the_pieces_and_their_area() {
        let crown = needles(2500, 0.05);
        let thinned = thin(&crown, 0.2, 7).expect("an aggregate");
        let share = triangles(&thinned) as f32 / triangles(&crown) as f32;
        assert!((share - 0.2).abs() < 0.03, "about a fifth of the pieces: {share}");
        let ratio = area(&thinned) / area(&crown);
        assert!((ratio - 1.0).abs() < 0.15, "the same area: {ratio}");
        assert!((extent(&thinned) - extent(&crown)).abs() < 0.2, "the same size");
        assert_eq!(thin(&crown, 0.2, 7).map(|m| triangles(&m)), Some(triangles(&thinned)), "repeatable");
    }

    #[test]
    fn a_solid_mesh_is_not_thinned() {
        let sphere: Mesh = Sphere::new(1.0).mesh().uv(96, 48);
        assert!(thin(&sphere, 0.2, 1).is_none(), "one piece: for clustering");
        assert!(thin(&needles(20, 0.05), 0.2, 1).is_none(), "too few pieces");
    }

    #[test]
    fn a_large_piece_is_kept_whole() {
        // A trunk (one large square) among needles: the trunk stays as it is.
        let crown = needles(400, 0.05);
        let mut with_trunk = needles(1, 4.0);
        with_trunk.merge(&crown).expect("same attributes");
        let thinned = thin(&with_trunk, 0.1, 3).expect("an aggregate");
        let trunk = 16.0;
        assert!(area(&thinned) > trunk, "the trunk is there: {}", area(&thinned));
        let Some(VertexAttributeValues::Float32x3(p)) = thinned.attribute(Mesh::ATTRIBUTE_POSITION) else { panic!() };
        assert!(p.contains(&[4.0, 0.0, 4.0]), "not grown");
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
