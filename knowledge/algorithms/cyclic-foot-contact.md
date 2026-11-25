---
type: Algorithm
title: Choosing a footprint over an entire stance interval
description: Uses an authored loop and its nominal travel to place a low foot and prepare its next landing.
resource: https://github.com/smichea/osage/blob/main/crates/osage-humans/src/stance/cycle.rs
licence: MIT OR Apache-2.0
status: draft
tags: [animation, characters, locomotion]
sources:
  - id: wisconsin
    title: Footskate Cleanup for Motion Capture Editing
    resource: https://graphics.cs.wisc.edu/Papers/2002/KSG02/
  - id: johansen
    title: Automated Semi-Procedural Animation for Character Locomotion
    resource: https://runevision.com/thesis/
  - id: replay
    title: Authored quadruped replay
    resource: https://github.com/smichea/osage/blob/main/tools/animation/fox-stride.mjs
generated: { by: "OpenAI/Codex", at: "2026-11-24T23:32:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

Anchoring the visible pad at touchdown can exhaust its correction range before liftoff. The fox has a low front paw whose authored motion spans over 36 cm after adding nominal root motion in Run. Its touchdown position lies close to one end of this interval.[^replay]

# Idea

Choose a footprint using the whole interval. A fixed contact during stance and a continuous transition around liftoff and landing are established animation constraints.[^wisconsin] A looping gait also permits measurement of foot trajectories and stride speed before playback.[^johansen]

# Method

Take equally spaced samples of one contact point relative to the actor. Add the nominal horizontal travel since the start of each cycle. Unwrap the sequence starting at a landing so a low interval crossing phase zero is treated as one step. For each low interval, place the footprint halfway between the extrema on each horizontal axis. Store the difference between that footprint and the point at each sample. During swing, interpolate from the last correction to the next landing correction with a cubic smoothstep.

At runtime, interpolate neighboring entries at the current clip phase. Map the correction to the actor heading and scale and pass the planned point to the existing world-space [foot planting](/algorithms/forefoot-plant.md). This leaves its bounded correction available for residual motion. When a low planned footprint exhausts leg reach, the visual armature can descend by the vertical shortfall, bounded by the requested displacement. The actor root follows its original path. Height and [terrain clearance](/algorithms/skinned-sole-clearance.md) still use the sampled pose and the world's traversable surfaces.

# Invariants

At the sampled low phases, adding the stored correction to the expected contact position yields one horizontal footprint per interval. Clip time, root trajectory and bone lengths remain under their existing owners. Both ends of the table represent adjacent samples of one loop.

# Cost

Building a table takes linear time and stores two floats per sample per limb. Playback reads two adjacent samples per limb. The fox replay uses 180 samples per paw and per clip.

# Failure modes

The band estimates stance from height. The method requires both a swing and a stance in the loop. It assumes the bound table belongs to the playing clip and its nominal travel. Rigs with insufficient reach can still slide. The measured contact point, rotation of the rest of the paw and a supporting mesh edge can follow different trajectories.

# Alternatives

The world-space anchor alone can serve clips without a calibrated cycle. The Wisconsin footskate solver uses prescribed footplant constraints.[^wisconsin] Johansen's locomotion system analyzes leg cycles and blends animated locomotion on uneven terrain.[^johansen]

# Oracle

Run the fox replay with identical trajectories, skinned terrain checks and stance classifications for both variants. For each foot and starting phase compare the 95th percentile of horizontal speed over consecutive low samples, peak speed over the whole cycle, largest penetration at the selected sole faces and gap during stance. Follow the previous frame’s closest skin point into the next pose using the same vertex or interpolated edge position. Its speed can differ from the speed at the rig marker when the foot rolls.[^replay]

The human replay can use the real Bevy clip clock, with the model library available locally. Run `pedestrian_stride --planned --curb --mesh-support --switch` for a 15 cm pavement, two remote movement trajectories, two changes between walk and jog, a transition to idle, then resumption. The CSV records the number of concurrent clips and the optional plan phase. Run `python3 tools/animation/pedestrian-contacts.py --check CAPTURE.csv` on its output. The scorer verifies that phase against the original glTF clip duration and separately reports penetration during blends. [Human replay scorer](https://github.com/smichea/osage/blob/main/tools/animation/pedestrian-contacts.py)

# Reuse

Attach a FootCycle built from actor-local data to each configured ankle. Set FootCyclePhase on the actor from the active animation clock. Remove the phase or replace the table when switching clips.

[^wisconsin]: [University of Wisconsin Graphics Group](https://graphics.cs.wisc.edu/Papers/2002/KSG02/).
[^johansen]: [Rune Skovbo Johansen's locomotion system](https://runevision.com/thesis/).
[^replay]: [Fox replay](https://github.com/smichea/osage/blob/main/tools/animation/fox-stride.mjs).
