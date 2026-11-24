// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The ordinary Rapier actor and solver, with individually replaceable static terrain.
mod clearance;
use crate::{
    ContactImpulse, DynamicBox, Engine, Lift, Part, RigStart, Settings, Simulation, Solver, mesh_contacts,
    page_edges::PageEdges,
};
use anyhow::{Result, ensure};
pub use clearance::UprightSweep;
use rapier3d::prelude::{Collider, ColliderBuilder, ColliderHandle};

pub struct TerrainSimulation {
    simulation: Simulation,
    edges: PageEdges,
    first_moving_step: bool,
}

impl TerrainSimulation {
    pub fn with_boxes(settings: Settings, parts: &[Part], start: RigStart<'_>, boxes: &[DynamicBox]) -> Result<Self> {
        let simulation = Simulation::from_rig(Engine::Rapier, &[], boxes, settings, parts, start)?;
        let mut terrain = Self { simulation, edges: PageEdges::default(), first_moving_step: !boxes.is_empty() };
        let queries = mesh_contacts::MeshQueries::for_pages(terrain.edges.clone());
        terrain.rapier_mut().sim.narrow_phase.set_query_dispatcher(queries);
        for i in 0..boxes.len() {
            let native = terrain.rapier_mut();
            let handle = native.box_collider(i);
            native.sim.colliders[handle].user_data = u128::from(handle.into_raw_parts().0);
        }
        // Until a caller supplies its district bounds, solve native contacts without assuming
        // that an unloaded static surface is absent from the capsule/box's swept neighbourhood.
        if !boxes.is_empty() {
            terrain.set_static_bounds(&[[[f32::MIN; 3], [f32::MAX; 3]]])?;
        }
        Ok(terrain)
    }

    pub fn new(settings: Settings, parts: &[Part], start: RigStart<'_>) -> Result<Self> {
        Self::with_boxes(settings, parts, start, &[])
    }

    /// Install the resident solver's driven body and use its native collider slot for page contacts.
    pub fn add_lift(&mut self, lift: Lift) -> Result<()> {
        self.simulation.add_lift(lift)?;
        let handle = self.box_collider(0);
        self.rapier_mut().sim.colliders[handle].user_data = u128::from(handle.into_raw_parts().0);
        self.first_moving_step = self.simulation.completed_ticks == 0;
        Ok(())
    }

    /// Minimum and maximum centre height over the next fixed ticks, including any reversal.
    /// While the district is held, the simulation clock and this interval stay at the same pose.
    pub fn lift_height_range(&self, ticks: u32) -> Option<[f32; 2]> {
        let state = &self.simulation;
        let lift = state.lift?;
        let period = f64::from(lift.period_s);
        let duration = f64::from(ticks) * f64::from(state.substeps) / f64::from(state.settings.tick_hz);
        let start = state.substep_time(0);
        let phase = start.rem_euclid(period);
        let end = phase + duration;
        let current = state.box_poses[0].center[1];
        let target = lift.center_at(start + duration)[1];
        let mut span = [current.min(target), current.max(target)];
        if end >= period {
            span[0] = span[0].min(lift.center_m[1]);
        }
        let peak = if phase <= 0.5 * period { 0.5 * period } else { 1.5 * period };
        if end >= peak {
            span[1] = span[1].max(lift.center_m[1] + lift.travel_m);
        }
        Some(span)
    }

    /// Upper bound on rise at the fastest upward velocity prescribed to the platform
    /// in this interval. A passenger can retain that velocity after losing contact.
    pub fn lift_carry_rise(&self, ticks: u8) -> Option<f32> {
        let state = &self.simulation;
        let lift = state.lift?;
        let count = u32::from(ticks) * u32::from(state.substeps);
        let start = state.completed_ticks as f64 * f64::from(state.substeps);
        let mut previous = state.box_poses[0].center[1];
        let mut max_delta = 0.0_f64;
        for step in 1..=count {
            let time = (start + f64::from(step)) / f64::from(state.settings.tick_hz);
            let next = lift.center_at(time)[1];
            max_delta = max_delta.max(f64::from(next) - f64::from(previous));
            previous = next;
        }
        Some((max_delta * f64::from(count)) as f32)
    }

    /// Provide conservative fixed geometry bounds before advancing a moving pair.
    /// Each bound may cover both installed and currently unavailable triangles.
    pub fn set_static_bounds(&mut self, bounds: &[[[f32; 3]; 2]]) -> Result<()> {
        ensure!(
            bounds.iter().all(|[lo, hi]| (0..3).all(|i| lo[i].is_finite() && hi[i].is_finite() && lo[i] <= hi[i])),
            "invalid fixed terrain bounds"
        );
        let bounds = || {
            bounds
                .iter()
                .map(|[lo, hi]| {
                    (glam::DVec3::from_array(lo.map(f64::from)), glam::DVec3::from_array(hi.map(f64::from)))
                })
                .collect()
        };
        if let Some(pair) = &mut self.simulation.free_pair {
            pair.set_fixed_bounds(bounds());
        }
        if let Some(articulation) = &mut self.simulation.articulated_sweep {
            articulation.set_fixed_bounds(bounds());
        }
        Ok(())
    }
    /// The native collider can change at the first step, after initial terrain is installed.
    /// The box ordinal, rigid body and reported pose remain stable.
    pub fn box_collider(&self, index: usize) -> ColliderHandle {
        self.rapier().box_collider(index)
    }

    fn rapier(&self) -> &crate::rapier::World {
        match &self.simulation.solver {
            Solver::Rapier(world) => world,
            #[cfg(not(target_arch = "wasm32"))]
            Solver::Jolt(_) => unreachable!("terrain requires Rapier"),
        }
    }

    fn rapier_mut(&mut self) -> &mut crate::rapier::World {
        match &mut self.simulation.solver {
            Solver::Rapier(world) => {
                // Even replacement in the same collider slot gives a new contact surface.
                world.invalidate_resting_contacts();
                world
            }
            #[cfg(not(target_arch = "wasm32"))]
            Solver::Jolt(_) => unreachable!("terrain requires Rapier"),
        }
    }

    /// The surface index in contacts and impulses is the slot of the currently installed handle.
    /// Copy the owner's identity before removing or replacing the collider.
    pub fn insert(
        &mut self,
        collider: ColliderBuilder,
        node: Option<usize>,
        id: &str,
        tile: [i32; 2],
    ) -> ColliderHandle {
        let native = &mut self.rapier_mut().sim;
        let handle = native.insert_collider(collider, None);
        native.colliders[handle].user_data = u128::from(handle.into_raw_parts().0);
        if let Some(mesh) = self.collider(handle).shape().as_trimesh() {
            self.edges.add(mesh, node, id, tile);
        }
        handle
    }

    pub fn remove(&mut self, handle: ColliderHandle) {
        if let Some(mesh) = self.collider(handle).shape().as_trimesh() {
            self.edges.remove(mesh);
        }
        self.rapier_mut().sim.remove_collider(handle);
    }

    pub fn collider(&self, handle: ColliderHandle) -> &Collider {
        &self.rapier().sim.colliders[handle]
    }

    pub fn part_collider(&self, index: usize) -> &Collider {
        self.collider(self.rapier().parts[index].1)
    }

    pub fn state(&self) -> &Simulation {
        &self.simulation
    }

    pub fn step(&mut self, impulses: Option<&mut Vec<ContactImpulse>>) -> Result<()> {
        if std::mem::take(&mut self.first_moving_step) {
            let lift = self.simulation.lift.is_some();
            self.rapier_mut().finish_moving_bootstrap(lift);
        }
        match impulses {
            Some(out) => self.simulation.step_with_impulses(out),
            None => self.simulation.step(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Capsule, StaticMesh};
    use rapier3d::prelude::{CoefficientCombineRule, Vector};

    #[test]
    fn the_driven_lift_keeps_time_and_its_contact_when_distant_pages_change() {
        let cfg = Settings {
            tick_hz: 60,
            gravity: 9.81,
            terminal_speed: 55.,
            capsule: Capsule { radius: 0.28, half_segment: 0.6, mass: 75., friction: 0.5 },
        };
        let lift = Lift {
            center_m: [32., 24., 8.],
            half_extents_m: [1.5, 0.125, 1.25],
            travel_m: 3.,
            period_s: 10.,
            friction: 0.6,
        };
        let parts = [cfg.capsule.part()];
        let start = || RigStart { origin_m: [32.5, 25.005, 7.625], velocity_m_s: [0.; 3], animation: None };
        let mut paged = TerrainSimulation::new(cfg, &parts, start()).unwrap();
        let mut resident = Simulation::from_rig(Engine::Rapier, &[], &[], cfg, &parts, start()).unwrap();
        paged.add_lift(lift).unwrap();
        resident.add_lift(lift).unwrap();
        let distant = || ColliderBuilder::ball(0.25).translation(Vector::splat(80.));
        let invalidated = paged.insert(distant(), Some(8), "distant", [2, 2]);
        let mut other = paged.insert(distant(), Some(9), "distant", [3, 2]);
        paged.remove(invalidated);
        let mut stable = None;
        for tick in 1..=600 {
            if tick == 185 || tick == 470 {
                paged.remove(other);
                other = paged.insert(distant(), Some(9), "distant", [3, 2]);
            }
            let [lo, hi] = paged.lift_height_range(1).unwrap();
            assert_eq!(paged.lift_height_range(605), Some([24., 27.]));
            for j in 0..=8 {
                let time = (f64::from(tick - 1) + f64::from(j) / 8.) / 60.;
                assert!((lo..=hi).contains(&lift.center_at(time)[1]));
            }
            paged.step(None).unwrap();
            resident.step().unwrap();
            let handle = paged.box_collider(0);
            assert_eq!(*stable.get_or_insert(handle), handle);
            assert_ne!(other, handle);
            assert_eq!(paged.state().box_poses, resident.box_poses, "lift at tick {tick}");
            assert_eq!(paged.state().part_poses, resident.part_poses, "rider at tick {tick}");
            assert!(!paged.state().contacts.is_empty());
            assert!(paged.state().contacts.iter().all(|c| c.surface == handle.into_raw_parts().0 as usize));
        }
        let mut fast = TerrainSimulation::new(cfg, &parts, start()).unwrap();
        fast.add_lift(Lift { period_s: 0.01, ..lift }).unwrap();
        assert_eq!(fast.lift_height_range(1), Some([24., 27.]));
    }

    #[test]
    fn a_vacated_bootstrap_slot_does_not_put_the_box_before_initial_terrain() {
        let cfg = Settings {
            tick_hz: 60,
            gravity: 9.81,
            terminal_speed: 55.,
            capsule: Capsule { radius: 0.28, half_segment: 0.6, mass: 75., friction: 0.5 },
        };
        let boxes = [DynamicBox {
            center_m: [0., 0.4, 0.],
            half_extents_m: [1.05, 0.4, 0.82],
            rotation_xyzw: [0., 0., 0., 1.],
            velocity_m_s: [0.25, 0., 0.],
            mass_kg: 1250.,
            friction: 0.5,
        }];
        let mesh = StaticMesh {
            id: "street".into(),
            material: None,
            vertices: vec![[-8., 0., -8.], [-8., 0., 8.], [8., 0., 8.], [8., 0., -8.]],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
        };
        let parts = [cfg.capsule.part()];
        let start = || RigStart { origin_m: [0.25, 3., 0.5], velocity_m_s: [0.5, -2., 0.], animation: None };
        let mut paged = TerrainSimulation::with_boxes(cfg, &parts, start(), &boxes).unwrap();
        paged.set_static_bounds(&[[[-8., 0., -8.], [8., 0., 8.]]]).unwrap();
        let temporary = paged.insert(ColliderBuilder::ball(0.25).translation(Vector::splat(40.)), None, "old", [-1, 0]);
        let street = mesh_contacts::collider(
            mesh.vertices.iter().copied().map(Vector::from_array).collect(),
            mesh.triangles.clone(),
        )
        .unwrap()
        .friction(cfg.capsule.friction.sqrt())
        .friction_combine_rule(CoefficientCombineRule::Multiply)
        .restitution_combine_rule(CoefficientCombineRule::Max);
        let existing = paged.insert(street, Some(1), "street", [0, 0]);
        paged.remove(temporary);
        let mut resident = Simulation::from_rig(Engine::Rapier, &[mesh], &boxes, cfg, &parts, start()).unwrap();
        for i in 1..=90 {
            resident.step().unwrap();
            paged.step(None).unwrap();
            assert!(paged.box_collider(0).into_raw_parts().0 > existing.into_raw_parts().0);
            assert_eq!(paged.state().box_poses, resident.box_poses, "box at tick {i}");
            assert_eq!(paged.state().part_poses, resident.part_poses, "actor at tick {i}");
        }
    }
}
