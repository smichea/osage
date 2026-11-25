// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

pub(super) mod resident;

use super::{
    models::Affine,
    source::Source,
    tiles::{File, Index},
};
use anyhow::{Context, Result, ensure};
use bevy::{
    math::DMat3,
    prelude::{Mat4, Vec3},
};
use rapier3d::parry::{
    bounding_volume::Aabb,
    partitioning::{Bvh, TraversalAction},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, btree_map::Entry},
    sync::OnceLock,
};

#[derive(Deserialize)]
pub struct Profile {
    id: String,
    nodes: Vec<Node>,
    acoustic_materials: BTreeMap<String, osage_objects::AcousticMaterial>,
    acoustic_meshes: Vec<File>,
    // New directories measure bounds after the native float placement.
    #[serde(default)]
    placed_bounds: bool,
    #[serde(skip)]
    spatial: OnceLock<Spatial>,
    #[serde(skip)]
    rooms: OnceLock<Option<std::sync::Arc<crate::object::rooms::Graph>>>,
}
#[derive(Deserialize)]
struct Node {
    node: usize,
    id: String,
    #[serde(default)]
    collision: Vec<osage_objects::CollisionSurface>,
    #[serde(default)]
    acoustic: Vec<Acoustic>,
    #[serde(rename = "acousticRoom", default)]
    room: Option<osage_objects::AcousticRoom>,
    #[serde(rename = "acousticPortal", default)]
    portal: Option<osage_objects::AcousticPortal>,
    #[serde(default)]
    indoor_pose: Option<Affine>,
}
#[derive(Deserialize)]
struct Acoustic {
    #[serde(flatten)]
    surface: osage_objects::AcousticSurface,
    resource: usize,
    bounds: [[f32; 3]; 2],
    pose: Affine,
}
#[derive(Deserialize)]
struct Faces {
    vertices: Vec<[f32; 3]>,
    triangles: Vec<[usize; 3]>,
}

impl Profile {
    pub(super) fn collision_materials(&self) -> impl Iterator<Item = ((usize, &str), &str)> {
        self.nodes.iter().flat_map(|node| {
            node.collision.iter().map(move |surface| ((node.node, surface.id.as_str()), surface.material.as_str()))
        })
    }

    pub fn validate(&self, index: &Index) -> Result<()> {
        let mut nodes = BTreeSet::new();
        let mut ids = BTreeSet::new();
        ensure!(self.acoustic_materials.values().all(|m| m.valid()), "invalid acoustic material");
        let mut acoustic = BTreeSet::new();
        let mut collision = BTreeSet::new();
        let mut files = BTreeMap::new();
        for file in &self.acoustic_meshes {
            file.validate("acoustic")?;
            ensure!(
                files.insert(&file.sha256, file.bytes).is_none_or(|before| before == file.bytes),
                "inconsistent acoustic resource length"
            );
        }
        for n in &self.nodes {
            ensure!(
                n.node < index.source_parents.len() && !n.id.is_empty() && ids.insert(&n.id) && nodes.insert(n.node),
                "invalid object owner"
            );
            for c in &n.collision {
                ensure!(collision.insert((n.node, &c.id)), "duplicate physical surface on owner");
            }
            for a in &n.acoustic {
                let s = &a.surface;
                ensure!(
                    a.resource < self.acoustic_meshes.len()
                        && self.acoustic_materials.contains_key(&s.material)
                        && acoustic.insert(&s.id)
                        && a.bounds.iter().flatten().all(|v| v.is_finite())
                        && (0..3).all(|i| a.bounds[0][i] <= a.bounds[1][i]),
                    "invalid acoustic binding"
                );
                ensure!(a.pose.matrix()?.row(3) == bevy::math::Vec4::W, "invalid acoustic affine");
            }
        }
        ensure!(
            index
                .tiles
                .iter()
                .filter_map(|t| t.physics.as_ref())
                .flat_map(|l| l.source_nodes.iter().copied().zip(&l.ids))
                .all(|key| collision.contains(&key)),
            "physics page differs from its authored surface binding"
        );
        if self.rooms.get().is_none() {
            let placed = self
                .nodes
                .iter()
                .filter(|n| n.room.is_some() || n.portal.is_some())
                .map(|n| {
                    let world = n.indoor_pose.as_ref().context("missing indoor node placement")?.matrix()?;
                    Ok(crate::object::rooms::Placed {
                        id: &n.id,
                        world,
                        room: n.room.as_ref(),
                        portal: n.portal.as_ref(),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let graph = crate::object::rooms::Graph::from_placed(&placed)?.map(std::sync::Arc::new);
            self.rooms.get_or_init(|| graph);
        }
        Ok(())
    }

    pub(super) fn attach_rooms(&self, geometry: &mut crate::object::AcousticGeometry) {
        geometry.rooms = self.rooms.get().and_then(Option::as_ref).cloned();
    }

    pub(super) fn file(&self, slot: usize) -> &File {
        &self.acoustic_meshes[slot]
    }

    /// The returned resource numbers refer to this profile and do not load content.
    pub(super) fn candidates(&self, ray: Segment) -> Result<Candidates> {
        let (from, to) = ray;
        ensure!(from.into_iter().chain(to).all(f32::is_finite) && from != to, "invalid acoustic ray");
        let spatial = self.spatial.get_or_init(|| Spatial::new(self));
        let mut selected = Candidates::default();
        spatial.tree.tree.traverse(|node| {
            selected.box_tests += 1;
            let bounds = node.aabb();
            if !segment_box(ray, bounds.mins.to_array(), bounds.maxs.to_array()) {
                return TraversalAction::Prune;
            }
            if let Some(i) = node.leaf_data() {
                selected.bindings.push(spatial.tree.ids[i as usize]);
            }
            TraversalAction::Continue
        });
        // Older directories lack a reliable enclosure for a singular or ill-conditioned placement.
        selected.bindings.extend_from_slice(&spatial.unbounded);
        selected.bindings.sort_unstable(); // source order for equal-distance hits
        selected.resources = selected
            .bindings
            .iter()
            .map(|&i| spatial.bindings[i].resource)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        Ok(selected)
    }
}

pub(super) type Segment = ([f32; 3], [f32; 3]);

#[derive(Default)]
pub(super) struct Candidates {
    pub(super) resources: Vec<usize>,
    bindings: Vec<usize>,
    box_tests: usize,
}

struct Binding {
    node: usize,
    surface: usize,
    resource: usize,
    bounds: Option<Aabb>,
}

struct Tree {
    tree: Bvh,
    ids: Vec<usize>,
}
struct Spatial {
    bindings: Vec<Binding>,
    tree: Tree,
    unbounded: Vec<usize>,
}

// Intersect the closed segment parameter interval with the three closed slabs.
// Keep tangencies and endpoints. The input coordinates are f32, the divisions f64.
fn segment_box((from, to): Segment, lo: [f32; 3], hi: [f32; 3]) -> bool {
    let (mut enter, mut leave) = (0_f64, 1_f64);
    for i in 0..3 {
        let a = f64::from(from[i]);
        let d = f64::from(to[i]) - a;
        let (l, h) = (f64::from(lo[i]), f64::from(hi[i]));
        if d == 0. {
            if a < l || a > h {
                return false;
            }
        } else {
            let (x, y) = ((l - a) / d, (h - a) / d);
            enter = enter.max(x.min(y).next_down());
            leave = leave.min(x.max(y).next_up());
            if enter > leave {
                return false;
            }
        }
    }
    true
}

fn raw_box([lo, hi]: [[f32; 3]; 2]) -> Aabb {
    Aabb::new(lo.into(), hi.into())
}

fn search_bounds(s: &Acoustic, placed: bool) -> Option<Aabb> {
    if placed {
        return Some(raw_box(s.bounds));
    }
    let pose = s.pose.matrix().expect("validated binding pose").as_dmat4();
    let m = DMat3::from_cols(pose.x_axis.truncate(), pose.y_axis.truncate(), pose.z_axis.truncate());
    if m.determinant() == 0. {
        return None;
    }
    if pose == bevy::math::DMat4::IDENTITY {
        return Some(raw_box(s.bounds));
    }
    let norm = |m: DMat3| (m.x_axis.abs() + m.y_axis.abs() + m.z_axis.abs()).max_element();
    // The cook bounds use double precision, whereas the placed vertices use a
    // float affine. Bound cancellation through its condition and translation.
    let error = 64. * f64::from(f32::EPSILON) * norm(m) * norm(m.inverse());
    if !error.is_finite() || error >= 0.125 {
        return None;
    }
    let magnitude = s.bounds.iter().flatten().map(|&x| f64::from(x.abs())).fold(1_f64, f64::max)
        + pose.w_axis.truncate().abs().max_element()
        + 1.;
    let pad = error * magnitude;
    let outward = |x: f32, lower: bool| {
        let v = if lower { ((f64::from(x) - pad) as f32).next_down() } else { ((f64::from(x) + pad) as f32).next_up() };
        v.clamp(-f32::MAX, f32::MAX)
    };
    Some(raw_box([s.bounds[0].map(|v| outward(v, true)), s.bounds[1].map(|v| outward(v, false))]))
}

impl Spatial {
    fn new(profile: &Profile) -> Self {
        let mut bindings = Vec::new();
        let mut aliases = BTreeMap::new();
        let mut bounds = Vec::new();
        let mut ids = Vec::new();
        let mut unbounded = Vec::new();
        for (node, n) in profile.nodes.iter().enumerate() {
            for (surface, a) in n.acoustic.iter().enumerate() {
                let file = profile.file(a.resource);
                let resource = *aliases.entry(&file.sha256).or_insert(a.resource);
                let bound = search_bounds(a, profile.placed_bounds);
                if let Some(box_) = bound {
                    bounds.push(box_);
                    ids.push(bindings.len());
                } else {
                    unbounded.push(bindings.len());
                }
                bindings.push(Binding { node, surface, resource, bounds: bound });
            }
        }
        let tree = Tree { tree: Bvh::from_leaves(Default::default(), &bounds), ids };
        Self { bindings, tree, unbounded }
    }
}

impl Faces {
    fn decode(data: &[u8]) -> Result<Self> {
        let faces: Self = serde_json::from_slice(data)?;
        ensure!(
            !faces.triangles.is_empty()
                && faces.vertices.iter().flatten().all(|c| c.is_finite())
                && faces.triangles.iter().flatten().all(|&i| i < faces.vertices.len()),
            "invalid acoustic geometry"
        );
        Ok(faces)
    }
    fn payload_bytes(&self) -> usize {
        self.vertices.len() * size_of::<[f32; 3]>() + self.triangles.len() * size_of::<[usize; 3]>()
    }
    fn hit(&self, a: &Acoustic, (from, to): Segment, placed_bounds: bool) -> Result<Option<f64>> {
        let vertices = self.placed_vertices(a, placed_bounds)?;
        Ok(osage_objects::first_hit(from, to, self.triangles.iter().map(|t| t.map(|i| vertices[i]))))
    }

    fn placed_vertices(&self, a: &Acoustic, placed_bounds: bool) -> Result<Vec<[f32; 3]>> {
        let pose: Mat4 = a.pose.matrix()?;
        let [lo, hi] = a.bounds.map(Vec3::from_array);
        let m = pose.as_dmat4();
        let mut vertices = Vec::with_capacity(self.vertices.len());
        for &v in &self.vertices {
            let local = Vec3::from_array(v);
            let world = pose.transform_point3(local);
            if placed_bounds {
                ensure!(
                    world.is_finite() && world.cmpge(lo).all() && world.cmple(hi).all(),
                    "acoustic geometry outside binding bounds: {}",
                    a.surface.id
                );
                vertices.push(world.to_array());
                continue;
            }
            let terms = m.x_axis.truncate().abs() * f64::from(local.x.abs())
                + m.y_axis.truncate().abs() * f64::from(local.y.abs())
                + m.z_axis.truncate().abs() * f64::from(local.z.abs())
                + m.w_axis.truncate().abs()
                + bevy::math::DVec3::ONE;
            let pad = terms * (64. * f64::from(f32::EPSILON));
            ensure!(
                world.is_finite()
                    && world.as_dvec3().cmpge(lo.as_dvec3() - pad).all()
                    && world.as_dvec3().cmple(hi.as_dvec3() + pad).all(),
                "acoustic geometry outside binding bounds: {}",
                a.surface.id
            );
            vertices.push(world.to_array());
        }
        Ok(vertices)
    }
}

pub(super) struct Series {
    pub(super) read_bytes: usize,
    pub(super) probes: Vec<Value>,
    pub(super) io: Value,
}

pub(super) fn probe_series(index: &Index, dir: &Source, rays: &[Segment]) -> Result<Series> {
    probe_series_with(index, rays, |file| dir.raw(file))
}

/// A caller that already holds an admitted payload can supply it here. Every
/// payload still passes the manifest's length and digest before deserialization.
pub(super) fn probe_series_with(
    index: &Index,
    rays: &[Segment],
    mut load: impl FnMut(&File) -> Result<Vec<u8>>,
) -> Result<Series> {
    let profile = index.object_profile.as_ref().context("district lacks an object acoustic profile")?;
    let plans: Vec<_> = rays.iter().map(|&ray| profile.candidates(ray)).collect::<Result<_>>()?;
    let spatial = profile.spatial.get_or_init(|| Spatial::new(profile));
    let mut remaining = BTreeMap::<usize, usize>::new();
    for id in plans.iter().flat_map(|p| &p.bindings) {
        *remaining.entry(spatial.bindings[*id].resource).or_default() += 1;
    }
    let mut loaded = BTreeMap::<usize, Faces>::new();
    let mut probes = Vec::new();
    let mut details = Vec::new();
    let (mut total_reads, mut total_bytes, mut resident, mut peak_bytes, mut peak_meshes) = (0, 0, 0, 0, 0);
    for (&ray, plan) in rays.iter().zip(plans) {
        let mut hits = Vec::new();
        let (mut reads, mut released) = (Vec::new(), Vec::new());
        let (mut read_bytes, mut reused) = (0, 0);
        for &id in &plan.bindings {
            let leaf = &spatial.bindings[id];
            let node = &profile.nodes[leaf.node];
            let binding = &node.acoustic[leaf.surface];
            let key = leaf.resource;
            let file = profile.file(key);
            let hit = {
                let count = loaded.len();
                let mesh = match loaded.entry(key) {
                    Entry::Occupied(e) => {
                        reused += 1;
                        e.into_mut()
                    }
                    Entry::Vacant(e) => {
                        let data = load(file)?;
                        file.check(&data)?;
                        let mesh = Faces::decode(&data).with_context(|| format!("decoding {}", file.file))?;
                        resident += mesh.payload_bytes();
                        peak_bytes = peak_bytes.max(resident);
                        peak_meshes = peak_meshes.max(count + 1);
                        read_bytes += data.len();
                        reads.push(key);
                        e.insert(mesh)
                    }
                };
                mesh.hit(binding, ray, profile.placed_bounds)?
            };
            if let Some(distance) = hit {
                let s = &binding.surface;
                hits.push(json!({"distance_m":distance,"id":s.id,"owner":node.node,"source_path":index.path(Some(node.node)),
                    "owner_id":node.id,"source_mesh":s.mesh,"source":s.source,"material":s.material,"coefficients":profile.acoustic_materials[&s.material]}));
            }
            let count = remaining.get_mut(&key).expect("counted binding use");
            *count -= 1;
            if *count == 0 {
                resident -= loaded.remove(&key).expect("last decoded reference").payload_bytes();
                released.push(key);
            }
        }
        hits.sort_by(|a, b| a["distance_m"].as_f64().unwrap().total_cmp(&b["distance_m"].as_f64().unwrap()));
        total_reads += reads.len();
        total_bytes += read_bytes;
        let mut probe = json!({"object":profile.id,"from":ray.0,"to":ray.1,"mesh_reads":reads.len(),"read_bytes":read_bytes,"hits":hits});
        if let Some(graph) = profile.rooms.get().and_then(Option::as_ref) {
            probe["roomPath"] = serde_json::to_value(graph.query(ray.0, ray.1))?;
        }
        probes.push(probe);
        details.push(json!({"box_tests":plan.box_tests,"candidate_bindings":plan.bindings.len(),"candidate_resources":plan.resources,
            "read_resources":reads,"read_bytes":read_bytes,"binding_reuses":reused,"released_resources":released,
            "resident_meshes_after":loaded.len(),"resident_payload_bytes_after":resident}));
    }
    Ok(Series {
        read_bytes: total_bytes,
        probes,
        io: json!({"bindings":spatial.bindings.len(),"loose_bindings":spatial.unbounded.len(),"mesh_reads":total_reads,"read_bytes":total_bytes,
        "peak_meshes":peak_meshes,"peak_decoded_payload_bytes":peak_bytes,"resident_meshes_after":loaded.len(),"resident_payload_bytes_after":resident,"rays":details}),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_segment_slabs_accept_boundary_and_exclude_disjoint_axis_intervals() {
        let lo = [2., 0., 4.];
        let hi = [4., 0., 6.];
        let hits = [
            ([2., -1., 4.], [2., 0., 4.]), // corner at the ending plane
            ([3., 0., 5.], [6., 0., 5.]),  // starts inside a zero-thickness box
            ([2., -1e-14, 5.], [4., 1e-14, 5.]),
            ([3., 4., 5.], [3., -4., 5.]),
        ];
        for (a, b) in hits {
            assert!(segment_box((a, b), lo, hi) && segment_box((b, a), lo, hi));
        }
        for ray in [
            ([0., 2., 0.], [8., -2., 4.]),
            ([3., 0.5, 5.], [3., 0.01, 5.]),
            ([4_f32.next_up(), 1., 5.], [4_f32.next_up(), -1., 5.]),
        ] {
            assert!(!segment_box(ray, lo, hi));
        }
        let far = 1e8_f32;
        assert!(segment_box(([far, 2., far], [far, -2., far]), [far, 0., far], [far, 0., far]));
    }

    #[test]
    fn a_decoded_slot_belongs_to_its_query_and_index() {
        fn scene(name: &str, height: f32) -> (Index, Vec<u8>) {
            let data = serde_json::to_vec(
                &json!({"vertices":[[0.,height,0.],[1.,height,0.],[0.,height,1.]],"triangles":[[0,1,2]]}),
            )
            .unwrap();
            let digest = crate::bench::sha(&data);
            let profile: Profile = serde_json::from_value(json!({
                "id":name,"nodes":[{"node":1,"id":name,"acoustic":[{
                    "id":"surface","mesh":0,"source":"authored","material":"wall","resource":0,
                    "bounds":[[0.,height,0.],[1.,height,1.]],"pose":{}}]}],
                "acoustic_materials":{"wall":{}},
                "acoustic_meshes":[{"file":format!("{digest}.acoustic"),"sha256":digest,"bytes":data.len()}]
            }))
            .unwrap();
            let index =
                Index { source_parents: vec![None, Some(0)], object_profile: Some(profile), ..Default::default() };
            index.object_profile.as_ref().unwrap().validate(&index).unwrap();
            (index, data)
        }
        let (a, first) = scene("first", 0.);
        let (b, second) = scene("second", 1.);
        let ray = ([0.25, 2., 0.25], [0.25, -2., 0.25]);
        for (index, data, distance, owner) in
            [(&a, &first, 2., "first"), (&b, &second, 1., "second"), (&a, &first, 2., "first")]
        {
            let mut reads = 0;
            let got = probe_series_with(index, &[ray, ray], |_| {
                reads += 1;
                Ok(data.clone())
            })
            .unwrap();
            assert_eq!(reads, 1);
            assert_eq!(got.probes[1]["mesh_reads"], 0);
            assert_eq!(got.probes[0]["hits"][0]["distance_m"], distance);
            assert_eq!(got.probes[0]["hits"][0]["owner_id"], owner);
            assert_eq!(got.io["resident_meshes_after"], 0);
        }
        assert!(probe_series_with(&a, &[ray], |_| Ok(second.clone())).is_err());
    }
}
