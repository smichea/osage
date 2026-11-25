// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::{AcousticBinding, AcousticEdge, AcousticIndex, Crossing};
use bevy::math::{DVec3, Vec3};
use osage_dynamics::StaticMesh;
use rapier3d::parry::{
    bounding_volume::BoundingVolume,
    query::{Ray, RayCast},
};
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{collections::BTreeMap, sync::OnceLock};

pub(super) fn blocks(transmission: Option<[f64; 3]>) -> bool {
    transmission.is_some_and(|bands| bands.into_iter().any(|t| t < 1.))
}

struct Boundary {
    a: [f64; 3],
    b: [f64; 3],
    // Coplanar bindings that own this same exposed span on the same side.
    // The first one among the direct-path crossings represents it once.
    earlier: Box<[usize]>,
}

pub(super) struct Cache {
    blocking: Vec<bool>,
    boundaries: Vec<OnceLock<Vec<Boundary>>>,
    #[cfg(test)]
    examined_leaves: AtomicUsize,
}

impl Cache {
    pub(super) fn new(bindings: &[AcousticBinding], blocks: impl Fn(&AcousticBinding) -> bool) -> Self {
        Self {
            blocking: bindings.iter().map(blocks).collect(),
            boundaries: (0..bindings.len()).map(|_| OnceLock::new()).collect(),
            #[cfg(test)]
            examined_leaves: AtomicUsize::new(0),
        }
    }

    pub(super) fn select(
        &self,
        index: &AcousticIndex,
        bindings: &[AcousticBinding],
        colliders: &[StaticMesh],
        crossings: &[Crossing],
        owner_path: impl Fn(&AcousticBinding) -> Vec<String>,
    ) -> Vec<AcousticEdge> {
        let by_id = index.binding_ids(bindings);
        let selected: BTreeMap<_, _> = crossings
            .iter()
            .filter(|c| blocks(c.transmission))
            .filter_map(|c| {
                let &n = by_id.get(&c.id)?;
                let path = owner_path(&bindings[n]);
                (self.blocking[n] && path == c.owner_path).then_some((n, path))
            })
            .collect();
        let faces = Neighbours {
            index,
            bindings,
            colliders,
            blocking: &self.blocking,
            #[cfg(test)]
            examined_leaves: &self.examined_leaves,
        };
        let mut result = Vec::new();
        for (&binding, path) in &selected {
            let edges = self.boundaries[binding].get_or_init(|| faces.boundaries(binding));
            for edge in edges {
                if !edge.earlier.iter().any(|n| selected.contains_key(n)) {
                    result.push(AcousticEdge {
                        wedge: None,
                        a: edge.a,
                        b: edge.b,
                        id: bindings[binding].id.clone(),
                        owner_path: path.clone(),
                    });
                }
            }
        }
        result
    }
}

struct Neighbours<'a> {
    index: &'a AcousticIndex,
    bindings: &'a [AcousticBinding],
    colliders: &'a [StaticMesh],
    blocking: &'a [bool],
    #[cfg(test)]
    examined_leaves: &'a AtomicUsize,
}

impl Neighbours<'_> {
    fn boundaries(&self, binding: usize) -> Vec<Boundary> {
        let mesh = self.bindings[binding].mesh(self.colliders);
        let mut output = vec![];
        for (index, tri) in mesh.triangles.iter().enumerate() {
            let p = tri.map(|i| Vec3::from(mesh.vertices[i as usize]).as_dvec3());
            for corner in 0..3 {
                if let Some(edge) = Edge::new(p[corner], p[(corner + 1) % 3], p[(corner + 2) % 3]) {
                    self.expose((binding, index), edge, &mut output);
                }
            }
        }
        output
    }

    fn expose(&self, source: (usize, usize), edge: Edge, out: &mut Vec<Boundary>) {
        let delta = (edge.b - edge.a).as_vec3().to_array();
        let ray = Ray::new(edge.a.as_vec3().to_array().into(), delta.into());
        let neighbours = self.index.candidates(self.bindings, self.colliders, |b| {
            b.loosened(edge.tolerance as f32).cast_local_ray(&ray, 1., true).is_some()
        });
        let mut events = Vec::new();
        for (binding, triangle, mesh) in neighbours {
            #[cfg(test)]
            self.examined_leaves.fetch_add(1, Ordering::Relaxed);
            let face = (binding, triangle);
            if source == face || !self.blocking[face.0] {
                continue;
            }
            let vertices = mesh.triangles[face.1].map(|i| Vec3::from(mesh.vertices[i as usize]).as_dvec3());
            let Some((lo, hi, other_side)) = edge.interval(vertices) else { continue };
            let duplicate = if other_side {
                None
            } else if face < source {
                // A duplicate triangle in this binding always has the same crossing.
                (face.0 != source.0).then_some(face.0)
            } else {
                continue;
            };
            if duplicate.is_none() && lo == 0. && hi == edge.length {
                return;
            }
            events.push((lo, duplicate, 1_i32));
            events.push((hi, duplicate, -1_i32));
        }
        // Sweep hidden and duplicate spans. A shorter neighbouring edge can end
        // halfway along this one, as at a panel junction surrounding an aperture.
        events.push((edge.length, None, 0));
        events.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        let mut earlier = BTreeMap::<usize, i32>::new();
        let mut covered = 0;
        let mut last = 0.;
        for (at, duplicate, change) in events {
            if at - last > edge.tolerance && covered == 0 {
                out.push(Boundary { a: edge.at(last), b: edge.at(at), earlier: earlier.keys().copied().collect() });
            }
            match duplicate {
                Some(binding) => {
                    let count = earlier.entry(binding).or_default();
                    *count += change;
                    if *count == 0 {
                        earlier.remove(&binding);
                    }
                }
                None => covered += change,
            }
            last = at;
        }
    }
}

struct Edge {
    a: DVec3,
    b: DVec3,
    along: DVec3,
    inside: DVec3,
    normal: DVec3,
    length: f64,
    tolerance: f64,
}

impl Edge {
    fn new(mut a: DVec3, mut b: DVec3, third: DVec3) -> Option<Self> {
        if a.to_array() > b.to_array() {
            std::mem::swap(&mut a, &mut b);
        }
        let distance = b - a;
        let length = distance.length();
        // World-space source vertices are f32. Limit seam welding to a tenth
        // of a millimetre even for very large coordinates.
        let tolerance = (a.abs().max(b.abs()).max_element() * f64::from(f32::EPSILON)).clamp(1e-6, 1e-4);
        if length <= tolerance {
            return None;
        }
        let along = distance / length;
        let inward = third - a;
        let across = inward - along * along.dot(inward);
        if across.length() <= tolerance {
            return None;
        }
        let inside = across.normalize();
        Some(Self { a, b, along, inside, normal: along.cross(inside), length, tolerance })
    }

    fn at(&self, t: f64) -> [f64; 3] {
        if t == self.length { self.b.to_array() } else { (self.a + self.along * t).to_array() }
    }

    /// Intersect the finite edge line with a triangle in its own plane. Only
    /// positive-length spans matter. A folded face occupying the line also
    /// seals it, e.g. the junction between two faces of a closed room.
    fn interval(&self, vertices: [DVec3; 3]) -> Option<(f64, f64, bool)> {
        let local = vertices.map(|v| v - self.a);
        let coplanar = local.iter().all(|v| v.dot(self.normal).abs() <= self.tolerance);
        let side = if coplanar {
            self.inside
        } else {
            let normal = (local[1] - local[0]).cross(local[2] - local[0]).try_normalize()?;
            if normal.dot(local[0]).abs() > self.tolerance || normal.dot(vertices[0] - self.b).abs() > self.tolerance {
                return None;
            }
            normal.cross(self.along).try_normalize()?
        };
        let h = local.map(|v| {
            let y = v.dot(side);
            if y.abs() <= self.tolerance { 0. } else { y }
        });
        if h.iter().all(|&v| v == 0.) {
            return None;
        }
        let x = local.map(|v| v.dot(self.along));
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for i in 0..3 {
            let j = (i + 1) % 3;
            let t = if h[i] == 0. {
                x[i]
            } else if (h[i] < 0. && h[j] > 0.) || (h[i] > 0. && h[j] < 0.) {
                x[i] + (x[j] - x[i]) * (h[i] / (h[i] - h[j]))
            } else {
                continue;
            };
            lo = lo.min(t);
            hi = hi.max(t);
        }
        lo = if lo <= self.tolerance { 0. } else { lo };
        hi = if self.length - hi <= self.tolerance { self.length } else { hi };
        (hi - lo > self.tolerance).then_some((lo, hi, !coplanar || h.iter().any(|&v| v < 0.)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::AcousticMesh;
    use osage_objects::AcousticSource;

    impl Cache {
        fn selected_edges(
            &self,
            tree: &AcousticIndex,
            bindings: &[AcousticBinding],
            colliders: &[StaticMesh],
            crossings: &[Crossing],
        ) -> Vec<AcousticEdge> {
            self.select(tree, bindings, colliders, crossings, |b| vec!["house".into(), b.id.clone()])
        }
    }

    fn surface(id: &str, vertices: Vec<[f32; 3]>, triangles: Vec<[u32; 3]>) -> AcousticBinding {
        AcousticBinding {
            owner_path: vec![id.into()],
            transmission: Some([0.; 3]),
            owner: 0,
            id: id.into(),
            material: "stone".into(),
            source: AcousticSource::Authored,
            spatial: Default::default(),
            mesh: AcousticMesh::Owned(StaticMesh { id: id.into(), vertices, triangles, material: None }.into()),
        }
    }

    fn quad(id: &str, p: [[f32; 3]; 4]) -> AcousticBinding {
        surface(id, p.to_vec(), vec![[0, 1, 2], [0, 2, 3]])
    }

    fn panel(id: &str, left: f32, right: f32, bottom: f32, top: f32) -> AcousticBinding {
        quad(id, [[left, bottom, 0.], [right, bottom, 0.], [right, top, 0.], [left, top, 0.]])
    }

    fn crossing(id: &str, transmission: Option<[f64; 3]>) -> Crossing {
        Crossing {
            shells: None,
            id: id.into(),
            owner_path: vec!["house".into(), id.into()],
            material: Some("stone".into()),
            transmission,
            distance_m: 3.,
        }
    }

    fn perimeter(edges: &[AcousticEdge]) -> f64 {
        edges.iter().map(|edge| DVec3::from(edge.a).distance(DVec3::from(edge.b))).sum()
    }

    #[test]
    fn a_split_room_is_certified_once_and_a_transparent_neighbour_cannot_join_two_rooms() {
        let faces = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
        let mut bindings = Vec::new();
        for x in [0., 10.] {
            for (wall, corners) in faces.iter().enumerate() {
                let points = corners.map(|i: u32| [x + (i & 1) as f32, ((i >> 1) & 1) as f32, ((i >> 2) & 1) as f32]);
                bindings.push(quad(&format!("room-{x}-wall-{wall}"), points));
            }
        }
        let mut bridge = panel("transparent", 0., 11., 0., 1.);
        bridge.transmission = Some([1.; 3]);
        bindings.push(bridge);
        let object = crate::object::AcousticGeometry::from_bindings(bindings);
        object.prepare_solid_edges(&[]);
        let prepared = || serde_json::to_value(object.index_storage()).unwrap()["solid_topology"].clone();
        assert_eq!(
            prepared(),
            serde_json::json!({"regions":0,"surfaces":0,"triangles":0,"certified_shells":0,"retained_vertices":0})
        );
        let trace = |x: f32, z: f32| {
            let (hits, _) = object.sound_crossings(&[], [x, 0.5, z], [x, 0.5, 0.5], [None; 2]);
            let edges = object.solid_edges(&[], &hits);
            assert_eq!(edges.len(), 12);
            let shell = edges[0].wedge.as_ref().unwrap().shell;
            assert!(edges.iter().all(|e| e.wedge.as_ref().unwrap().shell == shell));
            shell
        };
        let first = std::thread::scope(|scope| {
            let left = scope.spawn(|| trace(0.25, -1.));
            let right = scope.spawn(|| trace(0.75, 2.));
            let first = left.join().unwrap();
            assert_eq!(first, right.join().unwrap());
            first
        });
        assert_eq!(
            prepared(),
            serde_json::json!({"regions":1,"surfaces":6,"triangles":12,"certified_shells":1,"retained_vertices":8})
        );
        assert_ne!(trace(10.5, -1.), first);
        assert_eq!(
            prepared(),
            serde_json::json!({"regions":2,"surfaces":12,"triangles":24,"certified_shells":2,"retained_vertices":16})
        );
    }

    #[test]
    fn a_paged_subset_reuses_only_the_triangle_trees_intersected_by_sound() {
        let near = panel("near", -2., 2., -2., 2.);
        let far = panel("far", 100., 104., -2., 2.);
        let empty = surface("empty", vec![[0.; 3]], Vec::new());
        let one = crate::object::AcousticGeometry::from_bindings(vec![near.clone(), empty.clone(), far.clone()]);
        let subset = crate::object::AcousticGeometry::from_bindings(vec![near.clone()]);
        one.prepare_acoustics(&[]);
        assert!(near.spatial.triangles.get().is_none());
        assert!(one.sound_crossings(&[], [50., 0., 1.], [50., 0., -1.], [None; 2]).0.is_empty());
        assert!(near.spatial.triangles.get().is_none() && far.spatial.triangles.get().is_none());
        let hit = |g: &crate::object::AcousticGeometry| g.sound_crossings(&[], [0., 0., 1.], [0., 0., -1.], [None; 2]);
        let (first, tested) = hit(&one);
        assert_eq!((first.len(), tested), (1, 2));
        assert_eq!((first[0].id.as_str(), first[0].distance_m), ("near", 1.));
        let prepared = near.spatial.triangles.get().unwrap().as_ref();
        let (next, count) = hit(&subset);
        assert_eq!((next[0].id.as_str(), count), ("near", 2));
        assert!(std::ptr::eq(prepared, subset.acoustics[0].spatial.triangles.get().unwrap().as_ref()));
        assert!(empty.spatial.triangles.get().is_none() && far.spatial.triangles.get().is_none());
    }

    #[test]
    fn a_window_splits_the_long_edge_where_each_adjacent_blocking_panel_ends() {
        let bindings = vec![
            panel("left", -3., -1., -2., 2.),
            panel("right", 1., 3., -2., 2.),
            panel("above", -1., 1., 1., 2.),
            panel("below", -1., 1., -2., -1.),
        ];
        let tree = AcousticIndex::new(&bindings, &[]);
        let cache = Cache::new(&bindings, |_| true);
        let hits: Vec<_> = bindings.iter().map(|b| crossing(&b.id, Some([0.; 3]))).collect();
        let left = cache.selected_edges(&tree, &bindings, &[], &hits[..1]);
        assert_eq!(left.len(), 4);
        assert_eq!(perimeter(&left), 10.);
        assert!(left.iter().any(|e| e.a == [-1., -1., 0.] && e.b == [-1., 1., 0.]));
        assert!(left.iter().all(|e| e.id == "left" && e.owner_path == ["house", "left"]));
        assert_eq!(cache.boundaries.iter().filter(|b| b.get().is_some()).count(), 1);
        let whole = cache.selected_edges(&tree, &bindings, &[], &hits);
        assert_eq!(whole.len(), 12);
        assert_eq!(perimeter(&whole), 28.);
        assert_eq!(perimeter(&cache.selected_edges(&tree, &bindings, &[], &hits[..1])), 10.);
    }

    #[test]
    fn folds_of_a_closed_room_do_not_make_exposed_edges_even_across_bindings() {
        let corners: [[_; 3]; 8] =
            std::array::from_fn(|i| [(i & 1) as f32, ((i >> 1) & 1) as f32, ((i >> 2) & 1) as f32]);
        let faces = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
        let bindings: Vec<_> =
            faces.iter().enumerate().map(|(i, f)| quad(&format!("wall-{i}"), f.map(|j| corners[j]))).collect();
        let tree = AcousticIndex::new(&bindings, &[]);
        let cache = Cache::new(&bindings, |_| true);
        for face in &bindings {
            assert!(cache.selected_edges(&tree, &bindings, &[], &[crossing(&face.id, Some([0.; 3]))]).is_empty());
        }
        let open = Cache::new(&bindings, |b| b.id != "wall-0");
        let side = open.selected_edges(&tree, &bindings, &[], &[crossing("wall-2", Some([0.; 3]))]);
        assert_eq!(side.len(), 1);
        assert_eq!((side[0].a, side[0].b), ([0., 0., 0.], [1., 0., 0.]));
    }

    #[test]
    fn duplicate_sheets_keep_the_crossed_owner_and_transmitting_neighbours_leave_the_edge_open() {
        let points = vec![[-1., -1., 0.], [0., -1., 0.], [1., -1., 0.], [-1., 1., 0.], [0., 1., 0.], [1., 1., 0.]];
        let duplicate = surface("second", points, vec![[4, 1, 0], [3, 4, 0], [5, 2, 1], [4, 5, 1]]);
        let bindings = vec![panel("first", -1., 1., -1., 1.), duplicate, panel("open", 1., 2., -1., 1.)];
        let tree = AcousticIndex::new(&bindings, &[]);
        let cache = Cache::new(&bindings, |b| b.id != "open");
        let hits = [
            crossing("first", Some([0.5, 1., 1.])),
            crossing("second", Some([0.; 3])),
            crossing("open", Some([1.; 3])),
        ];
        let mut stale = crossing("first", Some([0.; 3]));
        stale.owner_path = vec!["different-house".into(), "first".into()];
        assert!(cache.selected_edges(&tree, &bindings, &[], &[stale]).is_empty());
        let both = cache.selected_edges(&tree, &bindings, &[], &hits);
        assert_eq!(perimeter(&both), 8.);
        assert!(both.iter().all(|s| s.id == "first"));
        let only = cache.selected_edges(&tree, &bindings, &[], &hits[1..]);
        assert_eq!(perimeter(&only), 8.);
        assert!(only.iter().all(|s| s.id == "second"));
        assert!(
            cache
                .selected_edges(&tree, &bindings, &[], &[crossing("first", None), crossing("second", Some([1.; 3]))])
                .is_empty()
        );
    }

    #[test]
    fn an_overlapping_sheet_clips_a_skewed_collision_proxy_in_world_coordinates() {
        let transform = |x: f32, y: f32| [17. + 2. * x + 0.5 * y, -6. + 0.5 * x + y, 10. + x + 0.5 * y];
        let patch = |id: &str, left, right, bottom, top| {
            quad(id, [transform(left, bottom), transform(right, bottom), transform(right, top), transform(left, top)])
        };
        let mut primary = patch("owner-sound", -2., 2., -2., 2.);
        let physical_id = "physical-screen";
        let AcousticMesh::Owned(mesh) =
            std::mem::replace(&mut primary.mesh, AcousticMesh::Proxy(physical_id.to_owned()))
        else {
            unreachable!()
        };
        let mut mesh = std::sync::Arc::try_unwrap(mesh).ok().expect("single owner of test mesh");
        mesh.id = physical_id.to_owned();
        let bindings = [primary, patch("neighbour", 1., 3., -1., 1.)];
        let colliders = [mesh];
        let tree = AcousticIndex::new(&bindings, &colliders);
        let cache = Cache::new(&bindings, |_| true);
        let edges = cache.selected_edges(&tree, &bindings, &colliders, &[crossing("owner-sound", Some([0.; 3]))]);
        assert_eq!(edges.len(), 5);
        for (start, end) in [(-2., -1.), (1., 2.)] {
            let from = Vec3::from(transform(2., start)).as_dvec3();
            let to = Vec3::from(transform(2., end)).as_dvec3();
            assert!(
                edges.iter().any(|e| from.distance(DVec3::from(e.a)) < 1e-7 && to.distance(DVec3::from(e.b)) < 1e-7)
            );
        }
        assert!(edges.iter().all(|e| e.id == "owner-sound" && e.owner_path == ["house", "owner-sound"]));
    }

    #[test]
    fn a_tessellated_sheet_queries_local_bvh_leaves_and_reuses_its_exposed_perimeter() {
        let mut previous_candidates = 0;
        for size in [16_u32, 32] {
            let pts: Vec<_> = (0..=size).flat_map(|y| (0..=size).map(move |x| [x as f32, y as f32, 0.])).collect();
            let faces: Vec<_> = (0..size)
                .flat_map(|y| {
                    (0..size).flat_map(move |x| {
                        let a = y * (size + 1) + x;
                        let b = a + size + 1;
                        [[a, a + 1, b + 1], [a, b + 1, b]]
                    })
                })
                .collect();
            let triangles = faces.len();
            let bindings = vec![surface("sheet", pts, faces)];
            let tree = AcousticIndex::new(&bindings, &[]);
            let cache = Cache::new(&bindings, |_| true);
            let hit = [crossing("sheet", Some([0.; 3]))];
            let out = cache.selected_edges(&tree, &bindings, &[], &hit);
            assert_eq!(out.len(), 4 * size as usize);
            assert_eq!(perimeter(&out), 4. * f64::from(size));
            let tested = cache.examined_leaves.load(Ordering::Relaxed);
            assert!(tested < 60 * triangles);
            assert!(previous_candidates == 0 || tested < 5 * previous_candidates);
            let warmed = cache.selected_edges(&tree, &bindings, &[], &hit);
            assert_eq!(warmed.len(), out.len());
            assert_eq!(cache.examined_leaves.load(Ordering::Relaxed), tested);
            println!(
                "{triangles} triangles, {tested} BVH candidate leaves on first extraction, 0 additional leaves on reuse"
            );
            previous_candidates = tested;
        }
    }
}
