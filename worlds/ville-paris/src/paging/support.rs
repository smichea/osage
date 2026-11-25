// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

mod bundle;
mod clearance;
mod corners;
mod faces;
mod rays;
mod regions;
use super::solver::{World, same_shape};
use super::source::Source;
use super::tiles::{Index, Shape, SupportFile};
use crate::bench::sha;
use anyhow::{Context, Result, ensure};
use flate2::read::ZlibDecoder;
use rapier3d::prelude::ColliderHandle;
pub(super) use rays::Gap;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
};

#[derive(Deserialize)]
struct Faces(usize, Vec<[f32; 3]>, Vec<[u32; 3]>);
#[derive(Deserialize)]
struct Catalogue {
    format: String,
    directory_sha256: String,
    pages: Vec<Vec<Faces>>,
    #[serde(default)]
    residual: Option<Vec<Vec<Barrier>>>,
    #[serde(default)]
    residual_shapes: Option<Vec<Vec<usize>>>,
    #[serde(default)]
    corner_meshes: Option<corners::Meshes>,
    face_order: Option<faces::Selection>,
    #[serde(default)]
    residual_triangle_corners: Vec<[usize; 3]>,
}

#[derive(Deserialize)]
struct Barrier([f32; 3], [f32; 3], usize);

type Bounds = [[f32; 3]; 2];

struct Residual {
    bounds: Bounds,
    shape: Option<usize>,
    faces: usize,
}

#[derive(Serialize)]
struct Surface {
    source_node: usize,
    source_path: Vec<usize>,
    id: String,
}

fn overlaps(a: &Bounds, b: &Bounds) -> bool {
    (0..3).all(|axis| a[0][axis] <= b[1][axis] && a[1][axis] >= b[0][axis])
}
#[derive(Default, Serialize)]
pub struct Stats {
    pub encoded_bytes: usize,
    pub decoded_bytes: usize,
    #[serde(skip_serializing_if = "zero_bytes")]
    pub embedded_bytes: usize,
    pub catalogue_triangles: usize,
    pub complete_corner_meshes: usize,
    pub residual_boxes: usize,
    pub held_ticks: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub capsule_sweep_previews: u32,
    #[serde(skip_serializing_if = "is_zero")]
    pub capsule_sweep_clear_ticks: u32,
    pub installations: usize,
    pub full_handoffs: usize,
    pub retained_certified_colliders: usize,
    pub replaced_certified_colliders: usize,
    pub proximity_evictions: usize,
    pub peak_active_pages: usize,
    pub peak_active_triangles: usize,
    pub synchronous_legacy_bytes: usize,
}
fn zero_bytes(value: &usize) -> bool {
    *value == 0
}
fn is_zero(value: &u32) -> bool {
    *value == 0
}
#[derive(Serialize)]
pub struct Event {
    tick: u32,
    tile: [i32; 2],
    state: &'static str,
    foot_m: [f32; 3],
    #[serde(skip_serializing_if = "Vec::is_empty")]
    surfaces: Vec<Surface>,
}

#[derive(Default)]
struct CertifiedPage {
    shapes: Vec<Shape>,
    slots: Vec<usize>,
    complete: Vec<usize>,
}

pub struct Guard {
    pub mode: &'static str,
    pub stats: Stats,
    pub events: Vec<Event>,
    regions: Option<regions::Regions>,
    catalogue: Vec<CertifiedPage>,
    residual: Vec<Vec<Residual>>,
    refinements: clearance::Refinements,
    held: BTreeSet<usize>,
    active: BTreeMap<usize, (Vec<ColliderHandle>, usize)>,
    triangles: usize,
}

impl SupportFile {
    fn decode(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        let mut raw = Vec::new();
        let size = u64::try_from(self.decoded_bytes)?;
        let limit = size.checked_add(1).context("support size overflow")?;
        if self.content.file.ends_with(".zst") {
            return super::directory::decode_zstd(bytes, self.decoded_bytes);
        }
        ZlibDecoder::new(bytes).take(limit).read_to_end(&mut raw)?;
        ensure!(raw.len() == self.decoded_bytes, "support catalogue size mismatch");
        Ok(raw)
    }
}

impl Guard {
    pub fn load(dir: &Source, idx: &mut Index, enabled: bool, global: bool) -> Result<Self> {
        let mut guard = Self {
            mode: "unprotected",
            stats: Stats::default(),
            events: vec![],
            regions: None,
            catalogue: vec![],
            residual: vec![],
            refinements: Default::default(),
            held: BTreeSet::new(),
            active: BTreeMap::new(),
            triangles: 0,
        };
        let embedded_global = idx.inline_global_support.take();
        let embedded_region = idx.inline_region.take();
        if !enabled {
            return Ok(guard);
        }
        let Some(file) = &idx.support else {
            // Prior cooks have no precomputed faces. Only a threatened page is read urgently.
            if dir.paced() {
                guard.mode = "wait_for_full";
                guard.residual = idx
                    .tiles
                    .iter()
                    .map(|t| {
                        t.physics
                            .iter()
                            .map(|l| Residual { bounds: l.bounds, shape: None, faces: l.triangles })
                            .collect()
                    })
                    .collect();
            } else {
                guard.mode = "legacy_synchronous";
            }
            return Ok(guard);
        };
        guard.catalogue.resize_with(idx.tiles.len(), Default::default);
        guard.residual.resize_with(idx.tiles.len(), Vec::new);
        guard.mode = "certified_support";
        if !global && !idx.support_regions.is_empty() {
            guard.regions = Some(regions::Regions::new(idx)?);
            for (residual, tile) in guard.residual.iter_mut().zip(&idx.tiles) {
                if let Some(layer) = &tile.physics {
                    residual.push(Residual { bounds: layer.bounds, shape: None, faces: layer.triangles });
                }
            }
        } else {
            let pages: Vec<_> =
                idx.tiles.iter().enumerate().filter_map(|(i, t)| t.physics.as_ref().map(|_| i)).collect();
            let bytes = match embedded_global {
                Some(bytes) => {
                    guard.stats.embedded_bytes = bytes.len();
                    bytes
                }
                None => file.content.read(dir)?,
            };
            guard.read_catalogue(idx, &pages, file, &bytes, false)?;
        }
        if !global && let Some((id, bytes)) = embedded_region {
            guard.install_inline_region(idx, id, &bytes)?;
            guard.stats.embedded_bytes = bytes.len();
        }
        Ok(guard)
    }

    fn read_catalogue(
        &mut self,
        idx: &Index,
        pages: &[usize],
        file: &SupportFile,
        bytes: &[u8],
        complete: bool,
    ) -> Result<()> {
        let raw = file.decode(bytes)?;
        if complete {
            self.read_bundle(idx, pages, &raw)?;
            self.stats.encoded_bytes += bytes.len();
            self.stats.decoded_bytes += raw.len();
            return Ok(());
        }
        let json = if let Some(region) = raw.strip_prefix(b"ODR1") {
            let (addresses, catalogue) = region
                .split_at_checked(pages.len().checked_mul(32).context("invalid address count")?)
                .context("truncated regional addresses")?;
            idx.resolve_region(pages, addresses.as_chunks::<32>().0)?;
            catalogue
        } else {
            &raw
        };
        let cat: Catalogue = serde_json::from_slice(json)?;
        let mut binding = Vec::new();
        for &i in pages {
            let t = &idx.tiles[i];
            binding.extend(t.x.to_le_bytes());
            binding.extend(t.z.to_le_bytes());
            binding.extend(idx.physical_file(i).context("missing regional addresses")?.sha256.as_bytes());
        }
        ensure!(
            matches!(cat.format.as_str(), "ville-paris/support-faces/1" | "ville-paris/support-faces/2")
                && cat.directory_sha256 == sha(&binding),
            "support catalogue belongs to a different collision directory"
        );
        ensure!(
            (cat.format == "ville-paris/support-faces/2") == cat.residual.is_some(),
            "incomplete support catalogue"
        );
        ensure!(cat.residual.is_some() || cat.residual_shapes.is_none(), "unexpected residual shapes");
        let mut slots = cat.residual_shapes.map(Vec::into_iter);
        let mut residual = cat.residual.map(Vec::into_iter);
        let mut catalogued = cat.pages.into_iter();
        for &i in pages {
            let t = &idx.tiles[i];
            let mut shapes = Vec::new();
            let mut shape_slots = Vec::new();
            let mut barriers = Vec::new();
            if let Some(l) = &t.physics {
                let entries = catalogued.next().context("missing page in support catalogue")?;
                ensure!(entries.len() == l.ids.len(), "missing source owners in support catalogue");
                for (j, Faces(owner, vertices, triangles)) in entries.into_iter().enumerate() {
                    ensure!(Some(&owner) == l.source_nodes.get(j), "support owner differs from full geometry");
                    ensure!(
                        vertices.iter().all(|v| (0..3).all(|a| v[a].is_finite()
                            && v[a] >= l.bounds[0][a] - 0.001
                            && v[a] <= l.bounds[1][a] + 0.001)),
                        "support outside the collision page"
                    );
                    for &[a, b, c] in &triangles {
                        let point = |i: u32| {
                            vertices
                                .get(i as usize)
                                .copied()
                                .map(bevy::prelude::Vec3::from_array)
                                .context("invalid support index")
                        };
                        let a = point(a)?;
                        let normal = (point(b)? - a).cross(point(c)? - a);
                        ensure!(normal.y > normal.length() * 0.5, "support face is not load bearing");
                    }
                    self.stats.catalogue_triangles += triangles.len();
                    if !triangles.is_empty() {
                        shape_slots.push(j);
                        shapes.push(Shape {
                            faces: Default::default(),
                            material: l.materials.get(j).copied(),
                            id: l.ids[j].clone(),
                            source_node: Some(owner),
                            vertices,
                            triangles,
                        });
                    }
                }
                let certified: usize = shapes.iter().map(|s| s.triangles.len()).sum();
                ensure!(certified <= l.triangles, "support face count exceeds full page");
                if let Some(iter) = &mut residual {
                    let mut total = certified;
                    let bounds = iter.next().context("missing residual page bounds")?;
                    let shapes_for_boxes =
                        slots.as_mut().map(|s| s.next().context("missing residual page shapes")).transpose()?;
                    ensure!(
                        shapes_for_boxes.as_ref().is_none_or(|s| s.len() == bounds.len()),
                        "missing residual face shapes"
                    );
                    for (n, Barrier(lo, hi, count)) in bounds.into_iter().enumerate() {
                        ensure!(
                            count > 0
                                && (0..3).all(|a| lo[a].is_finite()
                                    && hi[a].is_finite()
                                    && lo[a] <= hi[a]
                                    && lo[a] >= l.bounds[0][a] - 0.001
                                    && hi[a] <= l.bounds[1][a] + 0.001),
                            "invalid residual bounds"
                        );
                        total = total.checked_add(count).context("face count overflow")?;
                        let shape = shapes_for_boxes.as_ref().map(|s| s[n]);
                        ensure!(shape.is_none_or(|slot| slot < l.source_nodes.len()), "unknown residual shape");
                        barriers.push(Residual { bounds: [lo, hi], shape, faces: count });
                    }
                    ensure!(total == l.triangles, "incomplete collision faces");
                } else if certified < l.triangles {
                    // Older cooks give only the page bound for its omitted geometry.
                    barriers.push(Residual { bounds: l.bounds, shape: None, faces: l.triangles - certified });
                }
            }
            self.stats.residual_boxes += barriers.len();
            self.residual[i] = barriers;
            self.catalogue[i] = CertifiedPage { shapes, slots: shape_slots, complete: Vec::new() };
        }
        ensure!(
            catalogued.next().is_none()
                && residual.is_none_or(|mut it| it.next().is_none())
                && slots.is_none_or(|mut it| it.next().is_none()),
            "surplus support page"
        );
        self.refinements.bind(&cat.residual_triangle_corners, pages, &self.residual)?;
        if let Some(selection) = cat.face_order
            && idx.collision_face_order.is_some()
        {
            selection.apply(self, idx, pages)?;
        }
        if let Some(meshes) = cat.corner_meshes {
            ensure!(
                cat.format == "ville-paris/support-faces/2",
                "complete corner meshes require per-slot residual faces"
            );
            meshes.expand(self, idx, pages)?;
        }
        self.stats.encoded_bytes += bytes.len();
        self.stats.decoded_bytes += raw.len();
        Ok(())
    }

    pub(super) fn catalogued_surfaces(&self) -> impl Iterator<Item = &Shape> {
        self.catalogue.iter().flat_map(|page| &page.shapes)
    }

    /// Bounds for the isolated moving-pair sweep must also cover terrain that has not arrived.
    pub(super) fn static_bounds(&self, index: &Index) -> Vec<Bounds> {
        let mut owners = BTreeMap::new();
        for (i, tile) in index.tiles.iter().enumerate() {
            let Some(layer) = &tile.physics else { continue };
            let mut include = |slot: usize, bounds: Bounds| {
                let key = (layer.source_nodes.get(slot).copied(), layer.ids[slot].as_str());
                let existing = owners.entry(key).or_insert(bounds);
                for axis in 0..3 {
                    existing[0][axis] = existing[0][axis].min(bounds[0][axis]);
                    existing[1][axis] = existing[1][axis].max(bounds[1][axis]);
                }
            };
            if let Some(page) = self.catalogue.get(i) {
                for (shape, &slot) in page.shapes.iter().zip(&page.slots) {
                    for &point in &shape.vertices {
                        include(slot, [point, point]);
                    }
                }
                for omitted in &self.residual[i] {
                    if let Some(slot) = omitted.shape {
                        include(slot, omitted.bounds);
                    } else {
                        for slot in 0..layer.ids.len() {
                            include(slot, omitted.bounds);
                        }
                    }
                }
            } else {
                for slot in 0..layer.ids.len() {
                    include(slot, layer.bounds);
                }
            }
        }
        owners.into_values().collect()
    }

    /// Select collision pages whose missing faces could affect the upcoming movement.
    pub(super) fn required_sweeps(&self, index: &Index, parts: &[Bounds]) -> BTreeSet<usize> {
        let mut required = BTreeSet::new();
        if self.residual.is_empty() {
            return required;
        }
        for part in parts {
            index.physical_pages().overlapping(*part, |i| {
                if !required.contains(&i)
                    && self.residual[i].iter().any(|face| self.refinements.intersects(i, face, part))
                {
                    required.insert(i);
                }
            });
        }
        required
    }

    pub(super) fn required_bounds(&self, index: &Index, body: Bounds) -> BTreeSet<usize> {
        self.required_sweeps(index, std::slice::from_ref(&body))
    }

    fn can_speculate(&self, sweep: &osage_dynamics::streamed::UprightSweep, i: usize, face: &Residual) -> bool {
        !sweep.excludes_start_bounds(face.bounds)
            && (self.refinements.invalidated.contains(&i)
                || self.refinements.triangle(i, face).is_some_and(|triangle| !sweep.excludes_speculation(triangle)))
    }

    /// A known finite face can generate a real native speculative impulse before
    /// the ordinary next-step body bounds enter its AABB. Reserve its file in time.
    pub(super) fn require_speculative(&self, index: &Index, world: &World, required: &mut BTreeSet<usize>) {
        let mut pages = self.refinements.known_pages().peekable();
        if pages.peek().is_none() {
            return;
        }
        let Some(sweep) = world.upright_sweep() else { return };
        for i in pages {
            if !required.contains(&i)
                && index.tiles[i].physics.as_ref().is_some_and(|p| !sweep.excludes_start_bounds(p.bounds))
                && self.residual[i].iter().any(|r| self.can_speculate(&sweep, i, r))
            {
                required.insert(i);
            }
        }
    }

    pub fn can_step(
        &mut self,
        tick: u32,
        index: &Index,
        required: &BTreeSet<usize>,
        full: &BTreeMap<usize, Vec<ColliderHandle>>,
        world: &World,
        parts: &[Bounds],
    ) -> bool {
        let mut missing: BTreeSet<_> = required.iter().copied().filter(|i| !full.contains_key(i)).collect();
        for &i in &self.refinements.invalidated {
            if !full.contains_key(&i)
                && self.residual[i].iter().any(|r| {
                    parts.iter().any(|p| overlaps(p, &r.bounds))
                        || world.upright_sweep().is_some_and(|s| !s.excludes_start_bounds(r.bounds))
                })
            {
                missing.insert(i);
            }
        }
        if !missing.is_empty()
            && let Some(sweep) = world.upright_sweep()
            && self.residual.iter().enumerate().all(|(i, faces)| {
                faces.is_empty()
                    || full.contains_key(&i)
                    || index.tiles[i].physics.as_ref().is_some_and(|page| sweep.excludes_bounds(page.bounds))
                    || faces.iter().all(|r| {
                        sweep.excludes_bounds(r.bounds)
                            || self.refinements.triangle(i, r).is_some_and(|t| sweep.excludes_triangle(t))
                    })
            })
        {
            self.stats.capsule_sweep_previews += 1;
            if world.confirms_upright_sweep() {
                self.stats.capsule_sweep_clear_ticks += 1;
                missing.clear();
            }
        }
        if let Some(regions) = &self.regions {
            missing.extend(regions.threats.iter().copied().filter(|i| !self.region_ready(*i) && !full.contains_key(i)));
        }
        for &i in self.held.symmetric_difference(&missing) {
            let t = &index.tiles[i];
            let layer = t.physics.as_ref().expect("a hold refers to physical geometry");
            let state = if missing.contains(&i) { "clearance_hold" } else { "clearance_ready" };
            let sweep = world.upright_sweep();
            let slots: BTreeSet<_> = self.residual[i]
                .iter()
                .filter(|r| {
                    parts.iter().any(|p| self.refinements.intersects(i, r, p))
                        || sweep.as_ref().is_some_and(|s| self.can_speculate(s, i, r))
                })
                .filter_map(|r| r.shape)
                .collect();
            let surfaces = slots
                .into_iter()
                .map(|slot| {
                    let node = layer.source_nodes[slot];
                    Surface { source_node: node, source_path: index.path(Some(node)), id: layer.ids[slot].clone() }
                })
                .collect();
            self.events.push(Event { tick, tile: [t.x, t.z], state, foot_m: world.foot(), surfaces });
        }
        self.held = missing;
        self.stats.held_ticks += u32::from(!self.held.is_empty());
        self.held.is_empty()
    }

    pub fn install_full(
        &mut self,
        tick: u32,
        idx: &Index,
        i: usize,
        bytes: &[u8],
        world: &mut World,
    ) -> Result<Vec<ColliderHandle>> {
        let t = &idx.tiles[i];
        let tile = [t.x, t.z];
        let layer = t.physics.as_ref().context("missing full physics layer")?;
        let shapes = World::read_shapes(bytes, layer)?;
        self.refinements.verify(i, &shapes)?;
        if let Some(page) = self.catalogue.get_mut(i) {
            // The complete pattern already tests every ordered vertex, face and material.
            for &j in &page.complete {
                ensure!(
                    same_shape(&page.shapes[j], &shapes[page.slots[j]]),
                    "full collision differs from certified corner mesh"
                );
            }
            let verified = page
                .slots
                .iter()
                .zip(&page.shapes)
                .enumerate()
                .filter(|(j, _)| !page.complete.contains(j))
                .try_for_each(|(_, (&slot, certified))| {
                    let full = &shapes[slot];
                    certified.verify_face_order(&full.vertices, &full.triangles)
                });
            if let Err(error) = verified {
                // The certified face numbers just disagreed with verified full geometry.
                // Revoke them on active and future proxies before this tick is solved.
                for s in &mut page.shapes {
                    s.faces = Default::default();
                }
                if let Some((handles, _)) = self.active.get(&i) {
                    world.forget_source_faces(handles);
                }
                return Err(error);
            }
        }
        self.publish_shapes(tick, i, tile, Some(&shapes), world)
    }

    pub(super) fn install_bundled(
        &mut self,
        tick: u32,
        i: usize,
        idx: &Index,
        world: &mut World,
    ) -> Result<Vec<ColliderHandle>> {
        let t = &idx.tiles[i];
        self.publish_shapes(tick, i, [t.x, t.z], None, world)
    }

    fn publish_shapes(
        &mut self,
        tick: u32,
        i: usize,
        tile: [i32; 2],
        decoded: Option<&[Shape]>,
        world: &mut World,
    ) -> Result<Vec<ColliderHandle>> {
        let shapes = decoded.unwrap_or_else(|| &self.catalogue[i].shapes);
        if self.mode == "certified_support"
            && let Some((handles, triangles)) = self.active.get(&i)
        {
            let page = &self.catalogue[i];
            ensure!(page.shapes.len() == handles.len(), "incomplete certified installation");
            let previous = page
                .slots
                .iter()
                .copied()
                .zip(&page.shapes)
                .zip(handles.iter().copied())
                .map(|((slot, shape), handle)| (slot, shape, handle));
            let (installed, reused) = world.promote_support(shapes, tile, previous)?;
            self.stats.retained_certified_colliders += reused;
            self.stats.replaced_certified_colliders += handles.len() - reused;
            self.stats.full_handoffs += 1;
            self.triangles -= *triangles;
            self.active.remove(&i);
            self.events.push(Event { tick, tile, state: "full_handoff", foot_m: world.foot(), surfaces: Vec::new() });
            return Ok(installed);
        }
        world.insert_shapes(shapes, tile, "full")
    }

    pub fn sync(
        &mut self,
        tick: u32,
        index: &Index,
        dir: &Source,
        full: &BTreeMap<usize, Vec<ColliderHandle>>,
        withheld: Option<usize>,
        world: &mut World,
    ) -> Result<()> {
        if matches!(self.mode, "unprotected" | "wait_for_full") {
            return Ok(());
        }
        let bounds = world.sweep_bounds();
        // An active page outside the new sweep still needs its retention or eviction check.
        let mut pages: BTreeSet<_> = self.active.keys().copied().collect();
        index.physical_pages().overlapping(bounds, |i| {
            pages.insert(i);
        });
        for i in pages {
            let t = &index.tiles[i];
            let l = t.physics.as_ref().expect("support belongs to a physical page");
            let margin = if self.active.contains_key(&i) { 2. } else { 0. };
            let close =
                (0..3).all(|a| bounds[0][a] <= l.bounds[1][a] + margin && bounds[1][a] >= l.bounds[0][a] - margin);
            let tile = [t.x, t.z];
            if !close || full.contains_key(&i) {
                if let Some((handles, count)) = self.active.remove(&i) {
                    world.remove(handles);
                    self.triangles -= count;
                    let state = if full.contains_key(&i) {
                        self.stats.full_handoffs += 1;
                        "full_handoff"
                    } else {
                        self.stats.proximity_evictions += 1;
                        "left_sweep"
                    };
                    self.events.push(Event { tick, tile, state, foot_m: world.foot(), surfaces: Vec::new() });
                }
            } else if !self.active.contains_key(&i) && self.region_ready(i) {
                let (handles, count) = if self.mode == "legacy_synchronous" {
                    ensure!(
                        withheld != Some(i),
                        "legacy page withheld in the capsule sweep; recook the support catalogue"
                    );
                    let bytes = l.content.read(dir)?;
                    self.stats.synchronous_legacy_bytes += bytes.len();
                    (world.insert(&bytes, l, tile, self.mode)?, l.triangles)
                } else {
                    let shapes = &self.catalogue[i].shapes;
                    let count = shapes.iter().map(|s| s.triangles.len()).sum();
                    (world.insert_shapes(shapes, tile, self.mode)?, count)
                };
                self.active.insert(i, (handles, count));
                self.triangles += count;
                self.stats.installations += 1;
                self.stats.peak_active_pages = self.stats.peak_active_pages.max(self.active.len());
                self.stats.peak_active_triangles = self.stats.peak_active_triangles.max(self.triangles);
                self.events.push(Event { tick, tile, state: "installed", foot_m: world.foot(), surfaces: Vec::new() });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paging::tiles::{Layer, Tile};

    #[test]
    fn support_storage_reads_one_sized_zstandard_frame_and_legacy_zlib() {
        let descriptor = |name: &str, decoded_bytes| SupportFile {
            content: super::super::tiles::File { file: name.into(), ..Default::default() },
            decoded_bytes,
        };
        let zlib = descriptor("catalogue.zlib", 2);
        assert_eq!(zlib.decode(&[120, 218, 139, 142, 5, 0, 1, 21, 0, 185]).unwrap(), b"[]");
        // A single-segment Zstandard frame carrying the same JSON in one raw block.
        let data = [0x28, 0xb5, 0x2f, 0xfd, 0x20, 2, 17, 0, 0, b'[', b']'];
        let zstd = descriptor("catalogue.zst", 2);
        assert_eq!(zstd.decode(&data).unwrap(), b"[]");
        assert!(zstd.decode(&data[..10]).is_err());
        assert!(zstd.decode(&[data.as_slice(), &data].concat()).is_err());
        assert!(descriptor("catalogue.zst", 1).decode(&data).is_err());
        assert!(descriptor("catalogue.zst", 3).decode(&data).is_err());
        assert!(zlib.decode(&data).is_err());
    }

    #[test]
    fn required_pages_follow_individual_sweeps_and_include_touching_faces() {
        let mut index = Index::default();
        let mut guard = Guard::load(&Source::Directory(Default::default()), &mut index, false, false).unwrap();
        for x in -8..=8 {
            let left = (x * 10) as f32;
            // The layer spans three cells, but its omitted wall sits at the far end.
            let bounds = [[left, -2., -3.], [left + 30., 4., 3.]];
            let wall = [[left + 30., -2., -3.], bounds[1]];
            let physics = (x % 3 != 0).then(|| Layer { bounds, ..Default::default() });
            guard.residual.push(if physics.is_some() {
                vec![Residual { bounds: wall, shape: None, faces: 2 }]
            } else {
                vec![]
            });
            index.tiles.push(Tile { x, z: 0, physics, visual: Default::default(), parent_first: 0, parent_count: 0 });
        }
        let a = [[-41., -3., 0.], [-40., -2., 0.]];
        let b = [[80., 4., 3.], [81., 5., 4.]];
        assert_eq!(guard.required_sweeps(&index, &[a, b, a]), BTreeSet::from([1, 13]));
        assert_eq!(guard.required_sweeps(&index, &[]), BTreeSet::new());
        for x in -80..120 {
            let p = x as f32;
            let part = [[p, 0., -1.], [p + 0.25, 1., 1.]];
            let expected = guard
                .residual
                .iter()
                .enumerate()
                .filter_map(|(i, walls)| walls.iter().any(|w| overlaps(&part, &w.bounds)).then_some(i))
                .collect();
            assert_eq!(guard.required_bounds(&index, part), expected);
        }
    }
}
