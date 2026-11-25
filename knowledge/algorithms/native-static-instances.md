---
type: Algorithm
title: Sharing fixed collision shapes away from a moving body
description: Reuses native static mesh allocations for distant repeated surfaces and restores their original world coordinates before contact.
resource: https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/static_instances.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [physics, collision, geometry, paris]
sources:
  - id: grouping
    title: Exact translation checks and pending surface placements
    resource: https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/static_instances.rs
  - id: rapier
    title: Resident Rapier shape construction and prediction parameters
    resource: https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/rapier.rs
  - id: jolt
    title: Resident Jolt shape construction and body properties
    resource: https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/jolt.rs
  - id: states
    title: Native trajectories, contact owners and process measurements
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-native-mesh-instances.py
  - id: pointers
    title: Direct native shape address and allocation measurement
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-native-shape-storage.py
  - id: control
    title: Isolated same-executable control for native mesh construction
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/probes/native-shape-control.patch
  - id: voices
    title: Independent recordings of two simultaneous foot impacts
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-simultaneous-impacts.py
generated: { by: "OpenAI/Codex", at: "2026-11-23T22:33:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A city can place one collision mesh at many locations with different owners and materials. Creating a separate native mesh for every body duplicates the engine’s acceleration and contact data. Changing a collider’s local origin can alter native rounding in a contact or an impulse, even when every reconstructed world-space vertex agrees to the bit.[^rapier][^jolt][^states]

# Idea

Give each authored surface its own native collider or fixed body. Reuse an immutable shape for copies outside the next step's reach. Before the actor can encounter a translated copy, restore that surface’s original native world-space mesh on the same handle.[^grouping][^rapier][^jolt]

# Method

Hash each mesh’s ordered triangle indices and its binary64 vertex differences from the first vertex. Treat the hash as a candidate filter. Check every ordered triangle and every component against the candidate exemplar. The translation must be exactly representable in binary32. Applying and subtracting it must reproduce both original bit patterns. An unchanged component must preserve signed zero. Different relative rotations, reflections, index orders or deformations keep separate shapes unless the complete baked geometry passes the same test.[^grouping]

Install the fixed surfaces in their original order. The source identity, friction and restitution stay on each placement. For one isolated capsule or sphere, index distant translated copies by their original world bounds. Retain one exemplar's positions and indices for each outstanding group. Each native substep checks a world-space cube around the live actor centre. The half-width includes its circumscribed radius, a conservative centre displacement for the interval and the engine's prediction margins. Take the larger of the native linear speed cap and the measured speed plus the gravity allowance. The Rapier calculation reads the active integration length and prediction settings. The pinned Jolt adapter uses its 500 m/s speed cap, 0.02 m speculative distance and six configured position passes with 0.2 m maximum penetration correction. Pad the bounds for binary32 rounding. A high incoming speed enlarges the query.[^grouping][^rapier][^jolt]

When a placement enters the cube, recreate its ordered world vertices with the verified translation. Build the original triangle shape, put the fixed collider in its original identity frame and keep the same native handle. The surface remains detailed for the rest of the world’s lifetime. Release an exemplar's reconstruction data when all its pending placements are detailed. For jointed rigs and scenes with moving boxes, share only exact copies in the identical world frame. The source node and physical surface remain distinct from the native shape address.[^grouping][^rapier][^jolt]

# Invariants

Native contact generation uses the original world-space triangles for an approaching translated surface. Removing one fixed collider retains the immutable shape on the other body. The Rapier face ordinal refers to the original ordered triangle list. Jolt’s packed subshape ID has no source ordinal in this adapter. The streamed district continues to manage its own fixed colliders.[^grouping][^rapier][^jolt]

# Cost

On 2026-11-23 a native Linux x86_64 release on an Intel Core i5-1335U ran a 195-tick vertical drop above the far corner of a 512 m district. It contains 1,024 fixed placements of the same 81-face ramp. The counter observes actual native shape pointers during the benchmark. Rapier reports each triangle mesh's owned memory with `TriMesh::total_memory_size`, and Jolt reports the size of its `MeshShape` using the native `GetStats` method. These sizes exclude fixed body handles, the surrounding physics world, the input GLB and the reconstruction index.[^states][^pointers]

| Native shape storage | Previous | Initial after sharing | Observed through landing |
|---|---:|---:|---:|
| Rapier | 1,024 shapes, 18,075,648 B | 1 shape, 17,652 B | 3 shapes, 52,956 B |
| Jolt | 1,024 shapes, 2,349,568 B | 1 shape, 2,284 B | 3 shapes, 6,852 B |

Every run created 1,024 independent static colliders. Both engines replaced two nearby shape references during the high drop. The original per-placement world meshes remain available to other consumers. The old release expanded Jolt's mesh vertices in world coordinates and rejected the 8 km arrangement at its 2,000 m per-coordinate shape limit. Both solvers accept the compact district above.[^states][^pointers][^jolt]

The same release executable ran the 195-tick drop on logical CPU 0, alternating between separate native shapes and shared shapes. At 1,024 placements, eight pairs gave median world-build times of **152.29 to 17.38 ms for Rapier** and **97.18 to 23.85 ms for Jolt**. Every shared build finished within 25.3 ms. The fastest builds with separate shapes took 109.4 ms and 80.8 ms respectively. For Rapier, median whole-process CPU time fell from 329.11 to 179.00 ms and median peak RSS from 76,146 to 58,196 KiB. For Jolt these fell from 259.47 to 197.88 ms and from 58,886 to 57,294 KiB. The median of the runs’ 95th-percentile step times went from 66.49 to 50.91 µs for Rapier and from 20.25 to 19.75 µs for Jolt. With a single placement, six pairs gave Rapier a small overhead in median build time (0.402 to 0.415 ms) and step p95 (15.84 to 16.35 µs). Jolt went from 1.774 to 1.615 ms and from 28.78 to 22.53 µs. The single-placement build-time ranges overlap for each engine, and median peak RSS increased by 30 KiB for Rapier and 262 KiB for Jolt. Build and step times measure elapsed time. Whole-process CPU includes user and system time from startup, including the shell launcher used for the separate-shape control.[^states][^control]

# Failure modes

Using the shared translated geometry directly at contact was the negative control. In the twelve-drop fixture the unconditional prototype changed 173 of 1,092 Rapier state rows and also changed Jolt trajectories. The maximum Rapier centre discrepancy was 0.070 mm. Even a tiny change in native arithmetic can alter a contact impulse or its recorded sound. A route that eventually visits the full district may restore all its shapes. In a district of unrelated meshes, source matching and ownership checks still take initialization work.[^states][^voices][^grouping]

# Alternatives

A fully expanded world-space collider for every placement gives the original native arithmetic at the cost of one allocation per surface. Sharing only meshes in the identical world frame is available for scenes without a certified isolated-body reach. The paged terrain reader separately controls whether a physical page is resident at all.[^grouping][^rapier][^jolt]

# Oracle

The normal `--physics rapier` and `--physics jolt` routes exercise four owners, face centres, a shared edge, a sliding fall and a high fall that approaches its support after initialization. The material variant alternates restitution and friction by owner and compares the complete state CSV and run metadata. The native tests cover one-ULP changes, reordered and reversed triangles, nonrepresentable translations, shape lifetime after removing a body, exact regeneration and retirement of the final template. Two simultaneous foot impacts also retain their reference impulses and stereo samples.[^states][^grouping][^voices]

# Reuse

Keep the authoring tree as the authority for surface IDs and materials. An immutable shape can back independent fixed handles while there is an exact mapping between their geometry. Use the native representation required by the chosen physics contract before contact generation. A debugger-based allocation run and a separate uninstrumented replay provide independent storage and trajectory evidence. To reproduce paired resource measurements, apply the control patch in an isolated source checkout and build a single release. A wrapper setting `OSAGE_D178_EXPANDED_CONTROL=1` selects the fully expanded builder for `--previous`. The same executable without the variable supplies `--current`. Run `check-native-mesh-instances.py` with `--measure 8 --varied-materials`.[^grouping][^states][^pointers][^control]

[^grouping]: [Shape grouping and admission](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/static_instances.rs).
[^rapier]: [Rapier terrain and integration](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/rapier.rs).
[^jolt]: [Jolt terrain and integration](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/jolt.rs).
[^states]: [Placement and replay oracle](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-native-mesh-instances.py).
[^pointers]: [Native allocation observer](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-native-shape-storage.py).
[^voices]: [Two-impact sound oracle](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-simultaneous-impacts.py).

[^control]: [Measurement-only constructor control](https://github.com/smichea/osage/blob/main/worlds/ville-paris/probes/native-shape-control.patch).
