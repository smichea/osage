---
type: Technique
title: Ground that varies
description: "A ground texture that never repeats — its grain sampled by hexagonal cells shifted and turned by where they are, multiplied in world space by clumps and by drier and lusher patches, and on a road by its history along its length (paving seams, repaired patches, cracks, stains); octaves smaller than a few pixels fade out."
resource: https://github.com/smichea/osage/blob/main/crates/osage-render/src/ground.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
tiers: [T1, T2, T3, T4]
status: draft
tags: [rendering, materials, terrain, procedural]
sources:
  - id: quilez
    resource: https://iquilezles.org/articles/texturerepetition/
    title: "Inigo Quilez, Texture repetition"
  - id: heitz
    resource: https://eheitz.github.io/
    title: "Eric Heitz and Fabrice Neyret, High-Performance By-Example Noise using a Histogram-Preserving Blending Operator, HPG 2018"
  - id: mikkelsen
    resource: https://jcgt.org/published/0011/03/05/
    title: "Morten S. Mikkelsen, Practical Real-Time Hex-Tiling, JCGT 2022"
  - id: own
    resource: "osage's own design and measurements, 2026-11-11 and 2026-11-12"
    title: osage design
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-12T16:30:00Z" }
---

# What it does

A photographed grain of grass repeats every two or three metres. Up close it is convincing; from ten metres its repeats show, and from fifty it blurs to its mean colour and the ground looks painted in one green. Real grass varies at every scale: a clump greener than the next, a drier strip where the soil is thin, a lusher hollow. This material puts that variation back, in world space, over any ground texture[^quilez].

# How

In the fragment shader, after the standard material has computed its colour from the texture and the vertex colours:

1. **Clumps.** Two octaves of value noise in world space, of `s` and `0.37 s` metres (`s` = 1.6 m for a meadow), the second turned by 37° so that their lattices never line up, each centred on zero. The colour is multiplied by `1 + k · 1.6 · (n₁ + 0.6 n₂)`, with `k` the strength (0.38 for a meadow: about ±30 %).
2. **Patches.** Two other noises of `p` and `0.6 p` metres (`p` = 22 m): where the first is high the colour turns towards a dry multiplier (straw: ×1.35, ×1.12, ×0.72 in red, green, blue), where the second is low towards a lush one (×0.78, ×0.92, ×0.8).
3. **No shimmer.** Each octave is weighted by `1 − smoothstep(0.125, 0.5, f / size)`, where `f` is the size of the pixel on the ground: the geometric mean of the lengths of the world position's screen derivatives. An octave is whole above eight pixels and gone below two. The mean, not the longer side: a slope seen from the road is stretched along the view, and the longer side alone would flatten every hill.

The variation multiplies whatever the world paints: vertex colours for wide sweeps, the grain for the blades. A world chooses the strengths, sizes and colours; the engine provides the material[^own].

**No tile** (`Ground::with_grain`, V1.0 of the kart programme). The grain — the surface's colour map, handed to this material instead of the standard one (`untextured`) — is sampled by cells[^heitz][^mikkelsen]:

1. A triangular lattice over the texture coordinates, 1.6 cells a repeat (`HEX_CELLS`): its vertices are the centres of hexagons. A pixel lies in one triangle; its barycentric weights are its nearness to the three centres.
2. Each centre's hash gives its cell a turn (any angle) and a shift (anywhere in a repeat); the grain is read at the pixel's coordinate turned and shifted so, with the screen gradients turned too (`textureSampleGrad`: the right mip, no seams at cell edges).
3. The weights are raised to the 6th power and renormalised: wide blends wash a photograph's contrast out, narrow ones show the cells' edges. A cell weighing under 3 % (`SKIP`) is not read: most pixels read one cell, a few two or three.

The mean and the contrast stay the grain's; no period is left anywhere ([oracle](#oracle)). The normal and roughness maps still repeat as before: rotating a normal map's vectors with its cell is the next step.

**A road's history** (`Ground::paved`). On a mesh whose texture coordinates are metres across and along a road, a deterministic function of those two numbers draws what a road lives through: a paving seam along it where two passes of the paver met, sealed, wandering a little off the middle; in each stretch (40 m on the kart circuit), more often than not a repaired patch of fresher, darker asphalt with sealed edges and corners not quite square; cracks in the older stretches (the ridges of two noises, branching where they meet); rare stains, darker and smoother; and a tone that wanders over 45 m. Lines thinner than a pixel fade rather than shimmer. The racing line's rubber and the skid marks are the world's (vertex colours, decals): the material adds what time did.

**Rock on steep ground** (`Ground::with_rock`, D-081). A cliff has no texture coordinates of its own: where the surface's normal leans from the vertical by more than about 40° (`1 − |n.y|` from 0.22 to 0.42), a rock grain is read along the three axes and blended by the normal's components to the 4th power (triplanar), its contrast raised 1.9 times about its mean (cracks and lumps read stronger than grass's). Across it lie strata — two noises of the world height, `w` and `0.31 w` metres apart, wavering with a noise of the ground over 37 m — and varnish: dark streaks down the face where water ran, a noise of the position along the face (across its normal) over 1.9 m and of the height over 28 m. Roughness goes to 0.9. The kart's canyon sets bold strata 2.2 m apart, its mountain faint ones 1.4 m apart; the rock's colour stays the land's (vertex colours: a canyon's red sandstone, a mountain's grey).

**Mown stripes** (`Ground::mown`, D-101). Where the vertex colour's alpha says the grass is mown, it lies in straight stripes (4.5 m on the kart circuit) the mower cut one way and back: each stripe's blades lean the way it went, and look lighter seen from behind them (their lit sides) than from ahead (their tips and shade). The colour is multiplied by `1 + k · s · (0.35 sign(e·d) + 0.65 e·d)`, with `s` ±1 by stripe, `d` the mower's direction, `e` the direction to the eye on the ground (`k` 0.2): the bands swap as the eye goes round, and a third of the contrast stays seen across them. Stripe edges are softened over 8 % of a stripe, and fade where a stripe is a few pixels. The kart world mows the land a few metres past the barriers; beyond, meadow.

**Clouds' shadows** (D-108). Fair-weather clouds' shadows drift over the ground on the wind: two octaves of the same value noise, 180 m and 67 m across, carried by the wind (`clouds.zw`, m/s) times the engine's clock, read in the shader — the material itself changes only with the weather, as touching a material makes the renderer prepare it again — cut at a soft threshold into patches covering about a third of the ground, each darkening it by the world's strength (0.3 by day; none at night or in the rain). Only the ground takes them: karts, walls and trees do not.

# Settings per tier

The variation is the same at every tier: a few noise lookups a pixel on the ground. Whether a grain is sampled untiled is the world's choice per material; the kart circuit untiles its road from T2 and its grass and gravel, which fill most of the picture, from T3; the road's history is drawn at every tier.

# Cost

Measured at T3 on the Iris Xe at 2560×1440, 2026-11-11, the kart circuit: the race view 127 ms per frame before and after the variation, the wide view over the meadows 114 and 113 ms ([the render bench](/oracles/render-bench.md)) — within the noise of the measurement.

Untiled grains and the road's history, the kart bench at T2 (1280×720) on the Iris Xe, 2026-11-12: 16.4 ms a frame before, 18.1 with every ground untiled (+1.7 ms, about 10 %). As the kart circuit sets it: at T2, its road untiled with its history, 16.7 ms before and 17.9 after (+1.2 ms); at T3 (2560×1440), every ground untiled, 112.5 and 116.6 ms (+3.6 %). Measured on a laptop running other work: a few tenths of a millisecond are noise.

# Limits

Untiled, the colour no longer repeats, but the normal and roughness maps do, close up, and a grain with a strong direction (stripes, planks) must not be turned: `tiling.z` limits the turn. A photograph's own cracks are cut into pieces at the cells' edges, where the blend is narrow. A road's history is drawn, not modelled: patches and cracks have no relief of their own. Splatting several textures (grass, earth, gravel) by masks is the next step for paths and worn ground. Rock has no normal map of its own (triplanar normals): its relief is the land's geometry and its grain's shading.

# Oracle

[The render bench](/oracles/render-bench.md) screenshots: the wide view over the meadows, the race view. The crate's tests (`ground.rs`) run the shader's tiling step by step on the CPU (`hex_tiles`, kept the same as `ground.wesl`'s `untiled`): the weights add to one; a strip 64 repeats long of a grain with a feature every repeat correlates with itself above 0.99 at every multiple of the repeat when tiled, under 0.35 untiled, and under 0.5 at any lag; the mean is kept and the contrast within 20 %.

# Reuse

Any renderer with a programmable fragment stage, on any tiled ground: grass, sand, earth, snow.

[^quilez]: Quilez, Texture repetition
[^heitz]: Heitz and Neyret, by-example noise
[^mikkelsen]: Mikkelsen, hex-tiling
[^own]: osage design
