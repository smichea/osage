---
type: Algorithm
title: Far sound field
description: What is far from a cube heard as an ambisonic field made once at the cube's centre — the far emitters' ways written into 4 or 9 channels by direction, split from the near ones at equal power across a distance from the centre — and decoded for each listener through virtual loudspeakers turned with the head, in two bands, so that the far world costs a listener the same whether it holds 32 emitters or 1 024.
resource: https://github.com/smichea/osage/blob/main/crates/osage-sound/src/field.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [sound, ambisonics, level-of-detail, holograms]
sources:
  - id: gerzon
    resource: "M. Gerzon, Periphony: with-height sound reproduction, JAES 21(1), 1973"
    title: Periphony
  - id: zotter-frank
    resource: https://link.springer.com/book/10.1007/978-3-030-17207-7
    title: F. Zotter and M. Frank, Ambisonics (Springer, 2019, open access)
  - id: own
    resource: "osage's own design and measurements, issue #17, 2026-11-24"
    title: osage design
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-24T12:00:00Z" }
---

# Problem

A city holds thousands of sound sources. Following every one by its own ways to every listener costs each listener in proportion to the sources, and a server or a cube full of players pays that again for each. Yet what is far is heard by its direction and its colour, not by its detail, and it is nearly the same for everyone standing in the same place: osage already draws far light once per cube as a hologram ([osage-cubes](/modules/osage-cubes.md)).

# Idea

Gather what is far as a sound field at the cube's centre: a few channels that hold the sound by direction, the spherical harmonics of first or second order[^gerzon]. Make it once per cube, whoever listens. Each listener turns it to their own head and decodes it to their own ears; what that costs depends on the field's order, not on how many emitters made it[^own].

# Method

**Channels.** Real spherical harmonics in ACN order with SN3D normalisation (AmbiX)[^zotter-frank], in the field's own axes: ahead (x), left (y), up (z). For a unit direction (x, y, z): W = 1; first order Y = y, Z = z, X = x; second order √3·xy, √3·yz, ½(3z² − 1), √3·xz, (√3/2)(x² − y²). Four channels at order 1, nine at order 2.

**The boundary.** An emitter at distance d from the cube's centre is far by `s = smoothstep((d − 0.85·B) / (0.15·B))` for a boundary B. The near engine hears it by `√(1 − s)`, the far engine by `√s`: equal power, since the two renders are not the same sound. The rule depends on the cube's centre, not on the listener, so every listener in the cube agrees on what is far.

**Making the field.** The far engine is the ordinary engine with its "head" at the cube's centre along the world's axes. Each way is found and carried as usual (its delay, Doppler, bands, air), but instead of the ears its signal is added into each channel times that channel's harmonic in the way's direction, the gains moving evenly across the block. The reverberation's output, which comes from all round, goes into W. What is heard from nowhere stays near.

**Hearing it.** Virtual loudspeakers evenly spread round the head: an octahedron (6) at order 1, an icosahedron (12) at order 2, spherical designs of degree 3 and 5. Each block:

1. Turn: each loudspeaker's direction in the head's frame is taken into the world by the head's axes, and its harmonics evaluated there.
2. Part each channel at 700 Hz with a one-pole low-pass (the rest is the channel minus that).
3. Low band, the basic sampling decoder: `g_s = Σ_c (2n_c + 1)·h_c(s)·F_c / S`. Its feeds of a sound sum to the sound itself, which keeps the level and the time between the ears where the feeds reach an ear in step.
4. High band, the max-rE decoder: each order weighted by `P_n(cos(137.9° / (N + 1.51)))`, the whole scaled to keep energy — and, because the head model hears loudspeakers with the same lateral angle alike (they add in step), scaled again so that the energy reaching the ears with those groups summed is right on average over 256 directions.
5. Hear each loudspeaker through the engine's own ears ([sound delay lines](/algorithms/sound-delay-lines.md)): for headphones its fixed ear delays, shadow and outer ear; for loudspeakers its pan.

**Sending it.** A field is a few channels of ordinary audio: Opus has an ambisonic channel mapping (family 2).

# Invariants

- With nothing far, the near engine with a field gives the same samples as without one (tested at both orders).
- A far noise 300 m to the right stays on the right through the field, within 2 dB of its level by ways (tested at both orders).
- What a listener pays to hear the field is the same for 8 and 256 emitters (tested); below the crossover a decoder's feeds of a sound sum to it, a sound is loudest in the loudspeaker it comes from, and turning the head turns where it is heard (tested).
- The bridge with what is beyond 60 m as a field passes [soundscape verify](/oracles/soundscape-verify.md) at both orders: the river stays on the left, the train passes on the right; turning the head mirrors the ears within −140 dB.

# Cost

On the Intel Core i5-1335U, medium quality, headphones, 2026-11-24, while other sessions loaded the machine (load 5 to 7), ms of processor for a second of sound (`paris-soundscape far --sphere N`):

| Emitters far | By ways, per listener | The field, once (order 1 / 2) | Hearing it, per listener (order 1 / 2) | 16 listeners: ways / field order 1 |
|---:|---:|---:|---:|---:|
| 32 | 111 | 82 / 107 | 16.9 / 45 | 1 777 / 352 |
| 256 | 303 | 264 / 291 | 16.9 / 46 | 4 841 / 534 |
| 1 024 | 945 | 925 / 1 014 | 16.3 / 46 | 15 119 / 1 186 |

Making the field costs about what one listener pays by ways (the voices dominate); hearing it is constant. With one listener there is nothing to gain; from a few listeners in a cube, or when the field is made elsewhere and sent, the far world's cost stops growing with them.

Sent with Opus (ffmpeg's libopus, mapping family 2), a field of 64 far noises, heard back through the decoder against the field unsent (`bench/field_opus.py`): order 1 is within 0.15 dB in every octave and between the ears from 64 kbit/s asked (53 taken); order 2 from 128 kbit/s asked (105 taken). Below, Opus cuts the top octaves (45 dB and more at 8 kHz).

# Failure modes

Measured against every emitter by its own ways (`paris-soundscape far`, medium, headphones):

- **One far source alone** is blurred: at order 1 its level between the ears is off by up to 14 dB at 2 kHz and its time by 400 µs; order 2 brings this to 2.6 dB up to 4 kHz (6 dB at 8 kHz) and 190 µs.
- **Many far sources** (64 on a sphere at 150 m): loudness within 0.8 LU (order 1) and 0.5 LU (order 2), octaves within 2.5 dB, level between the ears within 2.9 dB.
- **The bridge**, beyond 60 m: loudness 1.1 LU (order 1) and 0.9 LU (order 2) quieter, octaves within 1.4 dB, level between the ears within 1.3 dB.
- A boundary too near pulls a near source into the field: the bridge's river at 30 m loses most of its side (−1.5 dB instead of −3.9 dB), and `verify` fails.
- The field is made at the centre: a listener elsewhere in the cube hears far sound from the centre's place, without parallax; crossing to another cube changes the field (a crossfade between the two is not built).
- The far reverberation is one channel (W): it loses the decorrelation of the near one.

# Alternatives

Higher orders (16 channels at order 3) sharpen a lone source at the cost of more channels and loudspeakers; hearing the virtual loudspeakers through the measured head responses the near ways now use ([measured ears](/algorithms/measured-ears.md)) would give height and front and back, which the sphere model cannot. A per-cube mix of far sources to a stereo bed would be cheaper but would not turn with the head. Clustering far sources into a few virtual ones keeps them as ways at a cost that grows with the clusters.

# Oracle

`crates/osage-sound/tests/field.rs` and the module's tests; `paris-soundscape far` (differences and cost), `paris-soundscape verify --far-beyond 60 --far-order N`, `tools/paris-soundscape/bench/field_opus.py` (bit rate).

# Reuse

Any engine that renders sources to a listener can render the far ones to a point instead and share the result: the equal-power boundary fixed by the cube, the two-band decoder, and the correction for loudspeakers a simple head model cannot tell apart carry over.

[^gerzon]: Periphony
[^zotter-frank]: F. Zotter and M. Frank, Ambisonics (Springer, 2019, open access)
[^own]: osage design
