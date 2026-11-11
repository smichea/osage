---
type: Technique
title: A photographed sky
description: An unclipped HDR photograph of the sky drawn as the sky, lighting the scene with its sun taken out, and calibrated by its own sun so that the sky has its true brightness.
resource: https://github.com/smichea/osage/blob/main/crates/osage-render/src/panorama.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
tiers: [T1, T2, T3, T4]
status: draft
tags: [rendering, sky, image-based-lighting, hdr, photorealism]
sources:
  - id: debevec
    resource: https://www.pauldebevec.com/Research/IBL/debevec-siggraph98.pdf
    title: "Paul Debevec, Rendering Synthetic Objects into Real Scenes, SIGGRAPH 1998"
  - id: ward
    resource: https://www.graphics.cornell.edu/~bjw/rgbe.html
    title: "Greg Ward, the Radiance RGBE image format"
  - id: polyhaven
    resource: https://polyhaven.com/hdris
    title: Poly Haven HDRIs (CC0)
  - id: own
    resource: "osage's own design and measurements, 2026-11-11"
    title: osage design
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-11T02:00:00Z" }
---

# What it does

A world that gives a photograph of the sky (an equirectangular HDR image) gets a real sky instead of a computed one: real clouds, the real colour of the air at the horizon. The same photograph lights the scene from T3, the sun is placed where it is in the photograph, and the land fades into the photograph's haze with distance. Lighting a scene from a photograph of its surroundings is image-based lighting[^debevec].

# How

1. **Decode** the Radiance HDR file[^ward]: a text header, a size line, then each pixel as three 8-bit mantissas and a shared exponent `e` (value = mantissa × 2^(e − 136)), each scanline usually run-length encoded one channel at a time.
2. **Find the sun.** Take the brightest pixel of the upper half. Everything within 2.5° of it is the sun's; the ring from 2.5° to 5° is the sky around it. The sun's light is the sum, over its disc, of what exceeds that ring, each pixel weighted by its solid angle (`2π/W × π/H × cos latitude`). Its direction is the luminance-weighted mean of the disc's directions; its colour is the excess, normalised.
3. **Calibrate.** A photograph has no absolute brightness, but an unclipped one keeps the ratio of sun to sky. Scale the whole panorama so that its sun gives 100 000 lux to a surface facing it: the sky then has its true luminance (about 3 600 cd/m² overhead for the midday sky the kart world uses).
4. **The sky seen**: a cubemap of the photograph, sun included, sampled bilinearly (512 texels a face below T3, 1024 from T3), drawn behind everything at the calibrated brightness.
5. **The light**: the same photograph with the sun's disc replaced by the sky around it — the sun is a directional light of its own, counting it twice would double it — and the lower half replaced by the land: the ground's colour lit by the sun and the sky (`albedo × (E_sun sin elevation + E_sky) / π`), joined to the horizon over a few degrees. From it, the two cubemaps of image-based lighting, matte and glossy ([light from the sky](/techniques/sky-light.md)).
6. **The haze**: the mean colour of the horizon, exposed as the camera exposes, is the colour distant land fades into (visibility 6 km).

The sun of the world is turned to the photograph's sun; a world turns the photograph about the vertical to put the sun where it wants it[^own].

# Settings per tier

| Tier | Sky seen | Light from the sky |
|---|---|---|
| T1, T2 | cubemap of 512 | a constant light of the photograph's colour and brightness |
| T3, T4 | cubemap of 1024 | image-based lighting from the photograph, sun taken out |

# Cost

The work is done once, at start-up, on the processor: decoding a 4k photograph and building the cubemaps take about a second in a release build. Drawing the sky costs one pass at every tier; image-based lighting costs what it costs from any cubemap (2 to 3 ms on the Iris Xe at T2, hence T3 and above).

# Limits

An HDRI clipped at the sun (a sun no brighter than the clouds) cannot calibrate itself: its sky comes out too bright. The sky does not move — clouds stand still, the time of day is the photograph's. The land below the horizon is the world's ground colour, not the photograph's. A panorama of 4k is soft when looked at closely; 8k is sharper and four times the memory.

# Oracle

The tests of `panorama.rs` (the sun is found where it is, it sets the sky's brightness, the light leaves it out, no jump at the horizon) and of `osage_content::panorama` (the decoder, and the real sky decodes with its sun unclipped); [the render bench](/oracles/render-bench.md) screenshots.

# Reuse

Calibrating by the sun is the whole trick, and it is engine-independent: any renderer with physical light units can take an unclipped panorama from Poly Haven[^polyhaven] and get a sky, a sun and an exposure that agree, with no hand tuning.

[^debevec]: Debevec 1998
[^ward]: Ward, RGBE
[^polyhaven]: Poly Haven HDRIs
[^own]: osage design
