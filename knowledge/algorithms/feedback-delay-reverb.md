---
type: Algorithm
title: Feedback delay reverberation
description: The space's own answer as eight delay lines of prime lengths feeding one another through a Householder matrix, each losing at every turn what a decay given at 250 Hz, 1 kHz and 4 kHz asks of each of its three bands — the shares solved so that gently parted bands still meet each decay, and capped so that nothing rings more than twice as long as asked.
resource: https://github.com/smichea/osage/blob/main/crates/osage-sound/src/engine.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [sound, reverberation, fdn]
sources:
  - id: jot
    resource: "J.-M. Jot and A. Chaigne, Digital delay networks for designing artificial reverberators, 90th AES Convention, 1991 (cited from memory)"
    title: Digital delay networks for designing artificial reverberators
  - id: own
    resource: "osage's own design, tools/paris-soundscape/DESIGN.md, 2026-11-23"
    title: osage design
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-24T09:00:00Z" }
---

# Problem

Beyond the first reflections, a place answers with a dense tail whose length differs by band (a stone bridge rings longer in the lows than the highs). Following every late reflection is out of reach; a game needs a tail that dies at the decay asked in each band, costs a few operations a sample, and is fed by what each face scatters, at the time it scatters.

# Idea

A feedback delay network[^jot]: a few delay lines whose outputs are mixed by a lossless matrix and fed back. The decay is set by a filter on each line that keeps, after one turn, the share of each band that a 60 dB decay in the asked time leaves after that line's length. The faces' scatter feeds it from each way's own delay[^own].

# Method

At 48 kHz:

1. **Lines:** eight, of 631, 809, 997, 1 259, 1 483, 1 741, 1 999 and 2 251 samples (13 to 47 ms, primes, so that no two repeat together).
2. **Bands per line:** two one-pole low-passes at 500 Hz and 2 kHz part what a line reads into low (`L₁`), middle (`L₂ − L₁`) and high (`1 − L₂`); the line keeps `k_low·low + k_mid·mid + k_high·high`.
3. **What a line must keep:** for a decay `T₆₀` at a centre frequency, after a line of `n` samples, `g = 10^(−3n / (T₆₀ · rate))`.
4. **Solving the shares:** the parting is gentle (6 dB an octave), so each centre (250 Hz, 1 kHz, 4 kHz) holds some of its neighbours' bands. Starting from `k = g`, repeat 48 times: for each band, compute what the line keeps at that band's centre with the current shares (the complex response of the two low-passes), and scale `k_band` by `g_band / kept`, clamped to (10⁻⁹, 1).
5. **Cap:** compute what the line keeps at every tenth of an octave from 20 Hz, at 0 Hz and at half the rate. If the most exceeds `√(max g)` — what a decay twice as long as the longest asked would keep — scale all three shares down to it. Whatever is asked, the tail dies, and nowhere more than twice as slowly as asked.
6. **Each sample:** read each line; take its kept value `oᵢ`; write back `input + oᵢ − ¼·Σⱼ oⱼ` (the Householder matrix `I − (2/8)·11ᵀ`, lossless: nothing added, nothing lost); left out `0.35·Σ ±oᵢ` with signs `+−+−+−+−`, right `0.35·Σ ±oᵢ` with signs `++−−++−−`, two decorrelated outputs.
7. **Input:** every way of every emitter adds to a send bus what its face scatters, `level · √(scattered energy) · reverb gain`, read from the emitter's line at that way's own delay — on the mirrored way when it is rendered, otherwise on a way of its own that reaches the ears with nothing. Ways left out by the budget still feed it.
8. Filter states under 10⁻¹⁵ are set to zero (denormals are a hundred times slower).

# Invariants

- The tail dies in each band as fast as asked, within a tenth, when neighbouring decays are no more than about twice apart (a test of the engine measures each band's decay).
- A line keeps less than everything at every frequency (tested): the network is stable.
- What a wall scatters arrives at the time its way takes (tested).

# Cost

Eight reads, eight pairs of one-pole filters and eight writes a sample: a few hundredths of the engine's time on the Pont de Bir-Hakeim. Memory: 11 170 samples.

# Failure modes

- Decays further apart than the gentle parting allows are not met; the cap holds the tail to twice the longest.
- The decay is given by the scene, not found from its geometry.
- What a face scatters is one late copy of the emitter's sound per face: two faces' copies of a steady tone add or cancel as the emitter moves, where a rough wall would blur them.
- An emitter none of whose ways reaches the ears feeds nothing: what is wholly hidden is not heard through the reverberation either.

# Alternatives

Convolution with a measured or simulated impulse response is exact for a fixed source and listener and costs far more; Schroeder all-pass chains are cheaper and colour the sound; a geometry-driven decay (from volume and absorption by Sabine or Eyring) would set `T₆₀` from the scene instead of asking it.

# Oracle

The engine's test `the_tail_dies_in_each_band_as_fast_as_it_is_asked`; the tests of `crates/osage-sound/tests/physics.rs` on scatter timing.

# Reuse

Any engine: the share solving for gently parted bands (step 4) and the cap (step 5) make a three-band FDN safe for any decays a designer types in.

[^jot]: Digital delay networks for designing artificial reverberators
[^own]: osage design
