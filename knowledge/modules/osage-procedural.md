---
type: Module
title: osage-procedural
description: Procedural geometry — shapes sculpted, painted and merged per material and patch of land — and a repeatable generator, so that what is generated is the same on every machine.
resource: https://github.com/smichea/osage/tree/main/crates/osage-procedural
licence: MIT OR Apache-2.0
status: draft
tags: [procedural, geometry, batching]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-11T11:00:00Z" }
---

# Purpose

Build static scenery from code at little cost, and scatter things the same way at every run. Split from the kart client (D-035) so that any world can use it.

# Concepts

- **Shape:** a mesh in its own space: a Bevy primitive, a solid of revolution, a tube along a path, flat faces, a block, a prism extruded from a convex outline, crisp (sheet metal) or with rounded corners shaded smooth (moulded plastic).
- **Paint:** vertex colours, plain or from foot to top.
- **Batch:** placed, painted shapes gathered into one mesh per patch of land, with texture coordinates in metres projected from the side each face faces most (`box_uv`), so that a grain laid on the batch keeps its size on every object.
- **Dice:** a small seeded generator: the same numbers on every machine.
- **Sculpture:** round cones, ellipsoids and boxes blended into one smooth surface, folded like cloth where asked, painted by groups: a figure without seams ([blended solids](/algorithms/blended-solids.md)).

# Architecture

The algorithm is [sculpted scenery](/algorithms/sculpted-scenery.md).

# Interfaces

`Shape::{new, lathe, gore, sweep, faces, block, extrude, extrude_rounded}`, `Paint::{plain, blend, shaded}`, `Batch::{add, add_within, append, merged, meshes}`, `box_uv`, `Dice::{roll, between, pick}`, `Sculpture::{new, cone, ellipsoid, cuboid, rounded, paint_cone, paint_box, folded, bone, distance, group_at, weights, shapes, skinned}`.

# Invariants

Every kind of shape faces outwards, mirrored placements and outlines turning either way included; a batch splits by patch of land; paint by height runs from foot to top; texture coordinates are metres on the side a face faces; a sculpture's mesh lies on its surface, faces out, and is painted by its pieces. Tested.

# Knowledge

- Algorithms: [sculpted scenery](/algorithms/sculpted-scenery.md), [blended solids](/algorithms/blended-solids.md).
- Used by [osage-kart-native](/modules/kart-native.md) and [osage-kart-roadside](/modules/kart-roadside.md).

# Reuse

Any Bevy app. Permissive licence.

# Limits

No levels of detail; vertex colours only, no textures of its own.
