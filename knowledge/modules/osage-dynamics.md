---
type: Module
title: osage-dynamics
description: A fixed-step CPU interface for rigid bodies, articulated actors and their contacts against city meshes and moving solids.
resource: https://github.com/smichea/osage/tree/main/crates/osage-dynamics
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [physics, collision, replay, rapier, jolt]
generated: { by: "OpenAI/Codex", at: "2026-11-24T04:42:57Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Purpose

Give a world one way to advance a chosen physical solver and read its results. The same metric meshes, masses, joint locations and initial velocities drive Rapier and Jolt. A world can compare a jump or impact one tick at a time. [City experiment](/modules/ville-paris.md)

# Concepts

The input frame is right-handed with Y up. Distances are in metres, masses in kilograms and angular velocities in radians per second. A pose locates the centre of a rigid part and gives its quaternion in XYZW order. A capsule has a radius and a straight half-length. A part with zero straight length is a sphere in both solvers and in the viewer. A box has three half-extents, a mass, material friction and a starting pose and velocity. It can move and turn under impulse.

# Architecture

The caller supplies triangle meshes in world coordinates and a fixed clock. Each backend constructs one static collider per mesh. The actor is one capsule or a parent-first tree of capsules and spheres. Built-in rigs derive each part's inertia from its shape. An imported rig supplies each part's mass, three principal moments and joint frame. [Imported impact check](/oracles/pedestrian-impact.md) The relative translation at each joint is locked. Rotation limits are given about the bind frame's X, Y and Z axes. Each adapter transforms the world anchor into the coordinates its solver expects. Parts of this actor collide with the scene and the moving bodies. Internal limb contacts are disabled on both paths.

A marked joint bounds diagonal tilt with a cone and retains an independent twist interval. [Swing envelope](/algorithms/articulated-swing-envelope.md)

Rapier uses continuous collision detection and impulse joints, with eight solver iterations for the articulated actor and up to four CCD intervals in each collision substep. Jolt uses linear casts and its native ragdoll with hinges for one-axis joints and six-degree-of-freedom constraints for the other joints, six position iterations and 2 mm of allowed penetration. The original city meshes and actors use zero restitution. New static object surfaces can supply their own friction and restitution. Both adapters use the geometric mean of the pair's friction coefficients and the larger restitution at contact. [Object materials](/modules/osage-objects.md) The world can set a maximum downward speed for actor parts. Solved angular and linear velocities pass through to the caller. [Rapier rigid bodies](https://rapier.rs/docs/user_guides/rust/rigid_bodies/) · [Jolt physics architecture](https://jrouwe.github.io/JoltPhysics/) · [Rust bindings for Jolt](https://github.com/pockerhead/oxijolt)

A scene can install one level kinematic support with `Simulation::add_lift`. Its authored centre is the bottom of the stroke. At native substep time t the target height is `base_y + travel_m × (1 − cos(2πt / period_s)) / 2`. Rapier receives the next kinematic translation and Jolt receives the target pose and substep duration. Gravity and the contact solver carry the passenger. The lift has the surface index immediately after the static meshes and publishes the native solved pose in `box_poses[0]`. The fixture and its contact measurements are described in [Ville-Paris](/modules/ville-paris.md). Near a finite edge, Rapier refreshes the platform witnesses from the collider currently attached to the lift body, including after page publication. [Lift contact response](/algorithms/lift-contact-response.md)

When an inelastic limb is already pushing on a road-supported box, Rapier uses the closing speed of the whole rig along the original face normal to decide how often to solve the load transfer within one substep. The box’s translation and rotation enter that relative speed. [Supported-box impact](/algorithms/supported-box-impact.md)

An animated actor can supply two named body transforms sampled one positive interval apart. The module creates the constraints in the authored bind pose and carries the current transforms, rotations and joint-point velocities into the native bodies. [Animation handoff](/algorithms/animated-ragdoll.md)

Rapier can also refine an incoming articulated impact on a box that the road is already supporting. [First impact on a supported box](/algorithms/supported-box-impact.md)

An isolated translating capsule or jointly translating rig can split a collision substep at its first swept contact with a free box. For a rig, the earliest touching part sets the split time, and the native solver resolves all contacts and joint constraints. The world can read the geometric time within that tick alongside the ordinary manifold and body poses. The collision solver retains control of nearby fixed surfaces, rotating pairs and multiple bodies. [Free rigid pair](/algorithms/free-pair-impact.md)

`TerrainSimulation::add_lift` exposes the same native support to the Rapier terrain adapter while static meshes enter and leave the solver. The district can ask for the height range over upcoming physics ticks with `lift_height_range`. [Paging moving supports](/algorithms/paged-ragdolls.md)

The continuous ballistic query builds an index into the existing fixed meshes. It returns surface and triangle indices so the world can recover the owner's model path. [Ballistic impact](/algorithms/ballistic-impact.md)

# Interfaces

A route can request a finer native clock around the first predicted lift encounter. The resulting report records the predicted body slots, the first nonpositive native witness and the extra intervals spent. A second route field can spend additional solves as the joints take the load during the next tick. [Lift contact clock](../algorithms/lift-contact-clock.md) · [Load transfer](../algorithms/lift-contact-response.md)

Measure the projected centre of mass against the upward contacts from a numbered substep. [Support footprint](../algorithms/articulated-support.md)

A diagnostic can call `Simulation::step_with_substeps` to borrow every numbered substep’s end poses and its contact records. The solver keeps the configured public clock, including the native intervals that it splits internally at an impact. [Pedestrian measurements](/oracles/pedestrian-impact.md)

`Simulation::step_with_impulses` advances the same tick and fills a reusable buffer with measured reactions. A solver point carries its member, surface, normal and normal impulse. A momentum balance can span several contacting members or surfaces and retains their candidate list. [Impulse attribution](/algorithms/contact-impulses.md)

`Simulation::new` creates a capsule. `Simulation::from_parts` takes a rig. Both take the selected engine, the fixed meshes and any dynamic boxes. `step` advances one tick. An articulated actor gets at least four collision substeps per public tick and a minimum internal rate of 240 Hz. At 20 Hz it advances twelve steps and at 60 Hz it advances four, giving both clocks the same 1/240 s physical step. A single-part actor against fixed meshes integrates once per public tick. With freely moving boxes, the same actor integrates in whole substeps at 120 Hz or faster. This resolves the moving contact point that can introduce torque in a centered impact. The caller reads the final pose per part and per box, and the manifolds from each numbered substep. Contact records carry the actor part, the touched surface, the point on that surface, its outward normal and the signed separation. Negative separation means penetration. A solver can also report a positive gap during collision prediction. Sleeping bodies can stop producing notifications.

Rapier also reports the triangle ordinal of a touched input mesh. It selects the subshape on the mesh side of the manifold. Welding equal vertex positions preserves triangle order. A dynamic cuboid has no mesh triangle. The Jolt adapter currently exposes the other contact fields while leaving the source triangle unknown, because the native subshape path does not identify the input face in that adapter. The resident city trace records these known ordinals in version 3. The earlier two trace versions still replay using their original contact fields. [Native contact tests](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/tests/contacts.rs) · [District and resident replay checks](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-source-faces.py).


# Invariants

Mesh indices and positive shape dimensions are checked at creation. Each joint refers to a preceding part. A step rejects non-finite poses or contacts. In each contact record, static meshes come first in the surface table and dynamic boxes follow in input order. Both adapters orient the normal towards the actor. Reporting order is stable for replay on the same tested build.

# Knowledge

Rapier reuses contact geometry when every limb and dynamic support remains asleep across a native step. [Sleeping rigs and terrain edits](/algorithms/resting-contact-witnesses.md)

[Settling a hanging ragdoll branch](/algorithms/hanging-ragdoll-branches.md) describes how the Rapier adapter follows contacts up the tree and applies angular friction down a suspended limb.

[Sharing fixed collision shapes away from a moving body](/algorithms/native-static-instances.md) covers native mesh storage, exact contact coordinates and the measured cost of repeated placements.

The native tests drop a capsule at 55 m/s onto a thin upper deck, check a contact against the equation of a sloping plane, bend a joined pair on impact and measure momentum transferred to an offset box in an isolated collision. The last test requires linear momentum drift below 1% and a rotating box. [Contact tests](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/tests/contacts.rs)

# Reuse

A world owns the rig and the meaning of a collision. It can consume contact records for effects or replay, render meshes at the reported poses, or run the same scenario in a headless test. Run `cargo test -p osage-dynamics` for the native examples.

# Limits

The tested native configuration uses one actor, resident scene meshes and a free box or a prescribed vertical lift. The Rapier terrain adapter also accepts static meshes that arrive or leave while a rig is moving. District receipts can map each stable contact surface index to its authored node and path. [Streamed terrain and ragdolls](/algorithms/paged-ragdolls.md) Joint methods and contact margins differ between solvers. A given engine and build can be replayed separately. Cross-platform bitwise equivalence needs its own evidence. Muscular control, recovery clips, tyre forces and chassis deformation belong to further experiments. Jolt currently uses the native adapter in Osage.
