---
type: Module
title: osage-kart-tracks
description: "The kart game's tracks as data: a plain text format (centre line, road and verge widths, crests, start, sky, scenery), checked for crossings, bends too tight and crests in bends, and built into the physics' Track; four tracks come with the game."
resource: https://github.com/smichea/osage/tree/main/worlds/kart/components/tracks
licence: MIT OR Apache-2.0
status: draft
tags: [kart, tracks, format, validation, component]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-12T19:30:00Z" }
---

# Purpose

What a race is run on (D-056). A component of the kart game (D-035): the world builds its circuit from a track, clients receive the result in `kart.track/1` and dress it.

# Concepts

- **Track file:** `kart-track/1`, then a key and its values a line, `#` for comments: `id`, `name`, `about`, `sky` (noon, evening), `scenery` (meadow, coast, forest, town), `half-width`, `verge`, `point X Z [start]` (in racing order; one marked start), `crest AFTER RISE FALL HEIGHT` (metres past the start line), `rain CHANCE` (0 to 1: how often a race on it is run in the rain, D-068; none unless said); a point's third number is the road's height there (D-077), `bridge AFTER LENGTH` and `tunnel AFTER LENGTH` spans of scenery. A road steeper than 16 % or a grid on a slope is refused. Six tracks come with the game: the four of the Grand Tour, Eagle Ridge (mountain: 40 m of climb, a tunnel, a bridge) and Red Gorge (canyon: two bridges, a tunnel).
- **Centre line:** the control points joined by a closed Catmull-Rom spline, ten samples a span — the same construction the world used before tracks were data.
- **Problems:** what makes a track unfit to race; the world refuses a track with any.

# Architecture

`TrackDef::parse` reads a file and says the line of an error; `centre_line` samples the spline; `build` makes the physics' `Track` with its crests and gives the distance of the start line; `problems` checks: half width 4–12 m, verge 1–10 m, length 400–3000 m; two stretches further apart along the loop than 2.5 times the road's room (both roads and verges, plus 2 m) never come within that room of each other; the tightest bend (heading over 12 m) is at least `TIGHTEST` (12 m of radius); a crest is 4 m or more up, 3 m or more down, at most 3 m high, on road that turns less than 0.25 rad from just before it to where a kart lands; the start and its grid likewise. `built_in()` gives the tracks that come with the game, in the order of a cup; `by_id`.

Battle arenas (D-065) are track files too, kept apart (`ARENAS`, `arenas`, `arena`) and checked by `arena_problems`: as a track, but 200 m long at least instead of 400, and with no grid to keep straight. One comes with the game: the Yard, a 280 m stadium bowl 24 m wide.

# Interfaces

`TrackDef`, `CrestDef`, `ParseError`, `HEADER`, `SAMPLES_PER_SPAN`, `TIGHTEST`, `BUILT_IN`, `built_in`, `by_id`, `tightest`. In the world: `Circuit::of(&TrackDef)`, `KartWorld.track`; `kart-server --track ID|FILE`; `kart-native --bench --track ID|FILE`.

# Invariants

- Every built-in track reads and has no problem; ids are distinct. Tested.
- A figure of eight and a square with sharp corners are refused; a file that does not read says which line. Tested.
- The Paddock is the circuit it was: the same control points, start and crest. Tested.
- Eight bots race every built-in track and keep to its road (the world's `tests/bots.rs`).

# Knowledge

- Built-in tracks: the Paddock (661 m, a club circuit in the hills, tightest bend 18 m), Gull Point (883 m, fast, along cliffs, 15 m), Pinewood (692 m, narrow, at dusk, 12.3 m), Harbour Town (699 m, a street circuit of right angles rounded at 17 m).
- Shapes are best drawn with a plot of the spline and the checks side by side: a corner made of one control point is a V; two control points a few metres apart round it.

# Reuse

Any closed-circuit racing game whose road is a centre line and a width: the format, the checks and the spline need nothing of karts beyond the physics' `Track`.

# Limits

No elevation but crests, no banking, no surfaces beyond the physics' road, kerb, verge and grass, no item rows (the world places crates from the road); a sky and a scenery are names clients dress as they can.
