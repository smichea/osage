---
type: Algorithm
title: Sculpted scenery
description: Static scenery built from simple shapes, painted through vertex colours and merged into one mesh per material and patch of land, so that thousands of objects cost a few dozen draws.
resource: https://github.com/smichea/osage/blob/main/crates/osage-procedural/src/lib.rs
licence: MIT OR Apache-2.0
status: draft
tags: [rendering, batching, procedural, performance]
sources:
  - id: wloka
    resource: https://www.nvidia.com/docs/IO/8228/BatchBatchBatch.pdf
    title: "Matthias Wloka, Batch, Batch, Batch: What Does It Really Mean?, GDC 2003"
  - id: own
    resource: "osage's own design and measurements, 2026-11-10"
    title: osage design
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-11T11:30:00Z" }
---

# Problem

A cheerful circuit needs many small things: trees, bushes, flowers, rocks, tyre stacks, signs, spectators. Drawn as one object each, a tree of four parts a thousand times is four thousand draws, each also tested for visibility, and drawn again in every shadow cascade. On an integrated GPU the cost is in the number of draws more than in the triangles[^wloka].

# Idea

Whatever does not move is merged, at load time, into as few meshes as there are materials, cut by square patches of land so that what is out of view is still skipped[^own]. Colour moves from the material into the vertices, so that a red tyre and a white one share a material.

# Method

1. **Shapes** are triangle lists in their own space, with normals: an engine primitive, a solid of revolution from a profile of (radius, height) pairs, a slice of one (alternate slices in two colours make striped tents and balloons), a round tube swept along a path, or flat convex polygons.
2. **Paint** is two colours, one at the shape's lowest point and one at its highest, blended by height. A darker foot is the cheapest shading there is: it reads as contact shadow and as depth inside a crown of leaves.
3. **A batch** belongs to one material. Adding a shape at a placement transforms its positions by the placement and its normals by the inverse transpose of the placement's linear part, writes the paint into the vertex colours and texture coordinates in metres projected from the side each face faces most (x–z from above, z–y or x–y from the sides), and appends the triangles to the patch of land where the object's anchor stands (90 m squares). A placement that mirrors (negative determinant) turns every triangle over, to keep the front faces outward.
4. **Objects** of several parts give all their parts the same anchor, so that an object is never split between patches.
5. At the end, each patch of each batch becomes one mesh. Three batches are enough here: rough things, smooth things, and small bright things that cast no shadow (flowers).
6. **What moves** (the sails of a mill, a big wheel, a balloon, a boat) is sculpted the same way, merged into one mesh of its own, and moved as one object.

# Invariants

- Placement is arithmetic only, from a seeded generator: every machine builds the same meshes.
- Every triangle faces outward after any placement, mirrors included (checked by tests on each kind of shape).
- A patch's mesh holds only what is anchored in it.

# Cost

Building the kart circuit's scenery (about 760 trees, bushes, flower drifts, rocks, the crowd and the landmarks) takes a fraction of a second, once. Drawing it: on the Intel Iris Xe, T2 at 1632×918, the scene with merged scenery and three times the content drew as fast on average as the previous scene of separate objects (18.5 to 19.4 ms against 19.0 to 20.1 ms, under the same background load), with the slowest frames much shorter (99th percentile 30 to 31 ms against 44 to 51 ms).

# Failure modes

- A patch too large is drawn whole when a corner of it is in view; too small, and the draws come back. 90 m suits a circuit seen from a kart.
- Merged meshes cannot be culled object by object, nor given levels of detail one by one.
- Vertex colours cannot carry a texture of their own; a batch's material can lay one grain over everything in it (the kart circuit gives its concrete a batch of its own, grained at the same size on every building, tier and post; steel and paint stay smooth), and projection from the box's sides streaks it where a face is turned 45°. A surface that needs its own texture keeps its own material.

# Alternatives

GPU instancing of one mesh many times (still one draw per kind, and each instance's colour needs its own data); engine-side automatic batching (not available for meshes with different shapes); hand-modelled merged meshes (authoring cost, nothing procedural).

# Oracle

The unit tests of [osage-procedural](/modules/osage-procedural.md) (orientation of every kind of shape, patches, paint by height), and [the render bench](/oracles/render-bench.md) before and after.

# Reuse

Any engine with static props placed by a program: sculpt, paint by height, merge by material and patch. The two-colour paint alone gives low-poly scenery most of its charm for no cost.

[^wloka]: Batch, Batch, Batch
[^own]: osage design
