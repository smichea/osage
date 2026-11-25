---
type: Algorithm
title: First impact of an articulated body on a supported box
description: Resolves the first crossing of a supported box and limits how far an articulated load can travel between subsequent contact solves.
resource: https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/rapier/supported_impact.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [physics, collision, ragdoll, rapier]
sources:
  - id: nonlinear
    title: Parry 0.31.1 nonlinear cast of two moving shapes
    resource: https://docs.rs/parry3d/0.31.1/parry3d/query/fn.cast_shapes_nonlinear.html
  - id: city
    title: Osage pedestrian and movable bonnet experiment
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/compare-impact.py
generated: { by: "OpenAI/Codex", at: "2026-11-24T07:57:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A pedestrian's feet can cross the bonnet of a parked car between two collision solves. By the time the solver responds, it must remove the incoming velocity while also correcting the overlap. The pedestrian and the box can acquire an unwanted separating velocity even when their materials specify zero restitution.

# Idea

A box supported by the road and a pedestrian falling onto it accelerate differently. Forecast their first inward crossing within the upcoming step. Let the native solver advance the joints, the box and its road contacts to that instant, then spend the remaining fraction of the step resolving the impact.

# Method

1. Check whether a dynamic box carries an upward impulse from fixed geometry. Reject actors already in contact and ordinary resting or separating pairs. Bound the travel of each shape by its translation and angular speed times its radius. Use the enlarged bounds to discard distant segments.
2. Cast the box and every nearby limb over the native interval with their current angular velocities. Use midpoint free-fall velocity for the limb and the box's current velocity under support. At a prospective hit reconstruct the two point velocities, including rotation. Use the capsule spine and oriented box to check the signed gap at the proposed instant. Refine the candidate from its normal closing speed if needed. A positive gap just before the candidate and a negative gap just after it bracket an inward crossing. Keep the earliest qualifying inward crossing.[^nonlinear]
3. Divide the current collision substep at that time. Advance the complete native world on both sides. During the second fraction the solver spends its articulated iteration budget relaxing velocities after the biased contact solve. Restore the usual stabilization count immediately afterwards.
4. After impact, keep the face normal in the box frame. Check that the box still receives an upward reaction from fixed terrain and that an inelastic limb receives a positive impulse along that face normal. For each segment take its velocity relative to the box velocity **at the segment’s mass centre**. Sum the inward normal momentum and divide by total rig mass. This includes the movement of the trunk after the feet have slowed, and accounts for support rotation.
5. Multiply the inward speed by the collision interval. While travel per slice exceeds the solver’s allowed linear error, halve the time slice up to the existing CCD budget (four for an articulated rig). Each slice advances the complete native world with current contacts and joint constraints. The public observer receives one pose per configured substep. Native force events retain the time interval of their own solve. Once a reacting face carries the rig with inward travel within the tolerance, close this compression episode. Rolling and a later change in limb posture use the normal solver schedule. Clearing the slab and making a new airborne impact can start another episode.

# Invariants

The two fractions sum to one native substep. Both solids keep their mass, inertia, friction and rotational degrees of freedom. The physical step supplies the contact points, velocities and impulses. The normal of the first impact follows the box in its own coordinates. A subsequent airborne encounter becomes eligible after every unloaded segment clears the same side of the slab bounded by the two box support planes along that normal, by at least the native prediction margin. Sliding over the edge can increase the distance to the finite box while the rig is still descending through the same plane.

# Cost

The broad-phase scan visits the N segments of the joint tree. A cast is needed only for shapes close enough to cross within the next substep. A qualified onset costs one additional native advance and extra constraint iterations on the remainder. After onset, testing load transfer visits the contacts on the box. A loaded face with a fixed support requires one mass-weighted sum over the rig. Qualifying compression costs two or four native advances. A sleeping root skips the calculation.

# Failure modes

The support can accelerate or turn during the interval. Its forecast uses the velocities available at the beginning. The initial scope is a single dynamic box, a freely approaching joint tree and an inelastic encounter. Active contacts on other surfaces, a terminal-speed clamp, a crossing too close to a step boundary or a positive restitution on the colliding shapes can leave the normal native step in charge. A discrete manifold can be speculative. Its first report and the first measured overlap have distinct times.

The travel bound is an estimate from the whole rig. A distal limb can rotate inward while the centre of mass moves outward. Refinement also changes the posture after impact and can lengthen the time before sleep. Both the peak overlap and the eventual rest require replay measurements.

# Alternatives

The relative motion of a freely translating pair sharing the same gravity admits a simpler forecast. A road-supported box needs its own trajectory. Increasing solver iterations on every frame would also spend that budget while the actor is far away or asleep. [Freely translating rigid pair](/algorithms/free-pair-impact.md)

# Oracle

The city captures the native states around the first reported contact and compares them with the ordinary replay.[^city] It measures separation and relative normal speed at the actual closest capsule and box surfaces, alongside the solver's reported points. A leaning two-link fixture samples eight phases of approach and a positive-restitution material. A rotating control clears the plate and strikes it again with the other link. Separate checks follow the deepest penetration and the final rest of the whole articulated body. [Pedestrian impact](/oracles/pedestrian-impact.md)

# Reuse

Use a supported rigid body's contact reactions and the authored collision shapes of an approaching joint tree. Both are available in the [dynamics interface](/modules/osage-dynamics.md).

[^nonlinear]: [Nonlinear shape casting in Parry](https://docs.rs/parry3d/0.31.1/parry3d/query/fn.cast_shapes_nonlinear.html).

[^city]: [City impact comparator](https://github.com/smichea/osage/blob/main/worlds/ville-paris/compare-impact.py).
