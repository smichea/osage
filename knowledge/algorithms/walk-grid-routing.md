---
type: Algorithm
title: Finding a pedestrian route on the walk grid
description: Searches supported cells between two street positions and shortens the result where the walker's footprint can pass.
resource: https://github.com/smichea/osage/blob/main/worlds/paris/world/src/walk/route.rs
licence: MIT OR Apache-2.0
status: draft
tags: [paris, navigation, pedestrians]
sources:
  - id: astar
    title: A Formal Basis for the Heuristic Determination of Minimum Cost Paths
    resource: https://doi.org/10.1109/TSSC.1968.300136
  - id: detour
    title: Detour navigation mesh query interface
    resource: https://recastnav.com/classdtNavMeshQuery.html
generated: { by: "OpenAI/Codex", at: "2026-11-25T01:35:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

An endpoint can be reachable even when walking straight towards it leaves the pavement or pushes the walker against a wall.

# Idea

Plan on the height field used for character movement. Accept a straight segment only when the ground supports the walker's footprint along its length. Keep moving people in the local [encounter calculation](/algorithms/pedestrian-encounters.md).

# Method

Test the straight segment first. Otherwise join each exact endpoint to its cell centre, or the nearest directly reachable neighbouring centre when the first link fails. Run A* on axial neighbours at unit edge cost. The Manhattan distance to the target cell is a lower bound on remaining edge count. Break ties by remaining distance and then cell coordinates. Store the parent when a cheaper route reaches a cell.[^astar]

Split a segment wherever the centre crosses a grid line. Inside each interval the centre has a fixed ground cell. Enumerate the cells in the swept disc's bounding box. A cell whose height differs by more than one step, or has no floor, can block the segment. Compute the nearest distance between the interval and its rectangle using segment endpoints and rectangle corners. Reject penetration inside the radius. Check the height transition between consecutive centre cells. Apply the same sweep to search edges, shortcuts, and the movement step itself. A stationary support query is a zero-length sweep. For cell widths at least as large as the radius, cache one bit per cell whose 3 × 3 neighbourhood is finite and fits within one step of vertical variation. A sweep within or between adjacent cells can use this certificate when every possible centre cell along the segment has the bit. Other steps use the geometric test. The immutable grid builds the bits when loaded.

Follow parents backwards from the target. Starting at the first waypoint, try successively farther waypoints until a shortcut fails. Keep the last reachable waypoint and repeat. Ground each retained point. The server then steers towards one waypoint at a time. A patrol reverses within 5 cm of a terminal. Within one metre of an intermediate corner, it tests the line from its actual position to the following waypoint. It turns as soon as the footprint clears that line. This allows two people steering around each other to pass the same corner.

# Invariants

A successful route includes the requested horizontal endpoints. An obstructed or disconnected pair returns no route if the search exhausts its reachable cells. A valid coincident pair returns one point. A server patrol requires two distinct points. When an authored patrol endpoint clips an obstacle corner, try standing positions within its original five-centimetre arrival band. Test the same cell at radii of 10, 20, 30, 40 and 49 mm along sixteen directions. The turn at each end is still measured against the authored coordinate. A patrol with no supported endpoint or no connecting route is skipped. Starts must clear previously admitted residents.

# Cost

Only visited cells need parent and cost records. With V visited cells, the heap search on a four-neighbour graph costs O(V log V) for fixed footprint and cell size. Segment validation sorts crossing parameters. A candidate segment crossing K grid lines costs O(K log K). Greedy shortening can reconsider long portions of the route. Paris plans once at startup and reuses the waypoints for the return trip.

# Failure modes

A single height per cell cannot describe stacked floors. The disc represents the horizontal collision body. Overhead clearance requires additional geometry. Narrow passages with no usable chain of cell centres can be missed. The smoothed route has no shortest-path guarantee in continuous space. Changes in static geometry require a fresh plan. Moving people can delay or block a patrol at a waypoint.

# Alternatives

Detour takes a polygon navigation mesh and can return a sequence of polygons followed by a straightened path through that corridor. Its sliced query API can distribute search work over several updates. The walk-grid implementation uses the existing Paris street export and ground support rules.[^detour]

# Oracle

Run `cargo test -p osage-world-paris passers::tests`. Cases cover a wall with an opening, opposing walkers, a high step sealing the exit, five rooms in a U and a passage narrower than the body. With the open Bistro grid installed, set `OSAGE_PARIS_WALK_DIR` to its export directory and run `cargo test -p osage-world-paris server_finds_the_bistro_detour -- --ignored --nocapture`. The test configures only the endpoints, runs the server world at 30 Hz and checks support and motion on both legs. Set `OSAGE_ROUTE_TRACE_DIR` to an existing output directory to save the simulated horizontal positions per tick.

# Reuse

`Walk::route(from, to)` accepts horizontal coordinates in metres and returns three-dimensional ground points. A Paris street may declare world-space endpoint pairs in the optional `patrols` field of its `walk.json`. An empty list requests no residents. Without the field the server chooses the two crossing routes near the entrance on a suitable street.

[^astar]: Hart, Nilsson and Raphael, [A Formal Basis for the Heuristic Determination of Minimum Cost Paths](https://doi.org/10.1109/TSSC.1968.300136), 1968.
[^detour]: Recast Navigation, [dtNavMeshQuery](https://recastnav.com/classdtNavMeshQuery.html).
