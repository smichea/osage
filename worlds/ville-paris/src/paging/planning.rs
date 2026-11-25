// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Actor-only broad phase used to schedule detailed collision reads.
pub(super) mod moving;
use bevy::math::{DVec3, Vec3};
use rapier3d::parry::{
    bounding_volume::{Aabb, BoundingVolume},
    partitioning::{Bvh, TraversalAction},
};
use serde::Serialize;
use std::collections::BTreeMap;

/// Static bounds of the physical payloads, independent of tile centres and visual meshes.
pub(super) struct CollisionPages {
    tree: Bvh,
    tiles: Vec<usize>,
    pub largest_bytes: usize,
}

impl CollisionPages {
    pub fn new(tiles: &[super::tiles::Tile]) -> Self {
        let mut bounds = Vec::new();
        let mut ids = Vec::new();
        let mut largest_bytes = 0;
        for (id, tile) in tiles.iter().enumerate() {
            if let Some(layer) = &tile.physics {
                let [lo, hi] = layer.bounds;
                bounds.push(Aabb::new(lo.into(), hi.into()));
                ids.push(id);
                largest_bytes = largest_bytes.max(layer.content.bytes);
            }
        }
        Self { tree: Bvh::from_leaves(Default::default(), &bounds), tiles: ids, largest_bytes }
    }

    fn visit(&self, overlaps: impl Fn(Aabb) -> bool, mut visit: impl FnMut(usize)) {
        self.tree.traverse(|node| {
            if !overlaps(node.aabb()) {
                return TraversalAction::Prune;
            }
            if let Some(slot) = node.leaf_data() {
                visit(self.tiles[slot as usize]);
            }
            TraversalAction::Continue
        });
    }

    pub fn overlapping(&self, bounds: [[f32; 3]; 2], visit: impl FnMut(usize)) {
        let query = Aabb::new(bounds[0].into(), bounds[1].into());
        self.visit(|b| b.intersects(&query), visit);
    }

    pub fn swept_capsule(&self, sweep: &osage_dynamics::streamed::UprightSweep, visit: impl FnMut(usize)) {
        self.visit(|b| !sweep.excludes_bounds([b.mins.to_array(), b.maxs.to_array()]), visit);
    }

    pub fn within(&self, point: Vec3, radius: f32, visit: impl FnMut(usize)) {
        self.visit(|b| point.distance(point.clamp(b.mins, b.maxs)) <= radius, visit);
    }

    pub fn approaching(&self, motion: &Motion, horizon: f64, visit: impl FnMut(usize)) {
        self.visit(|b| motion.entry([b.mins.to_array(), b.maxs.to_array()], horizon).is_some(), visit);
    }
}

#[derive(Clone, Copy)]
pub(super) struct Motion {
    pub center: DVec3,
    pub velocity: DVec3,
    pub gravity: f64,
    pub terminal: f64,
    /// Enclosing sphere covers every orientation the capsule can take before arrival.
    pub extent: f64,
}

impl Motion {
    fn height(&self, seconds: f64) -> f64 {
        let vy = self.velocity.y.max(-self.terminal);
        if self.gravity == 0. {
            return self.center.y + vy * seconds;
        }
        let limit_at = (vy + self.terminal) / self.gravity;
        let accelerating = seconds.min(limit_at);
        let y = self.center.y + vy * accelerating - 0.5 * self.gravity * accelerating * accelerating;
        if seconds <= limit_at { y } else { y - self.terminal * (seconds - limit_at) }
    }

    /// First time the expanded collision-page box can meet the capsule, in seconds.
    fn entry(&self, bounds: [[f32; 3]; 2], horizon: f64) -> Option<f64> {
        let low = DVec3::from(bounds[0].map(f64::from)) - DVec3::splat(self.extent);
        let high = DVec3::from(bounds[1].map(f64::from)) + DVec3::splat(self.extent);
        let (mut enter, mut exit) = (0_f64, horizon);
        for axis in [0, 2] {
            let v = self.velocity[axis];
            let p = self.center[axis];
            if v == 0. {
                if p < low[axis] || p > high[axis] {
                    return None;
                }
            } else {
                let a = (low[axis] - p) / v;
                let b = (high[axis] - p) / v;
                enter = enter.max(a.min(b));
                exit = exit.min(a.max(b));
                if enter > exit {
                    return None;
                }
            }
        }
        // Height is monotone on either side of its apex, including the terminal-speed tail.
        let apex = if self.gravity > 0. { (self.velocity.y / self.gravity).clamp(enter, exit) } else { enter };
        for (from, to) in [(enter, apex), (apex, exit)] {
            let y0 = self.height(from);
            if (low.y..=high.y).contains(&y0) {
                return Some(from);
            }
            let y1 = self.height(to);
            if y0.min(y1) > high.y || y0.max(y1) < low.y {
                continue;
            }
            let rising = y0 < low.y;
            let (mut a, mut b) = (from, to);
            // Only a horizontally overlapping candidate that crosses this vertical slab gets here.
            // Work in seconds; 16 bisections resolve a two-second forecast to about 31 microseconds.
            for _ in 0..16 {
                let m = (a + b) * 0.5;
                let y = self.height(m);
                if (rising && y < low.y) || (!rising && y > high.y) {
                    a = m;
                } else {
                    b = m;
                }
            }
            return Some(a);
        }
        None
    }
}

/// A forecast only starts content reads. Only a later solver reaction can produce a sound.
pub(super) struct Landing {
    terrain: osage_dynamics::ballistic::BallisticScene,
    owners: Vec<(String, Option<usize>)>,
}

impl Landing {
    pub fn new(index: &super::tiles::Index, support: &super::support::Guard) -> anyhow::Result<Self> {
        use osage_dynamics::StaticMesh;
        let mut meshes = Vec::new();
        let mut owners = Vec::new();
        for shape in support.catalogued_surfaces() {
            owners.push((shape.id.clone(), shape.source_node));
            meshes.push(StaticMesh {
                id: shape.id.clone(),
                material: None,
                vertices: shape.vertices.clone(),
                triangles: shape.triangles.clone(),
            });
        }
        // Legacy directories can still advertise a single horizontal surface on a page.
        // Its box can cover an opening. Reserve the prediction for loading; the solver
        // still decides whether and where this particular fall touches anything.
        if meshes.is_empty() {
            for layer in index.tiles.iter().filter_map(|t| t.physics.as_ref()) {
                let [lo, hi] = layer.bounds;
                if layer.ids.len() == 1 && (hi[1] - lo[1]).abs() <= 0.0001 {
                    let id = layer.ids[0].clone();
                    owners.push((id.clone(), layer.source_nodes.first().copied()));
                    meshes.push(StaticMesh {
                        id,
                        material: None,
                        vertices: vec![[lo[0], hi[1], lo[2]], [lo[0], hi[1], hi[2]], hi, [hi[0], hi[1], lo[2]]],
                        triangles: vec![[0, 1, 2], [0, 2, 3]],
                    });
                }
            }
        }
        Ok(Self { terrain: osage_dynamics::ballistic::BallisticScene::new(&meshes)?, owners })
    }

    pub fn predict(
        &self,
        motion: Motion,
        tick: u32,
        hz: u32,
        seconds: f64,
    ) -> anyhow::Result<Option<crate::impact::paged::Contact>> {
        let capsule = crate::physics::CAPSULE;
        self.predict_shape(motion, [f64::from(capsule.radius), f64::from(capsule.half_segment)], tick, hz, seconds)
    }

    pub fn predict_rig(
        &self,
        world: &super::solver::World,
        motions: &[Motion],
        tick: u32,
        hz: u32,
        seconds: f64,
        forecasts: &mut Vec<(usize, crate::impact::paged::Contact)>,
    ) -> anyhow::Result<()> {
        let mut earliest = u32::MAX;
        for (index, ((part, pose), &motion)) in world.parts().iter().zip(world.part_poses()).zip(motions).enumerate() {
            let axis = (bevy::math::Quat::from_array(pose.rotation_xyzw) * bevy::math::Vec3::Y).as_dvec3();
            let omega = DVec3::from(pose.angular_velocity.map(f64::from));
            // Sample the lower and upper caps and the shaft midpoint on sloping or narrow supports.
            let samples: &[f64] = if part.half_segment > 0. { &[-1., 0., 1.] } else { &[0.] };
            for (cap, &side) in samples.iter().enumerate() {
                let arm = axis * f64::from(part.half_segment) * side;
                let sample =
                    Motion { center: motion.center + arm, velocity: motion.velocity + omega.cross(arm), ..motion };
                if let Some(mut hit) = self.predict_shape(sample, [f64::from(part.radius), 0.], tick, hz, seconds)?
                    && hit.tick <= earliest
                {
                    if hit.tick < earliest {
                        forecasts.clear();
                        earliest = hit.tick;
                    }
                    hit.part = Some(index);
                    forecasts.push((index * 3 + cap, hit));
                }
            }
        }
        Ok(())
    }

    fn predict_shape(
        &self,
        motion: Motion,
        dimensions: [f64; 2],
        tick: u32,
        hz: u32,
        seconds: f64,
    ) -> anyhow::Result<Option<crate::impact::paged::Contact>> {
        use osage_dynamics::ballistic::Flight;
        if motion.gravity == 0. || self.owners.is_empty() {
            return Ok(None);
        }
        let flight = Flight {
            center: motion.center,
            velocity: motion.velocity,
            gravity: motion.gravity,
            terminal_speed: motion.terminal,
            radius: dimensions[0],
            half_segment: dimensions[1],
            duration: seconds,
        };
        let Some(hit) = self.terrain.first_impact(flight)? else {
            return Ok(None);
        };
        let normal = DVec3::from_array(hit.normal);
        let mut velocity = motion.velocity;
        velocity.y = (velocity.y - motion.gravity * hit.seconds).max(-motion.terminal);
        let speed = -velocity.dot(normal);
        if normal.y <= 0.5 || speed <= 0.5 {
            return Ok(None);
        }
        let (surface, owner) = &self.owners[hit.surface];
        Ok(Some(crate::impact::paged::Contact {
            tick: tick.saturating_add((hit.seconds * f64::from(hz)).ceil() as u32),
            point: hit.point.map(|v| v as f32),
            speed: speed as f32,
            surface: surface.clone(),
            owner: *owner,
            part: None,
            substep: None,
            voice: None,
            measured: None,
        }))
    }
}

#[derive(Clone, Copy, Serialize)]
pub(super) struct Forecast {
    pub entry_ticks: f32,
    pub service_ticks: u32,
    pub horizon_ticks: u32,
}

impl Forecast {
    pub fn horizon(service_ticks: u32) -> u32 {
        // One eighth of estimated service for variation, plus two ticks for admission and publication.
        service_ticks.saturating_add(service_ticks.div_ceil(8)).saturating_add(2)
    }

    pub fn new(motion: &Motion, bounds: [[f32; 3]; 2], hz: u32, service_ticks: u32) -> Option<Self> {
        let horizon_ticks = Self::horizon(service_ticks);
        let seconds = motion.entry(bounds, f64::from(horizon_ticks) / f64::from(hz))?;
        Some(Self { entry_ticks: (seconds * f64::from(hz)) as f32, service_ticks, horizon_ticks })
    }
    pub fn priority(self) -> f32 {
        -1. + self.entry_ticks / self.horizon_ticks as f32
    }
}

#[derive(Serialize)]
pub(super) struct Request {
    pub tick: u32,
    pub tile: [i32; 2],
    #[serde(flatten)]
    pub prediction: Forecast,
}

#[derive(Serialize)]
pub(super) struct ServiceSample {
    pub tick: u32,
    pub tile: [i32; 2],
    pub elapsed_ticks: u32,
    pub estimate_ticks: u32,
}

#[derive(Default)]
pub(super) struct Plan {
    pub pages: BTreeMap<usize, Forecast>,
    pub requests: Vec<Request>,
    pub samples: Vec<ServiceSample>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_both_horizontal_axes_and_the_rise_and_fall_with_terminal_velocity() {
        let body =
            Motion { center: DVec3::ZERO, velocity: DVec3::new(4., 20., 0.), gravity: 10., terminal: 10., extent: 0. };
        let band = |x0, x1, y0, y1| [[x0, y0, -1.], [x1, y1, 1.]];
        let up = body.entry(band(0., 8., 15., 16.), 10.).unwrap();
        let down = body.entry(band(11., 16., 14., 15.), 10.).unwrap();
        assert!((up - 1.).abs() < 0.0001 && (down - 3.).abs() < 0.0001);
        assert!((body.entry(band(20., 40., -15., -5.), 10.).unwrap() - 5.).abs() < 0.0001);
        assert!(body.entry(band(25., 30., 15., 16.), 10.).is_none());
        assert!(body.entry([[0., 0., 1.], [100., 100., 4.]], 10.).is_none());
    }

    #[test]
    fn an_immediate_stop_or_turn_retracts_the_original_path() {
        let mut body =
            Motion { center: DVec3::ZERO, velocity: DVec3::X * 60., gravity: 0., terminal: 55., extent: 0.9 };
        let east = [[70., -1., -1.], [74., 1., 1.]];
        let west = [[-74., -1., -1.], [-70., 1., 1.]];
        assert!(Forecast::new(&body, east, 60, 12).is_none());
        assert!(Forecast::new(&body, east, 60, 85).is_some());
        body.velocity = DVec3::ZERO;
        assert!(Forecast::new(&body, east, 60, 85).is_none());
        body.velocity = DVec3::NEG_X * 60.;
        assert!(Forecast::new(&body, east, 60, 85).is_none() && Forecast::new(&body, west, 60, 85).is_some());
    }
}
