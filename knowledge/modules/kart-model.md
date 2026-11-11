---
type: Module
title: osage-kart-model
description: A racing kart and its driver modelled to scale from simple shapes, in a team's colours, each finish — varnish, satin, rubber, steel, alloy, chrome, cloth, visor — its own material.
resource: https://github.com/smichea/osage/tree/main/worlds/kart/components/kart-model
licence: MIT OR Apache-2.0
status: draft
tags: [kart, vehicle, model, component]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-11T09:30:00Z" }
---

# Purpose

The karts of the kart game as a real racing kart looks: the replacement of the toy kart, a component of the kart game (D-035).

# Concepts

- **Finish:** how a surface reflects light; one material each, shared by every kart: glossy paint, satin plastic, rubber, painted steel, machined alloy, chrome, race-suit cloth, a tinted visor.
- **Livery:** a team's colours in sRGB: the main colour, a second for stripes and the helmet's lower half, the frame's paint, the suit's own colour (rarely the kart's: a driver in the kart's colour from head to foot reads as a toy; the team's colour goes on the suit's panels), and its racing number, drawn in seven segments on the front panel, on plates on the side pods and on the rear bumper's plate.
- **Parts:** the body (frame, bodywork, engine, seat, steering, the driver below the neck), the helmet, and a front and a rear wheel; each part is a mesh per finish.

# Architecture

Built from simple shapes ([osage-procedural](/modules/osage-procedural.md)): prisms with rounded corners for the nose, side pods and the black rear bumper, as moulded plastic is; tubes swept along paths for the frame, the exhaust and the driver's limbs; capsules and spheres scaled for the bodywork; solids of revolution for the tyres, rims and helmet; slices of revolution for the seat's shell, the helmet's stripe and the visor. Proportions are a real kart's: a 1.04 m wheelbase, 10 and 11 inch slicks, a frame five centimetres off the road, the engine on the driver's right with the chain guard down to the sprocket, a brake disc on the rear axle, a driver in a rib protector and a foam neck brace, whose helmet tops out about a metre high. Colours are in the vertices, so a kart is a few meshes with a handful of shared materials.

# Interfaces

`build(Livery) -> Kart { body, helmet, wheels }`, `Finish::{ALL, material}`, and the places the client animates around: `WHEELS`, `AXLES`, `NECK`, `EXHAUSTS`, `REAR_CONTACTS`.

# Invariants

- A real kart's size: about 1.9 m long, at most 1.4 m wide, its lowest point a few centimetres off the road, the helmet's top about a metre high. Tested.
- Each wheel touches the road and has its tyre's width; tyres are rubber, rims alloy. Tested.
- Each finish is its own material; chrome is metal and nearly a mirror, cloth is rough, paint is glossier than satin. Tested.
- A number lights its digits' segments, the digits' cells centred. Tested.

# Knowledge

- Used by [osage-kart-native](/modules/kart-native.md), which animates it: wheels that spin and steer, a body that leans and pitches a little, a hop into a drift, a helmet that turns into bends.
- Oracle: the crate's tests and [the render bench](/oracles/render-bench.md) screenshots.

# Reuse

The proportions and the finishes are what makes a kart read as real; the shapes are placeholders a modelled kart (glTF) can replace part for part.

# Limits

Bodywork rounded only along its profile, its ends square; stickers and numbers are blocks, not printed decals; no cloth folds. The driver is a mannequin under a helmet. Paint has no clear coat: Bevy's paled every dome facing the sky to near white. A modelled kart and driver (workstream R9) are the next step.
