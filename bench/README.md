# The bench

The oracle of the rendering programme ([design/03 §3](../design/03-rendering.md#3-oracles-how-done-is-decided)): the same frames on every machine, timed.

```sh
cargo run --release -p osage-kart-native -- --bench --tier T2
```

It plays 2400 frames (40 s of race) as fast as the machine can, and writes `report.json` and four screenshots to `bench/out/<tier>-<time>/` (not committed). The scene is drawn off screen at the tier's resolution and only shown in a window, so the display's scale factor and the window manager do not change what is measured. Options: `--frames N`, `--resolution WIDTHxHEIGHT`, `--out DIR`, and `--without TECHNIQUE` (sky, sky-light, shadows, ambient-occlusion, post, upscaling) to measure what one technique costs. `OSAGE_ASSETS=none` runs without the manifest's content.

- Close other programs; on a laptop, plug it in. Run `python3 tools/fetch-assets.py` first: the report says which content was there.
- Shaders are compiled before the first frame is drawn; the report gives that stall apart, as the worst warm-up frame.
- A change to rendering is proposed with the summary printed at the end, **before and after**, on the same machine.

## Budgets

| Tier | Resolution | Budget |
|---|---|---|
| T1 | 1280×720 | 33.3 ms |
| T2 | 1920×1080 | 16.7 ms |
| T3 | 2560×1440 | 16.7 ms |
| T4 | 2560×1440 | 33.3 ms |

## Reference machines

Add yours: one line here, and your reports in the pull requests you measure.

| Machine | GPU | Tiers | Role |
|---|---|---|---|
| ThinkPad E14 Gen 5 | Intel Iris Xe (Raptor Lake-P), Mesa | T1, T2 | the modest-hardware reference |
| Maintainer's desktop | ray-tracing GPU | T3, T4 | the high-end reference |

## Results

Latest results of the reference machines, one line per machine and tier.

| Date | Scene | Machine | Tier | Output (drawn at) | Mean | p95 | p99 | Over budget |
|---|---|---|---|---|---|---|---|---|
| 2026-11-10 | first scene: flat ground, box karts | ThinkPad E14 Gen 5 (Iris Xe) | T1 | 1280×720 | 11.2 ms (89 fps) | 13.9 ms | 16.6 ms | 0 % of frames |
| 2026-11-10 | first scene | ThinkPad E14 Gen 5 (Iris Xe) | T2 | 1920×1080 | 15.9 ms (63 fps) | 22.3 ms | 27.1 ms | 33 % |
| 2026-11-10 | hills, scenery, textured road, rounded karts, particles | ThinkPad E14 Gen 5 (Iris Xe), under load | T1 | 1280×720 | 13.4 ms (75 fps) | 16.8 ms | 19.6 ms | 0 % |
| 2026-11-10 | same | ThinkPad E14 Gen 5 (Iris Xe), under load | T2 | 1920×1080 (1632×918) | 16.9 ms (59 fps) | 23.9 ms | 31.4 ms | 41 % |
| 2026-11-10 | landmarks, sculpted scenery, new karts; bench `kart-lap/2` | ThinkPad E14 Gen 5 (Iris Xe), lighter load | T1 | 1280×720 | 11.2 ms (89 fps) | 13.5 ms | 14.7 ms | 0 % |
| 2026-11-10 | same | ThinkPad E14 Gen 5 (Iris Xe), lighter load | T2 | 1920×1080 (1632×918) | 14.0 ms (72 fps) | 16.9 ms | 19.3 ms | 6 % |

- "Under load": other programs were using the machine during these runs (load average 8 to 9 on 10 cores); a quiet run is expected to do better and will replace them.
- T2 holds its budget on average on the integrated GPU, not for every frame. Without drawing under the output size it took 23.2 ms in the same conditions against 18.0 ms with: the scene is bound by the number of pixels there.
- Bench version 2 (`kart-lap/2`) lets the karts overtake one another; in version 1 the followed kart ran into the karts ahead. Its frames differ, so compare runs of the same version only.
- The landmarks scene against the previous one, same binary bench (version 1), alternated under the same load (load average 9 to 11): T2 15.5 ms against 17.4 ms, 28 % of frames over budget against 53 %; T1 16.2 ms against 16.3 ms. Merging the static scenery ([sculpted scenery](../knowledge/algorithms/sculpted-scenery.md)) more than paid for three times the content.
- T3 and T4 await the high-end reference machine.

### What each technique costs

Measured on the Iris Xe at 1080p with `--without`, on the first scene, before the tiers were cut to fit (2026-11-10):

| Technique, as first configured | Cost | Decision |
|---|---|---|
| Scene alone (no technique, 4× MSAA) | 10.0 ms | — |
| Physical atmosphere, default tables | about 4 ms | T2 uses small tables |
| Scene lit by the atmosphere (environment map, every frame) | 5.3 ms | T3 and above |
| Shadows, 3 cascades of 2048 over 120 m | 4.7 ms | T2 uses 2 cascades of 1024 over 80 m: 2.6 ms |
| Ambient occlusion, medium | 9.8 ms | T3 and above |
| Temporal anti-aliasing and bloom | about 1 ms, plus a depth prepass | T2 uses FXAA: no prepass |
| Scene lit by a clear-day cubemap computed once (T2, 1632×918, the scene with landmarks) | 2.1 ms; 2.4 to 3.6 ms with an 8-texel cubemap | T3 and above: the cost is in the shading path, not the cubemap |

A defect found on the way: with a depth prepass (temporal anti-aliasing, ambient occlusion), the Iris Xe under Mesa leaves speckles where the prepass and the main pass disagree on a pixel's depth. T1 and T2 have no prepass; the defect is to be reported to Bevy and rechecked on other GPUs before T3 relies on a prepass there.
