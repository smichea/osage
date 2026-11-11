---
type: Algorithm
title: Blended solids
description: Organic shapes — a figure, cloth over limbs — made of round cones, ellipsoids and boxes blended by a signed distance field and meshed by surface nets, painted by the piece nearest each point or by paint volumes.
resource: https://github.com/smichea/osage/blob/main/crates/osage-procedural/src/sculpt.rs
licence: MIT OR Apache-2.0
status: draft
tags: [procedural, geometry, signed-distance-field, characters]
sources:
  - id: quilez-sdf
    resource: https://iquilezles.org/articles/distfunctions/
    title: "Inigo Quilez, Distance functions"
  - id: quilez-smin
    resource: https://iquilezles.org/articles/smin/
    title: "Inigo Quilez, Smooth minimum"
  - id: gibson
    resource: https://www.merl.com/publications/docs/TR99-24.pdf
    title: "Sarah F. F. Gibson, Constrained Elastic Surface Nets, MERL TR99-24, 1998"
  - id: own
    resource: "osage's own design and measurements, 2026-11-11"
    title: osage design
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-11T20:00:00Z" }
---

# Problem

A figure assembled from capsules and spheres shows a seam and a crease at every joint: the shoulder is a ball stuck on a tube, and the whole reads as a mannequin. Modelling a figure by hand needs an artist and a licence; a world wants many, in many liveries, generated.

# Idea

Describe the figure as a signed distance field — negative inside — made of simple pieces, and join the pieces with a smooth minimum instead of a plain one, so that where two pieces meet the surface fills the corner as flesh under cloth does[^quilez-smin]. Then mesh the field's zero level once[^gibson].

# Method

1. **Pieces**[^quilez-sdf]: a round cone from `a` (radius `ra`) to `b` (radius `rb`) for limbs and the neck; an ellipsoid, turned, for the pelvis, belly, chest and hands; a turned box, its edges rounded by `r` (the distance to the box shrunk by `r`, minus `r`): moulded plastic. Each piece belongs to a group (what the surface is there: suit, glove, boot).
2. **Blend**: the field is the pieces' distances folded with `smin(a, b, k) = min(a, b) − h²k/4`, `h = max(k − |a − b|, 0)/k`; `k` is how far pieces blend (4.5 cm for a driver). Far from a joint it changes nothing.
3. **Folds**: a piece may ripple: its distance minus `depth · sin(105·s + 1.7·sin(2θ + 31·s))`, `s` along its axis and `θ` around it — ridges about 6 cm apart, a few millimetres deep, wavering so that no two are alike, as a sleeve gathers at the elbow.
4. **Mesh by surface nets**: sample the field on a grid of cells (1.2 cm for a driver) over the pieces' boxes and a margin. In every cell whose corners differ in sign, place one vertex at the mean of the points where the field crosses the cell's twelve edges (linear interpolation). For every grid edge the surface crosses, join the vertices of the four cells around it in a quad, ordered counter-clockwise seen from the outside (the end of the edge that is inside tells which way that is); split each quad along its shorter diagonal.
5. **Normals** from the field's gradient by central differences at each vertex: smooth, and they carry the folds even where the grid is too coarse to.
6. **Skin** (for a figure that moves): each piece names a bone; a vertex takes the bone of the nearest solid piece and, within the blend of it, the next bone's too, weighted `0.5 − (d₂ − d₁)/2k` for the second (`d` the distances to the bones' nearest pieces): halfway at a joint, one bone a blend away from it. The meshes carry joint indices and weights for a renderer's skinning.
7. **Paint**: a triangle takes the group of a paint volume (a cone or box that adds no matter, as a panel is sewn on a suit) that contains its centre — the last listed wins — otherwise the group of the solid piece nearest its centre. Each group becomes its own mesh, so that each can have its material and colour.

In osage, a kart driver is 21 pieces and 10 paint volumes; sculpted once (the same body for every driver), each livery only paints its groups[^own].

# Invariants

Vertices lie on the surface within a cell; normals point outwards; at a joint two bones share a vertex half and half, away from it one bone has it whole; front faces face out; blending fills a corner and leaves far parts alone; paint adds no matter; folds are never deeper than asked. Tested.

# Cost

The grid's points times the pieces, once: a driver at 1.2 cm (about 240 000 points, 21 pieces) takes a fraction of a second in a release build, a few seconds in a debug one. About 20 000 triangles. Measured at T3 on the Iris Xe, 2026-11-11, the race view: 127 ms per frame with the capsule drivers, 130 ms with the sculpted ones ([the render bench](/oracles/render-bench.md)).

# Failure modes

- Pieces closer than the blend distance merge where they should not: fingers, legs held together. Keep separate parts more than `k` apart, or blend with a smaller `k`.
- A cell larger than a piece loses it; paint boundaries are as jagged as the cells.
- The ellipsoid's distance is approximate away from its surface: blending with it is slightly uneven.

# Alternatives

Marching cubes (more triangles, sliver triangles, the same surface); dual contouring (keeps sharp edges, needs gradients); a sculpted or scanned mesh (best, when there is an artist and a licence); skinned capsules (cheap, but the creases remain).

# Oracle

The tests of `sculpt.rs`, and [the render bench](/oracles/render-bench.md) screenshots of the drivers.

# Reuse

Engine-independent: any figure, animal, rock or piece of cloth built from simple parts. The method is the classic one of signed distance fields; only the pieces and the paint rules are osage's.

[^quilez-sdf]: Quilez, distance functions
[^quilez-smin]: Quilez, smooth minimum
[^gibson]: Gibson 1998
[^own]: osage design
