---
type: Algorithm
title: Sound from city contacts
description: Turns a physical impact and a moving camera into a timed stereo signal.
resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/impact/mod.rs
licence: MIT OR Apache-2.0
status: draft
tags: [paris, sound, physics]
sources:
  - id: voices
    title: Modal impact voice
    resource: https://github.com/smichea/osage/blob/main/crates/osage-sound-voices/src/contact.rs
  - id: propagation
    title: Speaker panning and headphone cues at reception
    resource: https://github.com/smichea/osage/blob/main/crates/osage-sound/src/ears.rs
generated: { by: "OpenAI/Codex", at: "2026-11-24T07:09:38Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A solver can report several contact points on both feet as a pedestrian lands on a car. It can also return one point on every tick while the body rests. The observer in a replay can keep moving before the first sound reaches the camera.

# Idea

Use the first supported impact of each contact episode to excite a material voice. Timestamp the excitation on the simulation clock. Determine when its direct wave reaches the moving observer and feed that signal to both the interactive and file outputs. [^voices] [^propagation]

# Method

The point control interpolates the foot crossing within its step. A single rigid capsule uses a confirmed swept time when available, or the substep of the first touching manifold. A single rigid capsule can also transfer momentum across a small positive gap. In that case one surface must account for the observed reduction in normal approach speed, the gap must be reachable within one substep, and the event is dated at the end of the confirming public tick. Incoming speed includes angular motion at the point and movement of the struck box. Each new hit needs more than 0.5 m/s of closing speed. For an articulated actor the renderer waits for a measured external reaction above 0.1 N·s and dates the emission at the end of that integration interval. Simultaneous reacting feet on the same surface excite one material voice, whose receipt preserves each measured contribution. When only an aggregate impulse is available, the receipt lists the candidate feet and the rendered source uses the mean of their surface witnesses. A reaction that spans several supports has no unique material voice. [Physical attribution](/algorithms/contact-impulses.md)

An object supplies its collision material key and node path. In the older bridge fixture the road selects asphalt and the rail deck and bonnet select a steel plate. Unrecognised keys use the default impact resonator in `osage-sound-voices`. Point and single-capsule controls map closing speed to a bounded strike strength. For articulated impacts, the initial measured group excites a single material voice. Its positive impulse contributions accumulate for 4 ms on the same surface and participating limbs. If the solver reports one balance for two feet, the renderer counts that vector once. A balance on one surface with several possible normals retains its net-vector magnitude and the receipt labels it accordingly. A newly reacting limb outside the group can start its own strike. The material still chooses the resonances and the softer contact hardness.

For cumulative impulse J and incoming normal speed v at the first reaction, the source uses W = J × v / 2 as a mechanical work proxy. Its normalized amplitude is √(W / (W + 25 J)). The 25 J reference sets the synthesis range. The proxy does not measure acoustic energy or absolute sound-pressure level. Each new contribution changes the envelope only from its own sample onward, over a 0.5 ms ramp. The creation time of the voice and all path delays keep the initial reaction time. Each path applies the same envelope at its own arrival. The receipt records its cumulative impulse, gain, source-frame offsets and attribution. [Excitation and tests](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/impact/excitation.rs)

The camera receives the direct sound with amplitude inversely proportional to distance beyond one metre. Speakers use equal-power panning. On each camera segment the reader finds the earliest intersection with the expanding sphere at 343 m/s, including a receiver that enters and exits the wave between two keys. The final camera position holds after the route ends. [Wave arrival](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/impact/arrival.rs)

With `--listening headphones` the receiver uses the structural ear model in `osage-sound` at wave arrival. Five fractional-delay reflections model the outer ear. Each ear receives this signal after its own delay around the head, followed by a one-pole, one-zero filter that models head shadow. The direct path and all five reflections persist across audio blocks in two 64-sample histories per active impact. The receipt gives the shared arrival in whole frames and the additional delay for each ear in fractional frames. The city mixer keeps that path's reception pose and uses linear interpolation for the ear delays. The general sound engine updates its propagation delay as the source and listener move. [Sound delay lines](/algorithms/sound-delay-lines.md) · [City ear rendering](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/impact/listener.rs)

# Invariants

The point and rigid-body replays use the same solvers in the window and the WAV exporter. Repeated solver points cannot generate an extra onset for a continuing contact. Subdividing a measured reaction among simultaneous points leaves the final source gain unchanged. Splitting it over time preserves the timestamps of each increment. A restart clears scheduled arrivals and stops the earlier player. The software player receives its first buffer before attachment to the device mixer.

# Cost

Each impact searches the remaining camera keys until receipt. Direct synthesis advances active voices for the current block at 48 kHz. A WAV uses four bytes per stereo frame. An uncompressed sampled sound bank is unnecessary for these impact voices.

# Failure modes

The two physics engines can predict different first reactions and rebounds. A longer integration window can include the later transfer through the hips and torso. On persistent support it can also include load under gravity. An articulated solver can report separated points before a penetration witness exists. The recorded acoustic event then follows the later confirmed contact. Intervening acoustic materials now filter the direct wave. The city mixes a delayed edge path for a screened impact. Reflections and continuous ambient voices remain separate. [Sound around city obstacles](/algorithms/diffracted-city-contacts.md) [Material transmission](/algorithms/direct-sound-transmission.md) The first nonzero PCM sample and the software mixer are observable endpoints. A physical speaker's buffer and acoustic output require a separate measurement.

# Alternatives

The bridge sound engine generates footsteps and wheel strikes from scene motion. It assigns them to a bounded pool by estimated audibility. Ville-Paris reuses the modal impact synthesizer and the listener cues. The replay excites the struck surface’s modes and uses the softer of the surface and rubber sole to shape the blow. The general engine can also excite the other body’s resonator. The Ville-Paris track dates the strike from the physical contact and uses the same synthesis seed for the direct and selected edge arrivals. [Sound engine](/modules/osage-sound.md) · [Contact voice allocation](/algorithms/contact-voices.md)

# Oracle

A Rust test varies all limb masses and inertias at the same closing speed, then checks both the measured source gain and the received stereo energy on the moving bonnet with Jolt and Rapier. The envelope test splits one impulse four ways and verifies sample-for-sample equality across different audio block boundaries. The city oracle measures a 2 m and a 22 m reception, mirrored cameras, sources directly above and below the listener, a doubling of distance on the right, listeners moving toward and away from an impact, quiet rest, an airborne cutoff, two physics engines, moving and parked bonnets, and two material keys on the same nested object. The integer timing comparison includes quantization to stereo PCM. The headphone controls check that swapping left and right swaps the actual PCM channels, that the measured onset follows the interaural delay and that signals from above and below differ after level normalization. The elevation check describes the signal. Perceptual localization needs a listener test. A Rust test feeds both listening modes to a software device mixer and compares their samples with the WAV, then resets during propagation. [Audio oracle](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-impact-audio.py)

# Reuse

Run `ville-paris --route moving --replay worlds/ville-paris/fixtures/bonnet.json --physics rapier --actor human` to watch the fall with impact sound. Add `--impact-wav moving.wav` to render the impact track with a JSON receipt on stdout. Use `--mute` for a quiet window, or build with `--no-default-features` to run the file renderer on a machine without a sound device library.

[^voices]: [Impact synthesis](https://github.com/smichea/osage/blob/main/crates/osage-sound-voices/src/contact.rs).
[^propagation]: [Direction at the listener](https://github.com/smichea/osage/blob/main/crates/osage-sound/src/ears.rs).
