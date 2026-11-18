# Research 10 — The light coming into a block

Date: **2026-11-18**. Decisions D-134, D-135, D-136. The maintainer's design: around a walker, a client traces the model of the 9 cubes of 3 m of its block; all else reaches it only as the light that comes into the block through its outer surface, sent by the server as the walker's place and speed ask.

## Question

Can what lies outside a block be drawn, from anywhere inside it, from pictures taken on the block's surface looking out — a few per face, with how far each pixel lies — well enough to stand for the whole scene?

## Method

`tools/bistro/incoming.py`, in Blender with Cycles, on Amazon Lumberyard's Bistro (NVIDIA ORCA, CC BY 4.0) prepared by `tools/bistro/prepare.py` (its materials rebuilt from its textures, its sky the scene's own photograph at the strength its Falcor file gives, ×10):

- **Walker:** where the scene's camera stands, 1.65 m above the ground, looking as it looks; 960 × 540, 75° across, 64 samples a pixel.
- **Truth:** the whole scene from the walker.
- **Inside:** the same, but every surface outside the block transparent to the camera's rays — it still casts its shadows and its light — the block as a client would trace it, with what it does not cover left transparent.
- **Probes:** on each face of the block the walker looks out through, one at the centre of each 3 m tile, looking out: a picture and where each of its pixels lies (from a second render whose material paints each point with its position).
- **Composed:** each ray of the walker that leaves the block marches out along itself; each of the four nearest probes on that face is asked where the ray first passes behind what it saw; those that agree (the gap between the ray's point and the probe's below the march's step) give the ray's colour, weighted by their place and their agreement.
- **Naive:** the nearest probe's colour in the ray's own direction, depth unused.
- PSNR against the truth, on sRGB-encoded colours, over the whole picture and over the part beyond the block.

## Results

| Run | Block | Probes | Composed | Naive |
|---|---|---|---|---|
| first | 27 cubes (three layers) | 18, perspective 120°, 512 px, sky left transparent | 13.2 dB | 12.5 dB |
| second | 9 cubes (one layer, D-136) | 15, fisheye of the half sphere, 768 px, sky drawn | **18.7 dB** (beyond the block 20.6 dB) | 15.8 dB (16.3 dB) |

Of the rays that leave the block (88 % of the picture), the probes agreed on 98 %. A probe of 768 px took about 115 s on the laptop's processor (colour, 32 samples, and position).

## What it shows

1. **The light coming in stands for the street.** Facades, lanterns, the far buildings, the bollards, the cobbles beyond the block are where they are in the truth; the composed picture reads as the street.
2. **Depth is what makes it work.** Without it, the nearest probe's colour in the ray's direction puts near facades in the wrong place by metres (naive, 3 dB lower); with it, each ray finds the point it meets.
3. **The first run's faults were the probes', not the idea's:** their sky was left transparent (black), and a perspective probe of 120° does not see along its own tile's plane — the ground just beyond a side face, a wall just above the top face. A fisheye of the whole half sphere fixed both (+5.5 dB).
4. **With one layer, the top face does much of the work:** it is a metre above the eyes, and the walls a walker looks up at leave the block through it. Its probes see those walls at grazing angles, along the rim of their fisheye — where the remaining black blotches are (the picture's samples catching the dark corners outside the fisheye's circle). Probes over 200° put that rim well inside the circle (`tools/bistro/tiles.py`, the client's `light.wesl`).
5. **Depth edges** (a bollard before a wall) leave thin dark lines where no probe agrees: a probe a tile apart sees behind the edge what the others do not.

## Limits of this experiment

- One place, one direction; the probes' 768 px over 180° are coarser than the walker's pixels (about 0.23° against 0.08°): beyond the block, the picture is softer than the truth.
- The inside is lit by the whole scene (Cycles' own light), not by the probes: it tests what is seen beyond the block, not how the probes light the block.
- The renders run on the laptop's processor (no GPU Cycles can use there).

## In the client (2026-11-18)

`osage-paris-native` draws the same thing in real time: Bistro's meshes that reach into the walker's block, and on the block's six faces the light coming in (`light.wesl`, the same march as above in the four nearest probes). The tiles come from `paris-server`, which decides from the walker's place and speed in the world what to send: 384 px fisheyes over 200°, 16 samples, AV1 at about 20 KiB a tile (colour 16–25, distance 2–4, sky under 1), radiance within about 4 % on average.

- **Against Cycles,** from the place and direction of the scene's camera (1280 × 720, T3): 19.6 dB over the picture. Brightness, client against Cycles: the cobbles inside the block 0.95, the street beyond 1.22, the facade 1.10, the sky 1.06.
- **The block lit by its own light:** lit by the sky's photograph, the cobbles came out 2.5 times too bright — the whole sky, where a street's floor sees a strip of it between facades. Lit instead by what its tiles saw come in from each direction, seen from its centre (an environment light made on a thread when the walker enters a block), they come within 5 %.
- **Cost:** 30 ms a frame at T3 1280 × 720 on the Iris Xe of the laptop, the faces' march included; 16–26 ms with fewer tiles.
- **Walking:** 4 s of walking, filmed, brought 104 tiles (2.5 MB) as the server sent them ahead; beyond the baked area the faces stay dark, as they should: no tile there yet.
- **Sharpness** beyond the block is the probes': 384 px over 200° is about 0.5° a pixel, nine times the picture's. The bake is what makes it sharp, not the client.

## Against Cycles again (2026-11-18, afternoon)

Set beside Cycles, the client's picture was blurred beyond the block and bent in places: a window drawn twice, a sign doubled. Measured region by region from the same place (PSNR on sRGB pictures; brightness, client over Cycles):

| Client | Picture | Cobbles in the block | Street beyond | Facades | Sky |
|---|---|---|---|---|---|
| first | 19.6 dB | 29.2 dB | 21.9 dB | 19.1–19.4 dB | 27.3 dB |
| exact march | 20.7 dB | 29.3 dB | 22.8 dB | 19.8–20.2 dB | 27.3 dB |
| and Blender's AgX, no SSAO | **22.4 dB** | 30.3 dB (×0.97) | 25.8 dB (×0.97) | 21.8–21.9 dB (×1.02–1.06) | 29.0 dB (×1.00) |

What each was:

1. **The march** took the colour where the ray had already passed behind what a probe saw, up to a third of the distance beyond (2.5 m at 10 m), accepted gaps of metres, and mixed four probes that disagreed: hence the doubled window. Now each probe's last step is halved eight times to find where the ray goes behind its surface, a probe that saw something nearer there is set aside (the march going on behind it), and those left weigh by how like the eye's their look at the point is. The deformations went.
2. **The distance** was lossy (libaom at 12): at the edges, up to 74 of its 4096 levels off, a metre and a half at 10 m. At constant quality 1 it stays within 7 levels for 11 KiB a tile instead of 2; exact, 31 KiB. Little gain in PSNR with these tiles, but no more edges moved.
3. **The look.** Both pictures go through AgX, but not the same one: Bevy's is Sobotka's first, Blender's (4.0 and on) another. Without tonemapping or bloom the light coming in came within 4 % of Cycles everywhere; through the two AgX, a fifth brighter in the mid-tones. A table of Blender's own AgX (`tools/bistro/agx-lut.py`: Blender shows a lattice of light, 64³, and the table reads it back; checked against Cycles' picture at 53.9 dB) replaces it in the client.
4. **SSAO**, over a light that comes in already shaded by the street, darkened the cobbles by a quarter. Left out (also 5 ms a frame on the Iris Xe).

The whole street drawn from its model by the client at T3, without the light coming in, comes to 13.7 dB: lit by one environment light, its facades come out up to 1.9 times too bright and it has no sky. What the light coming in carries is Cycles' own light; the raster renderer cannot make it.

What remains is sharpness: the tiles' 384 px over 200 degrees, 16 samples denoised, against the picture's nine times finer pixels.

## Sharp tiles, traced light, probes inside the cube (2026-11-18, evening)

On a rented NVIDIA L4 (`tools/bistro/cloud-bake.sh`, the render bench's project), the 30 tiles of the starting block baked at 2048 px and 256 samples (58 s a tile on OptiX), and the picture to measure against at 1280 × 720 and 1024 samples. Against it (PSNR on sRGB pictures, from the scene camera's place):

| Light | Picture | Street beyond | Left facade | Right facade | Unseen by any probe | Per cube crossed |
|---|---|---|---|---|---|---|
| 30 surface tiles, 384 px | 22.3 dB | 25.7 dB | 21.6 dB | 21.8 dB | — | ~0.4 MB |
| 30 surface tiles, 2048 px | 25.9 dB | 29.6 dB | 26.3 dB | 23.5 dB | 3.8 % | ~4–5 MB |
| the same, holes filled from behind | 26.2 dB | 29.3 dB | 27.0 dB | 25.6 dB | 3.8 % | ~4–5 MB |
| 1 probe at the cube's centre | 27.2 dB | 30.9 dB | 22.8 dB | 28.2 dB | — | 0.86 MB |
| **5 probes inside the cube** | **28.4 dB** | **31.7 dB** | **29.1 dB** | **28.6 dB** | **0.04 %** | 4.3 MB |

1. **Sharpness is the tiles'.** At 2048 px the facades, lanterns and far buildings come out as in Cycles; the client draws it at 19–25 ms a frame on the Iris Xe (T3). A tile weighs 355 KiB (light 251, distance 84, sky 20).
2. **What the surface tiles cannot do.** Where an object stands near the block (bollards, a kerb, pilasters, window jambs, 0.5–2 m beyond a face), the eye, 3–5 m back, sees behind it what no probe on the face sees: 3.8 % of the picture here. Stretching the object over the hole drew bollards as stairs; filling the hole with what the probe saw just beside and behind the object (`light.wesl`'s `beside`) hides that, but guesses.
3. **Probes inside the walker's cube** (`tools/bistro/cube-probes.py`, the client's `--cube-probes`): full spheres (equirectangular, 4096 × 2048, 256 samples, 2.9 min each on the L4) at the eyes' height, at the cube's centre and 0.75 m towards its corners; the block's surfaces made transparent to the camera's rays (from 30 cm over the ground: the street and its kerbs stay), so that each sees what comes into the block. Seen from nearly where the eye is, objects near the block hide nothing it sees: the holes fall a hundredfold, the stairs, broken awnings and striped facade go, 28.4 dB. One probe alone does as well beyond the street's end but leaves the left facade's jambs open (22.8 dB there).
4. **Traced (T4), on the L4**: drawn after the opaque pass, so that the traced light's denoiser leaves them, the faces come out exactly as at T3 (24.9 dB over the picture, 5 ms a frame). The block itself is lit by its faces as lamps of their tiles' light, weighed by the cosine: its cobbles come out at 0.64 of Cycles. A ray leaving the block should take the tiles' light in its own direction, inside Solari: not done.

## Walking with the probes (D-137)

The five probes of four more cubes along a walk baked (4096 × 2048, 128 samples, about 8 min a cube on the L4), the client walked 7 s online, `paris-server` sending each cube's probes as the walker's place and speed in the world asked (`needed_light`), or its block's surface tiles where it had none: 194 sendings, 16.9 MB, about 19 Mbit/s. A probe takes the laptop's processor 1.3 to 2 s to decode (AV1 and the colour's conversion, both now on several threads; 2.1 to 2.6 s before), and the walker enters a cube every two. What made the walk whole:

1. **The centres' probes first**, of every cube within reach, then the tiles of the cubes without probes, then the other probes; a client draws a cube from its centre's alone (27.2 dB) and again once all five have come.
2. **The server asked on a thread of its own**: behind probes a second long each, its answers came three seconds late.
3. **The block drawn stays** while the next one's light is on its way and the eyes are still inside it (a walker who steps into a neighbouring cube is): its faces, its light, its meshes. Falling back on surface tiles that had not all come drew black boxes.

The picture is then whole along the walk, sharp in the cubes with probes, the tiles' blur in the one without; 52 ms a frame on the Iris Xe while it decodes.

On the L4, from the scene camera's place with the cube's probes: 28.5 dB at T3, the cobbles at 0.99 of Cycles. At T4 the block is traced and lit by its faces as lamps of the centre probe's light through each (as in the tiles' case, about 0.64 of Cycles on the cobbles). Without the lamps, the rays leaving the block met nothing and Solari's realtime path took the camera's environment map — the centre probe's light, direction by direction — alone: the cobbles fell to 0.20. Lighting the traced block by the light coming in, ray by ray, is left to an RTX machine to work out. Meanwhile the lamps are scaled for the floor in the middle of the block to gather from them, over the half sphere above weighed by the cosine, what the centre probe saw come in: 27.3 dB over the picture at T4 (24.9 with the tiles' plain lamps), the cobbles at 0.71 of Cycles, all beyond the block as at T3.

## Next

1. The whole street baked, at 1024–1536 px and more samples: about 2 000 tiles, a cloud GPU's work (hours on the laptop's processor for 384 px).
2. Tiles of neighbouring planes as one video (they differ by a few metres of parallax), for the stream.
3. Probes between tiles' centres where walls stand near a face, to see behind depth edges.
4. The block traced at T4 (Solari): its faces are lamps of their tiles' mean light for the rays; tried on an RTX card.
