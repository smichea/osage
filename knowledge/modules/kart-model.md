---
type: Module
title: osage-kart-model
description: A racing kart and its driver modelled to scale from simple shapes, in a team's colours, covered in sponsors' stickers, each finish — varnish, satin, rubber, steel, alloy, chrome, cloth, visor, decal — its own material.
resource: https://github.com/smichea/osage/tree/main/worlds/kart/components/kart-model
licence: MIT OR Apache-2.0
status: draft
tags: [kart, vehicle, model, component]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-11T23:00:00Z" }
---

# Purpose

The karts of the kart game as a real racing kart looks: the replacement of the toy kart, a component of the kart game (D-035).

# Concepts

- **Finish:** how a surface reflects light; one material each, shared by every kart: glossy paint, satin plastic, rubber, painted steel, machined alloy, chrome, race-suit cloth, a tinted visor, and stickers (glossy, alpha-tested, their marks from an atlas the client gives the material).
- **Stickers:** an atlas of 32 abstract sponsors' marks drawn by the program — a panel, a disc or a ring, a stripe or a chevron, bars standing for a name, in sponsors' colour pairs, no real brand — laid as quads on the pods' sloping faces, the nose, beside the front panel's number, on the white patch of the driver's back, on the back of the helmet under its small spoiler, along the top of each pod, on the nose's flanks and along the rear bumper, a set for each racing number.
- **Livery:** a team's colours in sRGB: the main colour, a second for stripes and the helmet's lower half, the frame's paint, the suit's own colour (rarely the kart's: a driver in the kart's colour from head to foot reads as a toy; the team's colour goes on the suit's panels), and its racing number, drawn in seven segments on the front panel, on plates on the side pods and on the rear bumper's plate.
- **Parts:** the body (frame, bodywork, engine, seat, steering column), the driver below the neck, skinned to eight bones (the hips and legs; each arm's upper arm, forearm and hand; the spine, from the waist up: belly, chest, rib protector, neck and shoulders), the steering wheel around its column, the helmet with the neck brace, and a front and a rear wheel; each part is a mesh per finish.
- **Driver pose:** `driver_pose(turn, lean)` gives the bones with the steering wheel turned and the trunk bent about the waist: the hands stay on the rim at ten to two and turn with it, the shoulders go where the spine takes them, and each arm joins them by two-bone inverse kinematics (the arm's lengths kept, the elbow bending out and down as sculpted, its rest carried by the spine). `neck(lean)` is where the helmet then sits. `driver_cheering(turn, lean, raised, time)` raises either arm off the wheel by a share, the wrist eased from the rim to a fist above the shoulder that waves (D-087).

# Architecture

Built from simple shapes ([osage-procedural](/modules/osage-procedural.md)): the moulded plastic — the nose with its crown, black lip and the dome over the pedals, the side pods with their skirt and rounded fronts, the black rear bumper's bar and wings behind the wheels — sculpted once as rounded boxes and ellipsoids blended ([blended solids](/algorithms/blended-solids.md)), painted per livery; tubes swept along paths for the frame and the exhaust; the driver's body sculpted as one smooth surface, limbs and trunk blended, cloth folded at the elbows and knees, panels painted on — the team's colour down the trunk's sides and in a stripe down the outside of each sleeve and leg, the second colour across the chest only (D-099) ([blended solids](/algorithms/blended-solids.md)), once for every driver; capsules and spheres scaled for the bodywork; solids of revolution for the tyres (a scrubbed tread between black walls), the rims (a lip over a dark well, six spokes, a hub with three nuts, a valve) and the helmet (with a chin bar under the visor, its vent and the brow's slots); slices of revolution for the seat's shell, the helmet's stripe and the visor. Proportions are a real kart's: a 1.04 m wheelbase, 10 and 11 inch slicks, a frame five centimetres off the road, the engine on the driver's right with the chain guard down to the sprocket, a brake disc on the rear axle, a driver in a rib protector and a foam neck brace, whose helmet tops out about a metre high. Colours are in the vertices, so a kart is a few meshes with a handful of shared materials; the body's and the tyres' are soiled once built — tinted towards road dust lower down and mottled all over (D-098). The livery breaks the paint as painters do: a stripe along each pod's top, a slash rising across its flank, a chevron over the nose, the pieces meshed in cells of 8 mm so that the colours meet cleanly.

# Interfaces

`build(Livery) -> Kart { body, driver, steering, helmet, wheels }`, `steering_column()`, `DRIVER_BONES`, `SPINE`, `driver_rest()`, `driver_pose(turn, lean)`, `neck(lean)`, `Finish::{ALL, material}`, `decals::{atlas, Decals}` (the marks and the quads that lay them, for a world's boards too), `numerals(number, height, proud)` (seven-segment digits, for a world's signs too), and the places the client animates around: `WHEELS`, `AXLES`, `NECK`, `EXHAUSTS`, `REAR_CONTACTS`.

# Invariants

- At rest the pose is the sculpture's; turned, the arms keep their lengths and the hands stay on the rim. Tested.
- A real kart's size: about 1.9 m long, at most 1.4 m wide, its lowest point a few centimetres off the road, the helmet's top about a metre high. Tested.
- Each wheel touches the road and has its tyre's width; tyres are rubber, rims alloy. Tested.
- Each finish is its own material; chrome is metal and nearly a mirror, cloth is rough, paint is glossier than satin. Tested.
- A number lights its digits' segments, the digits' cells centred. Tested.

# Knowledge

- Used by [osage-kart-native](/modules/kart-native.md), which animates it: wheels that spin and steer, a steering wheel that turns twice as far with the driver's arms following it (a skinned mesh, its bones set from `driver_pose` every frame), the driver's trunk thrown out of bends and forward under braking on a spring, the helmet carried with it, a body that leans and pitches a little and rattles on kerbs and grass, a hop into a drift, a helmet that turns into bends.
- Oracle: the crate's tests and [the render bench](/oracles/render-bench.md) screenshots.

# Reuse

The proportions and the finishes are what makes a kart read as real; the shapes are placeholders a modelled kart (glTF) can replace part for part.

# Limits

Bodywork is moulded from a few rounded pieces, without a real kart's fine lines and vents; numbers are blocks; stickers are flat quads on the flat faces; the driver's body is generic, its face never seen; only its arms move. Paint has no clear coat: Bevy's paled every dome facing the sky to near white. A modelled kart and driver (workstream R9) are the next step.
