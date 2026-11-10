---
type: Algorithm
title: Particle pool
description: A fixed pool of camera-facing squares for sparks, smoke and flames, where a particle fades by shrinking so that each kind is drawn in one batch.
resource: https://github.com/smichea/osage/blob/main/worlds/kart/native/src/effects.rs
licence: MIT OR Apache-2.0
status: draft
tags: [effects, particles]
sources:
  - id: own
    resource: "osage's own design and measurements, 2026-11-09 and 2026-11-10"
    title: osage design
generated: { by: "claude-code/claude-fable-5-2", at: "2026-11-10T06:45:00Z" }
---

# Problem

Sparks, smoke and flames need hundreds of short-lived sprites. Creating and destroying objects every frame is slow, and a different colour or opacity per particle prevents the renderer from drawing them together.

# Idea

Create all particles once and reuse them; give each kind one material; express a particle's age only through its size[^own].

# Method

1. At start, create a fixed number of squares (640), hidden, and keep their identifiers in a list of idle ones.
2. **Launch:** take an idle square; set its kind's material, position, velocity, life span, size, growth and gravity; show it.
3. **Each frame, for each live particle:** subtract the frame's time from its life; if it is over, hide it and return it to the idle list. Otherwise apply gravity and velocity, bounce it on the ground with 35 % of its vertical speed, set its rotation to the camera's, and set its scale to `size · (1 + growth·age) · fade`, where `age` goes from 0 to 1 and `fade = min(1, 8·age) · min(1, 3·(1 − age))`.
4. **Kinds:** sparks and flames add their light to what is behind them and are brighter than white, so that they glow; smoke and dust are blended, half transparent. A round texture fading to nothing at its edge is shared by all.
5. **Variety without randomness:** every varying value comes from a hash of a counter, so that a run is repeatable.

Emitters: two sparks per rear tyre and per frame while drifting, in the colour of the charge reached; a puff of smoke every fourth frame; a flame per exhaust and per frame while boosting; dust every other frame off the road.

# Invariants

No allocation after start. With no idle square left, a launch does nothing.

# Cost

A loop over 640 small records per frame, and one draw per kind.

# Failure modes

Fading by opacity needs a material per particle and breaks batching. A random generator breaks the bench's repeatability.

# Alternatives

Particles simulated on the GPU scale to millions and belong to the higher tiers.

# Oracle

[The render bench](/oracles/render-bench.md): frame times with karts drifting.

# Reuse

Fade by shrinking, hash a counter: both transfer to any engine.

[^own]: osage design
