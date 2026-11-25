---
type: Algorithm
title: Finite segment intersections and missing physical coverage
description: Queries terrain triangles and moving support faces at their installed poses, including gaps in available terrain.
resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/solver/rays.rs
licence: MIT OR Apache-2.0
status: draft
tags: [geometry, physics, collision, streaming, paris]
sources:
  - id: segment
    title: Double-precision finite segment and triangle tests
    resource: https://github.com/smichea/osage/blob/main/crates/osage-objects/src/profile.rs
  - id: resident
    title: Resident object geometry and binding selection
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/rays.rs
  - id: gaps
    title: Coverage of absent and partially installed static terrain
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/support/rays.rs
  - id: object-test
    title: Authored face centres and coincident surfaces
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-object-rays.py
  - id: actor-test
    title: Actor shape intervals on live physical poses
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-actor-rays.py
  - id: moving-test
    title: Moving support intersections and coplanar intervals
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-moving-surface-rays.py
  - id: district-test
    title: Paired resident, partially installed and promoted district snapshots
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-paged-collision-rays.py
generated: { by: "OpenAI/Codex", at: "2026-11-23T22:01:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A distance to the nearest surface cannot identify which original triangles a segment crossed. Adjacent faces can meet on the segment and two authored faces can occupy the same position. In a streamed district the same empty hit list is possible both in open air and above an unloaded surface.

# Idea

Collect crossings of installed terrain and moving supports. A segment travelling along a box face also gives a contact interval. The streaming catalogue identifies the parts of the segment where the answer could change after additional geometry arrives.

# Method

The input is a pair of distinct finite world-space positions in metres. Build the segment direction and length once in double precision. Reject bounding boxes by intersecting each axis's parameter interval with the current closed interval. Test surviving triangles from both sides using barycentric coordinates. Include isolated intersections at either endpoint and preserve all distinct face ordinals at tied distances. Parallel and coplanar segments produce no unique triangle crossing.[^segment]

For a resident object, enumerate the selected primitives in each placement and retain their source face offsets. Share each physical mesh's tests with its acoustic proxies. When an intersection occurs, report it separately under each associated binding. The binding retains the tree path and its own material where one exists. Sort by distance, breaking ties within a binding by original face ordinal. The spatial index is built once for all the segments of that inspection.[^resident]

For streamed terrain, form a spatial index over installed native colliders and the unobserved regions of the district. An installed collider supplies its existing triangle hierarchy. The local triangle maps to a source ordinal where the directory declares it. An inactive page contributes its physical bounds. An active certified subset contributes its remaining bounds or certified residual triangles. A gap that intersects the segment makes its coverage incomplete even when installed geometry supplies a hit. Old directories may have complete coverage and unknown source ordinals. [^gaps]

The same outer index includes the colliders of the lift and the freely rotating bonnet. Take each pose and half extents from the solver at the reported tick. Transform the segment into the box frame using double precision. For each transverse face, intersect its plane and check the other two coordinates against its half extents. Report the local face name, world point, distance, surface identity and authored material coefficients. An edge can return two face names at one point. For a segment lying in a face plane, clip its interval against the other axes and report both endpoints in `box_face_overlaps`. A segment wholly inside the solid has no surface crossing. The default mask selects terrain and moving supports.[^moving-test]

With `--collision-ray-actor`, add each actor collider to the same outer spatial index. A physical part is a sphere or a capsule with a local Y axis. Read the actual pose from its native collider. For each selected part, compute the line interval inside its finite cylinder and the two end balls. Their union is convex. Intersect that interval with the requested segment. The sphere case needs one ball. Measure the roots from the point on the line nearest the body centre so even distant endpoints retain distinct local entry and exit points. Each boundary record has a world normal and a part index. The per-part occupied span also records its endpoints and length when a segment starts inside, stays inside or runs along the cylindrical side. Joint neighbours may overlap, so a segment can carry several spans. The outer query still records unknown terrain on the same segment.[^actor-test]

The actor receipt identifies each physical part and its parent. For an imported rig, `actor.bindings[part]` records its bone ID, node index in the actor GLB and authored material name. The GLB digest identifies the tree to which that index belongs. District node indices and actor rig node indices have separate owners.[^actor-test]

# Invariants

Changing the direction of a segment preserves the set of isolated crossings. Distances are measured from the requested start. Distinct source ordinals survive equal position, distance and material. A query leaves the simulated state and read schedule unchanged.[^object-test][^district-test]

# Cost

The resident inspector indexes model triangles once and places their bounds in an outer tree. A collision or acoustic surface can share its owner’s triangle tests when their agreement is established. Independent world-space geometry retains a separate tree. The [placement index](instanced-object-rays.md) reports its build, memory and query costs. The district inspector reuses native triangle trees and constructs the outer tree of installed colliders and missing regions once per batch. In the worst case a segment can visit every bound. Hits are sorted per binding in the resident reader and per segment in the district reader.[^resident][^district-test]

# Failure modes

A loose missing-page box can mark a clear segment incomplete until the reader obtains finer coverage. A missing historical source map leaves the ordinal unknown for an otherwise measured hit. Finite precision and the parallel tolerance limit the distinction between grazing contact and a coplanar segment.[^segment][^gaps]

# Alternatives

A contact manifold provides the face used by the native physical solve during a simulation step. An acoustic path may only need the first hit on a surface. The inspector retains all crossings for callers that need to examine a seam, a coincident binding or a partially installed page.[^segment][^object-test]

# Oracle

The object check sends a segment through each of 81 authored faces and probes a shared edge, distant points on one triangle and a clear segment. Coincident triangles in two consecutive primitives retain both ordinals in each of the three channels, from either direction and at the endpoint.[^object-test]

The district check queries the same location before and after a missing collision page arrives. The complete edge returns original faces 4 and 39 at the same world point. The partial snapshot returns face 4 and a gap. A second source has coincident faces 0 and 1. Paired runs without probes preserve streamed bytes and simulation events. A segment clear of the district performs zero triangle tests.[^district-test]

A second oracle compares every probed run with its original simulation without probes. It queries twelve solver snapshots, with resident and streamed terrain, using the lift at three heights and the moving bonnet at three poses. The fixture exercises entry and exit on each box axis, rays starting inside, endpoints, shared edges, clipped coplanar spans, the former lift position, and a ray which crosses a known moving surface and unknown terrain on the same snapshot.[^moving-test]

The actor oracle uses the capsule proxy, the built-in articulated pedestrian and the imported glTF rig. It compares live query results with an independent distance-to-segment calculation and bisection. Tests cover occupied interiors, both boundary normals, adjacent body overlaps, actor and terrain on one segment, and exact endpoint, tangent and far-origin controls. It changes one imported forearm material and verifies the reported node, material and coefficients against the source GLB. Across nine snapshots, 999 segments reached 1,686 exact actor shape tests out of 14,747 possible segment and part pairs. Sixty-seven segments returned both actor and environment surfaces.[^actor-test]

# Reuse

Use repeated `--ray FROM TO` with `--inspect-object` or repeated `--collision-ray FROM TO` with `--paging`. Each endpoint is a comma-separated XYZ position. Read `coverage.complete`, `intersections` and `box_face_overlaps` together when interpreting a support query.[^resident][^gaps]

[^segment]: [World-space segment tests](https://github.com/smichea/osage/blob/main/crates/osage-objects/src/profile.rs).
[^resident]: [Resident object inspection](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/rays.rs).
[^gaps]: [Static district coverage](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/support/rays.rs).
[^object-test]: [Object oracle](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-object-rays.py).
[^district-test]: [District oracle](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-paged-collision-rays.py).

[^moving-test]: [Moving-surface replay](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-moving-surface-rays.py).

[^actor-test]: [Physical actor segment oracle](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-actor-rays.py).
