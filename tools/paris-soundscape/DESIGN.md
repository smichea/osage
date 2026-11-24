<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Sound in the object tree

2026-11-23 · status: a prototype and a proposal · by claude-code (Fable 5.2), for the responsible person to review

The proposal — what an object carries for sound, qualities by device, server and clients, far sound as a field — is now [design/09](../../design/09-sound.md), with the decision D-185; the engine's cards are in [knowledge/](../../knowledge/modules/osage-sound.md). This note keeps the prototype: what it does, what it costs, against what it was measured, and where it stands.

A first batch of sound for osage: an engine that hears a scene offline, and what it shows about how an object should carry sound. The engine is `crates/osage-sound`, its voices `crates/osage-sound-voices`, what is measured in a recording `crates/osage-sound-analysis`; the command and the scene of the bridge are here, in `tools/paris-soundscape/`.

## What it does

A scene is an object tree, as the object format has it (the city programme's design note 08): nodes, each placed relative to its parent. Sound adds three things to a node, and a listener.

1. **Emitters**: what the node sounds like by itself. A voice (a generator, not a recording) placed at a position local to the node.
2. **Surfaces**: what sound meets. Flat faces with an acoustic material; they reflect and they hide.
3. **Touches**: what the node touches as it goes. Feet and wheels, so that a contact sounds as the two materials meeting do.

Every 128 samples (2.7 ms) the engine asks the tree where everything is and finds, for each emitter, the ways its sound takes to the listener's head:

- **straight**, when nothing hides it — or through what hides it, with what its material lets through in each band, when that arrives louder than round its edge. Just behind an edge the two are still one sound and add; a face has as much of an edge as it stops, and a pane that lets everything through has none;
- **round an edge**, when a face hides it: over the edge by which it arrives loudest — the one that lengthens the way least, among faces that stop everything — quieter and duller by how much longer that is, counted in wavelengths. A way that only grazes an edge already loses a little, so nothing switches as a source goes behind a deck. Maekawa, "Noise reduction by screens" (Applied Acoustics 1, 1968), in the piecewise form that continues into the lit side ([as tabulated here](https://strutt.arup.com/help/Environmental_Noise/EnviroBarrierAtten.htm));
- **mirrored** by each face that reflects, from the emitter's image behind it, less what the material takes in each band. [Allen and Berkley, 1979](https://doi.org/10.1121/1.382599). What the material scatters feeds a small reverberation instead, by the way it takes by that face and at the time that way takes.

Which way round an edge is followed is told by the ways alone — the louder, then the shorter, then where it turns — and the faces a way goes through are taken in the order it meets them: nothing depends on the order the faces are listed in. A face gone through does not take away the edge of the one beside it: a pane across the way leaves a screen what it took. What stands on a leg of a way round an edge or of a mirrored way takes from it what its material does not let through, band by band: a pane that lets everything through takes nothing, a wall that lets nothing through ends the way. Any number of faces may hide, in any order. Nor is the order a file lists things in any part of the scene: the engine takes the emitters, the surfaces and the touches in an order of its own, by the node each is on and by what it is, so that whatever is summed, taken in turn or told apart by its place in a list is the same for the same scene. And what a thing does by itself does not depend on who else is there: an emitter's voice, and the blocks in which its ways are found anew, come from its name; how strong a contact is and what grain it has, from whose contact it is and which one of them. A way longer than its emitter is heard from (`maxDistanceM`) is not rendered: the reach is a length of way, an echo's as well as the straight one's, and no sound is ever read sooner than its way takes.

Each emitter's voice writes into a line of its own. A way is that line read some time back: the time sound takes over the way's length. So a distant source is heard late, and one that comes nearer is read faster and sounds higher. There is no Doppler rule; a mirrored way reads the same line further back and has a shift of its own, found from where the emitter was when the sound now arriving by that way left it — earlier than for the straight way, and so elsewhere. An emitter that moves leaves a trail of where it has been, as long as its sound can take to arrive, and that is where this is read: what it does now changes nothing of what is still on its way. What a voice makes now is what its emitter is doing now. What is read goes through four bands (what distance, an edge or a material leave of each), a low-pass for the air (ISO 9613-1:1993, [as reproduced here](https://sengpielaudio.com/AirdampingFormula.htm)), and then the ears.

For headphones the ears are a model, not a measurement: the far ear hears up to 0.66 ms later and through the head's shadow, and five short echoes stand for the outer ear and say a little about height. [Brown and Duda, 1998](https://escholarship.org/content/qt0cr9d23p/qt0cr9d23p_noSplash_c6f0ae8be6c4003ba4f479c6ef3d369b.pdf). For loudspeakers, where the listener's own head does this, the level alone is shared between the two channels, as `osage-kart-native` places its karts.

The voices are synthesised sample by sample from the state of what makes them (`crates/osage-sound-voices`): the Seine as bubbles, a noise bed and laps ([van den Doel, 2005](http://persianney.com/kvdoelcsubc/publications/tap05.pdf)); cars shaped to the European road-noise model's octave levels (CNOSSOS-EU, Directive (EU) 2015/996); a scooter, which is the kart's two-stroke (`osage-kart-sound`, reused as its card suggests); a rubber-tyred metro as rolling bogies, the stepped whine of its traction, and the steel viaduct rumbling under it; contacts as a short push that sets ringing what was struck. Each lists its own assumptions in its module.

## What an object carries for sound

What an object carries for sound — its acoustic material, its emitters, the sound it makes when touched — the four contacts the `impact` voice makes, and the fields proposed for the object format are in [design/09 §2](../../design/09-sound.md#2-what-an-object-carries-for-sound). In the scene the walker's steps change as the ground does (asphalt, then a steel plate over an expansion joint), the cars knock on the same plate, and each axle of the train knocks on the viaduct's joint; here feet and wheels stand for the physics.

## What it costs, and for whom

The engine has four qualities. Each is what a class of device can afford; the scene is the same.

| | low | medium | high | reference |
|---|---|---|---|---|
| Reading the line | 2 samples | 4 | 4 | 8, windowed |
| Reflections | folded into the straight way's level | followed | followed, checked for what hides them | the same |
| Edges | yes | yes | yes | yes |
| Outer ear | no | no | yes | yes |
| Reverberation | no | yes | yes | yes |
| Ways followed at once | 16 | 48 | 128 | all |
| The scene looked at | 94 times a second | 188 | 375 | 375 |
| Air on a short way | left out under 1 dB | under 0.5 dB | always | always |
| Voices far off | less detail | less detail | less detail | full |

What is too quiet to matter is not made: a contact that does not reach the ears takes no voice — too far, or with no way to them within its reach among the faces, however near it is as the crow flies — and one takes a voice only from a contact heard less than it (as loud as it arrives by the loudest of its ways, by what its voice still makes of its loudest); the ways are ranked by how loud they arrive and only the loudest are rendered — of two as loud, the one that arrives sooner, then the one more to the left, never the one whose face is listed first; a voice none of whose ways is rendered sleeps, and swells in over 20 ms when it wakes; from afar a node stands for its branch. Between two looks at the scene every delay and gain goes on evenly to where it was last found to be going, so a lower rate costs 8 ms of lateness and no step.

Measured on 2026-11-23 on an Intel Core i5-1335U whose power setting held it at 1.3 GHz, on one performance core, while the machine was busy with other work: medians of three runs, in processor time. `paris-soundscape bench` prints them again; [bench/](bench/README.md) holds the baselines.

**The bridge**: 45 seconds, 41 emitters and their contacts. Milliseconds of one processor for a second of sound; under 1 000 it keeps up with the clock. (This table was measured again on 2026-11-24, in the same state of the machine, after a review's corrections to the search for ways and to the reverberation: five more ways are followed at the two highest qualities, those that pass through a deck, what each face scatters is read at its own time, and an emitter that is hidden is still asked what the faces beside its way take, which costs the search for ways a few hundredths of the whole; a contact's ways are looked for before it is given a voice, which costs nothing that shows. The tables of one source below are of the day before; what they measure did not change.)

| Quality | Ways at once | Voices at once | Speakers: native | Speakers: WebAssembly | Headphones: native | Headphones: WebAssembly |
|---|---:|---:|---:|---:|---:|---:|
| low | 16 | 18 | 40 | 50 | 54 | 65 |
| medium | 48 | 31 | 94 | 130 | 138 | 177 |
| high | 63 | 38 | 133 | 184 | 229 | 300 |
| reference | 63 | 38 | 188 | 241 | 344 | 416 |

WebAssembly is the same program built for `wasm32-wasip1` and run in V8 (Node 26), the engine of Chromium: what a browser client would pay. Voices, the search for ways and the ears are all in these figures.

It keeps 21.8 MB of sound on its way: a line for each emitter, as long as its sound can take to arrive (512 kB for a reach of 600 m). The whole program held 32 MB at most. The scene is a file of 24 kB and there is no sound file: nothing is read or sent for sound beyond where things are. No part of it runs on a GPU.

**One source**, on a ring of 32 to 256 noises turning round the listener (the scene the baselines play):

| For one source and one sample, ns | prototype | OpenAL Soft 1.25 | Web Audio, Chromium 155 |
|---|---:|---:|---:|
| Panned to two loudspeakers | 18 to 22 (low) | 9 to 11 | 266 |
| Heard by two ears | 37 to 47 (low) · 38 to 43 (medium) · 59 to 65 (high) · 95 to 105 (reference) | 83 to 89 | 342 |
| Heard by two ears, with its reflection off a road | 117 to 139 (high) | not done | not done |

- **Heard by two ears**, a source costs the prototype half of what it costs OpenAL Soft at `low` and `medium`, three quarters at `high`, and a little more at `reference`. With 256 sources `high` still keeps up with the clock on this core (803 ms a second); OpenAL Soft's HRTF does not (1 023 to 1 131 ms).
- **Panned to two loudspeakers**, it costs twice what it costs OpenAL Soft. The prototype makes each source's sound and keeps it to be late by its distance; OpenAL Soft plays a buffer it was given. One way alone, without its voice, costs 13 ns.
- **Chromium's Web Audio**, its sources moved every 1 024 frames as an offline render must move them, costs about thirteen times the prototype panned, and three to nine times heard by two ears.
- The two baselines convolve with measured responses of a head, which the prototype does not have. The prototype's figures include the delay of each way and what the air takes, which the baselines do not do.

| One way alone, ns a sample | speakers | headphones |
|---|---:|---:|
| low | 13 | 31 |
| medium | 14 | 33 |
| high | 14 | 46 |
| reference | 33 | 86 |

One way alone is a line of noise read through a way that goes from 40 m before the listener to 160 m beyond; the air's filter is at work on most of it.

### Server and clients, and what is far

How a server and its clients could share the work, and how what is far could be heard as a field made once per cube (sound holograms), are in [design/09 §5 and §6](../../design/09-sound.md#5-server-and-clients).

## Against what it was measured

**Open-source engines, on this machine.** `bench/` plays the same ring of noises in OpenAL Soft 1.25 through its loopback device and in Chromium's Web Audio through an offline context; the tables above set the prototype beside them. They do not do the same work, and [bench/README.md](bench/README.md) says exactly what each does. OpenAL Soft and Web Audio convolve with measured responses of a head, which give height and front-back cues that a sphere cannot. The prototype carries each sound over its way, with its delay, its Doppler shift, its air and its reflections, which neither does. So the fair reading is by function: a panned source, a source heard by two ears, a source with its reflections.

**The external reference** of the city programme (its design note 07) was not opened by the author of this code. Its behaviour was asked for as measurements and answered by the evaluation track; the questions, the answers and two 45-second captures are in the evaluator's private notes, with a side-by-side reading of them. `measure` reads those captures as it reads this prototype's renders: every sample of a file counts alike, its last tenth of a second as any other, and a mono capture's one channel counts once.

## Where it stands, and what is not there

Checked by `verify` and by the tests: the same seed gives the same bytes; nothing clips; the file is as loud as asked once its ends are faded; turning the listener half round moves a side source to the other ear, sample for sample; distance, Doppler shift (an echo's too), the floor's reflection and its delay, what a pane lets through, an echo's time of arrival and that of what a wall scatters, an echo unchanged until what changed it has had time to arrive, the reverberation's decay in each band, a contact's place and time agree with the physics they stand for within the tolerances the tests state; another glTF scene in the same file changes nothing; a wall hides wherever it is listed among the faces, and no way changes by a bit when faces drawn at random are listed in another order; a pane gone through leaves a screen beside it its edge; steps too far to be heard, behind a wall, or heard less, take no voice from a step that is heard; a step is the same whoever else walks; of two echoes as loud, the one kept is the same whichever wall is listed first; the file of the bridge with its nodes and their lists in another order renders to the same samples at every quality; nothing steps as an emitter goes behind a pane, whatever the pane lets through; a contact left out by solo or mute takes no voice from one that is heard. A check kept apart (`object-check/`, which needs the engine's `osage-objects`) loads the scene of the bridge in the engine's own reader of the object format and finds the same node paths.

What instruments say of the example (each group of sources rendered alone and set against all the rest, three seconds at a time):

- **The Seine**, in the left ear between 1 and 6 kHz, is level with the two loudest cars as they pass and 4 to 13 dB above everything else the rest of the time; it is 7 to 8 dB louder on the left than on the right there. It is louder than a slow river is beside traffic: the brief asks that it stay audible.
- **The cars** of the near carriageway stand 8 to 12 dB above the rest in the right ear as they pass, with the sweep of the road's reflection across the top of the spectrum; those of the far carriageway and the scooter 0 to 6 dB.
- **The train** comes out of the rest below 500 Hz from about the 15th second, stands 9 dB above it at the 24th, and is lost again about the 33rd, before the next cars. It is heard on the right by the time between the ears (0.4 ms), not by level: it is low, and a head casts no shadow there.
- **The steps** are regular, 13 to 21 dB under the rest on average, and those on the steel plate ring.

Not there, or weak:

- **Listened to once**, on 2026-11-24, on a laptop's loudspeakers (issue #20): the places, the passages, the train, the city and the steps are as described; the river is exaggerated (its bed too loud, its laps like waves on sand, bubbles that chirp where the Seine has hardly any); in the bridge the cars and the steps do not sound real, though the steps alone do; the car alone sounds a little like a propeller aircraft's engine, the metro's traction like a motorbike shifting gears, and the default contact like a hand on a thin metal sheet. The loudspeaker render and the viaduct on headphones are still to be heard. Every other claim here is a measurement.
- **Height** is conveyed weakly: the outer-ear model is fitted in front of the listener, and ears differ. Measured responses of a head (SADIE II is Apache-2.0, MIT KEMAR is free to use) would do better; they are data to download and to enter in the asset manifest, so they wait for a decision.
- **Behind** sounds like its mirror in front, for the same reason.
- **What a face scatters** is one late copy of the emitter's sound for each face: two faces' copies of a steady tone add or cancel in the reverberation as the emitter moves, where a rough wall would blur them. An emitter none of whose ways reaches the ears is not answered by the space either, so what is wholly hidden is not heard by what a wall scatters. A face that moves mirrors from where it is now, not from where it was when the sound met it.
- **A contact that does not reach the ears when it is made** is never heard: if the listener comes round the wall that hid it while it would still ring (2.5 s at most), it is not there.
- **Of two ways as loud** at the edge of what a device affords, one is kept and the other is not: the same one whatever the file's order, but a scene that is the same on both sides is then not heard the same on both sides.
- **`verify` reads the river's side a second at a time**: what the river does for less than a second is in its second's mean.
- **One reflection** off each face, and one edge. No reflection of a reflection, no way round two edges (a source under the middle of a deck is silent rather than faint).
- **The air** above 5 kHz on long ways is understated (two poles follow it to there).
- **The reverberation's length** is given by the scene, not found from its geometry: three decays, at 250 Hz, 1 kHz and 4 kHz, each met within a tenth when neighbours are no more than about twice apart. The bands are parted gently (6 dB an octave), so decays further apart than that are not met; whatever is asked, nothing rings more than twice as long as the longest.
- **Materials** are made-up test values in the range textbooks give.
- **The metro's spectrum** is a proxy: no public measurement of a rubber-tyred train on a steel viaduct was found.
- **One thread.** Emitters are independent and could be shared between cores; the figures above are for one.
- **A voice that wakes** starts in the middle of its sound and is heard only once that sound has arrived: a far emitter that comes within the budget is late by its distance.
- **The cars' engines** are steady tones (the road-noise model puts most of a car's power at 30 km/h in its lowest octave). Two of them a few turns a minute apart beat; the scene lets every car's speed drift so that they do not beat like a clock.

## Second batch

1. ~~Move the engine, its voices and the analyser into crates of the engine~~: done (#13): `crates/osage-sound`, `crates/osage-sound-voices` (the engine's licence keeps them out of `worlds/`), `crates/osage-sound-analysis`. Cards, a design note and a decision followed (#14: [design/09](../../design/09-sound.md), D-185). Still to do: emitters and contact sounds into the object format and its reader (#15).
2. Read real objects: surfaces from `acoustic[]` meshes through `osage-objects`, contacts from `osage-dynamics`.
3. ~~A Bevy audio source fed by the engine; the browser client through an AudioWorklet~~: done (#16). Natively, `osage_sound::live` plays the engine on a thread of its own a few blocks ahead of the output, its nodes placed by a game (`Engine::drive`), and `live::bevy::LiveSource` is a Bevy audio source; `paris-soundscape live` measures it. In a browser, `crates/osage-sound-web` is the engine as WebAssembly in an AudioWorklet, a block in each render quantum; `web/live.py` checks and measures it in Chromium.
4. ~~The sound field for what is far, and its stream~~: built and measured (#17): `osage_sound::field`, `paris-soundscape far`, `field`, `decode` and `bench/field_opus.py`; see the card [far sound field](../../knowledge/algorithms/far-sound-field.md). Still to do: a crossfade between two cubes' fields, and the stream itself.
5. Ears from measured responses, once the data is chosen.
6. A listening test against the external reference, blind, as design/07 asks.
