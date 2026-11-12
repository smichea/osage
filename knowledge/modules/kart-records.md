---
type: Module
title: osage-kart-records
description: "A kart world's records — best laps and best races per track, one per driver name, the fastest ten — kept by the world in a plain text file that survives a restart, published to clients and agents as kart.records/1."
resource: https://github.com/smichea/osage/tree/main/worlds/kart/components/records
licence: MIT OR Apache-2.0
status: draft
tags: [kart, records, leaderboard, persistence, component]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-12T14:30:00Z" }
---

# Purpose

Something to come back for: the best laps and races ever driven in a world, with who drove them and when (D-053). A component of the kart game (D-035): the world server keeps the records, its clients and AI agents read them. A board shared between worlds would be a service an operator runs from these, not the engine's (mechanisms, not policy).

# Concepts

- **Event:** what a time is of — a lap (line to line) or a race of a number of laps (lights to flag).
- **Board:** the times of one event on one track, fastest first, at most `KEPT` (10), one per driver name: that driver's best. A tie keeps the earlier time.
- **Record:** a name, a time in milliseconds, when (seconds since 1970), the kart body and the driver chosen ([osage-kart-roster](/modules/kart-roster.md), D-055).
- **Fresh:** set in the race just run; the world forgets which are fresh when the next race lines up.

# Architecture

`Records::offer(track, event, record)` puts a time on its board if it beats the driver's own best and makes the top ten, and says where (`Entered::At(place)`, 0 the record) or not (`Entered::No`). Names are kept on one line, without tabs or control characters, trimmed to 40 characters; an empty name sets nothing.

The file is text: the line `kart-records/1`, then a record a line, tab-separated: track, event (`lap` or `race:N`), ms, when, name, kart, driver. `save` writes it to `FILE.new` and renames it over `FILE`, so the file is always whole; `load` gives no records when there is no file, and skips any line that does not read (a file edited by hand keeps what it can); a file of another format reads as none.

In the world (`osage-world-kart`, `records.rs`): each lap and each race a human or an agent finishes is offered (bots set none), read from the race's progress — a lap from one crossing of the line to the next while racing, the race from the lights. What enters is saved at once and the race board's `kart.records/1` republished: the track, the best laps, the best races of the world's number of laps, each with `fresh`. `kart-server --records FILE` names the file; without it, records live as long as the process. The Windows launchers keep them in `kart-records.txt` beside the game, which updates leave alone.

# Interfaces

`Records::{offer, board, boards, to_text, from_text, load, save}`, `Event`, `Record`, `Entered`, `KEPT`, `HEADER`. The protocol: `kart.records/1` (`Records`, `Record` in `kart.proto`), described to agents by the world.

# Invariants

- A driver's slower time leaves its best; a faster one replaces it; a board keeps the ten fastest, the earlier of a tie first; laps, races of different lengths and tracks are separate boards. Tested.
- What is written reads back the same; odd names are cleaned; damaged lines and other formats are skipped. Tested.
- Saved and loaded again, the records are the same, and no `.new` file is left behind. Tested.
- In the world: an agent's lap and race enter fresh, are saved; a world started again on the file shows them, not fresh; a faster lap by the same driver replaces its own (the world's `tests/race.rs`).

# Knowledge

- Times come from the world's race (laps counted through every sector in order); the physics is deterministic, so a time is the same wherever it is replayed — the ground for checking a record against a replay later (G4.7).
- Shown by [osage-kart-native](/modules/kart-native.md) between races: the best five laps and races, the player's marked, new ones flagged.
- Shown by the browser client on its results the same way.

# Reuse

Any game with timed events and named players: the boards, the one-best-per-name rule and the atomic text file need nothing of karts.

# Limits

Names are what the world knows a driver by (its identity's per-world id): nobody proves a time was driven fairly beyond the world's own authority (trust by default, D-038). Boards are per track (the track's id, D-056); no per-kart or per-character boards; times in whole milliseconds as the world's clock gives them (a tick is 16.7 ms).
