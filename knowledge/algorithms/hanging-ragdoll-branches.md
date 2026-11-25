---
type: Algorithm
title: Settling a hanging ragdoll branch
description: Uses the support contacts and inertia of a joint tree to damp a suspended limb after the body lands.
resource: https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/rapier/hanging.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [physics, collision, ragdoll, rapier]
sources:
  - id: rapier
    title: Rapier joint constraints and motors
    resource: https://rapier.rs/docs/user_guides/rust/joints/
  - id: pendulum
    title: Osage finite-support pendulum fixtures
    resource: https://github.com/smichea/osage/blob/main/crates/osage-dynamics/tests/contacts.rs
generated: { by: "OpenAI/Codex", at: "2026-11-24T07:30:46Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A pedestrian lands with the chest on a bonnet and an arm over the edge. Gravity still swings the arm. With friction applied only at a wrist whose forearm touches a support, all three joints can keep moving for a minute.

# Idea

Use the authored joint tree to find where a freely hanging mass attaches to a supported body. Apply viscous friction to relative rotation once the attachment is quiet and the branch has remained unloaded. Gravity and the joint limits determine its eventual angle.

# Method

1. At construction, transform each segment's mass, centre and inertia to the common bind frame. Visit children before parents and add each subtree's mass properties to its parent. For a joint carrying descendants, let **r** point from its pivot to the subtree's centre of mass. Set **L** to the greater of the length of **r** and the child capsule's half-length including its cap.
2. For each free angular axis **a**, the moment about the pivot is **I = a·Icm·a + M |r × a|²**. A force-based joint motor with damping **2√(I M g L)** opposes relative angular velocity about that axis. This approximates critical damping of a small-angle compound pendulum in its bind configuration. For a terminal segment, retain the acceleration-based rate **√(g/L)** with **L** the greater of its anchor offset and bounding radius. Both motor types target zero relative angular velocity and have zero positional stiffness.[^rapier]
3. At each native physics step, inspect the preceding contact solve. A positive contact impulse resets a segment's unloaded time. A quiet segment can support a branch when its contact reaction has a component opposite gravity. Quietness uses the solver's own sleep-eligibility timer.
4. Walk the tree upwards, taking the shortest unloaded time among each segment and its descendants. Walk downwards from supported segments. A child becomes eligible when its subtree has remained unloaded for the sleep interval and the child is awake. Pass eligibility through fixed joints. Enable the prepared motor only on a qualifying joint's free angular axes and clear it when eligibility ends.

# Invariants

The two tree passes use parent-first indices in opposite orders. A hand pushing against a surface also loads its forearm and upper arm. An unsupported actor in flight never starts the chain. Relative rotation is measured across the joint, so common angular motion creates no motor torque.

# Cost

For N parts, construction and storage are O(N). Each native step costs O(N + P) for the tree walks and the P contact-pair visits. It allocates no temporary buffers. A change in eligibility updates the motor axes in place.

# Failure modes

The inertia estimate uses the bind pose. Folding a long chain can change both its effective moment and its lever arm. The coefficient approximates a passive limb's damping and has no measured anatomical calibration. Intermittent contact can repeatedly delay activation. This mechanism applies to the Rapier adapter.

# Alternatives

Damping just the terminal joint suffices when the forearm rests on the support. It leaves the shoulder free when the entire arm overhangs. Applying world-space drag during flight would also change the character's approach to its first contact.

# Oracle

A three-part fixture places a hinge beyond the edge of a finite deck. Its two hanging segments share a fixed joint. The branch swings freely at release, then approaches the vertical under gravity. The city oracle also measures first contact, overlap and lasting rest for complete pedestrians.[^pendulum] [Pedestrian impact](/oracles/pedestrian-impact.md)

# Reuse

Any passive, parent-first joint tree with mass properties, gravity and contact reactions can use the two passes. The solver adapter supplies the support impulse and the angular-motor model. [Osage dynamics](/modules/osage-dynamics.md)

[^rapier]: [Rapier joints and motors](https://rapier.rs/docs/user_guides/rust/joints/).
[^pendulum]: [Osage contact fixtures](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/tests/contacts.rs).
