---
type: Module
title: osage-kart-land
description: The land around the kart circuit as geometry — the road's height where karts drive (level but over a crest), hills beyond, a knoll and a lake inside the loop — for whoever draws, dresses or plants it.
resource: https://github.com/smichea/osage/tree/main/worlds/kart/components/land
licence: MIT OR Apache-2.0
status: draft
tags: [kart, terrain, component]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-11T02:00:00Z" }
---

# Purpose

One answer, shared by every part of the kart game that needs it, to: how high is the ground here, how far is the track, is this in the lake. A component of the kart game (D-035), split from the native client's drawing code.

# Concepts

- **Frame:** a point of the centre line with the directions ahead and to the right, the distance along, how sharply the road bends, the road's height and slope there, and the crest on the stretch to the next frame, if any (D-051): `pieces` says how many pieces to draw that stretch in, `height_towards` how high the road is on the way, exactly as the physics has it.
- **Site:** a disc kept clear — the knoll, the lake, a clearing.
- **Land:** the height of the ground everywhere, the distance to the track, inside or outside the loop, the lake's shore.
- **Hills, gorges, ridges, walls** (D-077): over a hilly track the land is the road's height beside it and the country's — about the road's — further off; `with_spans` lays a gorge under each bridge (16 m, `GORGE`) and a ridge beside each tunnel (12 m, `RIDGE`; the vault is `VAULT` high; up to `TUNNEL_CLEAR` past the barrier's face the land keeps under the road, so that no slope the land's grid joins crosses the vault, D-080), `with_relief` makes the hills taller (a mountain's), `with_walls` stands rock walls along the track (a canyon's: cliffs, D-081 — faces and ledges in steps of 8 m whose heights wander, a third of each step nearly flat; spurs and gullies every few metres pushing the face out and in; some faces slumped).
- **Sea:** on a coast (`with_sea`, D-061), the open side of the circuit falls past cliffs to a sea `SEA` (8 m) below the road; the shoreline wanders, 30 m beyond the farthest flat ground; `offshore(at)` is how far a point is out to sea (negative inland).

# Architecture

The algorithm is [land around a track](/algorithms/land-around-a-track.md): value noise in three octaves, flattened along the track and at the edge of the modelled land, raised with the road over a crest (and fading into the hills over 30 m), with a knoll and a lake placed in the widest rooms inside the loop. No drawing here: the colour of the ground belongs to whoever draws it.

# Interfaces

`frames(track)`, `Frame::{pieces, height_towards}`, `Land::{new, with_sea, height, road, distance_to_track, inside, shore, wet, offshore, centre}`, `Site`, `noise`, `smooth`, and the constants `LAND`, `HILLS`, `KNOLL`, `LAKE_BED`, `WATER`, `SEA`, `SEA_BED`, `BARRIER`.

# Invariants

- Where karts drive, up to the barriers, the land is the road's height: level, but over a crest, where it rises and falls with the road.
- Between frames, `height_towards` is the physics' own height of the road, to a millimetre.
- On a coast, the road runs above cliffs and the sea bed lies under the water; where karts drive nothing changes. Tested.
- The knoll and the lake are inside the loop, clear of the track and of each other; the lake's water is held in all the way round.

# Knowledge

- Algorithm: [land around a track](/algorithms/land-around-a-track.md).
- Used by [osage-kart-roadside](/modules/kart-roadside.md) and [osage-kart-native](/modules/kart-native.md).
- Oracle: the crate's tests.

# Reuse

Any circuit given as a closed centre line. Permissive licence.

# Limits

The distance to the track is measured to the centre line's points, not its segments: fine for points a few metres apart.
