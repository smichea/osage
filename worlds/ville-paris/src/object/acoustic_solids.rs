// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Exterior wedges of closed convex acoustic shells, including shells split across nodes.
use super::{AcousticBinding, AcousticEdge, AcousticGeometry, edges};
use crate::object::ExteriorWedge;
use bevy::math::{DVec3, Vec3};
use osage_dynamics::StaticMesh;
use std::collections::{BTreeMap, BTreeSet};

// The first original face of a shell provides a stable order across independently prepared regions.
pub(super) type Face = (usize, usize);

struct Triangle {
    local: usize,
    corners: [usize; 3],
    binding: usize,
}

struct Wedge {
    tolerance_m: f64,
    ends: [usize; 2],
    owners: [usize; 2],
    normals: [[f64; 3]; 2],
}

pub(super) struct Solids {
    vertices: Vec<DVec3>,
    shells: BTreeMap<Face, Vec<Wedge>>,
    pub(super) by_binding: BTreeMap<usize, Vec<Face>>,
    shell_by_face: BTreeMap<Face, Face>,
    pub(super) first_binding: usize,
    pub(super) triangle_count: usize,
}

fn side(a: usize, b: usize) -> (usize, usize) {
    (a.min(b), a.max(b))
}

impl Solids {
    pub(super) fn new(object: &AcousticGeometry, colliders: &[StaticMesh], bindings: &BTreeSet<usize>) -> Self {
        let mut vertices = Vec::new();
        let mut welded = BTreeMap::new();
        let mut faces = Vec::new();
        let mut seams = BTreeMap::<(usize, usize), Vec<usize>>::new();
        for &binding in bindings {
            let surface = &object.acoustics[binding];
            debug_assert!(edges::blocks(surface.transmission));
            let mesh = surface.mesh(colliders);
            for (local, tri) in mesh.triangles.iter().enumerate() {
                let corners = tri.map(|i| {
                    let p = mesh.vertices[i as usize];
                    // Signed zero denotes the same point in the submitted GLB.
                    let bits = p.map(|c| if c == 0. { 0 } else { c.to_bits() });
                    *welded.entry(bits).or_insert_with(|| {
                        vertices.push(Vec3::from(p).as_dvec3());
                        vertices.len() - 1
                    })
                });
                for i in 0..3 {
                    seams.entry(side(corners[i], corners[(i + 1) % 3])).or_default().push(faces.len());
                }
                faces.push(Triangle { corners, binding, local });
            }
        }
        let mut seen = vec![false; faces.len()];
        let mut shells = BTreeMap::new();
        let mut by_binding = BTreeMap::new();
        let mut shell_by_face = BTreeMap::new();
        for root in 0..faces.len() {
            if seen[root] {
                continue;
            }
            let mut stack = vec![root];
            seen[root] = true;
            let mut component = Vec::new();
            let mut closed = true;
            while let Some(i) = stack.pop() {
                component.push(i);
                let tri = &faces[i].corners;
                for j in 0..3 {
                    let neighbours = &seams[&side(tri[j], tri[(j + 1) % 3])];
                    closed &= neighbours.len() == 2;
                    for &n in neighbours {
                        if !seen[n] {
                            seen[n] = true;
                            stack.push(n);
                        }
                    }
                }
            }
            if !closed {
                continue;
            }
            let Some(wedges) = certify(&component, &faces, &vertices, &seams) else { continue };
            let first = (faces[root].binding, faces[root].local);
            for &i in &component {
                shell_by_face.insert((faces[i].binding, faces[i].local), first);
            }
            let owners: BTreeSet<_> = component.iter().map(|&i| faces[i].binding).collect();
            for owner in owners {
                by_binding.entry(owner).or_insert_with(Vec::new).push(first);
            }
            shells.insert(first, wedges);
        }
        // Open sheets can be remembered as checked without retaining their welded vertices.
        if shells.is_empty() {
            vertices = Vec::new();
        }
        Self {
            vertices,
            shells,
            by_binding,
            shell_by_face,
            first_binding: *bindings.first().expect("nonempty acoustic region"),
            triangle_count: faces.len(),
        }
    }

    pub(super) fn retained_geometry(&self) -> (usize, usize) {
        (self.shells.len(), self.vertices.len())
    }

    pub(super) fn shell_for(&self, binding: usize, triangle: usize) -> Option<Face> {
        self.shell_by_face.get(&(binding, triangle)).copied()
    }

    /// A leg on its own certified support plane can cross a GLB triangle after rounding
    /// to float. Apply the plane tolerance only to triangles of that same shell.
    pub(super) fn grazes(
        &self,
        face: (usize, usize),
        vertices: [[f32; 3]; 3],
        start: [f32; 3],
        end: [f32; 3],
        ends: [Option<&ExteriorWedge>; 2],
    ) -> bool {
        let Some(shell) = self.shell_by_face.get(&face) else { return false };
        let Some(wedge) = ends.into_iter().flatten().find(|w| w.shell == *shell) else { return false };
        let [a, b, c] = vertices.map(|p| Vec3::from(p).as_dvec3());
        let Some(normal) = (b - a).cross(c - a).try_normalize() else { return false };
        [start, end].into_iter().all(|p| {
            let p = Vec3::from(p).as_dvec3();
            let eps = wedge.tolerance_m + p.abs().max_element() * f64::from(f32::EPSILON);
            normal.dot(p - a).abs() <= eps
        })
    }

    pub(super) fn append_edges(
        &self,
        index: Face,
        surfaces: &[AcousticBinding],
        hit: &BTreeSet<usize>,
        output: &mut Vec<AcousticEdge>,
    ) {
        for wedge in &self.shells[&index] {
            let owner_index = (0..2)
                .min_by_key(|&i| {
                    let owner = wedge.owners[i];
                    (!hit.contains(&owner), &surfaces[owner].id)
                })
                .unwrap();
            let surface = &surfaces[wedge.owners[owner_index]];
            let other = &surfaces[wedge.owners[1 - owner_index]];
            let [a, b] = wedge.ends.map(|i| self.vertices[i].to_array());
            output.push(AcousticEdge {
                a,
                b,
                id: surface.id.clone(),
                owner_path: surface.owner_path.clone(),
                wedge: Some(ExteriorWedge {
                    shell: index,
                    tolerance_m: wedge.tolerance_m,
                    normals: [wedge.normals[owner_index], wedge.normals[1 - owner_index]],
                    adjacent_surface: other.id.clone(),
                    adjacent_owner_path: other.owner_path.clone(),
                }),
            });
        }
    }
}

// Certify the interior as the intersection of the face half-spaces. Winding in independent
// bindings can differ. The centroid of the vertices of a convex shell is strictly inside.
fn certify(
    component: &[usize],
    faces: &[Triangle],
    vertices: &[DVec3],
    seams: &BTreeMap<(usize, usize), Vec<usize>>,
) -> Option<Vec<Wedge>> {
    if component.len() < 4 {
        return None;
    }
    let indices: BTreeSet<_> = component.iter().flat_map(|&i| faces[i].corners).collect();
    let mut points: Vec<_> = indices.into_iter().map(|i| vertices[i]).collect();
    points.sort_by(|a, b| a.x.total_cmp(&b.x).then_with(|| a.y.total_cmp(&b.y)).then_with(|| a.z.total_cmp(&b.z)));
    let center = points.iter().sum::<DVec3>() / points.len() as f64;
    let blocks: Vec<_> = points
        .chunks(32)
        .map(|p| {
            let (lo, hi) = p.iter().fold((p[0], p[0]), |(lo, hi), &v| (lo.min(v), hi.max(v)));
            (lo, hi, p)
        })
        .collect();
    let radius = points.iter().map(|p| p.distance(center)).fold(0., f64::max);
    let tolerance = 2e-7 * radius.max(0.001);
    let mut normals = BTreeMap::new();
    for &i in component {
        let [a, b, c] = faces[i].corners.map(|v| vertices[v]);
        let mut n = (b - a).cross(c - a).try_normalize()?;
        let inside = n.dot(center - a);
        if inside.abs() <= tolerance {
            return None;
        }
        if inside > 0. {
            n = -n;
        }
        for &(lo, hi, block) in &blocks {
            let support = DVec3::select(n.cmpge(DVec3::ZERO), hi, lo);
            if (support - a).dot(n) > tolerance && block.iter().any(|p| (*p - a).dot(n) > tolerance) {
                return None;
            }
        }
        normals.insert(i, n);
    }
    let mut wedges = Vec::new();
    for &i in component {
        let tri = faces[i].corners;
        for j in 0..3 {
            let (a, b) = side(tri[j], tri[(j + 1) % 3]);
            let pair = &seams[&(a, b)];
            if pair[0] != i {
                continue;
            }
            let (n, m) = (normals[&i], normals[&pair[1]]);
            if n.dot(m) < 1. - 1e-10 {
                wedges.push(Wedge {
                    tolerance_m: tolerance,
                    ends: [a, b],
                    owners: [faces[i].binding, faces[pair[1]].binding],
                    normals: [n.to_array(), m.to_array()],
                });
            }
        }
    }
    (!wedges.is_empty()).then_some(wedges)
}
