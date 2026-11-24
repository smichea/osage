---
type: Algorithm
title: Load transfer at first contact with a lift
description: Chooses native solver intervals from contact velocity and rig momentum for one tick after first impact.
resource: https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/lift_response.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [physics, collision, replay]
sources:
  - id: implementation
    title: Load transfer on a moving support
    resource: https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/lift_response.rs
generated: { by: "OpenAI/Codex", at: "2026-11-24T17:24:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A paw can stop at the deck while the trunk still carries downward momentum. The first reported contact tells the solver when the impact began. It gives little information about how much further the rig is travelling into the deck as the joints take the load.

# Idea

Estimate normal travel from the rig's mass-weighted velocity and the local velocity at each contacting point. Use the larger closing speed to choose the number of native solves in the following base interval. Limit the work to the first impact.

# Method

After each solve in the loading window, retain the nonpositive contacts on the registered lift. At the next ordinary substep, calculate the velocity of each material point as linear velocity plus angular velocity crossed with its offset from the corresponding body centre. Project the lift velocity minus the limb velocity, and the lift velocity minus the centre-of-mass velocity, onto the witness normal. Take the largest positive projection across all contacts.

Multiply this speed by the base interval, divide by the requested travel and round up. Clamp the resulting count between one and the number of divisions permitted by the local clock. Advance every body and the prescribed lift through each division. The window covers the next replay tick's worth of ordinary substeps after the base substep containing the first manifold. The substep already in progress finishes on its [approach clock](lift-contact-clock.md).

At each native step Rapier checks whether a limb can approach the intersection of two faces of the finite lift. It expands the limb bounds by relative translation during the step, the rotation arc estimate, gravity travel and the native manifold recycling distance. Overlap with the three box slabs and proximity to at least two face planes activates manifold refresh on the lift collider. Rapier refreshes pairs carrying an active contact hook. Its no-op modification hook leaves the calculated witnesses and impulses to the solver. A passenger wholly inside a broad top face can use the ordinary cached contacts.

# Invariants

Numbered substeps retain their elapsed time and all normal impulses come from the selected solver. An empty set of nonpositive lift contacts requests one solve. A response record lists runs of consecutive base substeps with equal subdivision counts. Multiplying each run length by its number of extra divisions and summing recovers the reported extra cost.

# Cost

In the loading window the estimate takes linear time in the numbers of bodies and retained contacts. For four ordinary substeps per tick and eight allowed divisions per substep, response adds at most 28 solves per first impact. The recorded count excludes the approach window.

# Failure modes

Normal travel approximates compression. It also captures some rotation about a supported paw. Refining the time grid changes constraint coupling and can increase the peak depth or rebound on a particular trajectory. A geometric penetration limit and a guaranteed settling time require separate measurements. Later sliding and secondary impacts lie outside this first-impact window.

# Alternatives

A fixed clock is simpler. A finer clock throughout a long slide also spends work while the unsupported part of the rig is already falling beyond the platform.

# Oracle

A stationary-paw control checks the contribution of incoming trunk mass. The integration test combines the approach and load-transfer clocks and bounds penetration and joint excursions over the first quarter-second.[^implementation] [Loaded landing test](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/tests/loaded_landing.rs).

# Reuse

Call `Simulation::sample_lift_response` after planning an approach and inspect the extra intervals in the approach report.

[^implementation]: [Implementation and unit tests](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/lift_response.rs).
