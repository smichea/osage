---
type: Algorithm
title: Loading the geometry of a falling body's impact sound
description: Uses a predicted contact to reserve acoustic meshes and a solved contact to mix the direct and diffracted sound at the moving receiver.
resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/impact.rs
licence: MIT OR Apache-2.0
status: draft
tags: [audio, collision, geometry, streaming, paris]
sources:
  - id: segments
    title: Selection of acoustic resources by finite segment
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/channels.rs
  - id: playback
    title: City material voice and stereo receiver
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/impact/mod.rs
  - id: clock
    title: Solved emission frame and publication of a paged path
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/impact/paged.rs
  - id: prediction
    title: Ballistic reservation of support beneath the capsule
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/planning.rs
  - id: moving
    title: Acoustic reservation for two mobile shapes
    resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/planning/moving.rs
generated: { by: "OpenAI/Codex", at: "2026-11-24T07:09:38Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A player can hear an impact through a wall or around an opening. An initial line of sight can cross just one panel while the path around its edge meets another panel in a different file. At 60 simulation steps per second, even a short file delay can exceed the flight time of sound across the room.

# Idea

A falling capsule predicts a possible point of contact on nearby support. This lets the city request the geometry between that point and the camera before the landing. Once the solver measures a reaction, the actual point, material and emission time determine what reaches each ear.[^prediction]

# Method

The ballistic query uses the existing support geometry. A legacy district with no support triangles can predict against a single named horizontal layer. The forecast mode holds matching placements while the predicted source and receiver stay within 2.5 cm on each coordinate. The solved contact always gets its own finite-segment query at the camera's expected time of reception. Physics and the receiver clock continue while file requests are pending.[^segments]

A read verifies the content before placing the triangles on their original nodes. One decoded file can serve several differently transformed placements. A reflection of the coordinate axes reverses the order of the face indices to preserve the outward side of the surface. The direct sound becomes eligible as soon as all bindings on its segment are available. For indirect sound, an actual attenuating crossing admits adjacent blocking surfaces whose bounds touch. This keeps coincident seams and closed solids together. The same edge search as the resident renderer enumerates candidate legs. Each leg can request further bindings. Repeat until the set stops growing, then publish the selected path.

The district and the resident city replay use the same impact track. It constructs the modal voice from `osage-sound-voices` with material parameters from the bridge scene and takes panning or ear cues from `osage-sound`.[^playback] [City contact sound](/algorithms/city-contact-sound.md) · [Sound engine](/modules/osage-sound.md)

A forecast reserves a mesh. A solved contact supplies the source and its emission frame, including the confirming substep when available.[^clock] For simultaneous articulated contacts, the solver measurement time, authored surface name and owning node determine voice order before allocation of the synthesis seed. Within a single material reaction, solver measurements and fallback contact witnesses have a stable order before summation, so swapping the order of the two feet preserves the resulting waveform. The native collider slot can vary with the order of terrain reads. The city maps each completed simulation tick to a 48 kHz stereo frame. The material voice, transmission filters and ear histories advance to that boundary. When verified geometry arrives after the wavefront, advance the new path through those same recorded frame boundaries and discard its elapsed samples. Mix the remaining portion into future samples. Drop a path whose entire voice has elapsed. An acoustic prediction can read files even if the body subsequently misses the surface. The general sound engine advances its source delay lines in 128-frame blocks and assigns its own footsteps and wheel strikes to a bounded voice pool. [Sound delay lines](/algorithms/sound-delay-lines.md) · [Contact voices](/algorithms/contact-voices.md)

A moving support contributes its translation and angular velocity at the point of contact. For prefetching, cast each oriented limb capsule against the oriented box over successive short intervals. Both centers follow their estimated vertical acceleration, and their orientations advance with angular velocity. The last solved vertical velocity change estimates whether the box is falling or supported. Keep the earliest predicted tick and all limbs reaching it. The interval budget is 32 per limb and tick. Changes of acceleration, sliding or another collision can move the future contact before the next forecast.[^moving]

# Invariants

Only a solved reaction starts an impact. The receipt retains its physical surface, node path, speed and emission time. A late direct or indirect publication preserves that time. Already written samples are final. A file required for a path must pass the length, digest, mesh and placement checks. A rejected path leaves an explicit receipt. A successfully published direct path can remain audible during a subsequent failure on an indirect leg.

# Cost

The index uses file metadata before reading any acoustic vertices. Constructing candidate connected components uses an x-axis sweep over blocking surface bounds. Dense overlapping bounds can give quadratic comparisons and conservative extra reads. For a single event, each discovery round either adds a previously unseen binding or reaches the final answer. Decoded meshes can be shared between simultaneous forecasts and contacts and are released when demand ends. During a forecast-enabled articulated fall, a verified binding used for a physical voice can stay resident until the audible paths of that voice end. Successive limbs can then reuse it. Fetching real HTTP content uses the district workers. Decoding acoustic mesh data and preparing the selected geometry currently consume time on the simulation thread. Late publication can require synthesis for the remaining active voice's elapsed prefix.

# Failure modes

A forecast over coarse support can request the wrong files. A change in listener motion can move a bent leg outside the prefetched set. If the replay ends while a required read is pending, its receipt records the unfinished path. The authored district model describes static geometry during propagation. The impact renderer considers one best indirect path with up to two bends. For articulated actors, the district tests the centre and both end points of each limb. It keeps the candidate rays for every sample tied at the earliest predicted simulation tick. Each active ray has a stable body and sample index across forecasts. Mesh reads are shared by their content slot. A later change of pose or a new earliest tick releases obsolete requests. Two feet can therefore reserve different acoustic panels before their simultaneous contact. Solved impulses on the same surface drive the same material voice as in the resident replay. A path available at the physical substep can produce samples within that simulation tick. A later read can supply only the remaining signal. [Articulated terrain contacts](paged-ragdolls.md).

# Alternatives

The contact mode submits its first read when the solve confirms the impact. The fully resident replay provides a reference for a missing prefix or an incorrectly timed tail. A direct-only run isolates the transmitted wave.

# Oracle

Render the same geometry, fall and camera with the monolithic Rapier capsule and the cooked district. Compare the physical witness, first nonzero stereo frame and PCM samples. Delay district reads past the wavefront, then compare the suffix from the publication frame with the resident recording. A distant damaged file and a damaged required file exercise different paths through the reader. On the west slab with an offset opening, a 50 ms request delay brings the five files in before tick 12. The solver confirms the impact at tick 39 and the 96,000-frame stereo WAV matches the resident recording exactly. Contact-only loading with one service tick per file preserves all 779 nonzero stereo frames remaining at publication. [Integration oracle](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-paged-impact.py).

On two floor patches separated by 4 cm, the right foot strikes asphalt and the left foot strikes steel at audio frame 30,800. Each direct segment to the receiver crosses its own panel. The oracle varies the arrival order of the ground, the acoustic service delay, a post-impact support replacement, direct and diffracted listening, single-panel rejection, and cold and warm HTTP delivery. Twelve prefetched recordings match the complete resident PCM sample for sample. The two contact-only recordings join the correct tail after publication. [Two-foot oracle](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-simultaneous-impacts.py).

A panel between a freely moving box and the listener tests readiness for a moving impact. Two box velocities, a single capsule and an articulated actor exercise material selection, closing speed, source timing and complete stereo recordings. Separate controls delay the panel until contact or let the box leave the falling actor's path. [Moving-support audio comparison](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-paged-bonnet-audio.py).

Each immutable placed acoustic binding retains its triangle search tree once a sound path first reaches its world bounds. Multiple active paths can reuse this tree while the binding remains resident. [Resident acoustic surface index](acoustic-surface-index.md).

# Reuse

Pass `--impact-wav fall.wav` to the ordinary `ville-paris --paging` replay. Use `--listening headphones` for the two ear signals. The receipt under `impact_audio` reports the requested and ready ticks, individual path publications, number of frames, and first nonzero PCM frame. Use `--acoustic-plan contact --acoustic-delay-ms 50` to delay acoustic reads independently of the solver.

[^segments]: [Acoustic binding selection](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/channels.rs).
[^playback]: [Material voice and receiver clock](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/impact/mod.rs).
[^clock]: [Solved emission and path publication](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/impact/paged.rs).
[^prediction]: [Capsule support query](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/planning.rs).

[^moving]: [Predicting a contact between moving shapes](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/paging/planning/moving.rs).
