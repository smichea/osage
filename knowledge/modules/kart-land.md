---
type: Module
title: osage-kart-land
description: The land around the kart circuit as geometry — flat where karts drive, hills beyond, a knoll and a lake inside the loop — for whoever draws, dresses or plants it.
resource: https://github.com/smichea/osage/tree/main/worlds/kart/components/land
licence: MIT OR Apache-2.0
status: draft
tags: [kart, terrain, component]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-11T02:00:00Z" }
---

# Purpose

One answer, shared by every part of the kart game that needs it, to: how high is the ground here, how far is the track, is this in the lake. A component of the kart game (D-035), split from the native client's drawing code.

# Concepts

- **Frame:** a point of the centre line with the directions ahead and to the right, the distance along, and how sharply the road bends.
- **Site:** a disc kept clear — the knoll, the lake, a clearing.
- **Land:** the height of the ground everywhere, the distance to the track, inside or outside the loop, the lake's shore.

# Architecture

The algorithm is [land around a track](/algorithms/land-around-a-track.md): value noise in three octaves, flattened along the track and at the edge of the modelled land, with a knoll and a lake placed in the widest rooms inside the loop. No drawing here: the colour of the ground belongs to whoever draws it.

# Interfaces

`frames(track)`, `Land::{new, height, distance_to_track, inside, shore, wet, centre}`, `Site`, `noise`, `smooth`, and the constants `LAND`, `HILLS`, `KNOLL`, `LAKE_BED`, `WATER`, `BARRIER`.

# Invariants

- The land is flat where karts drive, up to the barriers.
- The knoll and the lake are inside the loop, clear of the track and of each other; the lake's water is held in all the way round.

# Knowledge

- Algorithm: [land around a track](/algorithms/land-around-a-track.md).
- Used by [osage-kart-roadside](/modules/kart-roadside.md) and [osage-kart-native](/modules/kart-native.md).
- Oracle: the crate's tests.

# Reuse

Any circuit given as a closed centre line. Permissive licence.

# Limits

The distance to the track is measured to the centre line's points, not its segments: fine for points a few metres apart.
