---
type: Algorithm
title: Selecting and releasing acoustic meshes for district rays
description: Looks up finite segments in a hierarchy of acoustic binding bounds and shares verified meshes until their last use in the current query.
resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/channels.rs
licence: MIT OR Apache-2.0
status: draft
tags: [audio, geometry, streaming, paris, collision]
sources:
  - id: geometry
    title: First two-sided triangle hit along a finite segment
    resource: https://github.com/smichea/osage/blob/main/crates/osage-objects/src/profile.rs
  - id: cook
    title: World-space directory bounds and original surface bindings
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/paging_channels.py
  - id: oracle
    title: Disk, HTTP, source geometry and corrupt input controls
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-acoustic-reads.py
  - id: series
    title: District ray planning, verified reads and room queries
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/channels.rs
  - id: placements
    title: Resident geometry for paged impact paths
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/channels/resident.rs
generated: { by: "OpenAI/Codex", at: "2026-11-24T07:09:23Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A diagonal ray and a distant panel can overlap on each coordinate axis separately even when they occupy different parts of the segment. The old district reader scanned every binding and read all files admitted by those independent axis ranges. Two consecutive rays could read and decode the same mesh twice.

# Idea

Use each acoustic binding's world-space bounds from the district catalogue to index the original tree's placements. Intersect candidate boxes with the actual finite segment before opening mesh files. Each verified resource can serve several rays, materials and owners.[^cook]

# Method

Build the bounding-volume hierarchy lazily on the acoustic profile. A leaf identifies the source node and its acoustic binding. The geometric test works in double precision. Start with the closed parameter interval from zero to one. Each axis intersects this interval with the range where the segment crosses that axis's slab. When the direction on an axis is zero, the origin must already be inside the slab. Outward rounding of the entry and exit parameters keeps a contact exactly at a box corner or ray endpoint.

The cooker composes each node's transform with its parent's world matrix on the same float grid as the resident object reader. It transforms the actual vertices before measuring each binding's world bounds. A directory with `placed_bounds` can use these measured boxes directly. An older directory can have double-precision bounds which exclude a float vertex near a transformed edge. For these directories, widen a well-conditioned placement according to its translation, magnitude and condition. A poorly conditioned or singular legacy placement gets tested for every query because its previous box cannot establish a conservative bound. Source order breaks ties between bindings at the same distance. [World placements](https://github.com/smichea/osage/blob/main/worlds/ville-paris/paging_affine.py).

Compute the candidate bindings for the ordered rays and count their remaining uses per content digest. Read the file on its first use, check length and SHA-256, decode the local vertices and triangles, and verify indices and finite values. Apply the binding's own affine, check the geometry against its advertised bounds with an evaluation allowance for older directories, and run the same two-sided triangle intersection as the object reader. Each binding contributes its nearest hit. Decrement the remaining use count and release the decoded arrays at zero, including when the last use falls midway through one ray.[^geometry]

# Invariants

The hit retains its surface identifier, owner, source-tree path, material and coefficients. Multiple resource slots declaring identical content share the decoded mesh. A distinct placement still tests its own transformed triangles. A series owns its decoded cache, and a profile owns its spatial index. The persistent HTTP cache stores verified encoded files independently. A file missing outside the selected bindings never needs to be opened.

# Cost

The public scene has 67 acoustic bindings, 66 unique meshes and two transformed bridge spans sharing a collision proxy. The chosen ray runs from (0, 5, 0) to (100, −5, 100) metres. It hits source node 5 at 70.88723439378913 m. The earlier reader opened 66 acoustic files totalling 10,676 bytes. The new reader tested six hierarchy boxes, admitted one binding and opened a single 108-byte file. Both reported the same hit and material. The scene and measurements are reproducible on the Core i5-1335U Linux host used on 23 November 2026.[^oracle]

Twelve ordered rays previously required 140 acoustic reads and 22,258 bytes across individual queries. The new batch decoded three files, 374 bytes altogether. It held at most two meshes and 192 bytes of decoded vertex and index elements. This memory count excludes vector capacity, transformed vertices for the current binding, maps and query results. No decoded mesh remained after its last use. A separate HTTP origin recorded three acoustic GETs carrying 374 body bytes on the first run, then zero on a fresh process reusing the disk cache. The origin still revalidated the district index. The warm process decoded the same three verified files locally.[^oracle]

# Failure modes

A large face, a coplanar ray or overlapping bounds can admit a mesh without a triangle hit. The index cannot certify triangle coverage from a box alone. For two singular placements the legacy directory admitted both and returned one true hit. The equivalent current directory needed one file. An indexed world bound must enclose the geometry produced by the cook. A demanded mesh whose vertices fall outside the asserted tolerance fails the query. Files skipped by the index are validated when a later query actually requests them. The batch stores its candidate lists until consumed. In the worst case the ray crosses every binding.

# Alternatives

The axis-envelope reader supplies the historical control. The object inspector tests complete in-memory geometry. Either can be compared with the district's distance, owner and material. The spatial preselection API returns distinct resource numbers without reading or decoding files.

# Oracle

The oracle regenerates the same public GLB, compares each ray with the complete object inspector, and optionally executes the earlier reader. The cases include both directions, a hit at each endpoint, a zero-thickness plane, a nearly parallel ray, a coplanar miss, and a ray one float step outside the bound. It checks shared geometry under different owners, equal-distance source order, a singular affine, and release after the last use. Removing all 65 irrelevant files leaves the diagonal's result intact. A required corrupt response, a correctly hashed invalid triangle index and incorrectly advertised bounds each fail. A good response after the corrupt HTTP attempt succeeds without a poisoned cache. Cooking the channel meshes without normals preserves physical and acoustic content keys. Removing normals from a visible mesh triggers the cook's validation.[^oracle]

Fourteen additional rays in each of two directory revisions check a reflected panel, a shear, a rotation, two cancelling reflections and nested translations. Each revision reproduces the nine resident hits and five misses using a single shared acoustic payload. An exact edge on a panel at x = −3.3782730102539062 m lies outside the earlier double-precision catalogue bound.[^oracle]

# Reuse

Pass repeated `--acoustic-ray X,Y,Z X,Y,Z` options to the ordinary `--paging` command. `acoustic_probes` follows request order. The legacy `acoustic_probe` selects the last submitted ray. `acoustic_io` reports candidate bindings, new reads, reuse, release and residency. The district's `storage.cold_file_bytes` includes newly read geometry from the series. The independent HTTP log measures transferred bodies even when a new process reads the same files from its disk cache. Run `python worlds/ville-paris/check-acoustic-reads.py` and supply `--reference PATH` for the old executable. Results are under `bench/out/ville-paris/acoustic-reads/qualified/`.[^oracle]

The probe calls `osage_objects::first_hit` on transformed triangles and returns geometric hits. If the directory declares rooms, it also runs the room graph on the same endpoints and reports the result separately as `roomPath`. For impact playback, the district loader can place a verified mesh on its original node and share that binding between several sound paths. This preserves the model’s owner and material even when several placements use one file. Once a contact has a location, these reads provide the geometry between it and the receiver. Choosing which procedural contacts get voices is a separate decision in the sound engine. [Room paths](/algorithms/acoustic-room-portals.md) · [Paged impact sound](/algorithms/paged-impact-sound.md) · [Contact voices](/algorithms/contact-voices.md).[^geometry][^series][^placements]

[^geometry]: [Finite triangle intersection](https://github.com/smichea/osage/blob/main/crates/osage-objects/src/profile.rs).
[^cook]: [Cooking source surface placements](https://github.com/smichea/osage/blob/main/worlds/ville-paris/paging_channels.py).
[^oracle]: [Native query and HTTP reference](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-acoustic-reads.py).
[^series]: [District ray planning, verified reads and room queries](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/channels.rs).
[^placements]: [Resident geometry for paged impact paths](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/channels/resident.rs).
