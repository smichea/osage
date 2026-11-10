---
type: Technique
title: Light from the sky
description: How the sky and the ground light what the sun does not, chosen per tier because lighting from a cubemap costs 2 to 3 ms on an integrated GPU whatever its size.
resource: https://github.com/smichea/osage/blob/main/crates/osage-render/src/sky_light.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
tiers: [T1, T2, T3, T4]
status: draft
tags: [rendering, lighting, image-based-lighting, sky]
sources:
  - id: karis
    resource: https://cdn2.unrealengine.com/Resources/files/2013SiggraphPresentationsNotes-26915738.pdf
    title: "Brian Karis, Real Shading in Unreal Engine 4, SIGGRAPH 2013 course notes"
  - id: hillaire
    resource: https://sebh.github.io/publications/egsr2020.pdf
    title: "Sébastien Hillaire, A Scalable and Production Ready Sky and Atmosphere Rendering Technique, EGSR 2020"
  - id: bevy
    resource: https://docs.rs/bevy/0.20.1/bevy/
    title: Bevy 0.20 documentation
  - id: own
    resource: "osage's own design and measurements, 2026-11-10"
    title: osage design
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-10T09:30:00Z" }
---

# What it does

Lights what the sun does not reach. Outdoors, a shadow is lit by the blue sky above it and by the ground around it; a glossy surface mirrors the horizon. With this technique the scene is lit by the sky; without it, by a constant ambient light of a clear sky's brightness (6 000 cd/m², slightly blue).

# How

Image-based lighting as Bevy implements it[^bevy]: two cubemaps around the camera, one that a matte surface reads in the direction of its normal, one that a glossy surface reads in the direction of its reflection, at a level of blur that follows its roughness (the split-sum approximation[^karis]).

Where the cubemaps come from depends on the world:

- **With an atmosphere** (the [sky and sun](/techniques/sky-and-sun.md) technique), Bevy renders them from the atmosphere every frame[^hillaire]: the light always matches the sky that is seen.
- **Without one**, osage computes a cubemap of a clear day once, on the processor[^own]. For a direction `d`, with `s` the direction of the sun and `g` the ground colour the world gives in its look:
  - above the horizon, a blend from a deep blue at the zenith to a pale, brighter horizon by `(1 − d.y)⁴`, plus a glow around the sun of `1.6 (d·s)⁴⁸ + 0.25 (d·s)⁶`;
  - below, the ground: `g (0.5 + 1.4 max(s.y, 0))`, joined to the horizon colour over the first 0.12 of `−d.y`, as haze.
  
  The matte cubemap (8 texels a side) averages 48 directions spread as `cos θ` around each texel's direction; each level of the glossy one (64 texels a side, 7 levels) averages 48 directions spread as `cosⁿ θ`, with `n = 2 / r⁴ − 2` for the roughness `r` that Bevy reads at that level. Directions are spread by the golden angle, so that the result is the same on every machine.

# Settings per tier

| Tier | Light from the sky |
|---|---|
| T1 | constant ambient |
| T2 | constant ambient |
| T3 | cubemaps from the atmosphere every frame, 128 texels a side; a clear day computed once without an atmosphere |
| T4 | the same, 256 texels a side |

# Cost

On the Intel Iris Xe (Raptor Lake-P, Mesa 26.2), T2 at 1632×918, 2026-11-10, four alternating runs of the kart bench: lighting from the computed clear-day cubemaps cost 2.1 ms a frame (19.3 ms against 17.2 ms). With a glossy cubemap of 8 texels instead of 64, it still cost 2.4 to 3.6 ms: the cost is in the shading path that reads any cubemap, not in its size. That is an eighth of T2's budget for a subtle change, so T1 and T2 keep the constant light. Rendering the cubemaps from the atmosphere costs about 5 ms more on the same machine.

Computing the clear day once takes a few milliseconds of processor time.

# Limits

The computed clear day knows nothing of what stands around: no shadow from a hill or a building in it. The constant light of T1 and T2 makes every shadow the same flat blue-grey. Too bright a ground colour tints white objects from below: grass sends back about a fifth of the light it gets.

# Oracle

[The render bench](/oracles/render-bench.md), with and without `--without sky-light`, alternated on the same machine.

# Reuse

The measurement is the lesson: before adding a technique to a low tier, measure it with the smallest data it could use; if the cost does not shrink, it is in the shader, and only a cheaper shading path will help. The clear-day formula is a dependency-free sky for any engine.

[^karis]: Real Shading in Unreal Engine 4
[^hillaire]: Hillaire 2020
[^bevy]: Bevy 0.20 documentation
[^own]: osage design
