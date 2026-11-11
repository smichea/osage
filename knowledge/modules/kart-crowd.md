---
type: Module
title: osage-kart-crowd
description: Spectators sitting in a grandstand, sculpted as whole figures in a few poses and shared, for a world to dress in its own clothes.
resource: https://github.com/smichea/osage/tree/main/worlds/kart/components/crowd
licence: MIT OR Apache-2.0
status: draft
tags: [kart, people, crowd, component]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-11T12:00:00Z" }
---

# Purpose

Fill a grandstand with people who read as people from the track: not a capsule under a ball, but a trunk, arms, legs, a head and hair in one surface, sitting as people sit. A component of the kart game (D-035), usable by any world with seats.

# Concepts

- **Pose:** how a spectator sits: upright with hands on the knees, leaning back with arms folded, or cheering with one arm raised.
- **Wear:** what a part of the surface is — shirt, trousers, skin, hair, shoes — for the world to paint.

# Architecture

Each pose is a [blended solid](/algorithms/blended-solids.md) of 22 pieces: pelvis, belly and chest leaning with the pose, neck, head, a cap of hair over the top and back of the head, and per side a shoulder, a short sleeve, a bare upper and lower arm, a hand, a thigh with cloth folds, a shin and a shoe. It is sculpted once, at 3.5 cm (under two pixels ten metres off), into one shape per part, and shared by every spectator in that pose. The figure sits on the origin — the seat's top — facing +X, its feet 0.45 m lower on the tier in front.

# Interfaces

`Pose::{HandsOnKnees, ArmsFolded, Cheering, ALL}`, `Wear::{Shirt, Trousers, Skin, Hair, Shoes}`, `seated(pose) -> &'static [(Wear, Shape)]`.

# Invariants

- Every pose has every part; the head is about 0.9 m above the seat, higher with an arm raised; the feet rest on the tier below; the figure fits in a seat's width and depth. Tested.
- A pose is sculpted once per process. Tested.

# Knowledge

- Built on [osage-procedural](/modules/osage-procedural.md) ([blended solids](/algorithms/blended-solids.md)); used by [osage-kart-native](/modules/kart-native.md), whose grandstand seats one in each seat out of two or so, turned a little, in clothes, skin, hair and shoes picked by its seeded generator. The crowd is its own batch, in a cloth material without grain, casting no shadow: under the grandstand's roof its shadows showed little and cost as much as the crowd (measured at T3 on the Iris Xe, 2026-11-11, the start view: 127 ms without the sculpted crowd, 146 ms with it at 2.5 cm and casting shadows, 127 ms at 3.5 cm without).
- Oracle: the crate's tests and [the render bench](/oracles/render-bench.md) screenshots.

# Reuse

Any world with seats: the poses and parts are generic; clothes and skin are the world's.

# Limits

Three poses, all seated, all still: no standing crowd, no animation. Faces have no features: the crowd is seen from the track.
