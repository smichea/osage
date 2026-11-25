---
type: Algorithm
title: Reusing contact geometry on a sleeping rig
description: Retains observed contact geometry while all moving bodies of an articulated Rapier scene remain asleep.
resource: https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/rapier.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [physics, collision, ragdoll, rapier]
sources:
  - id: wake
    title: Osage regression for a sleeping rig whose supporting page is replaced
    resource: https://github.com/smichea/osage/blob/main/crates/osage-dynamics/tests/resting_contacts.rs
generated: { by: "OpenAI/Codex", at: "2026-11-24T10:00:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A motionless pedestrian resting on a bonnet still publishes the contact points on its limbs at each physics substep. Reconstructing those world points from the same manifolds and testing how deeply the limbs press into the bonnet can cost more than advancing the sleeping island.

# Idea

Rapier tracks whether an island is asleep and processes changes that can wake it. The adapter can reuse the last contact geometry when the complete articulated scene sleeps on both sides of an ordinary native step.

# Method

1. Read the fall speed of each body. Acquire mutable access only when applying a terminal-speed correction. A mutable lookup alone registers a change in the native body set.
2. Check the native sleep flags for every limb and dynamic box. A single-body actor or a scene with a kinematic platform takes the ordinary observation path. Clear the stored observations when sleep is lost, the force-event mode changes or the streamed world is edited.
3. Advance Rapier on every substep, with the requested event handler. If all the bodies were asleep before and remain asleep afterwards, save the freshly observed contacts or copy those already saved. Stamp each copied observation with the current substep. Read the current poses from Rapier.
4. After building the cache, evaluate the supported box's stiffness on the refreshed contact graph. Reuse that decision while the cache remains valid and the substep duration matches.

# Invariants

The native pipeline owns the simulation clock, impulses, sleeping and waking. The stored data contains world points, normals, separations and the original limb, surface and triangle identities. Each reporting interval receives its own substep numbers. Invalidating on terrain edits prevents a recycled collider slot from inheriting the departed surface's contacts.[^wake]

# Cost

Both sleep checks visit at most all the moving bodies. Keeping and copying the witnesses requires space and time linear in the contacts of one substep. On the central parked replay, nine paired runs measured median process CPU costs of 98.1 and 24.2 µs per public tick before and after this change during seconds 20–60. [City measurements](/oracles/pedestrian-impact.md)

# Failure modes

Low linear and angular speeds alone cannot establish eligibility. A rig balancing on an edge can start moving again. An ordinary step also processes a removed support or an approaching platform while the actor is asleep. Every future API capable of editing contact geometry or material during sleep needs to invalidate the stored observations.

# Alternatives

Rebuilding the observations every substep gives a reference trace. Skipping the native update on a quiet frame can report a departed support and delay waking its passengers. The page-replacement regression distinguishes the two behaviours.[^wake]

# Oracle

Remove a balcony supporting a two-part articulated body and reuse its collider slot for a distant page. Check the first falling tick, the new supporting surface and the impulse records. Also compare the normal replay byte for byte before and after the optimization. [Public physics interface](/modules/osage-dynamics.md)

# Reuse

The adapter operates on the authored bodies of a joint tree and the fixed collision pages of its scene. It accepts the native sleep flags for both the rig and its dynamic supports. [Streamed terrain contacts](/algorithms/paged-ragdolls.md)

[^wake]: [Sleep, terrain slot reuse and contact regression](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/tests/resting_contacts.rs).
