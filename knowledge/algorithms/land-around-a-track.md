---
type: Algorithm
title: Land around a track
description: Hills generated from noise that stay flat along a circuit and fade out at the edge of the modelled land, with a knoll and a lake placed in the room the circuit leaves inside its loop.
resource: https://github.com/smichea/osage/blob/main/worlds/kart/components/land/src/lib.rs
licence: MIT OR Apache-2.0
status: draft
tags: [terrain, procedural]
sources:
  - id: own
    resource: "osage's own design and measurements, 2026-11-09 and 2026-11-10"
    title: osage design
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-10T09:30:00Z" }
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

**Features inside the loop.** Every 4 m over the circuit's bounding box, keep the points inside the closed centre line (even-odd crossing test) with their distance to it: the largest distance is the widest room. A knoll goes in the widest room, a lake in the widest room that does not overlap it, each with a radius of its room's distance less `flat` and 6 m, at most 30 m and 26 m. The knoll adds `7 · smooth(1 − |p − c| / r)`. For the lake, a shore that is not a circle: `s = |p − c| / (r · (0.84 + 0.11 cos(2a + 0.9) + 0.05 cos(5a + 2)))`, with `a` the angle of `p` around `c`; hills are multiplied by `smooth((s − 1) / 0.6)` and the bed sinks by `2.4 · smooth((1 − s) / 0.5)`, under a water plane at −0.45 m.

**Colour.** Each vertex is painted: two greens mixed by a large noise (meadows in patches) with a fine noise on top, paler on the heights, sand for `s` below 1.24, a deep blue-green for `s` below 0.9, seen through the water.

**Grid.** 200 × 200 cells over a 900 m square, spaced by `(0.4 u + 0.6 u³) · 450 m` for `u` in −1…1: about 1.8 m apart in the middle, 11 m at the edge. Each vertex takes its normal from its neighbours. Anything placed on the land asks the same function for its height. A plain a little below zero continues to the horizon.

# Invariants

The height is exactly zero within `flat` of the centre line: the road, drawn 2 cm above zero, is never buried. The knoll and the lake stay clear of the barriers, and the lake's shore is above the water all the way round. Unit tests check all three on the kart circuit.

# Cost

Once, when the circuit is known: about 40 000 evaluations for the grid, each a search over the centre line's 170 points, and as many again for the scenery placed on it.

# Failure modes

A grid coarser than the track corridor's width would cut corners into the flat zone; the cells are smallest where the circuit is. A circuit with no room inside its loop gets no knoll or lake; a very thin loop could put the lake's beach under a barrier, which the test that keeps sites clear of the barriers would catch.

# Alternatives

A stored height map (authoring cost, download size); terrain patches by level of detail (needed for worlds larger than one circuit).

# Oracle

The unit tests of `track.rs`, and [the render bench](/oracles/render-bench.md) screenshots: the road is visible everywhere.

# Reuse

The two masks apply to any path-shaped feature — a road, a river, a railway — in any terrain generator.

[^own]: osage design
