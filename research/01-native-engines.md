# 01 — Native open-source engines

Survey date: **2026-11-09**. Stars and release dates come from the GitHub API on that day. "Authors in 2026" counts distinct commit e-mails on each default branch since 2026-01-01 from a local clone — a rough measure (one person may use several e-mails). Items marked *unverified* could not be confirmed from a primary source.

Question: which existing engine, if any, could serve as the foundation (or as a client) of a community-built open-world engine with RAGE-class ambitions, that scales down to old hardware and lets many human + agent teams work in parallel?

## Summary table

| Engine | Licence | Latest | 2026 authors | Low-end path | Web | Open-world kit | Modularity |
|---|---|---|---|---|---|---|---|
| **Godot** | MIT | 4.7.2 (2026-08-18), 4.8 beta 2 | ~526 | Compatibility renderer (GL 3.3 / GLES3 / WebGL2) | WebGL2 only | plugins (Terrain3D), no streaming, no virtual geometry | GDExtension, modules, addons |
| **Bevy** | MIT/Apache-2.0 | 0.20.1 (2026-10-28) | ~420 | wgpu GLES3/WebGL2 (no compute features) | WebGL2 + WebGPU | GPU-driven, GPU occlusion, experimental meshlets, Solari RT GI; no terrain/streaming | **everything is an ECS plugin** |
| **O3DE** | Apache-2.0/MIT | 26.05.0 (2026-05-27) | ~57 | none (Vulkan/DX12/Metal) | no | Terrain gem, LODs, probe GI | Gems |
| **Stride** | MIT | 4.4.1 (2026-10-17) | ~53 | none (OpenGL removed in 4.4) | no | none built in | — |
| **Wicked** | MIT | v0.72.139 (2026-11-03) | ~15 (one lead) | none (DX12/Vulkan/Metal) | no | terrain, virtual texturing, meshlets, DDGI/surfel/VXGI/path tracing | monolithic |
| **Fyrox** | MIT | 1.0.0 (Mar 2026) | ~9 | GL 3.3 / GLES 3.0 / WebGL2 | WebGL2 | quadtree terrain, LOD groups, GPU occlusion; no RT GI | editor + hot reload |
| **Flax** | *source-available*, 4 % royalty | 1.13 (2026-10-21) | ~32 | DX11 (DX10 fallback) | experimental WebGPU | terrain, foliage, DDGI, HZB occlusion, double precision | C# scripting |

## Godot (MIT)

- **Releases:** 4.7 on 2026-06-18, 4.7.2 on 2026-08-18; 4.8 beta 2 on 2026-10-29. Feature release every ~4–5 months.
- **Health:** 119.8k stars, ~526 authors and ~6.4k commits in 2026. Run by the Godot Foundation (Dutch non-profit). Battlefield Studios became a Platinum sponsor in June 2026. W4 Games (founded by core developers) raised an $18M Series B led by Tencent in August 2026. Redot is a community fork (6.2k stars).
- **Open world:**
  - Terrain via the **Terrain3D** plugin (v1.1.0, 2026-10-30, Godot 4.5–4.8), up to 65.5 × 65.5 km.
  - **No built-in world streaming**; 4.8 adds mip-level texture streaming (beta).
  - Automatic mesh LOD and visibility ranges (usable as simple HLOD).
  - CPU occlusion culling (Embree) with baked occluders.
  - **No virtualised geometry, no GPU-driven rendering.**
  - GI: SDFGI, VoxelGI, SSIL on Forward+ only; LightmapGI and reflection probes everywhere.
  - Double precision requires a custom build (`precision=double`); GPU stays 32-bit with camera-relative emulation.
- **Hardware:** Forward+ and Mobile renderers on Vulkan / D3D12 / Metal; a separate **Compatibility** renderer on GL 3.3 / GLES3 / WebGL2 — the best low-end story of all engines here.
- **Web:** WebGL2 only (Compatibility). No C# on the web. Single-threaded by default (threads need COOP/COEP). Wasm ~27–42 MB raw, 4–9 MB compressed (*community figures*). WebGPU only in a community fork (*unverified*).
- **Modularity / AI:** GDExtension, compile-time modules, addons, new Asset Store in 4.7. Text scenes (`.tscn`). `--headless`. Many community MCP servers (largest: Coding-Solo/godot-mcp, 6.6k stars).

## Bevy (MIT/Apache-2.0)

- **Releases:** 0.20.0 on 2026-10-08, 0.20.1 on 2026-10-28; every ~3–4 months, **breaking changes each time**.
- **Health:** 49.6k stars, ~420 authors and ~2.1k commits in 2026; ~227 contributors to 0.20. Bevy Foundation (US 501(c)(3)) receives ~$11.4k/month from 216 members.
- **Open world:**
  - No built-in terrain or streaming; community terrain crates are stale.
  - **GPU-driven rendering** (more work moved to the GPU in 0.19) and GPU two-phase occlusion culling.
  - Visibility ranges (HLOD-like).
  - **Experimental `meshlet` virtual geometry** (Nanite-like).
  - GI: **Solari** path-traced GI (needs hardware ray tracing), irradiance volumes, lightmaps.
  - Large worlds via the `big_space` crate (0.13 still targets Bevy 0.19).
- **Hardware:** wgpu → Vulkan, DX12, Metal and GLES3/WebGL2; compute-based features unavailable on WebGL2.
- **Web:** WebGL2 and WebGPU (separate builds). Wasm ~13–30 MB raw before optimisation (*old figures*).
- **Modularity:** everything is a plugin on the ECS — **the best fit for many teams working in parallel**.
- **Editor:** still no official editor. BSN (scene format usable from code) shipped in 0.19; `.bsn` file loading had not shipped. `bevy_inspector` landed on main on 2026-09-28 (after the 0.20 branch). Community editor: Jackdaw (0.20.0).
- **AI:** Bevy Remote Protocol (JSON-RPC) + community `bevy_brp_mcp` (0.23, targets 0.20).

## O3DE (Apache-2.0/MIT, Linux Foundation)

- **Releases:** 26.05.0 on 2026-05-27; twice a year. 26.10 still stabilising (branch active 2026-10-31; draft notes: Qt 6 editor, experimental Apple Silicon, early WebAssembly groundwork). Not released.
- **Health:** 9.8k stars but only ~57 authors and ~485 commits in 2026 — the weakest momentum of the big three; robotics (ROS 2) is a large share of the work.
- **Open world:** built-in Terrain gem (macro/detail materials, heightfield physics); mesh LODs with automatic LOD generation (new in 26.05); masked occlusion culling and occlusion planes; ray-traced Diffuse Probe Grid GI. World streaming, HLOD and double precision: *nothing found*.
- **Hardware:** Vulkan, DX12, Metal only. No OpenGL, no web.
- **Modularity / AI:** strong Gem system, JSON prefabs, Python editor bindings; only a 3-star MCP gem found.

## Stride (MIT, C#)

- **Releases:** 4.4.0 on 2026-10-09 (~3,200 commits since previous), 4.4.1 on 2026-10-17; irregular.
- **Health:** 7.9k stars, ~53 authors in 2026; .NET Foundation project funded by donations.
- **Open world:** no built-in terrain, LOD or streaming; community StrideTerrain (*status unverified*).
- **Hardware:** OpenGL removed in 4.4; D3D11 (may be dropped), D3D12, Vulkan 1.3. No web.
- **Tooling:** new `stride` CLI; Windows-only editor (Linux via Wine/Proton), Avalonia rewrite under way.

## Wicked Engine (MIT, C++)

- **Releases:** rolling builds, v0.72.139 on 2026-11-03, roughly monthly. Essentially one lead developer (~15 authors in 2026), 7.3k stars.
- **Open world:** chunked procedural terrain with virtual texturing, texture streaming, meshlets, occlusion culling; GI: DDGI, surfel GI, VXGI, path tracing. Double precision *unverified*.
- **Hardware:** DX12, Vulkan, Metal only. No web.
- **Modularity:** Lua scripting; monolithic design. A rich renderer to **learn from**, a bus-factor risk to **build on**.

## Fyrox (MIT, Rust)

- **Releases:** 1.0.0 in late March 2026 after 7 years; no release since.
- **Health:** 9.7k stars but ~9 authors in 2026; the 1.0 post says the team is small with little or no funding.
- **Open world:** built-in quadtree-LOD terrain, LOD groups, grid-based GPU occlusion culling, reflection probes, no real-time GI, no double precision.
- **Hardware / web:** GL 3.3, GLES 3.0, WebGL2; wasm export. A wgpu backend was merged in August 2026, unreleased.
- **Tooling:** full editor with hot reload, text scenes by default, export CLI.

## Flax (source-available — not open source)

- **Licence:** EULA, 4 % royalty on revenue above $250k per quarter; full source on GitHub.
- **Releases:** 1.13 on 2026-10-21, hotfix 1.13.7014.1 on 2026-11-04; ~2 releases a year. 7.1k stars, ~32 authors in 2026, company-led.
- **Open world:** built-in terrain and foliage, async level preloading, production-ready DDGI, HZB / hardware-query occlusion (new in 1.13), optional double precision (`UseLargeWorlds`) with origin shifting — **the most complete open-world kit of this list, but not usable as an open-source base.**
- **Hardware / web:** DX11 (DX10 fallback), DX12, Vulkan; experimental WebGPU-only web export since 1.12 (May 2026), no C# on the web yet.
- **AI:** an `AGENTS.md` in the repo.

## Other contenders

- **Cocos 4:** moved to MIT in January 2026, pitched as "AI-native" (MCP/agents, editor moving to headless CLI tools). 3D is immature. 1.5k stars.
- **Balaur:** Rust engine from the Rapier physics team, MIT, deterministic. Started August 2026; 102 stars.
- **Spartan:** GPU-driven meshlet renderer, Hi-Z occlusion, path-traced GI — but a custom non-OSI licence.
- **Tiny agent-first engines** (nAIVE, SpawnForge, NodeTool): too immature to build on.

## Benchmark: Unreal Engine 5.8 (source-available, commercial)

- **5.8 released 2026-06-17** (5.8.1 hotfix since); probably the last 5.x. UE6 previews expected around late 2027 (*GameFromScratch, unverified*).
- New in 5.8: experimental **Mesh Terrain** (Nanite-based, caves and overhangs); experimental Procedural Vegetation Editor producing Nanite foliage; **Lumen Lite** (beta, ~2× faster than full Lumen); **MegaLights production-ready**; new World Partition HLOD build/debug tools; an experimental first-party MCP server plugin.
- Long-standing: World Partition, One File Per Actor (many people editing one map), Nanite, Lumen, Virtual Shadow Maps, double-precision Large World Coordinates, PCG.
- Licence: 5 % royalty on lifetime gross above $1M per product (3.5 % when launching first on the Epic Games Store) (*from search snippets; licence page refused the fetch*).
- No official web export. DX11/SM5 support in 5.8 *unverified*.

## Takeaways for opage

1. **No open-source engine matches UE5's — let alone RAGE's — open-world stack.** Each one covers a different subset.
2. **Godot** has the strongest community and funding and the only serious low-end + web path, but terrain and large worlds come from plugins/custom builds, and it lacks virtualised geometry and GPU-driven rendering.
3. **Bevy** is the most modular (ECS plugins map naturally onto many independent human + agent teams) and the most modern in GPU-driven rendering (meshlets, GPU occlusion, Solari RT GI), and runs in the browser — but no editor yet and API churn every release.
4. **O3DE** has a fitting architecture (Gems, terrain) but weak momentum and no low-end/web path.
5. **Flax** has the most complete open-world kit but its licence excludes it as a base; **Wicked** is the best renderer to study but depends on one person.
6. Agent tooling exists for Godot and Bevy (MCP servers, remote protocols), and Unreal now ships its own MCP plugin — **agent-operability is becoming table stakes**.

## Sources

- Godot: https://github.com/godotengine/godot/releases · https://godotengine.org/blog/ · https://godotengine.org/releases/4.7/ · https://godotengine.org/article/dev-snapshot-godot-4-8-beta-1/ · https://docs.godotengine.org/en/stable/tutorials/export/exporting_for_web.html · https://docs.godotengine.org/en/stable/tutorials/physics/large_world_coordinates.html · https://docs.godotengine.org/en/stable/tutorials/3d/occlusion_culling.html · https://docs.godotengine.org/en/stable/tutorials/3d/global_illumination/introduction_to_global_illumination.html · https://github.com/TokisanGames/Terrain3D/releases/tag/v1.1.0-stable · https://terrain3d.readthedocs.io/en/latest/docs/double_precision.html · https://w4games.com/blog/w4-games-news-1/w4-games-raises-18-million-to-accelerate-international-presence-193 · https://www.cgchannel.com/?p=176856 · https://talks.godotengine.org/godotcon-boston-2026/talk/CT739D/ · https://github.com/Coding-Solo/godot-mcp
- Bevy: https://bevy.org/news/bevy-0-20/ · https://bevy.org/news/bevy-0-19/ · https://bevy.org/donate/ · https://crates.io/crates/big_space · https://crates.io/crates/bevy_brp_mcp · https://crates.io/crates/jackdaw
- O3DE: https://github.com/o3de/sig-release/blob/main/releases/26.05.0/26.05.0.md · https://github.com/o3de/sig-release/blob/main/releases/26.10.0/26.10.0.md · https://o3de.org/a-look-back-at-2025-and-what-to-look-forward-to-in-2026/ · https://docs.o3de.org/docs/atom-guide/dev-guide/rhi/
- Stride: https://doc.stride3d.net/latest/en/releasenotes/ · https://doc.stride3d.net/latest/en/community-resources/terrain-and-water.html
- Wicked: https://github.com/turanszkij/WickedEngine
- Fyrox: https://fyrox.rs/blog/post/fyrox-game-engine-1-0-0/
- Flax: https://flaxengine.com/blog/flax-1-12-released/ · https://docs.flaxengine.com/manual/release-notes/1_13/index.html · https://flaxengine.com/licensing/ · https://docs.flaxengine.com/manual/editor/large-worlds/index.html · https://docs.flaxengine.com/manual/platforms/index.html
- Unreal: https://dev.epicgames.com/documentation/en-us/unreal-engine/unreal-engine-5-8-release-notes · https://gamefromscratch.com/unreal-engine-5-8-released/ · https://www.unrealengine.com/en-US/license
- Others: https://www.prnewswire.com/news-releases/cocos-4-is-here-fully-open-source-302652264.html · https://github.com/balaurengine/balaur · https://github.com/PanosK92/SpartanEngine
