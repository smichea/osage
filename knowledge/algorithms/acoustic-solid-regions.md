---
type: Algorithm
title: Preparing closed acoustic solids near a sound path
description: Selects a connected set of blocking bindings when a sound actually crosses a surface and certifies the closed shells in that set.
resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/acoustic_regions.rs
licence: MIT OR Apache-2.0
status: draft
tags: [audio, geometry, streaming, paris]
sources:
  - id: traversal
    title: World-space acoustic segment traversal
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/acoustic.rs
  - id: shells
    title: Welding, shell certification and selection of exterior wedges
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/acoustic_solids.rs
  - id: tests
    title: Simultaneous room queries and a transparent neighbour
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/acoustic_edges.rs
  - id: recording
    title: Differential silent, direct and diffracted city recordings
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-acoustic-solid-regions.py
  - id: candidate
    title: Using shell normals and both owners in city diffraction
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/impact/diffraction.rs
generated: { by: "OpenAI/Codex", at: "2026-11-24T07:09:23Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

An exterior corner can belong to two walls on different model nodes. Preparing every closed shell of a district before playing the first sound consumes CPU and memory even for a silent route.

# Idea

Use a real acoustic intersection to select the first surface. Find all blocking placements whose world-space boxes touch that surface, directly or through other blocking placements. Prepare the topology for this group together. An exact shared geometric edge implies an overlap of both closed boxes.[^traversal][^shells]

# Method

For a diffracted recording, allocate empty per-binding slots in the sound geometry. The direct segment tests original triangles. On the first interior crossing of a blocking binding, search the outer acoustic hierarchy for neighbours and traverse their transitive closure. Ignore transparent or unknown transmission when finding neighbours. Order the selected bindings by their position in the original model. Weld exactly coincident positions, find the edge-connected components and certify each candidate convex shell. Its first original face identifies the shell across independent groups.[^shells]

All bindings in a prepared group refer to the same immutable result. A lock serializes cold discovery in one acoustic geometry. Once a slot is published, reads use the initialized slot directly. A component made entirely of open sheets can retain its negative result and release its temporary welded vertices. The same geometry can serve direct transits, bent legs and later impacts. Each instantiated district subset owns its own group topology.[^traversal][^tests]

# Invariants

A room can be split across several owners. A wall shared through a chain of touching bounds must reach the exact topology test even when the original ray only crossed the first wall. Intersections at unrelated buildings use distinct shell identifiers. Changes to discovery order preserve source-face order and the candidate edge sequence across shells. A clear segment or a silent recording prepares no shell topology. The direct path, transmission coefficients, geometric tolerances, path optimizer and contact clock retain their definitions.[^shells][^recording]

# Cost

Cold work includes bounding-box queries for the members reached, the triangles of these bindings and the existing exact shell test. Later intersections of a known binding reuse the answer. Dense overlapping boxes can make discovery quadratic in the number of bindings. A single mesh binding can contain several disconnected closed components and is gathered as a whole. Resident world vertices and the per-surface triangle tree have their own memory cost. The recording receipt reports prepared regions, surfaces, tested topology triangles, certified shells and retained welded vertices.[^traversal][^shells]

# Failure modes

An unnecessarily large placement bound can connect distant parts of the district. Such a group can pay more setup time while the exact weld still keeps unrelated solids separate. A hole, a nonmanifold edge or a nonconvex component prevents certification under the existing solid model. Transparent neighbours cannot close a blocking room. A streamed subset can only classify its available geometry.[^shells][^tests]

# Alternatives

A whole-district pass welds all blocking triangles at load time. Selecting only the directly struck wall would omit the remaining walls of a closed volume. The independent sheet edge query continues to handle open boundaries.[^shells][^traversal]

# Oracle

Compare a one-tick silent replay and complete audible impacts with the earlier whole-district executable. The controls include 1,024 identical tessellated screens, 256 rooms with six separate walls, moved rooms that leave the sound path clear and two crossed boxes with interleaved source nodes. Compare every PCM sample and the complete contact and diffracted-path report. A concurrent unit test approaches opposite walls of one room and verifies that a transparent neighbour does not connect it to another room.[^recording][^tests]

# Reuse

Render through the normal city command with `--impact-wav`. Read `acoustic_index.solid_topology` for the completed recording. `--direct-only` bypasses topology preparation for a fresh recording.[^traversal][^recording]

In `osage-sound`, an authored quadrilateral already supplies its four rim edges for diffraction. A district wall can consist of many triangles on several model nodes. Counting two incident triangles per seam establishes closure. Testing every vertex against the outward face half-spaces establishes convexity. Coplanar pairs can then be discarded, so an internal triangulation diagonal cannot act as a corner. The city diffraction search uses both incident normals to test a path at the remaining wedges and retains both owners in its receipt. On the bundled bridge the same search can take the four edges directly from a placed sound-scene quadrilateral. [Sound ways](/algorithms/sound-ways.md).[^shells][^candidate]

[^traversal]: [Resident sound queries](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/acoustic.rs).
[^shells]: [Exterior shell geometry](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/acoustic_solids.rs).
[^tests]: [Split-room and concurrency checks](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/acoustic_edges.rs).
[^recording]: [Full stereo replay](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-acoustic-solid-regions.py).
[^candidate]: [Using shell normals and both owners in city diffraction](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/impact/diffraction.rs).
