---
type: Algorithm
title: Loudness mastering
description: An offline render made into a file without falsifying it — faded at both ends, brought to a loudness by ITU-R BS.1770-4 with one gain for the whole file (no limiter riding the sound), held lower only where peaks between samples would pass a true-peak ceiling, then dithered with triangular noise and rounded to 16 bits.
resource: https://github.com/smichea/osage/blob/main/crates/osage-sound/src/render.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [sound, loudness, mastering, dither]
sources:
  - id: bs1770
    resource: "Recommendation ITU-R BS.1770-4, Algorithms to measure audio programme loudness and true-peak audio level (2015)"
    title: ITU-R BS.1770-4
  - id: own
    resource: "osage's own design, tools/paris-soundscape/DESIGN.md, 2026-11-23"
    title: osage design
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-24T09:00:00Z" }
---

# Problem

A render must be compared with other renders and with recordings, by ear and by instrument. If one is louder, it is judged better; if a limiter or compressor follows the sound, a passing train is flattened and the measure of its passage is false; if the float signal is simply rounded, its quiet tails are distorted. And the loudness must be the file's as it will be heard, ends included.

# Idea

Measure the file as it will be written, then apply one gain to all of it: the loudness the scene asks for, or less if that gain would push a peak between samples over the ceiling[^bs1770]. Nothing follows the sound[^own]. Dither before rounding so the error is a steady hiss.

# Method

1. **Lead.** Start the engine three seconds before the first sample (a whole number of blocks, longer than sound takes from the farthest emitter), so that sound already on its way arrives; what the lead costs is not counted.
2. **Fade.** Raise from silence over `fade_in` and fall to it over `fade_out` with raised cosines (`0.5 − 0.5·cos(πx)`); the command uses 0.4 s and 1.5 s. In a file too short, both shrink in proportion so that together they take at most half of it.
3. **Measure.** The integrated loudness of the faded file by BS.1770-4: K-weighting, 400 ms blocks every 100 ms, a gate at −70 LKFS and one 10 LU under what passed it, both channels weighted 1. The true peak of each channel: four points a sample.
4. **Gain.** `wanted = 10^((target − measured)/20)`, `allowed = 10^(ceiling/20) / true_peak`, `gain = min(wanted, allowed)`. The command's ceiling is −1 dBTP. Report the loudness reached, `measured + 20·log10(gain)`.
5. **Dither and round.** For each channel and sample: `round(x · gain · 32 767 + (u₁ − 0.5) + (u₂ − 0.5))`, with `u₁`, `u₂` uniform on [0, 1) from a seeded noise per channel (a triangular distribution two steps wide), clamped to 16 bits. Count the samples held at the limits: none, in a render at its level.

# Invariants

- The file as written reads the loudness asked within 0.05 LU, or is held lower and then has its true peak at the ceiling (tested; checked by [soundscape verify](/oracles/soundscape-verify.md) within 0.3 LU).
- Nothing is held at the limits; the ends are silent to within the dither.
- The same seed gives the same bytes; another seed another dither.

# Cost

One pass to measure (a few filters and a four-times oversampled peak), one to write: negligible beside the render.

# Failure modes

- A file whose loudest moment is brief and whose rest is quiet is held low by its peaks: it then reads under the target, and says so.
- BS.1770 weighs every channel 1: right for stereo, not for surround, which this does not write.

# Alternatives

A brick-wall limiter after the gain reaches any loudness at the cost of changing the sound's dynamics; normalising to the sample peak is simpler and ignores both loudness and peaks between samples; noise-shaped dither pushes the hiss above hearing, at the cost of a filter.

# Oracle

The tests of `render.rs` (`mastering_keeps_the_level_fades_the_ends_and_holds_nothing`, `a_short_file_is_as_loud_as_asked_once_its_ends_are_faded`); [soundscape verify](/oracles/soundscape-verify.md); loudness and true peak measured by [osage-sound-analysis](/modules/osage-sound-analysis.md).

# Reuse

Any offline audio pipeline whose files are compared: one gain from loudness, capped by true peak, nothing that follows the signal.

[^bs1770]: ITU-R BS.1770-4
[^own]: osage design
