---
type: Module
title: osage-content
description: Content from the asset manifest made ready to draw — textures with full mipmaps and filtering, photographs of the sky decoded to linear light.
resource: https://github.com/smichea/osage/tree/main/crates/osage-content
licence: MIT OR Apache-2.0
status: draft
tags: [content, textures, hdr, manifest]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-11T02:00:00Z" }
---

# Purpose

Find the asset manifest's content wherever it was fetched, and turn its files into what a renderer needs: repeating textures that do not shimmer at grazing angles, and panoramas of the sky in linear light. A world runs without the content; with it, it gets real surfaces and a real sky.

# Concepts

- **Root:** the folder holding `manifest.toml` and the fetched files: `OSAGE_ASSETS` if set (`none` to do without), else `assets/` here or next to the build; always absolute.
- **Surface:** a Poly Haven texture set — colour, normal map, occlusion-roughness-metal — at the best resolution there is (2k, then 1k).
- **Grain:** a colour texture reduced to its light and dark around a fixed mean, so that what is painted under it keeps its colour and gains the surface's texture.
- **Equirect:** a panorama in linear light, row by row from straight up.

# Architecture

Textures are decoded on the processor (JPEG) and given their whole chain of mipmaps there: colours averaged in linear light, normals renormalised, data averaged as it is. They repeat, with anisotropic filtering. Panoramas are decoded from Radiance HDR (RGBE, flat or run-length encoded).

# Interfaces

`root()`, `Surface::{load, load_grain, material}`, `tiled`, `Kind`, `GRAIN`, `panorama::{load, decode, Equirect}`.

# Invariants

- Mipmaps of colours are averaged in linear light: a black and white checker fades to the grey of half the light, not to a darker one.
- Normals stay unit vectors down the levels.
- sRGB bytes survive the trip through linear light unchanged.
- What is not a Radiance HDR image is refused.

# Knowledge

- Used by [osage-kart-native](/modules/kart-native.md); its panoramas by [a photographed sky](/techniques/panorama.md).
- Oracle: the crate's tests, and an ignored test that decodes the manifest's sky (`cargo test -p osage-content --release -- --ignored`).

# Reuse

No dependency but Bevy and a JPEG decoder; the HDR decoder has none. Permissive licence.

# Limits

Textures are decoded at start-up on the main thread, without compression on the GPU (KTX2 and streaming are workstream R14).
