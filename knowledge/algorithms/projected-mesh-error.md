---
type: Algorithm
title: Visible mesh selection by projected error
description: Selects a mesh for a model node using a surface deviation in metres, a perspective view and the cost in triangles.
resource: https://github.com/smichea/osage/blob/main/crates/osage-objects/src/visual_detail.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [rendering, geometry, lod]
sources:
  - id: object-design
    title: Object geometry channels and levels
    resource: https://github.com/smichea/osage/blob/main/design/08-object-format.md
generated: { by: "OpenAI/Codex", at: "2026-11-23T13:56:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A fixed switch distance can leave a visible gap on a large screen or keep many unnecessary triangles on a small one. Each visible part of a model needs a choice that also accounts for scale, field of view and its position in the image.[^object-design]

# Idea

Give each alternate mesh a bound on its bidirectional surface error from the primary mesh, in local metres. Convert that bound into pixels for the current view.

# Method

Bound world-space stretch by the square root of the maximum absolute row sum of the transform’s Gram matrix. This works for rotation, scale, reflection and shear. Enclose the primary surface and all candidates in one world-axis-aligned box. Its nearest depth bounds the perspective denominator. Its farthest transverse offset bounds the extra motion caused by depth differences near the frame edges.

For focal length `f` in pixels, nearest depth `z` and transverse bound `r`, the factor is `(f/z) × sqrt(1 + (r/z)²)` pixels per metre. Multiply by the world-space error bound. Where the box crosses the near plane, retain the primary surface or an exact equivalent. A box entirely behind the near plane has no visible surface.

Choose a cheaper candidate below one pixel. Retain the current candidate until its bound exceeds two pixels. On refinement, choose the cheapest candidate satisfying the two-pixel limit.

# Invariants

The active level of each eligible visual node has its own mesh group. Each drawable stays below its canonical owner. Physical and acoustic levels are independent. A fixed visual override selects the requested level for inspection.

# Cost

The viewer shares geometry between instances that need the same mesh and local frame. The selection tests the available levels per eligible node per frame. All eligible meshes reside in memory for this viewer. A switch changes group visibility.

# Failure modes

An underestimated input deviation defeats the pixel bound. The bound covers surface geometry for a static perspective camera pose. Materials, alpha cutouts, shading, normal changes and animated vertices need other measurements. A conservative box or thin shape at a grazing angle can keep the primary mesh longer.

# Alternatives

Authored distances are inexpensive and available on older assets. A fixed-level override is useful for paired images and for checking the appearance of each level.

# Oracle

Two bridge spans under a rotated parent follow one camera toward and away from the model. The test changes the viewport height in place, checks the triangle counts, and confirms the original physical and acoustic owners. Without a bound both spans retain their primary mesh. Set `OSAGE_CITY_BIN` to the native viewer executable and `OSAGE_TEST_GPU=1` to include the offscreen oracle. It runs six captures at two camera distances, counts the actually visible mesh groups and compares decoded RGB against forced high and low levels. Separate unit tests check projection under shear and behavior at the near plane. [Native bridge test](https://github.com/smichea/osage/blob/main/tools/object-convert/test_hierarchy.py) · [Projection tests](https://github.com/smichea/osage/blob/main/crates/osage-objects/src/visual_detail.rs)

# Reuse

Use the panel or drawable node's local mesh bound, its actual world transform and the physical height of the target being rendered. Maintain the last chosen level separately for each placement and view.

[^object-design]: [Object format](https://github.com/smichea/osage/blob/main/design/08-object-format.md).


The Paris client also projects a standing walker's box through `DetailView::pixels_per_metre`. Its 0.20 m foot displacement allowance comes from two installed bodies on a curb. This is an empirical budget for that locomotion controller. Its error varies with the gait and terrain. The [crowd replay](https://github.com/smichea/osage/blob/main/worlds/paris/native/examples/pedestrian_crowd.rs) compares the resulting foot positions with full correction on every actor. [Paris native client](/modules/paris-native.md)
