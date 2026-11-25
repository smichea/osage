---
type: Algorithm
title: Continuous approach to a moving lift
description: Forecasts an airborne capsule meeting a finite moving box and spends native solver intervals near the earliest encounter in the rig.
resource: https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/lift_flight.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [geometry, physics, collision, replay]
sources:
  - id: flight
    title: Continuous flight and contact oracle on an authored moving lift
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/lift_approach_checks.py
generated: { by: "OpenAI/Codex", at: "2026-11-24T16:59:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A lift can rise into a falling paw between fixed solver samples. A held speculative contact and a penetrating witness can occur on different substeps. A sampled trajectory also misses a short encounter when the lift has moved away again by the next endpoint.

# Idea

Use relative motion to find an interval containing the first inward crossing. Then spend additional native solver steps around this time. Report the geometric prediction and the first nonpositive native contact independently.[^flight]

# Method

Advance the body centre in double precision under constant gravity until the terminal speed, then continue at that speed. Rotate the capsule spine about its centre at the supplied world angular velocity. Evaluate the sinusoidal lift at the absolute simulation time. In its frame find the closest points on the finite spine and box and subtract the radius. This covers sides, edges and corners as well as the deck. The [shared spine-to-box query](free-pair-impact.md) tests the endpoint derivatives before subdividing the segment at face planes.

Search the earlier half of an interval first. At its midpoint use the closest-pair normal as a separating plane. Bound the possible projected displacement by translational speed, the largest vertical relative acceleration and the angular travel of the spine endpoints. When this bound cannot close the gap, discard the whole interval. In intervals of at most 10 ns, look for an inward crossing among their endpoints and midpoint. A forecast starts with the bodies separated. Contacts within 10 ns of the earliest result are kept with their rig slots.

The replay can request a minimum sampling rate near this first approach. Subdivide the ordinary intervals intersecting a window of two base intervals either side of the forecast, until a nonpositive manifold has been read. Finish any interval already split. If another solid reports a nonpositive contact first, record when it interrupted the free flight and cancel the remaining refinement window. For each division move the platform to its prescribed endpoint and advance the full native solver, including joint constraints, other terrain and impulse reporting. Store the first native reporting interval, one of its witnesses and the number of additional solves.

# Invariants

All body and box coordinates use the same world frame. Physical radii belong to the rig slots. The target pose uses the simulation clock during paging holds. Dividing one numbered substep preserves its full elapsed time. A route without the optional clock retains its original step sequence.

# Cost

Each flight query evaluates one posed capsule and a box per retained interval. Work depends on the number of ambiguous intervals, especially near tangencies and rapid rotation. A horizontal bounding-box rejection skips flights outside the stroke. The solver pays for the local subdivisions once per configured first approach. For the 120-tick quadruped fixture at 60 Hz, a requested 1920 Hz near contact adds 21 solves to the 480 ordinary solves on each contacting route.

# Failure modes

A forecast assumes uninterrupted flight with the supplied linear and angular motion. Earlier collisions or joint torques can change the actual approach. A deeply overlapping initial pose has no new entry from free space. A nonpositive manifold records a native solver observation during an interval. Its end time can differ from the geometric first touch, and the native solve can change the trajectory. Very brief overlaps below the query time resolution can be missed.

# Alternatives

A uniformly faster clock also improves timing, with additional solves across the whole replay. A terrain-only ballistic query handles fixed triangles and supplies a source triangle ID. [Static geometry](ballistic-impact.md)

# Oracle

Tests cover a lift passing through and returning to the same endpoint, a rotating capsule, an edge with an oblique normal, terminal speed and a miss. The city oracle brackets the independent paw-to-box distance, checks the unmodified and refined clocks for both native solvers, verifies a deterministic replay, and compares paged and resident Rapier reports. A separate capsule falls onto the road above a lift, cancelling the predicted window.[^flight]

# Reuse

Call `Lift::first_impact` for a single shape. Call `Simulation::plan_lift_approach` after installing a lift to schedule the native contact window for the supplied rig. In a Ville-Paris replay set `lift_contact_hz` on the route. The resulting `lift_approach` record contains both measurements.

[^flight]: [Independent geometry and native replay checks](https://github.com/smichea/osage/blob/main/worlds/ville-paris/lift_approach_checks.py).
