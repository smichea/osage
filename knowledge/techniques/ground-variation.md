---
type: Technique
title: Ground that varies
description: A tiled ground texture multiplied in world space by clumps a metre or two across and by drier and lusher patches tens of metres across, so that grass never reads as a carpet; octaves smaller than a few pixels fade out.
resource: https://github.com/smichea/osage/blob/main/crates/osage-render/src/ground.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
tiers: [T1, T2, T3, T4]
status: draft
tags: [rendering, materials, terrain, procedural]
sources:
  - id: quilez
    resource: https://iquilezles.org/articles/texturerepetition/
    title: "Inigo Quilez, Texture repetition"
  - id: own
    resource: "osage's own design and measurements, 2026-11-11"
    title: osage design
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-11T10:00:00Z" }
---

# What it does

A photographed grain of grass repeats every two or three metres. Up close it is convincing; from ten metres its repeats show, and from fifty it blurs to its mean colour and the ground looks painted in one green. Real grass varies at every scale: a clump greener than the next, a drier strip where the soil is thin, a lusher hollow. This material puts that variation back, in world space, over any ground texture[^quilez].

# How

In the fragment shader, after the standard material has computed its colour from the texture and the vertex colours:

1. **Clumps.** Two octaves of value noise in world space, of `s` and `0.37 s` metres (`s` = 1.6 m for a meadow), the second turned by 37° so that their lattices never line up, each centred on zero. The colour is multiplied by `1 + k · 1.6 · (n₁ + 0.6 n₂)`, with `k` the strength (0.38 for a meadow: about ±30 %).
2. **Patches.** Two other noises of `p` and `0.6 p` metres (`p` = 22 m): where the first is high the colour turns towards a dry multiplier (straw: ×1.35, ×1.12, ×0.72 in red, green, blue), where the second is low towards a lush one (×0.78, ×0.92, ×0.8).
3. **No shimmer.** Each octave is weighted by `1 − smoothstep(0.125, 0.5, f / size)`, where `f` is the size of the pixel on the ground: the geometric mean of the lengths of the world position's screen derivatives. An octave is whole above eight pixels and gone below two. The mean, not the longer side: a slope seen from the road is stretched along the view, and the longer side alone would flatten every hill.

The variation multiplies whatever the world paints: vertex colours for wide sweeps, the grain for the blades. A world chooses the strengths, sizes and colours; the engine provides the material[^own].

# Settings per tier

The same at every tier: a few noise lookups a pixel on the ground.

# Cost

Measured at T3 on the Iris Xe at 2560×1440, 2026-11-11, the kart circuit: the race view 127 ms per frame before and after, the wide view over the meadows 114 and 113 ms ([the render bench](/oracles/render-bench.md)) — within the noise of the measurement.

# Limits

It varies colour only, not the grain itself: the texture's repeats still show where a pixel covers less than a repeat, close to the ground. Sampling the texture twice at different scales and rotations, or by tiles blended at random, would hide that too. Splatting several textures (grass, earth, gravel) by masks is the next step for paths and worn ground.

# Oracle

[The render bench](/oracles/render-bench.md) screenshots: the wide view over the meadows, the race view.

# Reuse

Any renderer with a programmable fragment stage, on any tiled ground: grass, sand, earth, snow.

[^quilez]: Quilez, Texture repetition
[^own]: osage design
