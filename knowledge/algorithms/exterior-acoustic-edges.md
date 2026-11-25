---
type: Algorithm
title: Sound around a closed convex object
description: Identify exterior wedges across mesh bindings and constrain each sound path to the outside of its obstacle.
resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/acoustic_solids.rs
licence: MIT OR Apache-2.0
status: draft
tags: [sound, geometry, paris]
sources:
  - id: regions
    title: Lazy topology for connected acoustic bindings
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/acoustic_regions.rs
  - id: objects
    title: Object profile and acoustic meshes
    resource: https://github.com/smichea/osage/blob/main/design/08-object-format.md
generated: { by: "OpenAI/Codex", at: "2026-11-24T07:09:38Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A thick obstacle can have a diffracting exterior corner even when the triangles on its two faces meet perfectly. A source enclosed by the same opaque geometry needs to stay isolated from an outside listener.

# Idea

Prove that a connected group of triangles bounds a solid convex volume. Its noncoplanar face junctions are exterior wedges. Both segments meeting at a wedge must approach from outside the volume or along its boundary.

# Method

Use the active, transformed acoustic meshes from the object tree, including physical proxies. Join exactly coincident vertex positions and normalize signed zero. On demand, collect blocking bindings connected by overlapping world bounds to the crossed surface. Connect their triangles along undirected edges across bindings. [Preparation of closed acoustic regions](acoustic-solid-regions.md). A potential shell has two incident triangles on each edge. Separate connected components even when they share a binding. [^objects]

Orient each face away from the centroid of the component's unique vertices. Reject flat components, degenerate faces and any face whose outward half-space contains another vertex beyond the model tolerance. Bounding boxes over blocks of vertices can discharge a half-space test for the whole block. Keep the distinct junctions of noncoplanar faces in certified components. Each wedge retains the normals and both owning surfaces, including their node paths. Preparing the resident track allocates the surface index and region slots. The first relevant query tests the connected group for closed convex components and caches the result for all member bindings.[^regions]

At a bend, compare the adjacent path points to each of the wedge's two support planes. Each point needs to lie outside at least one plane or within its geometric tolerance. When a segment follows a face, rounding the GLB vertices and the query to single precision can make it cross the triangles on that face. Ignore such a hit only when both endpoints are on its plane within tolerance and the triangle belongs to the exact certified shell at a bend. A screen on the same binding but on a separate component still contributes its material transfer. [Sound through openings](/algorithms/diffracted-city-contacts.md)

# Invariants

An interior source and exterior listener on opposite sides of an opaque enclosure have no exterior path through its corners. A triangulation diagonal on a planar face creates no wedge. Reversing face winding and separating or combining the face bindings keeps the same sound for the same pressure response. A certified edge carries both adjacent owners to the receipt.

# Cost

Building the incidence map sorts triangle edges. Certifying a closed component with F faces and V vertices can take O(F × V) plane tests. Bounds reject many blocks at once. A ray uses the triangle hits in the acoustic index to identify the actual intersected shell components. A distant object grouped under the very same binding introduces no extra wedge paths. The topology and the face-to-shell lookup are shared across impacts. An audible path reports the number of single edges and distinct ordered edge pairs tested.

# Failure modes

A missing triangle or a crack between nominally adjacent vertices prevents closed-shell certification. Concave geometry needs a different exterior test or a convex decomposition. The shell layer models up to two changes of direction with the city path solver. Three-band edge losses approximate an acoustic field. They need independent measurements for a particular building material and shape.

# Alternatives

A watertight material volume can be authored separately from the visible building. An acoustic sheet can describe a wall with an opening using just its finite triangles. Both representations supply edges to the city impact solver. For the bundled bridge the city can take the four boundary segments directly from each authored quadrilateral. The general sound engine also starts from quadrilateral boundaries and computes its own single-edge propagation. Convex-shell certification belongs to the city mesh query. [City diffraction](/algorithms/diffracted-city-contacts.md) · [Sound ways](/algorithms/sound-ways.md)

# Oracle

Place an extruded obstacle between a fixed physical impact and its listener. Compare the points and travel time of each bend with a straight line on the unfolded faces. Rotate the obstacle about the source-listener axis, reverse its triangle winding, use independent owners and disconnected components on one binding. Insert a separate sheet across the shortest side path. Repeat with the source inside a closed box and with transparent faces. [Executable solid check](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-impact-solids.py)

# Reuse

Run `python3 worlds/ville-paris/check-impact-solids.py --bin target/release/ville-paris`. Each solid bend in a recording has an `exterior_wedge` with the second face's owner and the two outward normals, ordered by the reported faces.

[^objects]: [Geometry channels](https://github.com/smichea/osage/blob/main/design/08-object-format.md).
[^regions]: [Region discovery and shell cache](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/acoustic_regions.rs).
