# The bench

The oracle of the rendering programme ([design/03 §3](../design/03-rendering.md#3-oracles-how-done-is-decided)): the same frames on every machine, timed.

```sh
cargo run --release -p osage-kart-native -- --bench --tier T2
```

It plays 2400 frames (40 s of race) as fast as the machine can, and writes `report.json` and four screenshots to `bench/out/<tier>-<time>/` (not committed). The scene is drawn off screen at the tier's resolution and only shown in a window, so the display's scale factor and the window manager do not change what is measured. Options: `--frames N`, `--resolution WIDTHxHEIGHT`, `--out DIR`, and `--without TECHNIQUE` (sky, shadows, ambient-occlusion, post) to measure what one technique costs.

- Close other programs; on a laptop, plug it in.
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

| Date | Machine | Tier | Resolution | Mean | p95 | p99 | Over budget | Shader warm-up |
|---|---|---|---|---|---|---|---|---|
| 2026-11-10 | ThinkPad E14 Gen 5 (Iris Xe) | T1 | 1280×720 | 11.2 ms (89 fps) | 13.9 ms | 16.6 ms | 0 % of frames | 3.9 s |
| 2026-11-10 | ThinkPad E14 Gen 5 (Iris Xe) | T2 | 1920×1080 | 15.9 ms (63 fps) | 22.3 ms | 27.1 ms | 33 % of frames | 2.4 s |

T2 holds its budget on average on the integrated GPU, not yet for every frame: a third are over. T3 and T4 await the high-end reference machine.

### What each technique costs

Measured on the Iris Xe at 1080p with `--without`, before the tiers were cut to fit (2026-11-10):

| Technique, as first configured | Cost | Decision |
|---|---|---|
| Scene alone (no technique, 4× MSAA) | 10.0 ms | — |
| Physical atmosphere, default tables | about 4 ms | T2 uses small tables |
| Scene lit by the atmosphere (environment map, every frame) | 5.3 ms | T3 and above |
| Shadows, 3 cascades of 2048 over 120 m | 4.7 ms | T2 uses 2 cascades of 1024 over 80 m: 2.6 ms |
| Ambient occlusion, medium | 9.8 ms | T3 and above |
| Temporal anti-aliasing and bloom | about 1 ms, plus a depth prepass | T2 uses FXAA: no prepass |

A defect found on the way: with a depth prepass (temporal anti-aliasing, ambient occlusion), the Iris Xe under Mesa leaves speckles where the prepass and the main pass disagree on a pixel's depth. T1 and T2 have no prepass; the defect is to be reported to Bevy and rechecked on other GPUs before T3 relies on a prepass there.
