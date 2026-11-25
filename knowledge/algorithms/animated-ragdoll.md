---
type: Algorithm
title: Activating a ragdoll from animation
description: Transfers the poses and joint-point velocities of a moving character into its passive rigid-body simulation.
resource: https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/animation.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [physics, animation, ragdoll]
sources:
  - id: activation-test
    title: Osage animated joint transition fixture
    resource: https://github.com/smichea/osage/blob/main/crates/osage-dynamics/tests/contacts.rs
generated: { by: "OpenAI/Codex", at: "2026-11-23T17:58:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A foot can be moving forwards and turning around the ankle when a game releases the character to physics. Starting all parts with the velocity of the actor loses this motion. The body's initial pose can also put a different limb nearest the support.

# Idea

Two samples of each rigid body supply its orientation change. The two bodies connected by a joint share the instantaneous velocity of its anchor. These conditions determine all centre velocities from the velocity of the root body.

# Method

1. Sample each physical body's centre of mass and orientation at the transition and one known interval earlier, in the same actor coordinate system. Associate the samples with the physical body names.
2. Compute the unit quaternion mapping each previous orientation to its current orientation. Choose the equivalent sign that gives the shorter rotation. Divide its rotation vector by the sample interval to obtain the angular velocity in actor axes.
3. Set the root centre velocity to its sampled displacement divided by the interval plus the actor's shared velocity. For each child, rotate the two bind-pose anchor offsets into the current parent and child orientations. The parent anchor velocity is its centre velocity plus its angular velocity crossed with its anchor offset. Subtract the child's angular velocity crossed with its own anchor offset to obtain the child centre velocity.
4. Construct the constraints in the authored bind pose. Set the native bodies to the sampled centres and orientations, add the actor origin to their positions and install the velocities. Leave the solver in charge from its first ordinary step.

# Invariants

The supplied rig is parent-first. Each body has exactly one pair of matching samples. The sample interval is finite and positive. The orientations are unit quaternions. The assigned velocities at each joint anchor agree. Joint limits refer to the authored bind frames throughout the transition.

# Cost

Initialization visits each body once and uses temporary storage proportional to the number of bodies. Subsequent integration uses the ordinary native rigid-body and joint code.

# Failure modes

Two orientations identify rotations up to their shortest arc. Motion through a half-turn or more between observations is ambiguous. A long sampling interval averages rapid changes in direction. An animation that violates the physical rig's anchor positions or angular limits can produce a constraint adjustment when physics takes over. Binding or retargeting an animation clip to the collision rig belongs to the caller.

# Alternatives

Freezing the current pose preserves the silhouette while losing the angular velocities. Differencing every centre individually approximates a chord. During rapid rotation the resulting velocities can disagree at a shared joint anchor.

# Oracle

A two-body hinge starts at a nonzero angle. Its child rotates about the anchor and the parent translates. The test supplies opposite equivalent quaternion signs and unordered named samples. It checks the child centre velocity, angular velocity and position at activation, the continued change in hinge angle and the total free-flight linear momentum in both solvers.[^activation-test]

# Reuse

Supply an animation handoff in `RigStart` to `Simulation::from_rig`. The optional replay field `ragdoll_activation` accepts the same data on the city's articulated actor path. [Dynamics module](/modules/osage-dynamics.md)

[^activation-test]: [Animated rigid-body contact fixtures](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/tests/contacts.rs).
