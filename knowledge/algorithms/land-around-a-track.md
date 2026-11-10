---
type: Algorithm
title: Land around a track
description: Hills generated from noise that stay flat along a circuit and fade out at the edge of the modelled land.
resource: https://github.com/smichea/osage/blob/main/worlds/kart/native/src/track.rs
licence: MIT OR Apache-2.0
status: draft
tags: [terrain, procedural]
sources:
  - id: own
    resource: "osage's own design and measurements, 2026-11-09 and 2026-11-10"
    title: osage design
generated: { by: "claude-code/claude-fable-5-2", at: "2026-11-10T06:45:00Z" }
---

# Problem

A circuit given only as a flat centre line should sit in a landscape, without the landscape ever touching the road, and without storing a height map.

# Idea

Multiply a noise by two masks: one that is zero near the track and one that is zero at the border of the land[^own].

# Method

For a point `p`:

- `d` = distance from `p` to the nearest point of the centre line.
- `rise = smooth((d − flat) / 70)`, where `flat` is the road's half width plus the verge plus 5 m, and `smooth(t) = t²(3 − 2t)` clamped to 0…1.
- `fade = smooth((half_side − max(|p − centre|)) / 120)`.
- `hills = 0.5·n(p/150) + 0.3·n(p/61 + 17) + 0.2·n(p/23 + 5)`, with `n` a value noise in 0…1 built from an integer hash, so that it is the same on every machine.
- `height = 26 · rise · fade · hills`.

The land is a regular grid of 9 m cells over a 900 m square; each vertex takes its height from the function and its normal from the heights 1 m away on each side. Anything placed on the land (trees) asks the same function for its height. A plain a little below zero continues to the horizon.

# Invariants

The height is exactly zero within `flat` of the centre line: the road, drawn 2 cm above zero, is never buried.

# Cost

Once, when the circuit is known: about 50 000 evaluations, each a search over the centre line's points.

# Failure modes

A grid coarser than the track corridor's width would cut corners into the flat zone; the 5 m margin covers a 9 m cell.

# Alternatives

A stored height map (authoring cost, download size); terrain patches by level of detail (needed for worlds larger than one circuit).

# Oracle

[The render bench](/oracles/render-bench.md) screenshots: the road is visible everywhere.

# Reuse

The two masks apply to any path-shaped feature — a road, a river, a railway — in any terrain generator.

[^own]: osage design
