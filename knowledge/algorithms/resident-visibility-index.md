---
type: Algorithm
title: Updating the district camera index on page publication
description: Reindexes camera placements in changed cells while retaining the source-owner groups and GPU resources of neighbouring cells.
resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/visibility.rs
licence: MIT OR Apache-2.0
status: draft
tags: [paris, rendering, streaming, visibility]
sources:
  - id: publisher
    title: Publication of district cells and owner detail variants
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/live.rs
  - id: oracle
    title: Real district and model-LOD publication checks
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-visibility-publication.py
generated: { by: "OpenAI/Codex", at: "2026-11-23T17:53:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A fine page displaces distant geometry and can change which opaque building faces qualify as occluders. Previously the main camera rebuilt its complete placement index for every publication. In a street view containing about 3,800 resident placements, even a single replacement page caused the reader to revisit all 323 cells.

# Idea

Keep a camera-index entry per source cell. Page residency and per-owner LOD revisions already tell the publisher which cells have changed. Use this information to retire and replace those cells' render entities.

# Method

The persistent renderer compares each current cell with its published page kind and revision. A change from parent to detail, an eviction back to the parent, or a new active LOD on a fine page can each mark a cell dirty. Replacements acquire their mesh handles before the renderer releases the old handles. This permits a shared mesh to move between cells in one frame.[^publisher]

Each indexed cell stores its combined bounds, source-owner groups, the identifiers of its current Bevy render entities, its count of certified opaque boxes and its highest certified roof. A dirty cell removes its former entities from the global entity lookup before registering the new source owners, bounds and render entities. A visually empty cell retains no camera entries. The global lookup maps each remaining entity to one cell and a local owner group.

A multi-level fine page keeps native entities whose source slot, owner, pose, mesh and material still match. The camera index then repairs the affected cell using those retained and newly published entities. [Stable LOD placements](/algorithms/stable-lod-placements.md)

After each publication, scan the small cell headers to compute dense offsets for per-frame owner decisions, the total number of blockers and the new maximum roof height. Equal-distance occluders use the directory cell and primitive order as a deterministic tie-break. Increment the scene generation once for the whole publication. A stationary camera then revalidates its selection on the next frame. The mesh and index buffers used by the GPU remain shared across placements. [Camera selection](/algorithms/district-camera-visibility.md)

# Invariants

A cell can be replaced while another cell still refers to the same source owner or model. Its neighbours keep their entities and collider ownership. A retired entity must disappear from the camera lookup and a newly published owner must be visible to the ordinary frustum test in the same frame. Removing the tallest building recomputes the correct automatic-occlusion threshold from the remaining cells. An unchanged revision does not advance the scene generation.

# Cost

The table covers work after the initial publication. Each time is the median, over three interleaved runs, of the total elapsed time on the main thread inside index maintenance. The previous production binary supplied the timing reference. All three modes rendered identical final images and admitted the same meshes at every sampled frame.[^oracle]

| Replay | Publication batches | Cells examined, full / incremental | Placements examined, previous / incremental | Main-thread time, previous / incremental |
| --- | ---: | ---: | ---: | ---: |
| Detail arriving in the street | 13 | 4,199 / 13 | 49,559 / 554 | 13.305 / 0.557 ms |
| Rail descent | 3 | 969 / 9 | 12,127 / 612 | 3.202 / 0.330 ms |
| Moving camera with four page evictions | 7 | 2,261 / 23 | 28,555 / 795 | 7.525 / 0.364 ms |
| Fully resident still scene | 0 | 0 / 0 | 0 / 0 | 0 / 0 ms |

The source model tree and rendered resources already existed. The cell index adds one reverse render-entity ID per placement. With all 4,959 district placements resident, the native allocation capacity of these lists totalled 39,672 bytes. The counter `resident_cell_entity_capacity_bytes` measures the lists themselves and excludes allocator headers and other index structures. The cold initial build for this scene measured 2.523 ms on the previous binary and 2.726 ms on the incremental path. Both indexed all 4,959 placements. Each subsequent publication still visits the 323 compact cell headers.

# Failure modes

A changed render geometry or material must advance its owning page revision. The ordinary page validator and the owner-LOD publisher supply that revision. A stale revision would prevent both the renderer and its index from seeing a replacement. Reindexing a one-cell model can save no placement work when that single cell changes. The controls exercise this case.

# Alternatives

Use `--gpu-scene-index full` to clear and rebuild the same index representation after any publication. The default `incremental` mode repairs only changed cells. A previous production binary can provide an independent reference for owner selection, primitive counts, pixels and CPU timings.

# Oracle

The Vulkan oracle uses the same frozen replay, camera poses, materials and LOD across the previous production binary, forced full rebuild and normal incremental mode. It compares the actual render-world camera counts, source-owner visibility, omitted indices, fine-cell residency, uploads, evictions, payload bytes and final RGB. A fully resident still scene has one initial build and zero subsequent repairs. A one-cell textured test model publishes High, Medium and High at ticks 15, 55 and 131 and reindexes the same three placements with either method. A Rust unit test retires a cell containing the tallest occluder, leaves the neighbouring owner intact and installs a new owner in a previously empty cell.[^oracle]

# Reuse

Run `python worlds/ville-paris/check-visibility-publication.py`. Set `--reference PATH` to add the comparison with the prior executable. The script alternates execution order across three trials and takes the shared GPU lock for each native run. The reports count a publication once by its scene generation even if subsequent camera frames repeat the last timing. The measured PNGs, individual event traces, binary fingerprints and timing samples are in `bench/out/ville-paris/visibility-publication/qualified/`.

[^publisher]: [Persistent render publication](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/live.rs).
[^oracle]: [City and owner-level publication oracle](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-visibility-publication.py).
