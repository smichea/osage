# 07 — Ocarina of Time reverse engineering: what exists, what osage may use

Date: **2026-11-10** · Technical-legal survey from public sources, **not legal advice**.

## What exists

The full reverse engineering of *Ocarina of Time* finished in November 2021. The 2026 news is probably one of two things:
- an AI-assisted iOS port of the community PC port (HarkinianPad, July 2026);
- renewed attention around Nintendo's Switch 2 remake, announced in June 2026.

| Project | What it is | Licence | Use for osage |
|---|---|---|---|
| [zeldaret/oot](https://github.com/zeldaret/oot), [zeldaret/mm](https://github.com/zeldaret/mm) | Matching C decompilations: they rebuild the original ROMs byte for byte. No assets; users need their own ROM. | none (all rights reserved); oot is moving its own contributions to CC0 ([PR #2780](https://github.com/zeldaret/oot/pull/2780), open) | **red line** for code; study how the project is organised |
| [Ship of Harkinian](https://github.com/HarbourMasters/Shipwright), [2 Ship 2 Harkinian](https://github.com/2ship2harkinian/2ship2harkinian), [HarkinianPad](https://github.com/chrissotraidis/harkinianpad) | PC and iOS ports built on the decompilations; they extract assets from the user's ROM | none / CC0 for its own code / all rights reserved | **red line** |
| [N64Recomp](https://github.com/N64Recomp/N64Recomp), [Zelda64Recomp](https://github.com/Zelda64Recomp/Zelda64Recomp) | Static recompilation of N64 code to C, and a port built with it | MIT / GPL-3.0 | methodology only |
| [RT64](https://github.com/rt64/rt64) + [plume](https://github.com/renderbag/plume) | N64 renderer (D3D12, Vulkan, Metal): ubershaders against compilation stutter, render rate decoupled from simulation with interpolation, high resolution, widescreen, streamed texture packs. Ray tracing is planned only. | MIT | **ideas**, with attribution if any code is ported; the core decodes N64 commands and is not reusable as is |
| [decomp.me](https://github.com/decompme/decomp.me), [objdiff](https://github.com/encounter/objdiff), [decomp-toolkit](https://github.com/encounter/decomp-toolkit), [splat](https://github.com/ethteck/splat), [decomp-permuter](https://github.com/simonlindholm/decomp-permuter) | The decompilation community's tools: shared scratches, diffing, automatic search, progress dashboards | MIT / Apache-2.0 | a model for osage's contributor workflow |

**AI-assisted decompilation, 2025–2026:**
- *Snowboard Kids 2* was decompiled with Claude Code running headless ([Chris Lewis](https://blog.chrislewis.au/using-coding-agents-to-decompile-nintendo-64-games/)).
- *Super Smash Bros. Melee* reached 100% on decomp.dev; 228 of its commits mention Claude.
- N64Recomp, RT64 and Zelda64Recomp **ban AI-generated contributions**: osage must not send agent-written changes to them.

## November 2026: the Switch 2 remake, recompiled within days

Nintendo's Switch 2 remake of *Ocarina of Time* came out in early November 2026. Within a week, AI-assisted projects were statically recompiling it into native PC programs (repository metadata and READMEs read on 2026-11-10; no code read):

| Project | What it is | Licence | State |
|---|---|---|---|
| [jaigner-hub/ocarina-of-time-recomp](https://github.com/jaigner-hub/ocarina-of-time-recomp) | Lifts the game's ARM code to C and links it with a host runtime that stands in for the console's OS and graphics API, on Vulkan | MIT (the tools and runtime) | created 2026-11-05; reaches the first area with rendering faults |
| [Mars7x/echora](https://github.com/Mars7x/echora) | The same idea in Rust: translates the game's 17.3 million instructions ahead of time | GPL-3.0-or-later | created 2026-11-06; does not reach gameplay |

**What is open source there is the translator and the runtime, not the game.** Neither repository contains game code or assets: each user needs their own dump of the game and the keys of their own console, and the translation happens on their machine. Its output is Nintendo's program in another language — unreadable as source, and Nintendo's. There is no open-source version of Ocarina of Time, old or new.

**For osage:**
- Nothing to reuse as code: the runtimes emulate a console's system and graphics interfaces, which is the opposite of what a renderer on Bevy needs. They also depend on console keys, which is legally sensitive.
- One useful, public datapoint: the remake's READMEs describe what a 2026 first-party engine uses — mesh and task shaders, bindless textures, GPU occlusion culling, more than 14,000 shader programs, a **neural upscaler** run on the GPU. That is the same direction as workstreams R8 (geometry) and R11 (upscaling) of [design/03](../design/03-rendering.md).
- One practice worth copying: a content check in the pre-commit hook and CI that keeps game content out of the repository.

## Legal position

- **Not clean-room.** A matching decompilation is a mechanical translation of Nintendo's copyrighted program. No court has ruled on these projects.
  - *Sega v. Accolade* protects disassembly to learn functional elements, not publishing the result.
  - The EU Software Directive allows decompilation for interoperability only (2009/24/EC Art. 6).
- **Repository licences cover only the contributors' own rights**, never Nintendo's. A repository with no licence may not be reused at all.
- **Nintendo enforces.**
  - It took down the *Super Mario 64* PC port as an "unauthorized derivative work" (2020).
  - It removes emulator repositories in waves (401 on 2026-08-17).
  - No takedown of zeldaret or Ship of Harkinian was found; that is not permission. The remake makes enforcement around anything Ocarina-branded more likely.

## Recommendations

**Use:**
- RT64's ideas: ubershaders as a fallback while specialised pipelines compile (Bevy's first-use stutter), render rate independent of simulation with interpolation, background streaming of texture packs.
- plume, as a point of comparison with wgpu.

**Learn from the decompilation community's organisation** — the main lesson for osage:
- Work is cut into small units (one function, one object).
- **An objective oracle decides "done"** (the byte-for-byte match).
- Progress is public (% matching, dashboards).
- CI rebuilds and checks hashes, and shared scratches reproduce a problem in one click.
- AI agents work in isolated workspaces, take the easiest tasks first, and let the verifier stand in for the reviewer.

osage's equivalents of matching, which [design/03](../design/03-rendering.md) can adopt:
- golden-image render tests;
- frame-time budgets per tier;
- determinism hashes, such as the kart physics golden file;
- conformance suites.

Each comes with a public "% done" per workstream and small, claimable tasks a machine can verify.

**Red lines — never in osage:**
- code from decompilations, recompiler output or ports, including Nintendo SDK (libultra) headers or names;
- leaked material;
- ROMs, extracted assets (`.z64`, `.n64`, `.v64`, `.o2r`, `.otr`), asset descriptors, ROM hashes or loaders;
- Nintendo trademarks or level designs;
- "porting" Ocarina's camera, animation or streaming code. Learn those techniques from talks, papers and experiments.

**Keeping contributors clean:**
- A clean-room rule: someone who worked on a decompilation does not implement the matching osage subsystem from memory.
- DCO provenance on every change.
- Agent instructions (`AGENTS.md`, `CLAUDE.md`) that forbid fetching decompilation or port sources while writing code.
- CI that rejects ROM and asset extensions and known SDK symbol names.
- Licence scanning with cargo-deny.
- A maintainer decides any doubt.

**Unverified:**
- the model named in the HarkinianPad coverage;
- the remake's release date;
- some 2026 press claims;
- whether zeldaret's CC0 move is complete.
