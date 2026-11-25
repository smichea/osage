---
type: Module
title: osage-paris-native
description: The native client of the Paris world — a walk on foot in Bistro, seen from the walker's eyes — drawing the 9 cubes of 3 m around the walker and, beyond them, the light coming into its block, streamed and decoded as it walks.
resource: https://github.com/smichea/osage/tree/main/worlds/paris/native
licence: MIT OR Apache-2.0
status: draft
tags: [paris, client, bevy, light]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-18T15:00:00Z" }
---

# Purpose

Walk in a street of Paris as real as its photographs, the client drawing in 3D only the walker's surroundings and the rest from the light coming in (D-136).

# Concepts

- **The block's faces:** six quads on the outer surface of the walker's block, facing in; the street's meshes inside the block hide them where they stand, and they hide everything beyond.
- **Incoming light material:** on each face, for each pixel, the ray from the eye leaves the block there; marching out along it in the probes of the four nearest tiles (fisheyes over 200 degrees, with distances), each finds where the ray first goes behind what it saw, halving the last step until the place is known; those that saw the ray's point itself there (not something nearer hiding it) give its radiance, the more the more their look at the point is like the eye's; where none did, what a probe saw just beside and behind the thing hiding the point fills the hole. The faces are drawn after the opaque pass (alpha blended, opaque), so that traced light's denoiser leaves them.
- **Probes inside the cube (D-137):** the light comes from the five full spheres taken inside the walker's cube (`tools/bistro/cube-probes.py`, equirectangular), each with where it stands (its `meta.json`, fetched with it), drawn as soon as the centre's has come and again once all have; where they are not baked, from the block's 30 surface tiles. While the next cube's light is on its way and the eyes are still inside the block drawn, that block stays, its faces, light and meshes. `--cube-probes N` takes N of them, 0 the surface tiles only. A trial, `--block-probes`: the block drawn from the centre probes of its nine cubes instead, one probe a cube shared by the blocks it is in, the nearest thing any probe met along the eye's ray taken (27.6 dB against the five's 28.0, blurred near the block). Another, `--ring`: the street's meshes round the block, out to 13.5 m, drawn with the light the probes saw of each point, without a march (28.0 dB, 33 ms a frame against 57).
- **Traced block (D-138):** at T4 the rays see the walker's block alone — each mesh of the street that reaches out of it cut at its surface (`clip`, its attributes interpolated along the cuts) — under a dome of 40 m radius round it, facing in, shining what the cube's centre probe saw come in from each direction: a ray leaving the block takes the light that came in from its own direction.
- **Look:** radiance is shown through Blender's own AgX, the view transform Cycles' pictures are seen through: a table of it (`tools/bistro/agx-lut.py`, under the asset content as `blender/agx-64.bin`) put where Bevy reads its Blender Filmic table. Bevy's AgX is Sobotka's first, brighter in the mid-tones by about a fifth.

# Architecture

`street` loads the street's glTF (from `tools/bistro/export.py`), places each mesh, and shows only those whose box reaches into the walker's block; it lights them by the sky's photograph at the bake's strength, as an environment light. `light` asks the world's server of light, a few times a second and one question at a time on a thread of its own, what the walker needs (or, walking alone, works it out from the street's folder), fetches and decodes the probes and tiles on four threads of their own, at a lower priority than the frame's: what is on the walker's way first, then the tiles of probes where it looks, then what is beside its way, as the main thread tells them again four times a second (D-142: what the walker no longer needs is never fetched). The probes' light goes into two texture arrays of six layers made once (radiance 10 bits a band, distance as floats; made anew for each cube, 320 MiB, the picture stopped for seconds each time the walker entered one): a layer is lent to each probe of the cube the eyes are in and to the centre of each of the first two cubes on the walker's way (standing, of those it faces), written before it gets there, and a cube left keeps its layers until another probe needs them, the farthest behind first. Each probe, as it comes, is written into its layer by the render world straight from its decoded light, 16 MiB a frame in bands of rows, and shown once whole — the walker's previous block drawn until the centre's is (stacking the probes again into new arrays on the main thread held frames up to 0.9 s). A block lit by its surface tiles stacks them into two arrays as before. Then `light` moves and tells the faces (`IncomingLight`, `light.wesl`). The block is lit by its own light: when the walker enters it, what its tiles saw come in from each direction, seen from its centre, is made into the camera's environment light on a thread (lit by the whole sky instead, a street's floor came out 2.5 times too bright). osage-render's sky, sky light and screen-space ambient occlusion are left out: over the light coming in, already shaded by the street, SSAO darkened the cobbles by a quarter against Cycles. `look` puts Blender's AgX in place. At T4 (`raytracing` feature), `street` cuts the meshes reaching out of the block to it, for the rays (and the eye, to whom the faces hide what is cut), and `light` puts the dome round the block once its centre probe has come, its picture made on the environment's thread; a block lit by its surface tiles has its faces as lamps of their tiles' mean light for the rays instead (`Emitter`). `walker` takes the keys and the mouse, steps the walker with the world's walk grid at once and puts the camera at its eyes; `net` sends its strolls, takes the server's place when it parts from the client's, learns the server of light from `paris.world/1`, and draws the other walkers as people. Units: the renderer's radiance (1 the white of Cycles' picture) times `NITS` is luminance under osage's sunny exposure. The walker sets out from `--at` (its eyes), facing `--heading`, or from the start of the street's walk; `--shot` draws one picture from there once the block's light has come and lights it, and times the frames before it; `--film` walks (`--run`: runs) straight ahead from there, a second after the street is there, saves every third picture, and tells the time from each frame to the next and how long after the eyes entered each cube its light showed; without a window a frame waits for the GPU to have drawn the one two before it, as showing it would (`hud::pace`); a probe packed as tiles (D-141) is drawn first from its half, doubled into its layer on a thread of its own, and its tiles, fetched whole, are decoded four at a time on the light threads from where the walker looks outwards — running, the middle of the view (35°); walking, all of it (70°); standing, all round — then those of the centres of the cubes it goes to, each written over the half into its layer once the half is (`Layer::at`) and kept no longer; the tiles decoded for cubes left behind go; `--shot` waits for every tile; `--plain` shows radiance without tonemapping or bloom and `--whole` draws the whole street from its model, for measures against Cycles.

# Interfaces

The program `paris` (`[ws://HOST:7878] [--tier T1..T4] [--street DIR] [--offline] [--fov DEGREES] [--at X,Y,Z] [--heading DEGREES] [--shot FILE] [--film DIR SECONDS] [--plain] [--whole] [--cube-probes N]`), built with `--features raytracing` to trace the block at T4. `look::TABLE`, the table of Blender's AgX it reads. The program `paris-update` and the launchers `worlds/paris/release/{windows/walk.bat, macos/walk.command}` (`paris --offline --ring --block-probes`), packed by the `release-paris` workflow for `paris-v…` releases; what they download is `osage-paris-street.zip` (the scene) and `osage-paris-light.zip` (the baked light), made by `tools/bistro/content-zip.sh` (D-143).

Replicated walkers request `PlannedHumanSteps` when the current camera resolves the difference at the feet. Paris uses the [object projection](/algorithms/projected-mesh-error.md) with an empirical 0.20 m displacement allowance, enabling correction above two pixels and keeping it down to one pixel. The four-metre neighbourhood around the eye keeps its contacts through a turn. When the view is unavailable Paris keeps full correction. The world still supplies heading, smoothed world travel and clip pace. The [human module](/modules/osage-humans.md) provides each body's sole geometry and reads the live animation player's phase.

The headless crowd replay uses both Quaternius bodies in 24 instances, walks and jogs them across repeated raised strips, and turns the observer after 120 of 240 frames. From the repository root with the human assets installed, the following commands check ground work, phase, restoration and projected foot displacement. Timed runs omit the counters. Linux also reports process CPU time summed over the workers. The clock covers the animation update and the placement of the walkers; rendering and asset preparation are outside it.

```sh
cargo build --release -p osage-paris-native --example pedestrian_crowd
out=bench/out/paris-crowd
mkdir -p "$out"
for mode in full view clip; do
    target/release/examples/pedestrian_crowd "$mode" --count > "$out/$mode-count.csv"
    target/release/examples/pedestrian_crowd "$mode" > "$out/$mode-time-1.csv"
done
python3 tools/animation/pedestrian-crowd.py "$out"
```

# Invariants

- Remote walkers choose between idle, walk and jog from the horizontal distance actually advanced on the rendered frame. Walk and jog scale their playback by the nominal speed of the clip supplied by [osage-humans](/modules/osage-humans.md). Snapshot spacing and vertical corrections do not directly set the cadence (D-222).
- Nothing beyond the walker's block is drawn from the model: a mesh is shown only while its box reaches into the block, and the faces hide the rest. Traced, nothing beyond it is in the rays' scene either: what reaches out is cut at its surface, and the dome shines in its place.
- Measured (research 10), from the scene camera's place against Cycles through the same AgX: with the 384 px tiles, 22.4 dB over the picture (19.6 before the exact march, Blender's AgX and SSAO left out); with 2048 px tiles, 26.2 dB; with five probes inside the cube, 28.4 dB, the part of the picture no probe sees down from 3.8 % to 0.04 %. Every region within 10 % of Cycles' brightness; 12–25 ms a frame at T3 1280 × 720 on an Iris Xe, 5–10 ms at T4 on an L4. Traced under the dome and cut to the block (L4): 28.1 dB, the block's floor at 0.88 of Cycles (27.3 and 0.71 with the faces as lamps; T3 28.5 and 0.99). The whole street drawn from its model at T3 instead: 13.7 dB.

# Knowledge

- Replicated walkers use [forefoot planting](/algorithms/forefoot-plant.md) with the raw cell height from the same walk grid used by the movement controller (D-223, D-224).

- Decisions: D-136, D-143 (the release). World and server: [osage-world-paris](/modules/paris-world.md); transport and decoding: [osage-hologram-stream](/modules/osage-hologram-stream.md); rendering by tier: [osage-render](/modules/osage-render.md).

# Reuse

The incoming light material and its arrays serve any world drawn as a block in 3D inside light that comes in through its surface.

# Limits

The block is rasterised at T1–T3; at T4 it is traced under the dome, its floor still 11 % darker than T3's under the same light: about 8 from Solari's realtime estimator (its reference path tracer, `OSAGE_PATHTRACE=1`, comes to 0.95 of T3), the rest the model (not the denoiser), and the dome is the centre probe's light wherever in the block a ray leaves it from. Solari traces the street's alpha-masked materials opaque. Beyond the block the picture is as sharp as the tiles: at 384 px over 200 degrees, nine times coarser than the picture; at 2048 px, near the picture's. Near the block, surface tiles leave holes where the eye sees behind an object no probe sees past; probes inside the cube close them, but only walking alone so far. Its environment light is seen from the block's centre: no shadow of a single thing outside falls inside it. Each block's arrays are made again when the walker enters it; decoding runs on the CPU.
