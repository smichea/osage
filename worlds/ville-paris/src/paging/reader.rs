// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::{
    Options,
    observers::{Focus, Lease, Observer, Reservations},
    planning::{Forecast, Motion, Plan, Request, ServiceSample},
    tiles::{Index, Layer, Tile},
};
use anyhow::{Context, Result};
use bevy::prelude::Vec3;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Visual,
    Physics,
}
impl Kind {
    pub fn layer(self, tile: &Tile) -> Option<&Layer> {
        match self {
            Self::Visual => (tile.visual.triangles > 0).then_some(&tile.visual),
            Self::Physics => tile.physics.as_ref(),
        }
    }
}
#[derive(Serialize)]
pub struct Event {
    pub tick: u32,
    pub kind: Kind,
    pub tile: [i32; 2],
    pub state: &'static str,
}
#[derive(Serialize)]
pub struct ModelEvent {
    pub tick: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<usize>,
    pub sha256: String,
    pub state: &'static str,
    pub due_tick: Option<u32>,
}
#[derive(Default, Serialize)]
pub struct Stats {
    pub reads: usize,
    pub read_bytes: usize,
    pub peak_pages: usize,
    pub peak_page_bytes: usize,
    pub evictions: usize,
    pub discarded_reads: usize,
    pub peak_in_flight: usize,
    pub rejected_reads: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub bundled_installs: usize,
}
fn is_zero(n: &usize) -> bool {
    *n == 0
}
pub enum PageSource {
    Pending,
    File,
    Region,
}
pub enum Change {
    InstallBundled(usize),
    Install(usize, Option<super::source::Ticket>),
    Evict(usize),
    LoadModel(usize, Option<super::source::Ticket>),
    EvictModel(usize),
    DropWaiting(usize),
}
struct Flight {
    page: usize,
    started: u32,
    lease: Option<Lease>,
    due: u32,
    ticket: Option<super::source::Ticket>,
}
pub struct Lane {
    pub kind: Kind,
    pub stats: Stats,
    pub(super) plan: Plan,
    pub(super) reservations: Option<Reservations>,
    observed_service: Option<u32>,
    priorities: Vec<f32>,
    pub events: Vec<Event>,
    pub resident: BTreeSet<usize>,
    pub waiting: BTreeSet<usize>,
    pub model_events: Vec<ModelEvent>,
    pub adaptive_resources: BTreeMap<usize, BTreeSet<usize>>,
    pub offscreen_tiles: BTreeSet<usize>,
    pub required_pages: BTreeSet<usize>,
    pub model_resident: BTreeSet<usize>,
    model_queue: BTreeSet<usize>,
    model_retry: BTreeMap<usize, u32>,
    model_flights: Vec<Flight>,
    queued: BTreeMap<usize, u32>,
    retry_after: BTreeMap<usize, u32>,
    flights: Vec<Flight>,
    wire_free: u32,
    bytes: usize,
}
impl Lane {
    pub fn new(kind: Kind) -> Self {
        Self {
            kind,
            stats: Stats::default(),
            plan: Plan::default(),
            reservations: None,
            observed_service: None,
            priorities: Vec::new(),
            events: vec![],
            resident: BTreeSet::new(),
            waiting: BTreeSet::new(),
            model_events: vec![],
            adaptive_resources: BTreeMap::new(),
            offscreen_tiles: BTreeSet::new(),
            required_pages: BTreeSet::new(),
            model_resident: BTreeSet::new(),
            model_queue: BTreeSet::new(),
            model_retry: BTreeMap::new(),
            model_flights: vec![],
            queued: BTreeMap::new(),
            retry_after: BTreeMap::new(),
            flights: vec![],
            wire_free: 0,
            bytes: 0,
        }
    }
    fn predict(&self, tick: u32, hz: u32, motion: Motion, index: &Index, o: &Options) -> BTreeMap<usize, Forecast> {
        let mut predictions = BTreeMap::new();
        let paced = o.source.paced() && !o.shape_link;
        let latency = (u64::from(o.delay_ms) + u64::from(o.link.jitter_ms)) * u64::from(hz);
        let latency = u32::try_from(latency.div_ceil(1000)).unwrap_or(u32::MAX);
        let queue = self.wire_free.saturating_sub(tick);
        let oldest = self
            .flights
            .iter()
            .filter(|f| f.ticket.as_ref().is_some_and(|t| !t.ready()))
            .map(|f| tick - f.started)
            .max()
            .unwrap_or(0);
        let pages = index.physical_pages();
        let service_ticks = |bytes| {
            let transfer = transfer_ticks(bytes, hz, o.mbps);
            let configured = latency.max(queue).saturating_add(transfer);
            if paced { self.observed_service.unwrap_or(configured).max(oldest).max(transfer) } else { configured }
        };
        let horizon = f64::from(Forecast::horizon(service_ticks(pages.largest_bytes))) / f64::from(hz);
        pages.approaching(&motion, horizon, |idx| {
            let layer = index.tiles[idx].physics.as_ref().unwrap();
            if let Some(p) = Forecast::new(&motion, layer.bounds, hz, service_ticks(layer.content.bytes)) {
                predictions.insert(idx, p);
            }
        });
        predictions
    }
    pub(super) fn forecast(&mut self, tick: u32, hz: u32, motion: Motion, index: &Index, o: &Options) {
        self.plan.pages = self.predict(tick, hz, motion, index, o);
    }
    /// Each observer holds its own interest in a tile. The admitted read and collider remain shared.
    pub(super) fn set_observers(
        &mut self,
        tick: u32,
        hz: u32,
        observers: &[Focus],
        required: &BTreeMap<Observer, BTreeSet<usize>>,
        index: &Index,
        o: &Options,
    ) {
        self.plan.pages.clear();
        let mut wanted: BTreeMap<usize, Vec<Observer>> = BTreeMap::new();
        let mut nearest = self.reservations.as_mut().map(|r| std::mem::take(&mut r.nearest)).unwrap_or_default();
        nearest.clear();

        self.required_pages = required.values().flatten().copied().collect();
        for observer in observers {
            let prediction = if o.predictive_physics && !o.all {
                self.predict(tick, hz, observer.motion, index, o)
            } else {
                BTreeMap::new()
            };
            let mut reserve = |i| {
                let owners = wanted.entry(i).or_default();
                // Several limbs or reasons can request the same page for this observer.
                if owners.last() != Some(&observer.id) {
                    owners.push(observer.id);
                }
            };
            let mut nearby = |i: usize| {
                let Some(layer) = &index.tiles[i].physics else { return };
                let dist = layer.distance(observer.foot);
                let closest = nearest.entry(i).or_insert((dist, observer.foot));
                if dist < closest.0 {
                    *closest = (dist, observer.foot);
                }
                let radius = o.physics_radius + if self.resident.contains(&i) { index.edge_m * 0.5 } else { 0. };
                if o.all || dist <= radius {
                    reserve(i);
                }
            };
            if o.all {
                (0..index.tiles.len()).for_each(nearby);
            } else {
                index.physical_pages().within(observer.foot, o.physics_radius + index.edge_m * 0.5, &mut nearby);
            }
            for &i in prediction.keys().chain(required.get(&observer.id).into_iter().flatten()) {
                if index.tiles.get(i).is_some_and(|t| t.physics.is_some()) {
                    reserve(i);
                }
            }
            for (i, p) in prediction {
                let q = self.plan.pages.entry(i).or_insert(p);
                if p.priority() < q.priority() {
                    *q = p;
                }
            }
        }
        let reservations = self.reservations.get_or_insert_with(Default::default);
        reservations.nearest = nearest;
        reservations.update(tick, index, wanted);
    }
    fn event(&mut self, tick: u32, page: usize, index: &Index, state: &'static str) {
        let row = &index.tiles[page];
        self.events.push(Event { tick, kind: self.kind, tile: [row.x, row.z], state });
    }
    /// A full page already on its way also needs its region when the transfer finishes.
    pub(super) fn upcoming_physical_pages(&self, index: &Index, motion: Motion) -> Vec<usize> {
        let mut pages: Vec<_> = self.flights.iter().map(|f| f.page).collect();
        if index.physical_addresses.is_some() {
            pages.extend(self.queued.keys().copied().filter(|&i| index.physical_file(i).is_none()));
        }
        index.physical_pages().approaching(&motion, 2., |i| pages.push(i));
        pages
    }

    /// Include previous interests so evictions, queue cancellations and waiting dependencies
    /// are handled in the same tile order as new requests.
    fn physical_worklist(&self, actor: Vec3, index: &Index, radius: f32) -> Vec<usize> {
        let mut pages = Vec::new();
        if let Some(pool) = &self.reservations {
            pages.extend(pool.nearest.keys().copied());
        } else {
            index.physical_pages().within(actor, radius, |i| pages.push(i));
        }
        pages.extend(&self.resident);
        pages.extend(&self.waiting);
        pages.extend(self.queued.keys());
        pages.extend(self.plan.pages.keys());
        pages.extend(self.required_pages.range(..index.tiles.len()));
        pages.sort_unstable();
        pages.dedup();
        pages
    }

    pub fn update(
        &mut self,
        tick: u32,
        hz: u32,
        focus: [Vec3; 2],
        index: &Index,
        o: &Options,
        mut resolve_address: impl FnMut(usize) -> Result<PageSource>,
    ) -> Result<Vec<Change>> {
        let [actor, camera] = focus;
        let radius = if self.kind == Kind::Physics { o.physics_radius } else { o.visual_radius };
        let mut changes = Vec::new();
        let worklist = (self.kind == Kind::Physics && !o.all)
            .then(|| self.physical_worklist(actor, index, radius + index.edge_m * 0.5));
        let mut priorities = std::mem::take(&mut self.priorities);
        priorities.resize(index.tiles.len(), f32::INFINITY);
        let mut model_priority =
            vec![
                f32::INFINITY;
                if self.kind == Kind::Visual { index.model_resources.len() + index.texture_resources.len() } else { 0 }
            ];
        let dense_count = if worklist.is_some() { 0 } else { index.tiles.len() };
        for i in (0..dense_count).chain(worklist.iter().flatten().copied()) {
            let t = &index.tiles[i];
            let Some(l) = self.kind.layer(t) else { continue };
            let (dist, physical_actor) = self
                .reservations
                .as_ref()
                .map(|r| r.nearest.get(&i).copied().unwrap_or((f32::INFINITY, Vec3::ZERO)))
                .unwrap_or_else(|| (l.distance(actor), actor));
            let cam = if self.kind == Kind::Visual { l.distance(camera) } else { f32::INFINITY };
            let distance = if self.offscreen_tiles.contains(&i) { f32::INFINITY } else { dist.min(cam) };
            let held = Some(i) == o.watched && o.hold == Some(self.kind) && tick < o.hold_ticks;
            let required = self.required_pages.contains(&i);
            let predicted = self.plan.pages.get(&i).copied();
            if self.resident.contains(&i) {
                if !o.all && !required && predicted.is_none() && distance > radius + index.edge_m * 0.5 {
                    self.resident.remove(&i);
                    self.bytes -= l.content.bytes;
                    self.stats.evictions += 1;
                    self.event(tick, i, index, "evicted");
                    changes.push(Change::Evict(i));
                }
            } else if o.all || required || predicted.is_some() || distance <= radius {
                let center = Vec3::new(t.x as f32 * index.edge_m, physical_actor.y, t.z as f32 * index.edge_m);
                // Nearest actor demand leads camera-only detail. The centre breaks overlapping-box ties.
                priorities[i] = if required {
                    -2.0
                } else if let Some(p) = predicted {
                    p.priority()
                } else if dist <= radius {
                    dist + 0.01 * physical_actor.distance(center)
                } else {
                    radius + cam
                };
                if !held
                    && !self.waiting.contains(&i)
                    && self.retry_after.get(&i).is_none_or(|due| tick >= *due)
                    && !self.flights.iter().any(|f| f.page == i)
                    && !self.queued.contains_key(&i)
                {
                    self.queued.insert(i, tick);
                    if !required
                        && dist > radius
                        && let Some(prediction) = predicted
                    {
                        self.plan.requests.push(Request { tick, tile: [t.x, t.z], prediction });
                    }
                    self.event(tick, i, index, "requested");
                }
            } else if self.queued.remove(&i).is_some() {
                self.event(tick, i, index, "cancelled");
            }
            if self.waiting.contains(&i) && (held || !priorities[i].is_finite()) {
                self.waiting.remove(&i);
                self.event(tick, i, index, "released_before_publication");
                changes.push(Change::DropWaiting(i));
            }
            if self.kind == Kind::Visual && (self.resident.contains(&i) || (!held && priorities[i].is_finite())) {
                for id in index.dependencies(l).chain(self.adaptive_resources.get(&i).into_iter().flatten().copied()) {
                    model_priority[id] = model_priority[id].min(priorities[i].min(radius));
                }
            }
        }
        for (id, &priority) in model_priority.iter().enumerate() {
            if priority.is_finite() {
                if !self.model_resident.contains(&id)
                    && self.model_retry.get(&id).is_none_or(|due| tick >= *due)
                    && !self.model_flights.iter().any(|f| f.page == id)
                    && self.model_queue.insert(id)
                {
                    self.model_event(tick, id, index, "requested", None);
                }
            } else {
                if self.model_queue.remove(&id) {
                    self.model_event(tick, id, index, "cancelled", None);
                }
                if self.model_resident.remove(&id) {
                    self.model_event(tick, id, index, "evicted", None);
                    changes.push(Change::EvictModel(id));
                }
            }
        }
        let (done, pending): (Vec<_>, Vec<_>) = self.model_flights.drain(..).partition(|f| {
            f.due <= tick && (!model_priority[f.page].is_finite() || f.ticket.as_ref().is_none_or(|t| t.ready()))
        });
        self.model_flights = pending;
        for f in done {
            if model_priority[f.page].is_finite() {
                changes.push(Change::LoadModel(f.page, f.ticket));
            } else {
                self.model_event(
                    tick,
                    f.page,
                    index,
                    if o.source.paced() { "released_after_request" } else { "discarded_before_read" },
                    None,
                );
            }
        }
        let (complete, pending): (Vec<_>, _) = self.flights.drain(..).partition(|f| {
            f.due <= tick
                && ((f.lease.is_none() && !priorities[f.page].is_finite())
                    || f.ticket.as_ref().is_none_or(|t| t.ready()))
        });
        self.flights = pending;
        for f in complete {
            let valid = match (f.lease, self.reservations.as_mut()) {
                (Some(token), Some(pool)) => {
                    let bytes = f.ticket.as_ref().and_then(|t| t.bytes().ok()).map(<[u8]>::len);
                    pool.arrived(tick, f.page, index, token, bytes)
                }
                _ => true,
            };
            if self.kind == Kind::Physics
                && o.source.paced()
                && !o.shape_link
                && f.ticket.as_ref().is_some_and(|ticket| ticket.ready() && ticket.bytes().is_ok())
            {
                let age = tick.saturating_sub(f.started).max(1);
                // Grow at once after a slow read and decay by one eighth after faster ones.
                let estimate_ticks = self
                    .observed_service
                    .map_or(age, |last| if age >= last { age } else { last - (last - age).div_ceil(8) });
                self.observed_service = Some(estimate_ticks);
                let tile = &index.tiles[f.page];
                self.plan.samples.push(ServiceSample {
                    tick,
                    tile: [tile.x, tile.z],
                    elapsed_ticks: age,
                    estimate_ticks,
                });
            }
            if valid && priorities[f.page].is_finite() {
                changes.push(Change::Install(f.page, f.ticket));
            } else {
                self.stats.discarded_reads += 1;
                self.event(
                    tick,
                    f.page,
                    index,
                    if o.source.paced() { "released_after_request" } else { "discarded_before_read" },
                );
            }
        }
        let mut requests: Vec<_> = self
            .queued
            .keys()
            .map(|&i| (priorities[i], false, i))
            .chain(self.model_queue.iter().map(|&i| (model_priority[i], true, i)))
            .collect();
        requests.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        let slots =
            if o.all { requests.len() } else { 4usize.saturating_sub(self.flights.len() + self.model_flights.len()) };
        for (_, model, i) in requests.into_iter().take(slots) {
            // When forecasting, leave one of four physical admissions free for next-step blockers.
            // Other active requests keep their place on the real transport.
            if self.kind == Kind::Physics
                && o.predictive_physics
                && !o.all
                && self.flights.len() >= 3
                && !self.required_pages.contains(&i)
            {
                break;
            }
            let content = if model {
                index.resource(i)
            } else if self.kind == Kind::Physics {
                match resolve_address(i)? {
                    PageSource::Pending => continue,
                    PageSource::File => {}
                    PageSource::Region => {
                        self.queued.remove(&i);
                        self.event(tick, i, index, "from_region");
                        changes.push(Change::InstallBundled(i));
                        continue;
                    }
                }
                index.physical_file(i).context("missing payload address after resolution")?
            } else {
                &index.tiles[i].visual.content
            };
            let Ok(ticket) = o.source.start(content, o.all) else { continue };
            if model {
                self.model_queue.remove(&i);
                let r = index.resource(i);
                let key = u64::from_str_radix(&r.sha256[..16], 16).expect("validated content hash");
                let due = if o.all { tick } else { self.reserve_transfer(tick, r.bytes, hz, key, o) };
                self.model_event(tick, i, index, "started", Some(due));
                if o.all {
                    changes.push(Change::LoadModel(i, ticket));
                } else {
                    self.model_flights.push(Flight { page: i, started: tick, lease: None, due, ticket });
                }
                continue;
            }
            let lease = self.reservations.as_mut().and_then(|r| r.started(tick, i, index));
            self.queued.remove(&i);
            self.event(tick, i, index, "started");
            if o.all {
                changes.push(Change::Install(i, ticket));
                continue;
            }
            let bytes = content.bytes;
            let t = &index.tiles[i];
            let key = (u64::from(t.x as u32) << 32) | u64::from(t.z as u32);
            let due = self.reserve_transfer(tick, bytes, hz, key, o);
            self.flights.push(Flight { page: i, started: tick, lease, due, ticket });
        }
        self.stats.peak_in_flight = self.stats.peak_in_flight.max(self.flights.len() + self.model_flights.len());
        if let Some(pages) = worklist {
            for i in pages {
                priorities[i] = f32::INFINITY;
            }
        } else {
            priorities.fill(f32::INFINITY);
        }
        self.priorities = priorities;
        Ok(changes)
    }
    fn model_event(&mut self, tick: u32, i: usize, index: &Index, state: &'static str, due_tick: Option<u32>) {
        self.model_events.push(ModelEvent {
            tick,
            model: (i < index.model_resources.len()).then_some(i),
            image: i.checked_sub(index.model_resources.len()),
            sha256: index.resource(i).sha256.clone(),
            state,
            due_tick,
        });
    }
    pub fn model_loaded(&mut self, tick: u32, i: usize, index: &Index) {
        self.model_resident.insert(i);
        self.model_retry.remove(&i);
        self.model_event(tick, i, index, "ready", None);
    }
    pub fn model_failed(&mut self, tick: u32, i: usize, index: &Index, hz: u32) {
        self.model_retry.insert(i, tick.saturating_add(hz));
        self.model_event(tick, i, index, "rejected", None);
    }
    pub fn stage(&mut self, tick: u32, i: usize, index: &Index) {
        self.stats.reads += 1;
        self.stats.read_bytes += self.kind.layer(&index.tiles[i]).unwrap().content.bytes;
        self.waiting.insert(i);
        self.event(tick, i, index, if self.kind == Kind::Physics { "waiting_for_support" } else { "decoded" });
    }
    /// Reserve service on this lane for a tile or a shared model. Both pass
    /// through the same wire clock, jitter and disconnection interval.
    pub(super) fn reserve_transfer(&mut self, tick: u32, bytes: usize, hz: u32, key: u64, o: &Options) -> u32 {
        if o.source.paced() && !o.shape_link {
            return tick;
        }
        let latency = o.link.latency_ticks(o.delay_ms, hz, tick, key, self.kind == Kind::Physics);
        let transfer = transfer_ticks(bytes, hz, o.mbps);
        let ready_to_send = self.wire_free.max(o.link.advance(tick, latency));
        self.wire_free = o.link.advance(ready_to_send, transfer);
        self.wire_free
    }
    pub fn reject(&mut self, tick: u32, i: usize, index: &Index, hz: u32) {
        self.waiting.remove(&i);
        self.stats.rejected_reads += 1;
        self.retry_after.insert(i, tick.saturating_add(hz));
        self.event(tick, i, index, "rejected");
    }
    pub fn install(&mut self, tick: u32, i: usize, index: &Index) {
        self.publish(tick, i, index, false);
    }
    pub fn install_bundled(&mut self, tick: u32, i: usize, index: &Index) {
        self.publish(tick, i, index, true);
    }
    fn publish(&mut self, tick: u32, i: usize, index: &Index, bundled: bool) {
        self.retry_after.remove(&i);
        self.resident.insert(i);
        let bytes = self.kind.layer(&index.tiles[i]).unwrap().content.bytes;
        self.bytes += bytes;
        if bundled {
            self.stats.bundled_installs += 1;
        } else if !self.waiting.remove(&i) {
            self.stats.reads += 1;
            self.stats.read_bytes += bytes;
        }
        self.stats.peak_pages = self.stats.peak_pages.max(self.resident.len());
        self.stats.peak_page_bytes = self.stats.peak_page_bytes.max(self.bytes);
        if let Some(reservations) = &mut self.reservations {
            reservations.installed(tick, i, index);
        }
        self.event(tick, i, index, "ready");
    }
}

pub(super) fn transfer_ticks(bytes: usize, hz: u32, mbps: f64) -> u32 {
    ((bytes as f64 * 8. * f64::from(hz) / (mbps * 1e6)).ceil() as u32).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paging::tiles::File;
    fn index() -> Index {
        let content = || File { file: String::new(), sha256: String::new(), bytes: 16 };
        let layer = |x: f32| Layer {
            content: content(),
            bounds: [[x, -1., -1.], [x + 10., 1., 1.]],
            triangles: 1,
            ids: vec![],
            source_nodes: vec![],
            uses: vec![],
            ..Default::default()
        };
        Index {
            format: String::new(),
            source_sha256: String::new(),
            edge_m: 64.,
            parent: content(),
            support: None,
            models: None,
            model_resources: vec![],
            model_variants: vec![],
            material_palette: vec![],
            encoded_bytes: 0,
            source_parents: vec![],
            source_instances: 2,
            source_triangles: 2,
            tiles: [0, 5]
                .into_iter()
                .map(|x| Tile {
                    x,
                    z: 0,
                    visual: layer(x as f32 * 64.),
                    physics: Some(layer(x as f32 * 64.)),
                    parent_first: 0,
                    parent_count: 0,
                })
                .collect(),
            ..Default::default()
        }
    }
    #[test]
    fn physical_index_matches_an_exhaustive_search_with_unequal_read_times() {
        use bevy::math::DVec3;
        let mut district = index();
        district.tiles.clear();
        for x in -16..=16 {
            for z in -8..=8 {
                let mut tile = index().tiles.remove(0);
                tile.x = x;
                tile.z = z;
                let layer = tile.physics.as_mut().unwrap();
                let lo = [x as f32 * 24., (x + z) as f32 * 3., z as f32 * 24.];
                layer.bounds = [lo, [lo[0] + 36., lo[1], lo[2] + 12.]];
                // Include long spanning footprints, distant expensive pages and holes in the directory.
                if z == 8 {
                    layer.bounds[0][2] = 0.;
                    layer.content.bytes = 500_000;
                } else if (x + z) % 7 == 0 {
                    tile.physics = None;
                }
                district.tiles.push(tile);
            }
        }
        let mut lane = Lane::new(Kind::Physics);
        lane.wire_free = 92;
        let o = Options { mbps: 2., delay_ms: 25, ..Default::default() };
        for tick in [0, 90, 180] {
            for (y, vy) in [(0., 0.), (-9., 20.), (123., -52.), (18., 4.)] {
                for vx in [-30., 0., 45.] {
                    let motion = Motion {
                        center: DVec3::new(-8., y, 0.),
                        velocity: DVec3::new(vx, vy, 1.),
                        gravity: 9.81,
                        terminal: 55.,
                        extent: 0.88,
                    };
                    let predicted = lane.predict(tick, 60, motion, &district, &o);
                    let expected: BTreeMap<_, _> = district
                        .tiles
                        .iter()
                        .enumerate()
                        .filter_map(|(i, t)| {
                            let layer = t.physics.as_ref()?;
                            let service = 2_u32.max(lane.wire_free.saturating_sub(tick))
                                + transfer_ticks(layer.content.bytes, 60, o.mbps);
                            Forecast::new(&motion, layer.bounds, 60, service).map(|f| (i, f))
                        })
                        .collect();
                    assert_eq!(serde_json::to_value(predicted).unwrap(), serde_json::to_value(expected).unwrap());
                }
            }
        }
        let eye = Vec3::new(12., 0., -12.);
        let mut nearby = BTreeSet::new();
        district.physical_pages().within(eye, 12., |i| {
            nearby.insert(i);
        });
        let expected: BTreeSet<_> = district
            .tiles
            .iter()
            .enumerate()
            .filter_map(|(i, t)| (t.physics.as_ref()?.distance(eye) <= 12.).then_some(i))
            .collect();
        assert_eq!(nearby, expected);
    }

    #[test]
    fn a_turn_retracts_the_eastern_request_and_only_the_western_page_becomes_resident() {
        let mut idx = index();
        for (tile, x) in idx.tiles.iter_mut().zip([70., -77.]) {
            tile.x = if x > 0. { 1 } else { -1 };
            tile.physics.as_mut().unwrap().bounds = [[x, -1., -1.], [x + 8., 1., 1.]];
        }
        let o = Options { physics_radius: 12., delay_ms: 1400, ..Default::default() };
        let mut lane = Lane::new(Kind::Physics);
        for tick in 0_u32..=89 {
            let x = if tick == 0 { 0. } else { 1. - f64::from(tick.saturating_sub(2)) };
            let v = if tick == 0 {
                60.
            } else if tick == 1 {
                0.
            } else {
                -60.
            };
            let motion = Motion {
                center: bevy::math::DVec3::X * x,
                velocity: bevy::math::DVec3::X * v,
                gravity: 0.,
                terminal: 55.,
                extent: 0.9,
            };
            lane.forecast(tick, 60, motion, &idx, &o);
            for change in lane
                .update(tick, 60, [motion.center.as_vec3(), Vec3::X * 1000.], &idx, &o, |_| Ok(PageSource::File))
                .unwrap()
            {
                if let Change::Install(i, _) = change {
                    lane.install(tick, i, &idx);
                }
            }
            if tick == 1 {
                assert!(lane.plan.pages.is_empty());
            }
        }
        assert_eq!(
            lane.plan.requests.iter().map(|r| (r.tick, r.tile)).collect::<Vec<_>>(),
            [(0, [1, 0]), (2, [-1, 0])]
        );
        assert_eq!(lane.resident, BTreeSet::from([1]));
        assert_eq!((lane.stats.reads, lane.stats.discarded_reads), (1, 1));
    }

    #[test]
    fn a_next_step_blocker_gets_the_fourth_admission_ahead_of_background_candidates() {
        let mut idx = index();
        for i in 2..7 {
            let mut more = index().tiles.remove(0);
            more.x = i;
            idx.tiles.push(more);
        }
        let o = Options { physics_radius: 1000., delay_ms: 1400, ..Default::default() };
        let mut lane = Lane::new(Kind::Physics);
        lane.update(0, 60, [Vec3::ZERO; 2], &idx, &o, |_| Ok(PageSource::File)).unwrap();
        assert_eq!(lane.flights.len(), 3);
        lane.required_pages.insert(6);
        lane.update(1, 60, [Vec3::ZERO; 2], &idx, &o, |_| Ok(PageSource::File)).unwrap();
        assert_eq!(lane.stats.peak_in_flight, 4);
        assert!(lane.flights.iter().any(|f| f.page == 6 && f.started == 1));
    }

    #[test]
    fn residency_expires_and_reentry_needs_another_read() {
        let idx = index();
        let mut lane = Lane::new(Kind::Physics);
        let o = Options { physics_radius: 20., delay_ms: 0, mbps: 100., ..Default::default() };
        let left = Vec3::ZERO;
        let right = Vec3::X * 320.;
        for (tick, point) in [(0, left), (1, left), (2, right), (3, right), (4, left), (5, left)] {
            for change in lane.update(tick, 60, [point, point], &idx, &o, |_| Ok(PageSource::File)).unwrap() {
                if let Change::Install(i, _) = change {
                    lane.install(tick, i, &idx);
                }
            }
        }
        assert_eq!(lane.resident, BTreeSet::from([0]));
        assert_eq!(lane.stats.reads, 3);
        assert_eq!(lane.stats.evictions, 2);
        assert_eq!(lane.stats.peak_pages, 1);
    }
    #[test]
    fn stale_completion_does_not_install_or_consume_file_bytes() {
        let idx = index();
        let mut lane = Lane::new(Kind::Visual);
        let o = Options { visual_radius: 16., delay_ms: 100, mbps: 100., ..Default::default() };
        lane.update(0, 60, [Vec3::ZERO; 2], &idx, &o, |_| Ok(PageSource::File)).unwrap();
        for tick in 1..=7 {
            lane.update(tick, 60, [Vec3::splat(2000.); 2], &idx, &o, |_| Ok(PageSource::File)).unwrap();
        }
        assert!(lane.resident.is_empty());
        assert_eq!(lane.stats.read_bytes, 0);
        assert_eq!(lane.stats.discarded_reads, 1);
    }
    #[test]
    fn a_rejected_transfer_is_retried_before_publication() {
        let idx = index();
        let mut lane = Lane::new(Kind::Physics);
        let o = Options { physics_radius: 20., delay_ms: 0, ..Default::default() };
        for tick in 0..=62 {
            for change in lane.update(tick, 60, [Vec3::ZERO; 2], &idx, &o, |_| Ok(PageSource::File)).unwrap() {
                if let Change::Install(i, _) = change {
                    if tick == 1 {
                        lane.reject(tick, i, &idx, 60);
                    } else {
                        lane.install(tick, i, &idx);
                    }
                }
            }
        }
        let successes: Vec<_> = lane.events.iter().filter(|e| e.state == "ready").map(|e| e.tick).collect();
        assert_eq!(successes, [62]);
        assert_eq!(lane.stats.rejected_reads, 1);
        assert_eq!(lane.stats.read_bytes, 16);
    }
}
