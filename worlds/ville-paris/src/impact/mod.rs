// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Contact onsets from the city solver, heard from the replay camera.
mod arrival;
pub(crate) mod capsule;
mod diffraction;
mod excitation;
mod listener;
mod output;
pub(crate) mod paged;
mod physical;
pub(crate) mod room_audio;
mod transmission;
use transmission::{DirectPath, Transmission};

use listener::Listener;
pub use osage_paris_soundscape::ears::Listening;
pub use output::{Live, record};

use crate::{
    physics::Actor,
    replay::{Grid, Replay, Route},
    scene::Sector,
};
use anyhow::Result;
use bevy::math::Vec3;
use osage_dynamics::Pose;
use osage_paris_soundscape::{
    ears::hear,
    scene::{Head, Scene},
    space::vec3,
    ways::SOUND,
};
use osage_paris_voices::{Drive, Hit, RATE, Voice, contact::Impact, dsp::seed_for};
use serde::Serialize;
use std::collections::{BTreeSet, VecDeque};

#[derive(Serialize)]
struct Onset {
    tick: u32,
    basis: &'static str,
    emission_frame: u64,
    arrival_frame: u64,
    ear_delay_frames: [f64; 2],
    direct_path: DirectPath,
    first_path_frame: Option<u64>,
    diffracted_path: Option<diffraction::EdgePath>,
    surface: String,
    owner_path: Option<Vec<String>>,
    part: Option<usize>,
    material: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    physical: Option<physical::Evidence>,
    point_m: [f32; 3],
    listener_at_arrival_m: [f64; 3],
    distance_m: f64,
    closing_m_s: f32,
}

struct Strike {
    voice: Impact,
    excitation: Option<excitation::Envelope>,
    arrival: u64,
    end: u64,
    hit: Option<Hit>,
    listener: Listener,
    transmission: Transmission,
}

#[derive(Clone, Copy)]
pub(crate) struct Playback {
    pub listening: Listening,
    pub direct_only: bool,
}

struct Track {
    playback: Playback,
    bridge: Scene,
    previous: BTreeSet<(usize, usize)>,
    pending: Vec<(physical::Cohort, Onset)>,
    physical_strikes: u64,
    mix_boundaries: Option<VecDeque<u64>>,
    impulses: Vec<osage_dynamics::ContactImpulse>,
    strikes: Vec<Strike>,
    parts: Vec<Pose>,
    boxes: Vec<Pose>,
    frame: u64,
}

fn frame(tick: u32, hz: u32) -> u64 {
    u64::from(tick) * u64::from(RATE) / u64::from(hz)
}

fn point_velocity(pose: &Pose, point: Vec3) -> Vec3 {
    Vec3::from(pose.linear_velocity) + Vec3::from(pose.angular_velocity).cross(point - Vec3::from(pose.center))
}

#[derive(Clone, Copy)]
struct Touch<'a> {
    point: Vec3,
    part: Option<usize>,
    surface: &'a str,
    closing: f32,
    at: u64,
    basis: &'static str,
}

#[derive(Clone, Copy, PartialEq)]
enum Component {
    Direct,
    Edge,
    Both,
}

struct Surface<'a> {
    component: Component,
    geometry: transmission::Geometry<'a>,
    material: &'a str,
    owner: Option<Vec<String>>,
}

impl Track {
    fn new(playback: Playback, scene: &Sector) -> Self {
        if let Some(object) = &scene.object {
            if playback.direct_only {
                object.sound.prepare_acoustics(&scene.colliders);
            } else {
                object.sound.prepare_solid_edges(&scene.colliders);
            }
        }
        Self::empty(playback)
    }

    fn empty(playback: Playback) -> Self {
        Self {
            playback,
            bridge: paris_soundscape::bridge(),
            previous: Default::default(),
            pending: Vec::new(),
            physical_strikes: 0,
            mix_boundaries: None,
            impulses: Vec::new(),
            strikes: Vec::new(),
            parts: Vec::new(),
            boxes: Vec::new(),
            frame: 0,
        }
    }

    fn strike(
        &mut self,
        t: Touch<'_>,
        physical: Option<(usize, physical::Evidence)>,
        tick: u32,
        hz: u32,
        scene: &Sector,
        route: &Route,
    ) -> Onset {
        let owner = scene.object.as_ref().and_then(|obj| obj.collision_path(t.surface));
        let key = scene.object.as_ref().and_then(|obj| obj.contact_material(t.surface)).unwrap_or(match t.surface {
            "rail" | "bonnet" => "steel-plate",
            "road" => "asphalt",
            _ => "default",
        });
        let prepared = physical.map(|(surface, evidence)| (surface, self.next_seed(t.surface), evidence));
        self.strike_at(
            t,
            prepared,
            tick,
            hz,
            route,
            Surface {
                component: Component::Both,
                geometry: scene.into(),
                material: key,
                owner: owner.map(|p| p.into_iter().map(str::to_owned).collect()),
            },
        )
    }

    fn next_seed(&mut self, surface: &str) -> u32 {
        let seed = seed_for(self.physical_strikes, surface);
        self.physical_strikes += 1;
        seed
    }

    fn strike_at(
        &mut self,
        t: Touch<'_>,
        physical: Option<(usize, u32, physical::Evidence)>,
        tick: u32,
        hz: u32,
        route: &Route,
        surface: Surface<'_>,
    ) -> Onset {
        let key = surface.material;
        let scene = surface.geometry;
        let sound = self.bridge.contacts.get(key).filter(|s| s.voice == "impact");
        let mut params = sound.map(|s| s.params.clone()).unwrap_or_default();
        let sole = self.bridge.contacts.get("rubber-sole").map_or(0.3, |s| s.hardness);
        params.set("hardness", sound.map_or(sole, |s| s.hardness.min(sole)) as f32);
        let ring = params.get("ring", 0.06).clamp(0.005, 4.);
        let (arrival, head, distance) = arrival::receiver(route, hz, t.at, t.point);
        let at = vec3(f64::from(t.point.x), f64::from(t.point.y), f64::from(t.point.z));
        let source_gain = sound.map_or(1., |s| s.gain);
        let (seed, source) = match &physical {
            Some((surface, seed, evidence)) => (*seed, Some(((*surface, t.at), &evidence.excitation))),
            None => (seed_for(t.at ^ t.part.map_or(u64::MAX, |i| i as u64), t.surface), None),
        };
        let tail_frames = ((ring.max(0.06) * 1.2 + 0.02) * RATE as f32).ceil() as u64;
        let direct_path = DirectPath::trace(scene, &self.bridge, at, head.at, arrival as f64 / f64::from(RATE));
        let mut diffracted_path = if self.playback.direct_only || surface.component == Component::Direct {
            None
        } else {
            diffraction::Search { scene, bridge: &self.bridge, route, hz, emitted: t.at, source: at }
                .best(arrival, &direct_path)
        };
        let mut add = |frame: u64, heard: osage_paris_soundscape::ears::Heard, distance: f64, energy: [f64; 3]| {
            self.strikes.push(Strike {
                voice: Impact::new(seed, &params),
                excitation: source.map(|(id, excitation)| {
                    let mut envelope = excitation::Envelope::new(id, excitation.samples[0].amplitude);
                    for sample in excitation.samples.iter().skip(1) {
                        envelope.add(sample.offset_frames, sample.amplitude);
                    }
                    envelope
                }),
                arrival: frame,
                end: frame.saturating_add(tail_frames),
                hit: Some(Hit { at: 0, strength: if source.is_some() { 1. } else { t.closing / (3. + t.closing) } }),
                listener: Listener::new(self.playback.listening, heard, source_gain / distance.max(1.)),
                transmission: Transmission::new(energy),
            });
            match self.playback.listening {
                Listening::Speakers => [0.; 2],
                Listening::Headphones => heard.delay.map(|d| d * f64::from(RATE)),
            }
        };
        let heard = hear(at, &head);
        let ear_delay_frames = if surface.component != Component::Edge {
            add(arrival, heard, distance, direct_path.transmission_energy)
        } else if self.playback.listening == Listening::Headphones {
            heard.delay.map(|d| d * f64::from(RATE))
        } else {
            [0.; 2]
        };
        let mut first_path_frame = (surface.component != Component::Edge
            && direct_path.transmission_energy.iter().any(|v| *v > 0.))
        .then_some(arrival);
        if let Some(path) = &mut diffracted_path {
            let [x, y, z] = path.point_m;
            path.ear_delay_frames = add(
                path.arrival_frame,
                hear(vec3(x, y, z), &path.head),
                path.reference_distance_m,
                path.received_energy,
            );
            first_path_frame = Some(first_path_frame.map_or(path.arrival_frame, |f| f.min(path.arrival_frame)));
        }
        Onset {
            tick,
            basis: t.basis,
            emission_frame: t.at,
            arrival_frame: arrival,
            ear_delay_frames,
            direct_path,
            first_path_frame,
            diffracted_path,
            surface: t.surface.into(),
            owner_path: surface.owner,
            part: t.part,
            physical: physical.map(|(_, _, evidence)| evidence),
            material: if sound.is_some() { key } else { "default" }.into(),
            point_m: t.point.to_array(),
            listener_at_arrival_m: head.at.array(),
            distance_m: distance,
            closing_m_s: t.closing,
        }
    }

    fn step(
        &mut self,
        actor: &mut Actor,
        tick: u32,
        replay: &Replay,
        route: &Route,
        scene: &Sector,
        grid: &Grid,
    ) -> Result<(Vec<f32>, Vec<Onset>)> {
        let mut onsets = Vec::new();
        if tick == 1 {
            self.pending.clear();
            self.physical_strikes = 0;
        }
        let start = frame(tick - 1, replay.tick_hz);
        let stop = frame(tick, replay.tick_hz);
        let prior = match actor {
            Actor::Point(body) => Some(*body),
            Actor::Rigid(sim, _) => {
                self.parts.clone_from(&sim.part_poses);
                self.boxes.clone_from(&sim.box_poses);
                None
            }
        };
        if let Actor::Rigid(sim, parts) = actor
            && !parts.is_empty()
        {
            sim.step_with_impulses(&mut self.impulses)?;
        } else {
            actor.advance(tick, replay, scene, grid)?;
        }
        match actor {
            Actor::Point(body) => {
                if let Some((when, i)) = body.landed
                    && when == tick
                {
                    let before = prior.expect("point actor");
                    let speed =
                        (-(before.velocity.y - replay.gravity_m_s2 / replay.tick_hz as f32)).min(replay.terminal_m_s);
                    if speed > 0.5 {
                        let seconds = (before.foot.y - body.foot.y) / speed;
                        let at = start
                            + (f64::from(seconds) * f64::from(RATE)).round().clamp(0., (stop - start) as f64) as u64;
                        onsets.push(self.strike(
                            Touch {
                                point: body.foot,
                                part: Some(0),
                                surface: &scene.supports[i].id,
                                closing: speed,
                                at,
                                basis: "crossing",
                            },
                            None,
                            tick,
                            replay.tick_hz,
                            scene,
                            route,
                        ));
                    }
                }
            }
            Actor::Rigid(sim, parts) if !parts.is_empty() => {
                let step = physical::PhysicalStep {
                    contacts: &sim.contacts,
                    parts,
                    before: &self.parts,
                    boxes: &self.boxes,
                    box_index: &|id| id.checked_sub(scene.colliders.len()),
                    surface: &|i| route.surface_name(&scene.colliders, i).map(|id| (id, None)),
                    from: start,
                    to: stop,
                };
                let cohorts = self.pending.iter().map(|(c, _)| c.clone()).collect();
                let impacts = step.responses(&self.impulses, &mut self.previous, cohorts);
                for mut r in impacts {
                    if let Some(key) = r.continuation {
                        let onset = &mut self.pending.iter_mut().find(|(c, _)| c.key == key).unwrap().1;
                        let offset = r.touch.at - key.1;
                        let envelope = &mut onset.physical.as_mut().unwrap().excitation;
                        let amplitude = r.evidence.append_to(envelope, offset, r.impulse_ns);
                        for path in
                            self.strikes.iter_mut().filter_map(|s| s.excitation.as_mut()).filter(|e| e.source == key)
                        {
                            path.add(offset, amplitude);
                        }
                    } else {
                        r.evidence.start(r.impulse_ns);
                        let cohort = physical::Cohort::from_evidence(r.surface, r.touch.at, &r.evidence);
                        let onset =
                            self.strike(r.touch, Some((r.surface, r.evidence)), tick, replay.tick_hz, scene, route);
                        self.pending.push((cohort, onset));
                    }
                }
                // Only receipt publication needs the full attack. Every path already renders at
                // its original retarded arrival time, including on a listener at the source.
                let mut i = 0;
                while i < self.pending.len() {
                    if self.pending[i].0.key.1 + excitation::WINDOW < stop || tick == route.ticks {
                        onsets.push(self.pending.remove(i).1);
                    } else {
                        i += 1;
                    }
                }
            }
            Actor::Rigid(sim, _) => {
                let step = capsule::Step {
                    tick,
                    hz: replay.tick_hz,
                    substeps: sim.substeps_per_tick(),
                    before: &self.parts,
                    after: &sim.part_poses,
                    prior_boxes: &self.boxes,
                    boxes: &sim.box_poses,
                    contacts: &sim.contacts,
                    swept: sim.swept_impact_offset_s,
                };
                let hits = step.touches(
                    &mut self.previous,
                    |id| id.checked_sub(scene.colliders.len()),
                    |id| route.surface_name(&scene.colliders, id),
                );
                for (_, hit) in hits {
                    onsets.push(self.strike(hit, None, tick, replay.tick_hz, scene, route));
                }
            }
        }
        Ok((self.mix_until(stop), onsets))
    }

    /// The paged renderer can materialize a path after the receiver has passed its arrival.
    fn retain_mix_boundaries(&mut self) {
        self.mix_boundaries.get_or_insert_with(VecDeque::new);
    }

    /// A newly available path might already be audible at the current receiver time. Advance
    /// its source, material filters and ears through the missed prefix without replaying it.
    fn catch_up(&mut self, first_new_strike: usize) {
        let mut mono = Vec::new();
        let mut discarded = Vec::new();
        let mut index = first_new_strike;
        while index < self.strikes.len() {
            let strike = &mut self.strikes[index];
            if strike.end <= self.frame {
                self.strikes.remove(index);
                continue;
            }
            let arrival = strike.arrival;
            if arrival < self.frame {
                let boundaries =
                    self.mix_boundaries.as_ref().expect("record receiver blocks before publishing late paths");
                let first = boundaries.partition_point(|&end| end <= arrival);
                let mut from = arrival;
                for &stop in boundaries.iter().skip(first) {
                    // The modal voice retires decayed modes at the end of each render call.
                    // Replaying the receiver's real block boundaries preserves its exact tail.
                    discarded.resize((stop - from) as usize, [0.; 2]);
                    discarded.fill([0.; 2]);
                    strike.render(from, &mut mono, &mut discarded);
                    from = stop;
                }
                debug_assert_eq!(from, self.frame);
            }
            index += 1;
        }
    }

    fn mix_until(&mut self, stop: u64) -> Vec<f32> {
        let mut out = vec![0.; 2 * (stop - self.frame) as usize];
        let mut mono = Vec::new();
        for s in &mut self.strikes {
            let from = self.frame.max(s.arrival);
            let until = stop.min(s.end);
            if from >= until {
                continue;
            }
            let offset = 2 * (from - self.frame) as usize;
            let count = 2 * (until - from) as usize;
            s.render(from, &mut mono, out[offset..offset + count].as_chunks_mut::<2>().0);
        }
        self.strikes.retain(|s| s.end > stop);
        if stop > self.frame
            && let Some(boundaries) = &mut self.mix_boundaries
        {
            // The longest material rings for 4 s; each path's scheduled synthesis tail is below 5 s.
            let earliest = stop.saturating_sub(5 * u64::from(RATE));
            while boundaries.front().is_some_and(|&end| end <= earliest) {
                boundaries.pop_front();
            }
            boundaries.push_back(stop);
        }
        self.frame = stop;
        for x in &mut out {
            *x = x.clamp(-1., 1.);
        }
        out
    }
}

impl Strike {
    fn render(&mut self, from: u64, mono: &mut Vec<f32>, output: &mut [[f32; 2]]) {
        mono.resize(output.len(), 0.);
        self.voice.render(&Drive { hit: self.hit.take(), ..Default::default() }, mono);
        if let Some(envelope) = &mut self.excitation {
            envelope.apply(from - self.arrival, mono);
        }
        self.transmission.apply(mono);
        self.listener.add(mono, output);
    }
}

#[cfg(test)]
mod late_tests {
    use super::*;
    use osage_paris_soundscape::ears::Heard;
    use osage_paris_voices::Params;

    fn path(listening: Listening, arrival: u64, end: u64, seed: u32) -> Strike {
        let heard = Heard {
            delay: [0.00024, 0.00003],
            shadow: [0.45, 1.6],
            pan: [0.37, 0.92],
            pinna: [0.00003, 0.00007, 0.00012, 0.00019, 0.00025],
        };
        let mut params = Params::default();
        params.set("ring", 0.1);
        let mut envelope = excitation::Envelope::new((1, 0), 0.41);
        envelope.add(52, 0.62);
        envelope.add(169, 0.83);
        Strike {
            voice: Impact::new(seed, &params),
            excitation: Some(envelope),
            arrival,
            end,
            hit: Some(Hit { at: 0, strength: 1. }),
            listener: Listener::new(listening, heard, 0.5),
            transmission: Transmission::new([0.71, 0.43, 0.24]),
        }
    }

    #[test]
    fn late_paths_join_at_the_same_samples_including_material_and_ear_histories() {
        let scene = Sector::read(crate::scene::DEFAULT).unwrap();
        for listening in [Listening::Speakers, Listening::Headphones] {
            let playback = Playback { listening, direct_only: false };
            for cut in [50, 371, 465, 1299, 5093, 6031, 6257, 6999, 11000] {
                let mut normal = Track::new(playback, &scene);
                let mut late = Track::new(playback, &scene);
                late.retain_mix_boundaries();
                for t in [&mut normal, &mut late] {
                    t.strikes.push(path(listening, 0, 16500, 12));
                }
                normal.strikes.push(path(listening, 303, 10400, 42));
                for t in (193..cut).step_by(193).chain(std::iter::once(cut)) {
                    normal.mix_until(t);
                    late.mix_until(t);
                }
                let first = late.strikes.len();
                late.strikes.push(path(listening, 303, 10400, 42));
                late.catch_up(first);
                assert_eq!(late.strikes.len(), if cut >= 10400 { 1 } else { 2 });
                for end in (cut + 113..20000).step_by(719) {
                    let resident = normal.mix_until(end);
                    let published = late.mix_until(end);
                    assert!(resident == published, "{listening:?}, late publication at {cut}, receiver through {end}");
                }
            }
        }
    }
}
