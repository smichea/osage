// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

#[path = "acoustic_edges.rs"]
mod edges;

use super::{AcousticBinding, AcousticEdge, AcousticGeometry, AcousticMesh};
use bevy::math::Vec3;
use osage_dynamics::StaticMesh;
use osage_objects::TriangleRay;
use rapier3d::parry::{bounding_volume::Aabb, partitioning::Bvh};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, OnceLock};

#[derive(Serialize)]
pub(crate) struct Crossing {
    #[serde(skip)]
    pub shells: Option<Vec<(usize, usize)>>,
    pub id: String,
    pub owner_path: Vec<String>,
    pub material: Option<String>,
    /// An unknown material has no measured transmission.
    pub transmission: Option<[f64; 3]>,
    pub distance_m: f64,
}

pub(super) struct AcousticHit<'a> {
    pub surface: &'a AcousticBinding,
    pub distance_m: f64,
    shells: Option<Vec<(usize, usize)>>,
}

impl AcousticBinding {
    fn mesh<'a>(&'a self, colliders: &'a [StaticMesh]) -> &'a StaticMesh {
        match &self.mesh {
            AcousticMesh::Owned(mesh) => mesh,
            AcousticMesh::Proxy(id) => {
                &colliders[colliders.binary_search_by(|m| m.id.cmp(id)).expect("validated physical surface")]
            }
        }
    }
}

#[path = "acoustic_solids.rs"]
mod solids;

#[path = "acoustic_regions.rs"]
mod regions;

// A placement's world mesh is immutable. The same binding is cloned into several
// paged impact paths and keeps its bounds and triangle tree across those paths.
#[derive(Default)]
pub(super) struct SurfaceIndex {
    bounds: OnceLock<Option<Aabb>>,
    triangles: OnceLock<Box<Bvh>>,
}

fn padded_bounds(lo: Vec3, hi: Vec3) -> Aabb {
    let pad = lo.abs().max(hi.abs()).max(Vec3::ONE) * (4. * f32::EPSILON);
    Aabb::new((lo - pad).to_array().into(), (hi + pad).to_array().into())
}

impl SurfaceIndex {
    fn bounds(&self, mesh: &StaticMesh) -> Option<Aabb> {
        *self.bounds.get_or_init(|| {
            let (&first, rest) = mesh.vertices.split_first()?;
            if mesh.triangles.is_empty() {
                return None;
            }
            let first = Vec3::from_array(first);
            let (lo, hi) = rest.iter().fold((first, first), |(lo, hi), &p| {
                let p = Vec3::from_array(p);
                (lo.min(p), hi.max(p))
            });
            Some(padded_bounds(lo, hi))
        })
    }

    fn triangles(&self, mesh: &StaticMesh) -> &Bvh {
        self.triangles.get_or_init(|| {
            let bounds = mesh
                .triangles
                .iter()
                .map(|indices| {
                    let [a, b, c] = indices.map(|i| Vec3::from_array(mesh.vertices[i as usize]));
                    padded_bounds(a.min(b).min(c), a.max(b).max(c))
                })
                .collect::<Vec<_>>();
            Box::new(Bvh::from_leaves(Default::default(), &bounds))
        })
    }
}

pub(crate) struct AcousticIndex {
    tree: Bvh,
    surfaces: Vec<(usize, Arc<SurfaceIndex>)>,
    by_id: OnceLock<HashMap<String, usize>>,
    edges: OnceLock<edges::Cache>,
    solids: OnceLock<regions::Regions>,
}

impl AcousticIndex {
    fn new(bindings: &[AcousticBinding], colliders: &[StaticMesh]) -> Self {
        let mut bounds = Vec::new();
        let mut surfaces = Vec::new();
        for (binding, surface) in bindings.iter().enumerate() {
            if let Some(bound) = surface.spatial.bounds(surface.mesh(colliders)) {
                bounds.push(bound);
                surfaces.push((binding, Arc::clone(&surface.spatial)));
            }
        }
        Self {
            tree: Bvh::from_leaves(Default::default(), &bounds),
            surfaces,
            by_id: Default::default(),
            edges: Default::default(),
            solids: Default::default(),
        }
    }

    fn binding_ids(&self, bindings: &[AcousticBinding]) -> &HashMap<String, usize> {
        self.by_id.get_or_init(|| bindings.iter().enumerate().map(|(i, b)| (b.id.clone(), i)).collect())
    }

    fn candidates<'a>(
        &'a self,
        bindings: &'a [AcousticBinding],
        colliders: &'a [StaticMesh],
        intersects: impl Fn(Aabb) -> bool + Copy + 'a,
    ) -> impl Iterator<Item = (usize, usize, &'a StaticMesh)> + 'a {
        self.tree.leaves(move |node| intersects(node.aabb())).flat_map(move |i| {
            let (binding, spatial) = &self.surfaces[i as usize];
            let mesh = bindings[*binding].mesh(colliders);
            spatial
                .triangles(mesh)
                .leaves(move |node| intersects(node.aabb()))
                .map(move |triangle| (*binding, triangle as usize, mesh))
        })
    }
}

impl AcousticGeometry {
    pub(crate) fn index_storage(&self) -> Option<IndexStorage> {
        self.acoustic_index.get().map(AcousticIndex::storage)
    }

    pub(crate) fn prepare_acoustics(&self, colliders: &[StaticMesh]) -> &AcousticIndex {
        self.acoustic_index.get_or_init(|| AcousticIndex::new(&self.acoustics, colliders))
    }

    pub(super) fn acoustic_hits(
        &self,
        colliders: &[StaticMesh],
        start: [f32; 3],
        end: [f32; 3],
        endpoint_margin: f64,
        ends: [Option<&super::ExteriorWedge>; 2],
    ) -> (Vec<AcousticHit<'_>>, usize) {
        let length = Vec3::from(start).as_dvec3().distance(Vec3::from(end).as_dvec3());
        if length <= 2. * endpoint_margin || self.acoustics.is_empty() {
            return (Vec::new(), 0);
        }
        let index = self.prepare_acoustics(colliders);
        let mut nearest = BTreeMap::<usize, (f64, BTreeSet<solids::Face>)>::new();
        let Some(segment) = TriangleRay::new(start, end) else { return (Vec::new(), 0) };
        let mut tested = 0;
        for (binding, triangle, mesh) in index
            .candidates(&self.acoustics, colliders, |b| segment.intersects_bounds(b.mins.to_array(), b.maxs.to_array()))
        {
            let face = mesh.triangles[triangle].map(|i| mesh.vertices[i as usize]);
            tested += 1;
            let Some(distance) = segment.hit(face) else { continue };
            if distance < endpoint_margin || distance > length - endpoint_margin {
                continue;
            }
            let solids = index.solids.get().and_then(|regions| regions.for_binding(index, self, colliders, binding));
            if ends.iter().any(Option::is_some)
                && solids.is_some_and(|s| s.grazes((binding, triangle), face, start, end, ends))
            {
                continue;
            }
            let hit = nearest.entry(binding).or_insert_with(|| (distance, BTreeSet::new()));
            hit.0 = hit.0.min(distance);
            if let Some(shell) = solids.and_then(|s| s.shell_for(binding, triangle)) {
                hit.1.insert(shell);
            }
        }
        let mut hits: Vec<_> = nearest
            .into_iter()
            .map(|(i, (distance_m, shells))| AcousticHit {
                surface: &self.acoustics[i],
                distance_m,
                shells: index.solids.get().map(|_| shells.into_iter().collect()),
            })
            .collect();
        hits.sort_by(|a, b| a.distance_m.total_cmp(&b.distance_m).then_with(|| a.surface.id.cmp(&b.surface.id)));
        (hits, tested)
    }

    /// Return the open boundary of each measured direct-path crossing in this
    /// object's active acoustic tree. The topology is cached on its static BVH.
    pub(crate) fn sound_edges(&self, colliders: &[StaticMesh], crossings: &[Crossing]) -> Vec<AcousticEdge> {
        if !crossings.iter().any(|c| edges::blocks(c.transmission)) {
            return Vec::new();
        }
        let index = self.prepare_acoustics(colliders);
        let cache = index
            .edges
            .get_or_init(|| edges::Cache::new(&self.acoustics, |surface| edges::blocks(surface.transmission)));
        cache.select(index, &self.acoustics, colliders, crossings, |s| s.owner_path.clone())
    }

    pub(crate) fn prepare_solid_edges(&self, colliders: &[StaticMesh]) {
        self.prepare_acoustics(colliders).solids.get_or_init(|| regions::Regions::new(self.acoustics.len()));
    }

    pub(crate) fn solid_edges(&self, colliders: &[StaticMesh], crossings: &[Crossing]) -> Vec<AcousticEdge> {
        if !crossings.iter().any(|c| edges::blocks(c.transmission)) {
            return Vec::new();
        }
        let index = self.prepare_acoustics(colliders);
        let regions = index.solids.get_or_init(|| regions::Regions::new(self.acoustics.len()));
        regions.edges(index, self, colliders, crossings)
    }

    pub(crate) fn sound_crossings(
        &self,
        colliders: &[StaticMesh],
        start: [f32; 3],
        end: [f32; 3],
        ends: [Option<&super::ExteriorWedge>; 2],
    ) -> (Vec<Crossing>, usize) {
        let (hits, tested) = self.acoustic_hits(colliders, start, end, 0.0001, ends);
        (
            hits.into_iter()
                .map(|hit| {
                    let s = hit.surface;
                    Crossing {
                        shells: hit.shells,
                        id: s.id.clone(),
                        owner_path: s.owner_path.clone(),
                        material: Some(s.material.clone()),
                        transmission: s.transmission,
                        distance_m: hit.distance_m,
                    }
                })
                .collect(),
            tested,
        )
    }
}

#[derive(Serialize)]
pub(crate) struct IndexStorage {
    surfaces: u32,
    triangle_trees: usize,
    indexed_triangles: u64,
    surface_tree_bytes: usize,
    triangle_tree_bytes: usize,
    binding_reference_bytes: usize,
    solid_topology: regions::Storage,
}

impl AcousticIndex {
    fn storage(&self) -> IndexStorage {
        let mut stats = IndexStorage {
            surfaces: self.tree.leaf_count(),
            triangle_trees: 0,
            indexed_triangles: 0,
            surface_tree_bytes: self.tree.heap_memory_size(),
            triangle_tree_bytes: 0,
            binding_reference_bytes: self.surfaces.capacity() * size_of::<(usize, Arc<SurfaceIndex>)>(),
            solid_topology: self.solids.get().map_or_else(Default::default, regions::Regions::storage),
        };
        for (_, surface) in &self.surfaces {
            if let Some(tree) = surface.triangles.get() {
                stats.triangle_trees += 1;
                stats.indexed_triangles += u64::from(tree.leaf_count());
                stats.triangle_tree_bytes += tree.heap_memory_size();
            }
        }
        stats
    }

    #[cfg(all(test, target_os = "linux", target_env = "gnu", target_pointer_width = "64"))]
    pub(super) fn measured_storage(&self) -> (u64, usize, usize) {
        let storage = self.storage();
        (
            u64::from(storage.surfaces) + storage.indexed_triangles,
            storage.surface_tree_bytes + storage.triangle_tree_bytes,
            storage.binding_reference_bytes,
        )
    }
}
