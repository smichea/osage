---
type: Module
title: osage-kart-physics
description: The kart world's arcade physics, one deterministic Rust crate run natively by the server and as WebAssembly by clients.
resource: https://github.com/smichea/osage/tree/main/worlds/kart/physics
licence: MIT OR Apache-2.0
status: draft
tags: [kart, physics, determinism, webassembly]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-12T12:30:00Z" }
---

# Purpose

One implementation of how a kart moves, shared by everything that must agree on it: the world server, the browser client that predicts its own kart, the native client, and local simulations such as the bench.

# Concepts

- **Kart:** position `x, z`, `heading`, forward `speed`, sideways `slide`, `drift` direction, drift `charge`, `boost` time left, `rocket` time left (a paddock object: faster than a boost, 34 m/s, D-048), the `spin` a knock gave it, and (D-050) its `height` and `climb` (over a crest and in the air), a `trick` done in the air, a `stall` after a start too early, its slipstream `draft` and whether the drift button was `held`. For contacts, an outline: a capsule 1.9 m by 1.3 m, with a mass and a moment of inertia.
- **Controls:** `throttle` and `steer` in −1…1, and the `drift` button.
- **Track:** a closed centre line, the road's half width, the verge beyond it, and crests across it (`Crest`: where, how far up, how far down, how high, D-051); kerbs (`KERB`, 1 m) beyond the road's edge, grass beyond them, and the barriers' face `BARRIER` (0.9 m) beyond the verge.
- **Ground:** what each wheel runs on — road, kerb or grass.
- **Step:** one sixtieth of a second. Nothing happens between steps.

# Architecture

A pure function `step(kart, controls, track)` and a `Track` that answers three questions: where a point is relative to the centre line (`locate`), where the centre line is at a distance from the start (`at`), what the ground is at a distance from it (`ground`), and how high the road is at a distance from the start (`lift`). The rules that involve several karts or the race are separate functions the world calls: `slipstream` and `launch`. Driving is arcade ([arcade kart physics](/algorithms/arcade-kart-physics.md)); contacts — between karts in `bump`, with the barriers at the end of `step`, with a loose body (a thrown tyre, a cone: its weight, size, velocity and give) in `struck` (D-079) — are impulses ([capsule contacts](/algorithms/capsule-contacts.md), D-045), shared by mass between karts. A kart carries its `Build` (D-055): five shares of the standard kart's top speed, acceleration, handling, grip and weight, sanitised at each step; the standard build is the old kart bit for bit, and its drivers and bodies are [osage-kart-roster](/modules/kart-roster.md)'s. No state outside its arguments, no clock, no random numbers. A small WebAssembly interface exchanges numbers only, with no shared memory.

# Interfaces

`step`, `bump` (two karts collide), `struck` (a loose body meets a kart: the body's change of velocity and where it is put out of the kart), `slipstream`, `launch`, `turbo_level`, `wheels` (the ground under each wheel), `Kart::velocity`, `Track::new`, `Track::with_crests`, `Track::with_heights`, `Track::heights`, `Track::hill` (hills, D-077; `HILL_TOP`, `HILL_PULL`), `Track::with_wet` and `Track.wet` (a wet road grips and brakes less, D-068; `WET_GRIP`, `WET_BRAKING`), `Crest::profile`, `Track::locate`, `Track::at`, `Track::ground`, `Track::lift`, `Controls::sanitized`, `Build` (`STANDARD`, `sanitized`), `wrap`, and the constants `KART_REACH`, `KART_RADIUS`, `KERB`, `BARRIER`, `TURBO_CHARGES`, `TURBO_BOOSTS`, `GRAVITY`, `TRICK_BOOST`, `DRAFT_CHARGE`, `LAUNCH_WINDOW`, `LAUNCH_EARLY`; in WebAssembly `track_clear`, `track_point`, `track_crest`, `track_close`, `track_lift`, `track_along` (how far along the centre line a point is, D-084), `track_wet`, `track_height` (after each point, over hills), `kart_set`, `kart_step`, `kart_get` (a kart's fields by number, so that a new field changes no signature: seventeen of its state, `slick` (oily tyres, D-072) the last, then the build's five, 17 to 21).

# Invariants

- **Same bits everywhere.** Every non-trivial function comes from `libm`, steps are fixed, and controls are sanitised: the native and the WebAssembly builds give identical results. [The golden file](/oracles/physics-golden.md) proves it on every change.
- Hostile controls (not a number, infinite, out of range) cannot produce a state that is not finite.
- A kart's outline never ends a step beyond the barriers' face.
- A contact between karts keeps their momentum and makes no energy; a heavier kart is moved less. Tested.
- A build sets top speed, acceleration, handling and grip where they apply; the standard build changes nothing (the golden file). Tested.

# Knowledge

- Algorithms: [arcade kart physics](/algorithms/arcade-kart-physics.md), [capsule contacts](/algorithms/capsule-contacts.md), [exact client prediction](/algorithms/exact-client-prediction.md).
- Oracle: [physics golden file](/oracles/physics-golden.md).

# Reuse

The crate has no dependency but `libm` and no engine type: it can be used as is in another Rust project or through its WebAssembly interface from any language. Permissive licence.

# Limits

Relief is a height along the centre line (hills, D-077) and crests: no banking, no kerbs that lift a wheel, and the road is level across (no roll). Contacts are in the plane, so no kart rides over another, even one in the air. A client predicting its kart steps the others on with their last controls (D-076): a driver who turns in during the lead is found a state late. A loose body is a point mass, without spin of its own.
