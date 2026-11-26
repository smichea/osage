---
type: Algorithm
title: Recorded voices and their levels of detail
description: Voices played from recordings a world gives as content — loops that follow a speed or a load, crossfaded and played faster or slower between the clips recorded at known values, and one-shots drawn from a set for each blow — over a synthesised layer, each clip kept at levels of half, a quarter and an eighth of its rate like a texture's mipmaps, the coarsest that still holds what the air leaves played from afar, with nothing heard at a change.
resource: https://github.com/smichea/osage/blob/main/crates/osage-sound-voices/src/recorded.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [sound, sampling, level-of-detail, resampling]
sources:
  - id: kart
    resource: "osage-kart-sound: recorded loops following a kart's engine speed, synthesis for the rest (D-058, D-070)"
    title: osage-kart-sound
  - id: iso9613
    resource: "ISO 9613-1:1993, Attenuation of sound during propagation outdoors, part 1: absorption by the atmosphere"
    title: ISO 9613-1
  - id: own
    resource: "osage's own design, issue #22, 2026-11-26"
    title: osage design
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-26T04:00:00Z" }
---

# Problem

Every voice of [osage-sound](/modules/osage-sound.md) was synthesised. A world may want to give a car, a scooter or a footstep a recorded sound as content, as the kart already does[^kart]. A game also has to spend less on what is far, where the air has taken the top of a sound and nobody can tell detail apart. It should play a far sound more simply, as a renderer draws a far texture from a smaller mipmap, with no change heard.

# Idea

**Loops.** A loop follows a state from a few clips, each recorded at a known value of the state. The two clips either side of the state are crossfaded, and each is played faster or slower so that its note follows the state[^kart].

**One-shots.** A one-shot set plays one clip of the set for each blow, drawn from the voice's seed.

**Layers.** Either sounds over a synthesised layer, for what was not recorded.

**Levels of detail.** Each clip is kept at its own rate and at each half of it. The engine tells the voice the highest frequency still worth making, from the air between[^iso9613], and the voice plays the coarsest level that still holds it. Far off, the voice also plays one loop where the other is almost silent, one shot of its set, and no layer. Every change is eased[^own].

# Method

**A clip** is mono.

- If it is at no level's rate, it is first read to the next level up by four-point Lagrange interpolation.
- Each coarser level is made from the one above with a Blackman-windowed sinc low-pass (63 taps, cut at a quarter of the rate), then every other sample is kept. A loop wraps round; a shot is silent outside itself.
- Level `k` is at `48 kHz / 2^k`, four levels in all. It holds up to `0.45 · 48 kHz / 2^k`: 21.6, 10.8, 5.4 and 2.7 kHz.
- A clip given at 12 kHz starts at level 2 and has no finer level.

**A loop voice.** Each clip has the value it was recorded at, of the speed (m/s) or of the load.

- **Crossfade.** For a state `x` between two clips' values, at fraction `t`, the clips are crossfaded at equal power: `cos(t·π/2)` and `sin(t·π/2)`. Below the first clip or above the last, that clip plays alone.
- **Speed.** A clip recorded at `a > 0` plays at `clamp(x/a, 0.5, 1.25)` times its own speed. One recorded at rest plays at its own speed.
- **Level.** Each clip is brought to an RMS of 0.1, the voices' nominal level, so the level holds between two clips.
- **Start.** Each clip starts at a place drawn from the seed, so two emitters of the same clips are not in step.
- **Easing.** Shares are eased by at most one part in 2 400 a sample (50 ms from nothing to all).
- **Unheard clips.** A clip that is not heard goes on in time all the same. Where a loop is never depends on what was heard of it, so a change of detail changes nothing but what is made.

**A shot voice.** On a blow, it picks a clip by `hash(seed, "blow n") mod size`, or the first with little detail.

- The clip plays from the blow's sample at its strength, times a gain that brings the clip's loudest 50 ms to an RMS of 0.1. That is as loud as the synthesised `impact` blow at its loudest (0.07 to 0.12).
- Two shots sound at once at most. A third takes the oldest one's place.

**A layer** is a synthesised voice at a gain. Under a detail of 0.35 it fades out over 50 ms, and back in above.

**Levels of detail.** The engine puts two figures in each block's drive:

- **`top`**: the frequency at which the air takes 24 dB on the straight way. It comes from a table of ISO 9613-1 at the scene's temperature and humidity, at a sixth of an octave from 500 Hz, interpolated on a scale of octaves.
- **`detail`**: `clamp(25 m / distance, 0.15, 1)`.

At reference quality, both are full (`top` 24 kHz, `detail` 1).

The voice then:

- plays, for each clip, the coarsest level `k` with `holds(k) · speed ≥ 1.2 · top`, crossfading by amplitude over 50 ms when `k` changes (two levels of a clip are the same sound below the coarser one's top);
- with a detail under 0.35, leaves out the quieter of two loops when its share is under 0.056 (−25 dB), plays the first shot of a set for every blow, and drops its layer.

**Content.** A scene names its clips under `asset.extras.osageSound.clips` (`uri` from the assets' root, `loop`). An emitter or a contact sound plays them with `recorded`: `clips`, `follows`, `layerGainDb`. `Scene::load_clips(root)` reads Ogg Vorbis (with `lewton`) or WAV files and mixes their channels. A browser can give them with `Scene::set_clip`. A recording that was not read leaves the synthesised voice alone, or silence when there is none.

# Invariants

Tested in `crates/osage-sound-voices/src/recorded.rs` and `crates/osage-sound/tests/recorded.rs`:

- Each level holds a tone below its top within 1 %, and keeps less than a thousandth of one above it. A clip at 44.1 kHz is brought to 48 kHz with its periods kept.
- A loop swept through its state's range is the same twice, never steps more than its highest note at its loudest makes from one sample to the next (no click), holds its level within 8 % between two clips, and its note follows the state.
- From afar the loop goes to its coarsest level and to one loop with no step. A clip made at 12 kHz holds a quarter of the bytes, and plays as the 48 kHz clip's level 2.
- A shot plays from the blow's sample, its loudest 50 ms at the strength's share of the nominal level. Blows are not all the same clip near, and all the first from afar.
- In the engine:
  - a recorded 1 kHz tone is heard as the synthesised one, within 1 %, the same pitch, the same samples twice;
  - recordings not read leave their voice or silence; read, the layer's power adds to theirs;
  - a recorded knock is heard where and when a wheel strikes the plate.
- From afar, at high quality, a noise loop 300 m off plays a coarser level and is within 0.5 dB of the whole recording in every third of an octave under 4 kHz. Passing from 450 m to 450 m at 108 km/h, it is within 1 dB of it a tenth of a second at a time, under 3 kHz.

`paris-soundscape lod` measures the kart's engine loops as the recorded bridge has them. The comparison is with the same loops at full detail, a tenth of a second at a time, by third of an octave below what the air leaves, and within 50 dB of the loudest third:

- **Levels of rate alone:** within 1 dB at every distance from 10 to 880 m.
- **One loop instead of two, the other 27 dB down:** within 2.5 dB. Left out at −16 dB, the other changed a third by up to 10 dB: two recordings differ in colour.
- **Loops shipped at 12 kHz:** within 2.5 dB beyond 600 m.

# Cost

A loop voice is about 200 ns a sample with two loops crossfaded, and 100 with one, whatever level it reads. A level of rate saves no processor time, since the voice makes 48 000 samples a second whatever it reads them from.

Memory: a clip with its four levels holds 1.9 times its own bytes. A clip shipped at a lower rate holds as many fewer.

On the recorded bridge at high quality, levels of detail cut the voices' time from 94 to 79 ms a second of sound. This counts the synthesised voices' own detail, and measures within the noise on the whole render (673 against 689 ms).

# Failure modes

- Two recordings differ in colour: with little detail, a loop is left out only when it is 25 dB down, which saves its cost only near a clip's own state.
- The recorded bridge plays the kart's dirt-bike loops for the scooter and plastic knocks for the steel plate: the mechanism is shown, not the right sounds.
- A shot's transient may peak higher than the synthesised blow at the same loudness. The recorded bridge's knocks peak 9 dB higher, which holds the render 0.8 dB under the loudness the scene asks.
- Reading a coarser level saves no processor time. The steps of a crowd are not yet merged into one texture.

# Alternatives

- **Granular playback** (as the kart's engine does, D-070) hides a short loop's period. It costs more grains a sample.
- **Rendering far voices at a lower rate and upsampling once** would save processor time, at the cost of a second path through the engine.
- **Streaming the levels by distance** would save memory and bandwidth where a world sends sound.

# Oracle

The tests named above; `paris-soundscape lod` (with the assets fetched) for the tables.

# Reuse

Any engine with sampled voices: a clip kept as its mipmaps, the level picked from what the air leaves, and loops that keep going while unheard, so that a change of level changes nothing but what is made.

[^kart]: osage-kart-sound
[^iso9613]: ISO 9613-1
[^own]: osage design
