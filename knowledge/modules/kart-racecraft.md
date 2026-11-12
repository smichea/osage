---
type: Module
title: osage-kart-racecraft
description: How the kart world's AI drivers race — a temperament each, braking for bends, a racing line, overtaking or waiting, defending, steering round things on the road, mistakes and grudges after a knock — deterministic, for the server's bots and the bench.
resource: https://github.com/smichea/osage/tree/main/worlds/kart/components/racecraft
licence: MIT OR Apache-2.0
status: draft
tags: [kart, ai, drivers, determinism, component]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-12T12:30:00Z" }
---

# Purpose

Opponents that race as people do, each in their own way: one brakes late and dives for every gap, another waits for a wide one, a third covers the inside of every bend; some make mistakes, and a knock makes a hothead race harder and a calm driver lift. A component of the kart game (D-035), used by the world server for its bots and by the native client's bench.

# Concepts

- **Temper:** pace (share of top speed on straights), nerve (how near the limit it takes bends), aggression, defence, consistency, heat (after a knock: lift or race harder), drifts or not. Presets — `STEADY`, `HOTHEAD`, `CAUTIOUS`, `ROOKIE`, `VETERAN` — and `Temper::of(seed)`, drawn within reason.
- **Mind:** a driver's state from step to step: where it steers across the road, its plan (race, pass on a side, wait, defend a side) and how long it has held it, a mistake under way, how shaken it is.
- **Field:** every kart located once on the track per step (`field`), seen by every driver.

# Architecture

`drive_among(kart, me, field, hazards, temper, mind, track) -> Controls`, once per physics step (`drive` is the same on an empty road):

1. **Bends:** the track's heading over 12 m gives each bend's radius, sampled every 5 m for 70 m. A kart turns at most at `TURN_RATE`, so a bend is taken at `margin · TURN_RATE · handling · (radius + 3 m)` at best (the kart's own build, D-055; its top speed likewise) (the margin from nerve, 0.62 to 0.87); braking at 18 m/s² from each bend back gives the speed allowed now.
2. **Line:** the driver's own line on straights (from its seed), the outside 12 to 45 m before a bend, the inside at the apex. It moves across the road at 2.5 m/s at most.
3. **Traffic:** a kart ahead within `6 + 0.7·speed` m, near its line, and not faster: it passes on the side with room — the aggressive on the inside of the coming bend — if the gap is at least `1.9 − 0.5·aggression` m (the cautious want a metre more); otherwise it follows, a gap behind. With nobody ahead and a faster kart within 12 m behind before a bend, a defender covers the inside. Alongside another, a driver with aggression under 0.7 gives room; the others hold their line, and contacts are physical ([capsule contacts](/algorithms/capsule-contacts.md)). A plan holds 1.2 s, except to start passing: nobody weaves.
   **Things on the road** (`Hazard`: a cone, oil, a tyre, where the items put them): the nearest within `8 + 1.2·speed` m whose edge comes within half a metre of the kart's on its line is passed on the side nearer the kart that fits on the road, moving across at twice the usual rate.
4. **Mistakes:** each tight bend (radius under 45 m) is judged once, 10 to 40 m before it: with probability `(1 − consistency) · 0.25`, more when shaken, the driver brakes too late (more speed and braking it does not have, and it runs wide) or hesitates (lifts to three quarters of its pace), for 2.5 s. The roll is a hash of the driver's seed and the bend's count.
5. **Grudges:** a sudden spin (a knock) shakes the driver, fading over 5 s: aggression moves by `shaken · (heat − 0.5)`, and a calm driver's pace drops by up to 12 %.
6. **Controls:** it aims at the line `6 + 0.55·speed` m ahead (nearer in a tight bend, never across it), steers towards it, accelerates up to its pace, brakes above the speed allowed, and drifts through tight bends if it drifts.

`follow(kart, pace, line, drift, track)` is the plain autopilot for AI agents (`kart.autopilot/1`): their pace, line and drift, with braking for bends, and no traffic.

# Interfaces

`Temper::{STEADY, HOTHEAD, CAUTIOUS, ROOKIE, VETERAN, of}`, `Mind::{new, plan, mistake, mistakes, shaken}`, `Plan`, `Mistake`, `Rival`, `Hazard`, `field`, `drive`, `drive_among`, `follow`.

# Invariants

- Deterministic: the same race twice is the same, bit for bit (no clock, no random numbers but a hash, `libm`). Tested.
- A driver who never errs laps a circuit of hairpins without a wheel on the grass; hairpins are taken slower than straights. Tested.
- The aggressive take a narrow gap the cautious wait behind; on a wide road both pass; a fast driver gets past a slow one. Tested.
- A driver steers round oil on its line, which it drives over when the oil is not there. Tested.
- A defender covers the inside of the next bend from a faster kart close behind. Tested.
- Mistakes come at the rate consistency sets, none at 1. Tested.
- A knock shakes a driver; a shaken calm driver lifts. Tested.
- On the world's circuit, eight drivers of different tempers all lap, with no wheel on the grass (the world's `tests/bots.rs`).

# Knowledge

- Drives [osage-kart-physics](/modules/kart-physics.md) ([arcade kart physics](/algorithms/arcade-kart-physics.md)); used by the world server for its bots (presets for five, drawn temperaments for three) and by [osage-kart-native](/modules/kart-native.md)'s bench ([timedemo bench](/algorithms/timedemo-bench.md), `kart-lap/3`).
- Oracle: the crate's tests and the world's.

# Reuse

Any racing game on a track given as a centre line: the bend speeds follow from the vehicle's turning rate, the temperaments from a seed.

In the air over a crest, the bold (nerve above 0.4) do a trick: a press of the drift button in the air (D-050).

# Limits

No memory of rivals (who knocked it), no team play, no pit strategy. Drivers do not see the barriers, only the road's width. The attitude is in how they drive; their bodies do not show it yet (no gesture after a knock).
