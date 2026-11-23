<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Sound in the object tree

2026-11-23 · status: a prototype and a proposal · by claude-code (Fable 5.2), for the responsible person to review

A first batch of sound for osage: a standalone program that hears a scene offline, and what it shows about how an object should carry sound. It lives in `tools/paris-soundscape/` and touches nothing else. The second batch moves its parts into the engine (the last section says where).

## What it does

A scene is an object tree, as the object format has it ([design/08](../../design/08-object-format.md)): nodes, each placed relative to its parent. Sound adds three things to a node, and a listener.

1. **Emitters**: what the node sounds like by itself. A voice (a generator, not a recording) placed at a position local to the node.
2. **Surfaces**: what sound meets. Flat faces with an acoustic material; they reflect and they hide.
3. **Touches**: what the node touches as it goes. Feet and wheels, so that a contact sounds as the two materials meeting do.

Every 128 samples (2.7 ms) the engine asks the tree where everything is and finds, for each emitter, the ways its sound takes to the listener's head:

- **straight**, when nothing hides it — or through what hides it, with what its material lets through in each band, when that arrives louder than round its edge;
- **round an edge**, when a face hides it: over the edge that lengthens the way least, quieter and duller by how much longer that is, counted in wavelengths. A way that only grazes an edge already loses a little, so nothing switches as a source goes behind a deck. Maekawa, "Noise reduction by screens" (Applied Acoustics 1, 1968), in the piecewise form that continues into the lit side ([as tabulated here](https://strutt.arup.com/help/Environmental_Noise/EnviroBarrierAtten.htm));
- **mirrored** by each face that reflects, from the emitter's image behind it, less what the material takes in each band. [Allen and Berkley, 1979](https://doi.org/10.1121/1.382599). What the material scatters feeds a small reverberation instead.

What stands on a leg of a way round an edge or of a mirrored way takes from it what its material does not let through, band by band: a pane that lets everything through takes nothing, a wall that lets nothing through ends the way. Any number of faces may hide, in any order. A way longer than its emitter is heard from (`maxDistanceM`) is not rendered: the reach is a length of way, an echo's as well as the straight one's, and no sound is ever read sooner than its way takes.

Each emitter's voice writes into a line of its own. A way is that line read some time back: the time sound takes over the way's length. So a distant source is heard late, and one that comes nearer is read faster and sounds higher. There is no Doppler rule; a mirrored way reads the same line further back and has a shift of its own, found from where the emitter was when the sound now arriving by that way left it — earlier than for the straight way, and so elsewhere. What is read goes through four bands (what distance, an edge or a material leave of each), a low-pass for the air (ISO 9613-1:1993, [as reproduced here](https://sengpielaudio.com/AirdampingFormula.htm)), and then the ears.

For headphones the ears are a model, not a measurement: the far ear hears up to 0.66 ms later and through the head's shadow, and five short echoes stand for the outer ear and say a little about height. [Brown and Duda, 1998](https://escholarship.org/content/qt0cr9d23p/qt0cr9d23p_noSplash_c6f0ae8be6c4003ba4f479c6ef3d369b.pdf). For loudspeakers, where the listener's own head does this, the level alone is shared between the two channels, as `osage-kart-native` places its karts.

The voices are synthesised sample by sample from the state of what makes them (`voices/`): the Seine as bubbles, a noise bed and laps ([van den Doel, 2005](http://persianney.com/kvdoelcsubc/publications/tap05.pdf)); cars shaped to the European road-noise model's octave levels (CNOSSOS-EU, Directive (EU) 2015/996); a scooter, which is the kart's two-stroke (`osage-kart-sound`, reused as its card suggests); a rubber-tyred metro as rolling bogies, the stepped whine of its traction, and the steel viaduct rumbling under it; contacts as a short push that sets ringing what was struck. Each lists its own assumptions in its module.

## What an object carries for sound

An object has a look: colour, relief, gloss. It also has a sound, and it has three sides.

| Side | What it says | Where it is carried |
|---|---|---|
| **What it does to sound that reaches it** | For each band: the share it absorbs, lets through, mirrors, scatters | The acoustic material of a surface. The object format has it already (`acousticMaterials`, `node.extras.osage.acoustic[]`), at 250, 1 000 and 4 000 Hz. |
| **What it emits by itself** | A voice and its parameters, a place in the node, a level, from how far it is heard | New: an **emitter** on a node. |
| **What it emits when touched** | How it rings (its lowest mode, how long it lasts, its shape), how hard it is, how much it rubs or crumbles | New: a **contact sound** on a collision material, beside its friction and its bounce. |

### A contact is two objects

The sound of a contact never belongs to one object. It is made of:

- **a push**, whose length the softer of the two sets. Steel on steel lasts a few tenths of a millisecond, a sole several milliseconds. A long push leaves the high modes alone: the sound is dull;
- **the modes of each object**: its shape and size set their frequencies, its material how fast each dies. The ear tells materials by how the decay quickens with pitch (Klatzky, Pai and Krotkov, "Perception of material from contact sounds", Presence 9(4), 2000), so one number per material — a decay rate proportional to frequency — goes a long way (van den Doel and Pai, "The sounds of physical shapes", Presence 7(4), 1998). These two papers are cited from memory and were not read again for this work;
- **a noise**: the rub of something rough or soft, the crackle of something brittle.

Four contacts, as the `impact` voice makes them (`voices/src/contact.rs`, whose test measures these):

| Contact | The push | What rings | Measured |
|---|---|---|---|
| Hammer on an anvil | very short | the anvil, for most of a second; the hammer's head ticks | within 40 dB of its peak for 0.89 s, weight at 1.7 kHz |
| Hammer on limestone | short, but the stone gives way | the block, a few hundredths of a second; then it crumbles | 0.07 s, weight at 4.1 kHz (the crackle) |
| Leather sole on a paving stone | long | almost nothing; the sole rubs | 0.07 s, weight at 2.7 kHz |
| Wooden clog on the same stone | medium | the clog itself, a hollow body: a note at 520 Hz | 0.08 s, weight at 1.3 kHz |

The same hammer ticks less on limestone than on steel, because the stone gives way and the push lasts longer. Nothing says so: it follows from giving each object its own modes and the pair one push.

In the scene the walker's steps change as the ground does (asphalt, then a steel plate over an expansion joint), the cars knock on the same plate, and each axle of the train knocks on the viaduct's joint. Here feet and wheels stand for the physics; in the engine the contact comes from the solver, which already reports where, between which surfaces and how hard ([osage-dynamics](../../knowledge/modules/osage-dynamics.md)).

### Proposed fields

They are written here in sibling `osageSound` keys, because the engine's reader refuses a key it does not know inside `osage` and `osageObject` and steps over a sibling: the scene file loads today as a version 1 object tree. Moving them in is a change to the reader, a decision-log entry and a card: second batch.

| Location | Meaning |
|---|---|
| `node.extras.osageSound.emitters[]` | `id`; `type`: `positional` or `global` (straight into the ears, with a `pan`); `voice` and `params`: a generator and what it is given; `positionM`: where in the node; `gainDb`; `maxDistanceM`; `load` or `loadedBy` (which other emitters, when near, excite it: a span under a train); `reflections`. The names follow the draft glTF extension `KHR_audio_emitter` (an open pull request on the Khronos glTF repository) where it has one. |
| `node.extras.osageSound.proxyBeyondM` | From this far from the listener, the node's own emitters stand for its whole branch. The same cut through the tree as `subtreeProxy` makes for the eye and for contact ([design/06 §2](../../design/06-objects.md)): thirteen emitters of a train become two. |
| `asset.extras.osageSound.contactSounds` | For a key of `collisionMaterials`: `voice` and `params` (`tone`, `ring`, `shape`, `scuff`, `grain`), `hardness`, `gainDb`. |
| `node.extras.osageSound.surfaces[]` | Stands for `acoustic[]` and `collision[]` while the scene has no meshes: `quadM` (four corners), `acoustic` and `collision` (material keys), `reflects`, `occludes`, `strikes`. |
| `node.extras.osageSound.motion`, `turning`, `touches` | Stand for the simulation: a node going along a straight way at a speed that changes, a node turning, feet and wheels. |

Two things the format should decide before these move in. A material's **modes belong to the object** (its size sets them) and its **damping to the material**: today both are parameters set by hand on the collision material. And an acoustic material has three bands; the engine shapes a way in four, the fourth for what the air and edges take first.

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

What is too quiet to matter is not made: the ways are ranked by how loud they arrive and only the loudest are rendered; a voice none of whose ways is rendered sleeps, and swells in over 20 ms when it wakes; from afar a node stands for its branch. Between two looks at the scene every delay and gain goes on evenly to where it was last found to be going, so a lower rate costs 8 ms of lateness and no step.

Measured on 2026-11-23 on an Intel Core i5-1335U whose power setting held it at 1.3 GHz, on one performance core, while the machine was busy with other work: medians of three runs, in processor time. `paris-soundscape bench` prints them again; [bench/](bench/README.md) holds the baselines.

**The bridge**: 45 seconds, 41 emitters and their contacts. Milliseconds of one processor for a second of sound; under 1 000 it keeps up with the clock. (This table was measured again on 2026-11-24, in the same state of the machine, after a review's corrections to the search for ways and to the reverberation: five more ways are followed at the two highest qualities, those that pass through a deck. The tables of one source below are of the day before; what they measure did not change.)

| Quality | Ways at once | Voices at once | Speakers: native | Speakers: WebAssembly | Headphones: native | Headphones: WebAssembly |
|---|---:|---:|---:|---:|---:|---:|
| low | 16 | 18 | 40 | 46 | 55 | 62 |
| medium | 48 | 31 | 94 | 124 | 139 | 184 |
| high | 63 | 38 | 126 | 171 | 227 | 288 |
| reference | 63 | 38 | 180 | 230 | 355 | 408 |

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

### Server and clients

Nothing in this batch needs a server. The design keeps three splits open, by what each side can afford:

1. **The client finds the ways and renders them** (what the prototype does). The server sends what it sends anyway: where things are. The voices are generators, so no sound is sent and none is read from disk: the bridge is 24 kB of scene and no audio file.
2. **The server (or an edge) finds the ways, the client renders them.** A way is seven numbers every few milliseconds (a delay, four gains, a direction). For a client too weak to hold the acoustic geometry this trades a few kilobytes a second for the search. The engine already separates the two: `finding the ways` and `carrying` in the tables above.
3. **Far sound as a field** (next section): what comes from far is the same for everyone in a place, so it is made once for the place.

The server never mixes for a listener: that would cost it a voice count per player and a round trip in latency.

No part of this uses the GPU. On a tier that already traces rays for light, the same acceleration structure could find ways; nothing here depends on it.

## Sound holograms: a field for what is far

osage draws what is far from a viewer as holograms: the light arriving at a cube from outside it, baked or streamed, shared by everyone in the cube ([osage-cubes](../../knowledge/modules/osage-cubes.md), [osage-hologram-stream](../../knowledge/modules/osage-hologram-stream.md)). Sound has the same shape, and a known form for it.

- **Near** (the listener's own cube and its neighbours): emitters, ways, ears — this prototype.
- **Far**: the sound arriving at a cube from outside it, kept as a **sound field** at the cube's centre: a few channels that hold the sound by direction (ambisonics; four channels for the first order, nine for the second). It is made once per cube, whoever listens, and each listener turns it to their own head and decodes it to their own ears. A listener that moves inside the cube hears the far field unchanged, as a far light field changes little with a step.
- **Carried** as light is: a stream per cube and level, the larger cubes at a lower order and rate. The research notes give 64 to 128 kbit/s for such a bed with Opus; that figure is from memory and is not verified here.

What this buys is the same as for light: the cost of the far world no longer grows with the number of sources or of listeners. What the prototype would need to show it: a bus that takes the ways of far emitters as a direction and a gain instead of two ears (cheaper than a way: no ear stage), a decoder to two ears, and a measurement of what is lost against rendering every source — the `measure` command gives the figures to compare. It is not built in this batch.

## Against what it was measured

**Open-source engines, on this machine.** `bench/` plays the same ring of noises in OpenAL Soft 1.25 through its loopback device and in Chromium's Web Audio through an offline context; the tables above set the prototype beside them. They do not do the same work, and [bench/README.md](bench/README.md) says exactly what each does. OpenAL Soft and Web Audio convolve with measured responses of a head, which give height and front-back cues that a sphere cannot. The prototype carries each sound over its way, with its delay, its Doppler shift, its air and its reflections, which neither does. So the fair reading is by function: a panned source, a source heard by two ears, a source with its reflections.

**The external reference** of the city programme ([design/07](../../design/07-city-scale-quality.md)) was not opened by the author of this code. Its behaviour was asked for as measurements and answered by the evaluation track; the questions, the answers and two 45-second captures are in the evaluator's private notes, with a side-by-side reading of them. `measure` reads those captures as it reads this prototype's renders: every sample of a file counts alike, its last tenth of a second as any other, and a mono capture's one channel counts once.

## Where it stands, and what is not there

Checked by `verify` and by the tests: the same seed gives the same bytes; nothing clips; the file is as loud as asked once its ends are faded; turning the listener half round moves a side source to the other ear, sample for sample; distance, Doppler shift (an echo's too), the floor's reflection and its delay, what a pane lets through, an echo's time of arrival, the reverberation's decay in each band, a contact's place and time agree with the physics they stand for within the tolerances the tests state; another glTF scene in the same file changes nothing; a wall hides wherever it is listed among the faces. A check kept apart (`object-check/`, which needs the engine's `osage-objects`) loads the scene of the bridge in the engine's own reader of the object format and finds the same node paths.

What instruments say of the example (each group of sources rendered alone and set against all the rest, three seconds at a time):

- **The Seine**, in the left ear between 1 and 6 kHz, is level with the two loudest cars as they pass and 4 to 13 dB above everything else the rest of the time; it is 7 to 8 dB louder on the left than on the right there. It is louder than a slow river is beside traffic: the brief asks that it stay audible.
- **The cars** of the near carriageway stand 8 to 12 dB above the rest in the right ear as they pass, with the sweep of the road's reflection across the top of the spectrum; those of the far carriageway and the scooter 0 to 6 dB.
- **The train** comes out of the rest below 500 Hz from about the 15th second, stands 9 dB above it at the 24th, and is lost again about the 33rd, before the next cars. It is heard on the right by the time between the ears (0.4 ms), not by level: it is low, and a head casts no shadow there.
- **The steps** are regular, 13 to 21 dB under the rest on average, and those on the steel plate ring.

Not there, or weak:

- **Nobody has listened to it.** Every claim here is a measurement. Whether the Seine sounds like water and the train like a train is for ears to say.
- **Height** is conveyed weakly: the outer-ear model is fitted in front of the listener, and ears differ. Measured responses of a head (SADIE II is Apache-2.0, MIT KEMAR is free to use) would do better; they are data to download and to enter in the asset manifest, so they wait for a decision.
- **Behind** sounds like its mirror in front, for the same reason.
- **One reflection** off each face, and one edge. No reflection of a reflection, no way round two edges (a source under the middle of a deck is silent rather than faint).
- **The air** above 5 kHz on long ways is understated (two poles follow it to there).
- **The reverberation's length** is given by the scene, not found from its geometry: three decays, at 250 Hz, 1 kHz and 4 kHz, each met within a tenth when neighbours are no more than about twice apart. The bands are parted gently (6 dB an octave), so decays further apart than that are not met; whatever is asked, nothing rings more than twice as long as the longest.
- **Materials** are made-up test values in the range textbooks give.
- **The metro's spectrum** is a proxy: no public measurement of a rubber-tyred train on a steel viaduct was found.
- **One thread.** Emitters are independent and could be shared between cores; the figures above are for one.
- **A voice that wakes** starts in the middle of its sound and is heard only once that sound has arrived: a far emitter that comes within the budget is late by its distance.
- **The cars' engines** are steady tones (the road-noise model puts most of a car's power at 30 km/h in its lowest octave). Two of them a few turns a minute apart beat; the scene lets every car's speed drift so that they do not beat like a clock.

## Second batch

1. Move `src/` to an engine crate (`crates/osage-sound`), `voices/` to a crate of the engine too (they carry the engine's licence, which keeps them out of `worlds/`), `analysis/` to the oracles; emitters and contact sounds into the object format and its reader, with a decision-log entry and cards. The city world (`worlds/ville-paris`) already uses the ears, the faces and the contact voice from here by path: what it uses is to be kept as it moves.
2. Read real objects: surfaces from `acoustic[]` meshes through `osage-objects`, contacts from `osage-dynamics`.
3. A Bevy audio source fed by the engine, as `osage-music` feeds its own; the browser client through an AudioWorklet (the engine's block is its render quantum).
4. The sound field for what is far, and its stream.
5. Ears from measured responses, once the data is chosen.
6. A listening test against the external reference, blind, as design/07 asks.
