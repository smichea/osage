---
type: Technique
title: Ray-traced light
description: At T4, direct and indirect light traced through the scene by Bevy Solari on hardware with ray queries, each drawn mesh doubled for the rays and each custom ground stood in for; elsewhere T4 draws deferred without it.
resource: https://github.com/smichea/osage/blob/main/crates/osage-render/src/raytracing.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
tiers: [T4]
status: draft
tags: [rendering, ray-tracing, global-illumination]
sources:
  - id: solari
    resource: https://docs.rs/bevy_solari/0.20.1/bevy_solari/
    title: Bevy Solari 0.20 documentation
  - id: restir
    resource: https://research.nvidia.com/publication/2020-07_spatiotemporal-reservoir-resampling-real-time-ray-tracing-dynamic-direct
    title: Bitterli et al., Spatiotemporal reservoir resampling for real-time ray tracing with dynamic direct lighting (2020)
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-14T01:00:00Z" }
---

# What it does

Light as it is: the sun and the lamps reach a surface only where nothing stands between, and every lit surface lights the others in turn — the grass greening the underside of a kart, a barrier's red on the kerb beside it, the soft shadow of a tree. Raster lighting fakes these with ambient terms and occlusion; tracing rays computes them[^solari].

# How

1. **The rays' scene.** Solari traces meshes of exactly four attributes — position, normal, texture coordinates, tangent — with 32-bit indices, shaded with the standard material, and builds an acceleration structure for every such mesh. Each drawn mesh with the standard material is given a double of those four attributes (texture coordinates and tangents made up when absent), made once per mesh and kept; the drawn mesh keeps its vertex colours. A mesh of no triangles is not doubled (Solari cannot build it). A ground drawn with the custom ground material gets a stand-in: its double under a standard material of the ground's colour (its base colour times a half, a grain's mean).
2. **Hidden things stay out.** Solari traces every double, shown or not: a hidden mesh's double is taken out of the traced scene and put back when the mesh is shown — else a kart's hidden guard bubble, round kart and driver, shaded them black.
3. **The camera.** Solari draws every opaque material deferred: the camera has the deferred, depth and motion-vector prepasses, no multisampling, and a main texture it can write to. The custom ground material has its deferred shader (the same source, writing the G-buffer).
4. **Lamps.** Solari has no punctual lights: only the sun, the sky and shining surfaces light the scene. A drawn mesh marked a lamp (`Emitter`, a luminance in cd/m²) gets a traced double shining at that luminance, beside its drawn face — whose glow is set for the eye, far dimmer (D-094); the raster's spot and point lights are left to the raster.
5. **The light.** Solari's realtime lighting shades each pixel of the G-buffer with direct light sampled from the scene's lights and indirect light traced through it, a world-space cache of irradiance between frames; ReSTIR[^restir] reuses light samples between neighbouring pixels and frames, which, without a learned denoiser, is what keeps sunlit asphalt from sparkling.

# Settings per tier

T4 only, built with the `raytracing` feature. On hardware without ray queries Solari's plugins stay out and T4 is a deferred raster picture. The traced light is the first technique D-090's search leaves out of a failing T4.

# Cost

The cloud bench on an NVIDIA L4 at 2560×1440, the kart bench, 2026-11-14: without ReSTIR 16.7 ms a frame (noisy), with it 26.3 ms (30-frame budget: 33.3 ms); T3 on the same machine, 12.9 ms.

# Limits

Spot and point lights are not traced: a lamp lights T4 only if it is marked. No learned denoiser (DLSS ray reconstruction is not built): some noise remains in motion. Skinned meshes (the drivers) are traced in their rest pose. The ground's stand-in is one colour: its bounce light has none of the meadow's variety. Vertex colours (paint, grime) do not reach the rays, only the picture.

# Oracle

[The render bench](/oracles/render-bench.md) on the cloud GPU: T4's frame times and screenshots. The crate's tests: a coloured mesh's double has exactly the four attributes and 32-bit indices; a line list and an empty mesh have none. Night: the bench with `--sky night --rain`, whose T4 must not be black.

# Reuse

Any engine whose ray tracer wants a narrower vertex layout than its raster: trace doubles, keep the drawn meshes as they are, and keep hidden shapes out of the traced scene.

[^solari]: Bevy Solari 0.20: `SolariPlugins`, `SolariLighting`, `RaytracingMesh3d`, and their requirements.
[^restir]: Bitterli, Wyman, Pharr, Shirley, Lefohn, Jarosz, "Spatiotemporal reservoir resampling for real-time ray tracing with dynamic direct lighting", ACM TOG 39(4), 2020.
