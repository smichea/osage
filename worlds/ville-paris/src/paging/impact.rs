// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Acoustic content for a moving actor. Predictions can request geometry. Only solver contacts can start voices.
use super::{
    Options,
    channels::{
        Profile,
        resident::{self, Bindings, Catalog, Page},
    },
    link::Link,
    reader::transfer_ticks,
    solver::World,
    source::{Source, Ticket},
    tiles::Index,
};
use crate::{
    impact::{
        Playback,
        paged::{Contact, Dynamics, Recording, Voice, ray},
    },
    object::AcousticBinding,
    replay::Route,
};
use anyhow::{Result, ensure};
use osage_paris_voices::RATE;
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

struct Lane {
    hz: u32,
    link: Link,
    mbps: f64,
    delay_ms: f64,
    free: u32,
    immediate: bool,
}

impl Lane {
    fn reserve(&mut self, tick: u32, slot: usize, bytes: usize) -> u32 {
        if self.immediate {
            return tick;
        }
        let hz = self.hz;
        let ms = self.delay_ms;
        let latency = if self.link.jitter_ms == 0 {
            (ms * f64::from(hz) / 1000.).ceil() as u32
        } else {
            self.link.latency_ticks(ms.ceil() as u32, hz, tick, slot as u64, false)
        };
        let start = self.free.max(self.link.advance(tick, latency));
        self.free = self.link.advance(start, transfer_ticks(bytes, hz, self.mbps));
        self.free
    }
}

#[derive(Serialize)]
struct Read {
    file: String,
    bytes: usize,
    requested_tick: u32,
    due_tick: u32,
    ready_tick: Option<u32>,
    rejected: Option<String>,
    cancelled_tick: Option<u32>,
}

struct InFlight {
    receipt: usize,
    ticket: Option<Ticket>,
}

#[derive(Default)]
struct Pages {
    pending: BTreeMap<usize, InFlight>,
    decoded: BTreeMap<usize, Page>,
    placed: BTreeMap<usize, AcousticBinding>,
    failures: BTreeMap<usize, String>,
    reads: Vec<Read>,
    peak: usize,
}

enum Status {
    Wait,
    Ready,
    Failed(String),
}

#[derive(Clone, Copy)]
struct Stage {
    tick: u32,
    at_emission: bool,
}

impl Pages {
    fn prepare(
        &mut self,
        wanted: &Bindings,
        catalog: &Catalog,
        profile: &Profile,
        source: &Source,
        stage: Stage,
        lane: &mut Lane,
    ) -> Status {
        let tick = stage.tick;
        let mut missing = false;
        for &id in wanted {
            if self.placed.contains_key(&id) {
                continue;
            }
            let slot = catalog.resource(profile, id);
            let file = profile.file(slot);
            if let Some(reason) = self.failures.get(&slot) {
                return Status::Failed(reason.clone());
            }
            if !self.decoded.contains_key(&slot) {
                if stage.at_emission && (!lane.immediate || source.paced()) {
                    missing = true;
                    continue;
                }
                if let std::collections::btree_map::Entry::Vacant(entry) = self.pending.entry(slot) {
                    let Ok(ticket) = source.start(file, false) else {
                        missing = true;
                        continue;
                    };
                    let due_tick = lane.reserve(tick, slot, file.bytes);
                    entry.insert(InFlight { receipt: self.reads.len(), ticket });
                    self.reads.push(Read {
                        file: file.file.clone(),
                        bytes: file.bytes,
                        requested_tick: tick,
                        due_tick,
                        ready_tick: None,
                        rejected: None,
                        cancelled_tick: None,
                    });
                }
                let flight = &self.pending[&slot];
                let log = &mut self.reads[flight.receipt];
                if tick < log.due_tick || flight.ticket.as_ref().is_some_and(|t| !t.ready()) {
                    missing = true;
                    continue;
                }
                let received = file.received(source, flight.ticket.as_ref()).and_then(|bytes| Page::decode(&bytes));
                self.pending.remove(&slot);
                match received {
                    Ok(page) => {
                        self.decoded.insert(slot, page);
                        log.ready_tick = Some(tick);
                        self.peak = self.peak.max(self.decoded.len());
                    }
                    Err(error) => {
                        let reason = format!("{}: {error:#}", file.file);
                        log.rejected = Some(reason.clone());
                        self.failures.insert(slot, reason.clone());
                        return Status::Failed(reason);
                    }
                }
            }
            match catalog.place(profile, id, &self.decoded[&slot]) {
                Ok(binding) => {
                    self.placed.insert(id, binding);
                }
                Err(error) => return Status::Failed(format!("placing {}: {error:#}", file.file)),
            }
        }
        if missing { Status::Wait } else { Status::Ready }
    }

    fn retain(&mut self, slots: &BTreeSet<usize>, bindings: &Bindings, tick: u32) {
        self.placed.retain(|id, _| bindings.contains(id));
        self.decoded.retain(|id, _| slots.contains(id));
        self.pending.retain(|id, flight| {
            if slots.contains(id) {
                true
            } else {
                self.reads[flight.receipt].cancelled_tick = Some(tick);
                false
            }
        });
        self.failures.retain(|slot, _| slots.contains(slot));
    }
}

struct Plan {
    contact: Contact,
    wanted: Bindings,
    direct: bool,
    complete: bool,
    receipt: Option<usize>,
}

pub(super) struct Audio {
    catalog: Option<Catalog>,
    lane: Lane,
    playback: Playback,
    recording: Recording,
    forecast: bool,
    predicted: BTreeMap<usize, Plan>,
    contacts: Vec<Plan>,
    journal: Vec<Value>,
    evidence: Vec<(usize, Voice)>,
    sounding: BTreeMap<usize, u64>,
    pages: Pages,
    tick: u32,
}

impl Audio {
    pub(super) fn new(
        path: &Path,
        playback: Playback,
        forecast: bool,
        delay_ms: Option<f64>,
        idx: &Index,
        opts: &Options,
        hz: u32,
    ) -> Result<Self> {
        ensure!(
            delay_ms.is_none_or(|v| v.is_finite() && v >= 0. && v <= f64::from(u32::MAX)),
            "invalid acoustic delay"
        );
        let immediate = delay_ms.is_none() && (opts.all || opts.source.paced() && !opts.shape_link);
        let lane = Lane {
            hz,
            link: opts.link.clone(),
            mbps: opts.mbps,
            delay_ms: delay_ms.unwrap_or(f64::from(opts.delay_ms)),
            free: 0,
            immediate,
        };
        let catalog = idx.object_profile.as_ref().map(|p| Catalog::new(p, idx)).transpose()?;
        let bundled = catalog.is_none() && idx.source_sha256 == crate::bench::sha(crate::scene::DEFAULT);
        Ok(Self {
            catalog,
            lane,
            forecast,
            playback,
            recording: Recording::new(path, playback, bundled)?,
            predicted: BTreeMap::new(),
            contacts: Vec::new(),
            journal: Vec::new(),
            evidence: Vec::new(),
            sounding: BTreeMap::new(),
            pages: Pages::default(),
            tick: 0,
        })
    }

    fn plan(&self, contact: Contact, receipt: Option<usize>, hz: u32, route: &Route, idx: &Index) -> Result<Plan> {
        let mut wanted = Bindings::new();
        if let Some((cat, prof)) = self.catalog.as_ref().zip(idx.object_profile.as_ref()) {
            cat.add_rays(prof, &[ray(&contact, hz, route)], &BTreeSet::new(), &mut wanted)?;
        }
        Ok(Plan { contact, wanted, receipt, direct: false, complete: false })
    }

    fn progress(
        &mut self,
        plan: &mut Plan,
        stage: Stage,
        hz: u32,
        route: &Route,
        idx: &Index,
        source: &Source,
    ) -> Result<()> {
        if plan.complete {
            return Ok(());
        }
        let authored = self.catalog.as_ref().zip(idx.object_profile.as_ref());
        let c = &plan.contact;
        let release_frame =
            if stage.at_emission { c.emission(hz) } else { u64::from(stage.tick) * u64::from(RATE) / u64::from(hz) };
        loop {
            if let Some((cat, prof)) = authored {
                match self.pages.prepare(&plan.wanted, cat, prof, source, stage, &mut self.lane) {
                    Status::Wait => return Ok(()),
                    Status::Failed(reason) => {
                        if let Some(i) = plan.receipt {
                            self.journal[i]["rejected"] = json!(reason);
                        }
                        plan.complete = true;
                        return Ok(());
                    }
                    Status::Ready => (),
                }
            }
            let mut mesh = resident::geometry(&plan.wanted, &self.pages.placed);
            if let Some(profile) = &idx.object_profile {
                profile.attach_rooms(&mut mesh);
            }
            if !self.playback.direct_only {
                mesh.prepare_solid_edges(&[]);
            }
            let material = || {
                self.catalog
                    .as_ref()
                    .filter(|_| c.owner.is_some())
                    .map(|cat| cat.material(c.owner, &c.surface))
                    .unwrap_or_else(|| {
                        let key = match c.surface.as_str() {
                            "rail" | "bonnet" => "steel-plate",
                            "road" => "asphalt",
                            _ => "default",
                        };
                        (None, key)
                    })
            };
            if !plan.direct {
                if let Some(i) = plan.receipt {
                    let onset = self.recording.publish(&mesh, c, material(), hz, route, false)?;
                    self.journal[i]["direct"] = publication(stage.tick, release_frame, onset);
                }
                plan.direct = true;
            }
            if !self.playback.direct_only {
                if let Some((cat, prof)) = authored {
                    let q = self.recording.queries(&mesh, c, hz, route);
                    let n = plan.wanted.len();
                    cat.add_rays(prof, &q.segments, &q.blockers, &mut plan.wanted)?;
                    if plan.wanted.len() > n {
                        continue;
                    }
                    if q.blockers.is_empty() {
                        plan.complete = true;
                        return Ok(());
                    }
                }
                if let Some(i) = plan.receipt {
                    self.journal[i]["edge"] = publication(
                        stage.tick,
                        release_frame,
                        self.recording.publish(&mesh, c, material(), hz, route, true)?,
                    );
                }
            }
            plan.complete = true;
            return Ok(());
        }
    }

    pub(super) fn capsule_reactions(&mut self, world: &World, tick: u32) -> Vec<Contact> {
        let step = crate::impact::capsule::Step {
            tick,
            hz: self.lane.hz,
            substeps: world.substeps(),
            before: world.prior_poses(),
            after: world.part_poses(),
            prior_boxes: world.prior_box_poses(),
            boxes: world.box_poses(),
            contacts: world.contacts(),
            swept: world.swept_impact_offset_s(),
        };
        self.recording.capsule_reactions(step, |id| world.box_index(id), |id| world.surface(id))
    }

    pub(super) fn reactions(&mut self, world: &World, tick: u32) -> Vec<Contact> {
        let state = Dynamics {
            parts: world.parts(),
            boxes: world.prior_box_poses(),
            box_index: &|surface| world.box_index(surface),
            before: world.prior_poses(),
            contacts: world.contacts(),
            impulses: world.impulses(),
            substeps: world.substeps(),
        };
        self.recording.reactions(tick, self.lane.hz, state, |i| world.surface(i))
    }

    fn flush_evidence(&mut self, frame: u64) -> Result<()> {
        let mut i = 0;
        while i < self.evidence.len() {
            if self.evidence[i].1.complete(frame) {
                let (row, voice) = self.evidence.swap_remove(i);
                let evidence = voice.evidence()?;
                let event = &mut self.journal[row];
                for path in ["direct", "edge"] {
                    if !event[path].is_null() {
                        event[path]["onset"]["physical"] = evidence.clone();
                    }
                }
                event["physical"] = evidence;
            } else {
                i += 1;
            }
        }
        Ok(())
    }

    pub(super) fn tick(
        &mut self,
        tick: u32,
        route: &Route,
        idx: &Index,
        source: &Source,
        contacts: Vec<Contact>,
        forecasts: impl Iterator<Item = (usize, Contact)>,
    ) -> Result<()> {
        let hz = self.lane.hz;
        let boundary = Stage { tick, at_emission: false };
        self.tick = tick;
        for c in contacts {
            let i = self.journal.len();
            self.journal.push(json!({"tick":c.tick,"point_m":c.point,"surface":c.surface,"owner":c.owner,
                "closing_m_s":c.speed,"direct":null,"edge":null}));
            let physical = c.voice.is_some() || c.measured.is_some();
            if let Some(voice) = &c.voice {
                self.evidence.push((i, voice.clone()));
            }
            let mut p = self.plan(c, Some(i), hz, route, idx)?;
            if physical {
                // Sound already resident at the solver substep can arrive inside this block.
                self.progress(&mut p, Stage { tick, at_emission: true }, hz, route, idx, source)?;
            }
            self.contacts.push(p);
        }
        self.recording.advance(tick, hz)?;
        let frame = u64::from(tick) * u64::from(RATE) / u64::from(hz);
        self.sounding.retain(|_, end| *end > frame);
        let mut active = std::mem::take(&mut self.contacts);
        for p in &mut active {
            self.progress(p, boundary, hz, route, idx, source)?;
            if self.forecast
                && p.complete
                && let Some(end) =
                    p.contact.voice.as_ref().and_then(|v| self.recording.voice_end(v)).filter(|&end| end > frame)
            {
                // Limbs of the same falling person can strike again during this material's ring.
                for &binding in &p.wanted {
                    if self.pages.placed.contains_key(&binding) {
                        self.sounding.entry(binding).and_modify(|e| *e = (*e).max(end)).or_insert(end);
                    }
                }
            }
        }
        active.retain(|p| !p.complete);
        self.contacts = active;
        let mut previous = std::mem::take(&mut self.predicted);
        for (sample, c) in forecasts {
            let predicted = previous.remove(&sample);
            // A prediction only reserves bytes. Every solved event traces its own exact coordinates.
            let keep = predicted.as_ref().is_some_and(|p| {
                let a = ray(&p.contact, hz, route);
                let b = ray(&c, hz, route);
                a.0.into_iter().chain(a.1).zip(b.0.into_iter().chain(b.1)).all(|(x, y)| (x - y).abs() < 0.025)
            });
            let mut p = if keep { predicted.unwrap() } else { self.plan(c, None, hz, route, idx)? };
            self.progress(&mut p, boundary, hz, route, idx, source)?;
            self.predicted.insert(sample, p);
        }
        if let Some((cat, prof)) = self.catalog.as_ref().zip(idx.object_profile.as_ref()) {
            let bindings: Bindings = self
                .contacts
                .iter()
                .chain(self.predicted.values())
                .flat_map(|p| p.wanted.iter().copied())
                .chain(self.sounding.keys().copied())
                .collect();
            let slots = bindings.iter().map(|&id| cat.resource(prof, id)).collect();
            self.pages.retain(&slots, &bindings, tick);
        }
        self.flush_evidence(frame)
    }

    pub(super) fn finish(mut self) -> Result<Value> {
        for p in &self.contacts {
            if let Some(i) = p.receipt {
                self.journal[i]["unresolved_at_end"] = json!(true);
            }
        }
        self.flush_evidence(u64::MAX)?;
        self.pages.retain(&BTreeSet::new(), &Bindings::new(), self.tick);
        let (frames, first) = self.recording.finish()?;
        let bytes: usize = self.pages.reads.iter().filter(|r| r.ready_tick.is_some()).map(|r| r.bytes).sum();
        Ok(json!({"frames":frames,"first_nonzero_pcm_frame":first,"sample_rate_hz":RATE,
            "plan":if self.forecast {"forecast"} else {"contact"},"requests":self.pages.reads,
            "contacts":self.journal,"read_bytes":bytes,"peak_decoded_pages":self.pages.peak,
            "final_decoded_pages":self.pages.decoded.len()}))
    }
}

fn publication(tick: u32, release_frame: u64, onset: Value) -> Value {
    json!({"published_tick":tick,"publication_frame":release_frame,"onset":onset})
}
