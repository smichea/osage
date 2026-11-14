---
type: Technique
title: Traced-light denoising
description: At T4, the grain of Solari's traced light filtered by an edge-avoiding à-trous wavelet over the light divided by each surface's colour, guided by the G-buffer's facing, plane and the light's local spread.
resource: https://github.com/smichea/osage/blob/main/crates/osage-render/src/denoise.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
tiers: [T4]
status: draft
tags: [rendering, ray-tracing, denoising]
sources:
  - id: atrous
    resource: https://doi.org/10.2312/EGGH/HPG10/067-075
    title: Dammertz, Sewtz, Hanika, Lensch, Edge-avoiding À-Trous wavelet transform for fast global illumination filtering (HPG 2010)
  - id: svgf
    resource: https://doi.org/10.1145/3105762.3105770
    title: Schied et al., Spatiotemporal variance-guided filtering (HPG 2017)
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-14T03:00:00Z" }
---

# What it does

Traced light with few rays a pixel is grainy: a wet road sparkles, a kart is speckled. A learned denoiser (DLSS ray reconstruction) is a proprietary SDK, out of bounds here. This filter averages each pixel's light with its neighbours' where they are the same surface, so the grain goes and the edges, textures and shapes stay[^atrous].

# How

1. **The light alone.** Each pixel's colour is divided by its surface's colour from the G-buffer (never below 0.03): what is filtered is the light, and the texture is put back after, untouched.
2. **À-trous passes.** Three passes of a 3 × 3 kernel (¼ ½ ¼ in each direction) whose taps are 1, 2 then 4 pixels apart: a 15-pixel reach for 27 taps a pixel.
3. **Edge-stopping weights**, as SVGF weighs them[^svgf]: a neighbour counts less the more its facing differs (normal dot product to the 32nd power), the farther it lies off the pixel's plane (relative to its distance from the camera), and the more its light differs, measured in standard deviations of the light over the nine taps: grain spreads, an edge of shadow stays.
4. **Left as drawn:** the sky, meshes drawn forward (no G-buffer), glowing surfaces.
5. **Decoded once.** Each 8 × 8 workgroup decodes its pixels and, round them, those its taps reach (16 × 16 at most) into workgroup memory before filtering: a pixel is decoded about four times a pass, not nine.
6. **Where.** After the opaque pass, before see-through things are drawn; temporal anti-aliasing then steadies the result over frames.

# Settings per tier

T4 with ray-tracing hardware only. `OSAGE_DENOISE=0` leaves the grain (to compare); `OSAGE_DENOISE=force` filters the deferred raster picture of hardware without ray queries, to check that edges stay.

# Cost

Three compute passes of nine taps over the picture. On the cloud bench's L4 at 2560×1440, night and rain: 3.7 ms decoding every tap (34.3 ms a frame, over T4's 33.3), before the workgroup tiles.

# Limits

No temporal accumulation of its own (the anti-aliasing's history is clamped to the neighbourhood, so slow grain survives). Reflections on smooth surfaces are blurred like any light.

# Oracle

[The render bench](/oracles/render-bench.md) on the cloud GPU: T4's frame time and screenshots, with and without the filter.

# Reuse

Any deferred renderer with a G-buffer of colour, normal and depth whose lighting is noisy.

[^atrous]: Dammertz et al. 2010.
[^svgf]: Schied et al. 2017, the edge-stopping functions without the temporal variance.
