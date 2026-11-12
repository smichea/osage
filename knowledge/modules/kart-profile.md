---
type: Module
title: osage-kart-profile
description: "A kart player's profile kept on their own computer — the driver, kart and paint raced with last, points won in cups, cups raced, won and on the podium — and the special paints those points unlock, cosmetic only."
resource: https://github.com/smichea/osage/tree/main/worlds/kart/components/profile
licence: MIT OR Apache-2.0
status: draft
tags: [kart, profile, progress, unlocks, persistence, component]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-13T01:00:00Z" }
---

# Purpose

Something to come back for that is the player's own (design/04 G4.6, D-064): the kart they like, kept from one evening to the next, and paints won in cups. A component of the kart game (D-035), used by the native client.

# Concepts

- **Paint:** an index into the palette: 0–7 the team colours, everyone's; 8–11 the special paints — silver, pearl, midnight, gold — unlocked at 20, 50, 90 and 140 points. A look, never a build.
- **Profile:** the driver, kart and paint chosen last (none: the driver's own colour), points won in cups all told, cups raced to the end, won, and finished in the first three.

# Architecture

`Profile::count_cup(points, place)` adds a finished cup and says which paints it unlocked; `has`, `paints`, `next` (the next paint to win and its points) and `next_paint` (round the paints the player has, the driver's colour first) answer the menus. The file is text: `kart-profile/1`, then `key = value` lines (`driver`, `kart`, `paint` — a number or `driver` — `points`, `cups`, `wins`, `podiums`); a line that does not read is skipped, another format reads as a new profile, and a paint not won is dropped on reading.

In the native client ([osage-kart-native](/modules/kart-native.md), `play.rs`, `Kept`): the profile is read at the start from `osage/kart-profile.txt`; once a connection its choice goes to the world as `kart.choice/1` with `paint`; a change of driver or kart is kept; P (a pad's West) paints the kart with the next paint; when a cup the player is in ends (its race number past its tracks), the player's points and place in its standings are counted once and the profile saved; the pick panel shows the paint, the points, the cups and the next paint to win, with NEW PAINT when a cup unlocked one. In the world (`roster.rs`, `Choices`): `Choice.paint` 0 gives back the driver's colour, 1–12 a paint, anything else changes nothing; the racer's colour is its paint, kept through a change of driver and into the next race.

# Interfaces

`Profile::{count_cup, has, paints, next, next_paint, to_text, from_text}`, `Unlocked`, `paint_name`, `PAINTS`, `TEAM`, `SPECIAL`, `HEADER`. Protocol: `Choice.paint` (optional), `Racer.color`.

# Invariants

- The team colours are everyone's; points unlock the special paints in order; a cup counts its points, a win, a podium. Tested.
- P goes round what the player has and back to the driver's colour. Tested.
- What is written reads back the same; damage is skipped; a paint not won is not kept. Tested.
- In the world: a paint colours the kart, stays through a change of driver and into the race; 0 gives back the driver's colour; a paint there is not changes nothing (the world's `tests/race.rs`).

# Knowledge

- Trust by default (D-038): the world does not check a paint was won; a profile edited by hand wears gold and harms nobody.
- Points are the cup's ([osage-kart-cup](/modules/kart-cup.md)): bots score as players do, so a cup won against the bots counts.

# Reuse

Any game with cosmetic unlocks earned by play and kept on the player's machine.

# Limits

The profile stays on one computer; the browser client keeps none. Only paints are unlocked: parts wait for models that have them.
