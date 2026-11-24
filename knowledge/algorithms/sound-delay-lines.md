---
type: Algorithm
title: Sound delay lines
description: Each emitter writes its voice into a ring of its own and each way reads it as far back as the way is long, so that a far sound is late, an approaching one is higher (Doppler) and an echo has its own shift with no rule for either; a moving emitter's trail tells where it was when the sound now arriving left it; ways are found anew in staggered blocks and glide evenly between, ranked by loudness under a budget.
resource: https://github.com/smichea/osage/blob/main/crates/osage-sound/src/engine.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [sound, doppler, delay-line, spatial-audio, scheduling]
sources:
  - id: own
    resource: "osage's own design, tools/paris-soundscape/DESIGN.md, 2026-11-23"
    title: osage design
  - id: brown-duda
    resource: https://escholarship.org/content/qt0cr9d23p/qt0cr9d23p_noSplash_c6f0ae8be6c4003ba4f479c6ef3d369b.pdf
    title: C. P. Brown and R. O. Duda, A structural model for binaural sound synthesis, IEEE Trans. Speech and Audio Processing 6(5), 1998
  - id: iso9613
    resource: https://sengpielaudio.com/AirdampingFormula.htm
    title: ISO 9613-1:1993, the air's absorption, as reproduced
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-24T09:00:00Z" }
---

# Problem

A sound that travels takes time. A game engine that plays each source's sound now, panned and attenuated, has to add a Doppler shift by rule (a pitch factor from the radial speed), cannot give an echo its own delay and shift, and replays what a moving source is doing now along every way — when the sound arriving by a long echo left the source a quarter of a second ago, from somewhere else. And with dozens of sources and several ways each, the engine must spend its budget on what is heard.

# Idea

Make each emitter's sound once, into a ring buffer: the pressure one metre from it, sample by sample. A way to the ears is that ring read `length / c` seconds back. If the way shortens, the read point advances faster than the write point: the sound is higher. That is the Doppler shift, exact, for the straight way and every echo alike[^own]. Where the emitter was when the sound left it is read from a short trail of its past positions.

# Method

At 48 kHz, in blocks of 128 samples.

**The ring.** Each emitter has a ring of `next_power_of_two(maxDistance/c · rate + 4·block)` samples (a reach of 600 m: 128 k samples, 512 kB). Each block its voice writes 128 samples at `block·128` onwards (indices masked).

**A way's target.** When an emitter's ways are found, each becomes an aim: the delay `length/c·rate` in samples, four band gains `level · bands[b] / max(length, 0.3 m)`, the air's low-pass, the share sent to the reverberation, and for each ear its extra delay, the head's shadow and the outer ear's five echo delays (or two pan gains for loudspeakers)[^brown-duda].

**Gliding.** The ways of an emitter are found anew every `every` blocks (1, 2 or 4 by quality). Between, every number of each way goes linearly from where it is to its target over the blocks left, sample by sample within a block. A new way starts silent and swells; a lost way fades over one block. So a lower rate costs lateness (8 ms at 4 blocks), never a step.

**Staggering.** Which block an emitter's ways are found in is `(block + turn) mod every = 0`, where `turn` is a hash of the scene's seed and the emitter's name (its node path and identifier): the searches spread over the blocks, and an emitter's schedule does not depend on which other emitters exist.

**Reading.** The read point is held as a fixed-point number of 32 integer and 32 fraction bits, advanced by `(1 − slope)·2³²` per sample, with `slope = (delay_goal − delay_now)/128`: no rounding piles up along a block. It is interpolated from 2 samples (linear), 4 (Lagrange cubic) or 8 (a windowed sinc in 256 phases, Blackman-Harris window), by quality.

**Shaping.** What is read is split by three one-pole low-passes at 500 Hz, 2 kHz and 6 kHz into four bands, each with its gain; when the four gains are equal (most short ways) one gain is used. Then the air: two one-pole low-passes set where the air has taken 3 dB, `corner = 4 kHz·√(6/loss₄ₖ)` where `loss₄ₖ` is the way's air loss at 4 kHz in dB[^iso9613]; a way on which the air takes less than the quality's threshold is left alone. Then the outer ear's echoes and each ear's delay and shadow, into the mix.

**The trail.** An emitter on a node that moves keeps where it was at the end of each of the last blocks, as many as its sound can take to arrive plus three. The straight way is found from where it was `lag₀` ago (the straight way's last travel time); each mirrored way from where it was `lagₖ` ago, that way's own travel time, interpolated between kept blocks. What the emitter does now changes nothing of what is still on its way. What its voice makes now follows what it is doing now: the ring carries it.

**The budget.** After finding, every way that reaches the ears is ranked by `max(gain at 250 Hz, gain at 1 kHz)`, times 1.2 if it was already being rendered (so two near the edge do not swap block after block). Only the first `ways` of the quality are rendered; ties are broken by the earlier arrival, then the one more to the left, then the rest of the aim — never by the index of a face. A way left out still feeds the reverberation what its face scatters. An emitter none of whose ways is rendered sleeps: its voice is not made, its ring is cleared, and when it wakes the voice swells in over 20 ms.

**The far branch.** A node with `proxyBeyondM` has its own emitters stand for its whole branch from afar: each side is weighted by `√(smoothstep)` across the last 15 % of that distance, at equal power.

# Invariants

- No sound is read sooner than its way takes; a way longer than its emitter's reach is not rendered.
- The Doppler shift of the straight way and of an echo, and an echo's time of arrival, match the physics within the tests' tolerances; an echo is unchanged until what changed it has had time to arrive.
- Turning the head half round swaps the ears sample for sample.
- An emitter's sound does not depend on which other emitters exist.

# Cost

One way alone (its line read through four bands, the air and the ears), on an Intel Core i5-1335U held at 1.3 GHz, 2026-11-23: 13 to 33 ns a sample for loudspeakers and 31 to 86 ns for headphones, from the lowest quality to the reference. Memory: the rings, 21.8 MB for the Pont de Bir-Hakeim's 41 emitters and their contact pool. Measured by `paris-soundscape bench`; see `tools/paris-soundscape/DESIGN.md`.

# Failure modes

- A voice that wakes starts in the middle of its sound and is heard only once that sound has arrived: a far emitter that comes back into the budget is late by its distance.
- Of two ways exactly as loud at the edge of the budget, one is kept: a symmetric scene is then not heard symmetrically.
- Memory grows with each emitter's reach: a city of distant emitters needs the far field (design/09) rather than long rings.

# Alternatives

A per-source pitch factor from the radial speed (OpenAL's Doppler) is cheaper and has no memory, but gives no echo its own delay or shift. Fractional delay lines per way without a shared ring cost one buffer per way. Convolution with a measured room response replaces mirrored ways and the tail at a fixed position.

# Oracle

The tests of `crates/osage-sound/tests/physics.rs`; [soundscape verify](/oracles/soundscape-verify.md).

# Reuse

The ring per source and the trail work in any engine: Doppler and echo delays come free once each way reads a delay line, and staggering the searches by a hash of the name keeps a scene's sound independent of who else is in it.

[^own]: osage design
[^brown-duda]: C. P. Brown and R. O. Duda, A structural model for binaural sound synthesis, IEEE Trans. Speech and Audio Processing 6(5), 1998
[^iso9613]: ISO 9613-1:1993, the air's absorption, as reproduced
