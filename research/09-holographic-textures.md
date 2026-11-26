# 09 — Holographic textures: first experiment

Date: **2026-11-17**. Issue [#7](https://github.com/smichea/osage/issues/7). The maintainer's idea (about 25 years old): texture 3D objects not with flat images but with *holograms* — how each point looks from every viewing angle — compressed as video is, the step from one view to the next being a step of viewing angle where a film's is a step of time.

## Question

Is a field of views of a detailed object much cheaper to store as a "video along the viewing angle" than as independent images, at the same quality? And does the order of the views (each a small turn from the last) matter?

## Method

- **Object:** Poly Haven's *modular urban apartments facade* (CC0, 118 000 triangles, 1k textures; the model and textures weigh 16 MB), its assembled wall framed (27 × 17 m, a sphere of 16.2 m).
- **Views:** `hologram-bake` (crate `osage-hologram`) photographs it off screen, orthographic — the framing the same in every view — under one fixed sun, on grey, 512 × 512.
  - *Coarse grid:* azimuth −70° to 70° by 10°, elevation −10° to 50° by 7.5°: 135 views.
  - *Fine grid:* azimuth −20° to 20° by 2.5°, elevation 0° to 15° by 3.75°: 85 views.
- **Compression** (`tools/hologram-measure.py`): every view alone as JPEG and WebP; all views as one video in path order (along a row of azimuths, back along the next) with H.265 (x265, slow) and AV1 (SVT-AV1); the same videos with the views shuffled. Quality: PSNR and SSIM over all views against the originals (ffmpeg, YUV 4:4:4).

## Results

Bytes per view, at about the same quality.

| PSNR | grid | WebP, each view | video along the angle | gain | video, views shuffled |
|---|---|---|---|---|---|
| ~44.6 dB | coarse (10°) | 21.0 KiB | 17.6 KiB (x265) | ×1.2 | 19.8 KiB |
| ~44.5 dB | fine (2.5°) | 20.4 KiB | 13.8 KiB (x265) | ×1.5 | 15.6 KiB |
| ~40.4 dB | coarse (10°) | 7.4 KiB | 3.9 KiB (x265) | ×1.9 | 5.0 KiB |
| ~41.5 dB | fine (2.5°) | 9.3 KiB (41.8 dB) | 1.6 KiB (AV1) | ×5.8 | 2.5 KiB |
| ~40.3 dB | fine (2.5°) | 6.8 KiB | 0.7 KiB (AV1) | ×10 | 1.1 KiB |

Full tables: `results.tsv` written by the tool beside the views.

## What it shows

1. **The idea holds, and gains with angular density.** With views 10° apart, consecutive views differ by large parallax and the codec gains little over flat images (×1.2–1.9). With views 2.5° apart, each further view costs 0.7–2 KiB instead of 7–9 KiB: ×5–10 at good quality. The finer the field of views, the cheaper each view — the opposite of flat images, whose cost grows with every view.
2. **The order of angles matters:** the same views in random order cost about 1.3–1.6 times more. The codec's motion compensation is finding the parallax from one angle to the next, as the idea supposes.
3. **Against the model:** at ~41 dB, the coarse field (135 views, ±70° × −10–50°) weighs about 0.5 MB as video, the model 16 MB — but these views are 512 px (about 6 cm a pixel on this wall) and lit by one fixed sun.

## Limits of this experiment

- Storage only. A GPU cannot sample a video: drawn, the views must be decoded — into a texture array (135 × 512² × 4 B = 141 MB raw, about 35 MB in BC7) or a cache of the views near the camera — or the field kept in a form the GPU reads directly (a low-rank factorisation per texel, as light field mapping does).
- The light is baked: one sun. Relighting (time of day, rain, night) needs more than colour per view.
- Views of a wall from the street; an object seen from all around needs the whole sphere (octahedral grids).
- PSNR of the views themselves; what matters in the end is the picture drawn between views, from a camera that is never exactly at one.

## Second experiment: drawn (2026-11-17)

- **How:** `hologram-view` draws the facade's model and, 62 m to its left, its hologram, under the same sun, filmed by a camera turning from 60° to one side to 60° to the other. The hologram is one rectangle in the facade's plane and its 135 views (coarse grid, 256 px, baked on magenta) in one texture array. For each pixel, `hologram.wesl` takes the direction to the eye, the four views of the grid around it, finds the point in each as that view's orthographic camera saw it, and blends the four by nearness of angle; magenta is the ground — at the object's edges, part of a pixel, its colour unmixed from it.
- **What it shows:** from every angle of the turn the hologram reads as the facade — its windows, frames and cornices in relief from the side — on a single rectangle instead of 118 000 triangles. GPU memory: 135 × 256² × 4 B = 35 MB uncompressed, against the model's mesh and 1k textures.
- **Its faults:** between views 10° apart, parts in relief (cornices, frames) blur or double, the parallax of the rectangle and the real surface not corrected; it casts no shadow on the ground; faint magenta at some edges; the light is the one it was baked under.

## Third experiment: the parallax corrected with depth (2026-11-17)

- **How:** `hologram-bake` also writes each view's depth — a twin of the model on its own layer, drawn by a material that writes how far each point stands towards the camera, in 16 bits over red and green (`depth.wesl`), saved byte for byte. The shader then finds, in each of the four views, the point where the eye's ray meets the object: its depth there read, the point on the ray at that depth taken, four times over, before the colour is sampled. `HOLOGRAM_FLAT=1` draws without it, for comparison.
- **What it shows:** close up (14 m, from between views: 35° to the side, 22° up), the corrected hologram keeps its panels' edges where they are and loses most of the magenta fringe; the flat one smears edges across the gaps. But close up both are blurred: 256 px over a 32 m sphere is 12 cm a pixel, the model's 1k textures far finer. Its cost: a second texture array as large as the first (35 MB at 256 px).
- **So:** the hologram is a far object. Near, it needs views of 1024 px or more (16× the memory, which the video compression of the first experiment would have to bring back down, decoded near the camera only), or to give way to the model.

## Fourth experiment: a street (2026-11-17)

- **How:** `hologram-street` lays 40 facades, 20 a side, 30 m apart, 18 m from the road's middle, and drives a camera down the road at 2 m, 25 m/s, off screen at 1280 × 720, on the reference laptop (Intel Iris Xe): every facade its model, every facade its hologram, or the model within 60 m and the hologram beyond. The holograms share one stack of views and depths (70 MB at 256 px).
- **Measured** (400 frames after 30 to warm up):

| street | mean | 95th percentile |
|---|---|---|
| 40 models (4.7 M triangles) | 63.6 ms | 524 ms |
| 40 holograms | **9.2 ms** | 10.8 ms |
| models within 60 m, holograms beyond | 39.5 ms | 258 ms |

- **What it shows:** drawn from its views, a street of facades costs a seventh of its models' time, and its time does not depend on how detailed the model was. Seen from the road, the far facades read alike; near ones, the model's relief and shadows are better.
- **To read it fairly:** each model is Poly Haven's whole kit (spare parts and cornices beside the wall), the hologram only the wall it framed: the models' cost is overstated, perhaps twice. The mixed street's spikes are the models' (see the sixth experiment), not the holograms'. The holograms carry the light they were baked in (one sun, from the front): a facade the world's sun lights otherwise is wrong — across the road, both look dark in the picture, for reasons not yet looked into; and they cast no shadow. *(Corrected the same day: an earlier line said they looked bright.)*

## Fifth experiment: lit where drawn (2026-11-17)

- **How:** `hologram-bake` also writes each view's colour without light (the model's materials made unlit, no tone mapping) and its normals (`normal.wesl`, in the object's frame). With `lit`, the hologram is drawn from those: the four views' colours as the surface's own, their normals blended and turned into the world's frame, through Bevy's lighting — the world's sun, lamps and shadows, as any surface.
- **Measured** on the same street: 10.1 ms a frame (p95 11.9), against 9.2 ms baked-lit and 63.6 ms as models. Memory: a third texture array (35 MB at 256 px).
- **What it shows:** lit where drawn, the holograms light as their models do — the side facing the sun bright, the side across the road in its own shade, the relief shaded by the sun's angle. Night, rain and a moving sun then need nothing more from them. Faults remaining: a faint magenta at edges in the unlit views; no shadow cast; normals from the geometry only (the models' normal maps not baked).

## Sixth experiment: the switch (2026-11-17)

- **How:** in the mixed street the model no longer is hidden and shown: it is seen up to 60 m and fades out over its last tenth by Bevy's visibility range (dithered, the model kept prepared), while the hologram (lit) fades in over the same 54–60 m by its own transparency (`fade_in`).
- **Measured:** 41.5 ms a frame (p95 294 ms), against 39.5 ms (p95 258) when the models were hidden and shown — the switch was not what cost.
- **What it shows:** the two meet without a jump: at the edge of the range the model's dither and the hologram's fade cross (a fine dither pattern shows in a still frame; temporal anti-aliasing would smooth it in a game). The stalls seen so far were the measurement's: without a window to pace it, the processor queued frames faster than the GPU drew the models, then waited for many at once — every 12–13 frames, 0.5–1.3 s. Kept to one frame on the GPU at a time (as the kart's bench does), they go.

## The street measured again, one frame at a time (2026-11-17)

| street (lit holograms) | mean | 95th percentile |
|---|---|---|
| 40 models | 59.5 ms | 125 ms |
| 40 holograms | **8.5 ms** | 9.9 ms |
| models within 60 m, holograms beyond, crossfaded | 37.7 ms | 101 ms |

The slowest frames are now the drive's first (most of the street in view, about 125 ms each, steadily), not stalls. Of the models' 60 ms, about 35 are the sun's shadow maps redrawing 4.7 million triangles (26 ms without them): a hologram, which casts no shadow yet, also spares those — a shadow of its own, from its depth, would cost a fraction of a model's.

## Seventh experiment: a forest (2026-11-17)

- **How:** `hologram-bake` scatters a plot of forest (`HOLOGRAM_FOREST`): 120 trees of Poly Haven's most detailed (island_tree_03, tree_small_02, island_tree_01, CC0: 3.7–4.8 million faces each), about 500 million faces, 60 m wide and 30 m deep, each tree turned and sized by a seeded hash (`osage_hologram::forest`, the same at every call). Its views only from the angles a circuit sees it from (D-133): 21 azimuths from −40° to 40° by 4°, 5 elevations from 0° to 12° by 3° — 105 views of 1024 × 256 (`HOLOGRAM_ASPECT`: a wide, low framing). Two faults on the way: drawn by four cameras at once (colour, depth, albedo, normals) with sun shadows, such a scene lost the integrated GPU; now each camera draws alone in turn, without shadow maps (13 min 36 s for the plot). And foliage on magenta came out pink and speckled: thousands of leaf edges mix with the ground; baked on black (`HOLOGRAM_KEY=black`), the object is now told where it is by the depth pictures (`masked`).
- **Measured** (`hologram-forest`, a camera driving past 60 m away at 2 m, one frame on the GPU at a time, no shadow maps for either): the trees **372 ms** a frame (p95 388), the hologram drawn on a box over the plot **7.6 ms** (p95 8.9) — 49 times faster, and its time does not depend on the forest's 500 million faces.
- **What it shows:** close up, the hologram keeps the trees' shapes, trunks and gaps; its colours are a little lighter and the foliage's edges speckled dark (a hard mask from depth). Memory: 105 views × 1024 × 256 × 4 B × 3 = 330 MB.

- **Tried in the kart world, not kept:** the plot placed in rows along the forest circuit in place of its tree cards. The circuit is hilly: of 82 places, 22 too steep for a plot baked on flat ground, 30 too near the road where the circuit comes back, 23 in its clearings — 5 plots stood, and the woods were gone. A generic plot does not fit a real forest round a circuit: the forest itself must be baked where it stands, chunk by chunk, on its own ground, from the angles the track sees each chunk from.

## Eighth experiment: the circuit's own woods, baked where they stand (issue #8)

- **How:** with `OSAGE_WOODS_CHUNKS=DIR`, building a circuit writes its woods (the trees `roadside::woods` places on the real ground) cut into squares of 60 m, each a scene (`osage_hologram::Scene`) — its trees in its own frame, turned to face the nearest point of the track, and the grid of directions from which the track, seen from a kart's height, sees it. The forest circuit: 26 330 trees in 181 chunks, each seen from 100–150 directions. `hologram-bake` reads a scene (`HOLOGRAM_SCENE`), fits its views to it (`hologram.json`), and the game draws every installed chunk as its hologram in place of its trees' cards (`woods_holograms`).
- **Found on the way:** the pines' and firs' needles are cut out of cards by their texture's alpha; the depth and normal passes drew whole cards, so the hologram showed dark blots. They now cut as the model does.
- **Measured:** a chunk of 208 pines and firs (about 400 million faces) bakes in 6 min 30 s on the laptop — 181 chunks would take about 20 hours: a bake for a cloud GPU.
- **What it shows, not yet good:** 44 m from the track, the chunk drawn from 512-pixel views (about 20 cm a view pixel over its 100 m) reads as dark, scattered blots where the cards show crisp firs — too coarse for fine needles at that distance, and darker lit than the cards. At the resolution it would need (2048 px), a chunk's three arrays are about 2 GB: 181 of them cannot all be on the GPU. The form a hologram takes on the GPU — views decoded near the camera only, or a compact form — is what decides whether a whole forest can be one.

## Ninth experiment: the GPU's share (issue #8)

- **Halved:** a hologram's views, loaded as ordinary images, were each uploaded to the GPU as well as in its stacked arrays. Kept on the CPU (they are only stacked from), the forest plot's holograms take half the GPU memory, 330 MB less.
- **A window of views:** a hologram may keep on the GPU only a window of its grid (`window`: columns × rows) around the direction the camera sees it from; when the views that direction needs leave it, the window slides and its arrays are made again from the views on the CPU. A view outside it is taken as the nearest inside, its point found by depth. The forest plot with a window of 5 × 3 views of its 105: drawn alike (the far side, seen at a slant, from a view at the window's edge, its point found by depth), 8.5 ms a frame (7.8 with every view), and a seventh of the GPU memory — 47 MB instead of 330.

## Tenth experiment: streamed (2026-11-18, D-134)

A world's holograms are kept by its server and sent to each viewer as it needs them (`osage-hologram-stream`): the grid cut into blocks of 3 × 4 views, each block of each series one short AV1 video along the angle that starts with a whole picture and decodes alone; the viewer tells the server where it is, in the hologram's frame, and how fast it goes, and gets the blocks it can see the hologram from within two seconds.

- **Series:** colours in 10-bit 4:2:0 (SVT-AV1), their ground filled first with the object's own colours spread outwards, so that halving the chroma bleeds nothing foreign into the edges; normals in 10-bit 4:4:4 RGB (libaom); depth in 12-bit grey, lossy, its ground filled the same way; the outline apart, 8-bit grey at the finest quantizer, so that no texel changes side. A first try kept depth nearly lossless with a gap between "nothing" and the nearest depth: the forest's depth then shrank only ×1.2.
- **Forest plot** (105 views of 1024 × 256 px, 4° × 3° apart): 102 MB of PNG become 20.8 MB of blocks (×4.9): colour without light ×10 (28 dB on the needles, the hardest case), normals ×2.7, depth ×12, outline exact. Turned round from −40° to 40°, the camera was sent 51 of the 105 views, 7.8 of the 20.8 MB; the hologram was drawn 4.5 s after the stream opened.
- **Facade** (135 views of 256 px): its colours come back at 26 dB however fine the quantizer: its views were baked on magenta, and its glass, transparent in colour but solid in depth, leaves magenta inside the object, which the halved chroma spreads. Views to be streamed are baked on a transparent ground.
- **Decoding**, one thread of the laptop, a view of 1024 × 256 px: colour 15 ms, depth 13 ms, outline 14 ms, normals 29 ms — rav1d with its assembly (nasm), the conversion to RGBA a row at a time. Without the assembly and converting texel by texel, colour took 61–92 ms.

## Eleventh experiment: the woods' chunk found along the ray (2026-11-26, issue #8)

- **Why the chunk drew blots:** not its resolution. Rebaked at 2048 px (100 views, 4 min 10 s on the laptop, 316 MB of pictures), it drew the same few skinny trees as at 512. The shader found each pixel's point from where the eye's ray crosses the stand-in, then refined it with the views' depth. A chunk of saplings is 60 m deep and mostly empty, so at the crossing most views see nothing and the pixel was dropped, even where the ray meets a tree in front of or behind it.
- **The change (`hologram.wesl`):** for a hologram told by its depth pictures (baked on black, `masked`), the ray is now marched through the photographed sphere in the nearest view: 24 steps, then 5 halvings, until it passes behind the surface that view saw. The point found is sampled in the four views around the eye's direction, and any view that sees it gives its colour. Where the march finds nothing, or for a hologram baked on magenta (a facade), the old way stays: each view's point is settled from the stand-in.
- **What it shows:** 44 m from the forest circuit's track, the chunk now stands as its 208 trees, with trunks, branches and parallax, on their own ground. The cards beside it are fuller and lighter. The street of 40 facades draws as before.
- **Measured** (load 7–8, 1920 × 1080, T2, runs alternated, two each): the chunk as its cards 24.1 ms a frame; as its hologram marched in 48 steps 34–37 ms, in 24 steps 29.1–29.6 ms, which looks the same. 24 steps kept. One chunk fills most of the screen here; the forest's other 180 are not baked yet, so a whole circuit's cost is still to be measured.

## Twelfth experiment: the woods' chunk from real trees (2026-11-26, issue #8)

**Change.** Chunk -3,0 of the forest circuit (208 trees) baked from Poly Haven's whole `pine_tree_01` and `fir_tree_01`, needles modelled, 5 to 6 million faces a tree, in place of the saplings the cards are made from, each at the cards' size (15 to 20 m). About a billion faces: the laptop's Iris Xe lost its device drawing it in one frame, and once in two runs in groups of eight. `tools/hologram-groups.py` bakes it in sixteen groups with one framing and merges them view by view by depth: 100 views at 2048 px, 2 h 55 on the laptop, 584 MB.

**What it shows** (same camera as the eleventh, T2, 1920×1080): an undergrowth of trunks row behind row under modelled crowns, where the cards made a flat wall. Two faults found on the way: one of `fir_tree_01`'s trunks has no texture coordinates and was drawn white (osage-props now wraps coordinates round such a piece); and the window of five columns cut the chunk down a vertical line, the camera 44 m from it seeing it across the whole grid (the window now widens to the angle the sphere subtends).

**Measures** (load 5 to 6, shared laptop): 41.8 ms mean with the chunk covering most of the screen and all its views, against 35.4 ms with the fixed window cutting it; the eleventh measured 24.1 ms with cards and 29.1 to 29.6 ms with the chunk baked from them (another load: to be measured again side by side).

**Open.** At 584 MB a chunk, the circuit's 181 chunks do not fit: 1024 px or compressed views before the whole forest. Measured: the views halved to 1024 px weigh 157 MB and draw in 34.8 ms, but the crowns turn to darker blots and the needles are lost; packed for streaming (`hologram-pack`, blocks of 3×4, CRF 30) at 2048 px they weigh 154 MB (×4.8: colours ×7.5 at 30 dB, normals only ×2.2, depth within 17556 of 65535 at worst), a client loading only the blocks near the track. Unpacked (`hologram-unpack`) and drawn in the kart at the same camera, the packed chunk is hard to tell from the baked one, its crowns a little smoother: 32.6 ms. Streamed into the kart (`OSAGE_HOLOGRAM_SERVER`), it draws at 32 ms once its blocks have come, but stalls frames for 6 to 27 s while they come: seen from 44 m it needs its whole grid, 100 views of 2048 px in three arrays, about 2.8 GB, and each remaking of the window allocates and uploads that again. Remaking it at most once a second, or once arrivals pause, does not help (the server never sends every view, so the window is never complete). Then a memory budget for a window: wider than 768 MB, its views are halved (each reduced texel the nearest of its square), reduced once each, a few a frame. The worst frame while blocks come falls from 13 s to 1.3 s (the arrays' upload; the main thread stays under 30 ms), 29.8 ms once they have come, the crowns a little softer, as at 1024 px. Next: the arrays updated layer by layer instead of remade, for that last second. The real trees are taller than the cards around them, which shows where a baked chunk meets cards.

## Ideas to explore (the maintainer's, 2026-11-17)

*Decided the same day:* the grandstands' crowd waits for its hologram version (issue #12). A 3D crowd of rigged people, each different and reacting to the race, was tried — 21.8 → 32.5 ms a frame at T2 on the laptop — and not kept: the people of a hologram do not react to the race, until the server that streams holograms (idea 3) renders them live.

1. **Transparent holograms: a view's pixel as a transfer, not a colour.** For a transparent or refracting object, a pixel of a view is not a colour but a small matrix that says how the light leaving it is made of the light entering the object's boundary at a few points: for a cube of clear glass, the colour at a point of its surface is the colour arriving where the ray leaves the cube. To bound reflections inside, only a finite number of escape points is kept (three, say), each with its weights. Baked once from the object's geometry (by tracing), drawn by looking up what the world shows at those escape points now — the environment seen through glass, water, ice, changing as the world does. Prior work to place it against: environment matting (Zongker et al. 1999; Chuang et al. 2000), light transport matrices, precomputed radiance transfer.
3. **The world as a stream: clients connect to the matrix.** If the world's servers run on powerful machines joined by very fast links, a client needs only a holographic stream — in effect a VR video — and a stream of inputs back to the server: what is drawn is the server's, at any detail, and the client is a screen and a controller. Questions: latency (a kart is steered at 60 Hz: the round trip must stay under a frame or two, or the client predicts locally and corrects, as osage's prediction does for karts), bandwidth per player, the cost of a server GPU per player, and how a hologram's views (a field of views around the camera, not one picture) let the client turn its head or move a little without waiting for the server.
4. **Any renderer makes the views.** What is drawn in the game is the hologram, not what produced it: its views may come from any renderer — a path tracer, a film renderer, or 3D Gaussian splatting, which renders scenes captured from real photographs (a street, a forest, a stadium filmed with a camera) with a realism no hand-made model reaches. Splats are costly and awkward to draw in a game; baked into a hologram's views, they cost what any hologram costs.
2. **Detail inside is free; make it real with AI.** A hologram's cost does not depend on how detailed its source is: the more faces inside, the more real, at the same frame time. So the views themselves can be made photographic: generated or refined by AI super-resolution or image-to-image models, as real as a filmed video — the views of one object then having to stay consistent from one angle to the next (video or multi-view models, which keep consistency across frames).

## Next

1. **GPU form:** compare decoding into a texture array, a cache of nearby views decoded as the camera moves, and a factorised form (a few basis images per texel and weights per direction) — three arrays (colour, depth, normals) at 256 px are 105 MB.
2. **Shadows:** the hologram casting its shadow, from its depth.
3. **Into the kart world:** buildings along a circuit as holograms beyond 60 m.
