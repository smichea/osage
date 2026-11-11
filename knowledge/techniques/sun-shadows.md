---
type: Technique
title: Sun shadows
description: Cascaded shadow maps for the sun, with cascades, reach and resolution chosen per tier from measurements, and from T3 edges that soften with distance as a real sun's do.
resource: https://github.com/smichea/osage/blob/main/crates/osage-render/src/shadows.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
tiers: [T1, T2, T3, T4]
status: draft
tags: [rendering, shadows]
sources:
  - id: csm
    resource: https://developer.download.nvidia.com/SDK/10.5/opengl/src/cascaded_shadow_maps/doc/cascaded_shadow_maps.pdf
    title: "Rouslan Dimitrov, Cascaded Shadow Maps, NVIDIA, 2007"
  - id: bevy
    resource: https://docs.rs/bevy/0.20.1/bevy/
    title: Bevy 0.20 documentation
  - id: pcss
    resource: https://developer.download.nvidia.com/shaderlibrary/docs/shadow_PCSS.pdf
    title: "Randima Fernando, Percentage-Closer Soft Shadows, NVIDIA, 2005"
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-11T21:00:00Z" }
---

# What it does

Gives every object a shadow from the sun, sharp near the camera and coarser far away.

# How

Cascaded shadow maps[^csm]: the view is cut into slices by distance, and the scene is drawn from the sun's point of view once per slice into a square depth map. Near slices cover little ground, so their shadows are sharp. Bevy provides the cascades[^bevy]; this technique chooses how many, how far and how large. From T3 it adds contact shadows, a short ray march in screen space that restores the small shadows a coarse map misses, and soft shadows: percentage-closer soft shadows[^pcss] search the map for the blockers around a point, and blur the shadow's edge by the gap between blocker and receiver times the light's size, so that a shadow is sharp where it touches what casts it and softer further off, as the half-degree disc of the sun makes it. Their noise is filtered over time (Bevy's temporal shadow filter, with the TAA of T3).

# Settings per tier

| Tier | Cascades | Reach | Map size | Contact shadows | Soft shadows |
|---|---|---|---|---|---|
| T1 | 2 | 60 m | 1024 | no | no |
| T2 | 2 | 80 m | 1024 | no | no |
| T3 | 4 | 200 m | 2048 | yes | light size 1.0 |
| T4 | 4 | 300 m | 4096 | yes | light size 1.0 |

The light size was chosen by eye against long evening shadows on the kart circuit (fence posts): 1.0 softens them as they lengthen; 4.0 blurred them away.

The first cascade always ends 12 m from the camera.

# Cost

On the Intel Iris Xe (Raptor Lake-P, Mesa 26.2), 1920×1080, 2026-11-10, first scene: two cascades of 1024 over 80 m cost 2.6 ms; three of 2048 over 120 m, 4.7 ms. With the scenery (about 3 000 objects) the T2 setting costs 2.3 ms at 1632×918. Each object is drawn once more per cascade: the cost follows the number of objects within reach as much as the map size. Soft shadows at T3 cost about 3 % of the frame on the Iris Xe at 2560×1440 (kart circuit, evening start view, 2026-11-11: 119 ms without, 123 ms with).

# Limits

Below T3, hard-edged shadows. Beyond the reach there is no shadow at all, which shows on hills.

# Oracle

[The render bench](/oracles/render-bench.md), with and without `--without shadows`.

# Reuse

The table and its method: start from the budget, measure each candidate setting on the weakest hardware of the tier, keep the best that fits.

[^csm]: Cascaded Shadow Maps
[^bevy]: Bevy 0.20 documentation
[^pcss]: Percentage-Closer Soft Shadows
