---
type: Algorithm
title: Reading a moving sound at a fractional delay
description: Reconstructs audio between the samples of an emitter's history as the length of a sound path changes.
resource: https://github.com/smichea/osage/blob/main/crates/osage-sound/src/engine.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [audio, propagation, doppler]
sources:
  - id: interpolation
    title: Julius O. Smith III, Physical Audio Signal Processing, Relation of Lagrange to Sinc Interpolation
    resource: https://www.dsprelated.com/freebooks/pasp/Relation_Lagrange_Sinc_Interpolation.html
  - id: regression
    title: Osage sound propagation tests
    resource: https://github.com/smichea/osage/blob/main/crates/osage-sound/tests/physics.rs
generated: { by: "OpenAI/Codex", at: "2026-11-24T07:10:31Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A moving emitter produces a different delay at each instant of reception. Rounding that delay to a whole sample introduces new tones. Even a reader that reproduces the correct Doppler pitch can leave audible-band spectral images around it.

# Idea

The sound engine supplies a fractional read position for each direct path or specular reflection. Its delay lines determine the arrival time and Doppler shift. D-186 changes how the reference tier reconstructs the signal at that position from eight neighbouring samples. [Sound delay lines](/algorithms/sound-delay-lines.md)

Seventh-order Lagrange interpolation assigns the sample weights. The same weights can be expressed as a normalized sinc under a binomial window. [^interpolation]

# Method

1. Split the position from the delay line into its integer sample index and fractional part f. Use the same read clock as the other quality tiers.
2. With the read position between samples 0 and 1, take samples −3 through +4. For fractional position f use x = 3 + f in this eight-sample array. Weight j is the product of (x − k)/(j − k) over all other indices k from 0 through 7.
3. Compute the weights at compile time for 257 positions including both endpoints. For each sample, interpolate the two adjacent rows using the fractional read position, then sum the eight weighted samples. The path's material, air and listener response follow this reconstruction.

# Invariants

The phase rows sum to one to floating-point precision. The two endpoint rows select the corresponding input sample. Every path uses the same stored rows. A delay change affects the read clock independently of the ear model.

# Cost

The table occupies 8,224 bytes per binary and requires no per-listener allocation. Each reference-quality direct or specular path reads eight mono samples per output sample. In twelve paired runs of the 48 kHz path benchmark on one Core i5-1335U hardware thread on 24 November 2026, the median paired change was −0.99 ns per sample for speakers (96% interval −1.02 to −0.08 ns). The headphone interval spanned −1.00 to +0.02 ns. Run `paris-soundscape bench --part way` to measure the path cost. [Benchmark command](https://github.com/smichea/osage/blob/main/tools/paris-soundscape/README.md)

# Failure modes

A finite filter attenuates signals near Nyquist. At 22 kHz its phase-averaged gain is −4.46 dB, compared with −4.18 dB for the previous eight-point Blackman–Harris window. Some secondary images are stronger even when the largest image and their combined energy fall. This reader does not prevent an input above the output bandwidth from aliasing under a large upward shift.

# Alternatives

The lower quality tiers use two-point linear or four-point cubic interpolation. The shorter windows trade passband accuracy for fewer source samples per path.

# Oracle

A 10 kHz source moving straight towards a listener at one seventeenth of the speed of sound produces a 10,625 Hz tone. The first negative-frequency sampling image folds to 7,625 Hz at the 48 kHz output rate. A Hann-windowed projection measures both components on the rendered loudspeaker channel. The test requires a carrier within 0.5 Hz and an image below −48 dBc. The previous coefficients give −35.15 dBc on this test. [^regression]

On a separate 72 km/h pass five metres from the listener, the loudest measured image in four approach and departure windows fell from −35.07 to −51.72 dBc in the reference CLI output at 10 kHz. The peak frequency error at five instants stayed below 0.1 Hz.

# Reuse

Run `cargo test -p osage-sound --test physics a_moving_tone` at the workspace root. The bridge tool accepts a scene and listening mode for a full stereo render. [Bridge tool](https://github.com/smichea/osage/blob/main/tools/paris-soundscape/README.md)

[^interpolation]: [Relation of Lagrange to Sinc Interpolation](https://www.dsprelated.com/freebooks/pasp/Relation_Lagrange_Sinc_Interpolation.html).
[^regression]: [End-to-end Doppler and spectral-image regression](https://github.com/smichea/osage/blob/main/crates/osage-sound/tests/physics.rs).
