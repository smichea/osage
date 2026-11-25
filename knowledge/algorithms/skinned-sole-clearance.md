---
type: Algorithm
title: Ground clearance of a skinned sole
description: Evaluates weighted foot vertices and their triangles against traversable surfaces before accepting a leg correction.
resource: https://github.com/smichea/osage/blob/main/crates/osage-humans/src/stance/skin.rs
licence: MIT OR Apache-2.0
status: draft
tags: [animation, characters, geometry, locomotion]
sources:
  - id: fox
    title: Authored fox walk and run replay
    resource: https://github.com/smichea/osage/blob/main/tools/animation/fox-stride.mjs
generated: { by: "OpenAI/Codex", at: "2026-11-24T23:18:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

The back of a paw can move with the lower leg as the ankle rotates. A curb can also pass through a triangle whose vertices all clear their respective ground heights.

# Idea

Use the same weights and inverse bind matrices as the drawn foot. Measure vertical clearance on a candidate joint pose and request more lift if skin still intersects the ground.

# Method

Bind each selected vertex once in each influencing joint's frame. Store its homogeneous coordinates multiplied by its normalized weight. Merge equal bindings and preserve the indices of the selected faces. Skin a sample by summing the weighted points transformed by the four limb nodes. The ankle's component can be built from loaded meshes under an actor or from explicitly supplied bindings.

The point query runs at each deformed sample. The world may also supply a signed lift across each face over the allowed range of ground heights. On triangular terrain, clip the horizontal projection of the sole face against the three terrain edges. Interpolate the height difference at the resulting vertices. This difference is affine over the intersection, so its maximum gives the required lift. A terrain corner can lie entirely inside the sole face. Both terrain windings are supported. [Triangle overlap](https://github.com/smichea/osage/blob/main/crates/osage-humans/src/stance/surface.rs)

The initial target also includes the horizontal correction from [foot planting](/algorithms/forefoot-plant.md). Evaluate the skin again after solving the leg, with the applied visual armature drop. Add the remaining penetration to the vertical target for up to eight further solves. Retain the pose with the smallest measured penetration and stop on loss of improvement or at the scaled tolerance of ten micrometres.

# Invariants

The controller keeps the authored ankle orientation, segment lengths and actor trajectory. A selected vertex must have finite, nonnegative weights summing to one within import tolerance. The mesh reader requires all its influences to lie within the bound limb. Ground queries return a signed height difference in world metres.

# Cost

Each evaluation skins the unique samples once and queries each selected face when the world supplies surface queries. Pose refinement is bounded. The face path reuses a per-foot buffer for world positions.

# Failure modes

The mesh reader selects rendered vertices with majority ankle and contact weight and the triangle-list faces wholly within that set. Morph targets and other mesh regions require separate evaluation. Point-only terrain queries can miss an intervening edge. A leg can run out of reach. Vertical clearance at sampled frames does not measure balance, sliding or continuous collision between frames.

# Alternatives

A rigid model can use ankle-local samples. A distant actor can use the central marker alone.

# Oracle

The authored fox traverses an eight-centimetre strip in Walk and Run. Four quarter-cycle starting phases check for regressions. Four additional starts halfway between them test the same settings.[^fox] The independent measurement intersects posed triangle edges with both strip boundaries. Across the eight starts, the deepest penetration of the selected faces was 0.0071 mm or less in Walk and 0.0089 mm or less in Run. The replay also records horizontal slip and the gap under a low paw. The human replay can enable weighted support with `pedestrian_stride --curb --mesh-support`.

# Reuse

Bind `SoleSkin` to a configured ankle. A world can supply `FootGround::with_surface_lift` and use `triangle_support_lift` to evaluate candidate terrain faces. The authored skin and triangle topology already live in the object tree.

[^fox]: [Fox replay](https://github.com/smichea/osage/blob/main/tools/animation/fox-stride.mjs).
