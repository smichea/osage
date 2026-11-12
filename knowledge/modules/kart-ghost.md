---
type: Module
title: osage-kart-ghost
description: "A kart's run kept to race against — the state it started from and the steps applied in each tick, driven again by the deterministic physics to the same bit — in a plain text file per track, laps and driver; the kart world's time trials are built on it."
resource: https://github.com/smichea/osage/tree/main/worlds/kart/components/ghost
licence: MIT OR Apache-2.0
status: draft
tags: [kart, ghost, time-trial, replay, determinism, persistence, component]
generated: { by: "claude-code/claude-fable-5-2", at: "2026-11-12T23:30:00Z" }
---

# Purpose

Racing one's own best, and a friend's: the ghost of a time trial (design/04 G4.1, D-062). A component of the kart game (D-035): the world server records and drives the ghosts, its clients only draw them.

# Concepts

- **Tick:** what the world applied to a kart in one of its ticks: one step as a rule, none while it waited for a late step of its driver's, two when it caught up ([exact client prediction](/algorithms/exact-client-prediction.md)).
- **Ghost:** a run — the track's id, who drove, the driver and kart chosen, how many laps in how many milliseconds, when, the kart as the run started (its build included) and its ticks. Not a path: the run is driven again.
- **Shelf:** the ghosts a world keeps, each driver's best run per track and number of laps.
- **Time trial (in the world):** the event `trial:TRACK` — every driver alone against the clock.

# Architecture

`Recorder::new(start)` then `tick(steps)` once per tick of the world gives a `Ghost`; `Ghost::drive(track, each)` steps a kart from the start through every tick with [osage-kart-physics](/modules/kart-physics.md) and shows it after each; `Replay` hands out a tick at a time, for a world that drives it beside the other karts. The physics has the same bits on every platform, so the ghost is where the kart was at every tick.

The file is text: the line `kart-ghost/1`, a line a field (`track`, `name`, `driver`, `kart`, `laps`, `ms`, `at`, `start` with the sixteen numbers of the kart's state, `build` with five, `ticks N`), then the ticks, runs of the same tick on one line: `RUN STEPS [THROTTLE STEER DRIFT]…`. Numbers are written as Rust prints them, which reads back to the same bits. `Ghost::from_text` gives the ghost whole or says what is wrong: a build that is not a kart's, controls beyond their range, a count that does not match, more than `LONGEST` (36 000) ticks. `Shelf` keeps the files in `FOLDER/TRACK.LAPS/NAME-HASH.ghost` (the name made safe for a file, a hash of the real one beside it), written to `.new` then renamed; files that do not read are listed in `unread`, not fatal.

In the world (`osage-world-kart`, `trial.rs`, `driving.rs`, `race.rs`): in a trial the grid is the drivers alone, all on the first place, then the ghosts `chosen` — each driver's own best, then the track's fastest, `SHOWN` (4) at most — with an empty paddock ([osage-kart-items](/modules/kart-items.md)); karts neither collide nor draft. A human's or an agent's run is recorded from the tick the lights go (after the start is judged) to the tick it finishes, and offered to the shelf (`trial::keep`). A ghost's kart (`Pilot::Ghost`) takes the run's start as the lights go, applies a tick's steps each tick, and is brought round by the autopilot once the run is over. Before a ghost is shown, `holds` drives it once: it must start on the grid's first place, finish its laps in its last tick, and say the time of that tick. The trial's clock is ticks (`trial::clock`), not the machine's. Finished trials go to the records as `Event::Trial` ([osage-kart-records](/modules/kart-records.md)). `kart-server --ghosts DIR` names the folder; the Windows launchers use `kart-ghosts` beside the game.

# Interfaces

`Tick::{of, steps}`, `Ghost::{drive, to_text, from_text}`, `Recorder::{new, tick, ticks, ghost}`, `Replay::{new, ghost, start, next_tick}`, `Shelf::{open, of, all, keep, forget, unread}`, `HEADER`, `LONGEST`. Protocol: `Racer.ghost`, `Race.trial`, `Records.trials`, the events `trial:ID` in `Race.events` and `Choice.event`.

# Invariants

- A ghost driven again is the run, kart for kart at every tick, ticks of no step and of two included. Tested.
- What is written reads back the same; a damaged file is no ghost, with why. Tested.
- A shelf keeps each driver's best, two names that make the same file name apart, survives a restart, tells of files that do not read, leaves no `.new` behind. Tested.
- In the world: a trial has no bots and no crates; the run is kept, and in the next trial its ghost starts from the same place and finishes in the same time to the millisecond; a slower run leaves the ghost, a faster one replaces it, a world started again has it; a ghost that no longer finishes when it says is left out (the world's `tests/race.rs`).

# Knowledge

- Rests on [arcade kart physics](/algorithms/arcade-kart-physics.md) being deterministic, as client prediction does (D-031).
- Drawn by [osage-kart-native](/modules/kart-native.md) seen through, silent, without smoke or sparks; by the browser client seen through.
- The oracle is the crate's tests and the world's `a_time_trial_keeps_a_ghost_that_drives_the_run_again`.

# Reuse

Any game whose simulation is deterministic and stepped by inputs: record the inputs per tick, replay them. The format's fields are a kart's; the recorder, the replay and the shelf are not.

# Limits

A change to the physics or to a track's shape retires the ghosts recorded before it. Only what the steps decide is replayed: a run with items or knocks (a race) is not one yet (G4.7). A ghost proves a run exists that the physics drives in that time, not who drove it (trust by default, D-038). Times are whole ticks (16.7 ms).
