---
type: Algorithm
title: Articulated swing envelope
description: Bounds tilt in the joint frame by two principal amplitudes while keeping a separate twist interval.
resource: https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/rapier/swing.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [geometry, physics, replay]
sources:
  - id: loaded
    title: Loaded twist and tilt on a passive quadruped
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-loaded-twist.py
  - id: oracle
    title: Two-body compound pose replay
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-swing-envelope.py
generated: { by: "OpenAI/Codex", at: "2026-11-24T18:00:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

Two angular ranges specify the extreme tilts on the principal axes. They leave the allowed diagonal tilt ambiguous.

# Idea

An object can give its joint an elliptical boundary in angular coordinates. The orientation of the child X axis determines tilt. A rotation around that axis also carries twist, which has its own interval.

# Method

Compose the two current body orientations with their respective bind frames. In the parent joint frame let d be the resulting direction of child X. Its tilt is atan2(length(d.yz), d.x). For a tilt around unit axis (0, y, z) with positive half-widths Y and Z, the allowed angle is 1 / hypot(y/Y, z/Z).

Rapier constrains the two swing axes with a circular stop of radius max(Y, Z). For unequal widths an auxiliary unilateral angular row additionally follows the gradient of tilt minus allowed angle. Its normal is that gradient divided by its length. The solver receives the excess divided by the same length, capped at a quarter turn per correction. The main joint carries translation and twist. Jolt uses the native six-degree-of-freedom cone, whose ellipse is evaluated in quaternion half-angle coordinates.

For an unequal pair of widths, Rapier also retains fixed angular constraints between the child X axis and directions in the parent YZ plane. Let the angular coordinates of the swing be (u, v) and its magnitude be r. The lateral coordinates of the swung X axis are (v, −u) times sin(r)/r. For a unit transverse direction h and half-widths Y and Z, the absolute lateral projection is bounded by hypot(Z h.y, Y h.z) times sin(m)/m, with m = min(Y, Z). For r below m, the circular core bounds the projection by sin(m). For larger r, sin(r)/r decreases. Each bound becomes a two-sided angular stop around 90°. Two directions follow the principal axes. During initial free motion two more follow their ordinary diagonals. After the rig receives a normal contact impulse, the oblique directions become the normals of the transformed unit disk. A disk normal (a, b) becomes the unit vector parallel to (a/Z, b/Y) in the transverse YZ plane. Both pairs contain every admissible pose. The contact pair reduces free angular travel between fixed projections in oblique directions of a narrow ellipse. It stays selected through loss of support until the actor sleeps or is given new starting poses. Changing the directions immediately on leaving a ledge can increase peak tilt during departure. The alternative rows are prebuilt and at most four fixed rows are active. A projection bound of one spans the whole axis and needs no constraint. If m is at least 90°, the core reaches every lateral direction and these bounds add nothing. The exact directional test still handles the space between the fixed stops. A backwards-facing axis can have a small lateral projection and must cross the equator on its way forward. The adapter disables the projection stops on that hemisphere. The circular and directional constraints continue to act while the axis points backwards.

For signed twist t = 2 atan2(q.x, q.w), its derivative with respect to relative angular velocity is a vector g = (a + b) / (1 + a · b), where a and b are the parent and child twist directions in world space. Rapier applies a single-axis impulse on its parent X direction. Before each native step the adapter rotates both local constraint frames so X follows the bisector of a and b. The two new frames differ by exactly t about X. The anchor locations stay on their original bodies. The circular stop and the anisotropic row still measure tilt from the original endpoint frames.

A loaded joint can accrue error when a contact impulse must travel through several links. At the start of a native step Rapier's adapter checks twist and tilt against a second envelope with half-widths scaled by 17/16. For a circle, squared transverse quaternion length tests the tilt threshold. An anisotropic joint reuses its measured directional radius. Two precomputed tangent thresholds test signed twist. A zero twist half-range takes its margin from the other side, or from the smaller transverse width when twist is locked. On a breach the child requests three extra projected Gauss–Seidel passes within that solve. Inside the band it releases the request. These are repeated impulse updates at the same native time.

For unequal tilt widths, a normal contact impulse on any limb in the preceding native step indicates a load on the connected rig. Under this condition, the twist refinement margin is at most one sixteenth of the narrowest tilt half-width. Successive native steps outside a twist or tilt margin request 3, 6, 12 and then at most 24 extra impulse passes. Each child tracks its own request. Re-entering both margins clears it. A step without a preceding contact reaction returns to the ordinary refinement rule. Rapier supplies the normal impulse on the contact pair after solving. Merely reporting a witness across a positive gap does not establish a reaction.

A circular cone under measured contact can load its rim and twist stop together. At fifteen sixteenths of its tilt half-width and beyond the signed twist limit, request three extra passes. Consecutive circular-rim contacts keep that request at three. The compound request drops on release of either stop, or on loss of contact. A cone extending to 180 degrees has no rim.

# Invariants

Rotating around child X preserves d. When Y equals Z the allowed angle is independent of azimuth. All native impulses act on both bodies. An omitted envelope preserves the object's previous interpretation.

# Cost

Constraint storage and work per native step are linear in the number of marked joints. Rapier adds a circular stop for each and a directional row and a bounded number of fixed angular stops for each unequal pair. Jolt changes the swing mode on its existing joint. For Rapier, the largest extra-pass request applies to the awake connected component, including other bodies coupled through contacts. Costs depend on how long the component exceeds the band and how long it sustains external contact. Only rigs with unequal-width envelopes query the previous contact impulses.

# Failure modes

A symmetric, positive interval is required on each transverse axis. The direction of swing is ambiguous at a half-turn. Rapier chooses a consistent axis from the relative orientation there. A finite solve can take several frames to return an almost inverted link to range. At opposing twist directions the bisector is undefined. The twist constraint uses the original bind frames when the cosine of half the swing is at most 0.00001. A finite impulse solve can leave residual error under load. For unequal widths Jolt's half-angle cone approximates the authored angular ellipse.

# Alternatives

The independent-axis mode accepts signed asymmetric intervals. A hinge needs only its signed rotation interval.

# Oracle

The replay starts two bodies sharing a pivot from rest in 300 poses, with variable tilt amplitude, direction and signed twist. It compares first-frame orientations against a free-joint run on the same backend and measures the remaining angular error after 200 ms. It verifies deterministic repeats, checks for external contacts, and requires less than 0.05° residual excess after 200 ms on both marked backends.[^oracle]

A second replay drops a sixteen-part actor with 30° stops onto a broad platform and a rear-paw ledge, slides it off a platform, and runs a free fall. It checks the maximum excess among all fifteen joints at each of 120 frames. The two-second mean and peak catch a twist drift that can arise after the last external contact. Repeating the test with narrow stops and with pads under only the left or right rear paw exercises impulse propagation under asymmetric load. A third rig keeps ±45° twist and gives the two tilt axes ±20° and ±5°. The replay checks the converted limits on every joint, samples five sliding speeds, and bounds the mean and peak across all fifteen joints. A paired near miss checks for spurious impulses. The zero-speed drop checks a peak below 2.4° both at the centre of the small pad and five centimetres to one side. The ledge departure checks the same threshold. A 4 m/s slide with 2° stops repeats with zero deck friction. For both drops the oracle records the peak joint excess, deepest sampled overlap and maximum signed vertical centre-of-mass velocity relative to the deck. The latter two use a quarter-second window from the first touching or penetrating deck witness. A negative maximum means that the centre of mass was still descending at every sampled instant in that window.[^loaded]

# Reuse

Use `--swing-envelope angular-ellipse` when converting a fragment that uses this convention. Inspect the portable joint's `swingEnvelope` and its three intervals before consuming the rig.

[^oracle]: [Two-body compound pose replay](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-swing-envelope.py).

[^loaded]: [Loaded twist replay](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-loaded-twist.py).
