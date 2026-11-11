---
type: Module
title: osage-kart-native
description: The native client of the kart world, built on Bevy, and the testbed of the rendering programme with its benchmark.
resource: https://github.com/smichea/osage/tree/main/worlds/kart/native
licence: MIT OR Apache-2.0
status: draft
tags: [kart, client, bevy, benchmark]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-10T09:30:00Z" }
---

# Purpose

Show that a native client joins the same world as a browser, and give rendering work a real scene to be measured on.

# Concepts

- **Pose:** a kart to draw, as it is this frame. Whoever computes the karts writes poses; the scene only draws them.
- **Two sources of poses:** the network replica when playing, a local simulation in the bench.
- **Content is optional:** with the files of the asset manifest, surfaces are real materials; without, plain colours. The game runs either way.

# Architecture

Each frame has two stages. In `Simulate`, one of two plugins writes the poses: `play` applies what the world sent and predicts the player's kart; `bench` steps a local race. In `Draw`, the scene modules read the poses: `track` (road, markings, kerbs, mown verges, barriers, the land with its knoll and lake), `scenery` (trees, bushes, flowers, rocks, clouds, balloons, mountains) and `landmarks` (start gantry and grandstand, tyre stacks and chevrons, arches, windmill, jetty and boat, fair with a big wheel), both built with [osage-procedural](/modules/osage-procedural.md), `karts` (model and motion), `effects` (particles), and the chase camera. `hud` shows a `RaceView`, a plain description of the race that `play` fills.

With the asset manifest's content ([osage-content](/modules/osage-content.md)) the circuit is photographed instead of toy-like: the sky is a photograph ([a photographed sky](/techniques/panorama.md)), the road, verges, gravel traps, kerbs and walls get real surfaces, and from T2 the roadside is dressed with models ([osage-kart-roadside](/modules/kart-roadside.md), on [osage-props](/modules/osage-props.md)) in place of the toys; procedural trees stand only far away. The start is a club circuit's: a galvanised gantry, a grandstand of concrete tiers with plastic seats and spectators in everyday clothes, under a sheet-metal roof, chain-link fences between the walls and the spectators along the straights near the start, a pit building across the straight (light concrete, six roller doors, a sign band, a flat roof), and advertising boards with abstract marks, no real brand, behind the walls where the circuit bends gently. The land itself is a component ([osage-kart-land](/modules/kart-land.md)).

# Interfaces

Command line: a world's address to play, `--bench` to measure, `--tier`, `--without TECHNIQUE`, `--autodrive`, `--screenshot-after`, `--camera X,Y,Z,TX,TY,TZ` to hold the camera at a place (pictures of the landmarks; a bench run with it is not the sanctioned bench). `OSAGE_ASSETS` names the content directory, or `none`.

# Invariants

- The scene never reads the network, and the network code never draws.
- Everything in the scene is placed by repeatable arithmetic, never by a random number generator: the bench draws the same frames on every run.
- The player's kart is predicted with the world's own physics, so that prediction is exact.

# Knowledge

- Algorithms: [exact client prediction](/algorithms/exact-client-prediction.md), [timedemo bench](/algorithms/timedemo-bench.md), [land around a track](/algorithms/land-around-a-track.md), [sculpted scenery](/algorithms/sculpted-scenery.md), [particle pool](/algorithms/particle-pool.md).
- Module it draws with: [osage-render](/modules/osage-render.md). Physics: [osage-kart-physics](/modules/kart-physics.md).
- Oracle: [the render bench](/oracles/render-bench.md).

# Reuse

The split between "who computes poses" and "who draws them" lets the same scene serve a game, a replay and a benchmark. The procedural pieces (ribbons along a centre line, land that stays flat near a track) are described as algorithms and do not depend on Bevy.

# Limits

Karts are a real kart's shape and size ([osage-kart-model](/modules/kart-model.md)), moulded but without decals beyond a few stickers; the grandstand's spectators are mannequins, and the lake's jetty and boat are still toy-like. Modelled content for them comes with workstreams R8 and R9. The interface is not shown in the bench.
