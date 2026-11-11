---
type: Module
title: osage-props
description: Models of the asset manifest placed many times, drawn in batches, each set seen up to its own distance.
resource: https://github.com/smichea/osage/tree/main/crates/osage-props
licence: MIT OR Apache-2.0
status: draft
tags: [content, gltf, instancing, scenery]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-11T14:30:00Z" }
---

# Purpose

Put a model of the asset manifest (a glTF file) at hundreds or thousands of places — tyres in a wall, tufts of grass, pines — at little cost, and without each game writing its own loader.

# Concepts

- **Scatter:** one model and the places it stands; how far it is seen; whether it casts shadows.
- **Use:** the whole model at each place, or one of its **variants**: libraries such as Poly Haven lay several plants or rocks side by side in one file, each a top-level node.
- **Waiting:** the number of scatters whose model is still loading.
- **Carpet:** a model laid around the camera in a grid of cells and moved with it, where the world says it grows: grass as seen up close.
- **Woods:** a model at many places always far from the camera, drawn as crossed cards showing pictures of it ([tree cards](/algorithms/tree-cards.md)); the number of trees is logged at `debug`.
- **Tint:** a plain colour instead of a model's colour texture, for tiny things whose texture is an atlas on black.
- **Lit as the ground:** grass whose normals all point up, both faces drawn and neither turned over, so that it is lit as the ground it grows from: upright blades lit by their own normals came out darker than the meadow and read as dark leaves scattered on it.
- **Opaque:** a model's blended materials drawn opaque, for leaves modelled as geometry whose texture has no transparency: blending sorts and costs, and leaves showed the sky through them.
- **Levels of detail:** for dense models seen far, two coarser meshes made from each mesh — by [thinning](/algorithms/aggregate-thinning.md) for foliage, by vertex clustering for solid things — the scatter's places drawing the whole mesh near, then the coarser ones, fading from one to the next.

# Architecture

A Bevy plugin. Each frame it looks at scatters not yet placed whose model has loaded with all its textures. It walks the model's nodes, composing each node's place with those above it, and collects its meshes; with variants, each top-level node is brought to the origin (its offset dropped, its turn kept). Each glTF material is converted to a standard material once and shared. Every place then gets children with the shared meshes and materials, so that Bevy draws each mesh of the set in one batch. A distance limit becomes a visibility range that fades the set out over its last tenth. A carpet is a fixed pool of instances, one per cell of a square window around the camera; instance `i` always stands in the one cell of the window equal to `i` modulo the window's side, so that when the camera moves on, only the instances whose cell left the window move, to the cells that entered it. Where each stands in its cell, its turn and size come from a hash of the cell: the same camera path gives the same carpet. They fade out before the window's edge. With levels of detail, each mesh is also simplified twice, once per mesh and shared. A mesh of many small separate pieces (needles, leaves) is [thinned](/algorithms/aggregate-thinning.md): 15 % and 3 % of its pieces kept, grown to cover as much; the whole mesh is drawn up to 4 % of the scatter's distance, the first coarse one to 15 %, the second beyond. Any other mesh is clustered: its box is cut into cells of 2.5 % and 6 % of its size, the vertices of a cell merge at their mean, and triangles that collapse vanish; the whole mesh is drawn up to 15 % of the distance, the first coarse one to 45 %, the second beyond. Each level fades into the next (measured at T3 on the Iris Xe, 2026-11-11: the roadside with twice the trees went from about 125 to 81 ms with clustering; thinning the foliage costs the same and keeps the crowns full). The triangles of each level are logged at `debug`.

# Interfaces

`PropsPlugin`, `Scatter { model, places, using, within, shadows, lods, tint, opaque, lit_as_ground }`, `Use::{Whole, Variants}`, `Waiting`, `poly_haven(server, id)` (the path of a Poly Haven model in the manifest's content), `lod::{simplify, thin, extent}`, `Carpet { model, using, spacing, radius, ground, tint, lit_as_ground }`, `Woods { model, using, places }`, `impostor::{VIEWS, View, axes}`.

# Invariants

- A scatter is placed only once its model and textures have loaded: nothing appears white first, and a benchmark can wait for `Waiting` to reach zero to draw the same frames at every run.
- A model that fails to load is skipped with a warning, never waited for forever.
- A malformed model (a cycle of nodes, an index out of range) places what it can and never loops.
- A carpet's window has exactly one instance per cell; moving one cell moves one row of instances; a place is the same at every call, inside its cell, and absent where the ground says nothing grows. Tested.
- A coarser level has fewer triangles, keeps the mesh's size and unit normals, and never a triangle collapsed to a line or a point; a level that would leave nothing keeps the finer one. Tested.
- Thinning keeps the share of pieces asked for and their area, leaves large pieces whole, refuses a solid mesh, and gives the same mesh at every run. Tested.

# Knowledge

- Algorithms: [aggregate thinning](/algorithms/aggregate-thinning.md), [tree cards](/algorithms/tree-cards.md).
- Used by [osage-kart-roadside](/modules/kart-roadside.md).
- Oracle: the crate's tests (node composition, variants at the origin, broken models) and [the render bench](/oracles/render-bench.md).

# Reuse

Any Bevy app: give the asset server the manifest's content as its root and spawn scatters. Permissive licence.

# Limits

Vertex clustering is coarse; thinning cannot lighten a tree whose needles are joined to its twigs. An error-driven simplification would go further; tree cards serve only trees that are always far. Skinned and animated models are drawn in their rest pose.
