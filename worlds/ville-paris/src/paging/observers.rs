// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Kinematic physical observers share immutable pages with the simulated actor.
use super::{
    planning::Motion,
    tiles::{Index, Tile},
};
use anyhow::{Result, ensure};
use bevy::{math::DVec3, prelude::Vec3};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(super) struct Observer {
    pub slot: u32,
    pub generation: u32,
}
pub(super) const PRIMARY: Observer = Observer { slot: 0, generation: 1 };

#[derive(Deserialize)]
pub(super) struct Tracks {
    version: u32,
    tracks: Vec<Track>,
}
#[derive(Deserialize)]
struct Track {
    slot: u32,
    start: u32,
    end: u32,
    keys: Vec<Key>,
    #[serde(skip)]
    generation: u32,
}
#[derive(Deserialize)]
struct Key {
    tick: u32,
    foot_m: [f32; 3],
    center_m: [f32; 3],
    velocity_m_s: [f32; 3],
    #[serde(default)]
    supported: bool,
}

#[derive(Clone, Copy)]
pub(super) struct Focus {
    pub id: Observer,
    pub foot: Vec3,
    pub motion: Motion,
}

impl Focus {
    pub fn next_bounds(self, hz: u32) -> [[f32; 3]; 2] {
        let dt = 1. / f64::from(hz);
        let p = self.motion.center;
        let v = self.motion.velocity * dt;
        let extent = DVec3::splat(self.motion.extent + 0.001);
        [
            (p + v.min(DVec3::ZERO) - DVec3::Y * self.motion.gravity * dt * dt - extent).as_vec3().to_array(),
            (p + v.max(DVec3::ZERO) + extent).as_vec3().to_array(),
        ]
    }
}

impl Tracks {
    pub fn read(bytes: &[u8]) -> Result<Self> {
        let mut tracks: Self = serde_json::from_slice(bytes)?;
        ensure!(tracks.version == 1, "unknown physics observer stream");
        tracks.tracks.sort_by_key(|t| (t.slot, t.start));
        let mut previous = None;
        for t in &mut tracks.tracks {
            ensure!(
                t.slot != 0
                    && t.start < t.end
                    && t.keys.first().is_some_and(|k| k.tick == t.start)
                    && t.keys.last().is_some_and(|k| k.tick == t.end - 1)
                    && t.keys.windows(2).all(|ks| ks[0].tick < ks[1].tick)
                    && t.keys.iter().all(|k| k
                        .foot_m
                        .into_iter()
                        .chain(k.center_m)
                        .chain(k.velocity_m_s)
                        .all(f32::is_finite)),
                "invalid physical observer keys"
            );
            let mut generation = 1_u32;
            if let Some((slot, end, last)) = previous
                && slot == t.slot
            {
                ensure!(end <= t.start, "overlapping occupants of one physical observer slot");
                generation =
                    u32::checked_add(last, 1).ok_or_else(|| anyhow::anyhow!("observer generation exhausted"))?;
            }
            t.generation = generation;
            previous = Some((t.slot, t.end, generation));
        }
        Ok(tracks)
    }

    pub fn active(&self, tick: u32, primary: Focus, gravity: f32) -> Vec<Focus> {
        let mut out = vec![primary];
        for t in self.tracks.iter().filter(|t| (t.start..t.end).contains(&tick)) {
            let lo = t.keys.partition_point(|k| k.tick <= tick) - 1;
            let a = &t.keys[lo];
            let b = t.keys.get(lo + 1).unwrap_or(a);
            let factor = if a.tick == b.tick { 0. } else { (tick - a.tick) as f32 / (b.tick - a.tick) as f32 };
            let interpolate = |x: [f32; 3], y: [f32; 3]| {
                if factor == 0. { Vec3::from(x) } else { Vec3::from(x).lerp(Vec3::from(y), factor) }
            };
            out.push(Focus {
                id: Observer { slot: t.slot, generation: t.generation },
                foot: interpolate(a.foot_m, b.foot_m),
                motion: Motion {
                    center: interpolate(a.center_m, b.center_m).as_dvec3(),
                    velocity: interpolate(a.velocity_m_s, b.velocity_m_s).as_dvec3(),
                    gravity: if a.supported { 0. } else { f64::from(gravity) },
                    extent: f64::from(crate::physics::CAPSULE.half_segment + crate::physics::CAPSULE.radius),
                    ..primary.motion
                },
            });
        }
        out
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub(super) struct Lease {
    slot: usize,
    generation: u64,
}
#[derive(Serialize)]
struct Event {
    tick: u32,
    state: &'static str,
    tile: [i32; 2],
    sha256: String,
    lease: Lease,
    observers: Vec<Observer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bytes: Option<usize>,
}
#[derive(Default, Serialize)]
struct Transfers {
    starts: usize,
    installs: usize,
    verified_reads: usize,
    verified_bytes: usize,
    stale_arrivals: usize,
    stale_bytes: usize,
}
#[derive(Default)]
pub(super) struct Reservations {
    // Slot storage carries no geometry. A flight can outlive the slot allocated to its page.
    slots: Vec<u64>,
    free: BTreeSet<usize>,
    pages: BTreeMap<usize, (Lease, Vec<Observer>)>,
    pub nearest: BTreeMap<usize, (f32, Vec3)>,
    events: Vec<Event>,
    transfers: BTreeMap<String, Transfers>,
}

impl Reservations {
    pub fn update(&mut self, tick: u32, index: &Index, wanted: BTreeMap<usize, Vec<Observer>>) {
        let gone: Vec<_> = self.pages.keys().filter(|page| !wanted.contains_key(page)).copied().collect();
        for page in gone {
            let (lease, owners) = self.pages.remove(&page).unwrap();
            self.record(tick, &index.tiles[page], "released", lease, owners, None);
            self.free.insert(lease.slot);
        }
        for (page, owners) in wanted {
            if let Some((lease, old)) = self.pages.get_mut(&page) {
                if *old != owners {
                    let lease = *lease;
                    *old = owners.clone();
                    self.record(tick, &index.tiles[page], "observers_changed", lease, owners, None);
                }
            } else {
                let slot = self.free.pop_first().unwrap_or(self.slots.len());
                if slot == self.slots.len() {
                    self.slots.push(0);
                }
                self.slots[slot] = self.slots[slot].checked_add(1).expect("page reservation generation exhausted");
                let lease = Lease { slot, generation: self.slots[slot] };
                self.pages.insert(page, (lease, owners.clone()));
                self.record(tick, &index.tiles[page], "reserved", lease, owners, None);
            }
        }
    }
    fn record(
        &mut self,
        tick: u32,
        tile: &Tile,
        state: &'static str,
        lease: Lease,
        observers: Vec<Observer>,
        bytes: Option<usize>,
    ) {
        self.events.push(Event {
            tick,
            state,
            tile: [tile.x, tile.z],
            sha256: tile.physics.as_ref().unwrap().content.sha256.clone(),
            lease,
            observers,
            bytes,
        });
    }
    pub fn current(&self, page: usize) -> Option<Lease> {
        self.pages.get(&page).map(|(lease, _)| *lease)
    }
    pub fn started(&mut self, tick: u32, page: usize, index: &Index) -> Option<Lease> {
        let (lease, owners) = self.pages.get(&page)?.clone();
        self.transfers.entry(index.tiles[page].physics.as_ref().unwrap().content.sha256.clone()).or_default().starts +=
            1;
        self.record(tick, &index.tiles[page], "read_started", lease, owners, None);
        Some(lease)
    }
    pub fn arrived(&mut self, tick: u32, page: usize, index: &Index, lease: Lease, bytes: Option<usize>) -> bool {
        let current = self.current(page) == Some(lease);
        let transfer =
            self.transfers.entry(index.tiles[page].physics.as_ref().unwrap().content.sha256.clone()).or_default();
        if let Some(n) = bytes {
            transfer.verified_reads += 1;
            transfer.verified_bytes += n;
        }
        if !current {
            transfer.stale_arrivals += 1;
            transfer.stale_bytes += bytes.unwrap_or(0);
        }
        let owners = self
            .pages
            .get(&page)
            .filter(|(token, _)| *token == lease)
            .map_or_else(Vec::new, |(_, owners)| owners.clone());
        self.record(
            tick,
            &index.tiles[page],
            if current { "current_arrival" } else { "stale_arrival" },
            lease,
            owners,
            bytes,
        );
        current
    }
    pub fn installed(&mut self, tick: u32, page: usize, index: &Index) {
        let Some((lease, owners)) = self.pages.get(&page).cloned() else { return };
        let tile = &index.tiles[page];
        self.transfers.entry(tile.physics.as_ref().unwrap().content.sha256.clone()).or_default().installs += 1;
        self.record(tick, tile, "installed", lease, owners, None);
    }
    pub fn verified_on_install(&mut self, tick: u32, page: usize, index: &Index, bytes: usize, all: bool) {
        if all && let Some(token) = self.current(page) {
            self.arrived(tick, page, index, token, Some(bytes));
        } else {
            let stat =
                self.transfers.entry(index.tiles[page].physics.as_ref().unwrap().content.sha256.clone()).or_default();
            stat.verified_reads += 1;
            stat.verified_bytes += bytes;
        }
    }
    pub fn report(&self) -> serde_json::Value {
        serde_json::json!({"slots_allocated":self.slots.len(),"active_reservations":self.pages.len(),
                           "events":self.events,"transfers":self.transfers})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paging::tiles::{File, Layer};

    #[test]
    fn stale_completion_cannot_install_after_the_same_page_and_observer_slot_are_reused() {
        let idx = Index {
            tiles: vec![Tile {
                x: 0,
                z: 0,
                visual: Layer::default(),
                physics: Some(Layer {
                    content: File { file: "physical.json".into(), sha256: "physical".into(), bytes: 12 },
                    ..Default::default()
                }),
                parent_first: 0,
                parent_count: 0,
            }],
            ..Default::default()
        };
        let mut reservations = Reservations::default();
        reservations.update(0, &idx, BTreeMap::from([(0, vec![Observer { slot: 1, generation: 1 }])]));
        let first = reservations.started(0, 0, &idx).unwrap();
        reservations.update(1, &idx, BTreeMap::new());
        assert!(reservations.current(0).is_none());
        reservations.update(2, &idx, BTreeMap::from([(0, vec![Observer { slot: 1, generation: 2 }])]));
        let second = reservations.started(2, 0, &idx).unwrap();
        assert_eq!(first.slot, second.slot);
        assert_ne!(first.generation, second.generation);
        assert!(!reservations.arrived(3, 0, &idx, first, Some(12)));
        assert_eq!(reservations.current(0), Some(second));
        assert!(reservations.arrived(4, 0, &idx, second, Some(12)));
    }
}
