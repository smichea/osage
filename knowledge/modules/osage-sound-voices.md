---
type: Module
title: osage-sound-voices
description: The voices osage-sound places in a scene, each made sample by sample from the state of what makes it — synthesised (a river against stone, cars after the European road-noise model, a scooter, a rubber-tyred metro and its steel viaduct, footsteps, the city far off, contacts that set ringing what was struck) or played from recordings a world gives, at levels of detail for afar — deterministic.
resource: https://github.com/smichea/osage/tree/main/crates/osage-sound-voices
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [sound, synthesis, procedural-audio]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-24T09:00:00Z" }
---

# Purpose

Give every emitter of [osage-sound](/modules/osage-sound.md) a sound made from what it is doing — its speed, its acceleration, its load, a blow — synthesised, so that no audio file need be read or sent (a scene of the Pont de Bir-Hakeim is 24 kB of JSON and no sound file), or from recordings a world gives as content. A voice says what is heard one metre from its emitter; where it is heard from, and through what, is the engine's business.

# Concepts

- **Drive:** what a voice follows, given once a block: `speed` (m/s), `accel` (m/s²), `load` (0…1: how much it is excited — a span by the bogies on it), `hit` (a blow in this block: when and how strong) `detail` (0…1: how much of its detail is worth making — 1 close and alone, less far off) and `top` (the highest frequency still worth making: what the air has not taken).
- **Voice:** `render(drive, out)` writes the next samples at 48 kHz: an RMS of `NOMINAL_RMS` (0.1) when driven as its documentation says, every sample finite and within −1…1.
- **Kind:** a voice a scene can name, its one-line description and its parameters with their defaults; `make(name, seed, params)` builds one.
- **Seed:** every noise is a seeded generator; there is no clock. The same seed and drive give the same samples.

# Architecture

Each kind lists its own assumptions in its module:

- **river** — a broad river against stone: a noise bed that swells, bubbles as short sines, the shallowest rising (after van den Doel, "Physically-based models for liquid sounds", 2005), a lap now and then as the water stirs — none on still water.
- **car** — tyres and engine shaped to the octave levels of the CNOSSOS-EU road source (Directive (EU) 2015/996) at its speed; a four-cylinder engine, its firing note over the rumble of its exhaust, or an electric whine. **scooter** — the 50 cc two-stroke of [osage-kart-sound](/modules/kart-sound.md) on its variator, without the kart's wind.
- **metro-bogie** — a rubber-tyred bogie rolling: tyre noise through the shape of a heavy vehicle's rolling noise, a low body, guide-wheel ticks. **metro-motor** — two GTO inverters whose tone steps through pulse modes as the motor speeds up, the gear mesh, the casing, a fan. **metro-joint** — a runway joint crossed: a thump and two muted knocks. **metro-span** — a span of the steel viaduct: six resonant modes fed by noise as the train loads it, ringing down when it leaves, and a rattle on the peaks.
- **footsteps** — heel then sole on asphalt, at the pace of the walker's speed. **city-bed** — distant traffic all round, drifting slowly.
- **impact** — two things touching: a short push (a raised cosine of 0.3 ms hard on hard to 6 ms for a sole), set by the softer of the two, rings up to six modes at the ratios of a slab, a plate or a bar, each decaying faster the higher it is (one damping number per material); a scuff of band-passed noise; a crackle for what is brittle. Both things sound, each a voice of its own: a hammer on an anvil is the anvil ringing and the hammer's head ticking. Preset matters: `ANVIL`, `HAMMER`, `PAVING`, `LEATHER` and others.
- **tone**, **noise** — references whose pitch and level are known, to measure a way by.
- **recorded** — not a kind a scene names by itself: loops that follow a speed or a load (`Loop`) and one-shots drawn for each blow (`Shot`), from `Clip`s a world gives, over a synthesised `Layer`, each clip kept at four levels of rate played by what the air leaves ([recorded voices](/algorithms/recorded-voices.md)).

Filters are state-variable band-passes and one-pole low-passes; what changes slowly is set every 32 samples. Under a `detail` of a half, a voice makes a cheaper sound of the same level and colour (fewer bubbles, fewer partials, no guide wheels), and must not click when `detail` changes.

The example `audition` renders each voice alone to a WAV file, driven as the scene would drive it, to listen to it without the bridge.

# Interfaces

`Drive`, `Hit`, `Voice`, `Params::{with, set, get, names}`, `Kind`, `KINDS`, `kind(name)`, `make(name, seed, params)`, `RATE`, `NOMINAL_RMS`; `contact::Matter` and its presets, `Matter::touched_by`; `dsp::{Noise, seed_for}`; `recorded::{Clip, Loop, Shot, Layer, Follows, holds, level_for, LEVELS, FEW, SOLO}`.

# Invariants

Tested in each module (about a hundred tests), and reviewed independently ([research 12](https://github.com/smichea/osage/blob/main/research/12-sound-voices-review.md)):

- Every kind sounds within bounds and the same twice from a seed, and another seed gives another sound; the length of the blocks changes nothing; no change of drive or of detail makes a step. A parameter that is not a finite number is its default, and a blow of strength NaN is no blow.
- The metro's bogie, motor and span are at the nominal level within 0.5 dB, the joint peaks at a half, all fall silent at rest, and the motor pushes no way for long (no offset).
- A car at a steady 30 km/h sounds at the nominal level, and no line holds half its power; each octave is at the road-noise model's level, which gives 97 dB at 30 km/h and two more at 50; the engine fires twice a turn and the driver changes up as the car gains speed; an electric car is its tyres and a faint whine.
- A bubble rings at three over its radius and dies as the paper says; less detail makes fewer bubbles and keeps the level and colour. Still water does not lap, a wake laps twice as often, and on an ordinary day the limiter is seldom at work.
- A walker steps once every step length, heel then sole, varying in time by up to 8 %.
- Nothing sounds until something is struck, and then it dies; steel rings and asphalt thuds; a contact tells what both things are made of.
- The city drifts slowly by 2 dB, never comes round again, and has the weight of distant traffic.
- A recorded loop follows its state with no click and the same twice; each level of a clip holds what is under its top and nothing above; a shot plays from its blow, as loud as it was hard, and draws its clip ([recorded voices](/algorithms/recorded-voices.md)).

# Knowledge

- Used by [osage-sound](/modules/osage-sound.md); measured with [osage-sound-analysis](/modules/osage-sound-analysis.md).
- Listened to on 2026-11-24 (issue #20): the scooter, the bogie, the runway joint, the footsteps alone and the city bed sound right; the car alone sounds a little like a propeller aircraft's engine; the metro's traction sounds like a motorbike shifting gears; the default contact sounds like a hand on a thin metal sheet; the river is exaggerated. Recordings and levels of detail came with issue #22 (D-245). After the review (issue #20), the river laps less and chirps less, the car rumbles under its firing note, and a contact's scuff no longer outlasts its ring; to be heard again.

# Reuse

Any engine that can give a voice a speed, a load and a blow can use these. The contact model — one push set by the softer thing, modes per object, damping per material — fits any physics that reports contacts with their strength.

# Limits

- Synthesis does not sound recorded; several voices are judged unrealistic by ear (see above). Recorded voices need content: the recorded bridge borrows the kart's loops and knocks.
- The metro's spectrum is a proxy: no public measurement of a rubber-tyred train on a steel viaduct was found.
- Contact modes are set by hand on the collision material; they belong to the object's size and shape.
- Mono, at 48 kHz only.
