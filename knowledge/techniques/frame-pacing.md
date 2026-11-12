---
type: Technique
title: Holding a frame rate
description: Given the frame rate a person wants, the picture climbs down a ladder of cheaper pictures until frames come at that rate, and back up when there is room.
resource: https://github.com/smichea/osage/blob/main/crates/osage-render/src/pace.rs
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
tiers: [T1, T2, T3, T4]
status: draft
tags: [rendering, frame-rate, dynamic-resolution]
sources:
  - id: own
    resource: "osage's own design, 2026-11-12, after the kart ran slowly on an Intel HD Graphics 630 at T1"
    title: osage design
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-12T06:00:00Z" }
---

# What it does

A tier fits a class of hardware; this fits the machine actually there, and the part of the world it is drawing. The application names a frame rate (`FrameTarget`); without one nothing changes, as in the bench.

# How

An arrangement of osage's own[^own]:

1. The rate aimed at is the target, never above the screen's refresh rate (the primary screen's, or where none is named, as on Wayland, the fastest screen's): with vertical sync no more can come.
2. The rate is the median of the last 30 frames, so that one slow frame (a shader compiled, a texture uploaded) moves nothing; after a change it waits 1.5 s before judging again.
3. Under 92 % of the aim, one rung down the ladder; under 60 %, two.
4. Going up is a trial: at 97 % of the aim or more for long enough (4 s at first), one rung up. A trial undone within 3 s doubles the wait before the next (up to 64 s): with vertical sync a rate held at the screen's cannot say how much room is left, and the doubling stops the picture from hunting.

The ladder, from the tier's own picture: picture size 100, 90, 80 %; then the costly screen effects off (ambient occlusion, contact shadows, motion blur, bloom; kept on the camera aside and put back); 70 %; the sun's shadows off; 55, 45, 33 %. The size is a share of what [resolution](/techniques/resolution-and-upscaling.md) would draw.

# Settings per tier

The same ladder at every tier; at T1 the effects rung changes nothing, since T1 has none of those effects.

# Cost

None measurable: a median of 30 numbers a frame. Changing the picture size recreates one texture.

# Limits

Measured on the Intel Iris Xe (Mesa 26.2), 2026-11-12, kart at T4 aiming at 60: the rate stayed near 45 whatever the size, so what bounds it there is not the pixels: the ladder does not yet reach the tier's other techniques (atmosphere, sky light), textures or scenery.

# Oracle

Its unit tests run machines of known speed through it: a fast one keeps the full picture, a slow one climbs down to the rate, a single long frame changes nothing, with vertical sync it settles without hunting, and room brings the picture back. In a game, every change is logged (`pace: … rung …`).

# Reuse

A median of frame times, a ladder, and trials up that back off: any engine with dynamic resolution can use it.

[^own]: osage design
