---
type: Algorithm
title: Keeping render entities when neighbouring objects change detail
description: Keeps unaffected source placements alive when a paged draw changes LOD and compares the native GPU cost of real two-level geometry.
resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/live.rs
licence: MIT OR Apache-2.0
status: draft
tags: [rendering, geometry, lod, streaming, paris]
sources:
  - id: choices
    title: Choice and publication of per-owner visual detail
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/detail.rs
  - id: field
    title: Moving-camera benchmark for multi-level objects
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-dense-lods.py
  - id: affine
    title: Front and back views of nested district placements
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-affine-render.py
  - id: winding
    title: glTF 2.0 mesh instantiation and winding
    resource: https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#instantiation
generated: { by: "OpenAI/Codex", at: "2026-11-23T19:46:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

Many placed objects can share the same two mesh resources while choosing different detail levels at the same instant. A change to a single object increments its page revision. Previously the persistent district renderer recreated every Bevy render entity on that page, even when most neighbours kept the same mesh, material and pose.

# Idea

Treat the immutable placement ordinal inside a fine page as the identity of one source draw. Its original scene-tree node owns it. Retain a native entity when that draw still has the same primitive, transform and material in the new revision. [District camera index](/algorithms/resident-visibility-index.md)

# Method

The CPU projects the source mesh's certified geometric deviation using the view and its perspective focal length. A cheaper mesh becomes eligible at one pixel and may remain active up to two pixels. An active placement keeps its previous verified mesh while a requested replacement arrives. A change to its resource or cell can then revise the drawable page.[^choices] [Projected error](/algorithms/projected-mesh-error.md) · [Publishing district LODs](/algorithms/publishing-paged-lods.md)

Within a fine-to-fine update, iterate the previous and new draw lists in source order. The comparison key starts with the immutable placement ordinal and the primitive ordinal inside its mesh. The full identity includes the resource and primitive, the source owner, the world affine, all bound PBR factors, the alpha mode and threshold, sidedness, and the selected base-colour texture. A surviving draw keeps its render entity and references. A disappearing or changed draw releases its previous references after all cells have acquired their replacements. Parent fallback slots follow the fine placements in the directory's parent order.

The geometry carries its owner's full world matrix from the root to the drawable. A rotated child below a nonuniformly scaled parent can acquire shear even when both nodes use only translation, rotation and scale. The native viewer writes this matrix to the render entity before visibility checks. glTF defines the front face by the sign of the global determinant. A mirrored single-sided instance selects a material that culls the opposite winding. Double-sided materials keep both windings. When a large primitive is cut into cells and its vertices are baked into world coordinates, the cooker reverses its mirrored triangles once and transforms its normals by the inverse transpose.[^winding][^affine]

# Invariants

A render entity stays under its original source-tree parent. Two placements using the same mesh keep distinct render entities. A material change can invalidate an entity even when the geometry bytes match. Ordinary frustum tests still select the final native draw list. Replacing a coarse page and evicting a fine page use complete replacement. Cells without alternate levels continue through the single-level publication path.

# Cost

The measured field contains 480 placements in twenty rows and nine 32-metre cells. A converted masked plant supplies the genuine High mesh with 126 triangles and the Medium mesh with eight. The certified deviation is 0.7850475 m. The placements and moving camera are benchmark inputs generated for this measurement. The historical Bir-Hakeim district has single-level source owners and is covered by a separate regression.[^field]

| Settled camera | Meshes admitted by the camera | Automatic GPU clipper inputs | Forced High | Forced Medium |
| --- | ---: | ---: | ---: | ---: |
| Distant field | 480 | 3,840 | 60,480 | 3,840 |
| First close pass | 281 | 35,406 | 35,406 | 2,256 |
| Side of the field | 408 | 51,408 | 51,408 | 3,264 |
| Looking away | 0 | 0 | 0 | 0 |

The Intel Iris Xe Vulkan vertex shader ran 7,680 times on the distant Medium field and 57,600 times on its forced High counterpart. At the close pose, forcing Medium changes 39,165 of the 57,600 RGB pixels. The moving view at tick 216 selects 362 High and 118 Medium. Its image differs from forced High at 3,649 pixels and from forced Medium at 31,522 pixels. At both distant poses the masked High and Medium fields render exactly the same final image as a control with all 480 objects moved outside the frustum. With the original field position, changing only the Medium materials to OPAQUE yields 25,962 changed pixels against the background at the distant return pose, with the same 3,840 clipper inputs. At the close pose the masked field changes 51,915 pixels against the empty background. These comparisons include the complete image post-processing. The driver reports asynchronous pipeline counters. The table uses held camera poses after the previous pass completed.[^field]

An ordinary publication reconstructs the affected camera cells after updating their native entities. Over 84 sampled frames, complete rebuilding created and retired 3,621 entities. The default path created and retired 1,668. It kept 1,953 bindings whose owners continued to use the same primitive. The sum of main-thread publication time after the first frame fell from 25.775 to 15.458 ms, using the median of three interleaved runs. In this original measurement the fully resident LOD decision and resource-demand pass averaged 209 µs per simulation tick over 333 ticks. Subsequent cell-level selection measurements quantify that CPU cost on moving and stationary routes. [District LOD requests](/algorithms/paged-lod-requests.md) The distant masked pass measured 0.060 ms with automatic detail and 0.202 ms with forced High on the return view. These GPU timestamps cover the main opaque pass on this machine.[^field]

With an 80 ms simulated read delay and 8 Mbit/s for visual reads, the stable path created and retired 1,604 entities, compared with 2,693 for complete rebuilding. Both read ten placement pages (3,147 bytes), three model resources (5,157 bytes) and one texture (18,494 bytes). The model total includes a second read of the Medium mesh. On the return leg, the reader requested it at tick 215 and verified it at 221. All 480 owners still rendered High at tick 216. The fully resident control had already switched 118 of them to Medium. The median cumulative publication times were 23.471 ms with stable bindings and 24.240 ms with rebuilding. Individual samples overlap. Each render binding now records one source-slot integer, eight bytes of logical fields per primitive on this platform.[^field]

Seven placements with mixed winding share one native mesh allocation in the affine test. Its prepared slices occupy 160 vertex bytes and 12 index bytes. Each single-sided material used in both directions needs a second raster state. Winding adds no mirrored vertex copies in this case.[^affine]

# Failure modes

A slot alone cannot identify an unchanged draw. A variant can change its material or the number of primitives, and a cell can move between coarse and fine representations. The publisher compares full draw identity before reusing an entity and replaces the entire binding set when the representation changes. A page with one changing owner can have no neighbouring work to save. The selector's error bound concerns surface geometry. Different alpha coverage, material colours and texture filtering can also affect pixels.

The static affine reader requires a finite, invertible transform. Source vertices, tile interpolation and the native mesh use binary32 values, so a newly clipped vertex can shift an interpolated colour by one 8-bit unit.[^affine]

# Alternatives

The normal persistent renderer uses stable bindings. Use `--gpu-bindings rebuild` with a GPU trace to repeat complete per-cell replacement while retaining the same LOD choices. The field benchmark also cooks forced High and forced Medium scenes with the same tree, transforms, material definitions, indices and vertex data.

# Oracle

The benchmark drives a camera from a distant pose through the field, across it, away from it and finally facing away. It measures selected levels on simulation ticks, the actual Bevy render-world camera submissions, Vulkan clipper and vertex work, native entity creations and retirements, uploads and cold reads. It runs with all resource bytes resident and with the ordinary simulated 80 ms read delay. The untraced native screenshot follows the tracked camera pixel for pixel at the close and mixed poses. The former production executable retains the same submitted geometry, choices and final pictures on all 84 sampled frames and five screenshot poses. A public generated two-level panel exercises the same pipeline independently.[^field]

The affine oracle builds a separate world-space reference from transformed tangent vectors and signed triangle order. In the initial reader-only control, the native paged images differed from their reference at 14,148 pixels from above and 16,585 from below. With corrected placements, the resident GLB, normal paged capture, persistent renderer and coarse fallback reproduce the front and rear views exactly at 360 × 192. For a 54 m face crossing cell boundaries, six pixels in the paged view differ from the reference by a single channel unit. The underside matches exactly. An independent wrong-winding control on the final shared scene differs on 13,140 pixels from above and 21,725 from below. Using the wrong normal transformation changes 6,988 pixels from above.[^affine]

# Reuse

Run `python worlds/ville-paris/check-dense-lods.py` to generate the independent panel field, or pass `--model PATH` for a compatible single-node converted object with one indexed primitive per level and a certified Medium deviation. Use `--reference PATH` to include the previous production executable. The source GLB and all generated field assets stay in the local benchmark directory. The private plant receipt, individual frame traces and rendered images are in `bench/out/ville-paris/dense-lods/qualified/`.

Run `python worlds/ville-paris/check-affine-render.py` for shared resources. Add `--one-cell` to keep the small placements in local page geometry. Both modes test the native viewer, the paged capture and its coarse fallback. The checked native receipts and images are in `bench/out/ville-paris/mirrored-visual/final-shared/` and `final-local/`.[^affine]

[^choices]: [V4 per-placement detail selection](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/detail.rs).
[^field]: [Native multi-level field controls](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-dense-lods.py).
[^affine]: [Affine reference scenes and native render comparisons](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-affine-render.py).
[^winding]: [glTF 2.0 mesh instantiation](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#instantiation).
