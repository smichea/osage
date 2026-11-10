---
type: Technique
title: Ambient occlusion
description: Screen-space ambient occlusion from T3, kept out of T2 by its measured cost and by a defect on integrated graphics.
resource: https://github.com/smichea/osage/blob/main/crates/osage-render/src/ambient_occlusion.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
tiers: [T3, T4]
status: draft
tags: [rendering, ambient-occlusion]
sources:
  - id: bevy
    resource: https://docs.rs/bevy/0.20.1/bevy/
    title: Bevy 0.20 documentation
generated: { by: "claude-code/claude-fable-5-2", at: "2026-11-10T06:45:00Z" }
---

# What it does

Darkens creases and the places where objects meet, where light from the sky cannot reach.

# How

Bevy's screen-space ambient occlusion[^bevy]: a pass over the depth and normal buffers estimates, for each pixel, how much of the hemisphere above it is blocked by nearby geometry. It needs a depth and normal prepass and multisampling off.

# Settings per tier

| Tier | Quality |
|---|---|
| T1, T2 | none |
| T3 | high |
| T4 | ultra |

# Cost

On the Intel Iris Xe (Raptor Lake-P, Mesa 26.2), 1920×1080, 2026-11-10: about 9.8 ms at medium quality, more than half of T2's 16.7 ms.

# Limits

**A defect to know:** on that GPU, any technique that needs a depth prepass (this one, temporal anti-aliasing) leaves speckles where the prepass and the main pass disagree on a pixel's depth. The cause is in how the two passes compute positions, not in ambient occlusion itself. To be reported to Bevy and rechecked on other GPUs before T3 relies on a prepass there.

# Oracle

[The render bench](/oracles/render-bench.md) with `--without ambient-occlusion`: the difference is the cost, and the screenshots show the speckles when they are there.

# Reuse

The decision rule: a technique that costs more than half of a tier's budget on that tier's hardware belongs to the next tier.

[^bevy]: Bevy 0.20 documentation
