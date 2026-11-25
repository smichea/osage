// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

mod rays;

use super::tiles::{Layer, Shape, faces::FaceOrder};
use crate::{
    people::Model,
    physics::CAPSULE,
    replay::{Replay, Route},
};
use anyhow::{Context, Result, ensure};
use osage_dynamics::{
    Contact, ContactImpulse, DynamicBox, ImpulseAttribution, Lift, Part, Pose as BodyPose, RigStart, Settings,
    mesh_contacts, streamed::TerrainSimulation,
};
use rapier3d::prelude::*;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Clone, Serialize)]
pub struct Hit {
    pub tick: u32,
    pub support: String,
    pub tile: Option<[i32; 2]>,
    pub geometry: &'static str,
    pub source_node: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_triangle: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub substep: Option<u8>,
    pub point: [f32; 3],
    pub normal: [f32; 3],
    pub separation_m: f32,
    pub vertical_speed_before_m_s: f32,
    pub vertical_speed_after_m_s: f32,
}

pub(super) fn same_shape(a: &Shape, b: &Shape) -> bool {
    let materials = |s: &Shape| s.material.map(|m| (m.friction.to_bits(), m.restitution.to_bits()));
    a.source_node == b.source_node
        && a.id == b.id
        && materials(a) == materials(b)
        && a.triangles == b.triangles
        && a.vertices.len() == b.vertices.len()
        && a.vertices.iter().zip(&b.vertices).all(|(u, v)| u.map(f32::to_bits) == v.map(f32::to_bits))
}

struct Owner {
    material: Option<osage_objects::CollisionMaterial>,
    faces: FaceOrder,
    handle: ColliderHandle,
    surface: usize,
    name: String,
    tile: Option<[i32; 2]>,
    geometry: &'static str,
    node: Option<usize>,
}

struct PreviousLanding {
    part: usize,
    surface: String,
    owner: Option<usize>,
    bounds: Aabb,
    minimum_distance: f32,
}

#[derive(Clone, Copy)]
enum MovingSupport {
    Bonnet(DynamicBox),
    Lift(Lift),
}

impl MovingSupport {
    fn identity(self) -> (&'static str, &'static str) {
        match self {
            Self::Bonnet(_) => ("bonnet", "dynamic_box"),
            Self::Lift(_) => ("lift", "kinematic_lift"),
        }
    }
}

pub struct World {
    sim: TerrainSimulation,
    parts: Vec<Part>,
    previous: Vec<BodyPose>,
    previous_boxes: Vec<BodyPose>,
    moving: Option<MovingSupport>,
    impulses: Vec<ContactImpulse>,
    contacts: Vec<Contact>,
    ids: HashMap<(Option<usize>, String), usize>,
    surfaces: Vec<(String, Option<usize>)>,
    grounded: Vec<bool>,
    articulated: bool,
    dt: f32,
    gravity: Vector,
    owners: HashMap<usize, Owner>,
    supported: bool,
    previous_landing: Option<PreviousLanding>,
    pub duplicate_layer_contact_ticks: usize,
}

impl World {
    pub fn new_with_model(replay: &Replay, route: &Route, model: &Model) -> Result<Self> {
        let articulated = !matches!(model, Model::Capsule);
        ensure!(articulated || route.ragdoll_activation.is_none(), "ragdoll activation requires an articulated actor");
        let parts = model.parts().unwrap_or_else(|| vec![CAPSULE.part()]);
        let mut origin = route.start_m;
        if !articulated {
            origin[1] += CAPSULE.half_segment + CAPSULE.radius;
        }
        let settings = Settings {
            tick_hz: replay.tick_hz,
            gravity: replay.gravity_m_s2,
            terminal_speed: replay.terminal_m_s,
            capsule: CAPSULE,
        };
        let mut sim = TerrainSimulation::with_boxes(
            settings,
            &parts,
            RigStart {
                origin_m: origin,
                velocity_m_s: route.velocity_m_s,
                animation: route.ragdoll_activation.as_ref(),
            },
            route.bonnet.as_slice(),
        )?;
        let moving = if let Some(lift) = route.lift {
            sim.add_lift(lift)?;
            if let Some(hz) = route.lift_contact_hz {
                sim.plan_lift_approach(&parts, f64::from(route.ticks) / f64::from(replay.tick_hz), hz)?;
                if let Some(travel) = route.lift_response_travel_m {
                    sim.sample_lift_response(&parts, travel)?;
                }
            }
            Some(MovingSupport::Lift(lift))
        } else {
            route.bonnet.map(MovingSupport::Bonnet)
        };
        let previous = sim.state().part_poses.clone();
        let count = parts.len();
        let previous_boxes = sim.state().box_poses.clone();
        let mut owners = HashMap::new();
        let mut surfaces = Vec::new();
        if let Some(support) = moving {
            let (name, geometry) = support.identity();
            let h = sim.box_collider(0);
            owners.insert(
                h.into_raw_parts().0 as usize,
                Owner {
                    material: Some(osage_objects::CollisionMaterial {
                        friction: match support {
                            MovingSupport::Bonnet(body) => body.friction,
                            MovingSupport::Lift(lift) => lift.friction,
                        },
                        restitution: 0.,
                    }),
                    faces: FaceOrder::default(),
                    handle: h,
                    surface: 0,
                    name: name.into(),
                    tile: None,
                    geometry,
                    node: None,
                },
            );
            surfaces.push((name.into(), None));
        }
        Ok(Self {
            sim,
            parts,
            previous,
            previous_boxes,
            moving,
            articulated,
            dt: 1. / replay.tick_hz as f32,
            gravity: Vector::NEG_Y * replay.gravity_m_s2,
            impulses: Vec::new(),
            contacts: Vec::new(),
            ids: HashMap::new(),
            surfaces,
            grounded: vec![false; count],
            owners,
            supported: false,
            previous_landing: None,
            duplicate_layer_contact_ticks: 0,
        })
    }

    pub fn parts(&self) -> &[Part] {
        &self.parts
    }
    pub fn part_poses(&self) -> &[BodyPose] {
        &self.sim.state().part_poses
    }
    pub fn prior_poses(&self) -> &[BodyPose] {
        &self.previous
    }
    pub fn box_poses(&self) -> &[BodyPose] {
        &self.sim.state().box_poses
    }
    pub fn prior_box_poses(&self) -> &[BodyPose] {
        &self.previous_boxes
    }
    pub fn box_index(&self, stable_surface: usize) -> Option<usize> {
        (self.moving.is_some() && stable_surface == 0).then_some(0)
    }
    pub fn lift_approach(&self) -> Option<&osage_dynamics::lift_flight::LiftApproach> {
        self.sim.state().lift_approach()
    }
    pub fn swept_impact_offset_s(&self) -> Option<f32> {
        self.sim.state().swept_impact_offset_s
    }
    pub fn set_static_bounds(&mut self, bounds: &[[[f32; 3]; 2]]) -> Result<()> {
        self.sim.set_static_bounds(bounds)
    }
    pub fn box_motion(&self, replay: &Replay) -> Option<super::planning::Motion> {
        use bevy::math::DVec3;
        let shape = match self.moving? {
            MovingSupport::Bonnet(b) => b,
            MovingSupport::Lift(lift) => {
                // The page planner accepts ballistics and constant velocity. A stationary sphere
                // enclosing the full lift stroke also covers its reversals at any request horizon.
                let travel = DVec3::Y * (0.5 * f64::from(lift.travel_m));
                return Some(super::planning::Motion {
                    center: DVec3::from(lift.center_m.map(f64::from)) + travel,
                    velocity: DVec3::ZERO,
                    gravity: 0.,
                    terminal: f64::INFINITY,
                    extent: (DVec3::from(lift.half_extents_m.map(f64::from)) + travel).length(),
                });
            }
        };
        let pose = self.box_poses()[0];
        Some(super::planning::Motion {
            center: bevy::math::DVec3::from(pose.center.map(f64::from)),
            velocity: bevy::math::DVec3::from(pose.linear_velocity.map(f64::from)),
            gravity: f64::from(replay.gravity_m_s2),
            terminal: f64::INFINITY,
            extent: f64::from(Vector::from_array(shape.half_extents_m).length()),
        })
    }

    fn box_sweep(&self, ticks: u32, margin: f32) -> Option<[Vector; 2]> {
        let b = match self.moving? {
            MovingSupport::Bonnet(b) => b,
            MovingSupport::Lift(lift) => {
                let [bottom, top] = self.sim.lift_height_range(ticks)?;
                let [x, _, z] = lift.center_m;
                let half = Vector::from_array(lift.half_extents_m) + Vector::splat(margin);
                let low = Vector::new(x, bottom, z) - half;
                let high = Vector::new(x, top, z) + half;
                return Some([low.to_array().map(f32::next_down).into(), high.to_array().map(f32::next_up).into()]);
            }
        };
        let dt = ticks as f32 * self.dt;
        let pose = self.box_poses()[0];
        let half = Vector::from_array(b.half_extents_m);
        let rotation = bevy::prelude::Quat::from_array(pose.rotation_xyzw);
        let axes = (rotation * Vector::X).abs() * half.x
            + (rotation * Vector::Y).abs() * half.y
            + (rotation * Vector::Z).abs() * half.z;
        let radius = half.length();
        let angle = Vector::from_array(pose.angular_velocity).length() * dt;
        let span = (axes + Vector::splat(radius * angle)).min(Vector::splat(radius)) + Vector::splat(margin);
        let center = Vector::from_array(pose.center);
        let motion = Vector::from_array(pose.linear_velocity) * dt;
        let gravity = self.gravity * dt * dt;
        Some([
            center + motion.min(Vector::ZERO) + gravity.min(Vector::ZERO) - span,
            center + motion.max(Vector::ZERO) + gravity.max(Vector::ZERO) + span,
        ])
    }
    pub fn box_bounds(&self) -> Option<[[f32; 3]; 2]> {
        self.box_sweep(1, 0.001).map(|a| a.map(|v| v.to_array()))
    }
    pub fn contacts(&self) -> &[Contact] {
        &self.contacts
    }
    pub fn impulses(&self) -> &[ContactImpulse] {
        &self.impulses
    }
    pub fn substeps(&self) -> u8 {
        self.sim.state().substeps_per_tick()
    }
    pub fn surface_count(&self) -> usize {
        self.surfaces.len()
    }
    pub fn surface(&self, id: usize) -> Option<(&str, Option<usize>)> {
        self.surfaces.get(id).map(|(name, node)| (name.as_str(), *node))
    }

    pub fn foot(&self) -> [f32; 3] {
        self.parts
            .iter()
            .zip(self.part_poses())
            .map(|(part, pose)| {
                let axis = bevy::prelude::Quat::from_array(pose.rotation_xyzw) * Vector::Y;
                let mut low = Vector::from_array(pose.center);
                // The historical capsule reports the vertical projection of its centre.
                if self.articulated {
                    low -= axis * (axis.y.signum() * part.half_segment);
                } else {
                    low.y -= axis.y.abs() * part.half_segment;
                }
                low.y -= part.radius;
                low
            })
            .min_by(|a, b| a.y.total_cmp(&b.y))
            .expect("validated nonempty actor")
            .to_array()
    }

    fn motion_for(&self, i: usize, supported: bool, replay: &Replay) -> super::planning::Motion {
        let pose = self.part_poses()[i];
        let part = &self.parts[i];
        super::planning::Motion {
            center: bevy::math::DVec3::from(pose.center.map(f64::from)),
            velocity: bevy::math::DVec3::from(pose.linear_velocity.map(f64::from)),
            gravity: if supported { 0. } else { f64::from(replay.gravity_m_s2) },
            terminal: f64::from(replay.terminal_m_s),
            extent: f64::from(part.half_segment + part.radius),
        }
    }
    pub fn motion(&self, replay: &Replay) -> super::planning::Motion {
        self.motion_for(0, self.supported, replay)
    }
    pub fn part_motions(&self, replay: &Replay, motions: &mut Vec<super::planning::Motion>) {
        motions.clear();
        motions.extend(self.grounded.iter().enumerate().map(|(i, &grounded)| self.motion_for(i, grounded, replay)));
    }

    pub fn sweep_bounds(&self) -> [[f32; 3]; 2] {
        let dt = 2. * self.dt;
        let carry_rise = self.lift_carry_rise(2);
        self.combined_bounds(
            self.parts
                .iter()
                .zip(self.part_poses())
                .map(|(part, pose)| {
                    let p = Vector::from_array(pose.center);
                    let q = p + (Vector::from_array(pose.linear_velocity) + self.gravity * dt) * dt;
                    let span = Vector::splat(part.half_segment + part.radius + 1.);
                    let mut upper = p.max(q);
                    upper.y = upper.y.max(p.y + carry_rise);
                    [p.min(q) - span, upper + span]
                })
                .chain(self.box_sweep(2, 1.)),
        )
    }

    fn lift_carry_rise(&self, ticks: u8) -> f32 {
        let Some(MovingSupport::Lift(lift)) = self.moving else { return 0. };
        let [lo, hi] = self.box_sweep(u32::from(ticks), 0.001).expect("registered lift");
        let top_min = lo.y + 2. * lift.half_extents_m[1];
        let supported_on_lift =
            self.contacts.iter().any(|c| c.surface == 0 && c.normal[1] > 0.5 && c.substep == self.substeps());
        let can_reach_top = || {
            self.part_sweeps(f32::from(ticks) * self.dt, 0.)
                .any(|[a, b]| a.cmple(hi).all() && b.cmpge(lo).all() && b.y >= top_min)
        };
        if supported_on_lift || can_reach_top() {
            self.sim.lift_carry_rise(ticks).expect("registered lift")
        } else {
            0.
        }
    }

    fn part_sweeps(&self, dt: f32, carry_rise: f32) -> impl Iterator<Item = [Vector; 2]> + '_ {
        self.parts.iter().zip(self.part_poses()).map(move |(part, pose)| {
            let axis = (bevy::prelude::Quat::from_array(pose.rotation_xyzw) * Vector::Y).abs();
            // Rotation of a unit vector changes its projection by at most the angle swept.
            let angle = Vector::from_array(pose.angular_velocity).length() * dt;
            let span =
                (axis + Vector::splat(angle)).min(Vector::ONE) * part.half_segment + Vector::splat(part.radius + 0.001);
            let motion = Vector::from_array(pose.linear_velocity) * dt;
            let accel = self.gravity * dt * dt;
            let p = Vector::from_array(pose.center);
            let mut upper = motion.max(Vector::ZERO);
            upper.y = upper.y.max(carry_rise);
            [p + motion.min(Vector::ZERO) + accel.min(Vector::ZERO) - span, p + upper + accel.max(Vector::ZERO) + span]
        })
    }
    fn combined_bounds(&self, bounds: impl Iterator<Item = [Vector; 2]>) -> [[f32; 3]; 2] {
        let [lo, hi] = bounds.reduce(|[a, b], [p, q]| [a.min(p), b.max(q)]).expect("validated nonempty actor");
        [lo.to_array(), hi.to_array()]
    }
    pub fn part_bounds(&self, out: &mut Vec<[[f32; 3]; 2]>) {
        out.clear();
        out.extend(self.part_sweeps(self.dt, self.lift_carry_rise(1)).map(|box_| box_.map(|v| v.to_array())));
    }
    pub(super) fn upright_sweep(&self) -> Option<osage_dynamics::streamed::UprightSweep> {
        (!self.articulated && self.moving.is_none()).then(|| self.sim.upright_sweep()).flatten()
    }

    pub(super) fn confirms_upright_sweep(&self) -> bool {
        self.sim.confirms_upright_sweep()
    }
    pub fn insert(
        &mut self,
        data: &[u8],
        layer: &Layer,
        tile: [i32; 2],
        geometry: &'static str,
    ) -> Result<Vec<ColliderHandle>> {
        self.insert_shapes(&Self::read_shapes(data, layer)?, tile, geometry)
    }

    pub(super) fn read_shapes(data: &[u8], layer: &Layer) -> Result<Vec<Shape>> {
        let mut shapes = layer.decode_collision(data)?;
        for (slot, s) in shapes.iter_mut().enumerate() {
            s.faces = layer.face_order(slot, s.triangles.len())?;
        }
        for (shape, &material) in shapes.iter_mut().zip(&layer.materials) {
            shape.material = Some(material);
        }
        ensure!(
            shapes.iter().map(|s| &s.id).eq(layer.ids.iter())
                && (layer.source_nodes.is_empty()
                    || shapes.iter().map(|s| s.source_node).eq(layer.source_nodes.iter().copied().map(Some)))
                && shapes.iter().try_fold(0usize, |count, s| count.checked_add(s.triangles.len()))
                    == Some(layer.triangles),
            "collision directory mismatch"
        );
        Ok(shapes)
    }

    /// Keep an existing native manifold only when its owning slot, shape, ordered
    /// triangle/vertex buffers and surface coefficients still describe the full collider.
    pub(super) fn promote_support<'a>(
        &mut self,
        shapes: &[Shape],
        tile: [i32; 2],
        previous: impl Iterator<Item = (usize, &'a Shape, ColliderHandle)>,
    ) -> Result<(Vec<ColliderHandle>, usize)> {
        let mut keep = vec![None; shapes.len()];
        let mut remove = Vec::new();
        for (slot, certified, handle) in previous {
            let shape = shapes.get(slot).context("certified slot missing from the full page")?;
            let owner = self.owners.get(&(handle.into_raw_parts().0 as usize)).context("missing live support")?;
            ensure!(
                owner.handle == handle
                    && owner.tile == Some(tile)
                    && owner.geometry == "certified_support"
                    && owner.name == certified.id
                    && owner.node == certified.source_node,
                "certified collider owner changed"
            );
            if same_shape(certified, shape) {
                keep[slot] = Some(handle);
            } else {
                remove.push(handle);
            }
        }
        let reused = keep.iter().flatten().count();
        let handles = self.install_shapes(shapes, tile, "full", &keep)?;
        self.remove(remove);
        Ok((handles, reused))
    }

    pub fn insert_shapes(
        &mut self,
        shapes: &[Shape],
        tile: [i32; 2],
        geometry: &'static str,
    ) -> Result<Vec<ColliderHandle>> {
        self.install_shapes(shapes, tile, geometry, &[])
    }

    fn install_shapes(
        &mut self,
        shapes: &[Shape],
        tile: [i32; 2],
        geometry: &'static str,
        keep: &[Option<ColliderHandle>],
    ) -> Result<Vec<ColliderHandle>> {
        if let Some(support) = self.moving {
            let (name, _) = support.identity();
            ensure!(shapes.iter().all(|s| s.id != name), "static surface uses the moving {name} id");
        }
        // A bad shape leaves every old handle, owner and contact untouched.
        let retained = |index: usize| keep.get(index).copied().flatten();
        let mut built = Vec::new();
        for (index, s) in shapes.iter().enumerate() {
            if retained(index).is_some() {
                continue;
            }
            ensure!(
                !s.triangles.is_empty()
                    && s.vertices.iter().flatten().all(|n| n.is_finite())
                    && s.triangles.iter().flatten().all(|&i| (i as usize) < s.vertices.len()),
                "invalid collision geometry"
            );
            let points = s.vertices.iter().copied().map(Vector::from_array).collect();
            let collider = mesh_contacts::collider(points, s.triangles.clone())
                .with_context(|| format!("invalid collision shape {}", s.id))?
                .friction(s.material.map_or(CAPSULE.friction, |m| m.friction).sqrt())
                .friction_combine_rule(CoefficientCombineRule::Multiply)
                .restitution(s.material.map_or(0., |m| m.restitution))
                .restitution_combine_rule(CoefficientCombineRule::Max);
            built.push(collider);
        }
        let mut fresh = built.into_iter();
        let mut handles = Vec::with_capacity(shapes.len());
        for (index, s) in shapes.iter().enumerate() {
            if let Some(handle) = retained(index) {
                let owner =
                    self.owners.get_mut(&(handle.into_raw_parts().0 as usize)).expect("validated live collider");
                owner.geometry = geometry;
                owner.faces = s.faces.clone();
                handles.push(handle);
                continue;
            }
            let collider = fresh.next().expect("new collider was built before publication");
            let surface = *self.ids.entry((s.source_node, s.id.clone())).or_insert_with(|| {
                let i = self.surfaces.len();
                self.surfaces.push((s.id.clone(), s.source_node));
                i
            });
            let handle = self.sim.insert(collider, s.source_node, &s.id, tile);
            self.owners.insert(
                handle.into_raw_parts().0 as usize,
                Owner {
                    material: s.material,
                    faces: s.faces.clone(),
                    handle,
                    surface,
                    name: s.id.clone(),
                    node: s.source_node,
                    tile: Some(tile),
                    geometry,
                },
            );
            handles.push(handle);
        }
        Ok(handles)
    }
    pub(super) fn forget_source_faces(&mut self, handles: &[ColliderHandle]) {
        for &handle in handles {
            if let Some(owner) = self.owners.get_mut(&(handle.into_raw_parts().0 as usize)) {
                owner.faces = FaceOrder::default();
            }
        }
    }

    pub fn remove(&mut self, handles: Vec<ColliderHandle>) {
        for h in handles {
            self.sim.remove(h);
            self.owners.remove(&(h.into_raw_parts().0 as usize));
        }
    }
    pub fn sound_contact_active(&self) -> bool {
        self.previous_landing.is_some()
    }

    /// Compare with all resident pieces of this authored support, including a full page
    /// replacing a certified proxy. If it has vanished, its previous AABB can still prove
    /// separation. An overlapping AABB alone cannot prove that the actor has left it.
    fn landing_distance(&self, last: &PreviousLanding) -> Option<f32> {
        let capsule = self.sim.part_collider(last.part);
        let bounds = capsule.compute_aabb();
        let box_distance =
            |other: Aabb| (bounds.mins - other.maxs).max(other.mins - bounds.maxs).max(Vector::ZERO).length();
        let mut closest = f32::INFINITY;
        for owner in self.owners.values() {
            if last.owner != owner.node || last.surface != owner.name {
                continue;
            }
            let other = self.sim.collider(owner.handle);
            let lower = box_distance(other.compute_aabb());
            let distance = if lower > 0.01 {
                lower
            } else {
                rapier3d::parry::query::distance(capsule.position(), capsule.shape(), other.position(), other.shape())
                    .ok()?
                    .distance
            };
            closest = closest.min(distance);
        }
        if closest.is_finite() {
            Some(closest)
        } else {
            let gap = box_distance(last.bounds);
            (gap > 0.).then_some(gap)
        }
    }

    fn check_sound_separation(&mut self) {
        if let Some(mut previous) = self.previous_landing.take() {
            if let Some(distance) = self.landing_distance(&previous) {
                if distance > previous.minimum_distance + 0.001 {
                    return;
                }
                previous.minimum_distance = previous.minimum_distance.min(distance);
            }
            self.previous_landing = Some(previous);
        }
    }

    pub fn step(
        &mut self,
        tick: u32,
        r: &Replay,
        observe_landing: bool,
    ) -> Result<(Option<Hit>, Option<crate::impact::paged::Contact>)> {
        if observe_landing {
            self.check_sound_separation();
        }
        self.previous.copy_from_slice(&self.sim.state().part_poses);
        self.previous_boxes.copy_from_slice(&self.sim.state().box_poses);
        self.impulses.clear();
        let previous_box = self.moving.map(|_| self.sim.box_collider(0));
        self.sim.step(observe_landing.then_some(&mut self.impulses))?;
        if let Some(old) = previous_box {
            let handle = self.sim.box_collider(0);
            if old != handle {
                let mut owner = self.owners.remove(&(old.into_raw_parts().0 as usize)).expect("registered moving box");
                owner.handle = handle;
                self.owners.insert(handle.into_raw_parts().0 as usize, owner);
            }
        }
        let state = self.sim.state();
        let mut closest: Option<&Contact> = None;
        let mut touching = Vec::new();
        let mut duplicate = false;
        self.grounded.fill(false);
        self.supported = false;
        self.contacts.clear();
        for contact in &state.contacts {
            let owner = &self.owners[&contact.surface];
            self.contacts.push(Contact {
                surface: owner.surface,
                surface_triangle: contact.surface_triangle.and_then(|i| owner.faces.source_triangle(i)),
                ..*contact
            });
            let key = (contact.substep, owner.tile, owner.node, owner.name.as_str());
            duplicate |= touching.iter().any(|&(other, layer)| other == key && layer != owner.geometry);
            if !touching.contains(&(key, owner.geometry)) {
                touching.push((key, owner.geometry));
            }
            if contact.normal[1] > 0.5 {
                let support_velocity = if owner.surface == 0 && matches!(self.moving, Some(MovingSupport::Lift(_))) {
                    state.box_poses[0].linear_velocity[1]
                } else {
                    0.
                };
                if contact.substep == state.substeps_per_tick()
                    && (state.part_poses[contact.part].linear_velocity[1] - support_velocity).abs() < 0.1
                {
                    self.grounded[contact.part] = true;
                    self.supported |= (state.part_poses[0].linear_velocity[1] - support_velocity).abs() < 0.1;
                }
                if closest.is_none_or(|prev| {
                    contact.substep < prev.substep
                        || (contact.substep == prev.substep && contact.separation < prev.separation)
                }) {
                    closest = Some(contact);
                }
            }
        }
        self.duplicate_layer_contact_ticks += usize::from(duplicate);
        let hit = closest.map(|c| {
            let owner = &self.owners[&c.surface];
            Hit {
                tick,
                support: owner.name.clone(),
                tile: owner.tile,
                geometry: owner.geometry,
                source_node: owner.node,
                source_triangle: c.surface_triangle.and_then(|i| owner.faces.source_triangle(i)),
                part: (self.articulated || self.moving.is_some()).then_some(c.part),
                part_name: (self.articulated || self.moving.is_some()).then(|| self.parts[c.part].name.clone()),
                substep: (self.articulated || self.moving.is_some()).then_some(c.substep),
                point: c.point,
                normal: c.normal,
                separation_m: c.separation,
                vertical_speed_before_m_s: self.previous[c.part].linear_velocity[1],
                vertical_speed_after_m_s: state.part_poses[c.part].linear_velocity[1],
            }
        });
        let mut landing = None;
        if observe_landing && self.previous_landing.is_none() {
            let mut strongest = 0.1;
            let mut earliest = u8::MAX;
            for rec in &self.impulses {
                let (Some(pair), Some(point), Some(normal), Some(impulse)) =
                    (rec.pair(), rec.point, rec.normal, rec.normal_impulse_ns())
                else {
                    continue;
                };
                if normal[1] <= 0.5
                    || rec.substep > earliest
                    || impulse <= 0.1
                    || rec.substep == earliest && impulse <= strongest
                {
                    continue;
                }
                let incoming = self.previous[pair.part];
                let mut velocity = Vector::from_array(incoming.linear_velocity);
                velocity.y = velocity
                    .y
                    .max((-r.terminal_m_s + r.gravity_m_s2 * self.dt / f32::from(state.substeps_per_tick())).min(0.));
                let at = Vector::from_array(point);
                let owner = &self.owners[&pair.surface];
                let mut relative = velocity
                    + Vector::from_array(incoming.angular_velocity).cross(at - Vector::from_array(incoming.center));
                if let Some(box_index) = self.box_index(owner.surface) {
                    let box_pose = self.previous_boxes[box_index];
                    relative -= Vector::from_array(box_pose.linear_velocity)
                        + Vector::from_array(box_pose.angular_velocity).cross(at - Vector::from_array(box_pose.center));
                }
                let closing = -relative.dot(normal.into());
                if closing <= 0.5 {
                    continue;
                }
                earliest = rec.substep;
                strongest = impulse;
                landing = Some((
                    crate::impact::paged::Contact {
                        tick,
                        point,
                        speed: closing,
                        surface: owner.name.clone(),
                        owner: owner.node,
                        voice: None,
                        measured: None,
                        part: self.articulated.then_some(pair.part),
                        substep: self
                            .articulated
                            .then_some((u32::from(rec.substep), u32::from(state.substeps_per_tick()))),
                    },
                    self.sim.collider(owner.handle).compute_aabb(),
                    pair.part,
                ));
            }
        }
        // Geometry keeps the tile and the actual collider handle. Audio and external observers
        // group solver records on one authored surface across tiles and certified replacements.
        for rec in &mut self.impulses {
            let pairs = match &mut rec.attribution {
                ImpulseAttribution::Pair(pair) => std::slice::from_mut(pair),
                ImpulseAttribution::Unresolved(pairs) => pairs.as_mut_slice(),
            };
            for pair in pairs {
                pair.surface = self.owners[&pair.surface].surface;
            }
        }
        let landing = landing.map(|(contact, bounds, part)| {
            self.previous_landing = Some(PreviousLanding {
                surface: contact.surface.clone(),
                owner: contact.owner,
                bounds,
                part,
                minimum_distance: f32::INFINITY,
            });
            contact
        });
        if observe_landing {
            self.check_sound_separation();
        }
        Ok((hit, landing))
    }
}

#[cfg(test)]
mod lift_tests {
    use super::*;

    #[test]
    fn accelerating_deck_reserves_the_riders_head_clearance_without_moving_a_distant_actors_bounds() {
        let input = include_bytes!("../../fixtures/lift.json");
        for aboard in [true, false] {
            let mut r = Replay::read(input, []).unwrap();
            let route = &mut r.routes[0];
            route.lift.as_mut().unwrap().period_s = 2.6;
            if !aboard {
                route.start_m[0] += 16.;
            }
            let mut world = World::new_with_model(&r, &r.routes[0], &Model::Capsule).unwrap();
            let mut bounds = Vec::new();
            let mut exceeded_unpowered = 0;
            for tick in 1..=600 {
                let free = world.part_sweeps(world.dt, 0.).next().unwrap();
                world.part_bounds(&mut bounds);
                if !aboard {
                    assert_eq!(bounds[0], free.map(|v| v.to_array()));
                }
                world.step(tick, &r, false).unwrap();
                let pose = world.part_poses()[0];
                let axis_y = (bevy::prelude::Quat::from_array(pose.rotation_xyzw) * Vector::Y).y.abs();
                let head = pose.center[1] + (axis_y * CAPSULE.half_segment + CAPSULE.radius);
                exceeded_unpowered += usize::from(head > free[1].y);
                assert!(head <= bounds[0][1][1], "tick {tick}: head {head}, bounds {:?}", bounds[0]);
                assert_eq!(world.supported, aboard, "tick {tick}");
            }
            assert_eq!(exceeded_unpowered > 0, aboard);
        }
    }
}

#[cfg(test)]
mod sound_tests {
    use super::*;

    fn drop_on(elasticity: f32, switch: bool) -> Vec<u32> {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "version":1,"tick_hz":60,"gravity_m_s2":9.81,"terminal_m_s":55.,
            "viewport":[128,128],"vertical_fov_deg":45,
            "routes":[{"id":"bounce","ticks":360,"start_m":[0.,2.,0.],"velocity_m_s":[0.,-2.,0.],
                "expected_support":"surface","camera":[{"tick":0,"eye":[1.,3.,0.],"aim":[0.,0.,0.]},
                                                      {"tick":360,"eye":[1.,3.,0.],"aim":[0.,0.,0.]}]}]
        }))
        .unwrap();
        let replay = Replay::read(&bytes, ["surface"]).unwrap();
        let mut world = World::new_with_model(&replay, &replay.routes[0], &Model::Capsule).unwrap();
        let shape = Shape {
            faces: Default::default(),
            material: Some(osage_objects::CollisionMaterial { friction: 0.5, restitution: elasticity }),
            id: "surface".to_owned(),
            source_node: Some(7),
            vertices: vec![[-8., 0., -8.], [-8., 0., 8.], [8., 0., 8.], [8., 0., -8.]],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
        };
        let mut handles = world.insert_shapes(std::slice::from_ref(&shape), [0, 0], "proxy").unwrap();
        let mut events = Vec::new();
        let mut clear_before = false;
        for tick in 1..=360 {
            if switch && tick == 100 {
                world.remove(std::mem::take(&mut handles));
                handles = world.insert_shapes(std::slice::from_ref(&shape), [0, 0], "full").unwrap();
            }
            clear_before |= !world.sound_contact_active();
            let (_, hit) = world.step(tick, &replay, true).unwrap();
            if let Some(hit) = hit {
                assert!(clear_before && hit.speed > 0.5);
                assert_eq!((hit.owner, hit.surface.as_str()), (Some(7), "surface"));
                assert!(hit.point[1].abs() < 0.001);
                events.push(tick);
                clear_before = false;
            }
        }
        events
    }

    #[test]
    fn a_separated_rebound_sounds_again_and_a_proxy_handoff_under_a_resting_actor_stays_quiet() {
        let elastic = drop_on(0.78, false);
        assert!(elastic.len() >= 3 && elastic.windows(2).all(|p| p[1] - p[0] > 1), "{elastic:?}");
        let rest = drop_on(0., false);
        assert_eq!(rest.len(), 1);
        assert_eq!(drop_on(0., true), rest);
    }
    #[test]
    fn resident_and_streamed_human_react_on_the_same_feet_and_native_substep() {
        use osage_dynamics::{Engine, Simulation, StaticMesh};
        let bytes = serde_json::to_vec(&serde_json::json!({
            "version":1,"tick_hz":60,"gravity_m_s2":9.81,"terminal_m_s":55.,
            "viewport":[128,128],"vertical_fov_deg":45,
            "routes":[{"id":"feet","ticks":60,"start_m":[0.,2.,0.],"velocity_m_s":[0.,-2.,0.],
                "expected_support":"slab","camera":[{"tick":0,"eye":[1.,3.,0.],"aim":[0.,0.,0.]},
                                                    {"tick":60,"eye":[1.,3.,0.],"aim":[0.,0.,0.]}]}]
        }))
        .unwrap();
        let replay = Replay::read(&bytes, ["slab"]).unwrap();
        let route = &replay.routes[0];
        let shape = Shape {
            faces: Default::default(),
            material: None,
            id: "slab".into(),
            source_node: Some(7),
            vertices: vec![[-8., 0., -8.], [-8., 0., 8.], [8., 0., 8.], [8., 0., -8.]],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
        };
        let meshes = [StaticMesh {
            material: shape.material,
            id: shape.id.clone(),
            vertices: shape.vertices.clone(),
            triangles: shape.triangles.clone(),
        }];
        let mut paged = World::new_with_model(&replay, route, &Model::Human).unwrap();
        let mut resident = Simulation::from_parts(
            Engine::Rapier,
            &meshes,
            &[],
            Settings {
                tick_hz: replay.tick_hz,
                gravity: replay.gravity_m_s2,
                terminal_speed: replay.terminal_m_s,
                capsule: CAPSULE,
            },
            route.start_m,
            route.velocity_m_s,
            paged.parts(),
        )
        .unwrap();
        assert_eq!(paged.part_poses(), &resident.part_poses);
        assert_eq!(paged.substeps(), resident.substeps_per_tick());
        let mut handles = paged.insert_shapes(std::slice::from_ref(&shape), [0, 0], "proxy").unwrap();
        let mut native_impulses = Vec::new();
        let mut first = None;
        for tick in 1..=60 {
            if tick == 44 {
                paged.remove(std::mem::take(&mut handles));
                // Occupy the old Rapier slot with a distant, homonymous object.
                let other = Shape {
                    faces: Default::default(),
                    id: shape.id.clone(),
                    source_node: Some(8),
                    material: None,
                    triangles: shape.triangles.clone(),
                    vertices: shape.vertices.iter().map(|p| [p[0] + 32., p[1], p[2]]).collect(),
                };
                paged.insert_shapes(&[other], [1, 0], "full").unwrap();
                handles = paged.insert_shapes(std::slice::from_ref(&shape), [0, 0], "full").unwrap();
            }
            resident.step_with_impulses(&mut native_impulses).unwrap();
            let (_, _) = paged.step(tick, &replay, true).unwrap();
            if tick < 44 {
                assert_eq!(paged.part_poses(), &resident.part_poses, "positions before collider replacement at {tick}");
            }
            let ids = |records: &[ContactImpulse]| -> std::collections::BTreeMap<(u8, usize), f32> {
                let mut sums = std::collections::BTreeMap::new();
                for rec in records {
                    let pair = rec.pair().unwrap();
                    assert_eq!(pair.surface, 0);
                    *sums.entry((rec.substep, pair.part)).or_default() += rec.normal_impulse_ns().unwrap();
                }
                sums
            };
            let a = ids(&native_impulses);
            let b = ids(paged.impulses());
            if first.is_none() {
                assert_eq!(a.is_empty(), b.is_empty());
                if a.is_empty() {
                    assert_eq!(paged.part_poses(), &resident.part_poses, "airborne poses at tick {tick}");
                } else {
                    let sub = a.first_key_value().unwrap().0.0;
                    let members = |map: &std::collections::BTreeMap<(u8, usize), f32>| {
                        map.keys()
                            .filter(|(s, _)| *s == sub)
                            .map(|(_, part)| paged.parts()[*part].name.as_str())
                            .collect::<std::collections::BTreeSet<_>>()
                    };
                    assert_eq!(members(&a), ["foot_l", "foot_r"].into_iter().collect());
                    assert_eq!(members(&a), members(&b));
                    for (key, x) in a.iter().filter(|((s, _), _)| *s == sub) {
                        assert!((x - b[key]).abs() < 0.001, "first impulse: {a:?}, {b:?}");
                    }
                    first = Some(tick);
                }
            }
            for c in paged.contacts() {
                assert_eq!(paged.surface(c.surface), Some(("slab", Some(7))));
            }
        }
        assert!(first.is_some_and(|t| t < 44));
        assert_eq!(paged.surface(0), Some(("slab", Some(7))));
        assert_eq!(paged.surface(1), Some(("slab", Some(8))));
    }
}

#[cfg(test)]
mod handoff_tests {
    use super::*;

    fn slab(id: &str, node: usize) -> Shape {
        Shape {
            faces: Default::default(),
            id: id.into(),
            source_node: Some(node),
            material: Some(osage_objects::CollisionMaterial { friction: 0.5, restitution: 0. }),
            vertices: vec![[0., 1., 0.], [0., 1., 1.], [1., 1., 1.], [1., 1., 0.]],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
        }
    }

    #[test]
    fn retain_only_bit_identical_owned_faces_after_a_whole_page_validates() {
        let mut replay = Replay::read(crate::replay::DEFAULT, ["road", "rail", "roof-grenelle"]).unwrap();
        replay.routes[0].start_m = [0.5, 2., 0.5];
        replay.routes[0].velocity_m_s = [0., -1., 0.];
        let mut world = World::new_with_model(&replay, &replay.routes[0], &Model::Human).unwrap();
        let shapes = [slab("same", 1), slab("same", 2)];
        let old = world.insert_shapes(&shapes, [0, 0], "certified_support").unwrap();
        let source = || shapes.iter().enumerate().map(|(i, s)| (i, s, old[i]));
        let mut invalid = shapes.to_vec();
        invalid[1].triangles[0][1] = 1000;
        assert!(world.promote_support(&invalid, [0, 0], source()).is_err());
        for &handle in &old {
            assert_eq!(world.owners[&(handle.into_raw_parts().0 as usize)].geometry, "certified_support");
        }
        let mut full = shapes.to_vec();
        full[1].triangles.push([2, 1, 0]);
        let (current, retained) = world.promote_support(&full, [0, 0], source()).unwrap();
        assert_eq!(retained, 1);
        assert_eq!(current[0], old[0]);
        assert_ne!(current[1], old[1]);
        assert_eq!(world.sim.collider(current[1]).shape().as_trimesh().unwrap().num_triangles(), 3);
        assert_eq!(world.owners[&(current[0].into_raw_parts().0 as usize)].node, Some(1));
        assert_eq!(world.owners[&(current[1].into_raw_parts().0 as usize)].node, Some(2));
        assert!(world.owners.values().all(|o| o.geometry == "full"));
        world.remove(current);
        assert!(world.owners.is_empty());
        for tick in 1..=90 {
            world.step(tick, &replay, false).unwrap();
            assert!(world.contacts().is_empty(), "retired native collider still generated a contact");
        }
        assert!(world.foot()[1] < 0.);
    }

    #[test]
    fn source_material_winding_and_single_float_changes_invalidate_reuse() {
        let replay = Replay::read(crate::replay::DEFAULT, ["road", "rail", "roof-grenelle"]).unwrap();
        let stencil = slab("repeat", 8);
        for changed in 0..5 {
            let mut world = World::new_with_model(&replay, &replay.routes[0], &Model::Capsule).unwrap();
            let handles = world.insert_shapes(std::slice::from_ref(&stencil), [1, 0], "certified_support").unwrap();
            let mut full = stencil.clone();
            match changed {
                0 => full.source_node = Some(9),
                1 => full.material.as_mut().unwrap().friction = 0.25,
                2 => full.triangles[0].swap(1, 2),
                3 => full.vertices[0][1] = f32::from_bits(1f32.to_bits() + 1),
                _ => full.id = "another".into(),
            }
            let (new, retained) =
                world.promote_support(&[full], [1, 0], std::iter::once((0, &stencil, handles[0]))).unwrap();
            assert_eq!(retained, 0);
            assert_ne!(handles, new);
            assert!(!world.owners.contains_key(&(handles[0].into_raw_parts().0 as usize)));
            world.remove(new);
            assert!(world.owners.is_empty());
        }
    }
}
