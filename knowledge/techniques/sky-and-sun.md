---
type: Technique
title: Sky and sun
description: A physically based atmosphere and sun whose cost is cut per tier, down to a flat sky with fog.
resource: https://github.com/smichea/osage/blob/main/crates/osage-render/src/sky.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
tiers: [T1, T2, T3, T4]
status: draft
tags: [rendering, sky, atmosphere, lighting]
sources:
  - id: hillaire
    resource: https://sebh.github.io/publications/egsr2020.pdf
    title: "Sébastien Hillaire, A Scalable and Production Ready Sky and Atmosphere Rendering Technique, EGSR 2020"
  - id: bevy
    resource: https://docs.rs/bevy/0.20.1/bevy/
    title: Bevy 0.20 documentation
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-10T09:30:00Z" }
---

# What it does

Draws the sky, the sun's disk and the haze over distant objects. How the sky lights the scene is a technique of its own, [light from the sky](/techniques/sky-light.md).

# How

The atmosphere is Hillaire's model[^hillaire] as Bevy implements it[^bevy]: small lookup tables for transmittance, multiple scattering, the sky seen from the camera and the haze by distance, recomputed each frame and sampled when shading.

One choice matters more than the tables: **the sun has its physical brightness at every tier** (about 100 000 lux on the ground), and the camera is exposed for a sunny day. Materials then look the same whatever the tier.

# Settings per tier

| Tier | Sky |
|---|---|
| T1 | flat colour, linear fog from 120 m to 420 m |
| T2 | atmosphere with half-size tables and half the samples |
| T3 | atmosphere, default tables |
| T4 | atmosphere, default tables |

# Cost

On the Intel Iris Xe (Raptor Lake-P, Mesa 26.2), 1920×1080, 2026-11-10: the atmosphere with default tables costs about 4 ms. It was measured by leaving the technique out with `--without sky` and by changing one setting at a time ([render bench](/oracles/render-bench.md)).

# Limits

No clouds in the atmosphere, no time of day yet.

# Oracle

[The render bench](/oracles/render-bench.md): the tier holds its budget, and the screenshots at fixed frames show the sky.

# Reuse

The split is the reusable idea: keep the physical sky cheap enough for integrated GPUs by shrinking its tables, and treat "the sky lights the scene" as a separate, optional cost ([light from the sky](/techniques/sky-light.md)).

[^hillaire]: Hillaire 2020
[^bevy]: Bevy 0.20 documentation
