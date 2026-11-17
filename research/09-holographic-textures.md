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

## Next

1. **Where it pays:** measure frame time and memory of the hologram against the model at the distances a race sees buildings (50–500 m), in the kart world — a street of facades, holograms beyond a distance, models near.
2. **GPU form:** compare decoding into a texture array, a cache of nearby views decoded as the camera moves, and a factorised form (a few basis images per texel and weights per direction).
3. **Light:** bake albedo and normals per view as well, and light the hologram at run time.
