---
type: Module
title: osage-sound-web
description: osage-sound in a browser — the engine built as WebAssembly with a handful of functions over numbers and memory, played in an AudioWorklet whose 128-frame render quantum is the engine's block, so that each block is made when the output needs it, with every pose told until then.
resource: https://github.com/smichea/osage/tree/main/crates/osage-sound-web
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [sound, webassembly, audioworklet, browser]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-24T12:00:00Z" }
---

# Purpose

Play [osage-sound](/modules/osage-sound.md) in a browser client (issue #16): the same engine, the same scene, the same samples as natively, heard as the player moves. The voices are generators, so nothing but the scene is fetched for sound.

# Concepts

- **The module:** `osage_sound_web.wasm`, built for `wasm32-unknown-unknown`; it imports nothing and exports its memory and ten functions.
- **The worklet:** an `AudioWorkletProcessor` named `osage-sound` that opens the engine on a scene and makes one block in each call of `process()`. The context must run at 48 kHz, the engine's rate.
- **A pose:** a node's placement in its parent and its velocity, posted to the worklet's port; it is given to the engine before the next block (`Engine::drive`).

# Architecture

There is no thread and no queue. The browser's audio thread calls `process()` for each render quantum of 128 frames, which is the engine's block: the processor calls `sound_render()`, which places the tree, finds the ways that are due, makes the voices and carries them, and copies the 128 left and 128 right samples into the two output channels. A pose posted to the port is handled before the next call. So a pose is heard within one block, plus the output's own buffer, which is the context's `baseLatency` and `outputLatency`.

Opening the engine makes the sound already on its way (three seconds of blocks, as an offline render does before its first sample): it happens in the processor's constructor, before the first quantum is due.

The interface (`src/lib.rs`), in the style of `osage-kart-physics`'s WebAssembly interface:

| Function | Does |
|---|---|
| `sound_text(len) → address` | room for `len` bytes, where the caller writes a scene or a node's path |
| `sound_open(quality, headphones, start) → 1 or 0` | the engine on the scene written: quality 0 to 3 (low to reference), headphones 1 or loudspeakers 0, from the instant `start` |
| `sound_error() → address`, `sound_error_len()` | why a scene did not open |
| `sound_node(len) → index or −1`, `sound_listener()` | a node by its path (identifiers joined by ` / `), the listener's node |
| `sound_drive(node, x, y, z, qx, qy, qz, qw, vx, vy, vz)` | a node placed and going on at a velocity, from the next block |
| `sound_render() → address` | the next block: 128 samples of the left ear, then 128 of the right |
| `sound_rate()`, `sound_block()` | 48 000 and 128 |

The engine's tally reads no clock on this target (`wasm32-unknown-unknown` has none).

`web/osage-sound.js` wraps the module in a class (`Sound`: `node`, `listener`, `drive`, `render`) used alike by a page and by the worklet; `web/osage-sound-worklet.js` is the processor. An AudioWorklet has no `TextEncoder`, so text is encoded to UTF-8 by hand. The processor times each block on the best clock it has (`performance.now` where it exists, `Date.now` in Chromium's worklet) and answers `{cost: true}` on its port with the blocks made, their mean and worst time and how many were certainly over their budget.

# Interfaces

The exported functions above; `osage_sound_web::open(scene, quality, listening, start)` for Rust callers; `Sound` and `QUALITIES` in `web/osage-sound.js`; the processor `osage-sound` (options `module`, `scene`, `quality`, `headphones`, `start`; port messages `drive`, `node`, `cost`).

# Invariants

- Opened on a scene, its blocks are those of an offline render of the same scene and quality, sample for sample (a test of the crate, natively).
- In Chromium, two seconds of the bridge rendered through the worklet in an `OfflineAudioContext` are the samples of the same module called directly, at low and medium (`tools/paris-soundscape/web/live.py check`, run by continuous integration).
- A node is found by its path; a scene that does not open says why. Tested.

# Knowledge

- Measured with `tools/paris-soundscape/web/live.py play`: the bridge for 45 s in a headless Chromium 155, the listener walked by a pose sixty times a second, on the Intel Core i5-1335U while other sessions loaded it (load 6 to 7), 2026-11-24. At low, 0.35 ms a block on average (13 % of the 2.67 ms budget) and no block over its budget; at medium, 0.74 ms (28 %) and 4 blocks of 17 132 over it, the worst 5 ms, which the context's 10.7 ms buffer can absorb. A headless browser plays to a fake output on its own clock: this says whether the engine keeps up, not what a sound card adds.
- Chromium 155 exposes neither `AudioContext.renderCapacity` nor `playoutStats`, even with its experimental features: the page reports them where a browser has them.
- The native counterpart is `osage_sound::live`, a thread a few blocks ahead of the output.

# Reuse

Any engine written in Rust can be played in an AudioWorklet this way: a block of 128 frames, a C interface over numbers and memory, the compiled module sent to the worklet in its options.

# Limits

- One engine per module instance; the worklet makes it on the audio thread, where a block that takes longer than its budget is a glitch.
- No shared memory: poses travel by the port's messages, which the browser delivers between quanta.
- Built and checked in Chromium only; other browsers' AudioWorklets are untested.
- The voices use Rust's own sine and exponential in WebAssembly: the samples differ in their last bits from a native render (but not from the same module called directly).
