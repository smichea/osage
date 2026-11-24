// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

mod animation;
pub use animation::{AnimatedBody, AnimationHandoff, BodyTransform, RigStart};
pub mod ballistic;
mod free_pair;
mod impulses;
pub use impulses::{ContactBodies, ContactImpulse, ImpulseAttribution, ImpulseMeasurement};
use impulses::{Interval, Observation};
#[cfg(not(target_arch = "wasm32"))]
mod jolt;
mod lift;
pub mod lift_flight;
pub mod lift_response;
pub use lift::Lift;
pub mod mesh_contacts;
pub mod page_edges;
mod rapier;
mod rig;
mod static_instances;
pub mod streamed;
pub mod support_footprint;
use glam::{Quat, Vec3};
pub use rig::Part;

/// Points and triangle winding are in the common right-handed world frame, in metres.
pub struct StaticMesh {
    /// A legacy mesh uses the actor friction and zero restitution.
    pub material: Option<osage_objects::CollisionMaterial>,
    pub id: String,
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

/// A freely moving solid. Its surface index follows all fixed meshes, in the order supplied.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct DynamicBox {
    pub center_m: [f32; 3],
    pub half_extents_m: [f32; 3],
    pub rotation_xyzw: [f32; 4],
    pub velocity_m_s: [f32; 3],
    pub mass_kg: f32,
    pub friction: f32,
}

impl DynamicBox {
    pub fn valid(self) -> bool {
        self.center_m.into_iter().chain(self.velocity_m_s).all(f32::is_finite)
            && Quat::from_array(self.rotation_xyzw).is_normalized()
            && self.half_extents_m.iter().all(|v| v.is_finite() && *v > 0.)
            && self.mass_kg.is_finite()
            && self.mass_kg > 0.
            && self.friction.is_finite()
            && self.friction >= 0.
    }

    fn initial_pose(self) -> Pose {
        Pose {
            center: self.center_m,
            rotation_xyzw: self.rotation_xyzw,
            linear_velocity: self.velocity_m_s,
            angular_velocity: [0.; 3],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Pose {
    pub center: [f32; 3],
    pub rotation_xyzw: [f32; 4],
    pub linear_velocity: [f32; 3],
    pub angular_velocity: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    Rapier,
    Jolt,
}

impl fmt::Display for Engine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Rapier => "rapier",
            Self::Jolt => "jolt",
        })
    }
}

impl FromStr for Engine {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "rapier" => Ok(Self::Rapier),
            "jolt" => Ok(Self::Jolt),
            _ => bail!("unknown physics engine {value}"),
        }
    }
}

#[derive(Clone, Copy)]
pub struct Capsule {
    pub radius: f32,
    pub half_segment: f32,
    pub mass: f32,
    pub friction: f32,
}

impl Capsule {
    /// One centred physical body for the standalone capsule actor.
    pub fn part(self) -> Part {
        Part {
            name: "capsule".to_owned(),
            inertia: None,
            material: None,
            joint_rotation: Quat::IDENTITY,
            parent: None,
            center: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            radius: self.radius,
            half_segment: self.half_segment,
            mass: self.mass,
            anchor: Vec3::ZERO,
            swing_envelope: Default::default(),
            limits_deg: [[0.; 2]; 3],
        }
    }
}

#[derive(Clone, Copy)]
pub struct Settings {
    pub tick_hz: u32,
    pub gravity: f32,
    pub terminal_speed: f32,
    pub capsule: Capsule,
}

impl Settings {
    fn dt(self) -> f32 {
        1.0 / self.tick_hz as f32
    }

    // Leave room for the gravity impulse in the upcoming step. Contacts still change velocity.
    fn fall_limit(self, vy: f32, dt: f32) -> f32 {
        vy.max((-self.terminal_speed + self.gravity * dt).min(0.0))
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
pub struct Contact {
    /// One-based collision substep within the public tick.
    pub substep: u8,
    pub part: usize,
    /// Fixed meshes followed by the moving supports. Point and normal belong to the touched surface.
    pub surface: usize,
    /// Ordinal in the input triangle mesh, when the solver identifies the touched triangle.
    #[serde(rename = "source_triangle", skip_serializing_if = "Option::is_none")]
    pub surface_triangle: Option<u32>,
    pub point: [f32; 3],
    pub normal: [f32; 3],
    /// Positive for a gap, negative for penetration. Manifolds may contain speculative points.
    pub separation: f32,
    /// Time of a directly applied isolated impulse when no native manifold reported the pair.
    /// The geometric time of first touch is reported separately in `swept_impact_offset_s`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied_offset_s: Option<f32>,
}

/// End state of one numbered substep on the solver's configured clock.
pub struct SubstepState<'a> {
    /// One-based index within the public tick.
    pub substep: u8,
    pub parts: &'a [Pose],
    pub boxes: &'a [Pose],
    /// All manifold witnesses emitted during this substep, including speculative contacts.
    pub contacts: &'a [Contact],
}

pub struct Simulation {
    solver: Solver,
    settings: Settings,
    substeps: u8,
    completed_ticks: u64,
    lift: Option<Lift>,
    lift_approach: Option<lift_flight::LiftApproach>,
    free_pair: Option<Box<free_pair::FreePair>>,
    articulated_sweep: Option<Box<free_pair::ArticulatedSweep>>,
    /// First inward contact time within the last public tick for an isolated translating
    /// capsule or joint tree against a free cuboid. Contact witnesses are recorded in `contacts`.
    pub swept_impact_offset_s: Option<f32>,
    pub pose: Pose,
    pub part_poses: Vec<Pose>,
    pub box_poses: Vec<Pose>,
    pub contacts: Vec<Contact>,
}

enum Solver {
    Rapier(Box<rapier::World>),
    #[cfg(not(target_arch = "wasm32"))]
    Jolt(Box<jolt::World>),
}

impl Simulation {
    pub fn new(
        engine: Engine,
        meshes: &[StaticMesh],
        boxes: &[DynamicBox],
        settings: Settings,
        center: [f32; 3],
        velocity: [f32; 3],
    ) -> Result<Self> {
        Self::from_parts(engine, meshes, boxes, settings, center, velocity, &[settings.capsule.part()])
    }

    pub fn from_parts(
        engine: Engine,
        meshes: &[StaticMesh],
        boxes: &[DynamicBox],
        settings: Settings,
        origin: [f32; 3],
        velocity: [f32; 3],
        parts: &[Part],
    ) -> Result<Self> {
        Self::from_rig(
            engine,
            meshes,
            boxes,
            settings,
            parts,
            RigStart { origin_m: origin, velocity_m_s: velocity, animation: None },
        )
    }

    /// Build the constraints in the bind pose, then activate the native bodies at the sampled motion.
    pub fn from_rig(
        engine: Engine,
        meshes: &[StaticMesh],
        boxes: &[DynamicBox],
        settings: Settings,
        parts: &[Part],
        start: RigStart<'_>,
    ) -> Result<Self> {
        let origin = start.origin_m;
        let velocity = start.velocity_m_s;
        let c = settings.capsule;
        ensure!(
            (1..=240).contains(&settings.tick_hz)
                && settings.gravity.is_finite()
                && settings.gravity >= 0.0
                && settings.terminal_speed.is_finite()
                && settings.terminal_speed > 0.0,
            "invalid simulation settings"
        );
        ensure!(
            c.friction.is_finite() && c.friction >= 0.0 && origin.into_iter().chain(velocity).all(f32::is_finite),
            "invalid capsule or spawn"
        );
        ensure!(!parts.is_empty(), "empty rig");
        for (i, p) in parts.iter().enumerate() {
            ensure!(
                (i == 0 && p.parent.is_none() || p.parent.is_some_and(|j| j < i))
                    && p.center.is_finite()
                    && p.rotation.is_finite()
                    && p.rotation.is_normalized()
                    && p.anchor.is_finite()
                    && p.joint_rotation.is_normalized()
                    && p.inertia.is_none_or(|i| i.is_finite() && i.min_element() > 0.)
                    && p.material.is_none_or(|m| m.valid())
                    && [p.radius, p.mass].iter().all(|v| v.is_finite() && *v > 0.)
                    && p.half_segment.is_finite()
                    && p.half_segment >= 0.
                    && p.swing_envelope.accepts(p.limits_deg)
                    && p.limits_deg.iter().all(|[a, b]| a.is_finite()
                        && b.is_finite()
                        && *a <= 0.
                        && *b >= 0.
                        && *a >= -180.
                        && *b <= 180.),
                "invalid physical part {}",
                p.name
            );
        }
        for mesh in meshes {
            ensure!(
                mesh.material.is_none_or(|m| m.valid())
                    && !mesh.triangles.is_empty()
                    && mesh.vertices.iter().flatten().all(|x| x.is_finite())
                    && mesh.triangles.iter().flatten().all(|i| (*i as usize) < mesh.vertices.len()),
                "invalid collision mesh {}",
                mesh.id
            );
        }
        ensure!(boxes.iter().all(|b| b.valid()), "invalid dynamic box");
        let substeps = if parts.len() > 1 {
            // Keep the existing four-way split and give low-rate clocks enough samples
            // for light limbs and joints during an impact.
            240_u32.div_ceil(settings.tick_hz).max(4) as u8
        } else if boxes.is_empty() {
            1
        } else {
            // A coarse moving contact point can create torque on a centered, co-moving pair.
            120_u32.div_ceil(settings.tick_hz) as u8
        };
        let settings = Settings { tick_hz: settings.tick_hz * u32::from(substeps), ..settings };
        let mut solver = match engine {
            Engine::Rapier => {
                Solver::Rapier(Box::new(rapier::World::new(meshes, boxes, settings, origin, velocity, parts)?))
            }
            #[cfg(not(target_arch = "wasm32"))]
            Engine::Jolt => Solver::Jolt(Box::new(jolt::World::new(meshes, boxes, settings, origin, velocity, parts)?)),
            #[cfg(target_arch = "wasm32")]
            Engine::Jolt => bail!("the Jolt adapter requires a native target"),
        };
        let part_poses: Vec<_> = if let Some(animation) = start.animation {
            let poses = animation.initial_poses(parts, origin.into(), velocity.into())?;
            match &mut solver {
                Solver::Rapier(world) => world.starting_poses(&poses),
                #[cfg(not(target_arch = "wasm32"))]
                Solver::Jolt(world) => world.starting_poses(&poses)?,
            }
            poses
        } else {
            parts
                .iter()
                .map(|p| Pose {
                    center: (Vec3::from_array(origin) + p.center).to_array(),
                    rotation_xyzw: p.rotation.to_array(),
                    linear_velocity: velocity,
                    angular_velocity: [0.; 3],
                })
                .collect()
        };
        Ok(Self {
            solver,
            settings,
            substeps,
            completed_ticks: 0,
            lift: None,
            lift_approach: None,
            free_pair: free_pair::FreePair::for_scene(parts, boxes, meshes, settings).map(Box::new),
            articulated_sweep: free_pair::ArticulatedSweep::for_scene(parts, boxes, meshes).map(Box::new),
            swept_impact_offset_s: None,
            pose: part_poses[0],
            part_poses,
            box_poses: boxes.iter().map(|b| b.initial_pose()).collect(),
            contacts: Vec::new(),
        })
    }

    pub fn substeps_per_tick(&self) -> u8 {
        self.substeps
    }

    /// Add one prescribed vertical support to a world without moving boxes.
    /// Its surface ordinal follows the input static meshes and its pose is recorded in
    /// `box_poses[0]`. A lift added after startup starts at the current simulation time.
    pub fn add_lift(&mut self, lift: Lift) -> Result<()> {
        ensure!(lift.valid(), "invalid lift geometry, stroke, period or friction");
        ensure!(self.lift.is_none(), "the simulation already has a lift");
        ensure!(self.box_poses.is_empty(), "a lift and free moving boxes cannot share this simulation");
        let pose = lift.pose_at(self.substep_time(0));
        match &mut self.solver {
            Solver::Rapier(world) => world.add_lift(lift, pose),
            #[cfg(not(target_arch = "wasm32"))]
            Solver::Jolt(world) => world.add_lift(lift, pose)?,
        }
        self.box_poses.push(pose);
        self.lift = Some(lift);
        Ok(())
    }

    /// Forecast the first lift encounter from the current rig pose and refine native
    /// integration around that event. All other intervals remain on the original clock.
    pub fn plan_lift_approach(&mut self, parts: &[Part], duration_s: f64, contact_hz: u32) -> Result<()> {
        let lift = self.lift.ok_or_else(|| anyhow::anyhow!("a lift is required for an approach"))?;
        ensure!(
            parts.len() == self.part_poses.len() && (1..=3840).contains(&contact_hz),
            "invalid lift approach rig or rate"
        );
        let clock = lift_flight::FlightClock {
            start_s: self.substep_time(0),
            duration_s,
            gravity_m_s2: f64::from(self.settings.gravity),
            terminal_m_s: f64::from(self.settings.terminal_speed),
        };
        let mut earliest = duration_s;
        let mut predicted_contacts = Vec::new();
        // Keep all contacts simultaneous to the flight query's time resolution.
        const SAME_TIME_S: f64 = 1e-8;
        for (part, (shape, pose)) in parts.iter().zip(&self.part_poses).enumerate() {
            let window = lift_flight::FlightClock { duration_s: (earliest + SAME_TIME_S).min(duration_s), ..clock };
            if let Some(hit) = lift.first_impact(shape, *pose, window)? {
                earliest = earliest.min(hit.seconds);
                predicted_contacts.push(lift_flight::PredictedContact { part, hit });
            }
        }
        predicted_contacts.retain(|c| c.hit.seconds <= earliest + SAME_TIME_S);
        predicted_contacts.sort_by(|a, b| a.hit.seconds.total_cmp(&b.hit.seconds).then(a.part.cmp(&b.part)));
        self.lift_approach = Some(lift_flight::LiftApproach {
            start_s: clock.start_s,
            contact_hz,
            predicted_contacts,
            response: None,
            first_manifold: None,
            interrupted_at_s: None,
            extra_intervals: 0,
        });
        Ok(())
    }

    /// Limit estimated incoming travel per solve after a measured lift contact.
    pub fn sample_lift_response(&mut self, parts: &[Part], target_travel_m: f32) -> Result<()> {
        ensure!(parts.len() == self.part_poses.len(), "invalid lift response rig");
        let plan = self.lift_approach.as_mut().ok_or_else(|| anyhow::anyhow!("configure the lift approach first"))?;
        plan.response = Some(lift_response::LiftResponse::new(parts, target_travel_m)?);
        Ok(())
    }

    pub fn lift_approach(&self) -> Option<&lift_flight::LiftApproach> {
        self.lift_approach.as_ref()
    }

    fn advance_lift(&mut self, lift: Lift, substep: u8, output: &mut Option<&mut Vec<ContactImpulse>>) -> Result<()> {
        let start = self.substep_time(substep - 1);
        let hz = self.settings.tick_hz;
        let plan = self.lift_approach.as_mut().expect("configured approach");
        let step = self.completed_ticks * u64::from(self.substeps) + u64::from(substep - 1);
        let mut count = plan.intervals(start, hz);
        if let Some(response) = &mut plan.response
            && response.is_loading(step)
        {
            count = response.divisions(
                &self.part_poses,
                self.box_poses[0],
                self.settings.dt(),
                plan.contact_hz.div_ceil(hz).max(1),
            );
            response.record(step, count);
        }
        let dt = self.settings.dt() / count as f32;
        let mut offset = f32::from(substep - 1) * self.settings.dt();
        for slice in 0..count {
            let elapsed = |index| f64::from(index) / (f64::from(hz) * f64::from(count));
            let target = lift.center_at(start + elapsed(slice + 1));
            let surface = match &mut self.solver {
                Solver::Rapier(world) => {
                    world.move_lift(target);
                    world.first_box_surface()
                }
                #[cfg(not(target_arch = "wasm32"))]
                Solver::Jolt(world) => {
                    world.move_lift(target, dt)?;
                    world.first_box_surface()
                }
            };
            let before = self.contacts.len();
            self.advance(dt, substep, false, &mut offset, output)?;
            let approach = self.lift_approach.as_mut().unwrap();
            if slice > 0 {
                approach.extra_intervals += 1;
            }
            if approach.first_manifold.is_none()
                && let Some(c) = self.contacts[before..].iter().find(|c| c.surface == surface && c.separation <= 0.)
            {
                if let Some(response) = &mut approach.response {
                    response.start(step, self.substeps);
                }
                approach.first_manifold = Some(lift_flight::ManifoldTime {
                    start_s: start + elapsed(slice),
                    end_s: start + elapsed(slice + 1),
                    part: c.part,
                    point_m: c.point,
                    separation_m: c.separation,
                });
            } else if approach.first_manifold.is_none()
                && approach.interrupted_at_s.is_none()
                && self.contacts[before..].iter().any(|c| c.surface != surface && c.separation <= 0.)
            {
                // An earlier terrain reaction invalidates the uninterrupted flight.
                approach.interrupted_at_s = Some(start + elapsed(slice + 1));
            }
            if let Some(response) = &mut approach.response
                && response.is_loading(step + 1)
            {
                response.observe(&self.contacts[before..], surface);
            }
        }
        Ok(())
    }

    fn substep_time(&self, substep: u8) -> f64 {
        (self.completed_ticks as f64 * f64::from(self.substeps) + f64::from(substep)) / f64::from(self.settings.tick_hz)
    }

    fn advance(
        &mut self,
        dt: f32,
        substep: u8,
        separated: bool,
        offset: &mut f32,
        output: &mut Option<&mut Vec<ContactImpulse>>,
    ) -> Result<()> {
        if dt <= 0. {
            return Ok(());
        }
        let time = Interval { substep, start: *offset, end: *offset + dt };
        *offset = time.end;
        let observation = Observation { time, output: output.as_deref_mut() };
        match &mut self.solver {
            Solver::Rapier(world) => {
                world.suppress_free_pair(separated);
                world.prepare_static(self.settings, dt)?;
                world.step(
                    self.settings,
                    dt,
                    observation,
                    &mut self.contacts,
                    &mut self.part_poses,
                    &mut self.box_poses,
                );
            }
            #[cfg(not(target_arch = "wasm32"))]
            Solver::Jolt(world) => {
                world.suppress_free_pair(separated)?;
                world.prepare_static(self.settings, dt)?;
                world.step(
                    self.settings,
                    dt,
                    observation,
                    &mut self.contacts,
                    &mut self.part_poses,
                    &mut self.box_poses,
                )?;
            }
        }
        Ok(())
    }

    pub fn step(&mut self) -> Result<()> {
        self.step_observed(None, |_| {})
    }

    /// Advance the same physics tick and replace the buffer with its post-solve impulse records.
    /// Reading [`Self::contacts`] still gives all reported geometric witnesses, including gaps.
    /// A unique pair, world contact point, normal and scalar have independent availability.
    pub fn step_with_impulses(&mut self, output: &mut Vec<ContactImpulse>) -> Result<()> {
        output.clear();
        self.step_observed(Some(output), |_| {})
    }

    /// Advance one public tick, borrowing solved poses and contacts after each native substep.
    /// Split collision intervals within a substep share its end observation.
    pub fn step_with_substeps(&mut self, visit: impl FnMut(SubstepState<'_>)) -> Result<()> {
        self.step_observed(None, visit)
    }

    fn advance_substep(&mut self, substep: u8, output: &mut Option<&mut Vec<ContactImpulse>>) -> Result<()> {
        let dt = self.settings.dt();
        let mut offset = f32::from(substep - 1) * dt;
        if let Some(lift) = self.lift {
            if self.lift_approach.is_some() {
                return self.advance_lift(lift, substep, output);
            }
            let target = lift.center_at(self.substep_time(substep));
            match &mut self.solver {
                Solver::Rapier(world) => world.move_lift(target),
                #[cfg(not(target_arch = "wasm32"))]
                Solver::Jolt(world) => world.move_lift(target, dt)?,
            }
        }
        if let Solver::Rapier(world) = &mut self.solver
            && let Some(at) = world.supported_impact_time(self.settings, dt)
        {
            self.advance(at, substep, false, &mut offset, output)?;
            // Both partners and the joint constraints are now in the impact island.
            // Use the same iteration budget to relax its post-impact velocities.
            let Solver::Rapier(world) = &mut self.solver else { unreachable!() };
            let parameters = &mut world.sim.integration_parameters;
            let ordinary = parameters.num_internal_stabilization_iterations;
            parameters.num_internal_stabilization_iterations = ordinary.max(parameters.num_solver_iterations);
            let result = self.advance(dt - at, substep, false, &mut offset, output);
            let Solver::Rapier(world) = &mut self.solver else { unreachable!() };
            world.sim.integration_parameters.num_internal_stabilization_iterations = ordinary;
            return result;
        }
        if let Solver::Rapier(world) = &mut self.solver {
            let slices = world.compression_substeps(dt);
            if slices > 1 {
                for _ in 0..slices {
                    self.advance(dt / slices as f32, substep, false, &mut offset, output)?;
                }
                return Ok(());
            }
        }
        if let Some(sweep) = self.articulated_sweep.as_mut()
            && let Some(at) = sweep.first_impact(&self.part_poses, self.box_poses[0], self.settings, dt)
        {
            self.swept_impact_offset_s = Some(f32::from(substep - 1) * dt + at);
            // Keep native contacts and all joint constraints active on both sides of impact.
            // At a boundary the ordinary substep already provides the required contact pose.
            if at >= 1e-6 && dt - at >= 1e-6 {
                self.advance(at, substep, false, &mut offset, output)?;
                self.advance(dt - at, substep, false, &mut offset, output)?;
                return Ok(());
            }
        }
        let plan = self.free_pair.as_mut().map_or(free_pair::Step::Native, |pair| {
            pair.plan(self.part_poses[0], self.box_poses[0], self.settings, dt)
        });
        match plan {
            free_pair::Step::Native => self.advance(dt, substep, false, &mut offset, output)?,
            free_pair::Step::Separated => self.advance(dt, substep, true, &mut offset, output)?,
            free_pair::Step::Impact { after_s, guard_s, candidate } => {
                self.swept_impact_offset_s.get_or_insert(f32::from(substep - 1) * dt + after_s);
                // A pair already touching at the start can resolve immediately, using
                // the ordinary native step for the remaining time.
                let [free, contact_window, rest] =
                    if after_s == 0. { [0., 0., dt] } else { contact_intervals(dt, after_s + guard_s) };
                self.advance(free, substep, true, &mut offset, output)?;
                let mut applied_witness = None;
                if let (Solver::Rapier(world), Some(contact)) = (&mut self.solver, candidate)
                    && let Some(impulse) =
                        world.resolve_sticking(contact, &mut self.part_poses[0], &mut self.box_poses[0])
                {
                    let pair = ContactBodies { part: 0, surface: world.first_box_surface() };
                    let point = Vec3::from_array(self.box_poses[0].center) + contact.arms[1];
                    let actor_point = Vec3::from_array(self.part_poses[0].center) + contact.arms[0];
                    applied_witness = Some(Contact {
                        substep,
                        part: pair.part,
                        surface: pair.surface,
                        surface_triangle: None,
                        point: point.to_array(),
                        normal: contact.normal.to_array(),
                        separation: (actor_point - point).dot(contact.normal),
                        applied_offset_s: Some(offset),
                    });
                    if let Some(records) = output.as_deref_mut() {
                        records.push(Interval { substep, start: offset, end: offset }.record(
                            ImpulseAttribution::Pair(pair),
                            Some(point.to_array()),
                            Some(contact.normal.to_array()),
                            ImpulseMeasurement::AppliedVector(impulse.to_array()),
                        ));
                    }
                }
                // Build/satisfy the new manifold close to the first touching pose. The
                // remainder then advances the moving pair with that response in effect.
                self.advance(contact_window, substep, false, &mut offset, output)?;
                self.advance(rest, substep, false, &mut offset, output)?;
                // At a tick boundary the free interval can consume the entire step. Keep
                // the measured reaction even when Rapier has had no time to form a manifold.
                if let Some(witness) = applied_witness
                    && !self
                        .contacts
                        .iter()
                        .any(|c| c.substep == substep && c.part == witness.part && c.surface == witness.surface)
                {
                    self.contacts.push(witness);
                }
            }
        }
        Ok(())
    }

    fn step_observed(
        &mut self,
        mut output: Option<&mut Vec<ContactImpulse>>,
        mut visit: impl FnMut(SubstepState<'_>),
    ) -> Result<()> {
        self.contacts.clear();
        self.swept_impact_offset_s = None;
        for substep in 1..=self.substeps {
            let start = self.contacts.len();
            self.advance_substep(substep, &mut output)?;
            visit(SubstepState {
                substep,
                parts: &self.part_poses,
                boxes: &self.box_poses,
                contacts: &self.contacts[start..],
            });
        }
        self.pose = self.part_poses[0];
        ensure!(
            self.part_poses.iter().chain(&self.box_poses).all(|p| p
                .center
                .iter()
                .chain(&p.rotation_xyzw)
                .chain(&p.linear_velocity)
                .chain(&p.angular_velocity)
                .all(|v| v.is_finite())),
            "non-finite body pose"
        );
        ensure!(
            self.contacts.iter().all(|c| c.point.iter().chain(&c.normal).chain([&c.separation]).all(|v| v.is_finite())),
            "non-finite contact"
        );
        self.contacts.sort_by(|a, b| {
            a.substep
                .cmp(&b.substep)
                .then_with(|| a.surface.cmp(&b.surface))
                .then_with(|| a.part.cmp(&b.part))
                .then_with(|| a.separation.total_cmp(&b.separation))
                .then_with(|| a.point[0].total_cmp(&b.point[0]))
                .then_with(|| a.point[1].total_cmp(&b.point[1]))
                .then_with(|| a.point[2].total_cmp(&b.point[2]))
                .then_with(|| a.normal.map(f32::to_bits).cmp(&b.normal.map(f32::to_bits)))
        });
        self.completed_ticks += 1;
        Ok(())
    }
}

// Jolt accepts intervals of at least one microsecond. Merge a smaller remnant
// into its neighbour without dropping elapsed time or moving the reported geometric TOI.
fn contact_intervals(dt: f32, target: f32) -> [f32; 3] {
    const MIN_S: f32 = 1.0e-6;
    const MANIFOLD_S: f32 = 0.00025;
    let free = if target < MIN_S {
        0.
    } else if dt - target < MIN_S {
        dt
    } else {
        target
    };
    let remaining = dt - free;
    let contact = if remaining - MANIFOLD_S < MIN_S { remaining } else { MANIFOLD_S };
    [free, contact, remaining - contact]
}

#[cfg(test)]
mod step_tests {
    #[test]
    fn a_submicrosecond_remnant_merges_without_losing_time() {
        let dt = 1.0 / 120.;
        for split in [0., 0.4e-6, 1.01e-6, dt - 0.0002504, dt - 0.5e-6, dt, dt + 0.5e-6] {
            let slices = super::contact_intervals(dt, split);
            assert!(slices.iter().all(|&v| v == 0. || v >= 1.0e-6));
            assert!((slices.into_iter().sum::<f32>() - dt).abs() < 1.0e-9);
        }
    }
}
