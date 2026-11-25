---
type: Algorithm
title: Measured ears through virtual loudspeakers
description: Headphones heard through a measured head (SADIE II's KEMAR) at a cost that does not grow with the ways — each way keeps its own time between the ears from a measured table and is shared among the three nearest of 54 virtual loudspeakers in rings, whose two measured responses, cut short with their onsets taken out, are applied once per loudspeaker.
resource: https://github.com/smichea/osage/blob/main/crates/osage-sound/src/hrir.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [sound, binaural, hrtf, virtual-loudspeakers, headphones]
sources:
  - id: sadie
    resource: https://doi.org/10.3390/app8112029
    title: C. Armstrong, L. Thresh, D. Murphy and G. Kearney, A Perceptual Evaluation of Individual and Non-Individual HRTFs, a Case Study of the SADIE II Database, Applied Sciences 8(11), 2029, 2018
  - id: pulkki
    resource: https://doi.org/10.17743/jaes.1997.0001
    title: V. Pulkki, Virtual sound source positioning using vector base amplitude panning, JAES 45(6), 1997
  - id: own
    resource: "osage's own design and measurements, issue #18, 2026-11-25"
    title: osage design
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-25T02:40:00Z" }
---

# Problem

The engine's ears for headphones were a sphere and five echoes (the structural model of Brown and Duda, see [sound delay lines](/algorithms/sound-delay-lines.md)): left and right were right, but height was weak and a sound behind was the mirror of one in front. Measured head responses tell those apart. Convolving every way with its own measured response would cost in proportion to the ways — up to 128 at high quality.

# Idea

Hear the ways through a fixed set of virtual loudspeakers round the head, each filtered by the measured responses of its direction, once for all the ways. The time between the ears, which panning between loudspeakers would blur, stays a delay of each way's own, read from the measured set; the loudspeakers' responses have it taken out[^own].

# Method

**The set.** Subject D2 (a KEMAR) of the SADIE II database: 8 802 directions, 256 taps at 48 kHz, diffuse-field equalised[^sadie]. `tools/make-hrir.py` takes its responses and directions out of the SOFA file with `h5dump`, and `paris-soundscape hrir` derives a file of 38 kB (`hrir::derive`):

- **The time between the ears** of each measured direction: where the two responses' low frequencies (one pole at 1.5 kHz) agree best within 1 ms, to a part of a sample. Kept on a grid of 5° in azimuth and elevation (each point from the nearest measured direction), read by bilinear interpolation.
- **The loudspeakers:** 54 in rings, as alike on the left and on the right — 16 on the horizon, 12 at ±35°, 6 at ±70°, the two poles — each moved to the nearest measured direction. Their triangles are the convex hull.
- **Their responses:** both ears cut to 64 taps, from 10 taps before the nearer ear's peak, the farther ear's moved earlier by the time between them, so that the pair has none of its own; faded at both ends (raised cosine over 4 and 16 taps); scaled so that a sound from ahead keeps its energy at each ear.

**Hearing a way** (with headphones, at the qualities that make the outer ear — high and reference — when a client gives the file):

1. Its direction in the head's frame. Its delay at each ear: the far ear late by the table's time between the ears, the near one not; no shadow and no echoes of the outer ear, which the responses carry.
2. Its share of the loudspeakers: the triangle of the hull its direction falls in (the one where its least vector-base weight is greatest[^pulkki]), the three weights divided by their sum: with the onsets taken out, the three responses mixed by weights that sum to one are the response between them. When the share changes, the old three fade out over the block as the new three fade in.
3. Each ear's delayed signal is added, times its weights, to that ear's feed of each loudspeaker.
4. After all the ways, each loudspeaker that was fed (or is still ringing) is filtered by its two responses into the ears: tap by tap over the block, sums of products that a processor does several at once.

The far field's own virtual loudspeakers ([far sound field](/algorithms/far-sound-field.md)) are still heard through the sphere model.

# Invariants

- With a set whose responses are known (an impulse, 1.5 against 0.5 from the side, 600 µs between the ears), a noise from the right is heard 600 µs sooner and 9.5 dB louder at the right ear (at 1 kHz, within 0.3 dB and 25 µs), from ahead with nothing between the ears, and between loudspeakers with the table's time (tested).
- Without the file, for loudspeakers, and at low and medium, not a sample changes (tested); the bridge's digests are unchanged.
- The file goes there and back; a cut or false file is refused (tested). The rings are their own mirror; the hull's triangles cover the sphere and every share sums to one (tested).
- `paris-soundscape verify --hrir` passes, its mirror check read octave by octave (within 0.5 dB from 250 Hz to 4 kHz) since a measured head is not its own mirror.

# Knowledge

A noise 3 m away round the horizon (every 30°) and from below to above ahead and behind (`tools/make-hrir.py --check`, reference quality), against the measured set's own full responses at the nearest measured direction, 2026-11-25:

| | Between the ears: time | level 250 Hz | 1 kHz | 4 kHz | 8 kHz |
|---|---:|---:|---:|---:|---:|
| Worst difference from the measured set | 53 µs | 1.3 dB | 2.7 dB | 1.4 dB | 9.7 dB |

The large differences at 8 kHz are at directions between loudspeakers (22.5° to 36° apart), where the measured responses change faster than three of them mixed can follow. Ahead against behind, the octaves' levels at both ears (behind minus ahead, 125 Hz to 8 kHz):

| | 125 | 250 | 500 | 1k | 2k | 4k | 8k |
|---|---:|---:|---:|---:|---:|---:|---:|
| Measured set, 0° → 180° | −0.2 | −0.9 | −1.1 | 1.4 | −2.0 | −3.3 | −6.0 |
| Virtual loudspeakers | −1.4 | −1.5 | −1.1 | 0.5 | −2.0 | −3.3 | −5.8 |
| Sphere model | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

At 30° → 150° the measured set gives −2.8, −6.6 and −3.1 dB at 2, 4 and 8 kHz, the loudspeakers −3.1, −6.2 and −5.4. Nobody has listened to it yet.

# Cost

Each way costs less than with the sphere model (no shadow filters, no echoes of the outer ear) plus three additions into loudspeaker feeds; each loudspeaker fed costs two filters of 64 taps per sample, whatever the number of ways. The bridge, headphones (`paris-soundscape bench --part bridge --quick`, under a load of 10 to 12 from other sessions, 2026-11-25), ms of one processor for a second of sound: high 486 with the sphere model, 628 with the measured responses; reference 732 and 819. The file is 38 kB.

# Failure modes

- Loudspeakers 22.5° to 36° apart blur what changes faster across the sphere: the top octave between them is off by up to 10 dB, and a sound moving across a loudspeaker's triangle changes colour a little as its share moves.
- One head (a mannequin's): what tells up from down differs from one person's ears to another's.
- The set is measured at 1.2 m: nothing of how a head hears a sound much nearer.
- The responses are cut to 64 taps (1.3 ms): what the set holds after that (little, for a diffuse-field equalised set) is lost.
- The reading of each ear's delay between samples is the line's (cubic at reference): it takes a little of the top from the farther ear.

# Alternatives

A short convolution for each way with responses interpolated for its direction is exact where the loudspeakers blur, at a cost that grows with the ways. More loudspeakers (or a higher order of an ambisonic decoder to them) sharpen the top octave at a cost that grows with them. A spherical-harmonic representation of the set (binaural decoding of an ambisonic mix, magnitude least squares) would share the field's decoder but blurs the time between the ears unless it is kept apart, as here.

# Oracle

`crates/osage-sound/src/hrir.rs` (tests), `crates/osage-sound/tests/hrir.rs`, `tools/make-hrir.py --check` (`paris-soundscape ears`), `paris-soundscape verify --hrir assets/sadie/d2-48k.hrir`.

# Reuse

Any engine that renders many ways to headphones: keep each way's time between the ears as its own delay, take it out of the measured responses, and pan the ways between virtual loudspeakers filtered once each.

[^sadie]: C. Armstrong, L. Thresh, D. Murphy and G. Kearney, A Perceptual Evaluation of Individual and Non-Individual HRTFs, a Case Study of the SADIE II Database, Applied Sciences 8(11), 2029, 2018
[^pulkki]: V. Pulkki, Virtual sound source positioning using vector base amplitude panning, JAES 45(6), 1997
[^own]: osage design
