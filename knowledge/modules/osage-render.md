---
type: Module
title: osage-render
description: Rendering techniques as independent Bevy plugins, each configured for the fidelity tier the application runs at.
resource: https://github.com/smichea/osage/tree/main/crates/osage-render
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [rendering, tiers, bevy]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-11T10:00:00Z" }
---

# Purpose

Let many people improve rendering in parallel: one technique is one plugin, and no technique needs another's source. Let the same world run from an old integrated GPU to a ray-tracing card: every technique is configured per tier.

# Concepts

- **Tier** (`T1`…`T4`): a class of hardware with a frame-time budget and a resolution. T1 old integrated GPUs, T2 current integrated GPUs, T3 discrete GPUs, T4 ray tracing.
- **Technique:** one way of producing part of the picture. One module, one plugin, settings per tier.
- **Look:** how a world wants its picture to look (tonemapper, saturation, exposure offset). The engine gives the means; the look is the world's.
- **Markers:** the application marks its main camera `OsageCamera` and its sun `Sun`; techniques configure what is marked.
- **Picture:** the image the scene is drawn into before it is shown.

# Architecture

`OsageRenderPlugin { tier, without }` inserts the tier and adds one plugin per technique not listed in `without`. Each technique plugin is a few systems that react to a newly marked camera or sun and insert Bevy components on it, chosen by tier. Techniques do not call each other; where one constrains another (ambient occlusion needs multisampling off), the constraint is written in both cards.

# Interfaces

- `OsageRenderPlugin`, `Tier`, `Technique`, `Look`, `OsageCamera`, `Sun`.
- `pace::{FrameTarget, Pace, LADDER, Governor}`: the frame rate wanted, and where the picture is on the ladder.
- `resolution::Output` (follow the window, or a fixed size) and `resolution::Picture` (the image and its size).
- `Tier::budget_ms`, `Tier::bench_resolution`, `Tier::max_height`, `Tier::render_scale`.
- `ground::{GroundMaterial, Ground}`: a standard material extended for ground that varies, registered at every tier for worlds to use.

# Invariants

- A lower tier never depends on a higher one.
- A tier's set of techniques fits its budget on the reference hardware, as measured by [the bench](/oracles/render-bench.md); a change that breaks this is not accepted.
- The sun has its physical brightness at every tier, and the camera the exposure of a sunny day: materials look the same across tiers.

# Knowledge

- Techniques: [sky and sun](/techniques/sky-and-sun.md), [a photographed sky](/techniques/panorama.md), [light from the sky](/techniques/sky-light.md), [sun shadows](/techniques/sun-shadows.md), [ambient occlusion](/techniques/ambient-occlusion.md), [post-processing](/techniques/post-processing.md), [resolution and upscaling](/techniques/resolution-and-upscaling.md), [holding a frame rate](/techniques/frame-pacing.md), [ground that varies](/techniques/ground-variation.md).
- Oracle: [the render bench](/oracles/render-bench.md).

# Reuse

The plugins are thin: most of their value is the table of what each tier can afford, and how it was measured. That table transfers to any engine; see each technique's `# Settings per tier` and `# Cost`.

# Limits

No global illumination, reflections, weather or upscaler beyond bilinear yet: workstreams R4, R5, R11 and R12 of the [rendering programme](https://github.com/smichea/osage/blob/main/design/03-rendering.md). T3 and T4 settings are not measured yet.
