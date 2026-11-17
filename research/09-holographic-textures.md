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

## Next

1. **Draw it:** a hologram component in osage that picks the views nearest the camera's direction and blends them, with each view's depth to correct the parallax between them; the facade drawn beside its model in the kart world: memory, frame time, look.
2. **GPU form:** compare decoding into a texture array, a cache of nearby views decoded as the camera moves, and a factorised form (a few basis images per texel and weights per direction).
3. **Light:** bake albedo and normals per view as well, and light the hologram at run time.
