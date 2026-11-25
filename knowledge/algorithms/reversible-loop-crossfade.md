---
type: Algorithm
title: Retaining clip time through a reversed crossfade
description: Returns a fading loop to the foreground while preserving its playback state and letting Bevy manage the weights.
resource: https://github.com/smichea/osage/blob/main/crates/osage-humans/src/loop_transition.rs
licence: MIT OR Apache-2.0
status: draft
tags: [animation, characters, locomotion]
sources:
  - id: bevy
    title: Bevy AnimationTransitions
    resource: https://docs.rs/bevy_animation/0.20.1/bevy_animation/transition/struct.AnimationTransitions.html
  - id: replay
    title: Live walk/run reversal on the Khronos Fox
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/examples/gait_reversal.rs
generated: { by: "OpenAI/Codex", at: "2026-11-25T00:10:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A player switches from walking to running, then asks to walk again while the walk clip still contributes to the pose. Restarting its clock changes the sampled joint rotations immediately. The walk can still carry most of the pose weight when this happens.

# Idea

Keep the outgoing loop's playback state as long as the animation player is still evaluating it. The time to use for a new entry only matters after that loop has actually stopped.

# Method

Read the requested node's active animation before asking Bevy to change the main node. If the main node already matches, keep playing it. Otherwise let the transition manager cancel the node's outgoing fade and start fading out the previous main node. Restore the saved animation, including its current and previous seek times and completion state. For a fresh node use its requested entry time. The caller supplies playback speed. Bevy continues to advance and retire animations.[^bevy]

# Invariants

Repeated requests for the main loop preserve its clock. Retargeting an active loop preserves its playback state at the command boundary. On a two-loop crossfade the replay verifies identical weights with and without this clock policy at each measured tick. Published human moves outside walk, jog and idle retain their existing entry behaviour.

# Cost

One animation-state copy and one lookup per transition on a live loop. No additional animation tracks, pose buffers or per-frame controller system.

# Failure modes

A stopped clip has lost its running clock. Its next entry uses the configured start time. The continuity test isolates the command at zero elapsed time. At nonzero elapsed time, pose velocity can still change when the blend changes direction. Synchronising the gait phases of two freshly started clips is a separate problem.

# Alternatives

Restart on entry for actions that should repeat their beginning. A controller can also finish its first transition before acting on a subsequent request.

# Oracle

The Ville-Paris example reads an array of trials on standard input. Each trial specifies a label, frame interval in seconds, fade duration, starting phase and one Boolean run request per tick. It applies each trial to two Bevy players on the installed Khronos Fox. At a new request, it holds time still, samples both poses and measures the largest displacement of four distal joints. At the end of the tick it measures the actual clip weight. An interrupted human replay uses `pedestrian_stride --planned --mesh-support --curb --quick-return` and records both live clocks.[^replay]

With the manifest’s Fox installed, a 32 Hz command reversal can be replayed from the workspace root.

```sh
printf '%s\n' '[{"label":"quick-return","dt":0.03125,"fade_s":0.25,"phase":0.5,"run":[true,true,true,false,false,false,false]}]' |
  cargo run -q -p osage-ville-paris --example gait_reversal
```

# Reuse

Call `osage_humans::crossfade_loop` with the animation player, its transition manager, the loop node, fade duration and fresh-entry time in seconds. The helper operates on graph nodes and can be used with any skeleton.

[^bevy]: [Bevy animation transitions](https://docs.rs/bevy_animation/0.20.1/bevy_animation/transition/struct.AnimationTransitions.html).
[^replay]: [Native fox reversal replay](https://github.com/smichea/osage/blob/main/worlds/ville-paris/examples/gait_reversal.rs).
