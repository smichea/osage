---
type: Algorithm
title: Direct sound through city surfaces
description: Trace a contact to its listener and apply the intersected materials to the voice before stereo rendering.
resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/impact/transmission.rs
licence: MIT OR Apache-2.0
status: draft
tags: [paris, sound, geometry]
sources:
  - id: materials
    title: Physical and acoustic bindings in the object profile
    resource: https://github.com/smichea/osage/blob/main/design/08-object-format.md
  - id: prototype
    title: Bridge scene with authored acoustic quadrilaterals
    resource: https://github.com/smichea/osage/blob/main/tools/paris-soundscape/scenes/bir-hakeim.json
  - id: face
    title: Finite segment test on a sound-engine quadrilateral
    resource: https://github.com/smichea/osage/blob/main/crates/osage-sound/src/ways.rs
generated: { by: "OpenAI/Codex", at: "2026-11-24T07:09:38Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A pedestrian strikes the same slab at the same speed. One observer has a clear view of the impact. Another hears it through a panel whose material transmits different amounts of low and high frequency sound.

# Idea

Trace the direct wave from the contact to the camera at reception. Each intervening acoustic surface contributes its three transmitted energy fractions. Their product determines the pressure that reaches the listener. [^materials]

# Method

The GLB profile selects the active acoustic bindings from the object tree. Each binding owns its transformed triangles or names a physical mesh as its proxy. The audio track prepares a bounding volume hierarchy over nonempty surfaces. A finite segment queries this hierarchy and then the lazily built triangle trees of the candidate surfaces. A double-precision, two-sided triangle test resolves candidate intersections. The nearest interior hit for each binding supplies its distance and owner path. Independent panels have independent binding IDs. The object inspector uses the same finite triangle primitive, includes endpoints and reports every crossed face in each selected channel. [Geometry query](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/object/acoustic.rs)

The bundled Bir-Hakeim sector also has a companion sound scene with quadrilaterals on named nodes in the same coordinates. The track evaluates their positions at reception and calls the shared quadrilateral segment test on faces marked as occluding.[^face] The two surfaces of the upper deck each carry the steel deck material. A custom legacy sector needs its own acoustic bindings to supply a material response. [^prototype]

The three authored energy fractions refer to 250, 1,000 and 4,000 Hz. Across different bindings the renderer multiplies the fractions, then takes their square roots to get pressure gains. Uniform pressure gain is a scalar. Unequal gains use two eighth-order Linkwitz–Riley crossovers at 500 and 2,000 Hz, each built from the biquad implementation in `osage-sound-voices`. Both contributions use both crossovers so their phases agree. The filtered mono signal feeds the existing speaker or headphone output. The physical arrival and the first nonzero 16-bit sample are recorded separately. [Biquads](https://github.com/smichea/osage/blob/main/crates/osage-sound-voices/src/dsp.rs)

# Invariants

The contact, emission time and wave arrival use the same clocks for every material. Unknown material responses keep unit gain and appear in the receipt. A shared triangle edge and a closed mesh under one binding contribute one transfer. Contacts within 0.1 mm of a segment endpoint do not obstruct their own impact. An empty ray preserves the original samples and a fully opaque sheet silences the direct signal. The filter keeps its state across audio blocks.

# Cost

Each placed binding builds its triangle tree when an audio query first reaches its outer bounds. Paged copies of the binding share that tree. The tree visits candidate boxes and tests the corresponding faces. It sorts intersected surfaces by range and identifier. Adding 300 out-of-path panels to the included test leaves the number of exact triangle tests unchanged. Each coloured voice uses sixteen biquads per sample. A clear path bypasses them.

# Failure modes

A surface needs a measured or authored transmission to attenuate sound. Two separate panels under a single acoustic binding share one material transfer. Use separate bindings for independent layers. The three bands approximate the continuous spectrum. The crossover has finite rejection, and the 16-bit recording can mask weak components. A fully blocked direct ray can have audible indirect paths in the physical world. The impact track traces a separate path around one or two screen edges. Reflected arrivals and reverberation would add further paths. [Hearing an impact around city obstacles](/algorithms/diffracted-city-contacts.md)

# Alternatives

Both renderers read the material transmission as three energy fractions. The general sound engine converts each to a pressure gain and copies the 4 kHz gain into a fourth, 9 kHz band. It can blend transmission with a path around the edge of a quadrilateral. Ville-Paris applies its three-band crossover to the direct strike and schedules any diffracted strike separately. [Sound ways](/algorithms/sound-ways.md) · [Sound delay lines and four-band shaping](/algorithms/sound-delay-lines.md)

# Oracle

Record the same solved contact through clear, unknown, transparent, opaque and frequency-dependent surfaces. Compare an acoustic proxy with an authored sheet and change the panel placement, leaving the contact fixed. Check the received energy in three twelfth-octave bands. A second recording at half the listener distance raises the dry signal level for the three single-frequency PCM measurements. A Rust test also measures the tonal gains before quantization. Cross the two authored faces of the upper deck in the bundled city. [Replay oracle](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-impact-transmission.py)

# Reuse

Run `python3 worlds/ville-paris/check-impact-transmission.py --bin target/release/ville-paris` to record the paths and their receipts. In an `--impact-wav` report each onset contains `direct_path.crossings`, the number of candidates tested and the combined energy fractions.

[^materials]: [Object format](https://github.com/smichea/osage/blob/main/design/08-object-format.md).
[^prototype]: [Bridge scene](https://github.com/smichea/osage/blob/main/tools/paris-soundscape/scenes/bir-hakeim.json).
[^face]: [Quadrilateral crossing](https://github.com/smichea/osage/blob/main/crates/osage-sound/src/ways.rs).
