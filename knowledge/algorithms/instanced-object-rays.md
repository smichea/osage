---
type: Algorithm
title: Sharing triangle indexes across object placements
description: Preserves each source node and face while querying repeated models through a two-level bounding-box hierarchy.
resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/rays.rs
licence: MIT OR Apache-2.0
status: draft
tags: [geometry, collision, acoustics, hierarchy, paris]
sources:
  - id: inspector
    title: Resident object bindings and placement index
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/rays.rs
  - id: mesh
    title: Source face order, exact correspondence and affine boxes
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/rays/mesh.rs
  - id: scaling
    title: Batched rays across repeated public ramp models
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-object-ray-scaling.py
  - id: stages
    title: Thread CPU, triangle counts and allocation measurements
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/rays/measurement.rs
  - id: affine
    title: Edges, reflected frames and deliberately different collision data
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-instanced-ray-affines.py
  - id: crossings
    title: Face centres, adjacent and coincident triangles
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-object-rays.py
generated: { by: "OpenAI/Codex", at: "2026-11-23T22:01:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

An inspector can return the visual, collision and acoustic triangles crossed by a finite segment. A city can place the same model hundreds of times. Building one world-space triangle hierarchy over all those copies spends memory proportional to the number of placements times the number of faces. Looking up every binding for each ray also charges a distant miss for the entire city.[^inspector][^scaling]

# Idea

Keep the model's triangles in local coordinates and put the placements in a second hierarchy. Attach source bindings to a placement so the exact intersection can be reused by every channel that demonstrably has the same geometry.[^inspector][^mesh]

# Method

Build a local bounding-volume hierarchy (BVH) for each distinct visual mesh. In the order of its primitives, record the cumulative triangle counts so each leaf maps back to an original face ordinal. Build a world-space BVH whose leaves carry placements and whose bounds enclose the full local model under the accumulated parent transform.[^inspector][^mesh]

A collision mesh read in world coordinates can share its owner's visual placement when its triangle triples equal the concatenated source index stream in order, and every transformed binary32 vertex has the same bits as the collision vertex. A declared acoustic collision proxy shares its physical mesh. An independently authored sound surface referencing the same source at the same owner can share with collision after comparing all ordered indices and vertex bits. Other world-space meshes get their own triangle tree.[^mesh][^inspector]

Intersect the segment with the outer tree. For every surviving placement project local node bounds into world coordinates, padding each axis by four binary32 epsilons times its largest absolute bound or one, whichever is larger. Translation-only placements shift the bounds directly. For a general affine, take the coordinate extrema of its eight transformed corners. A non-finite projected box keeps the node eligible. Test the surviving exact binary32 triangle in world coordinates with the common double-precision segment routine. Build the source paths, materials and intersection lists only for the bindings hit. Sort by distance and source ordinal within each binding.[^inspector][^mesh]

# Invariants

Sharing preserves the original face ordering, including ties on shared edges and coincident triangles in consecutive primitives. A geometric result reports separately under each owning node and material. Full parent transforms contribute to both the broad phase and the final point. The narrow phase uses the same world-space arithmetic as the previous flat index.[^inspector][^mesh][^crossings]

# Cost

On 2026-11-23 an Intel Core i5-1335U, pinned to logical CPU 0, ran the same 243,692-byte GLB with 1,024 placements and 3,072 bindings. The benchmark alternated two runs per release in opposite orders and discarded the first of eight rounds in each process. The medians below pool 14 warm thread-CPU samples per release. Each query figure divides the time for its group by 16. This is a CPU-only inspector.[^scaling][^stages]

| Operation or storage | Previous flat index | Placement and model indexes |
|---|---:|---:|
| Build the resident inspection index | 184.60 ms | 5.36 ms |
| Ray through one face | 54.70 µs, 4–6 triangle tests | 28.40 µs, 2 tests |
| Ray past the district | 19.39 µs, 0 tests | 2.42 µs, 0 tests |
| Coplanar segment along 32 placements | 1,352.82 µs, 6,400 tests | 822.20 µs, 2,560 tests |
| BVH leaves | 207,360 | 1,024 placements + 81 triangles |
| Tree allocation reported by Parry | 16,588,736 B | 88,272 B |
| Net live heap growth across index construction | 23,198,144 B | 467,984 B |
| Peak resident process memory for a 48-ray batch | 107,898 KiB | 63,284 KiB |

For RSS, a fresh small monitor launched each ordinary CLI process and collected its Linux high-water mark. Eight runs per case used alternating order. Without rays the two process medians were 61,092 and 62,060 KiB. The model already retained 3,575,808 bytes of physical positions and indices and 1,787,904 bytes of independently authored acoustic arrays on both versions. Hierarchy parsing took 16.41 and 15.12 ms. Building the profile JSON value took 14.87 and 14.36 ms, and pretty-printing the result took 5.01 and 5.33 ms. These stages sit outside the reported index build and queries.[^scaling][^stages]

# Failure modes

A rotated, elongated or strongly sheared model may project loose world boxes. The oblique control performed 123 triangle tests across 24 rays, compared with 120 for the flat index, for identical intersections. A singular authored placement receives the existing importer error before the inspector runs. The bounds unit test separately exercises a collapsed axis and a finite matrix whose projected corners overflow. Collision and acoustic source meshes already expanded for other consumers remain resident. Checking their correspondence at build time still visits their vertices and indices per placement.[^affine][^mesh][^stages]

# Alternatives

A flat world-space BVH offers tight boxes for individual placed triangles and avoids transforming node bounds during traversal. A traversal that moves the ray to local space would require an inverse and a separate treatment of rounding at edges. The implemented traversal keeps the established world-space edge and endpoint tests.[^mesh][^affine]

# Oracle

The scaling test places one 81-triangle public ramp at 1, 16, 64, 256 and 1,024 nodes under row parents. Each placement has three channel bindings. Half of the acoustic surfaces are authored and half are explicit collision proxies. For each scene the test compares all detailed hits for 48 rays with the preceding batched inspector. It also checks seven single rays against the earlier nearest-surface interface. The three groups hit a nearby face, miss the district or skim an entire row. The test recreates the production JSON from individually timed stages and verifies the digest of the pretty-printed output.[^scaling][^stages]

The transform controls exercise exact vertices, common edges, interior points, finite endpoints, shortened rays, reversed segments and empty space. Exchanging two colliding source faces must return face 80 on the collision mesh and face 0 on its visual sibling. Raising collision heights by one binary32 step must move its reported point while leaving the visual crossing at its original position. The same checks run through the ordinary `--inspect-object --ray` entry point.[^affine][^crossings]

# Reuse

Build the immutable index once per loaded object and reuse it across the requested segments. The object tree defines ownership and surface identity. The two BVHs are derived search structures. The streamed-district inspector has a separate availability and coverage contract.[^inspector][^mesh]

[^inspector]: [Resident object index](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/rays.rs).
[^mesh]: [Source mesh and affine bounds](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/rays/mesh.rs).
[^scaling]: [Placement and process comparison](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-object-ray-scaling.py).
[^stages]: [Native stage measurement](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/rays/measurement.rs).
[^affine]: [Transform and fallback controls](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-instanced-ray-affines.py).
[^crossings]: [Source-face intersection oracle](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-object-rays.py).
