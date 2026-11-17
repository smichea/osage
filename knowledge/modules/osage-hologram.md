---
type: Module
title: osage-hologram
description: Holographic textures (issue #7) — an object's appearance from every viewing angle, baked, and compressed as a video along the viewing direction.
resource: https://github.com/smichea/osage/tree/main/crates/osage-hologram
licence: MIT OR Apache-2.0
status: draft
tags: [content, rendering, research, light-field]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-17T03:00:00Z" }
---

# Purpose

Test the idea of texturing objects with how they look from every viewing angle — a field of views — compressed as video is, the step from one view to the next being a step of viewing angle (research [09](https://github.com/smichea/osage/blob/main/research/09-holographic-textures.md)).

# Concepts

- **Grid:** the viewing directions an object is photographed from, azimuths and elevations, its front facing +Z.
- **Path:** the views' order as a video: along a row of azimuths, back along the next, each view a step of angle from the last.

# Architecture

`hologram-bake OUT [MODEL] [SIZE] [X,Y,Z,R]` loads a model of the asset manifest through [osage-props](/modules/osage-props.md), off screen, and photographs it orthographically — the framing the same in every view — from every direction of the grid (`HOLOGRAM_GRID` for another), under one fixed sun, one PNG per view named by its row and column. `tools/hologram-measure.py` then compares, at several qualities, the views stored one by one (JPEG, WebP) with the views as one video along the angle (H.265, AV1) and in random order, by PSNR and SSIM.

# Interfaces

`Grid { azimuths, elevations }`, `Grid::new`, `Grid::facade`, `Grid::direction(azimuth, elevation)`, `Grid::path`, `Grid::name(row, column)`; the `hologram-bake` program.

# Invariants

- The facade grid has 15 × 9 views; the path visits each once, each a step from the last; directions are unit and azimuth 0, elevation 0 looks at the front. Tested.

# Knowledge

- Research: `research/09-holographic-textures.md` (first results: ×5–10 smaller than independent images at good quality when views are 2.5° apart).

# Reuse

Any Bevy app or tool that needs an object photographed from a grid of directions. Permissive licence.

# Limits

Baking and measuring only: nothing is drawn in a world yet. The light is baked.
