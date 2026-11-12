---
type: Module
title: osage-kart-roster
description: "The kart game's drivers (eight, light, medium or heavy) and kart bodies (four), each with the build it gives — top speed, acceleration, handling, grip and weight as shares of the standard kart's — balanced so that every pair laps within a narrow band."
resource: https://github.com/smichea/osage/tree/main/worlds/kart/components/roster
licence: MIT OR Apache-2.0
status: draft
tags: [kart, roster, characters, balance, component]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-12T16:40:00Z" }
---

# Purpose

Who drives a kart and in what (D-055). A component of the kart game (D-035): the world lines karts up with a driver and a body and publishes the roster to clients and agents; the physics only sees the build.

# Concepts

- **Build:** `osage_kart_physics::Build` — five shares of the standard kart's top speed, acceleration, handling, grip and weight; all 1.0 is the standard kart.
- **Class:** a driver's weight class, light, medium or heavy, and the build it gives: light karts start quicker and are pushed further, heavy ones are slower off the line, hold their line in a bump and gain a touch of top speed.
- **Driver:** an id, a name, a class, a palette colour and a line about them. Eight original characters: Nova, Pixel, Bolt, Comet, Zippy, Echo, Flash, Juno.
- **Body:** a kart body, an id, a name, a build and a line about it: Classic (the standard), Sprint (acceleration and handling, a little less top speed), Cruiser (top speed and grip, slow to wake), Drifter (handling and a looser rear).

# Architecture

`build(driver, body)` multiplies the class's build by the body's, share by share, and clamps it (`Build::sanitized`): top 0.85–1.15, acceleration 0.7–1.4, handling 0.75–1.3, grip 0.7–1.4, weight 0.7–1.4. The physics reads each share where it applies (top and boost speed, acceleration, turn rates, grip and drift grip); weight shares a bump's impulse by mass ([arcade kart physics](/algorithms/arcade-kart-physics.md)).

In the world (`osage-world-kart`, `roster.rs`): `Choices` keeps each session's pair; a `kart.choice/1` intent with unknown ids keeps what was there; bots take drivers and bodies in turn (`bot`). The roster is published once as `kart.roster/1` on the race board, each racer's driver, kart and build in `kart.racer/1`.

# Interfaces

`Class`, `Driver`, `Body`, `DRIVERS`, `BODIES`, `driver(id)`, `body(id)`, `build(driver, body)`. The protocol: `kart.choice/1` (`Choice`), `kart.roster/1` (`Roster`, `RosterDriver`, `RosterKart`), `Racer.driver/kart/build`.

# Invariants

- Every driver and body has a distinct id; the classic body with a medium driver is the standard kart. Tested.
- Balance: every one of the 32 pairs, driven two laps by the world's autopilot, is within 2 % of the mean (measured ±1.3 %; the world's `tests/bots.rs`).
- A choice on the grid changes the kart at once, during a race from the next race (the world's `tests/race.rs`).

# Knowledge

- The autopilot (racecraft) drives each build at its own limits: corner speeds from the build's handling, top speed from its top share — or a balance test would measure the autopilot, not the karts.
- Top speed dominated the first balance (−3 % to +4 %): top-speed shares are kept within 1 % and the differences carried by acceleration, handling and grip, which a player feels more and which trade against one another on a lap.

# Reuse

Any game with characters and vehicles whose differences are a few multipliers: the product-and-clamp rule and the band test need nothing of karts but a lap.

# Limits

The bodies differ in their build and colour only; their shapes are one model for now. Balance is measured on one track with the autopilot; a human feels acceleration and handling more than it does, and items (D-052) are drawn by place, not by build.
