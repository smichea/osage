# 09 — Sound: what an object carries, how it is heard, who pays

Date: 2026-11-24 · Status: **proposal**, with a first batch built — the engine renders offline; nothing plays live yet · From: the maintainer's brief for sound, after the prototype of the Pont de Bir-Hakeim ([tools/paris-soundscape/DESIGN.md](../tools/paris-soundscape/DESIGN.md)) and its review (issues #13 to #22)

osage's sound is synthesised from the state of what makes it and placed in the object tree. This note says what an object carries for sound, how a sound reaches a listener's ears, what each quality costs and for which device, how a server and its clients could share the work, and how what is far could be heard as a field. The engine is [osage-sound](../knowledge/modules/osage-sound.md), its voices [osage-sound-voices](../knowledge/modules/osage-sound-voices.md), its oracle [osage-sound-analysis](../knowledge/modules/osage-sound-analysis.md) (D-180); the decision is D-185.

## 1. Sound lives in the tree

An object is a tree of nodes, each placed relative to its parent ([06 — objects](06-objects.md)). Sound adds three things to a node, and a listener:

1. **Emitters:** what the node sounds like by itself. A voice — a generator, not a recording — and its parameters, placed at a position local to the node.
2. **Surfaces:** what sound meets. Faces with an acoustic material; they reflect and they hide.
3. **Touches:** what the node touches as it goes. Feet and wheels, so that a contact sounds as the two materials meeting do.

A sound is named by its node's path and placed in that node: when a parent moves or turns, its sound follows. Nothing else locates it. A server that sends where things are has sent everything the sound needs: the bridge is 24 kB of scene and no audio file.

## 2. What an object carries for sound

An object has a look: colour, relief, gloss. It also has a sound, and it has three sides.

| Side | What it says | Where it is carried |
|---|---|---|
| **What it does to sound that reaches it** | For each band: the share it absorbs, lets through, mirrors, scatters | The acoustic material of a surface. The [object format](08-object-format.md#fields) has it already (`acousticMaterials`, `node.extras.osage.acoustic[]`), at 250, 1 000 and 4 000 Hz. |
| **What it emits by itself** | A voice and its parameters, a place in the node, a level, from how far it is heard | New: an **emitter** on a node. |
| **What it emits when touched** | How it rings (its lowest mode, how long it lasts, its shape), how hard it is, how much it rubs or crumbles | New: a **contact sound** on a collision material, beside its friction and its bounce. |

### A contact is two objects

The sound of a contact never belongs to one object. It is made of:

- **a push**, whose length the softer of the two sets. Steel on steel lasts a few tenths of a millisecond, a sole several milliseconds. A long push leaves the high modes alone: the sound is dull;
- **the modes of each object**: its shape and size set their frequencies, its material how fast each dies. The ear tells materials by how the decay quickens with pitch (Klatzky, Pai and Krotkov, "Perception of material from contact sounds", Presence 9(4), 2000), so one number per material — a decay rate proportional to frequency — goes a long way (van den Doel and Pai, "The sounds of physical shapes", Presence 7(4), 1998). Both papers are cited from memory;
- **a noise**: the rub of something rough or soft, the crackle of something brittle.

Four contacts, as the `impact` voice makes them (measured by its tests):

| Contact | The push | What rings | Measured |
|---|---|---|---|
| Hammer on an anvil | very short | the anvil, for most of a second; the hammer's head ticks | within 40 dB of its peak for 0.89 s, weight at 1.7 kHz |
| Hammer on limestone | short, but the stone gives way | the block, a few hundredths of a second; then it crumbles | 0.07 s, weight at 4.1 kHz (the crackle) |
| Leather sole on a paving stone | long | almost nothing; the sole rubs | 0.07 s, weight at 2.7 kHz |
| Wooden clog on the same stone | medium | the clog itself, a hollow body: a note at 520 Hz | 0.08 s, weight at 1.3 kHz |

The same hammer ticks less on limestone than on steel, because the stone gives way and the push lasts longer. Nothing says so: it follows from giving each object its own modes and the pair one push. In the engine, a contact comes from the [physics solver](../knowledge/modules/osage-dynamics.md), which reports where, between which surfaces, how hard and when (issue #15): the world tells it to the engine (`Engine::strike_told`) as the struck node and its collision material, what struck it, and a strength — by default the change of speed the impulse made, full at 5 m/s. It is struck at its own sample. Feet and wheels remain as stand-ins for a scene without a solver. Which contacts are heard when there are more than voices is [contact voices](../knowledge/algorithms/contact-voices.md).

Listened to on 2026-11-24 (issue #20), the default contact — a slab struck by something soft — sounds like a hand on a thin metal sheet: the modes need checking against recordings before the presets are trusted.

### Proposed fields

The prototype writes them in sibling `osageSound` keys, because the engine's reader of the object format refuses a key it does not know inside `osage` and `osageObject` and steps over a sibling: the bridge's scene loads today as a version 1 object tree. Moving them into the format is a change to its reader, its specification and a decision entry of its own. Real objects are now read (issue #15) with the sibling keys kept: `osage-objects` validates the object and gives the meshes, the engine's reader gives what sound adds.

| Location | Meaning |
|---|---|
| `node.extras.osageSound.emitters[]` | `id`; `type`: `positional` or `global` (straight into the ears, with a `pan`); `voice` and `params`: a generator and what it is given; `positionM`: where in the node; `gainDb`; `maxDistanceM`; `load` or `loadedBy` (which other emitters, when near, excite it: a span under a train); `reflections`. The names follow the draft glTF extension `KHR_audio_emitter` (an open pull request on the Khronos glTF repository) where it has one. |
| `node.extras.osageSound.proxyBeyondM` | From this far from the listener, the node's own emitters stand for its whole branch. The same cut through the tree as `subtreeProxy` makes for the eye and for contact ([06 §2](06-objects.md)): thirteen emitters of a train become two. |
| `asset.extras.osageSound.contactSounds` | For a key of `collisionMaterials`: `voice` and `params` (`tone`, `ring`, `shape`, `scuff`, `grain`), `hardness`, `gainDb`. |
| `node.extras.osageSound.surfaces[]` | Stands for `acoustic[]` and `collision[]` while a scene has no meshes: `quadM` (four corners), `acoustic` and `collision` (material keys), `reflects`, `occludes`, `strikes`. |
| `node.extras.osageSound.surfaceSound` | For an object with meshes: by the identifier of an `acoustic[]` or `collision[]` surface, `reflects` and `occludes` (true when not said) and `strikes` (false). The faces are the meshes' triangles, two of a flat convex quad taken as one; faces of one plane that share a side are one surface ([sound faces from meshes](../knowledge/algorithms/sound-faces.md)). An acoustic material the format leaves unknown (`{}`) is not met by sound. |
| `node.extras.osageSound.motion`, `turning`, `touches` | Stand for the [simulation](07-city-scale-quality.md#movement-people-and-vehicles): a node going along a straight way at a speed that changes, a node turning, feet and wheels. |

Two things the format should decide before these move in. A thing's **modes belong to the object** (its size sets them) and its **damping to the material**: today both are parameters set by hand on the collision material. And an acoustic material has three bands; the engine shapes a way in four, the fourth for what the air and edges take first.

## 3. How a sound reaches the ears

Every 128 samples (2.7 ms, a Web Audio render quantum) the engine places the tree and finds, for each emitter, the ways its sound takes to the listener's head ([sound ways](../knowledge/algorithms/sound-ways.md)):

- **straight**, when nothing hides it — or through what hides it, with what its material lets through, when that is louder than round its edge;
- **round an edge**, over the edge by which it arrives loudest, quieter and duller by how much longer the way is in wavelengths (Maekawa, continued into the lit side so that nothing switches behind an edge);
- **mirrored** by each face that reflects, from the emitter's image behind it. What a face scatters feeds a small reverberation instead ([feedback delay reverberation](../knowledge/algorithms/feedback-delay-reverb.md)).

Each emitter's voice writes into a delay line of its own, and each way reads it as far back as the way is long ([sound delay lines](../knowledge/algorithms/sound-delay-lines.md)): a far sound is late, an approaching one higher, and an echo has its own delay and shift, with no rule for any of it. A moving emitter leaves a trail, and each way is found from where the emitter was when the sound now arriving by it left.

For headphones, the ears are a model (Brown and Duda, 1998): the far ear later and in the head's shadow, five short echoes of the outer ear for height. Where a client has the measured responses of a head (SADIE II's KEMAR, D-242), high and reference hear every way through 54 virtual loudspeakers filtered by them, each way keeping its own measured time between the ears ([measured ears](../knowledge/algorithms/measured-ears.md)). For loudspeakers, the level is shared between the two channels.

The order a scene is written in is no part of it: the engine takes emitters, surfaces and touches in an order of its own, and every choice between equals is made by what they are. What a thing does by itself does not depend on who else is there.

## 4. What each quality costs, and for which device

The engine has four qualities. The scene is the same; what changes is how much is made.

| | low | medium | high | reference |
|---|---|---|---|---|
| Meant for | a phone, a weak laptop, a browser tab among others | a laptop | a desktop, a console | measurements, offline renders |
| Reading the line | 2 samples | 4 | 4 | 8, windowed |
| Reflections | folded into the straight way's level | followed | followed, checked for what hides them | the same |
| Outer ear | no | no | yes | yes |
| Reverberation | no | yes | yes | yes |
| Ways followed at once | 16 | 48 | 128 | all |
| The scene looked at | 94 times a second | 188 | 375 | 375 |
| Contact voices | 4 | 6 | 8 | 16 |
| Voices far off | less detail | less detail | less detail | full |

What is too quiet to matter is not made: the ways are ranked by how loud they arrive and only the loudest are rendered; a voice none of whose ways is rendered sleeps; a contact that does not reach the ears takes no voice; from afar a node stands for its branch.

On the bridge (41 emitters, 45 s), on one core of an Intel Core i5-1335U held at 1.3 GHz, 2026-11-24, in milliseconds of processor for a second of sound (under 1 000 keeps up with the clock):

| Quality | Speakers: native | Speakers: WebAssembly | Headphones: native | Headphones: WebAssembly |
|---|---:|---:|---:|---:|
| low | 40 | 50 | 54 | 65 |
| medium | 94 | 130 | 138 | 177 |
| high | 133 | 184 | 229 | 300 |
| reference | 188 | 241 | 344 | 416 |

WebAssembly is the same program run in V8, as a browser client would pay. Heard by two ears, a source costs half of what it costs OpenAL Soft at `low` and `medium`, three quarters at `high`; panned to loudspeakers, twice. The engine keeps 21.8 MB of sound on its way; no part of it runs on a GPU. The prototype's DESIGN.md has the full measurements and what each baseline does and does not do.

Played live (issue #16), on the same laptop while other work loaded it: natively, a thread kept a few blocks ahead of a 10 ms output held low and, in one run of two, medium for the bridge's 45 s with nothing missing; a pose is heard 13 ms after it is told, 23 ms with the output's buffer. In Chromium's AudioWorklet, where a block is made in each render quantum, low costs 13 % of the audio thread with no block over its budget, medium 28 % with 4 blocks of 17 132 over it; a pose is heard within one block plus the output's buffer.

## 5. Server and clients

Nothing in the first batch needs a server. The design keeps three splits open, by what each side can afford:

1. **The client finds the ways and renders them** (what the engine does). The server sends what it sends anyway: where things are. The voices are generators, so no sound is sent and none is read from disk.
2. **The server (or an edge) finds the ways, the client renders them.** A way is seven numbers every few milliseconds (a delay, four gains, a direction). For a client too weak to hold the acoustic geometry this trades a few kilobytes a second for the search. The engine already separates the two stages (`finding` and `carrying` in its tally).
3. **Far sound as a field** (next section): what comes from far is the same for everyone in a place, so it is made once for the place.

The server never mixes for a listener: that would cost it a voice count per player and a round trip of latency.

On a tier that already traces rays for light, the same acceleration structure could find ways; nothing here depends on it.

## 6. Far sound as a field

osage draws what is far from a viewer as holograms: the light arriving at a cube from outside it, baked or streamed, shared by everyone in the cube ([osage-cubes](../knowledge/modules/osage-cubes.md), [osage-hologram-stream](../knowledge/modules/osage-hologram-stream.md)). Sound has the same shape, and a known form for it.

- **Near** (the listener's own cube and its neighbours): emitters, ways, ears — the engine as it is.
- **Far**: the sound arriving at a cube from outside it, kept as a **sound field** at the cube's centre: a few channels that hold the sound by direction (ambisonics; four channels for the first order, nine for the second). It is made once per cube, whoever listens, and each listener turns it to their own head and decodes it to their own ears. A listener who moves inside the cube hears the far field unchanged, as a far light field changes little with a step.
- **Carried** as light is: a stream per cube and level, the larger cubes at a lower order and rate. The research notes give 64 to 128 kbit/s for such a bed with Opus; that figure is from memory and not verified.

What this buys is what holograms buy for light: the cost of the far world no longer grows with the number of sources or of listeners. Built and measured (issue #17, [far sound field](../knowledge/algorithms/far-sound-field.md)): hearing a field costs a listener 16 ms of processor a second at order 1 and 46 at order 2, whether 32 or 1 024 emitters made it; 16 listeners in a cube with 1 024 far emitters pay 1.2 s a second with a field against 15 s by ways. Many far sources lose less than a decibel of loudness and a few of colour; a lone far source is blurred at order 1 and much less at order 2. With Opus, order 1 is transparent from about 53 kbit/s and order 2 from about 105: the research notes' 64 to 128 kbit/s holds. The bridge with what is beyond 60 m as a field still passes its checks.

A voice heard from far needs less of itself, as a texture seen from far needs fewer pixels: voices already take a `detail` that lowers with distance; recorded voices and audio levels of detail are issue #22.

## 7. Open questions

- Where the modes of a contact are set: on the object (from its size and shape) or on the material, and how a collision material names its sound (§2).
- When emitters and contact sounds move into the object format, and in which version of it (issue #15).
- How the engine is compared with the city programme's external reference: like-for-like measurements and a blind listening test (issue #19).
- The engine's limits, each documented with what removing it would cost (issue #21).
