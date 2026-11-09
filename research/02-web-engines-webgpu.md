# 02 — Browser clients: WebGPU and web 3D engines

Survey date: **2026-11-09**. Versions and dates were checked against GitHub releases, npm, caniuse raw data and official blogs on that day. Items marked *unverified* could not be confirmed from a primary source.

Question: what can a **browser client** of an osage world realistically render today, and which web stack should it use?

## 1. WebGPU availability

| Platform | Status (Nov 2026) |
|---|---|
| Chrome / Edge desktop | Default since 113 on Windows (D3D12), macOS, ChromeOS |
| Chrome Linux | Staged rollout since Chrome 144 (Jan 2026) on Intel Gen12+; Chrome 147–148 added modern NVIDIA drivers on Wayland; AMD "tentative" (*unverified* whether shipped). caniuse: "depends on hardware and drivers" |
| Chrome Android | Since 121 on Android 12+ with Qualcomm/ARM GPUs. Chrome 146 adds a **compatibility mode** (`featureLevel: 'compatibility'`, OpenGL ES 3.1), starting with Android; D3D11/ChromeOS "exploring" |
| Firefox | Windows since 141; Apple-silicon macOS since 145 (all supported macOS versions on Apple silicon since 147). **Not** on Linux, Android or Intel Macs |
| Safari | Safari 26: macOS Tahoe 26, iOS/iPadOS/visionOS 26 (so every iOS browser). Absent on macOS 15 and earlier |

Coverage: caniuse **88.6 %** of users (86.8 % full + 1.8 % partial); web3dsurvey (real working adapters) **81.2 %** — Linux 18.8 %, Android 72.8 %, Windows 83.3 %, Firefox 61 %.

**Consequence: about 1 user in 5 (more on old laptops and Linux) still needs a WebGL2 path.** Any osage browser client must keep a WebGL2 tier.

Feature availability across adapters (web3dsurvey):

- compute shaders and `drawIndirect` / `drawIndexedIndirect`: core WebGPU;
- `indirect-first-instance`: 99.98 %;
- **multi-draw-indirect: Chromium-only experiment behind a flag (0.14 %)** — GPU-driven renderers on the web must work without it;
- `timestamp-query`: 98.8 % (Safari 93 %); `shader-f16`: 93.9 %; `subgroups`: 68 %.

## 2. three.js

- **Current release r187 (2026-10-30)**; r186 on 2026-09-24, r185 on 2026-07-01.
- **WebGPURenderer is not the default.** It is imported from `three/webgpu`; the main `three` entry is still `WebGLRenderer`. WebGPURenderer falls back automatically to a WebGL2 backend (`forceWebGL` forces it).
- **TSL** (three shading language) node materials are mandatory on WebGPURenderer; GLSL `ShaderMaterial` code does not carry over. r184 added `WebGLNodesHandler` so TSL materials also run on the classic WebGLRenderer, which allows a gradual migration.
- **Post-processing:** three.js has its own node-based `RenderPipeline` (renamed from `PostProcessing` in r183). pmndrs **`postprocessing` 6.39.6 (2026-11-01) is WebGL-only**; its maintainer says WebGPU is "not the focus right now", and the v7 alpha has been idle since April 2024.
- **BatchedMesh** runs on both renderers. r186 added `intersectsFrustum()` and `batchIndirectIndex`; r187 merges consecutive draws.
- **GPU-driven rendering, new in r187 (PR #35191):** `renderer.drawingPipeline = new MeshletDrawingPipeline()` — 64-triangle clusters, LOD by screen-space error, GPU frustum / normal-cone / two-phase depth-pyramid occlusion culling, indirect draws. A `VisibilityBufferDrawingPipeline` variant adds compute rasterisation of small triangles. WebGPU-only (falls back to the standard pipeline on WebGL); brand new, expect API churn.
- **LOD / streaming:** only the `LOD` object; examples use `@needle-tools/gltf-progressive` and `3d-tiles-renderer`. **No built-in world streaming.**
- **Gaussian splats:** native `GaussianSplat` renderer and loaders since r186, written in TSL, on both backends.
- **Threlte** (Svelte): `@threlte/core` 8.6.0 (2026-08-26) added `@threlte/core/webgpu` (`<Canvas>` awaits init, `createRenderer` prop, Vite `target: 'esnext'`); current 8.7.0. `@threlte/extras` 9.22.0 has a `/webgpu` entry but many components are stubs that throw — **`<Text>` is blocked on troika PR #393** (opened 2026-09-28, not merged).
- **React Three Fiber:** 10.0.0-alpha.6 (2026-10-17) makes WebGPURenderer the default (GLSL users move to `/legacy`); stable is 9.8.2.

## 3. Babylon.js

- **9.0 shipped 2026-03-26; latest 9.35.0 (2026-11-06). Apache-2.0.**
- `EngineFactory.CreateAsync` picks WebGPU with WebGL fallback.
- New in 9.0: Frame Graph v1, clustered lighting (WebGL2 and WebGPU), **Large World Rendering / floating origin** (`floatingOriginMode` on the scene, `useLargeWorldRendering` on the engine) with multi-region Havok physics, Geospatial Camera, 3D Tiles through 3DTilesRendererJS's Babylon renderer, splat shadows and the SOG format.
- Snapshot rendering (WebGPU-only) records and replays draw calls: a CPU-side win.
- GPUPicker (since 8.0), Node Material and Node Render Graph editors, LOD, occlusion queries.
- Havok plugin `@babylonjs/havok` 1.3.14, MIT.
- No built-in terrain system found in core (*unverified*).

## 4. PlayCanvas

- **Engine MIT, v2.23.3 (2026-10-15).** The editor frontend became MIT in July 2025 (`playcanvas/editor`) but custom builds only run from localhost; the hosted service stays commercial.
- WebGPU is opt-in via `deviceTypes`, with WebGL2 added automatically as fallback.
- 2026 work is heavily splat-centred: GPU-driven splat sort/cull and a global splat budget (v2.17); WebGPU compute radix sort and WebGPU XR (v2.19); `KHR_gaussian_splatting`, SPZ, volumetric fog, physics backend abstraction (v2.21); persistent storage buffer for mesh matrices and nearest-first streamed splat LOD (v2.23).
- SuperSplat 3.0 (Sep 2026) reported WebGPU-only (secondary source).

## 5. Other options

- **Bevy 0.20.1 (2026-10-28) on wasm:** a build targets either WebGL2 or WebGPU (the `webgpu` feature overrides `webgl2`), so two builds plus JS detection. Web WebGPU is labelled "experimental" in the examples README; web builds are single-threaded; no mesh shaders on the web. Wasm size 4.8–13 MB for a small 3D scene, but that measurement dates from 2022 (*current size unverified*).
- **Godot 4.7.2 (2026-08-18):** web export uses the Compatibility renderer (WebGL2 only). The docs state "Godot currently does not support WebGPU". No C# on the web; large wasm (gzip brings it to about a quarter).
- **Needle Engine:** stable 5.1.16, 6.0 in alpha; runs on a three.js fork (`@needle-tools/three` 0.185) and ships gltf-progressive. Commercial use needs a Pro licence. WebGPU status *unverified*.
- **3d-tiles-renderer 0.5.4 (2026-11-03, Apache-2.0):** HLOD tile traversal with fade transitions; renderers for three, Babylon, PlayCanvas and R3F.
- **pmndrs ecosystem:** drei 10.7.9 (11 in alpha); three-mesh-bvh 0.9.18 (BVH build in workers, WebGPU compute examples); Spark 2.4.0 (MIT) splat renderer for three.js.

## 6. Techniques for huge worlds in the browser

- **Procedural chunk streaming is built into none of these engines.** Generate and mesh quadtree chunks in workers, pack them with BatchedMesh/InstancedMesh. For baked/static content use 3D Tiles HLOD or gltf-progressive.
- **GPU-driven rendering:** three r187 meshlet pipeline, Babylon snapshot rendering, PlayCanvas storage buffers. Keep a WebGL2 tier with CPU culling.
- **Workers:** OffscreenCanvas with WebGL in Chrome 69, Firefox 105, Safari 17. Put world generation, BVH building and networking in workers.
- **Coordinate precision:** Babylon has a built-in floating origin; three.js does not (origin rebasing or camera-relative rendering is up to you). `reversedDepthBuffer` exists on both three.js renderers (WebGL needs `EXT_clip_control`).
- **Gaussian splats:** three r186, Spark, PlayCanvas and Babylon 9 all render them.

## Takeaways for osage

1. A browser client is a **first-class but mid-fidelity tier**: WebGPU (~81–89 % of users) with a mandatory WebGL2 fallback, no multi-draw-indirect, no mesh shaders.
2. **three.js r187 and Babylon 9 are the two credible bases.** three.js has the bigger ecosystem and now a GPU-driven meshlet pipeline; Babylon ships large-world rendering (floating origin), Havok and editors out of the box.
3. **World streaming is osage's job, not the web engine's**: the protocol must deliver content in a form both a web client and a native client can stream (chunked, HLOD, glTF/3D Tiles style) — see [03](03-world-server-replication.md).
4. For an existing three.js/Threlte client such as 2030workspace, the blockers for moving to WebGPURenderer are **troika text and pmndrs postprocessing**; the shader path (GLSL → TSL) can be migrated gradually thanks to `WebGLNodesHandler`.

## Not verified

AMD on Linux in Chrome; Edge on Linux; WebGPU in workers on Safari; whether WebGPURenderer now matches WebGL performance (2025 forum threads said slower); Needle's WebGPU status and pricing; Godot 4.7 wasm64 export (secondary source only); current Bevy wasm sizes.

## Sources

- https://caniuse.com/webgpu · https://github.com/Fyrd/caniuse/blob/main/features-json/webgpu.json
- https://web.dev/blog/webgpu-supported-major-browsers
- https://developer.chrome.com/blog/new-in-webgpu-121 (and the 144, 146, 147–148 posts)
- https://www.firefox.com/en-US/firefox/147.0/releasenotes/
- https://web3dsurvey.com/webgpu · https://web3dsurvey.com/webgpu/features/timestamp-query
- https://github.com/mrdoob/three.js/releases/tag/r187 · https://github.com/mrdoob/three.js/pull/35191 · https://github.com/mrdoob/three.js/issues/30185
- https://threejs.org/docs/pages/WebGPURenderer.html
- https://github.com/pmndrs/postprocessing/issues/700
- https://threlte.xyz/docs/reference/core/webgpu · https://github.com/threlte/threlte/blob/main/packages/extras/src/lib/webgpu/index.ts · https://github.com/protectwise/troika/pull/393
- https://github.com/pmndrs/react-three-fiber/releases/tag/v10.0.0-alpha.6
- https://blogs.windows.com/windowsdeveloper/2026/03/26/announcing-babylon-js-9-0/ · https://blogs.windows.com/windowsdeveloper/2026/03/30/part-2-babylon-js-9-0-tooling-updates-and-new-geospatial-features/
- https://doc.babylonjs.com/setup/support/webGPU/webGPUOptimization/webGPUSnapshotRendering · https://doc.babylonjs.com/features/featuresDeepDive/mesh/interactions/picking_collisions
- https://github.com/playcanvas/engine/releases · https://blog.playcanvas.com/playcanvas-editor-frontend-is-now-open-source
- https://bevy.org/news/bevy-0-20/ · https://github.com/bevyengine/bevy/tree/main/examples#wasm
- https://docs.godotengine.org/en/stable/tutorials/export/exporting_for_web.html
- https://cloud.needle.tools/faq/can-i-use-needle-engine-for-commercial-projects
- https://github.com/NASA-AMMOS/3DTilesRendererJS
- https://github.com/gkjohnson/three-mesh-bvh
