---
type: Algorithm
title: First contact on a ballistic arc
description: Finds the first time an upright capsule on a gravity-driven arc touches a fixed triangle mesh.
resource: https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/ballistic.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [physics, collision, geometry]
sources:
  - id: city-design
    title: Paris at city scale
    resource: https://github.com/smichea/osage/blob/main/design/07-city-scale-quality.md
generated: { by: "OpenAI/Codex", at: "2026-11-23T12:57:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A falling capsule can cross the edge of a finite surface between simulation ticks. The city replay needs to predict the first time the shape touches the mesh and identify the part it hits.[^city-design]

# Idea

The capsule touches a triangle when the distance from its vertical centre segment to the triangle reaches its radius. Gravity gives a quadratic translation up to terminal speed and a straight path thereafter. A separating plane at one instant can certify a whole time interval as clear.

# Method

1. Index each fixed triangle's bounding box together with its surface and triangle numbers. Split the flight at terminal speed. For each portion, query its swept box, including the vertical apex, capsule radius and half-length.
2. For each candidate triangle, evaluate the closest points on the vertical segment and triangle at the middle of a time interval. Check the face interior and each edge, including its endpoints.
3. For a separated pair, project the displacement of the capsule onto the unit vector between those points. The minimum projected displacement on the interval occurs at an endpoint or the quadratic's minimum. Discard an interval if the entire capsule stays outside the plane.
4. Visit the earlier half first. Return the first sampled contact within the time and spatial tolerances. Retain the earliest candidate across the two portions of the arc. Translate the winning surface index back to the world object's owning node.

# Invariants

Geometry starts at its supplied binary32 mesh coordinates. Narrow-phase distances and time use binary64. Broad-phase bounds expand outwards when converted to binary32. A model path comes from the physical binding. It survives any reordering inside the spatial index. A start inside the contact band is an impact at time zero.

# Cost

The index needs one record per triangle. A query visits intersecting spatial boxes and only subdivides time intervals which may contain contact. Near-tangential trajectories can require more distance evaluations. No fixed query rate or frame budget has been qualified.

# Failure modes

The shape must translate upright against fixed geometry. Rotating bodies, moving cars and articulated limbs need their own motion model. The geometric classification includes a one-nanometre contact band. Paths nearly tangent to the surface can amplify that spatial tolerance into a larger time error. An initial spine intersection admits several possible separating normals.

# Alternatives

Fixed-step solvers also compute impulses, friction and the subsequent resting pose. A linear shape cast within a tick approximates the accelerating flight by a chord. A centre ray omits the radial extent of the capsule.

# Oracle

The slab fixture checks two-sided faces, a finite edge, an outside miss and short intersections between ticks. Unit tests include a rising ceiling contact, a tilted face, a terminal-speed transition, initial overlap and a degenerate triangle. The city command consumes the replay's own initial state. A nested bridge fixture verifies the returned path against the owner reported by the rigid solver. [Dynamics source](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/ballistic.rs) · [Nested fixture](https://github.com/smichea/osage/blob/main/tools/object-convert/test_hierarchy.py)

# Reuse

Build the triangle index once per fixed mesh set. Supply the starting centre, velocity, gravity, terminal speed, half-segment, radius and query horizon. Use the returned time for prediction and the two indices to map a contact back to the caller's geometry.

[^city-design]: [City scenario and replay](https://github.com/smichea/osage/blob/main/design/07-city-scale-quality.md).
