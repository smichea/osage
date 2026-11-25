// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The solved district contact uses the same material voice, path search and ears as a resident object.
use super::{
    Component, Playback, Surface, Touch, Track, arrival, diffraction, excitation, frame,
    output::PcmFile,
    physical,
    transmission::{DirectPath, Geometry},
};
use crate::{object::AcousticGeometry, replay::Route};
use anyhow::Result;
use bevy::math::Vec3;
use osage_paris_soundscape::space::vec3;
use osage_paris_voices::RATE;
use serde_json::Value;
use std::{cell::RefCell, collections::BTreeSet, path::Path, rc::Rc};

#[derive(Clone)]
pub(crate) struct Contact {
    pub tick: u32,
    pub point: [f32; 3],
    pub speed: f32,
    pub surface: String,
    pub owner: Option<usize>,
    pub part: Option<usize>,
    pub substep: Option<(u32, u32)>,
    pub voice: Option<Voice>,
    pub measured: Option<(u64, &'static str)>,
}

impl Contact {
    pub(crate) fn emission(&self, hz: u32) -> u64 {
        if let Some(v) = &self.voice {
            return v.cohort.key.1;
        }
        if let Some((at, _)) = self.measured {
            return at;
        }
        let end = frame(self.tick, hz);
        if let Some((step, total)) = self.substep {
            let start = frame(self.tick.saturating_sub(1), hz);
            return start + (end - start) * u64::from(step) / u64::from(total);
        }
        end
    }
}

pub(crate) fn ray(contact: &Contact, hz: u32, route: &Route) -> ([f32; 3], [f32; 3]) {
    let (_, head, _) = arrival::receiver(route, hz, contact.emission(hz), Vec3::from(contact.point));
    (contact.point, head.at.array().map(|v| v as f32))
}

pub(crate) struct Queries {
    pub segments: Vec<([f32; 3], [f32; 3])>,
    pub blockers: BTreeSet<String>,
}

/// A late path can refer to the original source even after its attack was measured.
#[derive(Clone)]
pub(crate) struct Voice {
    cohort: physical::Cohort,
    seed: u32,
    evidence: Rc<RefCell<physical::Evidence>>,
}
impl Voice {
    pub(crate) fn complete(&self, frame: u64) -> bool {
        self.cohort.key.1 + excitation::WINDOW < frame
    }
    pub(crate) fn evidence(&self) -> Result<Value> {
        Ok(serde_json::to_value(&*self.evidence.borrow())?)
    }
}

pub(crate) struct Dynamics<'a> {
    pub parts: &'a [osage_dynamics::Part],
    pub boxes: &'a [osage_dynamics::Pose],
    pub box_index: &'a dyn Fn(usize) -> Option<usize>,
    pub before: &'a [osage_dynamics::Pose],
    pub contacts: &'a [osage_dynamics::Contact],
    pub impulses: &'a [osage_dynamics::ContactImpulse],
    pub substeps: u8,
}

pub(crate) struct Recording {
    track: Track,
    file: PcmFile,
    bundled_bridge: bool,
    measuring: Vec<Voice>,
}

fn geometry(mesh: &AcousticGeometry, bundled_bridge: bool) -> Geometry<'_> {
    Geometry { object: (!bundled_bridge).then_some(mesh), colliders: &[], bundled_bridge }
}

impl Recording {
    pub(crate) fn new(path: &Path, playback: Playback, bundled_bridge: bool) -> Result<Self> {
        let mut track = Track::empty(playback);
        track.retain_mix_boundaries();
        Ok(Self { track, file: PcmFile::new(path)?, bundled_bridge, measuring: Vec::new() })
    }

    pub(crate) fn reactions<'a>(
        &mut self,
        tick: u32,
        hz: u32,
        state: Dynamics<'_>,
        surface: impl Fn(usize) -> Option<(&'a str, Option<usize>)>,
    ) -> Vec<Contact> {
        let start = frame(tick - 1, hz);
        self.measuring.retain(|v| !v.complete(start));
        let step = physical::PhysicalStep {
            contacts: state.contacts,
            parts: state.parts,
            before: state.before,
            boxes: state.boxes,
            box_index: state.box_index,
            surface: &surface,
            from: start,
            to: frame(tick, hz),
        };
        let cohorts = self.measuring.iter().map(|v| v.cohort.clone()).collect();
        let mut starts = Vec::new();
        for mut r in step.responses(state.impulses, &mut self.track.previous, cohorts) {
            if let Some(key) = r.continuation {
                let voice = self.measuring.iter().find(|v| v.cohort.key == key).expect("known contact attack");
                let offset = r.touch.at - key.1;
                let gain = r.evidence.append_to(&mut voice.evidence.borrow_mut().excitation, offset, r.impulse_ns);
                for e in self.track.strikes.iter_mut().filter_map(|s| s.excitation.as_mut()).filter(|e| e.source == key)
                {
                    e.add(offset, gain);
                }
            } else {
                r.evidence.start(r.impulse_ns);
                let substep = r.evidence.substep();
                let voice = Voice {
                    cohort: physical::Cohort::from_evidence(r.surface, r.touch.at, &r.evidence),
                    seed: self.track.next_seed(r.touch.surface),
                    evidence: Rc::new(RefCell::new(r.evidence)),
                };
                self.measuring.push(voice.clone());
                starts.push(Contact {
                    tick,
                    point: r.touch.point.to_array(),
                    speed: r.touch.closing,
                    surface: r.touch.surface.to_owned(),
                    owner: surface(r.surface).and_then(|(_, id)| id),
                    part: r.touch.part,
                    substep: Some((u32::from(substep), u32::from(state.substeps))),
                    voice: Some(voice),
                    measured: None,
                });
            }
        }
        starts
    }

    pub(crate) fn capsule_reactions<'s>(
        &mut self,
        step: super::capsule::Step<'_>,
        box_index: impl Fn(usize) -> Option<usize>,
        surface: impl Fn(usize) -> Option<(&'s str, Option<usize>)>,
    ) -> Vec<Contact> {
        step.touches(&mut self.track.previous, box_index, |id| surface(id).map(|(name, _)| name))
            .into_iter()
            .map(|(id, hit)| Contact {
                tick: step.tick,
                point: hit.point.to_array(),
                speed: hit.closing,
                surface: hit.surface.to_owned(),
                owner: surface(id).and_then(|(_, node)| node),
                part: hit.part,
                substep: None,
                voice: None,
                measured: Some((hit.at, hit.basis)),
            })
            .collect()
    }

    /// Inspect every leg tried by the same optimizer that renders the voice. The paging
    /// loop requests unseen surfaces and repeats until all those rays are certified.
    pub(crate) fn queries(&self, mesh: &AcousticGeometry, c: &Contact, hz: u32, route: &Route) -> Queries {
        let emitted = c.emission(hz);
        let (arrival, head, _) = arrival::receiver(route, hz, emitted, Vec3::from(c.point));
        let [x, y, z] = c.point.map(f64::from);
        let source = vec3(x, y, z);
        let scene = geometry(mesh, self.bundled_bridge);
        let direct = DirectPath::trace(scene, &self.track.bridge, source, head.at, arrival as f64 / f64::from(RATE));
        let mut rays = BTreeSet::new();
        let mut blockers = BTreeSet::new();
        let mut visit =
            |a: osage_paris_soundscape::space::Vec3, b: osage_paris_soundscape::space::Vec3, path: &DirectPath| {
                rays.insert((a.array().map(|v| (v as f32).to_bits()), b.array().map(|v| (v as f32).to_bits())));
                for cross in &path.crossings {
                    if cross.transmission.is_some_and(|e| e.into_iter().any(|v| v < 1.)) {
                        blockers.insert(cross.id.clone());
                    }
                }
            };
        visit(source, head.at, &direct);
        diffraction::Search { scene, bridge: &self.track.bridge, route, hz, emitted, source }
            .observe(arrival, &direct, &mut visit);
        Queries {
            segments: rays.into_iter().map(|(a, b)| (a.map(f32::from_bits), b.map(f32::from_bits))).collect(),
            blockers,
        }
    }

    pub(crate) fn publish(
        &mut self,
        mesh: &AcousticGeometry,
        c: &Contact,
        owner: (Option<Vec<String>>, &str),
        hz: u32,
        route: &Route,
        edge: bool,
    ) -> Result<Value> {
        let before = self.track.strikes.len();
        let touch = Touch {
            point: Vec3::from(c.point),
            part: if c.voice.is_some() { c.part } else { c.part.or(Some(0)) },
            surface: &c.surface,
            closing: c.speed,
            at: c.emission(hz),
            basis: c
                .measured
                .map_or(if c.voice.is_some() { "solved impulse" } else { "district solver" }, |(_, basis)| basis),
        };
        let surface = Surface {
            geometry: geometry(mesh, self.bundled_bridge),
            material: owner.1,
            owner: owner.0,
            component: if edge { Component::Edge } else { Component::Direct },
        };
        let prepared = c.voice.as_ref().map(|v| (v.cohort.key.0, v.seed, v.evidence.borrow().clone()));
        let onset = self.track.strike_at(touch, prepared, c.tick, hz, route, surface);
        self.track.catch_up(before);
        Ok(serde_json::to_value(onset)?)
    }

    pub(crate) fn voice_end(&self, voice: &Voice) -> Option<u64> {
        self.track
            .strikes
            .iter()
            .filter(|s| s.excitation.as_ref().is_some_and(|e| e.source == voice.cohort.key))
            .map(|s| s.end)
            .max()
    }

    pub(crate) fn advance(&mut self, tick: u32, hz: u32) -> Result<()> {
        self.file.write(self.track.mix_until(frame(tick, hz)))
    }

    pub(crate) fn finish(mut self) -> Result<(u64, Option<u64>)> {
        if let Some(stop) = self.track.strikes.iter().map(|s| s.end).max() {
            while self.track.frame < stop {
                self.file.write(self.track.mix_until(stop.min(self.track.frame + 4096)))?;
            }
        }
        self.file.finish()
    }
}
