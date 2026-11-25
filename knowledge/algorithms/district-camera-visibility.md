---
type: Algorithm
title: Conservative camera visibility during a district descent
description: Uses certified opaque faces near roof level and shares each cell's possible occluders between its source owners.
resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/visibility.rs
licence: MIT OR Apache-2.0
status: draft
tags: [paris, rendering, streaming, visibility]
sources:
  - id: live
    title: District render camera and GPU measurements
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/live.rs
  - id: replay
    title: Bir-Hakeim descent and street visibility oracle
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-descent-visibility.py
generated: { by: "OpenAI/Codex", at: "2026-11-23T17:31:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

At 634 m over Bir-Hakeim, Bevy's frustum admitted 2,856 meshes. The original building occlusion pass could test 87,822 pairs of faces and bounds without removing one mesh. At street level the same frustum included an entire row of façades behind a nearer opaque building.

# Idea

Choose when to spend CPU time proving occlusion. Above the highest certified opaque box in the resident scene, send the frustum survivors to the camera. At roof level and below, test building faces against cells, then test the original model owners inside those cells. The roof threshold follows the actual resident geometry and can change when the district publishes a page.

# Method

Run after Bevy has populated the main camera's visible mesh list. Only closed, correctly wound, axis-aligned boxes with opaque materials and six filled rectangles can supply a blocker. From up to twelve visible certified boxes, project the near exterior faces away from the eye. Inset each face by 3 cm and keep it clear of the near plane. A complete target bound must be behind the face and strictly inside all four side planes before the camera discards the target. The shadow views keep their own lists.[^live]

The resident index groups primitives by source node within each cell. When a page or a detail level changes, only the affected cells replace their indexed entities. [Index publication](/algorithms/resident-visibility-index.md) Test a cell's bounds against each face. If a face covers the complete cell, its owners share that proof. If the whole cell lies outside any side plane or in front of the face, its children cannot be covered by that face. Keep a bit for each remaining face and test only those bits against individual owner bounds. The twelve boxes supply at most 36 faces.

During a stationary view, retain at most one completed selection. Its identity includes the camera's world and projection matrices, the resident scene generation and diagnostic owner filters. Before reuse, compare the complete current list of frustum entities. An equal list can reuse the original output. A permutation with the same sorted identities uses its current order and skips the previously hidden entities. Camera motion, geometry publication or a different set of candidates causes a fresh test.

# Invariants

Every removal has a proof from an opaque face in the current scene. Above the roof threshold the renderer can submit additional occluded geometry. All modes preserve the source node, mesh detail, alpha semantics, page residency and physics. Capturing the district without a GPU trace exercises the normal automatic camera. A request for a single image can publish just its final scene.

# Cost

The stationary measurements below use the same poses, LOD, 320 × 180 target and Intel Iris Xe Vulkan adapter. “Meshes” counts entries extracted into the render world's main camera. “Triangles” counts the hardware clipper invocations from the completed opaque pass after the pose has settled.[^replay]

| Camera | Frustum meshes | Automatic meshes | Frustum triangles | Automatic triangles |
| --- | ---: | ---: | ---: | ---: |
| Rail descent, 634 m | 2,856 | 2,856 | 331,763 | 331,763 |
| Rail descent, 120 m | 1,842 | 1,842 | 183,975 | 183,975 |
| Rail approach, 28 m | 1,488 | 1,290 | 157,564 | 127,038 |
| Street behind the façade | 699 | 17 | 107,248 | 2,880 |
| Open view of the watched building | 571 | 21 | 84,306 | 2,922 |
| Cross street | 704 | 331 | 108,650 | 48,094 |

With forced occlusion at 634 m, sharing each cell's possible faces reduced face/bound pairs from 87,822 to 8,074 with the same submitted geometry. Automatic selection skips those tests above the 30.2 m certified roof height in this scene. At the 28 m approach the per-cell filter reduced 40,245 pairs to 10,847 before any stationary reuse. The automatic policy reused 11 of 13 stationary selections at the 28 m approach and on each street. With the same executable and watched owner, the median selection time was 18.4 µs at the approach, 14.5 µs in the blocked street, 12.6 µs in the open view and 15.5 µs at the crossing. Recomputing the face pass at each frame took 841.0, 126.0, 125.5 and 304.7 µs respectively. The moving descent reused zero frames. Its median was 80.3 µs in automatic mode and 591.1 µs with the pass forced at every altitude. CPU and opaque-pass GPU timings depend on pose and device. End-to-end frame time also includes shadow rendering, synchronization and the other GPU passes.

# Failure modes

Above a roof, a nearby building can still hide other geometry. This policy accepts the extra triangles. During the moving descent the forced control omitted 16 and 188 additional triangles at two samples above the threshold. Transparent, rotated or open buildings cannot certify these boxes. Unproven occlusion keeps geometry visible. A moving GPU trace reports a previously completed opaque pass, whose timestamp can lag the camera sample. Use the stationary rows to attribute hardware counts to a particular pose.

# Alternatives

Use `--gpu-groups off` for a frustum-only control. Use `--gpu-groups on` to force the geometric test above rooftops and recompute it at every pose. The default `auto` mode is used for regular district images and persistent GPU traces.

# Oracle

Seven matched native images compare the normal path with the frustum control. The six stationary views each supply thirteen frames and the streaming descent supplies thirty-one, including page installation and eviction. Compare the camera list extracted by Bevy, resident mesh bytes, mesh uploads, selected levels, visual reads and final RGB. Withholding source node 1077 changes zero pixels behind the nearer façade and 48,302 of 57,600 pixels from the open street. The ordinary captures agree with the traced renderer in the high, blocked and open views. A stationary camera also watched eight fine pages replace the distant scene. Every publication invalidated the selection, and all thirteen main-view counts agreed with the forced geometric pass. The raw pre-change and final receipts and images are in `bench/out/ville-paris/descent-visibility/{before,final}/`. The separate per-owner model fixture checks High, Medium and High publication while a hidden owner makes zero cold visual reads.[^replay]

# Reuse

Run `python worlds/ville-paris/check-descent-visibility.py --snapshot-controls` with the native release. Passing `--controls DIR` also compares the submissions to a frozen pre-change receipt at the same sector, viewport, source and camera poses. The script takes a GPU lock separately for each native capture.

[^live]: [District camera and GPU measurements](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/live.rs).
[^replay]: [Replay, image and hardware-counter oracle](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-descent-visibility.py).
