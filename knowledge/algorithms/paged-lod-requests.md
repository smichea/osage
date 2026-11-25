---
type: Algorithm
title: Requesting district LODs from a moving camera
description: Uses a world-space bound per cell and prior camera decisions to avoid repeating per-placement visibility and resource requests.
resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/detail.rs
licence: MIT OR Apache-2.0
status: draft
tags: [geometry, rendering, lod, streaming, paris]
sources:
  - id: selector
    title: Perspective error and hysteresis of an individual placement
    resource: https://github.com/smichea/osage/blob/main/crates/osage-objects/src/visual_detail.rs
  - id: exercise
    title: Moving-camera and stationary-camera demand oracle
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-lod-evaluation.py
generated: { by: "OpenAI/Codex", at: "2026-11-23T18:53:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

On a field of independently placed objects, the resident district can spend time testing every owner against the same camera planes and rebuilding identical mesh and texture dependencies every tick. A stationary camera also needs to notice a falling actor entering visual range and a model that just finished loading.

# Idea

Build a bound for each cell from the world-space bounds of its adaptive placements. Use this bound to decide whether all placements can skip individual frustum tests. Retain each placement's history of projected error and the cell's last resource demand. [Projected mesh error](/algorithms/projected-mesh-error.md)

# Method

During catalogue validation, transform each model bound through the placement's full affine transform. Take the componentwise union in double precision. A placement can extend across a tile boundary, so the union uses its actual bound. Keep its canonical owner in the source tree.[^selector]

At each simulation tick, test the cell's range against the actor and camera. The resident cell gets its ordinary half-cell release margin. Revisit its visibility if the range test changes, or if the camera position, direction, projection or viewport changes. Compute the near and four side planes once for the view. For each plane, evaluate the centre distance and the support radius of the enclosing cell box. A wholly outside cell releases previously requested owners. A wholly inside cell proceeds directly to each owner's projected-error selector. In a straddling cell use the individual owner's bounds. A scale-relative floating-point margin sends uncertain whole-cell comparisons to the individual test.

Track the desired level and the level currently drawable separately. A change to either marks the tile's resources for reconsideration. Gather its unique active and desired variants, then expand their mesh and texture dependencies. An initially preloaded catalogue registers every level once. The publishing pass considers decodes and evictions on each simulation tick and revisits the owners of affected cells. [Publishing district LODs](/algorithms/publishing-paged-lods.md)

# Invariants

The same view and current level give an idempotent hysteresis decision. A placement can still require a mesh after a neighbouring instance stops using it. The union includes every required active model until the replacement can be published. A static camera can receive new resources and can gain or lose eligibility through actor movement alone. Physical and acoustic owners stay in the source hierarchy. The ordinary district reader skips the selector entirely when the catalogue has no multi-level owners.

# Cost

The benchmark uses 480 placements of a converted two-level masked plant in 20 rows and nine cells. Its High and Medium meshes contain 126 and eight triangles. The controls use the same placements, projection and files for both release executables. The renderer is Vulkan on Intel Iris Xe, with a Core i5-1335U. The viewport is 320 × 180. A monotonic timer covers the main-thread LOD request and resource selection pass on every simulation tick, including any thread preemption. Other simulation, publication and GPU work have separate counters. Each number is the median of five interleaved run averages on 23 November 2026.[^exercise]

| Camera and streaming | Ticks | Previous µs/tick | New µs/tick | Previous projections | New projections |
| --- | ---: | ---: | ---: | ---: | ---: |
| Mixed motion and holds, 80 ms reads | 333 | 186.15 | 61.91 | 124,889 | 58,849 |
| Continuous motion, all resident | 333 | 133.47 | 83.90 | 140,127 | 140,127 |
| Moving, looking away | 181 | 56.53 | 1.62 | 0 | 0 |
| Static close camera, 80 ms reads | 181 | 144.42 | 5.26 | 51,042 | 282 |
| Static distant camera, moving actor | 141 | 69.84 | 8.78 | 7,200 | 240 |
| Moving almost vertically above the field | 151 | 198.78 | 100.31 | 66,259 | 66,259 |

During continuous motion all 140,127 in-view owner projections remained necessary. The cell test left 30,635 individual frustum checks out of 159,840 possible owner visits. The nine preloaded cell resource sets were built once. On the route mixing motion and holds, both programs made exactly 2,380 desired-level changes, 2,377 drawable-level changes and ten visual page reads. The stationary close view requested models at tick 0 and first published at tick 9. The separate moving actor entered range at tick 41, first published at 44, and first released an owner at 66. A mostly visible group therefore still benefits from shared view-plane and dependency work while a stationary one also saves projections.[^exercise]

# Failure modes

A group-wide test cannot certify partial cells. A close camera cutting through the group still demands owner tests and projections. The bounds and camera cache assume static source placements. A moving object or an edited catalogue needs updated bounds before it can join this path. Without a camera change, availability can still change both the active level and its resource requirements. Dirtying only on camera movement would keep obsolete resources or drop a working fallback.

# Alternatives

Running the individual test and rebuilding the demand on every tick supplies the baseline. A deeper spatial hierarchy could further subdivide a large boundary cell. The present reader uses the existing cell directory and keeps one pair of world-space bound vectors per adaptive cell.

# Oracle

The CPU oracle runs six deterministic routes against the previous executable. It compares the complete streams of desired and published levels, resource events, page and byte counts, publication times, each owner's final state, and native camera meshes on the sampled frames. The continuous movement control changes the camera on every tick. The away-facing control demands no pages and performs no individual bound tests. A fixed camera admits an actor only while its trajectory intersects the district's visual range. A separate fixed-camera run publishes initially delayed models. The overhead-camera route exercises a nearly vertical view. The numerical test also covers the exactly vertical basis, the near plane, and world coordinates around one billion metres.[^exercise]


All 84 render-world samples on the mixed route have the same submissions and resource counts. At tick 216 each pair of 320 × 180 native captures matches byte-for-byte, both with all models ready and with the replacement Medium mesh in transit. The two cases have distinct final images. On the historical Bir-Hakeim source with single-level owners the control kept the same 25 street samples and 31 aerial samples, and the two final native images matched each reference. A separate damaged-Medium test retained High and the original collision and acoustic owners. The per-case receipts and comparison images are under `bench/out/ville-paris/lod-evaluation/`.[^exercise]

# Reuse

Run `python worlds/ville-paris/check-lod-evaluation.py --reference PATH` with a previous executable exposing the LOD timing counters. The default builds an independent public two-level panel field. Pass `--pages DIR` for a cooked adaptive catalogue with the same field layout. The benchmark writes the raw traces, images and summary under its output directory.[^exercise]

[^selector]: [Projected geometric error](https://github.com/smichea/osage/blob/main/crates/osage-objects/src/visual_detail.rs).
[^exercise]: [Per-owner LOD evaluation oracle](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-lod-evaluation.py).
