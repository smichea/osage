---
type: Module
title: osage-props
description: Models of the asset manifest placed many times, drawn in batches, each set seen up to its own distance.
resource: https://github.com/smichea/osage/tree/main/crates/osage-props
licence: MIT OR Apache-2.0
status: draft
tags: [content, gltf, instancing, scenery]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-11T02:00:00Z" }
---

# Purpose

Put a model of the asset manifest (a glTF file) at hundreds or thousands of places — tyres in a wall, tufts of grass, pines — at little cost, and without each game writing its own loader.

# Concepts

- **Scatter:** one model and the places it stands; how far it is seen; whether it casts shadows.
- **Use:** the whole model at each place, or one of its **variants**: libraries such as Poly Haven lay several plants or rocks side by side in one file, each a top-level node.
- **Waiting:** the number of scatters whose model is still loading.

# Architecture

A Bevy plugin. Each frame it looks at scatters not yet placed whose model has loaded with all its textures. It walks the model's nodes, composing each node's place with those above it, and collects its meshes; with variants, each top-level node is brought to the origin (its offset dropped, its turn kept). Each glTF material is converted to a standard material once and shared. Every place then gets children with the shared meshes and materials, so that Bevy draws each mesh of the set in one batch. A distance limit becomes a visibility range that fades the set out over its last tenth.

# Interfaces

`PropsPlugin`, `Scatter { model, places, using, within, shadows }`, `Use::{Whole, Variants}`, `Waiting`, `poly_haven(server, id)` (the path of a Poly Haven model in the manifest's content).

# Invariants

- A scatter is placed only once its model and textures have loaded: nothing appears white first, and a benchmark can wait for `Waiting` to reach zero to draw the same frames at every run.
- A model that fails to load is skipped with a warning, never waited for forever.
- A malformed model (a cycle of nodes, an index out of range) places what it can and never loops.

# Knowledge

- Used by [osage-kart-roadside](/modules/kart-roadside.md).
- Oracle: the crate's tests (node composition, variants at the origin, broken models) and [the render bench](/oracles/render-bench.md).

# Reuse

Any Bevy app: give the asset server the manifest's content as its root and spawn scatters. Permissive licence.

# Limits

No levels of detail yet: a model is drawn whole up to its distance, then fades out. Poly Haven's plants are made to be seen close and carry tens of thousands of triangles; a level-of-detail chain (mesh simplification) is the next step. Skinned and animated models are drawn in their rest pose.
