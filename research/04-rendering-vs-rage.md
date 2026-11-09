# 04 — Rendering: RAGE / GTA 6 as the bar, and what open source has

Survey date: **2026-11-09**. Part A is only partly verified: Digital Foundry's site and several game-news sites blocked automated fetching, so GTA 6 claims rely on secondary write-ups of Digital Foundry (DF) videos. Parts B and C are better grounded (GitHub via `gh`, official release notes). Items marked *unverified* are flagged.

## A. The bar

### GTA 6 (RAGE)

Sources: DF's analysis of the ~26–27-minute "Extended Look" (Aug 2026), DF's launch-trailer breakdown (2026-10-16), DF's Trailer 2 coverage (May 2025).

| Area | What is publicly known |
|---|---|
| Resolution / framerate | Base PS5 ≈ 1440p internal, 4K output; DF saw no dynamic resolution and a locked 30 fps with rare drops. 30 fps stated by Rockstar North's Rob Nelson in an interview with a YouTuber; **no official Rockstar statement** |
| 60 fps | DF: extremely unlikely on current consoles because the game is **CPU-bound**. A 40 fps PS5 Pro mode is DF's guess only |
| Upscaling | DF: probably a simple spatial upscaler; no clear sign of FSR2/TAAU, no evidence of Sony's ML upscaler in the launch trailer |
| Global illumination | **Extensive ray-traced GI** (bounce light, colour bleeding, visible noise in some scenes). DF (May 2025): RTGI is part of how the game is built, so it cannot simply be switched off for a 60 fps mode. DF itself hedges ("some kind of RTGI") |
| Reflections | Ray-traced, low resolution, softer in motion; some mirrors appear planar |
| Shadows | Mostly shadow maps, possibly RT shadows in close-ups |
| Crowds / traffic | Very dense NPCs and vehicles; behaviour cannot be judged from curated footage |
| Animation / physics | Improved hair simulation, destructible objects; usual RAGE camera judder not visible in the launch trailer |
| Water | Sunlit droplets, ripples when wading through swamp, boat wakes (wakes: secondary source only) |
| Lineage | GTA V Enhanced on PC (2025-03-04) added RT GI and AO, DLSS 3 / FSR 3, on top of existing RT shadows/reflections — DF suggested a preview of GTA 6's engine work |
| *Unverified* | Volumetric clouds, weather system, vegetation technique, LOD/streaming, Euphoria (fan inference from Take-Two patents, not confirmed) |

**Reading:** GTA 6's bar is less about one exotic technique than about **ray-traced lighting baked into the design, at extreme content and simulation density, on fixed console hardware — and even Rockstar is CPU-bound at 30 fps.**

### Unreal Engine 5.8 (2026-06-17), the commercial open-world benchmark

- Production-ready: **MegaLights**; PCG and Substrate (since 5.7).
- Beta: **Lumen Lite** (irradiance fields, ~2× faster).
- Experimental: Mesh Terrain (Nanite-based, caves/overhangs), Procedural Vegetation Editor (Nanite foliage), MetaHuman Crowd (on Mass), Fast Geometry Streaming; Nanite Foliage (assemblies, voxels, skinning) since 5.7.
- Lumen converging on hardware ray tracing at 60 Hz; World Partition with custom HLODs.
- UE6 previews expected "late 2027-ish" (sources disagree).

## B. Open-source building blocks

### Engines and renderers

| Project | Licence · latest | Relevant features | Maturity |
|---|---|---|---|
| **Bevy** | MIT/Apache · 0.20.1 (2026-10-28) | **Solari** RT lighting (experimental, needs RT hardware; ReSTIR off by default in 0.20; DLSS-RR denoising on NVIDIA only); **meshlet virtual geometry** behind an experimental flag; GPU-driven batching, GPU light clustering, atmosphere, volumetric fog, contact shadows, SSR, initial mesh-shader support | Very active; pre-1.0 with breaking releases; no editor |
| **Godot** | MIT · 4.7.2 (2026-08-18) | SDFGI, VoxelGI, LightmapGI; HDDAGI rewrite (PR #119869) still open; Vulkan RT plumbing merged Jan 2026 for 4.7 (low-level only, no RT effects yet); 4.6 reworked SSR and made Jolt the default physics | Mature editor; no first-party streaming/HLOD; Terrain3D (MIT) third-party |
| **Wicked Engine** | MIT · v0.72.139 (2026-11-03) | DDGI, surfel GI, SSGI, voxel GI; RT shadows/AO/reflections; GPU path tracer; volumetric clouds; FFT ocean; virtual textures; texture streaming; Jolt | **Most complete open feature set**; PS5/Xbox ports exist but are private; essentially one maintainer |
| **O3DE Atom** | Apache/MIT · 26.05 (May 2026) | Diffuse Probe Grid (real-time RT GI with baked fallback), sky atmosphere, terrain gem | Full editor, small ecosystem |
| **Stride** | MIT · 4.4.1 (Oct 2026) | D3D12/Vulkan backend rewrite | No modern GI |
| **Fyrox** | MIT · 1.0 (Mar 2026) | PBR | No GI (planned for 2.0) |
| **The Forge** | Apache-2.0 · GitHub v1.63 (Mar 2025), continued on Codeberg | Triangle visibility buffer with programmable MSAA; used for Hades, Starfield platform work, Warzone Mobile | Its RT GI middleware and Ephemeris sky are **not public**; console backends need a paid licence |
| **Diligent** | Apache · active (last tag v2.5.6, 2024) | D3D11/12, Vulkan, Metal, WebGPU, GL/GLES3/WebGL2; RT and mesh shaders at API level | Framework, not an engine |
| **Filament** | Apache · v1.80.0 (Nov 2026) | GLES3/WebGL2 → Vulkan/Metal/WebGPU; clustered forward, SSR | No GI or RT |
| **bgfx** | BSD-2 · active, no tags | Very wide backend support; used by Minecraft, Guild Wars 2 | Draw-call layer only |
| **Granite** | MIT | Personal Vulkan renderer | "Do not expect any support" |
| **kajiya** | Apache | — | **Archived** |

### Vendor SDKs — mostly *not* open source

- **NVIDIA** RTXGI 2.8 (NRC/SHaRC), RTXDI 3.1, NRD 4.18, DLSS 310.9 use the NVIDIA RTX SDKs License, whose clause 4e forbids using the SDK "in any manner that would cause it to become subject to an open source software license" — **not open source, and in conflict with copyleft licences such as opage's AGPL-3.0.** DLSS is NVIDIA-only. **Streamline is MIT.** RTX Remix runtime (dxvk-remix, zlib) only remasters DX8/9 games.
- **AMD FSR SDK 2.4.0 (Oct 2026):** core is **binary-only, no reverse engineering**; only samples are MIT. Includes FSR4 ML upscaling, Ray Regeneration (ML denoiser), radiance-caching preview. **Last MIT AMD source: FidelityFX SDK 1.1.4 (May 2025)** with FSR 3.1 and **Brixelizer GI**.
- **Intel XeSS 3.0.2:** binary-only licence.

### Libraries

- **meshoptimizer v1.3 (Sep 2026, MIT):** meshlet building + `clusterlod.h`, the Nanite-style cluster-LOD core, now with foliage-friendly simplification.
- **Jolt 5.6 (MIT):** physics in Horizon Forbidden West and Death Stranding 2.
- **Recast/Detour (Zlib):** navigation; last release 2023, commits up to Feb 2026.

### Open Nanite-like / Lumen-like work

- Geometry: Bevy meshlets, nanite-webgpu (MIT, demo), meshoptimizer clusterlod.
- GI: Godot SDFGI/HDDAGI, Brixelizer GI, Wicked DDGI/surfel GI, Bevy Solari.
- **None is proven in a shipped open-world game.**

### Scaling from ray tracing down to GLES3 / WebGL2

- **Godot:** Forward+ → Mobile → Compatibility (OpenGL; the only web option). SDFGI, VoxelGI, SSIL and volumetric fog are dropped on lower tiers; switching tiers needs manual scene tweaks.
- **Bevy:** WebGL2 and WebGPU builds work with limitations; no mesh shaders on the web.
- **Diligent, bgfx, Filament:** GLES3/WebGL2 up to modern APIs, but no GI of their own.
- **Wicked, O3DE, The Forge:** no GL/WebGL backend.

## C. Main gaps, hardest first

1. **Content pipeline and world tools.** No open equivalent of World Partition, automatic HLOD, production-ready PCG and a dense-asset pipeline. Bevy has no editor; Godot has no first-party world streaming. Beyond that, GTA-scale content is a staffing problem no engine solves.
2. **Shipping on consoles at that density.** Open console backends are private or commercially licensed (Wicked, The Forge) — console SDKs are under NDA. GTA 6 is CPU-bound even for Rockstar, and open engines have no equivalent of Unreal's Mass framework for crowds and traffic.
3. **Production-grade dynamic GI, reflections and denoising on console-class RT hardware.** Solari is experimental, Wicked depends on one person, SDFGI uses no RT, HDDAGI isn't merged. The best denoisers and upscalers (NRD, DLSS-RR, FSR4/Ray Regeneration, XeSS) are proprietary or binary-only; the open fallbacks (FSR 3.1, Brixelizer) are frozen at 2025 code.
4. **Virtual geometry for dense foliage and skinned meshes.** Only experimental implementations; meshoptimizer provides building blocks.
5. **Animation and behaviour.** No open Euphoria-class system; Jolt and Recast cover the basics.
6. **Easiest: sky, volumetrics, water, post-processing.** Already in Wicked, Bevy and Godot.

**Verdict:** the best open renderers can match some pieces of GTA 6 in a tech demo — Wicked has the broadest feature set, Bevy and Godot the strongest communities. None comes with the tools, the console path or the hardened RT GI needed at GTA 6 / UE5 open-world scale today.

## Takeaways for opage

1. The renderer gap is real but **narrower than the tools/content gap**; the most valuable contributions are world tooling, streaming/HLOD and crowd/traffic simulation at scale.
2. **Open denoising and upscaling are a strategic hole**: the state of the art is proprietary, and opage's AGPL licence rules out NVIDIA's SDKs. An open ML denoiser/upscaler is a high-leverage project.
3. **Consoles are out of reach for an AGPL project** (NDA SDKs); target PC (Windows/Linux/macOS), mobile and the web.
4. **Bevy's direction (GPU-driven, meshlets, Solari) is the closest open trajectory to the RAGE/UE5 bar**; Wicked is the reference to study for breadth; meshoptimizer and Jolt are safe dependencies.

## Not verified

Diligent's and Filament's production users; whether third-party Godot console ports exist; Bevy virtual-geometry performance at scale; the GTA 6 items marked *unverified* above.

## Sources

- GTA 6: https://www.youtube.com/watch?v=am-DB9lgNZ0 · https://gtaintel.com/news/gta-6-launch-trailer-digital-foundry-analysis · https://mobidevices.com/en/digital-foundry-gta-6 · https://www.notebookcheck.net/GTA-6-PS5-Netflix-trailer-analysis-warns-PC-port-may-struggle-to-reach-60-fps.1381899.0.html · https://www.tomshardware.com/video-games/grand-theft-auto-6-will-run-at-only-30-fps-on-consoles-at-launch-no-performance-mode-promised-even-the-souped-up-ps5-pro-might-not-get-60-fps-support
- Unreal: https://dev.epicgames.com/documentation/unreal-engine/unreal-engine-5-8-release-notes · https://gamefromscratch.com/unreal-engine-5-8-released/ · https://tomlooman.com/unreal-engine-5-7-performance-highlights/
- Open source: https://bevy.org/news/bevy-0-20/ · https://jms55.github.io/posts/2026-04-12-solari-bevy-0-19 · https://github.com/godotengine/godot/pull/99119 · https://docs.godotengine.org/en/stable/tutorials/rendering/renderers.html · https://github.com/turanszkij/WickedEngine/blob/master/features.txt · https://github.com/ConfettiFX/The-Forge · https://github.com/zeux/meshoptimizer/releases
- Vendor licences: https://github.com/NVIDIA-RTX/RTXGI/blob/main/License.md · https://github.com/GPUOpen-LibrariesAndSDKs/FidelityFX-SDK/blob/main/docs/license.md
