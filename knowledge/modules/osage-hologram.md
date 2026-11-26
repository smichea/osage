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

`HologramPlugin` draws a `Hologram` (a folder of views baked on magenta, its grid, the photographed sphere, a stand-in rectangle): its views loaded and stacked into one texture array, an unlit material whose shader (`hologram.wesl`) blends, for each pixel, the four views around the direction of the eye, each sampled where its orthographic camera saw the point; magenta is nothing, partly magenta edges unmixed. `hologram-view` films a model and its hologram side by side; `hologram-street` times a street of 40 facades as models, holograms, or both by distance; `hologram-forest` a plot of 120 trees (about 500 million faces) as trees or as its hologram (`HOLOGRAM_FOREST` bakes such a plot, `HOLOGRAM_ASPECT` frames it wide and low, `HOLOGRAM_KEY=black` with `masked` tells the object by depth). A stand-in may be a box (`stand_depth`). Views stay on the CPU, only their stacks go to the GPU; with `window`, only a window of views around the direction it is seen from, slid as the camera moves, and widened to the directions its sphere spans when seen near (a chunk of woods beside the track spans the whole grid; a fixed window cut it down a vertical line). Holograms drawn from one folder share one stack. A hologram may fade in over a range of distances (`fade_in`) as its model's visibility range fades the model out.

`stream::StreamPlugin` draws holograms whose views come from a server ([osage-hologram-stream](/modules/osage-hologram-stream.md), D-134): an entity with `Streamed` (a server, the hologram's folder there, a window) gets its `stream.json`, then a few times a second tells the server where the viewer is, in the hologram's frame, and how fast it goes; the blocks the server answers are fetched and decoded by threads of their own, and their views fill the hologram's window as they come — where one has not come yet, the nearest that has stands in for it. `hologram-stream-view SERVER PATH OUT` films a streamed hologram turning round it and prints what came over the network.

# Interfaces

`HologramPlugin`, `Hologram { folder, grid, centre, radius, stand, flat, lit, fade_in }`, `stream::{StreamPlugin, Streamed, Stream}`, `DepthMaterial`, `NormalMaterial`, `decode_depth`, `HologramMaterial`, `Views`; `Grid { azimuths, elevations }`, `Grid::new`, `Grid::facade`, `Grid::direction(azimuth, elevation)`, `Grid::path`, `Grid::name(row, column)`; the `hologram-bake` program.

# Invariants

- The facade grid has 15 × 9 views; the path visits each once, each a step from the last; directions are unit and azimuth 0, elevation 0 looks at the front. Tested.

# Knowledge

- Research: `research/09-holographic-textures.md` (first results: ×5–10 smaller than independent images at good quality when views are 2.5° apart).

- Used by [kart-native](/modules/kart-native.md): apartment and factory blocks behind Harbour Town (D-132). Direction: in 3D only what physics needs, the rest holograms (D-133).
- Streamed by [osage-hologram-stream](/modules/osage-hologram-stream.md), as the nested cubes of [osage-cubes](/modules/osage-cubes.md) list them (D-134).

# Reuse

Any Bevy app or tool that needs an object photographed from a grid of directions. Permissive licence.

# Limits

With `lit`, it is drawn from views of its colour without light and of its normals, through the world's lighting. Each view's depth corrects the parallax (`HOLOGRAM_FLAT` not to): a `masked` hologram (a stand of trees, deep and mostly empty) marches the eye's ray through the sphere in the nearest view until it passes behind what that view saw; others settle each view's point from the stand-in; close up it is blurred at 256 px; no shadow; the light is baked; drawn blended (no depth prepass).
