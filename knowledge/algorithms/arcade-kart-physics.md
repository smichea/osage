---
type: Algorithm
title: Arcade kart physics
description: A small kart model with drifting and two mini-turbo stages, made of a forward speed, a sideways slide and a heading.
resource: https://github.com/smichea/osage/blob/main/worlds/kart/physics/src/lib.rs
licence: MIT OR Apache-2.0
status: draft
tags: [kart, physics, drift]
sources:
  - id: own
    resource: "osage's own design and measurements, 2026-11-09 and 2026-11-10"
    title: osage design
generated: { by: "claude-code/claude-fable-5-2", at: "2026-11-10T06:45:00Z" }
---

# Problem

A kart that is fun to drive on a keyboard, a pad or a touch screen, cheap enough to simulate for every player on the server, and deterministic.

# Idea

Not a car simulation. Three numbers describe the motion — speed along the kart, slide across it, heading — and a drift is a state with its own turning rate and grip, charged by time. Written without a public reference; the constants were tuned by playing[^own].

# Method

Each step of `dt = 1/60 s`, with `throttle` and `steer` in −1…1:

1. **Top speed:** 28 m/s while boosting, 22 on the road, 9 off it.
2. **Along:** throttle accelerates at 16 m/s² up to the top speed; braking decelerates at 32, then reverses up to 7 m/s; with no throttle the kart loses 4 m/s². A boost adds 40 m/s² up to 28 m/s. Above the top speed, the excess decays by `3·dt` per step.
3. **Drift state:**
   - starts when the button is held, `|steer| > 0.2` and speed is above 10 m/s; its direction is the sign of `steer`;
   - ends when the button is released or speed falls under 7 m/s. On release, a charge of 0.7 s gives a 0.6 s boost, of 1.5 s a 1.2 s boost.
4. **Turning:** scaled by `min(1, |speed|/8)` and by the sign of the speed.
   - Not drifting: `heading −= steer · 2.0 · scale · dt`; the slide is pulled to zero with grip 10.
   - Drifting: `tightness = 1 + 0.45 · steer · drift`; `heading −= drift · tightness · 2.7 · scale · dt`; the charge grows by `dt · tightness`; the slide is pulled towards `−drift · speed · 0.12` with grip 3.
5. **Move** by `speed` along the heading and `slide` across it.
6. **Walls:** if the kart is beyond the verge, put it back on the wall line, multiply its speed by `|cos(angle to the road)|` kept within 0.2…0.95, turn it 30 % towards the road, and end any drift.

Two karts closer than 1.8 m are pushed apart by half the overlap each and lose 3 % of their speed.

# Invariants

Every operation is a floating-point add, multiply, compare, or a `libm` function: the result is the same on every platform. Controls are sanitised first.

# Cost

A few dozen floating-point operations and one search of the nearest track segment per step.

# Failure modes

Steering into a drift tightens it, against it widens it: without this, a drift is a fixed arc the driver cannot adjust. A drift must turn tighter than plain steering, or there is no reason to drift.

# Alternatives

A tyre-force model gives realism the game does not want and determinism problems it cannot afford.

# Oracle

[Physics golden file](/oracles/physics-golden.md), and the unit tests of the crate.

# Reuse

The whole model is above; the constants are a starting point for any arcade racer.

[^own]: osage design
