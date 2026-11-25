// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

mod mesh;
use super::{AcousticBinding, AcousticMesh, ObjectContext, Primitive, Sector, StaticMesh};
use anyhow::{Context, Result, ensure};
use mesh::{Frame, Mesh, Source, equal_world_mesh};
use osage_objects::TriangleRay;
use rapier3d::parry::partitioning::Bvh;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};

enum Binding<'a> {
    Visual(usize),
    Collision { owner: usize, id: &'a str, material: &'a str },
    Sound(&'a AcousticBinding),
}
impl Binding<'_> {
    fn row(&self, ctx: &ObjectContext) -> (usize, Value) {
        let profile = &ctx.profile;
        match self {
            Self::Visual(owner) => {
                (0, json!({"id":profile.nodes[*owner].geometry.id,"nodePath":profile.node_path(*owner)}))
            }
            Self::Collision { owner, id, material } => {
                (1, json!({"id":id,"nodePath":profile.node_path(*owner),"material":material}))
            }
            Self::Sound(s) => (
                2,
                json!({"id":s.id,"nodePath":profile.node_path(s.owner),"source":s.source,
                "material":s.material,"coefficients":profile.header.acoustic_materials[&s.material]}),
            ),
        }
    }
}

#[derive(Clone, Copy, Serialize)]
struct Intersection {
    source_triangle: usize,
    distance_m: f64,
    point_m: [f64; 3],
}
struct Placement {
    mesh: usize,
    frame: Frame,
    bindings: Vec<usize>,
}

pub(super) struct Index<'a> {
    ctx: &'a ObjectContext,
    tree: Bvh,
    bounded: Vec<usize>,
    unbounded: Vec<usize>,
    meshes: Vec<Mesh<'a>>,
    placements: Vec<Placement>,
    bindings: Vec<Binding<'a>>,
}
impl<'a> Index<'a> {
    fn add_placement(&mut self, mesh: usize, frame: Frame, binding: usize) -> usize {
        let index = self.placements.len();
        self.placements.push(Placement { mesh, frame, bindings: vec![binding] });
        index
    }
    fn add_world(&mut self, m: &'a StaticMesh, visual: Option<usize>, binding: usize) -> Result<usize> {
        if let Some(slot) = visual {
            let p = &mut self.placements[slot];
            if self.meshes[p.mesh].source.matches(p.frame.matrix(), m) {
                p.bindings.push(binding);
                return Ok(slot);
            }
        }
        let id = self.meshes.len();
        self.meshes.push(Mesh::new(Source::World(m))?);
        Ok(self.add_placement(id, Frame::World, binding))
    }
    pub(super) fn new(scene: &'a Sector, ctx: &'a ObjectContext) -> Result<Self> {
        let mut index = Self {
            ctx,
            tree: Bvh::default(),
            bounded: Vec::new(),
            unbounded: Vec::new(),
            meshes: Vec::new(),
            placements: Vec::new(),
            bindings: Vec::new(),
        };
        let mut shared = HashMap::new();
        let mut visuals = HashMap::new();
        for (inst, &owner) in scene.instances.iter().zip(&ctx.visual_nodes) {
            let mesh = match shared.entry(inst.mesh) {
                std::collections::hash_map::Entry::Occupied(entry) => *entry.get(),
                std::collections::hash_map::Entry::Vacant(entry) => {
                    let id = index.meshes.len();
                    index.meshes.push(Mesh::new(Source::visual(&scene.meshes[inst.mesh]))?);
                    entry.insert(id);
                    id
                }
            };
            let binding = index.bindings.len();
            index.bindings.push(Binding::Visual(owner));
            visuals.insert(owner, index.add_placement(mesh, Frame::visual(inst.transform), binding));
        }
        let mut physics = HashMap::new();
        let mut physical_sources = HashMap::new();
        for m in &scene.colliders {
            let (owner, slot) = ctx.physical_bindings[&m.id];
            let surface = &ctx.profile.nodes[owner].geometry.collision[slot];
            let binding = index.bindings.len();
            index.bindings.push(Binding::Collision { owner, id: &m.id, material: &surface.material });
            let p = index.add_world(m, visuals.get(&owner).copied(), binding)?;
            physics.insert(m.id.as_str(), p);
            physical_sources.insert((owner, surface.mesh), (m, p));
        }
        for s in &ctx.sound.acoustics {
            let binding = index.bindings.len();
            index.bindings.push(Binding::Sound(s));
            match &s.mesh {
                AcousticMesh::Proxy(id) => {
                    let p = physics.get(id.as_str()).context("missing physical surface for acoustic query")?;
                    index.placements[*p].bindings.push(binding);
                }
                AcousticMesh::Owned(m) => {
                    let source = ctx.profile.nodes[s.owner].geometry.acoustic.iter().find(|surface| surface.id == s.id);
                    if let Some((physical, p)) =
                        source.and_then(|surface| physical_sources.get(&(s.owner, surface.mesh)))
                        && equal_world_mesh(physical, m)
                    {
                        index.placements[*p].bindings.push(binding);
                    } else {
                        index.add_world(m, visuals.get(&s.owner).copied(), binding)?;
                    }
                }
            }
        }
        ensure!(u32::try_from(index.placements.len()).is_ok(), "too many object placements for the query index");
        let mut boxes = Vec::new();
        for (id, p) in index.placements.iter().enumerate() {
            let local = &index.meshes[p.mesh].tree;
            if local.is_empty() {
                continue;
            }
            let root = local.root_aabb();
            if let Some(bounds) = p.frame.bounds(root.mins.to_array(), root.maxs.to_array()) {
                boxes.push(bounds);
                index.bounded.push(id);
            } else {
                index.unbounded.push(id);
            }
        }
        index.tree = Bvh::from_leaves(Default::default(), &boxes);
        Ok(index)
    }
    pub(super) fn query(&self, start: [f32; 3], end: [f32; 3]) -> Result<Value> {
        let segment = TriangleRay::new(start, end).context("invalid probe segment")?;
        let mut results = BTreeMap::<usize, Vec<Intersection>>::new();
        let mut tests = 0;
        let bounded = self
            .tree
            .leaves(|n| segment.intersects_bounds(n.mins().to_array(), n.maxs().to_array()))
            .map(|i| self.bounded[i as usize]);
        for id in bounded.chain(self.unbounded.iter().copied()) {
            let p = &self.placements[id];
            let mesh = &self.meshes[p.mesh];
            for triangle in mesh.tree.leaves(|n| {
                p.frame
                    .bounds(n.mins().to_array(), n.maxs().to_array())
                    .is_none_or(|b| segment.intersects_bounds(b.mins.to_array(), b.maxs.to_array()))
            }) {
                let ordinal = triangle as usize;
                tests += 1;
                if let Some(distance_m) = segment.hit(p.frame.points(mesh.source.face(ordinal))) {
                    let hit = Intersection { source_triangle: ordinal, distance_m, point_m: segment.point(distance_m) };
                    for &slot in &p.bindings {
                        results.entry(slot).or_default().push(hit);
                    }
                }
            }
        }
        let mut channels: [Vec<Value>; 3] = Default::default();
        for (slot, mut hits) in results {
            hits.sort_by(|a, b| a.distance_m.total_cmp(&b.distance_m).then(a.source_triangle.cmp(&b.source_triangle)));
            let (channel, mut row) = self.bindings[slot].row(self.ctx);
            row["distanceM"] = json!(hits[0].distance_m);
            row["intersections"] = json!(hits);
            channels[channel].push(row);
        }
        for channel in &mut channels {
            channel.sort_by(|a, b| {
                a["distanceM"]
                    .as_f64()
                    .unwrap()
                    .total_cmp(&b["distanceM"].as_f64().unwrap())
                    .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
            });
        }
        let [visual, collision, acoustic] = channels;
        Ok(
            json!({"from":start,"to":end,"visual":visual,"collision":collision,"acoustic":acoustic,"testedTriangles":tests}),
        )
    }
}

#[cfg(all(test, target_os = "linux", target_env = "gnu", target_pointer_width = "64"))]
mod measurement;
