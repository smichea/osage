---
type: Algorithm
title: Predicting encounters between walking pedestrians
description: Tests simultaneous motion on the ground plane and chooses a walkable velocity for each autonomous passer.
resource: https://github.com/smichea/osage/blob/main/worlds/paris/world/src/passers.rs
licence: MIT OR Apache-2.0
status: draft
tags: [paris, pedestrians, collision]
sources:
  - id: orca
    title: Optimal Reciprocal Collision Avoidance
    resource: https://gamma-web.iacs.umd.edu/ORCA/
  - id: crowd
    title: DetourCrowd navigation and local steering
    resource: https://recastnav.com/classdtCrowd.html
generated: { by: "OpenAI/Codex", at: "2026-11-25T00:00:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

Two routes can cross at the same point at different times. A walker needs to yield when both bodies would occupy that point together and keep walking when the other person will already have passed.

# Idea

In the horizontal plane, subtract one walker's velocity from the other's. The distance from their relative trajectory to the origin is the distance between their centres. Compute the minimum on the next simulation tick and again over one second.

# Method

For centre offset **d** and relative velocity **v**, the time of closest approach is −(**d** · **v**) / (**v** · **v**), clamped to the time interval. At zero relative speed use the current separation. Two discs overlap when their squared separation is below their squared combined radius. Paris uses a 0.20 m radius per walker.

The server first moves controlled walkers. In replicated ID order, each passer tries a step towards its endpoint, using the same ground-height and wall-sliding rules as player movement. If the resulting velocity clears both intervals, take that step. Otherwise try eleven headings at walking and half-walking speed, plus standing still. Maximise the current tick's clearance up to the combined radius. Break ties by desired velocity, change from the last velocity and a soft penalty for approaching within 0.65 m over the next second.

Use already accepted velocities for earlier walkers. For each undecided walker, test its previous velocity over both intervals and also allow for it stopping this tick. At startup Paris looks for two six-metre crossing routes near the entrance, checking each arm against the walk grid. A passer reverses within five centimetres of its endpoint.

# Invariants

Swapping the two walkers leaves the predicted separation unchanged. Controlled players keep their original steps. Only the server selects a passer's motion. Its replicated walker component identifies it as a passer to clients.

# Cost

For P passers and U players, steering takes O(P × (P + U)) neighbour tests per tick with a bounded number of candidate velocities. A clear straight route needs one candidate. The server stores one motion record per nearby-world participant in the current implementation.

# Failure modes

The one-second forecast assumes constant velocity. When all candidate moves overlap, the best achievable current-tick clearance can still be below the diameter. Dead ends and dense crowds can stall a walker. The grid stores one floor per cell. The height filter skips centres separated vertically by at least eye height. This world has two local patrol routes and no global route planner.

# Alternatives

ORCA divides avoidance responsibility between agents and solves velocity constraints.[^orca] DetourCrowd combines local steering with navigation corridors.[^crowd] Paris tests a finite set of velocities that its walk grid can execute.

# Oracle

Run `cargo test -p osage-world-paris passers`. Six pairs of straight trajectories cover head-on motion, simultaneous and staggered crossings, parallel travel and a lateral offset. Two route replays also measure continuous separation between server ticks and count visits to both endpoints. A stationary player and a grid too small to contain a crossing are separate controls. With the exported Bistro street installed, set `OSAGE_PARIS_WALK_DIR` to the directory containing `walk.json` and `walk.bin` and run `cargo test -p osage-world-paris two_passers_in_the_installed_street -- --ignored`.

For that street replay, set `OSAGE_ENCOUNTER_CSV` to a file path to record both centres, their current horizontal velocities in metres per second and the one-second overlap prediction for every tick. This records the velocities actually chosen. Each walker can change course on the following tick.

# Reuse

The closest-approach calculation applies to any two constant-velocity discs. A different world can supply its own footprint, traversability rules and preferred velocities.

[^orca]: [Optimal Reciprocal Collision Avoidance](https://gamma-web.iacs.umd.edu/ORCA/), University of North Carolina.
[^crowd]: [DetourCrowd class reference](https://recastnav.com/classdtCrowd.html), Recast Navigation.
