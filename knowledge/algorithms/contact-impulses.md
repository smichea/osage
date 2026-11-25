---
type: Algorithm
title: Attributing solved contact impulses
description: Identifies the actor part and the touched surface for each measured reaction, and retains the candidates of an unresolved momentum balance.
resource: https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/impulses.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [physics, collision, measurement]
sources:
  - id: rapier-events
    title: Rapier advanced collision detection and contact force events
    resource: https://rapier.rs/docs/user_guides/rust/advanced_collision_detection/
  - id: jolt-listener
    title: Jolt contact listener timing
    resource: https://jrouwe.github.io/JoltPhysics/class_contact_listener.html
  - id: impulse-tests
    title: Osage independent support and free-body momentum tests
    resource: https://github.com/smichea/osage/blob/main/crates/osage-dynamics/tests/impulses.rs
generated: { by: "OpenAI/Codex", at: "2026-11-23T18:26:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A contact manifold can have a small positive gap throughout a resolved impact. An articulated actor also transfers momentum among its members. A geometric witness or a change in a single limb's velocity therefore leaves the contribution of the touched surface undetermined.

# Idea

Record the impulse as close to the operative solve as the backend permits. Preserve which quantities and participants the measurement actually identifies.

# Method

1. On an observed Rapier tick, enable contact-force events on the actor's colliders. In the native post-solve callback, use each active solver contact's exact manifold point index. Read the accumulated normal impulse in N·s, orient its normal towards the actor, and transform the surface anchor to world coordinates. Capturing inside the callback includes contacts that disappear during a subsequent continuous-collision slice.[^rapier-events]
2. For the isolated inelastic sticking response, retain the full vector applied to the actor, its opposite on the box, the surface witness and the scheduled time of the application. In an oblique impact, tangential friction can make the full vector larger than its normal component. [Isolated impact](/algorithms/free-pair-impact.md)
3. Jolt reports newly created manifolds before resolving their impulses.[^jolt-listener] Read the total actor momentum before and after each native advance. Subtract the integrated gravity on bodies that were active. The terminal-velocity cap is applied before the first sample. With damping disabled and joint forces internal to the actor, the balance measures its external response together with integration roundoff.
4. Collect the distinct part and surface pairs which reported contact in that advance. A unique pair with unchanged sleep state can own the balance. Multiple pairs are returned together with the full vector as unresolved. No point is selected among distinct witnesses. A shared normal remains available for a single pair when all its observations agree.

# Invariants

The actor uses the input order of its parts. Surface indices address the fixed meshes followed by the freely moving boxes. The impulse acts on the actor. Sample times are relative to the current public tick and bound a native advance. Directly applied impulses have a single time. The caller chooses thresholds. Reporting can be enabled and disabled between public ticks.

# Cost

An observed Rapier advance visits the active points of force-producing pairs. A Jolt balance visits the actor's parts twice, then deduplicates the reported part and surface identities. The caller supplies a reusable output vector. An ordinary tick skips the momentum collection and the Rapier callback.

# Failure modes

A Jolt balance can span both feet, two surfaces, or a change of sleep state. The candidate list preserves the ambiguity. Floating-point residuals in free flight can yield an unresolved empty list. Rapier's per-point scalar describes the normal component of the force. A native advance can contain several smaller CCD solves. The time bounds express the available precision. A caller that also needs onset speed, the sound propagation time or a listener position must measure those quantities separately. The city impact voice dates articulated emissions at the interval end, groups simultaneous reactions on their common material surface and retains the full reaction records. For a single support with unresolved limbs it uses a mean of the contact witnesses for spatial playback and reports all candidates. Its short attack counts the measured common reaction once, retains the candidate set for every additional contribution and records when the gain changes. A balance spanning different material surfaces cannot identify a source for this voice. [City contact sound](/algorithms/city-contact-sound.md)

# Alternatives

A predictive collision response can forecast an approximate impulse from an incoming body pair. Independent constraints in the same solver island can change the result. Geometric contacts, swept time and measured reactions can be retained together when comparing integration methods.

# Oracle

A three-part actor lands on either, both or neither of two supports. Removing each support checks the reacting limb and surface. The sum of normal responses on these horizontal surfaces is compared to the actor's independently observed vertical momentum change. A freely moving box supplies an equal-and-opposite momentum check. The same simulation is also advanced with and without observation and its contacts and poses are compared exactly. A sphere simultaneously contacting two slopes checks the single-member, multiple-surface case. A late subscription verifies the force on an active support and the absence of stale impulses from sleeping bodies.[^impulse-tests]

# Reuse

Call `Simulation::step_with_impulses(&mut output)`, then inspect `pair()`, `normal_impulse_ns()` and the measurement variant. The regular geometric contacts remain available on the same `Simulation`. [Dynamics module](/modules/osage-dynamics.md)

[^rapier-events]: [Rapier advanced collision detection](https://rapier.rs/docs/user_guides/rust/advanced_collision_detection/).
[^jolt-listener]: [Jolt ContactListener reference](https://jrouwe.github.io/JoltPhysics/class_contact_listener.html).
[^impulse-tests]: [Attribution and momentum fixtures](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/tests/impulses.rs).
