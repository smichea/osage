---
type: Module
title: osage-kart-physics
description: The kart world's arcade physics, one deterministic Rust crate run natively by the server and as WebAssembly by clients.
resource: https://github.com/smichea/osage/tree/main/worlds/kart/physics
licence: MIT OR Apache-2.0
status: draft
tags: [kart, physics, determinism, webassembly]
generated: { by: "claude-code/claude-fable-5-2", at: "2026-11-10T06:45:00Z" }
---

# Purpose

One implementation of how a kart moves, shared by everything that must agree on it: the world server, the browser client that predicts its own kart, the native client, and local simulations such as the bench.

# Concepts

- **Kart:** position `x, z`, `heading`, forward `speed`, sideways `slide`, `drift` direction, drift `charge`, `boost` time left.
- **Controls:** `throttle` and `steer` in −1…1, and the `drift` button.
- **Track:** a closed centre line, the road's half width, and the verge beyond it up to the walls.
- **Step:** one sixtieth of a second. Nothing happens between steps.

# Architecture

A pure function `step(kart, controls, track)` and a `Track` that answers two questions: where a point is relative to the centre line (`locate`), and where the centre line is at a distance from the start (`at`). No state outside its arguments, no clock, no random numbers. A small WebAssembly interface exchanges numbers only, with no shared memory.

# Interfaces

`step`, `bump` (two karts push each other apart), `Track::new`, `Track::locate`, `Track::at`, `Controls::sanitized`, `wrap`; in WebAssembly `track_clear`, `track_point`, `track_close`, `kart_step`, `kart_get`.

# Invariants

- **Same bits everywhere.** Every non-trivial function comes from `libm`, steps are fixed, and controls are sanitised: the native and the WebAssembly builds give identical results. [The golden file](/oracles/physics-golden.md) proves it on every change.
- Hostile controls (not a number, infinite, out of range) cannot produce a state that is not finite.
- A kart never ends a step beyond the walls.

# Knowledge

- Algorithms: [arcade kart physics](/algorithms/arcade-kart-physics.md), [exact client prediction](/algorithms/exact-client-prediction.md).
- Oracle: [physics golden file](/oracles/physics-golden.md).

# Reuse

The crate has no dependency but `libm` and no engine type: it can be used as is in another Rust project or through its WebAssembly interface from any language. Permissive licence.

# Limits

Flat ground only: no jumps, slopes or surfaces other than road and grass. Kart-to-kart contact is a simple separation, not a collision with momentum.
