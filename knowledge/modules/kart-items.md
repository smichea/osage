---
type: Module
title: osage-kart-items
description: "Paddock objects for the kart world — racks of crates that hand out a nitro canister, a tyre to throw, a cone or an oil patch to leave behind, drawn to help the back of the field — and when an AI driver uses them; deterministic, run by the world."
resource: https://github.com/smichea/osage/tree/main/worlds/kart/components/items
licence: MIT OR Apache-2.0
status: draft
tags: [kart, items, gameplay, determinism, component]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-12T06:00:00Z" }
---

# Purpose

Something to pick up and use during a race, in the spirit of a kart paddock rather than of any other game: a canister of nitro, a spare tyre, a traffic cone, a can of oil. Items mix the field — the back gets help, the front has to defend — and give AI drivers one more way to race. A component of the kart game (D-035, D-046), run by the world server, which has the authority; clients only draw what it sends.

# Concepts

- **Rack:** four crates across the road at 25 %, 55 % and 85 % of the lap. A kart whose outline comes within 0.5 m of a crate breaks it; the crate is back 4 s later.
- **Item:** what a kart holds, one at a time. A kart with its hands full still breaks crates, for nothing.
- **Thing:** what a used item leaves on the track: a rolling tyre (6 s), a cone (20 s), an oil patch of 1.5 m radius (15 s). At most 32 at once; the oldest goes first.
- **Draw:** weighted by place — nitro `1 + 4·back`, tyre `3 − back`, cone `3 − 2·back`, oil `2`, where `back` is 0 for the leader and 1 for the last — from a hash of the race's seed and the number of draws so far.

# Architecture

`Paddock::step(karts, uses, places, track)` runs once per physics step, after driving and contacts:

1. Broken crates count down.
2. Each kart breaks the crates it touches and draws an item if its hands are free.
3. A use button that went down (pressed now, not the step before) uses what the kart holds:
   - **nitro:** 1.2 s of boost, as a long drift gives;
   - **tyre:** thrown from the kart's nose at its forward speed plus 14 m/s; it slows at 2 m/s², and off a barrier (the physics' wall face) it bounces with 60 % of its speed across it;
   - **cone / oil:** left just behind the kart.
4. Things move and meet karts (the kart's capsule outline against the thing's radius):
   - a **tyre** spares its thrower for 0.5 s; the kart it hits keeps 45 % of its speed, loses its drift and boost, and spins at 5 rad/s the way the blow turned it; the tyre goes;
   - a **cone** slows the kart to 85 % and knocks it away (an impulse through `Kart::take`); the cone goes;
   - **oil** acts once as a kart drives in (a bit per kart, so its owner, in it from the start, is spared until it leaves and comes back): 90 % of its speed and a spin of 3.5 rad/s the way it was sliding; the oil stays.

It returns events (picked, used, hit, bounced) for sounds and pictures.

`wants(item, me, field, kart, track)` is when an AI driver uses what it holds: a nitro above 60 % of top speed with 30 m of straight ahead; a tyre with 25 m of straight and a kart 4 to 30 m ahead within 1.5 m of its line; a cone or oil with a kart 2 to 12 m behind within 2 m.

# Interfaces

`Item`, `Kind`, `Crate`, `Thing`, `Event`, `Paddock::{new, step, crates, things, held, hold}`, `wants`, and the constants of the design (`NITRO`, `THROW`, `ROLLING`, `TYRE_BOUNCE`, radii and lives, `RACKS`, `RACK`, `CRATE_BACK`, `MOST`).

The world server (`osage-world-kart`) carries it in its protocol: `Step.use_item` (the button, for humans), `Autopilot.use_items` (agents let the world's rule decide), `Racer.held`, and `kart.paddock/1` (crates standing or not; things with their velocity and life) on the race board, sent when it changes.

# Invariants

- Deterministic: no clock, no random numbers but a hash of the seed, `libm`.
- A crate gives an item to a kart with free hands, nothing to one with full hands, and comes back. Tested.
- The back of the field draws nitros more than twice as often as the leader; the same seed draws the same items. Tested.
- One press, one use: holding the button does not use the next item. Tested.
- A tyre rolls ahead and spins the kart it meets, not its thrower; it bounces off a barrier and stays on the track. Tested.
- A cone slows and knocks; oil spins a kart once as it drives in and spares its owner; things go after their time. Tested.
- On the world's circuit, bots break crates, hold items and use them (the world's `tests/race.rs`).

# Knowledge

- Acts on karts through [osage-kart-physics](/modules/kart-physics.md) — `Kart::take` for knocks ([capsule contacts](/algorithms/capsule-contacts.md)), `boost`, `spin` — and finds the barriers where the physics puts them.
- Its AI rule complements [osage-kart-racecraft](/modules/kart-racecraft.md), which drives; the world asks `wants` for its bots and for agents who let it.
- Drawn by [osage-kart-native](/modules/kart-native.md): crates, rolling tyres, cones and oil, and the item in hand.

# Reuse

Any track game with pickups: the racks, the place-weighted draw, edge-triggered use, and things that roll and bounce against a track's walls take a centre line and a kart outline.

# Limits

A human's own kart is predicted without items (a nitro, a hit) and corrected when the world's state comes back, as after a contact. Things do not hit one another; tyres do not jump kerbs; a thing on the grass behaves as on the road. No sound of its own yet: a hit is heard as the knock of the spin it gives. The bench does not race with items (its golden race stays `kart-lap/3`).
