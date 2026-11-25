// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::scene::Support;
use anyhow::{Result, ensure};
use bevy::prelude::*;
use osage_dynamics::{DynamicBox, Lift, StaticMesh};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

pub const DEFAULT: &[u8] = include_bytes!("../fixtures/descent.json");

#[derive(Deserialize, Resource)]
pub struct Replay {
    version: u32,
    pub tick_hz: u32,
    pub gravity_m_s2: f32,
    pub terminal_m_s: f32,
    pub viewport: [u32; 2],
    pub vertical_fov_deg: f32,
    pub routes: Vec<Route>,
}

#[derive(Deserialize)]
pub struct Route {
    pub id: String,
    pub ticks: u32,
    pub start_m: [f32; 3],
    pub velocity_m_s: [f32; 3],
    pub expected_support: String,
    #[serde(default)]
    pub bonnet: Option<DynamicBox>,
    #[serde(default)]
    pub lift: Option<Lift>,
    #[serde(default)]
    pub lift_contact_hz: Option<u32>,
    #[serde(default)]
    pub lift_response_travel_m: Option<f32>,
    #[serde(default)]
    pub ragdoll_activation: Option<osage_dynamics::AnimationHandoff>,
    pub camera: Vec<Key>,
}

#[derive(Deserialize)]
pub struct Key {
    pub tick: u32,
    pub eye: [f32; 3],
    pub aim: [f32; 3],
}

impl Replay {
    pub fn read<'a>(bytes: &[u8], surface_names: impl IntoIterator<Item = &'a str>) -> Result<Self> {
        let r: Self = serde_json::from_slice(bytes)?;
        let surface_names: HashSet<_> = surface_names.into_iter().collect();
        ensure!(r.version == 1 && (1..=240).contains(&r.tick_hz), "unsupported replay clock");
        ensure!(
            r.gravity_m_s2.is_finite()
                && r.terminal_m_s.is_finite()
                && r.gravity_m_s2 > 0.
                && r.terminal_m_s > 0.
                && (1.0..179.0).contains(&r.vertical_fov_deg),
            "invalid dynamics or optics"
        );
        ensure!(r.viewport.iter().all(|n| (1..=8192).contains(n)), "invalid viewport");
        ensure!(!r.routes.is_empty(), "empty replay");
        let mut ids = HashSet::new();
        for route in &r.routes {
            ensure!(
                !route.id.is_empty() && !route.id.contains([',', '\n', '\r']) && ids.insert(&route.id),
                "duplicate or empty route ID"
            );
            ensure!(
                Vec3::from(route.start_m).is_finite() && Vec3::from(route.velocity_m_s).is_finite(),
                "non-finite initial state"
            );
            ensure!(
                route.lift_contact_hz.is_none_or(|hz| route.lift.is_some() && (1..=3840).contains(&hz)),
                "lift_contact_hz requires a lift and a rate in 1..=3840"
            );
            ensure!(
                route.lift_response_travel_m.is_none_or(|m| m.is_finite() && m > 0. && route.lift_contact_hz.is_some()),
                "lift_response_travel_m needs a positive travel and a configured lift contact rate"
            );
            ensure!((1..=100_000).contains(&route.ticks), "invalid tick count");
            ensure!(
                route.camera.first().is_some_and(|k| k.tick == 0)
                    && route.camera.last().is_some_and(|k| k.tick == route.ticks)
                    && route.camera.windows(2).all(|p| p[0].tick < p[1].tick),
                "invalid camera keys for {}",
                route.id
            );
            ensure!(
                route.bonnet.is_none_or(|b| b.valid() && !surface_names.contains("bonnet")),
                "invalid or duplicate bonnet"
            );
            ensure!(
                route.lift.is_none_or(|l| l.valid() && !surface_names.contains("lift")),
                "invalid or duplicate lift"
            );
            ensure!(route.lift.is_none() || route.bonnet.is_none(), "a route supports one moving surface");
            ensure!(
                surface_names.contains(route.expected_support.as_str())
                    || route.moving_surface() == Some(route.expected_support.as_str()),
                "unknown landing surface {}",
                route.expected_support
            );
            for key in &route.camera {
                let dir = Vec3::from(key.aim) - Vec3::from(key.eye);
                ensure!(dir.is_finite() && dir.cross(Vec3::Y).length_squared() > 1e-8, "degenerate camera key");
            }
        }
        Ok(r)
    }
}

impl Route {
    pub fn moving_surface(&self) -> Option<&'static str> {
        self.lift.map(|_| "lift").or_else(|| self.bonnet.map(|_| "bonnet"))
    }

    pub fn moving_half_extents(&self) -> Option<[f32; 3]> {
        self.lift.map(|lift| lift.half_extents_m).or_else(|| self.bonnet.map(|body| body.half_extents_m))
    }

    pub fn surface_name<'a>(&self, meshes: &'a [StaticMesh], index: usize) -> Option<&'a str> {
        if index == meshes.len() { self.moving_surface() } else { meshes.get(index).map(|m| m.id.as_str()) }
    }

    pub fn camera_at(&self, tick: u32) -> (Vec3, Vec3) {
        let right = self.camera.partition_point(|k| k.tick < tick).clamp(1, self.camera.len() - 1);
        let (a, b) = (&self.camera[right - 1], &self.camera[right]);
        let t = (tick.min(self.ticks) - a.tick) as f32 / (b.tick - a.tick) as f32;
        let interpolate = |start: [f32; 3], end: [f32; 3]| {
            let (start, end) = (Vec3::from(start), Vec3::from(end));
            // An unchanged coordinate must retain exactly its authored value on every frame.
            Vec3::select(start.cmpeq(end), start, start.lerp(end, t))
        };
        (interpolate(a.eye, b.eye), interpolate(a.aim, b.aim))
    }
}

#[derive(Resource)]
pub struct Grid {
    cells: HashMap<(i32, i32), Vec<usize>>,
    wide: Vec<usize>,
}

const CELL_M: f32 = 32.0;

fn cell(p: Vec2) -> IVec2 {
    (p / CELL_M).floor().as_ivec2()
}

// Very large sheets and long diagonal sweeps are cheaper to scan directly.
fn cell_count(lo: IVec2, hi: IVec2) -> u64 {
    let n = |a, b| (i64::from(b) - i64::from(a) + 1) as u64;
    n(lo.x, hi.x).saturating_mul(n(lo.y, hi.y))
}

impl Grid {
    pub fn new(surfaces: &[Support]) -> Self {
        let mut cells = HashMap::<_, Vec<_>>::new();
        let mut wide = Vec::new();
        for (i, s) in surfaces.iter().enumerate() {
            let (lo, hi) = (cell(s.low), cell(s.high));
            if cell_count(lo, hi) > 4096 {
                wide.push(i);
                continue;
            }
            for z in lo.y..=hi.y {
                for x in lo.x..=hi.x {
                    cells.entry((x, z)).or_default().push(i);
                }
            }
        }
        Self { cells, wide }
    }
}

#[derive(Clone, Copy)]
pub enum Search<'a> {
    Full,
    Grid(&'a Grid),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    pub foot: Vec3,
    pub velocity: Vec3,
    pub landed: Option<(u32, usize)>,
}

impl Body {
    pub fn new(route: &Route) -> Self {
        Self { foot: route.start_m.into(), velocity: route.velocity_m_s.into(), landed: None }
    }

    /// Semi-implicit Euler. The first surface crossed by the descending foot ends the drop.
    pub fn step(&mut self, tick: u32, replay: &Replay, surfaces: &[Support], search: Search) -> usize {
        if self.landed.is_some() {
            return 0;
        }
        let dt = 1.0 / replay.tick_hz as f32;
        self.velocity.y = (self.velocity.y - replay.gravity_m_s2 * dt).max(-replay.terminal_m_s);
        let end = self.foot + self.velocity * dt;
        let (hit, checks) = first_hit(self.foot, end, surfaces, search);
        if let Some((t, i)) = hit {
            self.foot = self.foot.lerp(end, t);
            self.foot.y = surfaces[i].y;
            self.velocity = Vec3::ZERO;
            self.landed = Some((tick, i));
        } else {
            self.foot = end;
        }
        checks
    }
}

fn first_hit(from: Vec3, to: Vec3, surfaces: &[Support], search: Search) -> (Option<(f32, usize)>, usize) {
    if to.y >= from.y {
        return (None, 0);
    }
    let mut best = None;
    let mut checks = 0;
    let mut check = |i: usize| {
        checks += 1;
        let s = &surfaces[i];
        if s.y > from.y || s.y < to.y {
            return;
        }
        let t = (from.y - s.y) / (from.y - to.y);
        let at = from.lerp(to, t);
        if at.x >= s.low.x
            && at.x <= s.high.x
            && at.z >= s.low.y
            && at.z <= s.high.y
            && best.is_none_or(|(prev, j)| t < prev || (t == prev && i < j))
        {
            best = Some((t, i));
        }
    };
    match search {
        Search::Full => (0..surfaces.len()).for_each(check),
        Search::Grid(grid) => {
            let a = Vec2::new(from.x, from.z);
            let b = Vec2::new(to.x, to.z);
            let (lo, hi) = (cell(a.min(b)), cell(a.max(b)));
            if cell_count(lo, hi) > 64 {
                (0..surfaces.len()).for_each(check);
            } else {
                grid.wide.iter().copied().for_each(&mut check);
                for z in lo.y..=hi.y {
                    for x in lo.x..=hi.x {
                        if let Some(ids) = grid.cells.get(&(x, z)) {
                            ids.iter().copied().for_each(&mut check);
                        }
                    }
                }
            }
        }
    }
    (best, checks)
}

/// Rational accumulator shared by the timed window and the cadence checks.
#[derive(Default)]
pub struct Clock {
    phase: u128,
}

impl Clock {
    pub fn advance(&mut self, nanos: u128, hz: u32) -> u32 {
        self.phase += nanos * u128::from(hz);
        let due = self.phase / 1_000_000_000;
        self.phase %= 1_000_000_000;
        due.min(u128::from(u32::MAX)) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Sector;

    #[test]
    fn stationary_camera_coordinates_are_exact_between_keys() {
        let start = Key { tick: 0, eye: [1., 4., 15.5], aim: [12., 2., 10.] };
        let end = Key { tick: 140, eye: [3., 4., 15.5], aim: start.aim };
        let route = Route {
            id: "camera".into(),
            ticks: 140,
            start_m: [0.; 3],
            velocity_m_s: [0.; 3],
            bonnet: None,
            lift: None,
            lift_contact_hz: None,
            lift_response_travel_m: None,
            ragdoll_activation: None,
            expected_support: "floor".into(),
            camera: vec![start, end],
        };
        for tick in 0..=140 {
            let (eye, aim) = route.camera_at(tick);
            assert_eq!((eye.y, eye.z, aim.to_array()), (4., 15.5, [12., 2., 10.]));
        }
    }

    #[test]
    fn grid_finds_hits_between_cells_and_below_an_overhead_deck() {
        let scene = Sector::read(crate::scene::DEFAULT).unwrap();
        let grid = Grid::new(&scene.supports);
        let sweep = |a, b| {
            let flat = first_hit(a, b, &scene.supports, Search::Full).0;
            assert_eq!(flat, first_hit(a, b, &scene.supports, Search::Grid(&grid)).0);
            flat.map(|(_, i)| scene.supports[i].id.as_str())
        };
        assert_eq!(sweep(Vec3::new(-35., 20., -62.), Vec3::new(35., 12., 62.)), Some("rail"));
        assert_eq!(sweep(Vec3::new(-5., 14., 8.), Vec3::new(-4., 0., -4.)), Some("road"));
        assert_eq!(sweep(Vec3::new(-42., 5., -32.), Vec3::new(-42., 0., -32.)), None);
    }

    #[test]
    fn wide_surfaces_and_long_sweeps_use_bounded_search() {
        let surfaces = [Support { id: "land".into(), low: Vec2::splat(-1e6), high: Vec2::splat(1e6), y: 0. }];
        let grid = Grid::new(&surfaces);
        assert!(grid.cells.is_empty());
        for x in [2., 1e5] {
            let a = Vec3::new(-x, 1., -x);
            let b = Vec3::new(x, -1., x);
            assert_eq!(first_hit(a, b, &surfaces, Search::Grid(&grid)), (Some((0.5, 0)), 1));
        }
    }

    #[test]
    fn adjacent_footprints_share_a_deterministic_edge() {
        let surfaces = [
            Support { id: "a".into(), low: Vec2::splat(-32.), high: Vec2::ZERO, y: 1. },
            Support { id: "b".into(), low: Vec2::ZERO, high: Vec2::splat(32.), y: 1. },
        ];
        let grid = Grid::new(&surfaces);
        for search in [Search::Full, Search::Grid(&grid)] {
            assert_eq!(first_hit(Vec3::Y * 2., Vec3::ZERO, &surfaces, search).0, Some((0.5, 0)));
        }
    }
}
