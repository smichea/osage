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
With ReSTIR off as well (Solari's world cache alone), the environment map left the cobbles at exactly 0.20 again: the realtime path does not seem to take the camera's environment map here at all, whichever way it gathers. To be found on a machine that traces rays, before the lamps can go.

## The traced block under a dome (D-138, 2026-11-19)

The block traced at T4, on the L4, from the scene camera's place, with the cube's probes:

| The block's light for the rays | Picture | Floor (traced) |
|---|---|---|
| Its faces as lamps of the centre probe's light, scaled | 27.3 dB | 0.71 |
| A dome round it, shining the centre probe's light, direction by direction | 28.0 dB | 0.86 |
| The same, the street cut to the block for the rays | 28.1 dB | 0.88 |
| The same, unfiltered | 27.7 dB | 0.81 |
| T3 (raster, the same light as its environment map) | 28.5 dB | 0.99 |

The floor is measured where the block's own cobbles are seen (the picture's lowest tenth); the street beyond the block, drawn by the faces, is 0.98 at T3 and T4 alike.

1. **A dome.** A sphere of 40 m radius round the block, facing in, its picture the centre probe's (1024 × 512, half floats): an emissive mesh, which Solari samples as a light, triangle by triangle. A ray that leaves the block meets it and takes the light that came in from its own direction, where lamps on the faces gave each face's mean light. 40 m keeps the dome within the 50 m Solari's light cache traces to: a ray of the cache that met nothing would take the camera's environment map besides the dome.
2. **The rays see the block alone.** A mesh is shown when its box reaches into the block; four buildings did here, up to 34 m high, and were traced whole: they hid the light the probes saw beyond the block (Cycles' light) and lit the floor from Solari's instead. Each mesh that reaches out of the block is now cut at its surface (`clip.rs`, Sutherland–Hodgman, its attributes interpolated along the cuts); drawn at T3 with the cut, the picture is the same to 55.9 dB.
3. **What is left.** In radiance (both drawn plain), the traced floor is 0.895 of T3's under the same light. Not the denoiser: without it the floor is darker still (0.81, grain darkening through AgX's curve), and none of it is black — not a normal map's normals turned from the eye. The street's materials are all alpha-masked and Solari traces them opaque, but their colour textures are opaque throughout: the eye sees the same layer as the rays. The pixel's light is one ReSTIR sample weighed; the 10 % is to be found inside Solari's realtime estimator. A first try against Solari's reference path tracer (`OSAGE_PATHTRACE=1`) averaged nothing: the walker's camera, written every frame, restarted its average, and one sample a pixel through AgX's curve and 8 bits came out at 0.55. With the camera written only when the eyes move, the reference averaged about 1 200 samples a pixel (6 s at 4.9 ms): on the traced floor, in radiance (all drawn plain), the reference is 0.94–0.97 of T3 and the realtime light 0.88–0.89 — the reference 1.07–1.10 of the realtime. Of the 11 %, about 8 are Solari's realtime estimator (ReSTIR and its light cache) falling short of its own path tracer under the same dome; 4 to 5 are the model: Solari's surfaces lit ray by ray against T3's environment light, the dome seen from the cube's centre, the block's own shadows.

Solari's realtime settings one by one, against the same reference (`OSAGE_SOLARI`, all drawn plain, none clipped; the traced floor against T3): by default 0.890, its reference 0.975; the confidence cap at 1, 0.886; four times the direct light's samples, 0.896; eight bounces, 0.889; the light cache's cells at 5 cm, 0.890; without ReSTIR, 0.847. Unfiltered, the realtime light is 0.817: Solari's realtime path falls 16 % short of its path tracer, whatever its settings, and osage's denoiser brightens it by 9 % (a filter should keep the mean: to be looked into).

## A smoother walk (2026-11-19)

The maintainer walked the client on the laptop: "ça lagge un peu". Seven seconds of an offline film at T3 on the Iris Xe, from the scene camera down the five cubes with probes:

1. **The main thread stacked the probes.** Each time a probe came, the cube's probes were copied byte by byte into new texture arrays and sent whole to the GPU: 30 to 862 ms a time, the picture held. Now a cube's two arrays of five layers are made once, and each probe is written into its layer by the render world straight from its decoded light, 16 MiB a frame in bands of rows, shown once whole (the previous block drawn until the centre's is). The main thread spends 0.03–3 ms; sent whole, a probe's two layers had held the render thread 15–90 ms, several at once 0.66 s.
2. **Decoding gives way.** The fetching threads, and those they start, run at a lower priority than the frame's (Linux: a nice value of 10).
3. **The radiance's packing.** A probe's decoding spent more packing its radiance into 10 bits, on one thread, from a hundred megabytes of floats, than converting it from YUV: converted straight into its packing on the bands' threads, a probe decodes in 565 ms instead of 773 (AV1 colour 314 ms, distance 80, sky 52).

Measured against the former client, alternately, while other programs loaded the laptop's ten cores (load 6 to 16): at a moderate load 148 frames in the 7 s instead of 99, a median of 25 ms instead of 44; past a load of 10 both stutter alike. All 25 probes now come in time during the walk (22 before).

**What a whole street would ask (D-139).** With only five cubes baked, the walk hid how much the rule of D-137 asks: simulated down the whole street baked, every cube within reach in every direction, five probes each, comes to 9.3 probes a second walking (45 at the start) and 31 running — fifteen and more of the laptop's cores to decode. Sent along the walker's way, centres only, the four cubes beside its own for turns, its own cube whole when it stands: 1.4 probes a second walking, 3.7 running, nine standing. On the laptop's film: nine probes decoded instead of twenty-five, 180 frames in the 7 s, a median of 17 ms (load 3.8).

**Lighter probes (D-140).** The five probes of the scene camera's cube packed again, the client's picture against Cycles:

| Probes | Picture | Colour a probe | A probe decoded |
|---|---|---|---|
| as baked, quality 10 | 28.3 dB | 729 KiB | 546 ms |
| packed again, 10 | 28.3 dB | 697 KiB | 590 ms |
| 18 | 28.1 dB | 366 KiB | 482 ms |
| 26 | 28.0 dB | 237 KiB | 408 ms |
| 34 | 27.6 dB | 137 KiB | 406 ms |
| half the size, 10 | 26.9 dB | 261 KiB | 169 ms |

Packing again costs nothing; the quality past 18–26 buys bytes, not decoding (which goes with the pixels); half the size decodes 3.2 times faster, blurred. Now packed at 22 (28.0 dB standing), the halves sent to a runner: running down the five cubes on the laptop, each cube's centre came in time, the frames' median 21 ms.

Two remedies tried and set aside. **The GPU's AV1 decoder** (VA-API on the Iris Xe) decodes the colour (Main profile, 10-bit 4:2:0) but neither the distance (12-bit, Professional profile) nor the sky's mask (monochrome): 0.41 s for the colour instead of 0.67 with dav1d on four threads, a third of a probe's decoding at best. **A cube's five probes as one video** (the centre's frame alone, the four others predicted from it): 3.5–3.9 MB a cube instead of 4.3–4.6 (−16 %), but no less to decode — one thread, the four probes after the centre: 1.56 s of CPU as pictures of their own, 1.79 s joined after the centre's (re-decoded: the colour 1.04 → 1.02 s, the distance 0.32 → 0.66 s at quality 1, the mask 0.20 → 0.12 s). A first measure through five ffmpeg processes had made it look three times faster.

**Where the walker looks (D-141).** A view reads 15–26 % of each probe's panorama, its middle third 5–11 %. Each probe inside a cube is now packed as 16 × 8 tiles of 256 pixels, every one a key frame carrying its sequence header, so that any of them decodes alone (SVT-AV1 with `keyint=1`: ffmpeg's `-g 1` does not reach it; libaom with `-g 1` and without looking ahead, which failed to allocate its noise filter's buffers and would have filtered a tile with the next ones); the half (D-140) is packed beside it. The client draws the half at once, doubled into the probe's layer, then decodes the tiles four at a time from where the walker looks outwards — running, those within 35° (the middle of the picture: its corners are 55° off at 70° of field); walking, within 70°; standing, all — and writes each over the half, after it.

| Measure | Tiles | Whole probe |
|---|---|---|
| Radiance against the probe as baked, quality 22 | 5.1 % off, 235 KiB | 4.1 % off, 305 KiB |
| … tiles at quality 16 | 4.3 % off, 323 KiB | |
| Decoding, one probe (laptop, load 6–7) | 128 tiles: 1.07 s on one thread, 0.37 s on four; a tile 5 ms in the sky, about 10 in the street | 0.55–0.64 s on four threads; its half 0.16–0.19 s |
| Standing, against Cycles | every tile: 27.9 dB | 28.0 dB; the halves alone, doubled: 26.6 dB |

Tiles that are key frames each alone need about six levels of quality more than a probe's single picture to come back as close (why, not yet looked into). The tiles bring the halves' 26.6 dB back to 27.9, mostly on the façades and the sky. Filmed against D-140 on the laptop, alternately, two rounds (load 3.6–11): walking 7 s, 155/156 and 123/211 frames, medians 20.5/23.6 and 16.8/17.1 ms, 25/30 and 30/31 over 50 ms; running 2.5 s, medians 20.5/25.5 and 22.4/21.4 ms, 11/9 and 11/8 over 50 ms (D-140/D-141). No smoother, within the load's noise; running, D-141 decoded four batches of four tiles a cube, the middle of the view sharp where D-140 showed the halves; 9.8 MB down the five cubes against 7.8. Standing before setting out, every tile of the cube is decoded (640 in about two seconds).

## The light ahead made ready (D-142, 2026-11-22)

Asked whether the light of the cubes ahead should not be loaded before the walker gets there, the films' logs were read frame by frame first.

**What the logs of 2026-11-21 said.** In every film, D-140's as D-141's, the main thread's systems fell silent for 1 to 7 s right after the eyes entered a cube: the picture stopped. That is when the client made the cube's two texture arrays (4096 × 2048 × 5 layers: 160 MiB each, the Iris Xe's memory being the laptop's) and let go those of the cube left, with 0.5 GB of memory free. The films' statistics had not shown it, for three flaws of the measure:

1. The system counting a film's frames was not ordered with the walker's: in the frames where it ran between the keys read (no stride) and the film's stride set, it saw the walker standing and left the frame out, about one in three. Films of 7 s walked 10, out of the five baked cubes.
2. A frame's time was the walk's clock's, which stops at 250 ms a frame.
3. Without a window nothing paces the frames. The processor sent them every 16 ms, the GPU drew a cube's five probes slower, and about once a second the driver held the processor 0.47 s (always in the render graph's run; gone with one probe a cube, or when each frame waits for the GPU). A frame now waits for the GPU to have drawn the one two before it, as a window's presentation would.

**What was changed.** The arrays are made once, six layers lent to probes: the five of the cube the eyes are in, and the centres of the first two cubes on the walker's way, their light written before it gets there (the half doubled, then the tiles where it looks). A cube left keeps its layers until they are needed. And the light is fetched by what presses: `needed_light` tells what is on the walker's way from what is beside it, and the light threads, told again four times a second, take the first, then the tiles where the walker looks, then the second.

**Measured, 2026-11-22**, 16 GB free, against the client before given the same mended measure, three films each, alternately (load 7–12):

| | Before | After |
|---|---|---|
| Walking: a cube's light shown after the eyes entered it | 0.21–0.49 s | 0 ms (12 of 12) |
| Running: the same | 0.22–0.93 s; 2 cubes of 4 never | 0 ms; 0.17–0.49 s for the cube whose corner the way cuts |
| Walking: frames in 7 s, median, longest | 183–230, 20–27 ms, 209–332 ms | 216–251, 21–27 ms, 150–248 ms |
| Walking: frames over 50 ms | 55–57 | 8–27 |
| Running: longest frame | 283–486 ms | 63–184 ms |

The cube whose corner the way cuts is entered 0.2 s after the one before: setting out from a cube whose five probes are drawn, every layer is lent, and its centre's light can only be written once the first cube is left. Online, through the server's two lists, the same: the three baked cubes of the walk shown at once. Standing, 28.0 dB against Cycles, as before (and with D-140's probes, their light whole written over the half).

**What is not shown.** With 16 GB free the client before did not stop for seconds either: the arrays made for each cube are the cause by their timing in the logs of the 21st, not by a measure with and without under the same shortage of memory.

**The GPU's part.** Paced, a frame standing among five probes takes 53 ms on the Iris Xe at 1280 × 720, and 36 ms drawn from the surface tiles: the 12–25 ms told so far were the processor's. The faces' march through five probes a pixel is the next thing to lighten.

## One probe a cube, nine a block (a trial, 2026-11-22)

The maintainer's question: instead of five probes inside the walker's cube, the centre probe of each of the block's nine cubes, each baked with its own block transparent — one probe a cube, shared by the nine blocks it is in: a fifth to bake, keep and send. The centres of the six cubes missing round the scene camera's baked on the L4 (`OSAGE_BAKE_PROBES=1`, 2 min a probe); `paris --block-probes` draws the block from the nine. A neighbour's probe sees through its own block, so through what stands just outside this one on its side: of what the probes met along the eye's ray, the nearest is what the eye sees, and only the probes that met it tell.

| From the scene camera, against Cycles | One centre | Nine centres | The cube's five |
|---|---|---|---|
| Picture | 26.9 dB | 27.6 dB | 28.0 dB |
| Left façade, near the block | 22.7 dB | 26.1 dB | 28.8 dB |
| Right façade | 28.0 dB | 27.6 dB | 28.3 dB |
| Street beyond | 29.9 dB | 30.8 dB | 30.8 dB |
| A frame standing, Iris Xe | 23 ms | 95 ms | 57 ms |

The eight neighbours bring much (3.4 dB on the near façade over the centre alone), nothing is seen through or twice; what is left is blur near the block: the probe nearest the eye stands 0.7 m off and the next 2.3 m, where the cube's five all stand within 1.5 m. Keeping only the probes whose look is most like the eye's changed nothing. The march through nine probes a pixel costs 95 ms a frame: it should keep three or four. How much of the picture no probe saw was not measured.

## The street's meshes round the block, lit by the probes (a trial, 2026-11-22)

The maintainer's next thought: draw the geometry of the eight blocks round the walker's too, and mix it with the probes. First step, with the probes as they are: `paris --ring` draws the street's meshes out to 13.5 m from the block's middle with the light the cube's five probes saw of each point — no march, the mesh says where the point is, and a probe saw it if what it saw that way lies as far —, the faces standing on the ring's outer surface. (The meshes are the model's own, not simplified yet; those drawn lit in the block reach out of it uncut, the same surface with the probes' light going before them.)

| From the scene camera, against Cycles | The faces' march (five probes) | The ring |
|---|---|---|
| Picture | 28.0 dB | 28.0 dB |
| Floor | 30.2 dB | 31.4 dB |
| Left façade, right façade | 28.8, 28.3 dB | 28.2, 28.3 dB |
| Seen by no probe | not measured | 0.02 % of the picture |
| A frame standing, Iris Xe | 57 ms | 33 ms |

Edges come out where the model has them (a bollard against a wall, a kerb); the walls stay as blurred as the probes are: their sharpness would come from the model's own textures, the probes giving the light alone — which asks for probes baked with the light apart from the surfaces' colour.

**Second step: the meshes' own colours, the probes' light.** For each probe, what colour each surface it saw is, apart from its light (`cube-probes.py` with `OSAGE_ALBEDO=1`: every material shining its base colour, one sample, 23 s a probe on the laptop's processor; `light-pack --albedo`: 450 KiB a probe). The ring's material then draws a point as the light the probe saw of it, divided by the colour the probe saw there and multiplied by the mesh's own base colour at that point: the model's textures give the detail, the probe the light.

| From the scene camera, against Cycles | The probes' colour | The meshes' own colour |
|---|---|---|
| Picture | 28.0 dB | 28.0 dB |
| Floor | 30.3 dB | 31.4 dB |
| Left façade, right façade | 28.1, 28.3 dB | 28.4, 28.6 dB |
| A frame standing, Iris Xe | 30 ms | 31 ms |

To the eye the walls, the wood and the stone come out with their grain, where the probes blurred them; the measure moves little (what is left: the surfaces' relief, which Cycles shades and a base colour does not carry).

**Third step: the ring's meshes simplified.** With what the engine has (`osage-props`' vertex clustering: the vertices of a cell merged at their mean), `--ring-cell METRES`. The street has 2.83 million triangles.

| Cells | Triangles drawn | With the probes' colour | With the meshes' own colour |
|---|---|---|---|
| none | 2 829 226 | 28.0 dB | 28.0 dB |
| 0.1 m | 857 546 | 27.7 dB | 26.2 dB |
| 0.25 m | 544 405 | 26.8 dB | 25.5 dB |
| 0.5 m | 321 546 | 25.4 dB | 24.5 dB |

Clustering is the wrong tool here: it moves the façades' edges (a window's frame, a sill) by a cell, and it averages texture coordinates across seams, so that the meshes' own textures land wrong. A frame's time barely moves (39 to 32 ms): the triangles are not what costs, so simplifying would save what is sent, not what is drawn. What it asks for is a simplification that keeps edges and seams, done once when the street is made.

### The ring with nine centres, each with its surfaces' colours

The 93 probes packed in the street (five a cube along the corridor, one a cube elsewhere) now each have their `albedo.ivf`, baked on the laptop in about 50 minutes. Same view, T3, Iris Xe:

| | Picture | Left façade | Right façade | Floor | Frame |
|---|---|---|---|---|---|
| Five probes of the cube, no ring | 27.9 dB | 28.8 dB | 28.1 dB | 29.5 dB | 57 ms |
| Nine centres, no ring | 27.6 dB | 26.3 dB | 27.3 dB | 29.4 dB | 95 ms |
| Ring, five probes | 28.0 dB | 28.6 dB | 28.5 dB | 30.6 dB | 33 ms |
| Ring, nine centres | 27.7 dB | 26.3 dB | 28.3 dB | 30.5 dB | 54 ms |

The left façade's 2 dB are lost by the nine centres themselves, ring or not, and the colours of all nine change nothing to it (26.1 with one centre's colours, 26.3 with nine). Why is not shown: the centres stand 3 to 4 m apart where the cube's five stand within a metre, so what they tell of one point of a wall differs more and is blended.

A walk of 7 s through four cubes: with the ring and five probes, a median frame of 47 ms (113 frames, one over 250 ms); with the ring and nine centres, 66 ms (93 frames, two over 250 ms). Every cube entered was shown within 5 ms.

### What the street's bake taught (2026-11-22)

Two faults showed once the bake left the corridor, and the walk with it.

- **Where a centre stands.** A probe stood over its cube's middle, on the first surface under the cube's top. The middle of a walked cube is sometimes inside a wall (cube 2,0,-8: the probe saw nothing and the batch stopped) or over a table or an awning: 22 of the 73 centres baked stood more than 0.5 m from where a walker stands, 11 of them 0.6 to 2.4 m too high. `tools/bistro/probe-spots.py` now gives each cube the cell of the walk's grid nearest its middle, and `cube-probes.py` takes it (`X,Y,Z:x,y,z`).
- **Tiles that did not decode alone.** The 64 centres packed on the bake machine had one key frame in their colours and 127 frames leaning on it: SVT-AV1's `keyint=1` means every frame there (ffmpeg 9, Arch) and the first alone on Ubuntu's. A walk out of the corridor logged `AV1 data refused` for each. The tiles' colours go through libaom now, as their distances and masks did, which decoded alone from both machines: 274 KiB a probe against 305, as close (4.7 % against 4.8 %), 13 s a probe. `light-pack` decodes the last tile alone after packing, and fails if it cannot.

**Which probe lights a wall.** The ring's left façade loses 2 dB with nine centres against the cube's five. Taking for each point the one centre that saw it from nearest the eye's direction (`OSAGE_RING_PICK=best`) gives 27.2 dB there against 26.3 blended, and 27.8 against 28.3 on the right one: 27.8 dB over the picture against 27.7. The cube's centre alone with the ring gives 27.3 on the left façade (26.9 over the picture, 20 ms a frame). So blending centres 3 m apart costs about 1 dB on that wall, and the rest of the gap to five probes (28.6) is what four more probes within a metre of the eye add; why they add it is not shown (they may average the bake's noise). Left as a trial, the blend unchanged.

### One black frame on entering a cube (2026-11-22)

Walking, the maintainer saw black flash at some cube changes. Films keep one picture in three and the black lasts one frame: with every frame kept (`OSAGE_FILM_EVERY=1`), 2 or 3 frames of each film were more than 10 % black, with the ring or without it, at T2 as at T3. The cause: on entering a cube the client makes the block's environment light anew and gave it to the camera in the frame its two images were made, before the GPU had them; the block's meshes were drawn without any light for that frame. It now keeps the new light two frames before giving it, the one before staying meanwhile: no black frame left in three films (105, 120 and 153 frames). Why the ring went black in that frame as well is not explained.

## Next

1. The whole street baked, at 1024–1536 px and more samples: about 2 000 tiles, a cloud GPU's work (hours on the laptop's processor for 384 px).
2. Tiles of neighbouring planes as one video (they differ by a few metres of parallax), for the stream.
3. Probes between tiles' centres where walls stand near a face, to see behind depth edges.
4. Solari's realtime path 16 % short of its own path tracer under the dome, whatever its settings: to report to Bevy's Solari, with the measures. And osage's denoiser, which brightens the traced light by 9 %.
