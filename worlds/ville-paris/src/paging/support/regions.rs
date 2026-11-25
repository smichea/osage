// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::{Bounds, Guard, Index, Source, World};
use crate::paging::source::Ticket;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy)]
enum Read {
    Spawn,
    Support,
    Address { retry_at: u32 },
}

pub(super) struct Regions {
    for_tile: Vec<Option<usize>>,
    loaded: BTreeSet<usize>,
    pending: BTreeMap<usize, Ticket>,
    required_reads: BTreeSet<usize>,
    retry_at: BTreeMap<usize, u32>,
    pub(super) threats: BTreeSet<usize>,
    events: Vec<(u32, usize, &'static str)>,
}

impl Regions {
    pub(super) fn new(idx: &Index) -> Result<Self> {
        let mut for_tile = vec![None; idx.tiles.len()];
        for (id, region) in idx.support_regions.iter().enumerate() {
            let f = &region.file;
            f.content.validate(if f.content.file.ends_with(".zst") { "zst" } else { "zlib" })?;
            ensure!(f.decoded_bytes > 0 && !region.tiles.is_empty(), "empty support region");
            ensure!(region.tiles.windows(2).all(|p| p[0] < p[1]), "unordered support region");
            for &tile in &region.tiles {
                ensure!(idx.tiles.get(tile).is_some_and(|t| t.physics.is_some()), "unknown physical region tile");
                ensure!(for_tile[tile].replace(id).is_none(), "duplicate support region tile");
            }
        }
        ensure!(
            idx.tiles.iter().zip(&for_tile).all(|(t, r)| t.physics.is_some() == r.is_some()),
            "support regions do not cover the collision directory"
        );
        Ok(Self {
            for_tile,
            loaded: BTreeSet::new(),
            pending: BTreeMap::new(),
            required_reads: BTreeSet::new(),
            retry_at: BTreeMap::new(),
            threats: BTreeSet::new(),
            events: Vec::new(),
        })
    }
}

impl Guard {
    pub(super) fn install_inline_region(&mut self, idx: &Index, id: usize, bytes: &[u8]) -> Result<()> {
        let r = &idx.support_regions[id];
        self.read_catalogue(idx, &r.tiles, &r.file, bytes, r.complete_pages)?;
        let regions = self.regions.as_mut().context("first-frame support requires a regional directory")?;
        regions.loaded.insert(id);
        regions.events.push((0, id, "installed_from_pin"));
        Ok(())
    }

    pub(super) fn region_ready(&self, tile: usize) -> bool {
        self.regions.as_ref().is_none_or(|r| r.for_tile[tile].is_none_or(|id| r.loaded.contains(&id)))
    }

    fn load_region(&mut self, tick: u32, purpose: Read, id: usize, idx: &Index, source: &Source) -> Result<bool> {
        let regions = self.regions.as_mut().expect("regional support");
        if regions.loaded.contains(&id) {
            return Ok(true);
        }
        let region = &idx.support_regions[id];
        let optional = matches!(purpose, Read::Address { .. }) && !regions.required_reads.contains(&id);
        if optional && regions.retry_at.get(&id).is_some_and(|&due| tick < due) {
            return Ok(false);
        }
        if !matches!(purpose, Read::Spawn) && source.paced() {
            if let std::collections::btree_map::Entry::Vacant(slot) = regions.pending.entry(id) {
                let Ok(Some(ticket)) = source.start(&region.file.content, false) else { return Ok(false) };
                slot.insert(ticket);
                regions.events.push((tick, id, "requested"));
            }
            if !optional {
                regions.required_reads.insert(id);
            }
            if !regions.pending[&id].ready() {
                return Ok(false);
            }
        }
        let ticket = regions.pending.remove(&id);
        let bytes = match region.file.content.received(source, ticket.as_ref()) {
            Err(_) if optional => {
                if let Read::Address { retry_at } = purpose {
                    regions.retry_at.insert(id, retry_at);
                    regions.events.push((tick, id, "address_retry"));
                }
                return Ok(false);
            }
            bytes => bytes.with_context(|| format!("reading support region {id}"))?,
        };
        self.read_catalogue(idx, &region.tiles, &region.file, &bytes, region.complete_pages)
            .with_context(|| format!("validating support region {id}"))?;
        let regions = self.regions.as_mut().unwrap();
        regions.required_reads.remove(&id);
        regions.retry_at.remove(&id);
        regions.loaded.insert(id);
        regions.events.push((tick, id, "installed"));
        Ok(true)
    }

    pub(in crate::paging) fn prepare_address(
        &mut self,
        tick: u32,
        hz: u32,
        i: usize,
        idx: &Index,
        dir: &Source,
    ) -> Result<crate::paging::reader::PageSource> {
        use crate::paging::reader::PageSource;
        let region = self.regions.as_ref().and_then(|r| r.for_tile[i]);
        let bundled = region.is_some_and(|id| idx.support_regions[id].complete_pages);
        if !bundled && idx.physical_file(i).is_some() {
            return Ok(PageSource::File);
        }
        let id = region.context("unmapped collision address")?;
        if !self.load_region(tick, Read::Address { retry_at: tick.saturating_add(hz) }, id, idx, dir)? {
            return Ok(PageSource::Pending);
        }
        Ok(if bundled { PageSource::Region } else { PageSource::File })
    }

    pub(in crate::paging) fn prepare_tile(&mut self, tick: u32, i: usize, idx: &Index, dir: &Source) -> Result<bool> {
        if let Some(id) = self.regions.as_ref().and_then(|r| r.for_tile[i]) {
            return self.load_region(tick, Read::Support, id, idx, dir);
        }
        Ok(true)
    }

    pub(in crate::paging) fn prepare_step(
        &mut self,
        tick: u32,
        idx: &Index,
        dir: &Source,
        world: &World,
        parts: &[Bounds],
        upcoming_pages: impl FnOnce() -> Vec<usize>,
    ) -> Result<()> {
        let Some(regions) = self.regions.as_mut() else { return Ok(()) };
        let tree = idx.physical_pages();
        let mut needed = BTreeSet::new();
        let mut close = BTreeSet::new();
        let mut immediate = |i: usize| {
            close.insert(i);
            needed.insert(regions.for_tile[i].expect("physical region"));
        };
        tree.overlapping(world.sweep_bounds(), &mut immediate);
        for part in parts {
            tree.overlapping(*part, &mut immediate);
        }
        if let Some(sweep) = world.upright_sweep() {
            tree.swept_capsule(&sweep, |i| {
                needed.insert(regions.for_tile[i].expect("physical region"));
                if !sweep.excludes_start_bounds(idx.tiles[i].physics.as_ref().unwrap().bounds) {
                    close.insert(i);
                }
            });
        }
        // Approaching pages can start on the normal I/O workers. A speculative
        // read is optional, but a required region must validate before solving.
        if dir.paced() {
            let upcoming: BTreeSet<_> =
                upcoming_pages().into_iter().map(|i| regions.for_tile[i].expect("physical region")).collect();
            needed.extend(regions.required_reads.iter().filter(|id| regions.pending[id].ready()));
            regions
                .pending
                .retain(|id, _| needed.contains(id) || upcoming.contains(id) || regions.required_reads.contains(id));
            for id in upcoming {
                if !needed.contains(&id)
                    && regions.retry_at.get(&id).is_none_or(|&due| due <= tick)
                    && regions.pending.len() < 4
                    && !regions.loaded.contains(&id)
                    && !regions.pending.contains_key(&id)
                    && let Ok(Some(ticket)) = dir.start(&idx.support_regions[id].file.content, false)
                {
                    regions.pending.insert(id, ticket);
                    regions.events.push((tick, id, "prefetched"));
                }
            }
        }
        for id in needed {
            self.load_region(tick, if tick == 0 { Read::Spawn } else { Read::Support }, id, idx, dir)?;
        }
        close.retain(|&i| !self.region_ready(i));
        self.regions.as_mut().unwrap().threats = close;
        Ok(())
    }

    pub(in crate::paging) fn region_report(&self) -> Option<Value> {
        self.regions.as_ref().map(|r| json!({"loaded":r.loaded.len(), "events":r.events}))
    }
}
