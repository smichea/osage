// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::{
    models::{Affine, Draw, Page},
    reader::Lane,
    tiles::Index,
};
use anyhow::{Context, Result, ensure};
use bevy::{
    math::DVec3,
    prelude::{Mat4, Vec3},
};
use osage_objects::{DetailView, VisualDetail, VisualLevel, VisualLod, maximum_stretch};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
pub struct Placement {
    owner: usize,
    bounds: [[f32; 3]; 2],
    pose: Affine,
    levels: Vec<Level>,
}

#[derive(Deserialize)]
struct Level {
    level: VisualLod,
    variant: usize,
    error_m: f64,
}

struct Owner {
    tile: usize,
    pose: Mat4,
    geometry: VisualDetail,
    history: usize,
    desired: Option<usize>,
    active: Option<usize>,
}

pub struct Frame {
    pub tick: u32,
    pub actor: Vec3,
    pub view: DetailView,
    pub viewport: [u32; 2],
}

struct Tile {
    ids: Vec<usize>,
    lo: DVec3,
    hi: DVec3,
    close: Option<bool>,
    has_desired: bool,
    resources_changed: bool,
    resolve_changed: bool,
    fallback_changed: bool,
    available: bool,
}

impl Default for Tile {
    fn default() -> Self {
        Self {
            ids: vec![],
            lo: DVec3::splat(f64::INFINITY),
            hi: DVec3::splat(f64::NEG_INFINITY),
            close: None,
            has_desired: false,
            resources_changed: true,
            resolve_changed: false,
            fallback_changed: false,
            available: false,
        }
    }
}

#[derive(PartialEq)]
struct Camera {
    eye: DVec3,
    forward: DVec3,
    near: f64,
    focal_pixels: f64,
    viewport: [u32; 2],
}

struct Frustum {
    planes: [(DVec3, f64); 5],
    count: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum Coverage {
    Outside,
    Boundary,
    Inside,
}

impl Frustum {
    fn new(view: DetailView, viewport: [u32; 2]) -> Self {
        let tan_y = f64::from(viewport[1]) / (2. * view.focal_pixels);
        let tan_x = tan_y * f64::from(viewport[0]) / f64::from(viewport[1]);
        let right = view.forward.cross(DVec3::Y).normalize_or_zero();
        let up = right.cross(view.forward);
        Self {
            planes: [
                (view.forward, view.near),
                (view.forward * tan_x + right, 0.),
                (view.forward * tan_x - right, 0.),
                (view.forward * tan_y + up, 0.),
                (view.forward * tan_y - up, 0.),
            ],
            // Looking exactly along world up has no horizontal camera basis in the ordinary selector.
            count: if right == DVec3::ZERO { 1 } else { 5 },
        }
    }

    fn visible(&self, delta: DVec3, extents: DVec3) -> bool {
        self.planes[..self.count].iter().all(|&(n, offset)| delta.dot(n) + extents.dot(n.abs()) >= offset)
    }

    fn coverage(&self, view: DetailView, tile: &Tile) -> Coverage {
        let center = (tile.lo + tile.hi) * 0.5;
        let extents = (tile.hi - tile.lo) * 0.5;
        let delta = center - view.eye;
        let scale = center.abs() + extents + view.eye.abs() + DVec3::ONE;
        let mut entire = true;
        for &(n, offset) in &self.planes[..self.count] {
            let abs = n.abs();
            let distance = delta.dot(n) - offset;
            let radius = extents.dot(abs);
            // Avoid certifying a side at a rounded world-space or frustum boundary.
            let rounding = (scale.dot(abs) + offset.abs()) * (32. * f64::EPSILON);
            if distance + radius < -rounding {
                return Coverage::Outside;
            }
            entire &= distance - radius > rounding;
        }
        if entire { Coverage::Inside } else { Coverage::Boundary }
    }
}

#[derive(Default, Serialize)]
pub(super) struct Work {
    stationary_tiles: u64,
    outside_tiles: u64,
    boundary_tiles: u64,
    inside_tiles: u64,
    owner_bounds: u64,
    projected_owners: u64,
    resource_rebuilds: u64,
}

#[derive(Default, Serialize)]
pub(super) struct PublicationWork {
    resolved_cells: u64,
    resolved_owners: u64,
    fine_pages: u64,
    fine_slots: u64,
    new_pages: u64,
    new_page_slots: u64,
    dependency_cell_links: usize,
}

#[derive(Default)]
pub struct Choices {
    owners: BTreeMap<usize, Owner>,
    tiles: BTreeMap<usize, Tile>,
    camera: Option<Camera>,
    preload_all: Option<bool>,
    pub(super) work: Work,
    pub(super) publication: PublicationWork,
    full_scan: bool,
    resource_cells: BTreeMap<usize, Vec<usize>>,
    dependencies: BTreeMap<usize, BTreeSet<usize>>,
    events: Vec<Value>,
    requests: Vec<Value>,
}

impl Choices {
    pub fn new(index: &Index, full_scan: bool) -> Result<Self> {
        let mut owners = BTreeMap::new();
        let mut tiles: BTreeMap<_, Tile> = BTreeMap::new();
        let mut dependencies = BTreeMap::new();
        for (t, tile) in index.tiles.iter().enumerate() {
            ensure!(!tile.visual.adaptive_only || !tile.visual.lods.is_empty(), "adaptive-only tile lacks levels");
            for placement in &tile.visual.lods {
                let Placement { owner, bounds, pose, levels } = placement;
                let [lo, hi] = bounds.map(Vec3::from_array);
                ensure!(
                    index.demand()
                        && *owner < index.source_parents.len()
                        && lo.is_finite()
                        && hi.is_finite()
                        && lo.cmple(hi).all(),
                    "invalid LOD owner or bounds"
                );
                ensure!(
                    levels.len() >= 2 && levels[0].level == VisualLod::High && levels[0].error_m == 0.,
                    "missing primary LOD"
                );
                let pose = pose.matrix()?;
                let (center, half_extents) = VisualDetail::world_bounds(lo, hi, pose);
                let mut choices = Vec::new();
                let mut names = BTreeSet::new();
                for level in levels {
                    let variant = index.model_variants.get(level.variant).context("unknown LOD model")?;
                    let triangles = index.model_resources[variant.resource].triangles;
                    ensure!(
                        names.insert(level.level) && level.error_m.is_finite() && level.error_m >= 0.,
                        "invalid visual level"
                    );
                    dependencies.entry(level.variant).or_insert_with(|| {
                        let mut wanted = BTreeSet::from([variant.resource]);
                        for &mat in &variant.materials {
                            if let Some(tex) = &index.material_palette[mat].pbr.texture {
                                wanted.insert(index.model_resources.len() + index.textures[tex.index].source);
                            }
                        }
                        wanted
                    });
                    choices.push(VisualLevel {
                        level: level.level,
                        mesh: level.variant,
                        triangles,
                        error_m: level.error_m * maximum_stretch(pose),
                    });
                }
                ensure!(
                    choices.iter().skip(1).all(|l| l.triangles < choices[0].triangles),
                    "alternate LOD must reduce triangles"
                );
                let geometry = VisualDetail { center, half_extents, levels: choices };
                ensure!(
                    owners
                        .insert(*owner, Owner { tile: t, pose, geometry, history: 0, desired: None, active: None })
                        .is_none(),
                    "LOD owner occurs in multiple cells"
                );
                let tile = tiles.entry(t).or_default();
                tile.ids.push(*owner);
                tile.lo = tile.lo.min(center - half_extents);
                tile.hi = tile.hi.max(center + half_extents);
            }
        }
        let mut resource_cells: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for (&cell, tile) in &tiles {
            let resources: BTreeSet<_> = tile
                .ids
                .iter()
                .flat_map(|id| &owners[id].geometry.levels)
                .flat_map(|level| &dependencies[&level.mesh])
                .copied()
                .collect();
            for id in resources {
                resource_cells.entry(id).or_default().push(cell);
            }
        }
        let links = resource_cells.values().map(Vec::len).sum();
        Ok(Self {
            owners,
            tiles,
            resource_cells,
            publication: PublicationWork { dependency_cell_links: links, ..Default::default() },
            full_scan,
            dependencies,
            ..Self::default()
        })
    }

    pub fn is_empty(&self) -> bool {
        self.owners.is_empty()
    }

    pub fn is_adaptive(&self, owner: usize) -> bool {
        self.owners.contains_key(&owner)
    }

    pub fn validate_page(&self, tile: usize, page: &Page, pure: bool) -> Result<()> {
        if let Some(group) = self.tiles.get(&tile) {
            let ids = &group.ids;
            ensure!(!pure || ids.len() == page.draws.len(), "unexpected fixed draw in adaptive-only tile");
            let mut found = BTreeSet::new();
            for (&id, draw) in page.owners.iter().zip(&page.draws) {
                if let Some(owner) = self.owners.get(&id) {
                    ensure!(
                        owner.tile == tile
                            && found.insert(id)
                            && matches!(draw,Draw::Shared(i,mat) if *i==owner.geometry.levels[0].mesh && *mat==owner.pose),
                        "LOD metadata differs from source placement"
                    );
                }
            }
            ensure!(found.len() == ids.len(), "missing LOD placement");
        }
        Ok(())
    }

    pub fn request(&mut self, index: &Index, lane: &mut Lane, frame: Frame, options: &super::Options) {
        let Frame { tick, actor, view, viewport } = frame;
        let (radius, all) = (options.visual_radius, options.all);
        let camera =
            Camera { eye: view.eye, forward: view.forward, near: view.near, focal_pixels: view.focal_pixels, viewport };
        let moved = self.camera.as_ref() != Some(&camera);
        self.camera = Some(camera);
        let mode_changed = self.preload_all.replace(all) != Some(all);
        let frustum = Frustum::new(view, viewport);

        for (&tile, group) in &mut self.tiles {
            let layer = &index.tiles[tile].visual;
            let range = radius + if lane.resident.contains(&tile) { index.edge_m * 0.5 } else { 0. };
            let close = all || layer.distance(actor).min(layer.distance(view.eye.as_vec3())) <= range;
            let previous_range = group.close.replace(close);
            if moved || previous_range != Some(close) {
                let coverage = if close { frustum.coverage(view, group) } else { Coverage::Outside };
                match coverage {
                    Coverage::Outside => self.work.outside_tiles += 1,
                    Coverage::Inside => self.work.inside_tiles += 1,
                    Coverage::Boundary => self.work.boundary_tiles += 1,
                }
                if coverage != Coverage::Outside || group.has_desired {
                    group.has_desired = false;
                    for id in &group.ids {
                        let owner = self.owners.get_mut(id).expect("validated LOD owner");
                        let geometry = &owner.geometry;
                        let visible = match coverage {
                            Coverage::Outside => false,
                            Coverage::Inside => true,
                            Coverage::Boundary => {
                                self.work.owner_bounds += 1;
                                frustum.visible(geometry.center - view.eye, geometry.half_extents)
                            }
                        };
                        let desired = visible.then(|| {
                            self.work.projected_owners += 1;
                            owner.history = geometry.choose(owner.history, view);
                            owner.history
                        });
                        if owner.desired != desired {
                            // A waiting owner can start or stop using the parent even
                            // when no verified fine model is ready in either state.
                            group.fallback_changed |=
                                owner.active.is_none() && owner.desired.is_some() != desired.is_some();
                            group.resolve_changed = true;
                            owner.desired = desired;
                            group.resources_changed = true;
                            self.requests
                                .push(json!({"tick":tick,"owner":id,"level":desired.map(|i|geometry.levels[i].level)}));
                        }
                        group.has_desired |= visible;
                    }
                }
            } else {
                self.work.stationary_tiles += 1;
            }

            if !all && !group.has_desired && layer.adaptive_only {
                lane.offscreen_tiles.insert(tile);
            } else {
                lane.offscreen_tiles.remove(&tile);
            }
            // This lane retains its dependency sets between ticks. Active variants may
            // advance even with a stationary camera, when their replacement bytes arrive.
            if mode_changed || (!all && group.resources_changed) {
                self.work.resource_rebuilds += 1;
                let wanted = lane.adaptive_resources.entry(tile).or_default();
                wanted.clear();
                let mut variants = BTreeSet::new();
                for id in &group.ids {
                    let owner = &self.owners[id];
                    let levels = &owner.geometry.levels;
                    if all {
                        variants.extend(levels.iter().map(|l| l.mesh));
                    } else if let Some(desired) = owner.desired {
                        variants.insert(levels[desired].mesh);
                        if let Some(active) = owner.active {
                            variants.insert(levels[active].mesh);
                        }
                    }
                }
                for variant in variants {
                    wanted.extend(&self.dependencies[&variant]);
                }
            }
            group.resources_changed = false;
        }
    }

    pub fn publish(
        &mut self,
        tick: u32,
        lane: &Lane,
        fine: &mut BTreeMap<usize, Page>,
        index: &Index,
        resources: &[usize],
    ) {
        for id in resources {
            if let Some(cells) = self.resource_cells.get(id) {
                for cell in cells {
                    self.tiles.get_mut(cell).expect("indexed LOD cell").resolve_changed = true;
                }
            }
        }
        let mut events = Vec::new();
        for (&cell, group) in &mut self.tiles {
            let available = lane.resident.contains(&cell) || lane.waiting.contains(&cell);
            if !self.full_scan && !group.resolve_changed && group.available == available {
                continue;
            }
            group.available = available;
            group.resolve_changed = false;
            let mut redraw = std::mem::take(&mut group.fallback_changed) || self.full_scan;
            self.publication.resolved_cells += 1;
            self.publication.resolved_owners += group.ids.len() as u64;
            for &id in &group.ids {
                let owner = self.owners.get_mut(&id).expect("indexed LOD owner");
                let prior = owner.active;
                owner.active = owner.desired.and_then(|wanted| {
                    if !available {
                        return None;
                    }
                    let ready =
                        |i: usize| self.dependencies[&owner.geometry.levels[i].mesh].is_subset(&lane.model_resident);
                    if ready(wanted) { Some(wanted) } else { prior.filter(|&i| ready(i)) }
                });
                if owner.active != prior {
                    redraw = true;
                    group.resources_changed = true;
                    let value = owner.active.map(|i| &owner.geometry.levels[i]);
                    events.push((id,json!({"tick":tick,"owner":id,"level":value.map(|l|l.level),"variant":value.map(|l|l.mesh),"triangles":value.map_or(0,|l|l.triangles)})));
                }
            }
            if redraw && let Some(page) = fine.get_mut(&cell) {
                self.publication.fine_pages += 1;
                self.publication.fine_slots += page.draws.len() as u64;
                Self::apply(&self.owners, page, index);
            }
        }
        // Input ownership order remains stable when several cells finish on the same tick.
        events.sort_unstable_by_key(|(id, _)| *id);
        self.events.extend(events.into_iter().map(|(_, e)| e));
    }

    pub fn ready(&self, tile: usize, only_adaptive: bool) -> bool {
        // A pending owner cannot hold back fixed draws or another verified owner.
        if !only_adaptive {
            return true;
        }
        let owners = &self.tiles[&tile].ids;
        owners.iter().any(|id| self.owners[id].active.is_some())
            || owners.iter().all(|id| self.owners[id].desired.is_none())
    }

    pub fn activate(&mut self, tile: usize, page: &mut Page, index: &Index) {
        if self.tiles.contains_key(&tile) {
            self.publication.new_pages += 1;
            self.publication.new_page_slots += page.draws.len() as u64;
            Self::apply(&self.owners, page, index);
        }
    }

    fn apply(owners: &BTreeMap<usize, Owner>, page: &mut Page, index: &Index) {
        for (&owner, draw) in page.owners.iter().zip(&mut page.draws) {
            let Some(o) = owners.get(&owner) else {
                continue;
            };
            let needs_parent = o.desired.is_some() && o.active.is_none();
            let fallback_changed =
                if needs_parent { page.fallback.insert(owner) } else { page.fallback.remove(&owner) };
            page.revision += u64::from(fallback_changed);
            let next = o.active.map(|i| o.geometry.levels[i].mesh);
            let before = match draw {
                Draw::Shared(i, _) => Some(*i),
                Draw::Hidden => None,
                Draw::Local(_) => unreachable!(),
            };
            if next != before {
                let triangles = |i: usize| index.model_resources[index.model_variants[i].resource].triangles;
                page.triangles = page.triangles - before.map_or(0, triangles) + next.map_or(0, triangles);
                *draw = next.map_or(Draw::Hidden, |v| Draw::Shared(v, o.pose));
                page.revision += 1;
            }
        }
    }
    pub fn report(&self) -> Value {
        let owners = self
            .owners
            .iter()
            .map(|(&id, o)| {
                let info = |n: Option<usize>| {
                    n.map(|i| {
                        let l = &o.geometry.levels[i];
                        json!({"level":l.level,"variant":l.mesh,"triangles":l.triangles})
                    })
                };
                json!({"owner":id,"tile":o.tile,"desired":info(o.desired),"published":info(o.active)})
            })
            .collect::<Vec<_>>();
        json!({"owners":owners,"events":self.events,"requests":self.requests})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tile_certified_inside_or_outside_agrees_with_individual_camera_tests() {
        let viewport = [320, 180];
        for origin in [DVec3::ZERO, DVec3::new(1e9, -2e8, 4e7)] {
            for direction in [-DVec3::Z, DVec3::new(0.5, -0.4, -0.7), -DVec3::Y] {
                let view = DetailView::new(origin, direction, std::f64::consts::FRAC_PI_3, 180, 0.1).unwrap();
                let frustum = Frustum::new(view, viewport);
                let mut seen = [false; 3];
                for depth in [-4., 0.1, 3., 25.] {
                    for x in -6..=6 {
                        let c = origin + view.forward * depth + DVec3::X * f64::from(x) * 6.;
                        let half = DVec3::new(2., 1., 1.);
                        let tile = Tile { lo: c - half, hi: c + half, ..Tile::default() };
                        let result = frustum.coverage(view, &tile);
                        seen[match result {
                            Coverage::Outside => 0,
                            Coverage::Boundary => 1,
                            Coverage::Inside => 2,
                        }] = true;
                        if result == Coverage::Boundary {
                            continue;
                        };
                        for dx in [-1., 1.] {
                            for dy in [-1., 1.] {
                                for dz in [-1., 1.] {
                                    let p = c + half * DVec3::new(dx, dy, dz) * 0.6;
                                    assert_eq!(frustum.visible(p - origin, half * 0.4), result == Coverage::Inside);
                                }
                            }
                        }
                    }
                }
                assert_eq!(seen, [true; 3]);
                let point = origin + view.forward * view.near;
                assert!(
                    frustum.coverage(view, &Tile { lo: point, hi: point, ..Tile::default() }) == Coverage::Boundary
                );
            }
        }
    }
}
