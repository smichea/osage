// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::{
    Touch,
    excitation::{Excitation, WINDOW},
    point_velocity,
};
#[cfg(test)]
use crate::scene::Sector;
use bevy::math::Vec3;
#[cfg(test)]
use osage_dynamics::Simulation;
use osage_dynamics::{Contact, ContactBodies, ContactImpulse, ImpulseAttribution, ImpulseMeasurement, Part, Pose};
use osage_paris_voices::RATE;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Serialize)]
pub(super) struct Member {
    part: usize,
    name: String,
}

#[derive(Clone, Serialize)]
pub(super) struct Evidence {
    /// All records apply to the same material surface and end in the same solver advance.
    attribution: &'static str,
    position_basis: &'static str,
    impulse_basis: &'static str,
    members: Vec<Member>,
    measurements: Vec<ContactImpulse>,
    pub(super) excitation: Excitation,
}

impl Evidence {
    pub fn substep(&self) -> u8 {
        self.measurements[0].substep
    }

    pub fn start(&mut self, impulse: f32) -> f32 {
        self.excitation.add(
            0,
            impulse,
            self.attribution,
            self.members.iter().map(|p| p.part).collect(),
            self.impulse_basis,
        )
    }

    pub fn append_to(&self, target: &mut Excitation, offset: u64, impulse: f32) -> f32 {
        target.add(offset, impulse, self.attribution, self.members.iter().map(|p| p.part).collect(), self.impulse_basis)
    }
}

#[derive(Clone)]
pub(super) struct Cohort {
    pub key: (usize, u64),
    members: Vec<usize>,
}
impl Cohort {
    pub fn from_evidence(surface: usize, at: u64, evidence: &Evidence) -> Self {
        Self { key: (surface, at), members: evidence.members.iter().map(|m| m.part).collect() }
    }
    fn owns(&self, at: u64, candidates: &[ContactBodies]) -> bool {
        at <= self.key.1 + WINDOW
            && candidates.iter().all(|p| p.surface == self.key.0 && self.members.contains(&p.part))
    }
}

pub(super) struct Reaction<'a> {
    pub touch: Touch<'a>,
    pub evidence: Evidence,
    pub surface: usize,
    pub impulse_ns: f32,
    /// A continuation of a material voice whose onset is already known.
    pub continuation: Option<(usize, u64)>,
}

fn pairs(r: &ContactImpulse) -> &[ContactBodies] {
    match &r.attribution {
        ImpulseAttribution::Pair(p) => std::slice::from_ref(p),
        ImpulseAttribution::Unresolved(p) => p,
    }
}

struct Witness {
    point: Vec3,
    normal: Vec3,
}

fn witnesses(contacts: &[Contact], substep: u8, body: ContactBodies) -> Vec<Witness> {
    let mut found = Vec::new();
    for c in contacts.iter().filter(|c| c.substep == substep && c.part == body.part && c.surface == body.surface) {
        let point = Vec3::from(c.point);
        let normal = Vec3::from(c.normal);
        if !found.iter().any(|w: &Witness| w.point == point && w.normal == normal) {
            found.push(Witness { point, normal });
        }
    }
    found.sort_by_key(|w| (w.point.to_array().map(f32::to_bits), w.normal.to_array().map(f32::to_bits)));
    found
}

pub(super) struct PhysicalStep<'a, 'scene> {
    pub contacts: &'a [Contact],
    pub parts: &'a [Part],
    pub before: &'a [Pose],
    pub boxes: &'a [Pose],
    pub box_index: &'a dyn Fn(usize) -> Option<usize>,
    pub surface: &'a dyn Fn(usize) -> Option<(&'scene str, Option<usize>)>,
    pub from: u64,
    pub to: u64,
}

impl<'scene> PhysicalStep<'_, 'scene> {
    pub fn responses(
        &self,
        records: &[ContactImpulse],
        active: &mut BTreeSet<(usize, usize)>,
        mut cohorts: Vec<Cohort>,
    ) -> Vec<Reaction<'scene>> {
        let mut present: BTreeSet<_> = self.contacts.iter().map(|c| (c.surface, c.part)).collect();
        let mut batches = BTreeMap::new();
        for rec in records {
            let candidates = pairs(rec);
            present.extend(candidates.iter().map(|p| (p.surface, p.part)));
            if let Some(first) = candidates.first()
                && (rec.pair().is_some() || candidates.len() > 1)
                && candidates.iter().all(|p| p.surface == first.surface)
                && let Some((name, owner)) = (self.surface)(first.surface)
            {
                // File arrival can swap local surface slots. Order simultaneous attacks by authored identity.
                let key = (rec.end_offset_s.to_bits(), name, owner, first.surface);
                batches.entry(key).or_insert_with(Vec::new).push(rec);
            }
        }
        active.retain(|p| present.contains(p));
        let mut result = Vec::new();
        for ((end, name, _, surface), batch) in batches {
            let at = (self.from + (f64::from(f32::from_bits(end)) * f64::from(RATE)).round() as u64).min(self.to);
            let mut groups = BTreeMap::new();
            for rec in batch {
                let bodies = pairs(rec);
                let already = cohorts.iter().rev().find(|c| c.owns(at, bodies)).map(|c| c.key);
                // A balance spanning both an existing contact and unrelated candidates cannot
                // be divided into a continued load and a new impact.
                if already.is_some() || bodies.iter().all(|p| !active.contains(&(p.surface, p.part))) {
                    groups.entry(already).or_insert_with(Vec::new).push(rec);
                }
            }
            for (continuation, mut records) in groups {
                // Solver handles can permute otherwise identical measurements from the two feet.
                // Fix their order before accumulating weighted positions, speeds and impulse in f32.
                records.sort_by(|a, b| {
                    pairs(a).cmp(pairs(b)).then_with(|| {
                        let key = |r: &ContactImpulse| {
                            let load = match r.measurement {
                                ImpulseMeasurement::SolverNormal(j) => (0, [j, 0., 0.]),
                                ImpulseMeasurement::AppliedVector(v) => (1, v),
                                ImpulseMeasurement::MomentumBalance(v) => (2, v),
                            };
                            (
                                r.substep,
                                r.start_offset_s.to_bits(),
                                r.point.map(|v| v.map(f32::to_bits)),
                                r.normal.map(|v| v.map(f32::to_bits)),
                                load.0,
                                load.1.map(f32::to_bits),
                            )
                        };
                        key(a).cmp(&key(b))
                    })
                });
                if let Some(hit) = self.hit(surface, name, at, &records, continuation) {
                    if continuation.is_none() {
                        active.extend(hit.evidence.members.iter().map(|p| (surface, p.part)));
                        cohorts.push(Cohort::from_evidence(surface, at, &hit.evidence));
                    }
                    result.push(hit);
                }
            }
        }
        result
    }

    fn hit(
        &self,
        surface_id: usize,
        name: &'scene str,
        at: u64,
        records: &[&ContactImpulse],
        continuation: Option<(usize, u64)>,
    ) -> Option<Reaction<'scene>> {
        let boxed = (self.box_index)(surface_id).and_then(|i| self.boxes.get(i));
        let mut weighted_point = Vec3::ZERO;
        let mut speed_times_impulse = 0.;
        let mut total = 0.;
        let mut geometric = false;
        let mut vector_magnitude = false;
        let mut measured_parts = BTreeSet::new();
        let mut used = Vec::new();
        for &rec in records {
            let candidates = pairs(rec);
            let mut point = Vec3::ZERO;
            let mut closing = 0.;
            let mut normals = Vec::new();
            for &body in candidates {
                let samples = match (rec.point, rec.normal) {
                    (Some(p), Some(n)) => vec![Witness { point: p.into(), normal: n.into() }],
                    _ => {
                        geometric = true;
                        witnesses(self.contacts, rec.substep, body)
                    }
                };
                if samples.is_empty() {
                    return None;
                }
                let mut member_point = Vec3::ZERO;
                let mut member_speed = 0.0_f32;
                for w in &samples {
                    let other = boxed.map_or(Vec3::ZERO, |pose| point_velocity(pose, w.point));
                    let speed = (other - point_velocity(&self.before[body.part], w.point)).dot(w.normal);
                    member_speed = member_speed.max(speed);
                    member_point += w.point;
                    normals.push(w.normal);
                }
                closing += member_speed;
                point += member_point / samples.len() as f32;
            }
            let normal = normals[0];
            let impulse = match rec.measurement {
                ImpulseMeasurement::SolverNormal(j) => j,
                ImpulseMeasurement::AppliedVector(v) | ImpulseMeasurement::MomentumBalance(v) => {
                    let vector = Vec3::from(v);
                    if normals.iter().all(|&n| normal.dot(n) > 0.9999) {
                        vector.dot(normal)
                    } else if normals.iter().any(|&n| vector.dot(n) > 0.) {
                        // The net reaction has several candidate normals on the same support.
                        // The vector is measured; its distribution between these directions is unknown.
                        vector_magnitude = true;
                        vector.length()
                    } else {
                        0.
                    }
                }
            };
            if impulse <= 0. {
                continue;
            }
            let count = candidates.len() as f32;
            weighted_point += point * (impulse / count);
            speed_times_impulse += closing * (impulse / count);
            total += impulse;
            measured_parts.extend(candidates.iter().map(|p| p.part));
            used.push(rec.clone());
        }
        // Reject weight-support chatter and approach without a measured reaction. Closing speed
        // uses the same public-tick input poses and units as the existing impact voice.
        if total <= 0. || (continuation.is_none() && (total <= 0.1 || speed_times_impulse <= 0.5 * total)) {
            return None;
        }
        let unresolved = used.iter().any(|r| r.pair().is_none());
        let part = (measured_parts.len() == 1 && !unresolved).then(|| *measured_parts.first().unwrap());
        let members = measured_parts.iter().map(|&i| Member { part: i, name: self.parts[i].name.clone() }).collect();
        let attribution = if unresolved { "unresolved" } else { "resolved" };
        let pos = if geometric {
            "geometric witness mean"
        } else if used.len() == 1 {
            "solver point"
        } else {
            "impulse-weighted solver points"
        };
        let closing = speed_times_impulse / total;
        Some(Reaction {
            touch: Touch { point: weighted_point / total, part, surface: name, closing, at, basis: "solved impulse" },
            evidence: Evidence {
                attribution,
                position_basis: pos,
                impulse_basis: if vector_magnitude { "net vector magnitude" } else { "normal impulse" },
                members,
                measurements: used,
                excitation: Excitation::new(closing),
            },
            surface: surface_id,
            impulse_ns: total,
            continuation,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        impact::{Listening, Playback, Track},
        people::Model,
        physics::{Actor, Physics},
        replay::{Grid, Replay},
    };
    use osage_dynamics::Engine;

    #[test]
    fn simultaneous_feet_keep_their_solver_attribution_in_the_material_voice() {
        let scene = Sector::read(crate::scene::DEFAULT).unwrap();
        let replay =
            Replay::read(include_bytes!("../../fixtures/bonnet.json"), scene.colliders.iter().map(|s| s.id.as_str()))
                .unwrap();
        let route = replay.routes.iter().find(|r| r.id == "moving").unwrap();
        let grid = Grid::new(&scene.supports);
        for engine in [Engine::Rapier, Engine::Jolt] {
            let mut actor = Actor::new(Physics::Rigid(engine), &Model::Human, &scene, &replay, route).unwrap();
            let mut track = Track::new(Playback { listening: Listening::Speakers, direct_only: true }, &scene);
            let mut first = None;
            for tick in 1..=40 {
                let (_, events) = track.step(&mut actor, tick, &replay, route, &scene, &grid).unwrap();
                if let Some(hit) = events.into_iter().next() {
                    first = Some(hit);
                    break;
                }
            }
            let hit = first.unwrap();
            assert_eq!((&*hit.surface, &*hit.material, hit.part), ("bonnet", "steel-plate", None));
            let data = hit.physical.unwrap();
            assert_eq!(data.attribution, if engine == Engine::Rapier { "resolved" } else { "unresolved" });
            assert_eq!(
                data.members.iter().map(|m| m.name.as_str()).collect::<BTreeSet<_>>(),
                BTreeSet::from(["foot_l", "foot_r"])
            );
            for r in data.measurements {
                let frame = crate::impact::frame(hit.tick - 1, replay.tick_hz)
                    + (f64::from(r.end_offset_s) * f64::from(RATE)).round() as u64;
                assert_eq!(frame, hit.emission_frame);
                assert_eq!(r.pair().is_some(), engine == Engine::Rapier);
            }
        }
    }
    #[test]
    fn at_the_same_incoming_speed_a_heavier_rig_drives_a_louder_first_blow() {
        use crate::physics::CAPSULE;
        use osage_dynamics::{RigStart, Settings};
        let scene = Sector::read(crate::scene::DEFAULT).unwrap();
        let replay =
            Replay::read(include_bytes!("../../fixtures/bonnet.json"), scene.colliders.iter().map(|s| s.id.as_str()))
                .unwrap();
        let route = replay.routes.iter().find(|r| r.id == "moving").unwrap();
        let grid = Grid::new(&scene.supports);
        let settings = Settings {
            tick_hz: replay.tick_hz,
            gravity: replay.gravity_m_s2,
            terminal_speed: replay.terminal_m_s,
            capsule: CAPSULE,
        };
        for engine in [Engine::Jolt, Engine::Rapier] {
            let mut readings = Vec::new();
            for mass_scale in [0.25, 1., 4.] {
                let mut parts = crate::people::human();
                for p in &mut parts {
                    p.mass *= mass_scale;
                    p.inertia = p.inertia.map(|i| i * mass_scale);
                }
                let start = RigStart { origin_m: route.start_m, velocity_m_s: route.velocity_m_s, animation: None };
                let sim =
                    Simulation::from_rig(engine, &scene.colliders, route.bonnet.as_slice(), settings, &parts, start)
                        .unwrap();
                let mut actor = Actor::Rigid(Box::new(sim), parts);
                let mut track = Track::new(Playback { listening: Listening::Speakers, direct_only: true }, &scene);
                let mut samples = Vec::new();
                let mut hits = Vec::new();
                for tick in 1..=40 {
                    let (block, events) = track.step(&mut actor, tick, &replay, route, &scene, &grid).unwrap();
                    samples.extend(block);
                    hits.extend(events);
                }
                let first = &hits[0];
                let charge = &first.physical.as_ref().unwrap().excitation;
                let gain = charge.samples.last().unwrap().amplitude;
                let from = 2 * first.arrival_frame as usize;
                let energy: f32 = samples[from..from + 2 * 960].iter().map(|v| v * v).sum();
                readings.push((first.closing_m_s, gain, energy));
            }
            for pair in readings.windows(2) {
                let [(v1, gain1, energy1), (v2, gain2, energy2)] = pair else { unreachable!() };
                assert!((v1 - v2).abs() < 0.001 && gain1 < gain2 && energy1 < energy2, "{engine} {pair:?}");
            }
        }
    }
}
