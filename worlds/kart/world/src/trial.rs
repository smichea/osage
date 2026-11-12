// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Time trials (G4.1, D-062): the event `trial:TRACK`. Every driver alone
//! against the clock, from the same place on the grid: no bots, no crates,
//! karts passing through one another. A run that beats its driver's best
//! is kept as a ghost ([`osage_kart_ghost`]) — what the kart did tick by
//! tick — and driven again in the trials that follow: each driver's own
//! best and the fastest of the track, a few at most.
//!
//! A ghost read from a file is driven once before it is shown: one that no
//! longer starts on the grid and crosses the line when it says — the track
//! or the physics changed since — is left out.

use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use bevy::prelude::*;
use osage_kart_ghost::{Ghost, Shelf};
use tracing::warn;

use crate::{Circuit, Course, Rules, circuit::SECTORS, driving::Launch, race::Progress};

/// Ghosts driven in one trial, at most.
pub const SHOWN: usize = 4;

/// The world's ghosts.
#[derive(Resource)]
pub struct Ghosts(pub Shelf);

/// A trial's clock after `ticks` ticks of the world: trials are timed in
/// ticks, so that a run driven again takes the same time to the
/// millisecond, however busy the machine the world runs on.
pub fn clock(ticks: u64) -> Duration {
    Duration::from_nanos(ticks * 1_000_000_000 / u64::from(crate::TICK_HZ))
}

/// Moves `sector` on when a kart `from_start` metres round the circuit has
/// entered the next one; true when that is the first: the line is crossed.
/// Laps count through every sector in order.
pub fn enters(from_start: f32, sector_length: f32, sector: &mut u32) -> bool {
    let now = ((from_start / sector_length) as u32).min(SECTORS - 1);
    if now != (*sector + 1) % SECTORS {
        return false;
    }
    *sector = now;
    now == 0
}

/// Whether `ghost`, driven again on `circuit`, starts where trials start
/// and finishes its laps in its last tick, in the time it says: it is the
/// run it says.
pub fn holds(ghost: &Ghost, circuit: &Circuit) -> bool {
    let grid = circuit.grid_slot(0);
    // A trial is timed in ticks: its time is its ticks'.
    let ticked = clock((ghost.ticks.len() as u64).saturating_sub(1)).as_millis() as u64;
    if ghost.ticks.is_empty() || (ghost.start.x - grid.x).hypot(ghost.start.z - grid.z) > 0.5 || ghost.ms != ticked {
        return false;
    }
    let (mut lap, mut sector, mut finished) = (0, SECTORS - 1, None);
    ghost.drive(&circuit.track, |tick, kart| {
        let from_start = circuit.from_start(circuit.track.locate(kart.x, kart.z).along);
        if enters(from_start, circuit.sector_length(), &mut sector) {
            lap += 1;
            if lap > ghost.laps {
                finished.get_or_insert(tick);
            }
        }
    });
    finished == Some(ghost.ticks.len() - 1)
}

/// The ghosts to drive in a trial of `laps` laps on `circuit` with these
/// drivers: each one's best, then the fastest of the track, [`SHOWN`] at
/// most. Those that no longer hold are forgotten.
pub fn chosen(shelf: &mut Shelf, circuit: &Circuit, laps: u32, drivers: &[String]) -> Vec<Arc<Ghost>> {
    let track = circuit.about.id.as_str();
    let all = shelf.all(track, laps);
    for (path, why) in shelf.unread.drain(..) {
        warn!("ghosts: {}: {why}", path.display());
    }
    let mut chosen: Vec<Arc<Ghost>> = Vec::new();
    let own = all.iter().filter(|ghost| drivers.contains(&ghost.name));
    for ghost in own.chain(all.iter()) {
        if chosen.len() >= SHOWN {
            break;
        }
        if chosen.iter().any(|c| c.name == ghost.name) {
            continue;
        }
        if holds(ghost, circuit) {
            chosen.push(ghost.clone());
        } else {
            warn!("ghosts: {}'s run on {track} is no longer one: left out", ghost.name);
            shelf.forget(ghost);
        }
    }
    chosen
}

/// A trial finished: the run is offered to the shelf.
pub fn keep(
    rules: Res<Rules>,
    course: Res<Course>,
    mut ghosts: ResMut<Ghosts>,
    mut karts: Query<(&Progress, &mut Launch)>,
) {
    for (progress, mut launch) in &mut karts {
        let (Some(ms), true) = (progress.finish_ms, launch.run.is_some()) else { continue };
        let Some(run) = launch.run.take() else { continue };
        let about = Ghost {
            track: course.0.about.id.clone(),
            name: progress.name.clone(),
            driver: progress.pair.0.id.to_owned(),
            kart: progress.pair.1.id.to_owned(),
            laps: rules.laps,
            ms,
            at: SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()),
            start: default(),
            ticks: Vec::new(),
        };
        let Some(ghost) = run.ghost(about) else { continue };
        if let Err(e) = ghosts.0.keep(ghost) {
            warn!("ghosts: {}'s run: {e}", progress.name);
        }
    }
}
