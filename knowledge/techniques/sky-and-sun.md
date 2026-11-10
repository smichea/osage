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
generated: { by: "claude-code/claude-fable-5-2", at: "2026-11-10T06:45:00Z" }
---

# What it does

Draws the sky, the sun's disk and the haze over distant objects, and decides how the sky lights the scene.

# How

The atmosphere is Hillaire's model[^hillaire] as Bevy implements it[^bevy]: small lookup tables for transmittance, multiple scattering, the sky seen from the camera and the haze by distance, recomputed each frame and sampled when shading.

Two choices matter more than the tables:

1. **The sun has its physical brightness at every tier** (about 100 000 lux on the ground), and the camera is exposed for a sunny day. Materials then look the same whatever the tier.
2. **How the sky lights the scene is a tier decision.** From T3, an environment map is rendered from the atmosphere every frame and lights every surface. Below, a constant ambient light of a clear sky's luminance (6 000 cd/m², slightly blue) stands in for it.

# Settings per tier

| Tier | Sky | Lighting from the sky |
|---|---|---|
| T1 | flat colour, linear fog from 120 m to 420 m | constant ambient |
| T2 | atmosphere with half-size tables and half the samples | constant ambient |
| T3 | atmosphere, default tables | environment map, 128 pixels a side |
| T4 | atmosphere, default tables | environment map, 256 pixels a side |

# Cost

On the Intel Iris Xe (Raptor Lake-P, Mesa 26.2), 1920×1080, 2026-11-10: the atmosphere with default tables costs about 4 ms; lighting the scene from it 5.3 ms more. Both were measured by leaving the technique out with `--without sky` and by changing one setting at a time ([render bench](/oracles/render-bench.md)).

# Limits

No clouds in the atmosphere, no time of day yet. With a constant ambient light, a surface in shadow has no sense of where the sky is.

# Oracle

[The render bench](/oracles/render-bench.md): the tier holds its budget, and the screenshots at fixed frames show the sky.

# Reuse

The split is the reusable idea: keep the physical sky cheap enough for integrated GPUs by shrinking its tables, and treat "the sky lights the scene" as a separate, optional cost.

[^hillaire]: Hillaire 2020
[^bevy]: Bevy 0.20 documentation
