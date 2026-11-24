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

Advance a rigid-body solver at a fixed clock and report the poses and contacts back to its caller. Rapier and the native Jolt adapter accept the same meshes, masses, joint locations and initial velocities.

# Concepts

The input frame is right-handed with Y up. Distances are in metres, masses in kilograms and angular velocities in radians per second. A pose gives the centre of a part and an XYZW quaternion. An actor can be a capsule or a parent-first tree of capsules and spheres. A moving box has three half-extents, mass, material and initial velocity.

# Architecture

Each backend constructs the terrain and the constrained actor from the common inputs. Rapier uses continuous collision detection and impulse joints. Jolt uses linear casts, hinges and six-degree-of-freedom constraints. Static meshes can supply their own friction and restitution through [osage-objects](/modules/osage-objects.md). [Rapier adapter](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/rapier.rs) · [Jolt adapter](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/jolt.rs)

# Interfaces

Advance a simulation to retrieve limb and box poses and the contact manifolds for each substep. Each contact reports the actor part, touched surface, point, outward normal and signed separation. Rapier also reports the source triangle of a static mesh. The native Jolt adapter leaves that field unknown. [Public types](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/lib.rs)

# Invariants

Mesh indices and positive shape dimensions are checked at creation. Each joint refers to a preceding part. In a contact record, static meshes come first in the surface table and dynamic boxes follow in input order. Both adapters orient the normal towards the actor.

Circular and elliptical swing limits use the joint bind frames. The first approach to a prescribed platform can request a finer local clock. The support footprint reports the signed distance from the projected mass centre to the support boundary. [Angular stops](/algorithms/articulated-swing-envelope.md) · [Approach](/algorithms/lift-contact-clock.md) · [Transfer of load](/algorithms/lift-contact-response.md) · [Support footprint](/algorithms/articulated-support.md)

# Knowledge

The native tests drop a capsule at 55 m/s onto a thin upper deck, compare contact with the equation of a sloping plane and measure the transfer of momentum to an offset box. Further tests check supported impacts, accumulated impulses and sleeping contacts after terrain edits. [Contacts](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/tests/contacts.rs) · [Supported impact](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/tests/supported_impact.rs) · [Sleeping contacts](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/tests/resting_contacts.rs)

# Reuse

A world supplies geometry and owns the meaning of a collision. It can consume contact records for effects or replay and render the reported poses. Run `cargo test -p osage-dynamics` for the native controls.

# Limits

Jolt is available on native targets. Source triangle ordinals on its terrain contacts are currently unavailable. Different solvers and solver versions can produce different contact sequences from the same initial pose.
