---
type: Algorithm
title: Publishing district LODs when their data becomes available
description: Rechecks the owners of affected cells after a request, model, image or placement page changes, and preserves the last drawable mesh.
resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/detail.rs
licence: MIT OR Apache-2.0
status: draft
tags: [geometry, rendering, lod, streaming, paris]
sources:
  - id: stream
    title: Resource verification and page publication in the ordinary district reader
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/mod.rs
  - id: compare
    title: Per-tick publication and native camera comparison
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-lod-evaluation.py
  - id: corrupt
    title: Failed model, independent sibling and physical and acoustic controls
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-paged-lods.py
generated: { by: "OpenAI/Codex", at: "2026-11-23T18:57:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

An owner may request Medium while its verified High mesh is already visible. Its neighbour may share the texture or request a different mesh. The original reader revisited every owner's draw state and scanned every resident adaptive page on each simulation tick, including ticks where the files and requested levels stayed the same.

# Idea

The rendered level can change when a request changes, a page enters or leaves memory, or a model or image completes or is removed. Track the cells affected by those events. The original tree still decides which owner receives the new draw. [District LOD requests](/algorithms/paged-lod-requests.md)

# Method

When opening the catalogue, build a reverse index from each mesh or image resource number to the cells containing a variant that can use it. Add a cell once per distinct resource even when many owners use the same model. The regular reader reports its discrete model and image read completions and evictions each tick.[^stream]

Test whether each adaptive placement page is decoded or already installed. Revisit that cell's owners after a change to its availability, any desired level or any relevant resource. For each affected owner, publish the desired level if all its dependencies are verified. Otherwise retain its previous level while all dependencies of that level remain available. Release the fine draw if both alternatives are unavailable. If the owner still requests a missing mesh, expose its coarse parent slot when the source provides one.

Apply a cell's revised slots to its existing fine page only when a drawable level or the need for its parent changes. A newly installed fine page always receives the current owner state. Sort same-tick level changes by canonical source owner before recording them. The scene publisher receives the ordinary per-page revisions. [Native entity reuse](/algorithms/stable-lod-placements.md)

# Invariants

A failed read cannot replace a valid draw. A retry uses the same resource identity and rechecks all affected cells. An owner can switch its parent fallback on or off while both its previous and next fine states are empty. Different placements can still select different levels of one model. A placement page leaving both the decoded and installed sets immediately loses its active owners. Physical and acoustic bindings keep their original source owners.

# Cost

The native comparison times the simulation thread from the start of LOD resolution through the binding and installation of newly ready pages. The monotonic timer includes reverse-index dispatch, page draw-slot updates and any thread preemption during that interval. Disk decoding and subsequent Bevy render-entity publication have separate instrumentation. The dense field has nine cells and 480 instances of a masked model with 126 High triangles and eight Medium triangles. The viewport is 320 × 180. Measurements use an Intel Core i5-1335U with Iris Xe and Vulkan on 23 November 2026.[^compare]


Each time is the median of five interleaved run means against the frozen complete-scan executable. Slot counts include the initial binding of every decoded page. The earlier complete-scan count can be recovered from each cell's owner count and its exact installation and eviction ticks. The built-in full-scan control verifies that accounting directly on the public field.[^compare]

| View | Previous µs/tick | New µs/tick | Old owner checks | New owner checks | Old draw slots checked | New draw slots checked |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Mixed motion and holds, 80 ms reads | 95.75 | 27.67 | 159,840 | 13,625 | 130,180 | 7,935 |
| Continuous motion, all resident | 78.53 | 38.69 | 159,840 | 17,580 | 159,840 | 17,580 |
| Moving, looking away | 9.83 | 0.42 | 86,880 | 0 | 0 | 0 |
| Static close camera, 80 ms reads | 116.03 | 7.46 | 86,880 | 1,850 | 74,380 | 445 |
| Static distant camera, moving actor | 26.48 | 9.47 | 67,680 | 1,990 | 6,720 | 240 |
| Moving almost vertically | 74.27 | 41.12 | 72,480 | 6,420 | 72,480 | 6,420 |

The shared model and texture resources create 27 resource-to-cell references in this field, representing 216 bytes of cell identifiers on this machine before map and vector overhead. The fixed close camera installs eight pages and binds their 445 slots once. The continuously moving view still revisits 330 cells over 333 ticks. On the mixed route the median per-run maximum time is 1.235 ms before and 1.201 ms after, and the largest single observations are 1.359 and 2.886 ms. Average work improved while individual update peaks overlapped.[^compare]

# Failure modes

A common texture can affect every cell using the model. That event takes time proportional to the owners of those cells. The static reverse index needs rebuilding when authored dependencies change. A cell holding many unrelated variants can cause extra owner checks. A fine page can first become ready when its selected level has been known for many ticks, so newly decoded pages need an initial slot update of their own.

# Alternatives

The reader accepts `--lod-publication scan` to revisit all owners and fine pages on the same executable. This supplies a control for requests, publication and event order. Individual per-resource owner lists can reduce work inside affected cells at the cost of more catalogue memory when a dense cell shares one model.

# Oracle

The ordinary reader covers camera motion, static views, an actor entering and leaving range, visible files in transit and a moving empty view. The reference and candidate must produce the same complete sequence of requests, resource and page events, published levels, owner identities, native camera submissions and captured RGB. A separate two-owner control corrupts a model while leaving the other sibling's verified geometry usable.[^compare][^corrupt]


The actual converted field retained all 84 sampled render-world submissions on the mixed route. The two native images at tick 216 matched their references byte-for-byte with all resources resident and while the replacement was pending. The single-owner scene still published High, Medium and High at ticks 15, 55 and 131. A failed Medium kept the active High. Both the distant failure at tick 113 and the independent sibling at tick 35 matched their forced-High RGB controls and each bound the 12-byte High index slice. Rendering the wrong mesh changed 22,895 pixels at the close sibling pose. The mixed fixed/adaptive page exposed one coarse instance in transit and removed it when the fine geometry arrived. Bir-Hakeim's single-level control kept its 25 street and 31 aerial samples and the two final RGB images. Receipts and images are under `bench/out/ville-paris/lod-publication/`.[^compare][^corrupt]

# Reuse

Run `python worlds/ville-paris/check-lod-evaluation.py --stage publication` for the changed-cell and complete-scan paths on one executable. Supply `--reference PATH` to compare a frozen earlier executable with publication timing. The default field is generated entirely from the public panel recipe. The optional `--pages DIR` accepts the corresponding cooked multi-level field. The raw timings, read events, camera traces and images are retained in the output directory.[^compare]

[^stream]: [District reader and publication](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/mod.rs).
[^compare]: [Publication comparison](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-lod-evaluation.py).
[^corrupt]: [Independent multi-level and failure controls](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-paged-lods.py).
