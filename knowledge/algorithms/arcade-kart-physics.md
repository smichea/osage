---
type: Algorithm
title: Arcade kart physics
description: A small kart model with drifting, three mini-turbo levels, crests and tricks, starts and slipstreams, made of a forward speed, a sideways slide, a heading and a height.
resource: https://github.com/smichea/osage/blob/main/worlds/kart/physics/src/lib.rs
licence: MIT OR Apache-2.0
status: draft
tags: [kart, physics, drift]
sources:
  - id: own
    resource: "osage's own design and measurements, 2026-11-09 and 2026-11-10"
    title: osage design
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-12T12:30:00Z" }
---

# Problem

A kart that is fun to drive on a keyboard, a pad or a touch screen, cheap enough to simulate for every player on the server, and deterministic.

# Idea

Not a car simulation. Three numbers describe the motion — speed along the kart, slide across it, heading — and a drift is a state with its own turning rate and grip, charged by time. A height and a speed upwards are added for crests (D-051). Skill is rewarded where the genre rewards it (D-050): longer drifts, tricks in the air, starts in time, driving in a wake. Written without a public reference; the constants were tuned by playing[^own].

# Method

Each step of `dt = 1/60 s`, with `throttle` and `steer` in −1…1:

0. **Buttons and the engine:** a press of the drift button is a change from the last step (`held`). A stalled engine (`stall` > 0, a start too early) gives no throttle until it runs out. The kart **flies** when it is higher than the road under it (over a crest, or 0).
1. **Ground and top speed:** each wheel (0.52 m ahead or behind the centre, 0.55 m to a side) is on the road, a kerb (up to 1 m beyond the road's edge) or grass. The top speed, 28 m/s while boosting and 22 otherwise, falls towards 9 in proportion to the wheels on grass. Grip is scaled by `1 − 0.35·(share on kerbs) − 0.5·(share on grass)`.
2. **Along:** throttle accelerates at 16 m/s² up to the top speed; braking decelerates at 32, then reverses up to 7 m/s; with no throttle the kart loses 4 m/s². A boost adds 40 m/s² up to 28 m/s. Above the top speed, the excess decays by `3·dt` per step.
3. **Drift state:**
   - starts when the button is held, `|steer| > 0.2` and speed is above 10 m/s; its direction is the sign of `steer`;
   - ends when the button is released or speed falls under 7 m/s. On release, a charge of 0.6, 1.2 or 2.0 s (levels 1, 2, 3: blue, orange, violet sparks) gives a boost of 0.5, 1.0 or 1.6 s;
   - cannot start in the air, and charges only on the ground.
4. **Turning:** scaled by `min(1, |speed|/8)` and by the sign of the speed.
   - Not drifting: `heading −= steer · 2.0 · scale · dt`; the slide is pulled to zero with grip 10.
   - Drifting: `tightness = 1 + 0.45 · steer · drift`; `heading −= drift · tightness · 2.7 · scale · dt`; the charge grows by `dt · tightness`; the slide is pulled towards `−drift · speed · 0.12` with grip 3.
   - Grass drags: `heading += (left wheels on grass − right ones) / 2 · 0.02 · speed · dt`.
   - Rain (D-068): on a wet road (`track.wet`, 0 to 1) every wheel's footing is multiplied by `1 − 0.4 · wet` — the slide and the spin scrub off more slowly — and braking by `1 − 0.3 · wet`; dry, both are 1, and the bits are those of a dry road.
   - Oil (D-072): while `slick` (seconds of oily tyres) lasts, every wheel's footing is × 0.12 and the steering × 0.3; it counts down.
5. **Spin** from a knock: the heading turns by `spin · dt` while the velocity on the ground stays, so speed becomes slide; the spin decays at `4 · grip scale` per second and stops under 0.001 rad/s.
6. **Move** by `speed` along the heading and `slide` across it.
7. **Barriers:** if the kart's outline (a capsule) passes the barriers' face, 0.9 m beyond the verge, push it back and give it an impulse at the corner that touches ([capsule contacts](/algorithms/capsule-contacts.md): restitution 0.2, friction 0.3). An impulse over 2.5 m/s ends a drift.
8. **Up and down:** a crest (D-051) lifts the whole width of the track, barriers and land too: half a cosine up over `rise` metres to `height`, half a cosine down over `fall`. Each step the kart falls freely from where it was, with the speed upwards it had (gravity 18 m/s²); if the road under it is higher than that, the road carries it, and how fast the road rose under it becomes its climb — whichever way it drives. Over the top the road curves away; where it curves faster than the kart falls (speed² × curvature > gravity) the kart leaves it, and lands further on. In the air the wheels neither drive nor brake, nothing grips or drags (grip × 0.1, no grass), steering turns a quarter as much, and a press of the drift button is a **trick**: landing after one gives a 0.7 s boost.

Two karts whose outlines overlap collide the same way, both moving (restitution 0.3, friction 0.4).

Two rules involve more than one kart or the race, and the world runs them:

- **Slipstream** (`slipstream`, after the contacts): a kart above 14 m/s, on the ground, with another (above 9.8 m/s) 1.5 to 10 m ahead along its heading and within 1.4 m across, charges one step; out of a wake it loses two. 1.2 s of charge is a 0.6 s boost.
- **Start** (`launch`, as the lights go): throttle pressed at most 0.4 s before and held is a 1 s boost; pressed more than 1.5 s before, the engine stalls for 0.8 s.

# Tuning

The values that make the feel, in one place: what to change, and the oracle that will see it. The bots' race time over two laps on the paddock circuit (`bots_lap_the_circuit_in_their_time_band`, 60.1 s when set) bounds what a change may do to the pace.

| Value | Constant | Now | Raises it… |
| --- | --- | --- | --- |
| Top speed | `MAX_SPEED` | 22 m/s | everything faster; bends harder |
| Acceleration, braking | `ACCELERATION`, `BRAKING` | 16, 32 m/s² | quicker out of bends, shorter braking |
| Grass | `OFF_ROAD_SPEED`, `GRASS_GRIP` | 9 m/s, 0.5 | less punishment for a wide line |
| Steering | `TURN_RATE`, `FULL_STEER_SPEED` | 2.0 rad/s, 8 m/s | twitchier at speed; more turn when slow |
| Drift | `DRIFT_TURN_RATE`, `DRIFT_GRIP`, `DRIFT_MIN_SPEED` | 2.7 rad/s, 3, 10 m/s | tighter drifts; less slide; drifts only when faster |
| Mini-turbo | `TURBO_CHARGES`, `TURBO_BOOSTS` | 0.6/1.2/2.0 s → 0.5/1.0/1.6 s | harder to charge; more reward |
| Boost | `BOOST_SPEED`, `BOOST_ACCELERATION` | 28 m/s, 40 m/s² | each boost worth more |
| Air | `GRAVITY`, `AIR_STEER`, `TRICK_BOOST` | 18 m/s², 0.25, 0.7 s | shorter jumps; more control in the air; more for a trick |
| Slipstream | `DRAFT_CHARGE`, `DRAFT_BOOST` | 1.2 s, 0.6 s | longer in the wake for less |
| Start | `LAUNCH_WINDOW`, `LAUNCH_EARLY` | 0.4 s, 1.5 s | an easier boost; a later stall |
| Barriers | `WALL_RESTITUTION`, `WALL_FRICTION` | 0.2, 0.3 | more bounce; more scrape |

Builds (D-055): a kart's top speed and boost speed, acceleration, turn rates (driving and drifting) and grip (driving and drifting) are the table's values times its build's shares; its weight is a mass in contacts, so that the impulse between two karts moves each by its inverse mass (yaw included) and momentum is kept. The shares are clamped narrowly: a build is a character, not another vehicle.

# Invariants

Every operation is a floating-point add, multiply, compare, or a `libm` function: the result is the same on every platform. Controls are sanitised first.

# Cost

A few dozen floating-point operations and one search of the nearest track segment per step.

# Failure modes

Steering into a drift tightens it, against it widens it: without this, a drift is a fixed arc the driver cannot adjust. A drift must turn tighter than plain steering, or there is no reason to drift.

# Alternatives

A tyre-force model gives realism the game does not want and determinism problems it cannot afford.

# Oracle

[Physics golden file](/oracles/physics-golden.md) (its script drives over two crests), the unit tests of the crate (each level of drift a longer boost; a crest smooth and closed; a crest that lifts a kart flat out and holds slower or reversing ones; a trick's boost and a held button's none; a wake's boost; a start in time and one too early), and the world's tests: starts judged against the lights, every flight in a race of bots landing on the road, the bots' time band.

# Reuse

The whole model is above; the constants are a starting point for any arcade racer.

[^own]: osage design
