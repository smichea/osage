---
type: Algorithm
title: Building acoustic triangle indexes on first use
description: Indexes resident acoustic placements by their world bounds and retains the triangle tree of each placement first reached by a sound path.
resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/acoustic.rs
licence: MIT OR Apache-2.0
status: draft
tags: [audio, geometry, streaming, paris]
sources:
  - id: traversal
    title: World bounds and lazy triangle traversal
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/acoustic.rs
  - id: neighbours
    title: Boundary and seam selection for open acoustic surfaces
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/acoustic_edges.rs
  - id: paging
    title: Reusing immutable acoustic placements across sound paths
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/channels/resident.rs
  - id: replay
    title: Complete stereo recording with repeated acoustic screens
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-acoustic-surface-index.py
  - id: direct
    title: City transmission through object bindings and bridge faces
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/impact/transmission.rs
generated: { by: "OpenAI/Codex", at: "2026-11-24T07:09:23Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A single footfall can be heard through one wall in a district containing thousands of resident surfaces. Building a search tree over all their triangles before tracing the first path makes the recording pay for distant geometry.

# Idea

Index each placed acoustic binding by its world-space bounds. When a direct segment or an edge search first intersects a binding, build a smaller search tree from its original triangles. Keep that tree with the placement for later sounds.[^traversal]

# Method

Use all the placed mesh's vertices to form a conservative world-space axis-aligned box. Pad its extrema by the same coordinate-dependent margin used for individual triangle bounds. Bindings with no triangles are excluded. Use the same two-level traversal when tracing a direct segment and when finding exposed parts of a sound-blocking edge. The latter expands the boxes by the edge's own tolerance. The narrow phase retains the existing positioned vertices, physical proxy, triangle order, material and owner path.[^traversal][^neighbours]

When a paged impact path borrows a verified binding, the original binding and its path-specific clones share the bound and triangle tree through a thread-safe, single-initialization cell. Each path can still build its own outer tree for a different subset of bindings. The last owner releases the cached placement. Separate placements of one source mesh keep distinct world coordinates and separate lazy trees.[^paging]

# Invariants

A broad-phase candidate can be a false positive. An actual triangle intersection or overlapping seam must survive both stages. The cached geometry is immutable for the lifetime of the binding. A replacement placement receives a new cache. Two materials can still contribute independently at the same distance. The source and receiver position, transmission filters, diffraction and mixing use the existing definitions.[^traversal][^neighbours]

# Cost

The first lookup of the bounds scans the resident vertices of each placement. The outer tree stores one leaf per nonempty surface. Triangle trees are allocated and built as their surfaces are first reached. The first event on a fresh surface therefore pays its local construction cost. A segment crossing the entire scene can eventually instantiate every tree. The WAV receipt gives the number of prepared surfaces and triangles and reports the heap bytes of the tree arrays separately from the binding reference array. These counts exclude world-geometry buffers, topology for diffraction, the mixer and process memory.[^traversal][^replay]

# Failure modes

Conservative outer bounds may admit a placement whose triangles all miss. A large surface spanning much of the district can be expensive even when only one corner is audible. A diffracted path can additionally prepare the exact topology of neighbouring blocking surfaces after an actual triangle crossing. [Closed acoustic regions](acoustic-solid-regions.md).[^traversal]

# Alternatives

The earlier resident reader built one tree over every placed acoustic triangle at startup. The district file reader performs a separate test against the directory before deciding which mesh files to load. The two mechanisms act at different points in the life of a surface.[^traversal][^paging]

# Oracle

The CLI replay uses one tessellated screen on the path of an impact and instances of the same screen beyond the source and receiver. It alternates fresh processes for the earlier and current binaries, records their CPU and peak resident memory, and compares the entire stereo WAV and contact report in direct and diffracted listening. A separate unit check traces a miss, then traces a shared edge through two different paged subsets and verifies that the same triangle tree is reused.[^replay][^neighbours]

# Reuse

Render an object with `--impact-wav` to inspect `acoustic_index` in its JSON receipt. Use `--direct-only` for a direct recording and omit it to include diffraction. The same traversal serves live and offline impact propagation.[^traversal][^replay]

The bundled Bir-Hakeim fallback reads the placed quadrilaterals of `paris_soundscape::bridge()` and calls `osage_sound::ways::Face::crossed` on the occluding faces. When an object supplies its own acoustic bindings, Ville-Paris queries this triangle index. A binding can supply a dedicated mesh or refer to its physical proxy. The resulting crossing carries the owner path and available transmission coefficients in both representations so the city can accumulate transmission along the leg. [Osage sound](/modules/osage-sound.md) · [Sound ways](/algorithms/sound-ways.md).[^traversal][^direct]

[^traversal]: [Acoustic traversal](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/acoustic.rs).
[^neighbours]: [Neighbour and cache checks](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/acoustic_edges.rs).
[^paging]: [Paged placement lifetime](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/channels/resident.rs).
[^replay]: [Full-recording comparison](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-acoustic-surface-index.py).
[^direct]: [City transmission through object bindings and bridge faces](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/impact/transmission.rs).
