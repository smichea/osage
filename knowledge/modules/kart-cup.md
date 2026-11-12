---
type: Module
title: osage-kart-cup
description: "Cups of kart races: points by finishing place, standings between races with their tie-breaks, the podium; the world races a cup or one track, the host choosing and the others voting."
resource: https://github.com/smichea/osage/tree/main/worlds/kart/components/cup
licence: MIT OR Apache-2.0
status: draft
tags: [kart, cup, scoring, standings, component]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-12T21:00:00Z" }
---

# Purpose

Something to win over an evening (D-057): a few races on different tracks, points for each place, standings between them and a podium. A component of the kart game (D-035), pure scoring: a race's result is an order of names.

# Concepts

- **Cup:** an id, a name, its tracks in the order they are raced. Built in: the Grand Tour (paddock, coast, forest, harbour).
- **Points:** 10, 8, 6, 5, 4, 3, 2, 1 for first to eighth; none after.
- **Standing:** a driver's name, whether a bot, points, and their place in each race (none for a race missed).
- **Series:** a cup being raced: races run, standings so far.
- **Event (in the world):** what a world races — `cup:ID` or `track:ID`.

# Architecture

`Series::score(order)` gives each name its place's points in the race being run and moves on to the next track; a name twice counts once, at its first place; once over, a result changes nothing. `standings()` ranks by points, then by the count of each place from first down (more wins first), then by the place in the latest race, then by name.

In the world (`osage-world-kart`, `cup.rs`): `Cups` keeps the event, the series and each driver's wanted event. `next(host)` is the host's choice, or with no host the most wanted (votes, first of a tie in the order of `events()`), when it differs from the event raced; `advance` gives the track of the next race — the chosen event's first, the cup's next, its first again once over — and the race flow changes the circuit before lining up. Places are scored when a race ends, from the race's order; the last race of a cup keeps its results (the podium) up 16 s instead of 8. `kart.race/1` carries `cup` (name, tracks, race number, points, standings), `event`, `votes` and `events`.

# Interfaces

`POINTS`, `points`, `Cup`, `Standing`, `Series::{new, track, over, score, standings}`, `built_in`. Protocol: `Race.cup`, `Race.event`, `Race.votes`, `Race.events`, `Choice.event`; `kart-server --track` races one track instead of the cup.

# Invariants

- Places score points; a cup goes track by track and is over after its last; a fifth result changes nothing. Tested.
- More wins break a tie of points; the latest race breaks a tie of everything else; a late joiner starts from nothing. Tested.
- In the world: a whole cup is raced on its four tracks in order with standings after each race (the points of every place, first first), a podium that stays up, then a fresh cup on the first track; the host's choice changes the event at once on the grid, a non-host's is a vote (the world's `tests/race.rs`, `cup.rs`).

# Knowledge

- Bots score like everyone: a cup against the bots is a cup.
- Names are what standings are kept by, as records are (D-053): a driver who reconnects under the same name keeps their points.

# Reuse

Any racing world with several races in a row: the scoring and the ranking need nothing of karts.

# Limits

One built-in cup; no team standings, no handicaps; standings live as long as the world process (a restart starts the cup again).
