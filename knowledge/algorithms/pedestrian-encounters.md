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

The server first moves controlled walkers. In replicated ID order, each passer tries a step towards its endpoint, using the same ground-height and wall-sliding rules as player movement. If the resulting velocity clears both intervals, take that step. Otherwise try eleven headings at walking and half-walking speed, plus standing still. First maximise clearance to committed trajectories and to the starting positions of undecided walkers. Among ties prefer the step with most clearance if undecided walkers continue at their previous velocities. Cap both distances at the body diameter. Remaining ties use the distance to the preferred velocity, change from the last velocity and a soft penalty for approaching within 0.65 m over the next second. When the direct step forecasts more than one neighbour inside that comfort distance, turn the preferred velocity 45 degrees towards the walker's left for candidate scoring. A passer that has moved slower than 0.1 m/s for 0.75 s can yield for the following 0.6 s. While its direct route remains obstructed, it prefers a sideways step and can also try headings 135 degrees to either side or directly backwards. Each candidate still goes through the walk grid and reserved-space checks. A clear direct step takes priority during the manoeuvre.

Use already accepted velocities for earlier walkers. For each undecided walker, test its previous velocity over both intervals and also allow for it stopping this tick.
For more than eight participants, sort indices of the starting positions by each horizontal coordinate once per tick. Keep the greatest absolute velocity component on each axis, updating the bound after each passer decides. For the candidate velocity **v**, an upper bound on relative travel on an axis is (|v| + greatest neighbour speed on that axis) × max(tick duration, forecast duration). Binary-search each sorted axis to find neighbours whose axis separation minus that travel is at most 0.65 m. Scan the shorter span and check separation on the other axis. A neighbour outside either span cannot change the hard gap or the comfort penalty, even if it stops this tick. Each retained neighbour goes through the same two-body calculation. The default two-passer street uses a direct scan.

By default Paris looks for two six-metre crossing routes near the entrance. Streets can also supply endpoint pairs for the [walk-grid route search](/algorithms/walk-grid-routing.md). A passer reverses within five centimetres of its endpoint.

On entering a yield interval, a stalled resident checks whether its disc can move one diameter sideways on either side. If both moves fail, look for a nearby oncoming resident. Give priority to the resident that has yielded more passages, breaking ties by the lower replicated ID. Snapshot those values at the start of the tick. Exclude a resident already pulling aside. In open space the local velocity search suffices.

The yielding resident plans back towards the endpoint from which it came. Sample the retreat at intervals of at most one radius, retaining bends, until a supported lateral step of 0.65 m reaches a waiting place. If the original endpoint is still inside the bottleneck, follow its outward tangent on supported floor. Walk to the waiting place under the usual collision checks. Leave when the other resident crosses the mouth of the bay, turns and moves away along the intended direction, or leaves the world. Rejoin the patrol as soon as its next waypoint is visible to the swept disc. A completed retreat counts towards priority on later encounters. [Passing-place search](https://github.com/smichea/osage/blob/main/worlds/paris/world/src/passers/refuge.rs)

# Invariants

Swapping the two walkers leaves the predicted separation unchanged. Controlled players keep their original steps. Only the server selects a passer's motion. Its replicated walker component identifies it as a passer to clients.

# Cost

For N participants, sorting the two arrays costs O(N log N) time and O(N) memory per tick. Each candidate costs O(log N + S), where S is the smaller of the horizontal coordinate spans. Only walkers inside both spans need a closest-approach calculation. A clear straight route needs one candidate. A stalled resident in single-file traffic can additionally scan the resident list once on entering a yield interval and request one reverse route on the static grid. Sampling that route until the first bay costs one pair of lateral sweeps per radius of retreat. In a densely packed crowd, or when all participants can reach the same small area, S can still approach N.

# Failure modes

The one-second forecast assumes constant velocity. When all candidate moves overlap, the best achievable current-tick clearance can still be below the diameter. A passage with no accessible space to pull aside can still deadlock. The retreat search follows the route towards the original endpoint and then its tangent. A bay on a different branch can be missed. The grid stores one floor per cell. The height filter skips centres separated vertically by at least eye height. Static obstacles are handled when the route is planned. This local decision considers other walkers during a step.

# Alternatives

ORCA divides avoidance responsibility between agents and solves velocity constraints.[^orca] DetourCrowd combines local steering with navigation corridors.[^crowd] Paris tests a finite set of velocities that its walk grid can execute.

# Oracle

Run `cargo test -p osage-world-paris passers`. Single-file replays check complete outward and return journeys on a straight passage and one with a bend, a start inside the passage, and a narrow strip with no bay. Every movement is checked for swept terrain support, speed and continuous pair separation. A 64-person plaza replay starts each walker on a five-metre circle and sends it to the opposite point. Every pair must clear the diameter throughout each tick and everyone must reach both ends within thirty seconds. A second plaza offsets the 64 destinations around an ellipse so that walkers can block each other close to their endpoints. It must meet the same clearance and return requirements. The indexed search is checked against every participant for a 60-person field, including later decisions after a faster velocity is accepted and a tick longer than the forecast. The check compares the collision gap and the distance range used by the comfort penalty. Six pairs of straight trajectories cover head-on motion, simultaneous and staggered crossings, parallel travel and a lateral offset. Two route replays also measure continuous separation between server ticks and count visits to both endpoints. A stationary player and a grid too small to contain a crossing are separate controls. With the exported Bistro street installed, set `OSAGE_PARIS_WALK_DIR` to the directory containing `walk.json` and `walk.bin` and run `cargo test -p osage-world-paris two_passers_in_the_installed_street -- --ignored`.

For that street replay, set `OSAGE_ENCOUNTER_CSV` to a file path to record both centres, their current horizontal velocities in metres per second and the one-second overlap prediction for every tick. This records the velocities actually chosen. Each walker can change course on the following tick.

# Reuse

The closest-approach calculation applies to any two constant-velocity discs. A different world can supply its own footprint, traversability rules and preferred velocities.

[^orca]: [Optimal Reciprocal Collision Avoidance](https://gamma-web.iacs.umd.edu/ORCA/), University of North Carolina.
[^crowd]: [DetourCrowd class reference](https://recastnav.com/classdtCrowd.html), Recast Navigation.
