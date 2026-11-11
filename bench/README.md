# The bench

The oracle of the rendering programme ([design/03 §3](../design/03-rendering.md#3-oracles-how-done-is-decided)): the same frames on every machine, timed.

```sh
cargo run --release -p osage-kart-native -- --bench --tier T2
```

It plays 2400 frames (40 s of race) as fast as the machine can, and writes `report.json` and four screenshots to `bench/out/<tier>-<time>/` (not committed). The scene is drawn off screen at the tier's resolution and only shown in a window, so the display's scale factor and the window manager do not change what is measured. Options: `--frames N`, `--resolution WIDTHxHEIGHT`, `--out DIR`, and `--without TECHNIQUE` (sky, sky-light, shadows, ambient-occlusion, post, upscaling) to measure what one technique costs. `OSAGE_ASSETS=none` runs without the manifest's content; `--headless` opens no window and needs no display (machines without a screen, such as cloud GPUs).

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
| Google Cloud g2-standard-4, Singapore | NVIDIA L4 | T3, T4 | the high-end reference, for the changes merged into `main` (D-042, D-043) |

## On a cloud GPU

The workflow `cloud-bench` ([.github/workflows/cloud-bench.yml](../.github/workflows/cloud-bench.yml)) benches T3 and T4 when a push to `main` changes what can be drawn, once a night to catch up, and on demand (`gh workflow run cloud-bench -f tiers="T3"`). It benches only if something drawn changed since the last benched commit, and never twice within an hour (D-043): run the bench on your own machine first, and push in batches. An ordinary runner builds the bench; a machine with one NVIDIA L4 is made for the run, benches with `--headless`, puts its receipts in a bucket, and is deleted. The receipts and the attester's verdicts are the run's artefact `bench-receipts`, and its summary is a table. A run takes about ten minutes of the machine, about 0.15 USD.

- No key is stored: GitHub Actions acts as the project's `bench-ci` account through workload identity, only for this repository's `main` branch, pushed or started by hand. Pull requests never run it (D-041).
- The machine is deleted when the run ends, and by Google after 40 minutes if the run dies first. It sits on a network that lets nothing in.
- To set up a project of your own: `tools/cloud-bench/setup.sh PROJECT OWNER/REPOSITORY`, then `tools/cloud-bench/make-image.sh PROJECT` (Ubuntu 24.04 with NVIDIA's driver and Vulkan), then the three repository variables `setup.sh` prints. To bench by hand from a build: `tools/cloud-bench/bench.sh PROJECT kart.tar.gz OUT T3 T4`.

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
| 2026-11-10 | same, commit 195c7e3 | Google Cloud g2-standard-4 (NVIDIA L4, driver 580), headless | T3 | 2560×1440 | 5.7 ms (174 fps) | 6.9 ms | 7.5 ms | 0 % |
| 2026-11-10 | same, commit 195c7e3 | Google Cloud g2-standard-4 (NVIDIA L4, driver 580), headless | T4 | 2560×1440 | 6.7 ms (149 fps) | 8.2 ms | 12.8 ms | 0 % |
| 2026-11-11 | photographed circuit with woods, grandstand, levels of detail, grass carpet, fences, commit 8c91dee | Google Cloud g2-standard-4 (NVIDIA L4, driver 580), headless | T3 | 2560×1440 | 11.9 ms (84 fps) | 14.7 ms | — | 0.09 % |
| 2026-11-11 | same | Google Cloud g2-standard-4 (NVIDIA L4, driver 580), headless | T4 | 2560×1440 | 12.1 ms (82 fps) | 14.7 ms | — | 0.04 % |
| 2026-11-11 | photographed circuit (D-044), commit 883fa2b, roadside models from T3 | Google Cloud g2-standard-4 (NVIDIA L4, driver 580), headless | T3 | 2560×1440 | 10.0 ms (100 fps) | 13.4 ms | 16.6 ms | 0.9 % |
| 2026-11-11 | same | Google Cloud g2-standard-4 (NVIDIA L4, driver 580), headless | T4 | 2560×1440 | 12.0 ms (84 fps) | 17.7 ms | 21.2 ms | 0 % |
| 2026-11-11 | photographed circuit (D-044): sky panorama, 2k surfaces, racing karts; no roadside models below T3 | ThinkPad E14 Gen 5 (Iris Xe), headless, load average 10 to 13 | T1 | 1280×720 | 14.1 ms (71 fps) | 19.5 ms | 23.7 ms | 0.1 % |
| 2026-11-11 | same | ThinkPad E14 Gen 5 (Iris Xe), headless, load average 13 | T2 | 1920×1080 (1632×918) | 17.5 ms (57 fps) | 24.7 ms | 31.1 ms | — |
| 2026-11-11 | firs and broadleaf trees, thinned foliage, meadows that vary, sculpted driver, commit 7401532 | Google Cloud g2-standard-4 (NVIDIA L4, driver 580), headless | T3 | 2560×1440 | 13.7 ms (73 fps) | 15.9 ms | 18.2 ms | — |
| 2026-11-11 | same | Google Cloud g2-standard-4 (NVIDIA L4, driver 580), headless | T4 | 2560×1440 | 14.6 ms (69 fps) | 18.2 ms | 21.5 ms | — |
| 2026-11-11 | woods of 18 000 tree cards in place of 700 modelled trees, concrete grain, sculpted crowd, onlookers and marshals, commit b7b0c78 | Google Cloud g2-standard-4 (NVIDIA L4, driver 580), headless | T3 | 2560×1440 | 12.9 ms (78 fps) | 14.5 ms | 15.1 ms | 0.09 % |
| 2026-11-11 | same | Google Cloud g2-standard-4 (NVIDIA L4, driver 580), headless | T4 | 2560×1440 | 13.2 ms (76 fps) | 15.1 ms | 16.7 ms | 0.04 % |
| 2026-11-11 | sculpted crowd, commit 1159de1 | ThinkPad E14 Gen 5 (Iris Xe), headless | T1 | 1280×720 | 10.6 ms (95 fps) | 13.0 ms | 14.9 ms | — |
| 2026-11-11 | tree cards, commit 997aaca | ThinkPad E14 Gen 5 (Iris Xe), headless | T2 | 1920×1080 (1632×918) | 12.3 ms (81 fps) | 14.8 ms | 16.3 ms | — |
| 2026-11-11 | onlookers, marshals, pit lane, worn markings, mixed-age woods, woods at T2, commit 253dbb9 | Google Cloud g2-standard-4 (NVIDIA L4, driver 580), headless | T3 | 2560×1440 | 12.4 ms (81 fps) | 13.9 ms | 14.9 ms | 0.13 % |
| 2026-11-11 | same | Google Cloud g2-standard-4 (NVIDIA L4, driver 580), headless | T4 | 2560×1440 | 13.1 ms (76 fps) | 15.3 ms | 16.8 ms | 0.04 % |

- "Under load": other programs were using the machine during these runs (load average 8 to 9 on 10 cores); a quiet run is expected to do better and will replace them.
- T2 holds its budget on average on the integrated GPU, not for every frame. Without drawing under the output size it took 23.2 ms in the same conditions against 18.0 ms with: the scene is bound by the number of pixels there.
- Bench version 2 (`kart-lap/2`) lets the karts overtake one another; in version 1 the followed kart ran into the karts ahead. Its frames differ, so compare runs of the same version only.
- The landmarks scene against the previous one, same binary bench (version 1), alternated under the same load (load average 9 to 11): T2 15.5 ms against 17.4 ms, 28 % of frames over budget against 53 %; T1 16.2 ms against 16.3 ms. Merging the static scenery ([sculpted scenery](../knowledge/algorithms/sculpted-scenery.md)) more than paid for three times the content.
- The photographed circuit costs nothing measurable at T2: the same run without the content (`OSAGE_ASSETS=none`) took 18.1 ms. Its roadside models (tyre walls, plants, rocks, pines) took T2 to 54 ms on the Iris Xe: they start at T3.
- Without a window, the bench keeps one frame in flight, as a window does (2026-11-11). Before, nothing paced it: a scene the GPU could not keep up with gave frames of a few milliseconds and then one of seconds, and only the mean was right. Waiting for the GPU after every frame instead left it idle while the next one was prepared (T2: 26 ms against 18).
- T3 and T4 on the L4 use about a third of their budget (16.7 and 33.3 ms): the high tiers have room for techniques that cost more. Shader compilation stalls the first frame for about 4.5 s on that machine (warm-up, not timed).
- Commit b7b0c78 on the L4: the slowest timed frame took 0.77 s at both tiers. Commit 253dbb9, whose report names the slowest frames, put it at frame 262 at both tiers; the shaders compiled there were a level of detail's, with its fading visibility range, the first time one came into range (the bench makes pipelines as it first needs them, to draw the same frames every run). osage-props now draws every level once at the start; on the Iris Xe no shader of the scene is compiled after timing starts. The warm-up now takes about 13 s on that machine: tree cards and sculptures are made on the processor at load.

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
