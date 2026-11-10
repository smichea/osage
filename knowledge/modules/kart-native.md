---
type: Module
title: osage-kart-native
description: The native client of the kart world, built on Bevy, and the testbed of the rendering programme with its benchmark.
resource: https://github.com/smichea/osage/tree/main/worlds/kart/native
licence: MIT OR Apache-2.0
status: draft
tags: [kart, client, bevy, benchmark]
generated: { by: "claude-code/claude-fable-5-2", at: "2026-11-10T06:45:00Z" }
---

# Purpose

Show that a native client joins the same world as a browser, and give rendering work a real scene to be measured on.

# Concepts

- **Pose:** a kart to draw, as it is this frame. Whoever computes the karts writes poses; the scene only draws them.
- **Two sources of poses:** the network replica when playing, a local simulation in the bench.
- **Content is optional:** with the files of the asset manifest, surfaces are real materials; without, plain colours. The game runs either way.

# Architecture

Each frame has two stages. In `Simulate`, one of two plugins writes the poses: `play` applies what the world sent and predicts the player's kart; `bench` steps a local race. In `Draw`, the scene modules read the poses: `track` (road, markings, kerbs, barriers, land), `scenery` (trees, clouds, grandstand, tyre stacks), `karts` (model and motion), `effects` (particles), and the chase camera. `hud` shows a `RaceView`, a plain description of the race that `play` fills.

# Interfaces

Command line: a world's address to play, `--bench` to measure, `--tier`, `--without TECHNIQUE`, `--autodrive`, `--screenshot-after`. `OSAGE_ASSETS` names the content directory, or `none`.

# Invariants

- The scene never reads the network, and the network code never draws.
- Everything in the scene is placed by repeatable arithmetic, never by a random number generator: the bench draws the same frames on every run.
- The player's kart is predicted with the world's own physics, so that prediction is exact.

# Knowledge

- Algorithms: [exact client prediction](/algorithms/exact-client-prediction.md), [timedemo bench](/algorithms/timedemo-bench.md), [land around a track](/algorithms/land-around-a-track.md), [particle pool](/algorithms/particle-pool.md).
- Module it draws with: [osage-render](/modules/osage-render.md). Physics: [osage-kart-physics](/modules/kart-physics.md).
- Oracle: [the render bench](/oracles/render-bench.md).

# Reuse

The split between "who computes poses" and "who draws them" lets the same scene serve a game, a replay and a benchmark. The procedural pieces (ribbons along a centre line, land that stays flat near a track) are described as algorithms and do not depend on Bevy.

# Limits

Karts and scenery are built from primitive shapes; modelled content comes with workstreams R7 to R9. The interface is not shown in the bench.
