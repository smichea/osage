---
type: Technique
title: Resolution and upscaling
description: The scene is drawn into a picture whose size the tier decides, then stretched to the window under a sharp interface.
resource: https://github.com/smichea/osage/blob/main/crates/osage-render/src/resolution.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
tiers: [T1, T2, T3, T4]
status: draft
tags: [rendering, resolution, upscaling]
sources:
  - id: own
    resource: "osage's own design, 2026-11-10, from measurements on an integrated GPU"
    title: osage design
  - id: bevy
    resource: https://docs.rs/bevy/0.20.1/bevy/
    title: Bevy 0.20 documentation
generated: { by: "claude-code/claude-fable-5-2", at: "2026-11-10T06:45:00Z" }
---

# What it does

Separates the number of pixels the scene costs from the number of pixels of the window.

# How

An arrangement of osage's own[^own], made of two ordinary Bevy features, a camera that renders to an image and an interface node that shows one[^bevy]:

1. The scene's camera does not draw into the window but into an off-screen picture.
2. The picture's size is `output × fit`, where `output` is the window's size in physical pixels (or a fixed size, for a benchmark) and `fit = min(1, tier_max_height / output_height) × tier_render_scale`.
3. A second camera draws only the interface. The picture is shown as the bottom-most element of that interface, stretched to the whole window with bilinear filtering; text and gauges are drawn at the window's full resolution on top.
4. When the window changes size, the picture is recreated at the new size.

# Settings per tier

| Tier | Tallest picture | Render scale |
|---|---|---|
| T1 | 720 lines | 1 |
| T2 | 1080 lines | 0.85 |
| T3 | 1440 lines | 1 |
| T4 | 2160 lines | 1 |

# Cost

It saves rather than costs. On the Intel Iris Xe (Raptor Lake-P, Mesa 26.2), 1920×1080, 2026-11-10, under load, a 1080p output took 23.2 ms drawn at full size and 18.0 ms drawn at 1632×918, measured back to back with `--without upscaling`. On a high-density laptop screen (2880×1620), not capping the height would more than double the pixels of a 1080p picture.

# Limits

Bilinear stretching softens the picture. No sharpening, no temporal upscaling yet.

# Oracle

[The render bench](/oracles/render-bench.md): the report gives both the output size and the size drawn.

# Reuse

Draw the world small and the interface sharp: it needs two cameras and one texture in any engine, and it is the first thing to do for integrated GPUs and high-density screens.

[^own]: osage design
[^bevy]: Bevy 0.20 documentation
