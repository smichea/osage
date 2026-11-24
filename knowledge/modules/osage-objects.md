---
type: Module
title: Osage objects
description: A glTF object tree binds visual detail levels, physical surfaces and acoustic meshes to the same local coordinates.
resource: https://github.com/smichea/osage/tree/main/crates/osage-objects
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [objects, gltf, geometry, physics, sound]
generated: { by: "OpenAI/Codex", at: "2026-11-23T12:12:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Purpose

An object can draw one mesh, use a second for collisions and a third for sound. Its glTF scene tree locates all three. The object profile gives a client the owning node and material for each surface.

# Concepts

Metres and right-handed glTF coordinates are common to all channels. The primary visual mesh belongs to a scene node. Alternate visible levels and the other channels refer to mesh indices in the same asset. Acoustic inputs carry three energy bands or an explicitly unknown response.

# Architecture

The core type defines a physical material. Enabling the `profile` feature adds the scene reader, articulations and projected visual detail. The reader composes parent-relative transforms, validates references and returns geometry bindings. The caller supplies mesh buffers. [Profile reader](https://github.com/smichea/osage/blob/main/crates/osage-objects/src/profile.rs)

An articulation describes bodies, shapes, masses, principal moments, bind poses, angular stops and parent joints. The reader can resolve bone-local body and joint frames against the same object tree. [Articulations](https://github.com/smichea/osage/blob/main/crates/osage-objects/src/articulation.rs)

# Interfaces

`ObjectProfile::read` distinguishes an ordinary glTF from a versioned object. `ObjectNode::visual_at` selects a visual level. `first_hit` returns the nearest two-sided intersection in metres along a finite segment. Physical materials can be passed directly to [osage-dynamics](/modules/osage-dynamics.md).

# Invariants

Each reachable node has at most one parent. Each referenced mesh and material must exist. An acoustic proxy names a physical surface on its own node. In each defined band, the transmitted and absorbed energy fractions sum to at most one.

# Knowledge

The profile tests construct a glTF tree with independently assigned channels. Separate controls check nearest hits on a shared diagonal, acoustic energy bounds, joint endpoints and the effect of shear on projected geometry error. [Profile tests](https://github.com/smichea/osage/blob/main/crates/osage-objects/src/profile.rs) · [Projection tests](https://github.com/smichea/osage/blob/main/crates/osage-objects/src/visual_detail.rs)

# Reuse

Use the default features for the shared collision material. Enable `profile` to inspect a glTF scene. Run `cargo test -p osage-objects --all-features` for the reader and articulation controls.

# Limits

The profile describes surfaces and resolves their owning nodes. The caller loads vertex buffers, streams geometry and supplies the renderer, physics solver and audio engine. Unknown acoustic coefficients remain unknown until the caller chooses a material response.
