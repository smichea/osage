---
type: Module
title: osage-kart-roadside
description: What stands along the kart circuit when the asset manifest's models are there — tyre walls, grass, wild plants, rocks, pines, firs and broadleaf trees — placed the same way on every machine.
resource: https://github.com/smichea/osage/tree/main/worlds/kart/components/roadside
licence: MIT OR Apache-2.0
status: draft
tags: [kart, scenery, vegetation, component]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-11T14:30:00Z" }
---

# Purpose

Dress the kart circuit as a real one: the photorealistic counterpart of the procedural scenery, a component of the kart game (D-035).

# Concepts

- **Detail:** `Some` (tyre walls, bushes, boulders, trees: few and large) or `Full` (also grass, wild flowers, ferns, low plants and stones). The kart client uses `Full` from T3; below, the integrated GPUs of the reference cannot afford the models.
- **Roadside:** the barrier line, `half width + verge + barrier` from the centre line: plants grow beyond it, tyres stand just inside it.

# Architecture

One pure function, `scatters`, from the land ([osage-kart-land](/modules/kart-land.md)), the track's frames, the clearings other builders keep free, the models and the detail, to a list of `osage_props::Scatter`. Plants are placed near the track, where they are seen: a random point along the track, a side, a distance beyond the barrier, rejected if not free (on the road side, in the lake or on its beach, in a clearing). Pines grow in groves where a low-frequency noise is high, near the track in clumps of a tall sapling and a few smaller ones, which read as one full crown from afar. Firs and broadleaf trees, full grown, stand among the pines near the track where another noise is high; their leaves are geometry marked as blended with textures that have no transparency, so they are drawn opaque. Tyres lie flat in stacks of three, two rows deep, along the outside of every bend. Off the circuit, from 110 m to the edge of the modelled land, `woods` grows groves 14 to 45 m across where a noise is high, a tree every 20 m² or so and never two in a 3.6 m cell — pines, firs and broadleaf trees, some 18 000 — drawn as [tree cards](/algorithms/tree-cards.md): from T3, no procedural tree is needed. Each set has a distance it is seen up to, by its size: a tuft of grass 45 m, a pine by the track 450 m, a fir or a broadleaf tree 500 m; trees, bushes and rocks get coarser meshes with distance ([osage-props](/modules/osage-props.md)): foliage is thinned, rocks clustered.

# Interfaces

`Models::load(server)`, `Models::trees(server)` (the trees alone, for the woods), `Detail`, `scatters(land, frames, track, clearings, models, detail)`, `woods(land, frames, track, clearings, models, from)` and `WOODS_FROM` (110 m, where modelled trees stand nearer; a client with nothing nearer may bring the woods closer), `carpet(land, frames, track, models)`: grass around the camera, mown short on the verges, tall beyond the wall, none on the road, kerbs, wall, gravel traps or in the lake. All the grass is lit as the ground it grows from, in the meadow's green.

# Invariants

- Nothing grows on the road side, in the lake or in a clearing, and everything stands on the ground. Tested.
- The woods are dense, at least `from` metres from the track, on the land, one tree per cell. Tested.
- Tyre walls line only the outside of bends, between the verge and the barrier, flat. Tested.
- The same circuit gives the same roadside: a seeded generator, no clock. Tested.

# Knowledge

- Built on [osage-props](/modules/osage-props.md) and [osage-kart-land](/modules/kart-land.md); used by [osage-kart-native](/modules/kart-native.md).
- Oracle: the crate's tests and [the render bench](/oracles/render-bench.md) screenshots.

# Reuse

The placement pattern — sample beside a path, reject by predicates, seeded — fits any track or road. The kart's numbers are its own.

# Limits

Dense sets near the track are limited by count and distance; the woods are cards, so they must stay far. Grass is tufts, not a field: the meadows' grass is a texture.
