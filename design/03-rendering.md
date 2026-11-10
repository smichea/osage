# 03 — Rendering programme: towards the GTA 6 bar, together

Date: 2026-11-10 · Status: **proposed** · Bar and gap analysis: [research/04](../research/04-rendering-vs-rage.md)

osage aims at rendering in the class of RAGE / GTA 6 and at scaling down to old hardware. No single team gets there in one go: this programme organises the work so that many people and AI agents can advance it **in parallel, measurably, without stepping on each other**. The kart world is its testbed.

## 1. Principles

1. **One technique, one plugin.** Every rendering technique (sky, sun shadows, ambient occlusion, GI, reflections, vegetation…) is a Bevy plugin in `osage-render`, independent of the others, configured per **tier**. A contributor improves or adds one without touching the rest.
2. **Tiers, not one quality.** The same world renders at every tier ([research/00 §fidelity tiers](../research/00-synthesis.md#fidelity-tiers--same-world-same-source-content)): T1 minimal (old integrated GPUs, WebGL2) · T2 standard (current integrated GPUs, WebGPU) · T3 high (discrete GPUs, no ray tracing) · T4 ultra (ray tracing hardware). A technique states which tiers it serves and what it costs; lower tiers never depend on higher ones.
3. **An oracle decides "done".** Every task names the machine-checkable proof that it is finished (§3): a frame-time budget held on reference hardware, a reference image matched, a determinism hash, a conformance test. Decompilation communities finish whole games with thousands of contributors and AI agents this way — small tasks, an objective check, public progress ([research/07](../research/07-zelda-reverse-engineering.md)). A rendering change comes with the **bench** before and after; a regression beyond a tier's budget blocks it.
4. **Upstream first.** What belongs in Bevy goes to Bevy (MIT/Apache): osage carries open-world specifics (tiers, streaming, HLOD, vegetation at scale, the bench) and contributes general techniques upstream.
5. **Clean content.** Only content with a known free licence enters the repository or the asset manifest (§5).

## 2. Architecture

| Piece | Path | Licence |
|---|---|---|
| Render plugins and tiers | `crates/osage-render` | AGPL-3.0-or-later WITH osage-exceptions (ADR-0001 §5) |
| Testbed: the native kart client — online play and the bench | `worlds/kart/native` | MIT OR Apache-2.0 (D-031) |
| Kart physics, circuit, rules (shared with the server) | `worlds/kart/physics`, `worlds/kart/world` | MIT OR Apache-2.0 |
| Light tier: the browser kart client | `worlds/kart/client` | MIT OR Apache-2.0 |

`osage-render` gives the app an `OsageRenderPlugin` with a `Tier`; every technique plugin reads the tier and configures the camera marked `OsageCamera` and the sun marked `Sun`. A technique plugin is a module with its settings per tier, its documentation and a bench scene showing it.

## 3. Oracles: how "done" is decided

| Oracle | Proves | State |
|---|---|---|
| **The bench** (below) | a technique's cost stays within its tiers' budgets on reference hardware | `kart-native --bench` |
| **Reference images** | a change did not alter what is drawn, or altered it as intended | screenshots at fixed frames from the bench; automatic comparison in CI (software Vulkan) to come |
| **Determinism hashes** | the same inputs give the same bits on every platform | kart physics: native and WebAssembly checked against `golden.txt` in CI |
| **Conformance tests** | a client or a world module speaks the protocol and follows the rules | end-to-end and world tests in CI |

**The bench.** `kart-native --bench` is a timedemo. It runs without a server: the world's autopilot drives six karts round the circuit with the world's own physics, **one step per frame**, so every run draws the same frames whatever the machine; frames are drawn as fast as the machine can and their times recorded. It writes to `bench/out/<tier>-<time>/`:

- `report.json`: tier, commit, GPU, driver and CPU, resolution, frame times (mean, p50, p95, p99, worst; the worst warm-up frame apart, since first-use stutter is a defect too), share of frames over the tier's budget;
- `shot-NNNNN.png`: screenshots at fixed frames, to compare across changes and machines.

How to run it, and the results of the reference machines: [`bench/README.md`](../bench/README.md).

**Reference hardware** (others welcome: add yours to `bench/README.md`):

| Machine | Tier | Role |
|---|---|---|
| Laptop, Intel Iris Xe (Raptor Lake-P) | T2 | the "modest hardware" reference |
| Desktop with ray tracing (maintainer) | T3–T4 | the high-end reference |
| CI, software Vulkan (lavapipe) | — | correctness screenshots only, no timings |

**Budgets** (frame time at the bench's resolution; first proposal, to be refined with measurements):

| Tier | Resolution | Budget |
|---|---|---|
| T1 | 720p | 33.3 ms |
| T2 | 1080p (upscaled allowed) | 16.7 ms |
| T3 | 1440p | 16.7 ms |
| T4 | 1440p | 33.3 ms |

First results (2026-11-10, Iris Xe): T1 11.2 ms; T2 15.9 ms on average, with a third of the frames over budget. To get there, T2 gave up ambient occlusion, temporal anti-aliasing and sky lighting, each measured with `--without` ([bench/README](../bench/README.md#what-each-technique-costs)).

## 4. Workstreams

Each workstream has a scope, a state, its next milestones and its oracle. Milestones are cut into issues small enough for one contributor or agent to finish and for a machine to check (label `render:<workstream>`); a workstream's progress is its accepted milestones over its listed ones, and is public.

| # | Workstream | State (2026-11) | Next milestones | Accepted when |
|---|---|---|---|---|
| R1 | **Sky and sun**: physical atmosphere, sun, time of day | `osage_render::sky`: physical atmosphere lighting the scene (T2+), flat sky and fog (T1) | time of day; clouds (with R12) | bench screenshots at three times of day; no budget regression |
| R2 | **Shadows**: cascades, contact shadows, soft shadows | `osage_render::shadows`: cascades per tier, contact shadows T3+ | soft shadows (PCSS) T3+; tuning from the bench shots | no visible shadow acne or peter-panning in shots; budget held |
| R3 | **Ambient occlusion** | `osage_render::ambient_occlusion`: screen-space, T2+ | cost on integrated GPUs; GTAO-class quality T3 | shots; budget |
| R4 | **Global illumination** | none (T1–T3); Solari experimental (T4) | probe GI (T2–T3: irradiance volumes or DDGI-like); Solari on the desktop (T4) | indirect light visible under the bridge/barriers; budget per tier |
| R5 | **Reflections** | Bevy SSR; Solari reflections (T4) | SSR T3; RT reflections T4; reflection probes | wet asphalt and kart paint shots |
| R6 | **Materials and textures** | flat colours | PBR asphalt, grass, kerbs, barriers from CC0 sets; texture streaming later | shots; asset manifest checks |
| R7 | **Vegetation and terrain** | none | instanced grass and trees with LOD; terrain beyond the track | 10 000+ plants within T2 budget |
| R8 | **Geometry and LOD** | none | glTF kart; meshlets (T3+); HLOD for scenery | dense scenery shot within budget |
| R9 | **Vehicles and effects** | primitive kart | glTF kart model, wheel/suspension visuals, drift smoke and sparks, boost flame | shots; effects within budget |
| R10 | **Post-processing and anti-aliasing** | `osage_render::post`: HDR and tonemapping (all), TAA and bloom (T2+), motion blur (T3+) | SMAA for T1; auto exposure; colour grading | shots; budget |
| R11 | **Upscaling and denoising** | none | open temporal upscaler; open denoiser research (research/04 takeaway 2); DLSS behind licence answer (§A) | quality vs native resolution at equal time |
| R12 | **Weather, water, volumetrics** | none | volumetric fog, clouds, rain, wet surfaces | shots; budget |
| R13 | **Performance and scalability** | the bench | GPU timings per pass; automatic tier detection; frames drawn between physics steps on fast displays; a universal shader while specialised ones compile (no first-use stutter); CI budgets on reference machines | every PR shows its bench |
| R14 | **Content pipeline and tools** | none | asset manifest + fetch + licence check; glTF/KTX2 import; debug views (wireframe, overdraw, GI probes) | CI checks the manifest |

The research is clear that the hardest gap is **content and tools** (research/04 §C): R6–R9 and R14 are as important as the techniques.

## 5. Content

- Sources: **CC0** libraries — Poly Haven (HDRIs, textures, models), ambientCG (textures), Kenney (models) — and procedural generation.
- Every downloaded file is listed in [`assets/manifest.toml`](../assets/manifest.toml) (URL, SHA-256, licence, author, what uses it). Large binaries stay out of git. To come (R14): a fetch script that downloads and verifies them, and a CI check that nothing outside the manifest is used.
- Content generated by AI tools waits for the legal review of AI-generated works (follow-up 1 of the decision log).
- Never: assets or code taken from commercial games, including decompiled, recompiled or extracted ones — see the clean-sources rule in [CONTRIBUTING](../CONTRIBUTING.md#clean-sources) and [research/07](../research/07-zelda-reverse-engineering.md).

## 6. How to contribute a technique

1. Pick or open an issue in a workstream; for anything touching several workstreams or the tiers themselves, write a short design note in the PR (problem, approach, tiers served, expected cost).
2. Implement it as a plugin in `osage-render` (or a change to one), with settings per tier and a doc comment that says what it does and costs.
3. Run the bench on at least one reference machine before and after; paste both summaries and two screenshots in the PR.
4. AI agents follow the same path; their PRs are signed off by their responsible human (CONTRIBUTING).
