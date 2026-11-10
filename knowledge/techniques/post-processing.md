---
type: Technique
title: Post-processing and anti-aliasing
description: HDR, exposure, the world's look, anti-aliasing and bloom, chosen per tier so that the lower tiers need no depth prepass.
resource: https://github.com/smichea/osage/blob/main/crates/osage-render/src/post.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
tiers: [T1, T2, T3, T4]
status: draft
tags: [rendering, anti-aliasing, tonemapping, bloom]
sources:
  - id: fxaa
    resource: "Timothy Lottes, FXAA, NVIDIA white paper, 2009"
    title: Fast Approximate Anti-Aliasing
  - id: bevy
    resource: https://docs.rs/bevy/0.20.1/bevy/
    title: Bevy 0.20 documentation
generated: { by: "claude-code/claude-fable-5-2", at: "2026-11-10T06:45:00Z" }
---

# What it does

Turns the rendered light into the final picture: exposure, tonemapping and colour, smoothing of jagged edges, glow around very bright things.

# How

Every tier renders in high dynamic range with the exposure of a camera in full sun (EV100 15), because the sun has its physical brightness ([sky and sun](/techniques/sky-and-sun.md)). On top:

- **The look is the world's.** A world provides a `Look`: its tonemapper, a saturation factor and an exposure offset in stops. The technique applies it; it does not choose it.
- **Anti-aliasing by tier.** T1 and T2 use FXAA[^fxaa], a single pass over the final image that needs nothing else. From T3, temporal anti-aliasing blends each frame with the previous ones; it needs a depth and motion prepass.
- **Bloom** from T2; **motion blur** from T3.

The passes themselves are Bevy's[^bevy]; this technique chooses which run at each tier, and with what settings.

# Settings per tier

| Tier | Anti-aliasing | Bloom | Motion blur |
|---|---|---|---|
| T1 | FXAA | no | no |
| T2 | FXAA | yes | no |
| T3, T4 | temporal | yes | yes |

# Cost

On an Intel Iris Xe at 1080p (2026-11-10), temporal anti-aliasing with bloom cost about 1 ms, plus its prepass. FXAA with bloom was not measured apart; the whole T2 set is measured by the bench.

# Limits

FXAA softens fine detail. The prepass that temporal anti-aliasing needs triggers the speckle defect described in [ambient occlusion](/techniques/ambient-occlusion.md) on integrated Intel graphics, which is one more reason T2 does without.

# Oracle

[The render bench](/oracles/render-bench.md).

# Reuse

Two ideas: a physical exposure shared by all tiers, so that content is authored once; and the look as data owned by the world, not by the engine.

[^fxaa]: FXAA
[^bevy]: Bevy 0.20 documentation
