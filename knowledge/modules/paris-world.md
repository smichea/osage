---
type: Module
title: osage-world-paris
description: The Paris world — a walk on foot in Amazon Lumberyard's Bistro — its walk grid and walkers, the tiles of light coming into each walker's block, and the server that sends them as each walker's place and speed ask.
resource: https://github.com/smichea/osage/tree/main/worlds/paris/world
licence: MIT OR Apache-2.0
status: draft
tags: [paris, world, streaming, light]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-18T13:40:00Z" }
---

# Purpose

A small world to show a street as real as a photograph, walked on foot, where a client draws in 3D only what is around its walker and receives all else as the light coming into it (D-136), taken inside the walker's cube (D-137).

# Concepts

- **Walk grid:** cells of a quarter metre over the street, each with the height of its ground, or none where nobody stands; made from the street's model by `tools/bistro/export.py`.
- **Block:** the 9 cubes of 3 m around the cube the walker's eyes are in, one layer: what its client traces.
- **Cube's probes (D-137):** five full spheres inside a 3 m cube at the eyes' height, the centre and 0.75 m towards each corner, seeing through the cube's block what comes into it; named `c_X_Y_Z_K`.
- **Tile (D-136, where a cube's probes are not baked):** a face of a 3 m cube on a block's outer surface, its probe at its centre looking out; the same tile for every block it bounds.
- **Light coming in:** what a probe sees — radiance and distance — sent as AV1 ([osage-hologram-stream](/modules/osage-hologram-stream.md)'s `light`).

# Architecture

`walk::Walk` reads the grid; `step` moves a walker by its stride (forward and right relative to its heading, walking or running), sliding along what stops it and never up a step higher than 0.4 m; the server moves walkers with it and each client predicts its own the same way. `tiles::Block::of(eyes)` is a walker's block and `tiles()` its 30 tiles; `tiles::ahead(eyes, velocity, horizon)` the cubes the walker meets by then going straight on, a metre either side of its way, its own first, `probe_keys(cube)` a cube's probes, and `needed_light(eyes, velocity, horizon, there)` what to send (D-139): the centres' probes of those cubes and of the four beside its own, then the tiles of the cubes without probes, then, standing (under 0.5 m/s), its own cube's four other probes. `ParisWorld` (a world module) gives each person and agent a walker at the street's start, takes their `paris.stroll/1` intents, steps them every tick, announces its server of light in `paris.world/1`, and tells `Walkers` where each walker is. `paris-server` loads the grid and the baked tiles, serves the world, and beside it answers `/need?walker=ID` from the walker's place and velocity in the world — not from what its client says — then sends each probe's or tile's files (a probe's `meta.json` says where it stands). `paris-tiles` lists the tiles a street needs, for the baker.

# Interfaces

`ParisWorld { walk, walkers, light_port, horizon }`, `Walkers`, `Seen`, `TICK_HZ`, `types::{STROLL, WALKER, WORLD}`, `proto::{Stroll, Walker, World}`; `walk::{Walk, Stride, WALK, RUN, EYES, RADIUS, STEP}`; `tiles::{Block, Tile, Face, CUBE, PROBES, BESIDE, STANDING, ahead, needed_light, probe_keys}`; the programs `paris-server` and `paris-tiles`.

# Invariants

- A walker goes where it faces at its pace; walls and bollards stop it and it slides along them; the ground is smooth and a kerb a step; a grid round-trips through its files. Tested.
- A block has 30 tiles on its surface, each looking out; neighbouring blocks share most of theirs; a walker needs more ahead when it goes faster; a cube with its probes baked sends them, one without its block's tiles. Tested.

# Knowledge

- Decisions: D-134, D-135, D-136 (the client traces the 9 cubes around the walker and receives only the light coming into them), D-137 (that light taken by five probes inside the walker's cube).
- Its client: [osage-paris-native](/modules/paris-native.md). Its light travels with [osage-hologram-stream](/modules/osage-hologram-stream.md).

# Reuse

Any world walked on foot over a model, where clients should draw only what is near: the walk grid and the tiles do not depend on the street.

# Limits

The walk grid is a height field: one floor a cell, no stairs inside buildings, no bridges over walkers. The tiles are baked for a fixed light (Bistro's sky); a world that moves (doors, crowds, weather) needs them made again, or computed live.
