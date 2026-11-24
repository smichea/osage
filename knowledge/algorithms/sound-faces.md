---
type: Algorithm
title: Sound faces from meshes
description: The faces sound meets taken from an object's acoustic and collision meshes — triangles paired into flat convex quads, faces of one plane that share a side joined into one sheet whose inner sides neither bend sound nor end a reflection, a tree of boxes over the faces that stay — so that a surface sounds and costs the same however finely its mesh is cut, and a way is looked for only among the faces near it.
resource: https://github.com/smichea/osage/blob/main/crates/osage-sound/src/faces.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [sound, acoustics, meshes, bounding-volume-hierarchy, object-format]
sources:
  - id: allen-berkley
    resource: https://doi.org/10.1121/1.382599
    title: J. Allen and D. Berkley, Image method for efficiently simulating small-room acoustics, JASA 65(4), 1979
  - id: own
    resource: "osage's own design and measurements, issue #15, 2026-11-24"
    title: osage design
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-24T23:30:00Z" }
---

# Problem

The [sound ways](/algorithms/sound-ways.md) were found among a few flat four-cornered faces written by hand. A real object gives sound triangle meshes ([osage-objects](/modules/osage-objects.md): `acoustic[]` and `collision[]` on each node). Taken one triangle at a time, a wall is wrong three ways: sound bends round the diagonal that cuts it in two, as if the wall ended there; a reflection fades out as its mirror point nears that diagonal, as it should at the wall's real edge; and every triangle scatters as much as the whole wall did. A mesh also has many faces, and the ways looked at every face for every emitter.

# Idea

A surface is a sheet, however it is cut: faces of one node, in one plane, alike (the same material and flags), that share a side. Their shared sides are not edges. A sheet mirrors from its plane, by the faces under the mirror point, and scatters once. The ways ask an index for the faces near them, and the search returns exactly what it would among all faces, because it never depended on their order[^own].

# Method

**Faces.** A face is a quad (four corners in order round it), or a triangle as a quad whose last two corners are one: that side has no length, bounds nothing, and is not an edge. Two triangles of a mesh that share a side, lie in one plane (the fourth corner within 1 µm of the first triangle's plane) and make a convex quad become that quad. Triangles of no area are dropped.

**Sheets.** Sides are keyed by their node and their two corners (to 10 µm). Two faces that meet on a side are joined when their normals are parallel (|n₁·n₂| > 1 − 10⁻⁹), they lie on either side of the side (their inward directions across it are opposite: the two faces of a panel seen from both sides stay two sheets) and they are alike. The side is then shut in both. A sheet is named by its first face.

- *Straight through.* A point of the plane on a shared side belongs to one of the two faces only: the one whose inward direction across it comes first positive by x, then y, then z. Nothing slips between two faces of a wall, and nothing is stopped twice.
- *Round an edge.* A way that crosses a sheet goes round the open sides of all its faces, wherever they are, not round the sides of the face it crossed. The grazing loss beside a straight way counts open sides only.
- *Mirrored.* The image and the mirror point `p` come from the sheet's plane[^allen-berkley]. Each face of the sheet near `p` sends `smoothstep(m)`, where `m` is the least, over its sides, of `d/R` for an open side and `½ + d/R` for a shut one (`d` how far inside across that side `p` is, `R` = 0.5 m). Across a shut side the two faces' shares sum to one; at an open side the reflection fades out as before. What stands on the legs to `p` is checked once for the sheet.
- *One way.* The faces of a sheet are alike, so what they mirror is one way: the same image, the same point, their bands summed. The engine keeps one line for each sheet.
- *Scattered.* From the sheet's point nearest `p` (`p` itself when a face is under it, else the nearest point of the faces where the sheet ends), once per sheet, with the material of the face it is on.

**The index.** Two trees of boxes, split at the middle box along the longest side, four boxes to a leaf: one over the faces, one over the sheets (each the box of its faces). What stays is in the trees, built once and again when a node comes to be driven; faces and sheets on a node that moves are kept beside them and looked at each time. Queries:

- `along(a, b, r)`: faces (or sheets) whose box comes within `r` of the segment. A straight way asks the faces with `r = 0` for those it goes through, and the sheets with `r = √(d(2L + d))/2 + d`, `d = c/(8·250 Hz)`, for the edges it grazes: an edge farther than that lengthens the way by more than a quarter of a half wavelength at the lowest band and takes nothing from it. Legs ask the faces with `r = 0`; a foot along the vertical from 0.6 m above to 1.5 m below.
- `within(a, b, ℓ)`: sheets whose box can be on a way from `a` to `b` no longer than `ℓ` (the distance from `a` to the box and from the box to `b`). An emitter asks with how far it is heard, plus the farthest its trail has taken it from where it is now.

**Edges.** A sheet's open sides that lie end to end on one line are one edge, found once (from the faces' corners, so it moves with them): a road cut into a thousand strips has its four edges, and a way round it or beside it looks at four.

What the index returns is sorted by face number where an order could matter.

**Contacts from a solver.** A contact the physics reports (`osage-dynamics`: a point, the surfaces, an impulse, an instant) is told to the engine as a node, a collision material, what struck it and how hard; it is struck at its own sample. Feet and wheels stay as stand-ins for scenes without a solver; a wheel's blow is told by a change of surface (node and identifier), not of face, so a mesh's triangles do not knock.

# Invariants

- The bridge, with its faces as meshes of the object format (two triangles each, 32-bit corners), renders within −126 dB (high) and −140 dB (low) of the bridge with its faces written by hand; the scene digests of the hand-written bridge are unchanged by the index and the sheets (tested).
- A wall cut into 1, 9 or 16 cells, as quads or triangles, gives the level of the whole wall to 0.01 dB; the wall takes 26 dB from what it hides (tested).
- The index finds the same faces as a search of all of them, with faces that move and faces that stay (tested); every way test runs with half its faces in the tree and half beside it.
- A contact the Rapier solver reports as a body lands 20 m to the right is heard on the right, its distance late after the instant the solver gave, within 3 ms (tested).

# Cost

The bridge with each of its 7 faces cut into strips of triangles (`paris-soundscape bench --part faces`), headphones, ms of one processor for a second of sound, Intel Core i5-1335U under a load of 10 to 24 from other sessions, 2026-11-25:

| Quality | 7 faces | 70 | 700 | 7 000 |
|---|---:|---:|---:|---:|
| low | 126 | 150 | 198 | 251 |
| medium | 306 | 337 | 386 | 493 |
| high | 487 | 570 | 744 | 993 |
| reference | 763 | 824 | 966 | 1 188 |

Before the sheets' edges, one line per sheet and the tree over sheets, the same 7 000 faces cost 5 372 ms at low: every face near a way, and every face of a sheet's rim, was looked at.

# Failure modes

- Coplanar faces that meet only at a corner, or overlap without sharing a side, stay separate sheets: their sides are edges.
- A mesh whose plane is curved (a dome, a cylinder) is many small sheets: each scatters and each has edges. Smoothly curved surfaces are not modelled.
- The faces sound meets are meshes given whole. Paging a district's meshes in and out (as the city world does for collisions) is not built into the engine.
- Each emitter keeps a line and an aim for every sheet (most idle): memory and the per-block bookkeeping grow with sheets × emitters — with the surfaces, not with how finely their meshes are cut.

# Alternatives

Simplifying a mesh for sound (merging faces into fewer, larger planes) would also remove inner edges, but changes the geometry and must be redone for each mesh; the sheets keep the mesh as it is. A uniform grid instead of a tree of boxes is as fast for faces of one size, and worse for a city's mix of large roads and small fittings. Ray-traced or beam-traced acoustics find higher-order reflections and need the same index, at a cost per listener that grows with the rays.

# Oracle

`crates/osage-sound/src/faces.rs` (tests), `crates/osage-sound/tests/objects.rs`, `tools/paris-soundscape/tests/object_format.rs`, `paris-soundscape bench --part faces`.

# Reuse

Any image-source and edge-diffraction search over meshes: join coplanar neighbours into sheets before searching, and give each shared side the half-open rule and the crossfading rim.

[^allen-berkley]: J. Allen and D. Berkley, Image method for efficiently simulating small-room acoustics, JASA 65(4), 1979
[^own]: osage design
