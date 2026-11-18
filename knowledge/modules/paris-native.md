---
type: Module
title: osage-paris-native
description: The native client of the Paris world — a walk on foot in Bistro, seen from the walker's eyes — drawing the 9 cubes of 3 m around the walker and, beyond them, the light coming into its block, streamed and decoded as it walks.
resource: https://github.com/smichea/osage/tree/main/worlds/paris/native
licence: MIT OR Apache-2.0
status: draft
tags: [paris, client, bevy, light]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-18T07:30:00Z" }
---

# Purpose

Walk in a street of Paris as real as its photographs, the client drawing in 3D only the walker's surroundings and the rest from the light coming in (D-136).

# Concepts

- **The block's faces:** six quads on the outer surface of the walker's block, facing in; the street's meshes inside the block hide them where they stand, and they hide everything beyond.
- **Incoming light material:** on each face, for each pixel, the ray from the eye leaves the block there; marching out along it in the probes of the four nearest tiles (fisheyes of the half sphere, with distances), those that agree that the ray meets what they saw give its radiance.

# Architecture

`street` loads the street's glTF (from `tools/bistro/export.py`), places each mesh, and shows only those whose box reaches into the walker's block; it lights them by the sky's photograph at the bake's strength, as an environment light. `light` asks the world's server of light, a few times a second, what the walker needs (or, walking alone, works it out from the street's folder), fetches and decodes the tiles on threads of their own, and when the walker enters another block or light comes for its own, stacks that block's tiles into two texture arrays (radiance 10 bits a band, distance as floats) and moves and tells the faces (`IncomingLight`, `light.wesl`). `walker` takes the keys and the mouse, steps the walker with the world's walk grid at once and puts the camera at its eyes; `net` sends its strolls, takes the server's place when it parts from the client's, learns the server of light from `paris.world/1`, and draws the other walkers as people. Units: the renderer's radiance (1 the white of Cycles' picture) times `NITS` is luminance under osage's sunny exposure. `--shot` draws one picture from a place once the light has come.

# Interfaces

The program `paris` (`[ws://HOST:7878] [--tier T1..T4] [--street DIR] [--offline] [--shot FILE --at X,Y,Z --heading DEGREES]`).

# Invariants

- Nothing beyond the walker's block is drawn from the model: a mesh is shown only while its box reaches into the block, and the faces hide the rest.

# Knowledge

- Decisions: D-136. World and server: [osage-world-paris](/modules/paris-world.md); transport and decoding: [osage-hologram-stream](/modules/osage-hologram-stream.md); rendering by tier: [osage-render](/modules/osage-render.md).

# Reuse

The incoming light material and its arrays serve any world drawn as a block in 3D inside light that comes in through its surface.

# Limits

The block is rasterised at T1–T3; traced at T4 it is not yet. Its light comes from the sky's photograph, not from the tiles: shadows of what stands outside the block do not reach inside it. Each block's arrays are made again when the walker enters it; decoding runs on the CPU.
