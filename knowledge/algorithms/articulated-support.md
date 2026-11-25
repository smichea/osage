---
type: Algorithm
title: Horizontal support footprint of an articulated body
description: Measures the signed distance from a mass centre to the boundary formed by upward solver witnesses in one numbered substep.
resource: https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/support_footprint.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [physics, collision, geometry, replay]
sources:
  - id: experiment
    title: Independent geometric check on a passive articulated quadruped
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-quadruped-support.py
generated: { by: "OpenAI/Codex", at: "2026-11-24T16:28:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

Two rear paws on the lip of a platform can register ground contact while the centre of mass projects well beyond the supported edge. A Boolean contact flag loses that distinction.

# Idea

Compare the horizontal projection of the whole actor's centre of mass with the convex hull of its upward contact witnesses. Any body can supply a witness, including a shoulder during a fall.

# Method

Read body masses in kilograms and their world-space centres in metres. Accumulate the mass-weighted centre in double precision. Select witnesses in the requested numbered substep whose world normal has Y greater than 0.5 and whose signed separation is at most the caller's limit. Sort distinct XZ points lexicographically and build lower and upper convex chains. Record hull area and the nearest Euclidean distance from the projected centre to a boundary segment. Give this distance a positive sign inside and a negative sign outside.

The optional projection sums linear momentum and divides by total mass. Advance the horizontal mass centre at this velocity over a caller-supplied interval. For each witness, retain its material point on its own surface. A fixed surface holds it in place. A moving surface translates its centre at the reported velocity and rotates the arm about its world angular velocity. Rebuild the hull from the moved points, including when several surfaces move independently. The result describes which current supports could still contain the extrapolated centre. Ville-Paris uses 100 ms in its footprint receipts.

# Invariants

The footprint measures all selected contacts, including coincident witnesses from several parts. Body indices stay in rig order. A single point or a line has zero area and gives zero margin only on the point or line. An empty set has zero area and an absent margin. All positions use the same Y-up world frame.

# Cost

For B bodies and W witnesses the work is O(B + W log W) plus the scan of the contact buffer. Temporary storage is O(B + W). Ville-Paris can sample only the final pose or record the measure at every traced tick.

# Failure modes

A positive geometric margin does not establish equilibrium. Acceleration, friction, contact forces and active joint control affect whether the actor will tip. A tilted surface, sideways gravity or an animal pushing off a wall calls for a different choice of projection and eligible normals. Native solvers report contacts at their own sampling times. If a numbered substep was split, witnesses can come from more than one interval. The pose is the reported end pose. Future steps may create fresh contacts or change velocities. The projected margin can cross zero early, at the same tick as the observed margin, or without a subsequent loss of support.

# Alternatives

A Boolean support flag answers whether any contact qualifies. Measuring force distribution would require separate solver impulse data and a time convention. A controller that balances a walking actor would additionally need joint targets.

# Oracle

The core unit test covers interior points, exterior corners and degenerate hulls. The Ville-Paris oracle independently walks the hull by gift wrapping. It recomputes the centre, area and margin from raw poses and witnesses on eight 120-tick routes, checks streamed and preloaded terrain and reads the final Rapier and Jolt receipts. Four cross-paw segment queries verify the posed sphere diameters. The lateral runs start at 0 through 4 m/s. A separate core test mixes fixed and rotating surface witnesses.[^experiment]

# Reuse

Call the dynamics measurement with the ordered rig, the matching poses, the reported contacts, a numbered substep and the allowed separation in metres. The result is an observation suitable for a replay, a debug overlay or comparison between solvers.

[^experiment]: [Quadruped oracle](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-quadruped-support.py).
