---
type: Module
title: osage-sound
description: Sound placed in an object tree and carried to a listener's two ears — emitters, surfaces and touches on the tree's nodes, the ways each sound takes (straight, through, round an edge, mirrored), a delay line per emitter so that distance and Doppler need no rule, ears without a measured head, a reverberation tail — at four qualities a device can afford, rendered offline or played live — on a thread of its own natively, in an AudioWorklet in a browser — with its nodes placed by a game.
resource: https://github.com/smichea/osage/tree/main/crates/osage-sound
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [sound, spatial-audio, acoustics, object-tree]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-24T12:00:00Z" }
---

# Purpose

Hear a scene as the object tree has it: what each node sounds like, what sound meets, what touches what, and where the listener is. The engine is the first batch of osage's sound (D-180): it renders offline to a file, so that its behaviour can be measured and listened to, and plays live for a client (issue #16). The command that renders the Pont de Bir-Hakeim is `tools/paris-soundscape`.

# Concepts

- **Tree:** nodes, each with an identifier, a parent and a placement relative to it — the object format's tree. A source is named by its node's path and placed by a position local to that node: when a parent moves or turns, its sound follows.
- **Emitter:** what a node sounds like by itself — a voice (a generator from [osage-sound-voices](/modules/osage-sound-voices.md)) and its parameters, a position in the node, a gain, a reach (`maxDistanceM`), and optionally a load from other emitters (a viaduct span loaded by the bogies on it). `positional`, or `global` (straight into the ears, panned).
- **Surface:** a flat face of four corners with an acoustic material (for each of three bands, the share it absorbs, lets through, mirrors and scatters) and a collision material. It may reflect, hide, and be struck.
- **Touch:** what a node touches as it goes — feet with a stride, wheels at points — so that a contact sounds as the two materials meeting do.
- **Way:** one route from an emitter to the head: its length, the point it arrives from, and what is left of the pressure in four bands (250 Hz, 1, 4 and 9 kHz).
- **Quality:** what a device can afford — how a line is read, whether reflections are followed, whether the outer ear and the reverberation are made, how many ways at once, how often the scene is looked at.
- **Listening:** headphones (two ears modelled) or loudspeakers (level panned between two channels).

# Architecture

48 000 samples a second, in blocks of 128 (2.7 ms, a Web Audio render quantum). For each block:

1. **Place:** every node is placed once at the block's end; the head is the listener node's point and axes.
2. **Touch:** feet and wheels are walked along; each contact on a surface that strikes may take a voice from a small pool ([contact voices](/algorithms/contact-voices.md)).
3. **Find the ways** of each emitter whose turn it is ([sound ways](/algorithms/sound-ways.md)): the straight one (or through, or round an edge) and one mirrored by each face that reflects, each from where the emitter was when the sound now arriving by it left.
4. **Rank:** the ways are ranked by how loud they arrive; only the loudest the quality affords are rendered. An emitter none of whose ways is rendered sleeps.
5. **Voice:** each voice that is heard makes its block into its own ring (its delay line).
6. **Carry:** each way reads its ring as far back as its length takes, through four bands, the air and the ears, moving evenly from where the last block left it ([delay lines](/algorithms/sound-delay-lines.md)).
7. **Tail:** what faces scatter feeds an eight-line feedback delay network ([feedback delay reverberation](/algorithms/feedback-delay-reverb.md)).

**Live** (`live`), the engine runs on a thread of its own and keeps a set number of blocks made ahead of the audio output (the output's buffer in blocks, and one more), making the next as soon as the output takes one; the output never waits, and a block it finds missing is silence, counted. A game places nodes as it learns where they are (`Engine::drive`, a placement and a velocity): the node goes on at that velocity until it is told again, so that a pose told sixty times a second moves it evenly and what it carries is not heard to lurch. Poses recorded in advance (`Told`) are given at the same blocks live and offline: the same replay gives the same samples. With the `bevy` feature, `live::bevy::LiveSource` is the feed as a Bevy audio source. In a browser, [osage-sound-web](/modules/osage-sound-web.md) plays the engine in an AudioWorklet, a block made in each render quantum.

Offline (`render`), the engine starts three seconds before the first sample so that sound already on its way arrives, and the file is faded at both ends, brought to a loudness by one gain and dithered to 16 bits ([loudness mastering](/algorithms/loudness-mastering.md)).

Ears: the structural model of Brown and Duda (1998) — the far ear later by up to 0.66 ms (a sphere of 8.75 cm), the head's shadow as one pole and one zero per ear, five short echoes of the outer ear for height. For loudspeakers, an equal-power pan.

Order: the engine sorts the scene's emitters, surfaces and touches by node path and content before it starts (`settle`), and every choice between two equal things is made by what they are, never by their index. The same scene written in another order renders to the same samples.

| Quality | Line read | Reflections | Outer ear | Reverberation | Ways at once | Ways found | Contact voices |
|---|---|---|---|---|---:|---|---:|
| low | 2 samples (linear) | folded into the straight way's level | no | no | 16 | every 4 blocks | 4 |
| medium | 4 samples (Lagrange) | followed | no | yes | 48 | every 2 | 6 |
| high | 4 samples | followed, checked for what hides them | yes | yes | 128 | every block | 8 |
| reference | 8 samples, windowed sinc | the same | yes | yes | all | every block | 16 |

# Interfaces

- `scene::Scene::from_json(text)`: a scene from the JSON of a glTF 2.0 asset without meshes. The object format's header and node identifiers are read where the format has them; what is new is in sibling `osageSound` keys (`asset.extras.osageSound.contactSounds`, `node.extras.osageSound.{emitters, surfaces, touches, motion, turning, proxyBeyondM}`, `scene.extras.osageSound.{seconds, seed, loudnessLufs, reverb}`).
- `engine::Engine::{new(scene, options), render(left, right)}`: the next block for each ear. `Options`: `quality`, `listening`, `turn` (the head turned), `solo` and `mute` (by name), `start`. `Quality::{LOW, MEDIUM, HIGH, REFERENCE}`. `Tally`: what a render cost, stage by stage.
- `ways::{direct, mirrored, edge_loss, air_loss}`, `ears::hear_for`: usable alone.
- `Engine::{drive, drive_at, node}`, `Pose`: a node placed from outside.
- `render::{render, render_told, Told, Replay, fade, level, master}`: a scene to a file's samples, with or without poses told.
- `live::{Live::{start, drive, feed, report}, Feed, Listening::{sample, next_block}, Report}`; `live::bevy::{LivePlugin, LiveSource}` with the `bevy` feature.
- `bench::{cost, ring, peak_memory}`: processor time and memory, on the bridge and on a ring of noises.

# Invariants

Each is a test of `crates/osage-sound/tests/` or of the module, or a check of [soundscape verify](/oracles/soundscape-verify.md):

- The same scene and seed give the same bytes; the scene's file in another order (nodes, children, lists shuffled) gives the same samples at every quality.
- Turning the head half round moves a side source to the other ear, sample for sample.
- Distance, Doppler shift (of an echo too), the floor's reflection and its delay, what a pane lets through, an echo's time of arrival, the reverberation's decay in each band and a contact's place and time agree with the physics they stand for, within stated tolerances.
- Nothing steps as an emitter goes behind a face, whatever the face lets through. A way never reads sound sooner than its length takes.
- Played live with poses told in advance, it gives the samples of an offline render with the same poses, block for block (the bridge with a walking listener, at two qualities); a node told where it is sixty times a second sounds as one that moves by itself, within −60 dB.
- A contact with no way to the ears takes no voice; a step is the same whoever else walks; of two echoes as loud, the one kept does not depend on which wall is listed first.

# Knowledge

- Algorithms: [sound ways](/algorithms/sound-ways.md), [sound delay lines](/algorithms/sound-delay-lines.md), [contact voices](/algorithms/contact-voices.md), [feedback delay reverberation](/algorithms/feedback-delay-reverb.md), [loudness mastering](/algorithms/loudness-mastering.md).
- Oracle: [soundscape verify](/oracles/soundscape-verify.md), and the crate's tests, which measure their claims with [osage-sound-analysis](/modules/osage-sound-analysis.md).
- Voices: [osage-sound-voices](/modules/osage-sound-voices.md). The scooter is [osage-kart-sound](/modules/kart-sound.md)'s two-stroke.
- Design: [design/09](https://github.com/smichea/osage/blob/main/design/09-sound.md) (what an object carries for sound, qualities by device, server and clients, far sound as a field; decision D-185) and the prototype's [DESIGN.md](https://github.com/smichea/osage/blob/main/tools/paris-soundscape/DESIGN.md) (its measurements and its limits).

# Reuse

The engine knows nothing of Paris: any tree of nodes with emitters, faces and touches. The delay line per emitter (distance, Doppler and echoes without a rule), the way-finding with a smooth passage behind an edge, and the ranking of ways by loudness under a budget transfer to any engine.

# Limits

- One thread for the engine, at the system's ordinary priority: on a busy machine it can be late, and the output then hears silence (measured below). In a browser, see [osage-sound-web](/modules/osage-sound-web.md).
- Live, a pose is heard after the blocks kept ahead and the one made with it, plus the output's own buffer: on the bridge with a 10 ms output buffer, 13 ms on average from a pose to the output taking it, 23 ms to the ears. Rendering in the output's callback would save the blocks kept ahead, at the cost of making the output wait on the engine.
- One reflection off each face and one edge per way: no reflection of a reflection, no way round two edges.
- Ears without a measured head: height is weak and behind sounds like its mirror in front (issue #18).
- Faces are flat quadrilaterals written in the scene; real meshes and contacts from the physics are issue #15.
- The reverberation's decay is given by the scene, not found from its geometry.
- See `tools/paris-soundscape/DESIGN.md` and issue #21 for the full list.
