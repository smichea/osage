---
type: Algorithm
title: Acoustic paths through authored rooms
description: Locates endpoints in transformed rooms and computes a reciprocal geometric path through finite convex apertures, with a lower bound on its length.
resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/rooms.rs
licence: MIT OR Apache-2.0
status: draft
tags: [audio, geometry, paris]
sources:
  - id: model
    title: Authored node metadata and placement
    resource: https://github.com/smichea/osage/blob/main/crates/osage-objects/src/profile.rs
  - id: routing
    title: Room location, connectivity and simple paths
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/rooms.rs
  - id: length
    title: Finite convex aperture optimizer
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/rooms/path.rs
  - id: oracle
    title: Independent visibility, reciprocity and graph controls
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-room-portals.py
  - id: roomaudio
    title: Rendering a stationary source through a room route
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/impact/room_audio.rs
generated: { by: "OpenAI/Codex", at: "2026-11-24T07:09:23Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A listener across several rooms may hear a source through a succession of openings. The direct segment can pass through solid wall. Choosing each door centre can overestimate travel distance, and the route with the fewest doors can be longer than another route.[^oracle]

# Idea

Represent an opening as a finite quadrilateral shared by two convex rooms. For a given room sequence, minimize the sum of the segment lengths over all allowed crossings. Convexity of each room keeps the resulting straight segments inside it. Re-entering a room cannot shorten an optimum since the portion between its two visits can be replaced by a straight segment.[^routing][^length]

# Method

Compose the node transforms. Index the eight transformed room corners by their enclosing axis-aligned box and use the inverse room transform for precise containment. Validate positive extents, convex ordered portal corners, coplanarity and the two shared wall faces. Index the open connections by owner ID and mark connected components containing cycles. A breadth-first search supplies one feasible route. A tree has a unique room sequence. In a cyclic component, enumerate simple paths with an explicit stack and prune branches using world-space separations of spheres enclosing their apertures.[^model][^routing]

For a given sequence, hold adjacent points fixed and solve for one crossing. Unfold the two neighbouring points across the opening plane. If the optimum lies outside the polygon, solve the sum of the two distances on each finite edge and keep the shortest. Sweep in both directions. A subgradient of the total length gives a supporting affine function on the product of polygons. Its minimum, obtained independently at polygon vertices, bounds the optimum below. The best feasible length bounds it above. Stop on a small bracket or after 128 sweeps.[^length]

# Invariants

Room node IDs fix the internal direction of the solve. Reversing source and listener reverses the returned polyline and owner IDs. A fully closed portal contributes no edge. Partial closure reduces its open area by the declared fraction. Shared closed bounds can contain an endpoint in multiple rooms and the query reports the ambiguity. A connected query supplies both endpoints, every intervening portal and a feasible upper distance.[^routing][^oracle]

# Cost

Building the graph resolves two room names per portal, visits each adjacency to mark components and constructs a spatial hierarchy for room lookup. The first breadth-first search and visited maps cover the reachable portion of the local component. Each geometric sweep takes linear time in its number of openings. A dense cyclic component can have exponentially many simple paths. After 8,192 accepted branch extensions, return the best feasible path and fall back to the direct distance as a global lower bound.[^routing][^length]

# Failure modes

Overlapping room volumes and points exactly on a common wall can have multiple owners. Missing room IDs, singular placements, invalid closures or openings off a shared face invalidate import. Coincident crossing points or slow convergence can leave a wider distance interval. Inspect both endpoints of that interval even when topological search completed. Full closure models a sealed connection in this graph. Wall transmission, acoustic attenuation and sound playback require separate models.[^routing][^length]

# Alternatives

A straight-line test handles a sight line through aligned openings. A shortest-hop graph search supplies an inexpensive feasible starting route. A planar visibility graph over aperture endpoints provides an independent reference for upright openings with equal endpoint heights.[^oracle]

# Oracle

Exercise the resident object reader on original room trees, rotated placements, skew and vertically staggered openings, partial and full closure, ambiguous membership, disconnected components and a district of repeated buildings. Compare feasible crossing positions, independently computed path lengths, brackets, forward and reverse routes, and search counters. Include a short route with four portals that beats a two-portal route and a cyclic work-limit case with a known strict gap.[^oracle]

# Reuse

Run the Ville-Paris resident inspector with a segment in world metres or probe the cooked district with an acoustic ray. Read the optional `roomPath` member. The room and portal geometry is carried by the model tree and its cooked directory.[^model][^routing]

A room declares which adjacent room a door opens into and the polygon through which sound can pass. The room graph supplies the crossing points and the bounds on path length. In `--room-audio`, Ville-Paris schedules the mono clip at its emission frame plus the rounded travel time at 343 m/s. It passes the last incoming direction to `osage_sound::ears::hear_for` and uses the sound crate’s `edge_loss` for a single bend. The city renderer checks the material crossed by each leg and applies the resulting gains and ear delays. The sound crate’s moving-source renderer reads a voice continuously from a delay line as the distance changes. [Sound ways](/algorithms/sound-ways.md) · [Sound delay lines](/algorithms/sound-delay-lines.md).[^roomaudio]

[^model]: [Object profile](https://github.com/smichea/osage/blob/main/crates/osage-objects/src/profile.rs).
[^routing]: [Room graph](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/rooms.rs).
[^length]: [Path length](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/rooms/path.rs).
[^oracle]: [Independent controls](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-room-portals.py).
[^roomaudio]: [Rendering a stationary source through a room route](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/impact/room_audio.rs).
