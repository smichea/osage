---
type: Algorithm
title: First impact of a freely translating rigid pair
description: Locates first contact between a translating capsule or joint tree and a freely moving oriented box, then advances the native solver through that instant.
resource: https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/free_pair.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [physics, collision, geometry]
sources:
  - id: city-fixtures
    title: Osage rigid contact fixtures
    resource: https://github.com/smichea/osage/blob/main/crates/osage-dynamics/tests/contacts.rs
generated: { by: "OpenAI/Codex", at: "2026-11-23T17:33:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A discrete solver can respond before two bodies touch or use a point displaced along a moving surface. A small timing error can create a spurious torque, especially on a centered collision with a translating support.

# Idea

Equal gravitational acceleration cancels in the relative motion of two freely flying bodies. With constant orientations, the squared distance between the capsule spine and the box is a convex function of time. Its derivative brackets the closest approach. The first inward crossing occurs when the distance equals the capsule radius.

# Method

1. In the box frame, project the first endpoint onto the solid box. If the residual dotted with the spine direction is nonnegative, the squared distance is already minimal there. At the last endpoint a strictly negative derivative certifies a unique endpoint minimum. Keeping the strict comparison at the far end preserves the first witness when a whole interval is equally close. For the remaining cases, intersect the parametric spine segment with the six face planes. Between successive parameters, the squared distance to the solid box is a quadratic. Minimize it on each interval to obtain the two closest points.
2. Dot their difference with the relative velocity. This gives half the time derivative of squared distance. Bisect first for the minimum, then on the approaching branch for the first contact. With a minimum inside the spatial tolerance, a tangent trajectory transfers zero impulse.
3. While the two shapes are provably separate, disable collisions only between that pair. Advance both bodies to contact. Allow up to 2 µs of additional approach to accommodate single-precision poses, bounded by shape size, speed and the remaining approach. Sample the native contact at most 0.25 ms later, then advance the rest of the step.
4. For an inelastic contact at a unique capsule end, Rapier supplies the inverse mass and world inverse inertia of each body. Form the three columns of the contact compliance matrix from unit impulses. For lever arm `r` and impulse `j`, the contact velocity increment on one body is `j/m + (I⁻¹(r × j)) × r`. Solve the three point-velocity equations. Apply equal and opposite impulses and their torques only when the normal impulse is compressive and the tangential impulse is inside the Coulomb friction cone. Sliding, bouncing and contacts with multiple witnesses use Rapier's native contact solver. Jolt uses its native solver for every response.
5. A jointly translating tree of capsules and spheres uses the same distance query for each part against one free box. Find the earliest approaching member. Split the native step once at that time and solve the entire tree with all its contacts and joints on both sides. When the impact lies within 1 µs of a step boundary, use the existing step boundary.

# Invariants

An isolated capsule already touching the box and moving inward reports a geometric offset of zero. Rapier can apply the sticking impulse at the start of that step, and both backends advance the full native interval from the touching pose. A tangent, a separating pair and a pre-existing overlap deeper than the grazing tolerance do not report a new inward crossing. [Initial-contact test](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/tests/impulses.rs)

Both paths require one freely translating box and fixed orientations before impact. Members of a joint tree must translate together. Their poses can have arbitrary initial rotations. Pair filtering applies only to the single-capsule path. Broad-phase bounds exclude nearby static meshes, and the actor must stay above its terminal-speed clamp throughout the forecast. The public geometric time is separate from the solver's manifold event and from the tick on which a caller observes the final state. An isolated sticking reaction can fall at the end of the tick after the final native integration. If the pair produced no manifold in that substep, the contact retains both surface witnesses' signed separation and the applied impulse's time within the tick. A near miss produces no such observation. [Paired impulse and recoil tests](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/tests/impulses.rs).

# Cost

Endpoint minima take at most two projections and two dot products. An interior search uses at most eight sorted parameters. The temporal search uses up to 38 minimum and 42 crossing bisections. A 0.20 s forecast amortizes that work until its relative motion, orientation or horizon changes. The impact substep can require two extra native integrations. A fixed-mesh overlap check visits the stored bounds of the supplied meshes when constructing a fresh forecast. The joint-tree path tests the swept bounds of each member once per eligible substep. Its first crossing adds at most one native integration for the scene.

# Failure modes

Rotation, relative limb motion, additional dynamic bodies, an existing penetration or a nearby static bound keep the native schedule. The joint-tree test allows up to 10 µm of relative displacement from numeric joint projection over its forecast. The conservative bound can include empty space in a large mesh. A 1 µm grazing tolerance classifies tiny penetrations as grazing. Both backends integrate single-precision body poses, so very large world coordinates can limit the useful precision of the split.

# Alternatives

A native manifold on a short fixed timestep also transfers momentum. Its first sampled pose depends on the timestep and can create an impulse during a tangential near miss. A sweep alone reports geometry. It needs a dynamic response to change either body's motion.

# Oracle

The spine regression checks a parallel face, entry into a flat minimum, and both endpoint orderings against a box corner. The tilted-face fixture derives the collision time from a known normal, closing speed and gap. It checks the physical impulse on both bodies and exercises the fallback with a second body and a fixed surface. Separate tests move past the top and side of the box, at tangency and with a millimetre of clearance, and check velocity, angular velocity and manifold events. A sphere and an elastic capsule verify the native response outside the sticking branch. A symmetric pair of joined equal spheres hits a freely translating box. The exact two-mass recoil tests the change in velocity of the box while both contact records verify that the full joint tree participated.[^city-fixtures]

# Reuse

Use `Simulation::new` or `Simulation::from_parts` with a capsule or a jointly translating rig and one dynamic box. After each `step`, read `swept_impact_offset_s` for the geometric contact time within that tick. Read the body poses and `contacts` for the solver's response. [Dynamics interface](/modules/osage-dynamics.md)

[^city-fixtures]: [Osage rigid contact fixtures](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/tests/contacts.rs).
