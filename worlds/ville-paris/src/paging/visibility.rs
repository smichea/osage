// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::scene::Primitive;
use bevy::{
    camera::visibility::VisibleEntities,
    ecs::entity::{EntityHashMap, EntityHashSet},
    prelude::*,
};
use osage_render::OsageCamera;
use serde_json::{Value, json};
use std::{any::TypeId, collections::BTreeMap, time::Instant};

#[derive(Clone, Copy)]
pub(super) struct Bounds {
    lo: Vec3,
    hi: Vec3,
}
impl Bounds {
    pub(super) fn from_mesh(p: &Primitive) -> Self {
        p.positions.iter().copied().map(Vec3::from_array).fold(
            Self { lo: Vec3::splat(f32::INFINITY), hi: Vec3::splat(f32::NEG_INFINITY) },
            |b, v| Self { lo: b.lo.min(v), hi: b.hi.max(v) },
        )
    }
    fn union(self, b: Self) -> Self {
        Self { lo: self.lo.min(b.lo), hi: self.hi.max(b.hi) }
    }
    pub(super) fn world(self, m: Mat4) -> Self {
        let center = m.transform_point3((self.lo + self.hi) * 0.5);
        let h = (self.hi - self.lo) * 0.5;
        let half = m.x_axis.truncate().abs() * h.x + m.y_axis.truncate().abs() * h.y + m.z_axis.truncate().abs() * h.z;
        Self { lo: center - half, hi: center + half }
    }
    pub(super) fn building(self) -> bool {
        let size = self.hi - self.lo;
        size.x >= 10. && size.z >= 10. && size.y >= 8.
    }
    fn distance2(self, eye: Vec3) -> f32 {
        eye.distance_squared(eye.clamp(self.lo, self.hi))
    }
    // All camera rays to a target inside a face's four side planes must hit its opaque triangles.
    fn faces(self, eye: Vec3, forward: Vec3) -> [Option<Face>; 3] {
        let center = (self.hi + self.lo) * 0.5;
        let half = (self.hi - self.lo) * 0.5;
        let inset = 0.03;
        std::array::from_fn(|axis| {
            let (plane, sign) = if eye[axis] < self.lo[axis] - inset {
                (self.lo[axis], 1.)
            } else if eye[axis] > self.hi[axis] + inset {
                (self.hi[axis], -1.)
            } else {
                return None;
            };
            let a = (axis + 1) % 3;
            let b = (axis + 2) % 3;
            let mut face_center = center;
            face_center[axis] = plane;
            // The whole face must be beyond the near plane, even at its closest corner.
            let nearest = (face_center - eye).dot(forward) - forward[a].abs() * half[a] - forward[b].abs() * half[b];
            if nearest <= 0.13 {
                return None;
            }
            let depth = (plane - eye[axis]) * sign;
            let slope = |k: usize| [(self.lo[k] + inset - eye[k]) / depth, (self.hi[k] - inset - eye[k]) / depth];
            Some(Face { axis, sign, depth, slope: [slope(a), slope(b)] })
        })
    }
}

struct Face {
    axis: usize,
    sign: f32,
    depth: f32,
    slope: [[f32; 2]; 2],
}
impl Face {
    // An entire cell outside any plane cannot contain an occluded child.
    fn may_cover(&self, target: Bounds, eye: Vec3) -> bool {
        let lo = target.lo - eye;
        let hi = target.hi - eye;
        let (near, far) =
            if self.sign > 0. { (lo[self.axis], hi[self.axis]) } else { (-hi[self.axis], -lo[self.axis]) };
        if far <= self.depth + 0.03 {
            return false;
        }
        let near = near.max(self.depth + 0.03);
        self.slope.iter().enumerate().all(|(i, &[low, high])| {
            let k = (self.axis + i + 1) % 3;
            hi[k] > low * if low > 0. { near } else { far } && lo[k] < high * if high > 0. { far } else { near }
        })
    }

    fn covers(&self, target: Bounds, eye: Vec3) -> bool {
        let lo = target.lo - eye;
        let hi = target.hi - eye;
        let (near, far) =
            if self.sign > 0. { (lo[self.axis], hi[self.axis]) } else { (-hi[self.axis], -lo[self.axis]) };
        if near <= self.depth + 0.03 {
            return false;
        }
        self.slope.iter().enumerate().all(|(i, &[low, high])| {
            let k = (self.axis + i + 1) % 3;
            lo[k] > low * if low > 0. { far } else { near } && hi[k] < high * if high > 0. { near } else { far }
        })
    }
}

/// Certify six genuinely opaque rectangular faces, including outward winding and a diagonal on each face.
pub(super) fn solid_box(p: &Primitive, b: Bounds) -> bool {
    if p.indices.len() != 36
        || (b.hi - b.lo).min_element() <= 0.
        || p.colors.as_ref().is_some_and(|c| c.iter().any(|v| v[3] < 1.))
    {
        return false;
    }
    let mut faces = [Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    for tri in p.indices.as_chunks::<3>().0 {
        let corners = tri.map(|i| Vec3::from_array(p.positions[i as usize]));
        let mut codes = [0u8; 3];
        for (j, q) in corners.iter().enumerate() {
            for k in 0..3 {
                if q[k] == b.hi[k] {
                    codes[j] |= 1 << k;
                } else if q[k] != b.lo[k] {
                    return false;
                }
            }
        }
        let normal = (corners[1] - corners[0]).cross(corners[2] - corners[0]);
        let Some(axis) =
            (0..3).find(|&k| normal[k] != 0. && corners[0][k] == corners[1][k] && corners[0][k] == corners[2][k])
        else {
            return false;
        };
        let high = corners[0][axis] == b.hi[axis];
        if (normal[axis] > 0.) != high {
            return false;
        }
        let mask = codes.into_iter().fold(0u8, |m, c| m | 1 << c);
        if mask.count_ones() != 3 {
            return false;
        }
        faces[axis * 2 + usize::from(high)].push(mask);
    }
    faces.iter().all(|f| {
        if f.len() != 2 {
            return false;
        }
        let shared = f[0] & f[1];
        if shared.count_ones() != 2 || (f[0] | f[1]).count_ones() != 4 {
            return false;
        }
        let a = shared.trailing_zeros();
        let b = 7 - shared.leading_zeros();
        (a ^ b).count_ones() == 2
    })
}

pub(super) fn axial(m: Mat4) -> bool {
    m.x_axis.y == 0.
        && m.x_axis.z == 0.
        && m.y_axis.x == 0.
        && m.y_axis.z == 0.
        && m.z_axis.x == 0.
        && m.z_axis.y == 0.
        && m.x_axis.x * m.y_axis.y * m.z_axis.z > 0.
}

#[derive(Clone, Copy)]
pub(super) struct Piece {
    pub entity: Entity,
    pub owner: Option<usize>,
    pub bounds: Bounds,
    pub blocker: bool,
    pub indices: usize,
}
struct Group {
    owner: Option<usize>,
    bounds: Bounds,
}
struct Placement {
    cell: usize,
    group: usize,
    indices: usize,
    blocker: Option<([usize; 2], Bounds)>,
}

struct Cell {
    bounds: Bounds,
    groups: Vec<Group>,
    entities: Vec<Entity>,
    group_offset: usize,
    blockers: usize,
    highest_blocker: Option<f32>,
}

#[derive(Resource, Default)]
pub(super) struct Scene {
    cells: Vec<Option<Cell>>,
    group_count: usize,
    tile_entity_bytes: usize,
    placements: EntityHashMap<Placement>,
    blocks: usize,
    highest_blocker: Option<f32>,
    generation: u64,
    build_ns: u64,
    changed_cells: usize,
    examined_cells: usize,
    inserted: usize,
    removed: usize,
}
impl Scene {
    pub fn update<I: Iterator<Item = Piece>>(
        &mut self,
        tile_count: usize,
        dirty: &[usize],
        full: bool,
        pieces: impl Fn(usize) -> I,
    ) {
        if dirty.is_empty() {
            return;
        }
        let started = Instant::now();
        assert!(
            self.cells.is_empty() || self.cells.len() == tile_count,
            "resident camera uses one immutable directory"
        );
        self.changed_cells = dirty.len();
        self.examined_cells = if full { tile_count } else { dirty.len() };
        self.inserted = 0;
        self.removed = 0;
        if full {
            self.removed = self.placements.len();
            self.placements.clear();
            self.cells.clear();
        }
        self.cells.resize_with(tile_count, || None);
        let mut refresh = |tile: usize| {
            if let Some(old) = self.cells[tile].take() {
                self.removed += old.entities.len();
                for entity in old.entities {
                    self.placements.remove(&entity);
                }
            }
            let mut parts = pieces(tile).peekable();
            let Some(first) = parts.peek() else { return };
            let bounds = first.bounds;
            let mut cell = Cell {
                bounds,
                groups: Vec::new(),
                entities: Vec::with_capacity(parts.size_hint().0),
                group_offset: 0,
                blockers: 0,
                highest_blocker: None,
            };
            let mut owners = BTreeMap::new();
            for (ordinal, p) in parts.enumerate() {
                cell.bounds = cell.bounds.union(p.bounds);
                // A legacy unowned placement stays an individual conservative group.
                let identity = (p.owner, p.owner.is_none().then_some(p.entity));
                let group = *owners.entry(identity).or_insert_with(|| {
                    cell.groups.push(Group { owner: p.owner, bounds: p.bounds });
                    cell.groups.len() - 1
                });
                cell.groups[group].bounds = cell.groups[group].bounds.union(p.bounds);
                let blocker = p.blocker.then(|| {
                    cell.blockers += 1;
                    cell.highest_blocker = Some(cell.highest_blocker.map_or(p.bounds.hi.y, |h| h.max(p.bounds.hi.y)));
                    // Equal-distance blockers retain the source tile and draw order after any replacement.
                    ([tile, ordinal], p.bounds)
                });
                self.placements.insert(p.entity, Placement { cell: tile, group, indices: p.indices, blocker });
                cell.entities.push(p.entity);
            }
            self.inserted += cell.entities.len();
            self.cells[tile] = Some(cell);
        };
        if full {
            (0..tile_count).for_each(refresh);
        } else {
            for &tile in dirty {
                refresh(tile);
            }
        }
        // Prefixes are dense for the per-camera owner decisions. These summaries touch no mesh or placement.
        self.group_count = 0;
        self.tile_entity_bytes = 0;
        self.blocks = 0;
        self.highest_blocker = None;
        for cell in self.cells.iter_mut().flatten() {
            cell.group_offset = self.group_count;
            self.group_count += cell.groups.len();
            self.tile_entity_bytes += cell.entities.capacity() * size_of::<Entity>();
            self.blocks += cell.blockers;
            if let Some(h) = cell.highest_blocker {
                self.highest_blocker = Some(self.highest_blocker.map_or(h, |v| v.max(h)));
            }
        }
        self.generation = self.generation.checked_add(1).expect("visibility scene generation exhausted");
        self.build_ns = started.elapsed().as_nanos() as u64;
    }
    fn cell(&self, tile: usize) -> &Cell {
        self.cells[tile].as_ref().expect("indexed entity must belong to a resident cell")
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Auto,
    On,
    Off,
}

#[derive(Resource)]
pub(super) struct Settings {
    pub mode: Mode,
    pub full_index: bool,
    pub watch: Option<usize>,
    pub withhold: Option<usize>,
}
#[derive(PartialEq)]
struct CameraState {
    scene_generation: u64,
    world_from_view: Mat4,
    clip_from_view: Mat4,
    watch: Option<usize>,
    withhold: Option<usize>,
}

/// The frustum system may reorder equal candidates. Preserve the order of its current frame.
struct Selection {
    input: Vec<Entity>,
    submitted: Vec<Entity>,
    sorted_input: Vec<Entity>,
    scratch: Vec<Entity>,
    hidden: EntityHashSet,
}
impl Selection {
    fn new(input: Vec<Entity>, submitted: &[Entity]) -> Self {
        let mut sorted_input = input.clone();
        sorted_input.sort_unstable();
        let mut hidden: EntityHashSet = input.iter().copied().collect();
        for entity in submitted {
            hidden.remove(entity);
        }
        Self { input, submitted: submitted.to_vec(), sorted_input, scratch: Vec::new(), hidden }
    }
    fn apply(&mut self, candidates: &mut Vec<Entity>) -> bool {
        if candidates.len() != self.input.len() {
            return false;
        }
        if candidates == &self.input {
            candidates.clone_from(&self.submitted);
            return true;
        }
        self.scratch.clone_from(candidates);
        self.scratch.sort_unstable();
        if self.scratch != self.sorted_input {
            return false;
        }
        candidates.retain(|entity| !self.hidden.contains(entity));
        true
    }
}
#[derive(Resource, Default)]
pub(super) struct Stats {
    pub last: Value,
    previous: Option<CameraState>,
    selection: Option<Selection>,
}

pub(super) fn cull(
    scene: Res<Scene>,
    cfg: Res<Settings>,
    mut stats: ResMut<Stats>,
    mut camera: Single<(&GlobalTransform, &Camera, &mut VisibleEntities), With<OsageCamera>>,
) {
    let start = Instant::now();
    let (transform, lens, visible) = &mut *camera;
    let eye = transform.translation();
    let forward = *transform.forward();
    let list = visible.get_mut(TypeId::of::<Mesh3d>());
    let before = list.len();
    // Retaining the frustum list is conservative. Above every certified roof the full
    // face pass is expensive and removes little geometry on a district descent.
    let enabled = match cfg.mode {
        Mode::On => true,
        Mode::Off => false,
        Mode::Auto => scene.highest_blocker.is_some_and(|roof| eye.y <= roof),
    };
    let state = (enabled && cfg.mode == Mode::Auto).then(|| CameraState {
        scene_generation: scene.generation,
        world_from_view: transform.to_matrix(),
        clip_from_view: lens.clip_from_view(),
        watch: cfg.watch,
        withhold: cfg.withhold,
    });
    let repeated = state.is_some() && stats.previous == state;
    if !repeated {
        stats.selection = None;
    }
    stats.previous = state;
    if repeated && stats.selection.as_mut().is_some_and(|selection| selection.apply(list)) {
        stats.last["reused_visibility"] = true.into();
        for field in ["cell_face_tests", "group_face_tests", "volume_tests", "tested_boxes_limit", "candidate_faces"] {
            stats.last[field] = 0.into();
        }
        stats.last["cpu_ns"] = (start.elapsed().as_nanos() as u64).into();
        return;
    }
    let remember = repeated.then(|| list.clone());
    let mut near = Vec::new();
    if enabled {
        near.extend(list.iter().filter_map(|e| {
            let placement = scene.placements.get(e)?;
            let (order, bounds) = placement.blocker?;
            if cfg.withhold.is_some() && scene.cell(placement.cell).groups[placement.group].owner == cfg.withhold {
                return None;
            }
            let faces = bounds.faces(eye, forward);
            faces.iter().any(Option::is_some).then(|| (bounds.distance2(eye), order, faces))
        }));
        let order = |a: &(f32, [usize; 2], [Option<Face>; 3]), b: &(f32, [usize; 2], [Option<Face>; 3])| {
            a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1))
        };
        if near.len() > 12 {
            near.select_nth_unstable_by(12, order);
            near.truncate(12);
        }
        near.sort_unstable_by(order);
    }
    let chosen_boxes = near.len();
    let faces: Vec<_> = near.into_iter().flat_map(|(_, _, f)| f.into_iter().flatten()).collect();
    let mut owners = 0;
    let mut whole_cells = 0;
    let mut multiple_owner_cells = 0;
    let mut forced = 0;
    let mut indices = 0;
    let mut watch_in = 0;
    let mut watch_out = 0;
    let mut cell_face_tests = 0usize;
    let mut group_face_tests = 0usize;
    let mut cells = vec![(None, 0usize); if enabled { scene.cells.len() } else { 0 }];
    let mut hidden_groups = vec![None; if enabled { scene.group_count } else { 0 }];
    if enabled || cfg.watch.is_some() || cfg.withhold.is_some() {
        // Walk the actual frustum output once. Cache each owner and cell decision on first visit.
        list.retain(|entity| {
            let Some(p) = scene.placements.get(entity) else {
                return true;
            };
            let cell = scene.cell(p.cell);
            let g = &cell.groups[p.group];
            let hidden = enabled
                && *hidden_groups[cell.group_offset + p.group].get_or_insert_with(|| {
                    let (cell_result, seen) = &mut cells[p.cell];
                    let (whole, mut possible) = *cell_result.get_or_insert_with(|| {
                        let bounds = cell.bounds;
                        let mut mask = 0_u64;
                        for (i, face) in faces.iter().enumerate() {
                            cell_face_tests += 1;
                            if face.covers(bounds, eye) {
                                whole_cells += 1;
                                return (true, 0);
                            }
                            if face.may_cover(bounds, eye) {
                                mask |= 1 << i;
                            }
                        }
                        (false, mask)
                    });
                    *seen += 1;
                    multiple_owner_cells += usize::from(whole && *seen == 2);
                    let mut hidden = whole;
                    while possible != 0 {
                        let face = possible.trailing_zeros() as usize;
                        group_face_tests += 1;
                        if faces[face].covers(g.bounds, eye) {
                            hidden = true;
                            break;
                        }
                        possible &= possible - 1;
                    }
                    owners += usize::from(hidden);
                    hidden
                });
            let withheld = cfg.withhold.is_some() && g.owner == cfg.withhold;
            let keep = !hidden && !withheld;
            if !keep {
                forced += usize::from(withheld && !hidden);
                indices += p.indices;
            }
            if cfg.watch.is_some() && g.owner == cfg.watch {
                watch_in += 1;
                watch_out += usize::from(keep);
            }
            keep
        });
    }
    if let Some(input) = remember {
        stats.selection = Some(Selection::new(input, list));
    }
    stats.last = json!({"reused_visibility":false,"frustum_candidates":before,"camera_candidates":list.len(),"rejected_camera_primitives":before-list.len(),
        "rejected_owners":owners,"rejected_whole_cells":whole_cells,"omitted_triangle_indices":indices,"certified_building_boxes":scene.blocks,
        "rejected_cells_with_multiple_owners":multiple_owner_cells,"withheld_camera_primitives":forced,"tested_boxes_limit":chosen_boxes,"candidate_faces":faces.len(),
        "volume_tests":cell_face_tests + group_face_tests,"cell_face_tests":cell_face_tests,"group_face_tests":group_face_tests,
        "automatic":(cfg.mode == Mode::Auto).then_some(enabled),"highest_certified_roof_m":scene.highest_blocker,"watched_primitives_before":watch_in,"watched_primitives_after":watch_out,"resident_index_placements":scene.placements.len(),
        "scene_generation":scene.generation,"last_changed_cells":scene.changed_cells,"last_indexed_cells":scene.examined_cells,
        "resident_cell_entity_capacity_bytes":scene.tile_entity_bytes,"last_indexed_placements":scene.inserted,"last_retired_placements":scene.removed,"last_resident_index_build_ns":scene.build_ns,"cpu_ns":start.elapsed().as_nanos() as u64});
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacing_one_cell_retires_only_its_entities_and_updates_the_highest_occluder() {
        let entity = |id| Entity::from_raw_u32(id).unwrap();
        let part = |id, owner, top, blocker| Piece {
            entity: entity(id),
            owner: Some(owner),
            bounds: Bounds { lo: Vec3::ZERO, hi: Vec3::new(10., top, 10.) },
            indices: 12,
            blocker,
        };
        let mut pages = [vec![part(1, 10, 12., true), part(2, 10, 12., false)], vec![part(3, 10, 30., true)], vec![]];
        let mut scene = Scene::default();
        scene.update(3, &[0, 1, 2], false, |i| pages[i].iter().copied());
        assert_eq!((scene.group_count, scene.blocks, scene.highest_blocker), (2, 2, Some(30.)));
        assert_eq!(scene.cell(0).groups[0].owner, scene.cell(1).groups[0].owner);
        pages[1].clear();
        scene.update(3, &[1], false, |i| pages[i].iter().copied());
        assert_eq!((scene.group_count, scene.blocks, scene.highest_blocker), (1, 1, Some(12.)));
        assert_eq!((scene.inserted, scene.removed, scene.examined_cells), (0, 1, 1));
        assert!(!scene.placements.contains_key(&entity(3)));
        assert_eq!(scene.placements[&entity(1)].blocker.unwrap().0, [0, 0]);
        pages[2].push(part(4, 22, 18., true));
        scene.update(3, &[2], false, |i| pages[i].iter().copied());
        assert_eq!(
            (scene.placements.len(), scene.group_count, scene.blocks, scene.highest_blocker),
            (3, 2, 2, Some(18.))
        );
        assert_eq!(scene.cell(2).group_offset, 1);
        let generation = scene.generation;
        scene.update(3, &[], false, |i| pages[i].iter().copied());
        assert_eq!(scene.generation, generation);
    }

    #[test]
    fn remembered_selection_preserves_order_and_requires_identical_current_entities() {
        let [a, b, c, d] = [1, 2, 3, 4].map(|i| Entity::from_raw_u32(i).unwrap());
        let mut snapshot = Selection::new(vec![a, b, c], &[a, c]);
        for (mut input, expected) in [(vec![a, b, c], vec![a, c]), (vec![c, b, a], vec![c, a])] {
            assert!(snapshot.apply(&mut input));
            assert_eq!(input, expected);
        }
        for mut input in [vec![a, c], vec![a, d, c], vec![a, a, c]] {
            let given = input.clone();
            assert!(!snapshot.apply(&mut input));
            assert_eq!(input, given);
        }
    }

    #[test]
    fn holes_reversed_triangles_and_transparent_vertices_do_not_occlude() {
        let source = crate::scene::Sector::read(crate::scene::DEFAULT).unwrap();
        let mut part = source.meshes[23][0].clone();
        let bounds = Bounds::from_mesh(&part);
        assert!(solid_box(&part, bounds));
        let original = part.indices.clone();
        part.indices.swap(0, 1);
        assert!(!solid_box(&part, bounds));
        part.indices = original;
        part.indices.copy_within(0..3, 3);
        assert!(!solid_box(&part, bounds));
        part = source.meshes[23][0].clone();
        part.colors = Some(vec![[1., 1., 1., 0.8]; part.positions.len()]);
        assert!(!solid_box(&part, bounds));
        let sloping_roof = &source.meshes[20][0];
        assert!(!solid_box(sloping_roof, Bounds::from_mesh(sloping_roof)));
    }

    #[test]
    fn silhouette_near_plane_and_interior_keep_visible_targets() {
        let house = Bounds { lo: Vec3::new(1., -1., -1.), hi: Vec3::new(3., 1., 1.) };
        let eye = Vec3::ZERO;
        let faces: Vec<_> = house.faces(eye, Vec3::X).into_iter().flatten().collect();
        assert_eq!(faces.len(), 1);
        let target = Bounds { lo: Vec3::new(6., -1., -1.), hi: Vec3::new(8., 1., 1.) };
        assert!(faces[0].covers(target, eye));
        let edge = Bounds { hi: Vec3::new(8., 6., 1.), ..target };
        assert!(!faces[0].covers(edge, eye));
        assert!(faces[0].may_cover(edge, eye));
        let beside = Bounds { lo: Vec3::new(6., 30., -1.), hi: Vec3::new(8., 31., 1.) };
        let before = Bounds { lo: Vec3::splat(-2.), hi: Vec3::splat(0.9) };
        assert!(!faces[0].may_cover(beside, eye));
        assert!(!faces[0].may_cover(before, eye));
        assert!(house.faces(Vec3::new(0.9, 0., 0.), Vec3::X).iter().all(Option::is_none));
        assert!(house.faces(Vec3::new(2., 0., 0.), Vec3::X).iter().all(Option::is_none));
        assert!(house.faces(eye, Vec3::NEG_X).iter().all(Option::is_none));
        assert!(!axial(Mat4::from_rotation_y(0.3)));
        assert!(!axial(Mat4::from_scale(Vec3::new(-1., 1., 1.))));
    }
    #[test]
    fn accepted_bounds_cover_every_perspective_ray_on_the_same_face() {
        let house = Bounds { lo: Vec3::splat(-2.), hi: Vec3::splat(2.) };
        let mut accepted = 0;
        for direction in [
            Vec3::X,
            Vec3::NEG_Y,
            Vec3::Z,
            Vec3::new(-1., 0.2, 0.3),
            Vec3::new(0.4, -1., 0.7),
            Vec3::new(0.2, 0.25, -1.),
        ] {
            let eye = direction * 10.;
            let forward = -direction.normalize();
            let faces = house.faces(eye, forward);
            for xi in -2..=2 {
                for yi in -2..=2 {
                    for zi in -2..=2 {
                        let center = -eye + Vec3::new(xi as f32, yi as f32, zi as f32) * 1.7;
                        let target = Bounds { lo: center - Vec3::splat(0.6), hi: center + Vec3::splat(0.6) };
                        for face in faces.iter().flatten().filter(|f| f.covers(target, eye)) {
                            accepted += 1;
                            for padding in [Vec3::ZERO, Vec3::new(14., 2., 8.), Vec3::splat(30.)] {
                                let enclosing = Bounds { lo: target.lo - padding, hi: target.hi + padding };
                                assert!(face.may_cover(enclosing, eye), "cell mask removed a hidden child");
                            }
                            let plane = eye[face.axis] + face.sign * face.depth;
                            for corner in 0..8 {
                                let q = Vec3::from_array(std::array::from_fn(|k| {
                                    if corner & (1 << k) == 0 { target.lo[k] } else { target.hi[k] }
                                }));
                                let t = (plane - eye[face.axis]) / (q[face.axis] - eye[face.axis]);
                                let point = eye + t * (q - eye);
                                assert!(t > 0. && t < 1. && (point - eye).dot(forward) > 0.1);
                                for k in (0..3).filter(|&axis| axis != face.axis) {
                                    assert!(point[k] > house.lo[k] && point[k] < house.hi[k]);
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(accepted > 100);
    }
}
