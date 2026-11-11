---
type: Algorithm
title: Capsule contacts with impulses
description: "Two bodies in the plane, each a capsule with a mass and a moment of inertia, touch — they are pushed apart and exchange one impulse at the contact point, with restitution and friction, so that a knock pushes, slows and spins each by where it lands."
resource: https://github.com/smichea/osage/blob/main/worlds/kart/physics/src/lib.rs
licence: MIT OR Apache-2.0
status: draft
tags: [physics, collision, kart, determinism]
sources:
  - id: ericson
    resource: "Christer Ericson, Real-Time Collision Detection, Morgan Kaufmann, 2005, §5.1.9"
    title: "Closest points of two segments"
  - id: hecker
    resource: https://www.chrishecker.com/Rigid_Body_Dynamics
    title: "Chris Hecker, Rigid Body Dynamics, Game Developer Magazine, 1996–1997"
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-12T00:30:00Z" }
---

# Problem

Karts that touch must react as bodies do — the one behind slows and the one ahead is pushed on, a knock on a corner spins a kart, a glancing blow on a barrier keeps most of the speed — while the simulation stays a handful of floating-point operations, deterministic, and the driving model stays arcade.

# Idea

Each kart is a capsule: a short spine along its heading, thickened by a radius. Two capsules touch when the closest points of their spines are nearer than twice the radius[^ericson]. They are pushed apart along the line joining those points, and receive one equal and opposite impulse at the contact point, sized so that the closing speed there is reversed by a coefficient of restitution, plus a friction impulse along the contact, bounded by Coulomb's law[^hecker]. The impulse changes both the velocity and the rate of turn. A barrier is a body that does not move.

# Method

In the plane, a body turning at `ω` (positive as the heading grows) moves its point at `r` from the centre with velocity `v + ω·(r.z, −r.x)`; call `(r.z, −r.x)` the swept direction of `r`. Masses are equal (one unit); `I` is the moment of inertia per unit mass.

1. **Closest points** of spines `a + s·u` and `b + t·v`, `s, t` in `−h…h` with `u, v` unit: with `w = a − b`, `c = u·v`, `d = u·w`, `e = v·w`, take `s = clamp((c·e − d)/(1 − c²))` (0 when parallel), then `t = clamp(c·s + e)`, then `s = clamp(c·t − d)`.
2. **Overlap:** `2·radius − |p_b − p_a|`. None: nothing happens. Otherwise the normal `n` points from `a` to `b`; each body moves half the overlap along it.
3. **Contact:** the midpoint of the closest points; `r_a`, `r_b` from each centre. Relative velocity there `v_r = v_b(r_b) − v_a(r_a)`; if `v_r·n ≥ 0` they are already parting.
4. **Normal impulse:** `j = −(1 + e)·(v_r·n) / (2 + (k_a² + k_b²)/I)` with `k = swept(r)·n`.
5. **Friction:** along `t = (−n.z, n.x)`, `j_t = −(v_r·t) / (2 + (k_a'² + k_b'²)/I)` with `k' = swept(r)·t`, clamped to `±μ·j`.
6. **Apply:** `J = j·n + j_t·t`; `v_b += J`, `ω_b += swept(r_b)·J / I`, and the opposite to `a`.

Against a barrier, the denominator's `2` is `1` (one body moves) and only the kart's terms remain. In the kart physics the velocity is then written back as the kart's speed and slide; the spin turns the kart while its velocity stays, and the tyres scrub the slide and the spin off.

# Invariants

- Momentum is kept by a contact between two karts; no contact makes energy (restitution below one, friction bounded).
- Two touching outlines are apart after the contact.
- Every operation is an add, a multiply, a divide, a compare or a `libm` square root: the same bits on every platform.

# Cost

A dozen dot products and one square root per pair that touches; a few multiplications per pair that does not. Pairs are all tested: fine for a grid of tens of karts.

# Failure modes

- One impulse per pair per step, in a fixed order: three karts in a pile settle over a few steps, not in one.
- A kart fast enough to cross another's outline in one step (over 70 m/s at 60 Hz) passes through it.
- Contacts are in the plane: no kart rides over another, as wheels touching can make real karts do.

# Alternatives

A physics engine (rigid bodies, a solver with iterations) gives piles and stacking, at the cost of determinism across platforms and of the arcade driving model. Separating without momentum (what the kart physics did before D-045) is cheaper and feels like karts made of soap.

# Oracle

The unit tests of [osage-kart-physics](/modules/kart-physics.md) (momentum kept, a rear-end knock passes the speed on, a knock on the rear spins the kart, side by side karts part, a glancing blow on a barrier keeps the speed) and its [golden files](/oracles/physics-golden.md), which record a script of contacts bit for bit.

# Reuse

Any top-down game with vehicles, boats or bodies that are longer than wide: the capsule's spine and radius are the only shape parameters, `e`, `μ` and `I` the only physical ones.

[^ericson]: Closest points of two segments
[^hecker]: Chris Hecker, Rigid Body Dynamics, Game Developer Magazine, 1996–1997
