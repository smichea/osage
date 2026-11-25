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

A stable object can have a visible panel, a physical surface at a different height and a sound barrier with different extents. An independently authored pair of input files and a third optional sound surface test all three roles in a single loadable object. [File format](https://github.com/smichea/osage/blob/main/design/08-object-format.md)

# Concepts

Metres and right-handed glTF coordinates are common to all channels. The primary visual mesh belongs to a scene node. Alternate visible levels and the other channels refer to mesh indices in the same GLB. A surface has its own ID and material. Acoustic inputs contain three energy bands or an explicitly unknown response.

# Architecture

The core type defines a physical material without a dependency on a renderer or solver. The optional profile feature reads the object header and the default scene tree, composes node transforms, validates references and returns the geometry bindings. The world supplies buffers and draws its selected visible meshes. It passes the physical triangles to a rigid-body solver. Acoustic geometry can reuse a physical mesh when its binding declares that origin. [Reader](https://github.com/smichea/osage/blob/main/crates/osage-objects/src/profile.rs) · [Native world](/modules/ville-paris.md)

An optional articulation carries named bodies, mass and principal moments, shapes, parent links, bind poses and angular stops. The city reads both the new rig and its part materials through the profile. [Format](https://github.com/smichea/osage/blob/main/design/08-object-format.md) · [Impact check](/oracles/pedestrian-impact.md)

The public profile keeps the roots, ordered children and parent-relative transforms. `node_path` reconstructs a sequence of component IDs. The native viewer attaches drawn meshes to the corresponding entities. Each collider or acoustic binding retains its owner. A group with an authored substitute can replace its descendants independently for vision, contact or sound at a selected level. Articulations can resolve local bone and joint frames against the same tree and reject inconsistent joint endpoints.

# Interfaces

The fragment converter can recover the same joint endpoints from Z-twist body-to-joint matrices. The principal swing amplitudes follow their corresponding axes of rotation. [Joint frame conversion](/algorithms/articulated-joint-frames.md)

An optional joint field selects an [angular swing envelope](/algorithms/articulated-swing-envelope.md) with positive symmetric tilt amplitudes.

On articulated fragment import, the converter can read each sphere's physical radius from the declared bound or from its contact margin. Explicit selection records both values against the source bone. A positive radius is required to instantiate a physical sphere. The source visual mesh remains attached to the same node.

The optional `read_with_source_indices` reader also returns the original glTF node index at each parent-first profile slot. The actor query uses this mapping to link a posed body back to the identified asset even when file order differs from tree traversal order. [Actor and terrain queries](/algorithms/scene-ray-coverage.md)

A visual alternate may declare its maximum local surface deviation in metres. A perspective view can then select a mesh for each eligible node. [Projected mesh error](/algorithms/projected-mesh-error.md)

`ObjectProfile::read` distinguishes an ordinary glTF from a versioned object. `ObjectNode::visual_at` chooses the requested level or its primary mesh. `first_hit` returns the nearest two-sided intersection in metres along a finite segment. The offline converter reads static XML or loose binary resources using an optional local resource tool and emits the same profile. [Converter interface](https://github.com/smichea/osage/blob/main/tools/object-convert/README.md)

An optional `asset.extras.osageFootRig` resolves persistent armature and contact IDs into parent-first slots. Each contact gives one upper joint, lower joint, ankle and contact path for the [foot planting system](/algorithms/forefoot-plant.md). The reader checks ancestry and rejects intersecting or nested chains (D-225). Optional `solePoints` resolves finite ankle-local samples against declared contact IDs (D-226).

# Invariants

Each reachable node has one parent at most. IDs are unique per channel. Referenced meshes and materials must exist. A proxy references a physical mesh on the same node. A change of visible detail keeps the other two geometries fixed. Fractions of transmitted and absorbed energy sum to at most one in each defined frequency band.

# Knowledge

The original slab puts the visible object at 1.25 m, the physical surface at 1 m and the independent acoustic surface at 0.75 m. A downward segment checks these heights separately. A second run reuses the collision as sound geometry. The rigid-body test drops a capsule on the physical plane and checks the normal, final resting height and identical replay for each solver. [Cross-channel check](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-object.py)

# Reuse

A native client can inspect one GLB without a replay using `ville-paris --scene file.glb --inspect-object --ray 0,3,0 0,0,0`. Other readers can use standard glTF accessors and the versioned object metadata. [Portable mesh specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html)

# Limits

The object reader loads the complete file. The static importer supports a defined subset of source geometry and records material assumptions. Sound coefficients in the examples are authored test values. The ray checks geometry and material selection. Audible synthesis, diffraction, skin animation and district page streaming need their own runtime paths.
