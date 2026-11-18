---
type: Module
title: osage-cubes
description: Nested cubes of 81, 27, 9 and 3 m around a viewer — the 3 m cubes around it in 3D, the coarser ones holograms, each level centred on the cube the viewer is in — and which views of those holograms it needs before long.
resource: https://github.com/smichea/osage/tree/main/crates/osage-cubes
licence: MIT OR Apache-2.0
status: draft
tags: [streaming, holograms, level-of-detail, world]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-18T03:40:00Z" }
---

# Purpose

Decide, for a viewer anywhere in a world, what it sees in 3D and what as holograms the server provides, and which views of each hologram it will need soon — the world cut into nested cubes (D-134).

# Concepts

- **Cube:** a level (0 for 81 m, 1 for 27 m, 2 for 9 m, 3 for 3 m) and a place in that level's grid, which starts at the world's origin.
- **Neighbourhood:** a cube and its 26 neighbours, 3 × 3 × 3 cubes of one level.
- **Centre:** at each level, the cube the viewer is in. The finest centre's neighbourhood, a block of 9 m around the viewer, is drawn in 3D (D-135). Stepping into a neighbouring 3 m cube makes it the centre: the block slides by one cube; each coarser level slides when the viewer crosses one of its own cubes.
- **Carving:** a coarser level's neighbourhood holds the finer one, but they do not line up (the finer is centred on a smaller cube): a hologram is drawn without what lies inside the next finer neighbourhood, so that every place is drawn once, by the finest level around it.
- **Reach:** the directions a cube may be seen from within a horizon by a viewer moving at most so fast — from any place it can reach towards any point of the cube; far cubes need few, near ones all.

# Architecture

`shown(eye, tops, filled)` lists the viewer's 3D cubes and its holograms: the 27 cubes of 3 m around the viewer's, in 3D; the 27 of 9 m around its 9 m cube, carved by that block; the 27 of 27 m carved by the 9 m neighbourhood; every 81 m cube of the world (`tops`, those with something in them) carved by the 27 m neighbourhood. Cubes with nothing in them (`filled`) and those the finer neighbourhood holds whole are left out. `reach(cube, eye, speed, horizon)` is the cone of directions, from the cube towards the viewer, it may be seen from; `Directions { step }` the views a level is photographed from, all around and from straight below to straight above, and `within(cone)` those worth having. `Plan::new(eyes, …)` gathers, for every place a viewer's eye can be, the views of every cube it sees: what a world bakes.

# Interfaces

`Cube { level, x, y, z }` (`containing`, `size`, `min`, `max`, `centre`, `radius`, `contains`, `parent`, `children`, `key`, `parse`), `SIZES`, `FINEST`, `FINEST_HOLOGRAM`, `Region` (`around`, `contains`, `holds`), `Hologram { cube, carve }`, `Shown { models, holograms }`, `shown`, `Cone`, `reach`, `Directions` (`azimuths`, `elevations`, `direction`, `within`), `Plan`.

# Invariants

- A point is in its cube at every level, and its cube's parent is its cube a level up. Tested.
- Wherever the viewer is, every place of the world is drawn exactly once: by a 3D cube, or by one hologram whose carving leaves it. Tested.
- Stepping into a neighbouring 3 m cube makes it the centre of the 3D block; no hologram lies inside the block. Tested.
- A far cube needs a narrow cone of directions, a near one every direction; the view towards the viewer is always among those within the cone. Tested.

# Knowledge

- Decisions: D-134 (the maintainer's nested cubes, centred on the viewer; holograms streamed by the server, only the angles walking speed and place need); D-135 (the 3 m cubes around the viewer in 3D, ray traced; holograms from 9 m).
- Streams the holograms it lists: [osage-hologram-stream](/modules/osage-hologram-stream.md); draws them: [osage-hologram](/modules/osage-hologram.md).

# Reuse

Any world that keeps near things in detail and far ones coarse around a moving viewer, whatever draws them. Bevy's math only, no rendering; permissive licence.

# Limits

Cubes only (no other shapes of block). A hologram is carved by its depth, point by point: what the carved part hid behind it in the coarse view was not photographed, and may show a hole at an angle the views were not taken from. How many views a world's plan holds is not measured yet.
