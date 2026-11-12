---
type: Module
title: osage-kart-items
description: "Paddock objects for the kart world — racks of crates that hand out ten items (nitro, rocket, tyre, stack of tyres, cone, oil, smoke, drone, bumper, pit-crew push), drawn by place and gap to the leader, some held behind the kart as a shield — and how AI drivers use them; deterministic, run by the world."
resource: https://github.com/smichea/osage/tree/main/worlds/kart/components/items
licence: MIT OR Apache-2.0
status: draft
tags: [kart, items, gameplay, determinism, component]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-12T06:00:00Z" }
---

# Purpose

Something to pick up and use during a race, in the spirit of a kart paddock rather than of any other game: a canister of nitro, a rocket, spare tyres, a traffic cone, a can of oil, a smoke canister, a drone, an air bag, a push from the pit crew. Items keep a race open (D-052): the back of the field and those far behind get strong help, the leader gets things to defend with. A component of the kart game (D-035, D-046), run by the world server, which has the authority; clients only draw what it sends.

# Concepts

- **Rack:** five crates across the road at 25 %, 55 % and 85 % of the lap. A kart whose outline comes within 0.5 m of a crate breaks it; the crate is back 1.5 s later (4 s let a pack in single file share one crate: 13 items in a race of nine, 2026-11-12).
- **Item:** what a kart holds, one kind at a time (`count`: three tyres for a stack). A kart with its hands full still breaks crates, for nothing.
- **Trailing:** a tyre, a stack, a cone or oil goes **behind** the kart as the use button goes down and is used as it comes up. Held behind, it is a shield: a tyre that meets it from behind stops there, and the shield goes (one tyre of a stack). A kart spun by a tyre drops what it held behind.
- **Guard:** seconds nothing on the track touches a kart, after a pit-crew push: what it drives into goes, a drone does nothing.
- **Thing:** what a used item leaves on the track: a rolling tyre (6 s), a cone (20 s), an oil patch of 1.5 m radius (15 s), a drone in the air (10 s at most). At most 32 at once; the oldest goes first.
- **Standing:** a kart's place (0 leads) and how far behind the leader it is, metres of race.
- **Draw:** from a hash of the race's seed and the number of draws so far, weighted by `back` — the larger of the kart's place as a share of the field and its gap to the leader as a share of 120 m, 0 for the leader: nitro `1 + 4b`, tyre `3 − b`, cone `3 − 2b`, oil `2 − b`, rocket `3b²`, smoke `1.5`, stack `6b(1 − b)` (the middle of the field), drone `2.5b²`, bumper `1.5`, push `3b³` (the very back). The leader never draws a rocket, a drone or a push.

# Architecture

`Paddock::step(karts, uses, standings, track)` runs once per physics step, after driving and contacts:

1. Broken crates and guards count down.
2. Each kart breaks the crates it touches and draws an item if its hands are free.
3. The use button: what trails goes behind on the way down and is used on the way up; anything else is used on the way down (pressed now, not the step before):
   - **nitro:** 2.5 s of boost (a long drift gives 1.6 s at most);
   - **smoke:** 6 s of smoke of a bright colour of its own (a hue from the race's hash) poured behind the kart, a screen; `Paddock::smoke(kart)` gives its colour, published as `Racer.smoke`;
   - **rocket:** 3.5 s of the physics' `rocket`: up to 34 m/s against a boost's 28; a tyre's hit puts it out;
   - **tyre** (and each of a stack's three): thrown from the kart's nose at its forward speed plus 14 m/s; it slows at 2 m/s², and off a barrier (the physics' wall face) it bounces with 60 % of its speed across it;
   - **cone / oil:** left just behind the kart;
   - **drone:** launched to the leader (the leader's own goes nowhere): it flies straight at 40 m/s, climbs to 4 m, over the others and the infield, comes down over its last 12 m and lands on the leader, who keeps 30 % of its speed, loses its drift and boosts and spins at 6 rad/s;
   - **bumper:** every other kart within 4.5 m takes an impulse away from the kart (7 m/s near, half at the edge), off its centre so that it turns, and loses its drift;
   - **push:** 2 s of boost and 2.5 s of guard.
4. Things move and meet karts (the kart's capsule outline against the thing's radius):
   - a **tyre** spares its thrower for 0.5 s; a kart holding something behind it, hit from behind, is shielded; otherwise the kart keeps 45 % of its speed, loses its drift and boost, and spins at 5 rad/s the way the blow turned it; the tyre goes;
   - a **cone** slows the kart to 85 % and knocks it away (an impulse through `Kart::take`), but spares the kart that left it for 6 s; the cone goes;
   - **oil** acts once as a kart drives in (a bit per kart): 90 % of its speed and a spin of 3.5 rad/s the way it was sliding; the oil stays. Its owner is spared for 6 s (`OWN_SPARE`): it does not slip as it pulls away or when it is knocked back into it, but can a lap later. (Sparing the owner only until it first left had it slip on its own oil when knocked back, 2026-11-12.)
   - a kart under **guard** is touched by nothing: a tyre or cone it meets goes, oil does nothing.

It returns events (picked, trailing, used, hit, blocked, bumped, bounced) for sounds and pictures.

`wants(item, me, field, standings, kart, track)` is when an AI driver uses what it holds: a nitro above 60 % of top speed with 30 m of straight ahead; a rocket above 50 % with 40 m; a push above 40 % with 30 m; smoke with a kart 3 to 20 m behind within 3 m; a tyre or a stack with 25 m of straight and a kart 4 to 30 m ahead within 1.5 m of its line; a cone or oil with a kart 2 to 12 m behind within 2 m; a bumper with a kart within 3.5 m along and 3 m across; a drone whenever it does not lead. `Paddock::button` turns that into a button for what trails: down (behind the kart) when a kart is 1 to 15 m behind on its line or when it wants to use it, kept down while held behind until it wants to, then up.

# Interfaces

`Item` (`ALL`, `trails`), `Kind`, `Standing`, `Crate`, `Thing`, `Event`, `Paddock::{new, step, button, crates, things, held, count, trailing, guard, smoke, hold}`, `wants`, `touches`, and the constants of the design (`NITRO`, `ROCKET`, `SMOKE`, `STACK`, `THROW`, `ROLLING`, `TYRE_BOUNCE`, `DRONE_*`, `BUMPER_REACH`, `BUMPER_KNOCK`, `PUSH`, `GUARD`, `FAR_BEHIND`, radii and lives, `RACKS`, `RACK`, `CRATE_BACK`, `MOST`).

The world server (`osage-world-kart`) carries it in its protocol: `Step.use_item` (the button, for humans), `Autopilot.use_items` (agents let the world's rule decide), `Racer.held`, `held_count`, `trailing` and `guard`, and `kart.paddock/1` (crates standing or not; things with their velocity, life and height) on the race board, sent when it changes. The world gives each kart's standing from its race: place, and the distance it has covered against the leader's.

# Invariants

- Deterministic: no clock, no random numbers but a hash of the seed, `libm`.
- A crate gives an item to a kart with free hands, nothing to one with full hands, and comes back. Tested.
- Over 4000 draws per place: the leader draws no rocket, drone or push; the mean strength of a draw grows with the place; third place 150 m behind draws as the last does; every item comes out somewhere; the same seed draws the same items. Tested.
- One press, one use: holding the button does not use the next item. Tested.
- What trails is held behind while the button is down and used as it comes up; a tyre held behind stops a tyre from behind and goes; without it the tyre spins the kart. Tested.
- A stack throws three tyres, one press each. A drone flies over the others to the leader, comes down on it and goes; the leader's own goes nowhere. A push boosts and nothing touches the kart meanwhile. A bumper knocks karts beside aside, not those further, not its own. Tested.
- A tyre rolls ahead and spins the kart it meets, not its thrower; it bounces off a barrier and stays on the track. Tested.
- A cone slows and knocks; oil spins a kart once as it drives in and spares its owner, even knocked back into it; things go after their time. Tested.
- An AI driver holds a tyre behind with a kart behind and throws at one ahead; it sends a drone when it does not lead, a bumper with a kart beside, a push on a straight. Tested.
- Clients draw things as far ahead in time as the player's kart is predicted, or a tyre just thrown shows behind its thrower and runs through it (kart-native).
- On the world's circuit, bots break crates, hold at least eight kinds, keep items behind, and leave or send at least three kinds of things, drones flying among them (the world's `tests/race.rs`).

# Knowledge

- Acts on karts through [osage-kart-physics](/modules/kart-physics.md) — `Kart::take` for knocks ([capsule contacts](/algorithms/capsule-contacts.md)), `boost`, `rocket`, `spin` — and finds the barriers where the physics puts them.
- Its AI rule complements [osage-kart-racecraft](/modules/kart-racecraft.md), which drives; the world asks `button` for its bots and for agents who let it.
- Drawn by [osage-kart-native](/modules/kart-native.md): crates, rolling tyres, cones, oil and drones; the item in hand, on the kart's back or behind it; a push's glow and a bumper's wave. Heard through [osage-kart-sound](/modules/kart-sound.md): a drone's rotors, the cues of a bumper and a push.

# Reuse

Any track game with pickups: the racks, the draw by place and gap, edge-triggered use and items held while a button is down, things that roll and bounce against a track's walls, and a homing flyer take a centre line and a kart outline.

# Limits

A human's own kart is predicted without items (a nitro, a push, a hit) and corrected when the world's state comes back, as after a contact. Things do not hit one another; tyres do not jump kerbs; a thing on the grass behaves as on the road; a drone flies through scenery. A shield stops only tyres from behind. The bench does not race with items (its golden race stays `kart-lap/3`). The browser client does not draw items yet.
