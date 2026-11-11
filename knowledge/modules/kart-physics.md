---
type: Module
title: osage-kart-physics
description: The kart world's arcade physics, one deterministic Rust crate run natively by the server and as WebAssembly by clients.
resource: https://github.com/smichea/osage/tree/main/worlds/kart/physics
licence: MIT OR Apache-2.0
status: draft
tags: [kart, physics, determinism, webassembly]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-12T00:30:00Z" }
---

# Purpose

One implementation of how a kart moves, shared by everything that must agree on it: the world server, the browser client that predicts its own kart, the native client, and local simulations such as the bench.

# Concepts

- **Kart:** position `x, z`, `heading`, forward `speed`, sideways `slide`, `drift` direction, drift `charge`, `boost` time left, and the `spin` a knock gave it. For contacts, an outline: a capsule 1.9 m by 1.3 m, with a mass and a moment of inertia.
- **Controls:** `throttle` and `steer` in −1…1, and the `drift` button.
- **Track:** a closed centre line, the road's half width, and the verge beyond it; kerbs (`KERB`, 1 m) beyond the road's edge, grass beyond them, and the barriers' face `BARRIER` (0.9 m) beyond the verge.
- **Ground:** what each wheel runs on — road, kerb or grass.
- **Step:** one sixtieth of a second. Nothing happens between steps.

# Architecture

A pure function `step(kart, controls, track)` and a `Track` that answers three questions: where a point is relative to the centre line (`locate`), where the centre line is at a distance from the start (`at`), and what the ground is at a distance from it (`ground`). Driving is arcade ([arcade kart physics](/algorithms/arcade-kart-physics.md)); contacts — between karts in `bump`, with the barriers at the end of `step` — are impulses ([capsule contacts](/algorithms/capsule-contacts.md), D-045). No state outside its arguments, no clock, no random numbers. A small WebAssembly interface exchanges numbers only, with no shared memory.

# Interfaces

`step`, `bump` (two karts collide), `wheels` (the ground under each wheel), `Kart::velocity`, `Track::new`, `Track::locate`, `Track::at`, `Track::ground`, `Controls::sanitized`, `wrap`, and the constants `KART_REACH`, `KART_RADIUS`, `KERB`, `BARRIER`; in WebAssembly `track_clear`, `track_point`, `track_close`, `kart_step`, `kart_get`.

# Invariants

- **Same bits everywhere.** Every non-trivial function comes from `libm`, steps are fixed, and controls are sanitised: the native and the WebAssembly builds give identical results. [The golden file](/oracles/physics-golden.md) proves it on every change.
- Hostile controls (not a number, infinite, out of range) cannot produce a state that is not finite.
- A kart's outline never ends a step beyond the barriers' face.
- A contact between karts keeps their momentum and makes no energy.

# Knowledge

- Algorithms: [arcade kart physics](/algorithms/arcade-kart-physics.md), [capsule contacts](/algorithms/capsule-contacts.md), [exact client prediction](/algorithms/exact-client-prediction.md).
- Oracle: [physics golden file](/oracles/physics-golden.md).

# Reuse

The crate has no dependency but `libm` and no engine type: it can be used as is in another Rust project or through its WebAssembly interface from any language. Permissive licence.

# Limits

Flat ground only: no jumps, slopes, or kerbs that lift a wheel; contacts are in the plane, so no kart rides over another. Contacts between karts run on the server: a client predicting its kart sees a knock when the server's state arrives.
