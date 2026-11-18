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

## Next

1. The probes over 200° in the bake; more samples and pixels where the street is seen from close (a cloud GPU).
2. In the client (`osage-paris-native`), the same march in the faces' shader, the block lit by the probes' light rather than by the sky's photograph alone.
3. Probes between tiles' centres where walls stand near a face, to see behind depth edges.
