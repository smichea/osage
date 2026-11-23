<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Baselines: what a source costs in two open engines

The prototype's cost per source means little alone. These two programs measure, on the same machine, what one more spatialised source costs in OpenAL Soft and in Chromium's Web Audio API, so that the prototype can be set beside them.

Both play the same scene: N mono sources, each looping its own second of seeded white noise, evenly spread on a circle of 10 m and turning at 10 m/s. The listener stands 6 m off the centre, so the distance of every source goes from 4 m to 16 m and its closing speed reaches 6 m/s. Positions change every 1024 frames. Ten seconds are rendered to stereo float at 48 kHz, once to warm up and then five times; the median is kept, and the smallest and largest are shown.

Nothing here is the prototype's code, and nothing here is read by it. Python standard library only.

## What each one does, and does not

The prototype renders, for every source and every path from it to the listener: a fractional delay line (so the true propagation delay, and Doppler as its consequence), 1/d, air absorption, a time difference and a head-shadow filter per ear, and first-order reflections as further paths.

| | prototype | OpenAL Soft 1.25.2, as measured | Web Audio in Chromium, as measured |
| --- | --- | --- | --- |
| Propagation delay | yes, a delay line per path | **no**: a source 343 m away is heard at once | **no** |
| Doppler | yes, from the changing delay | yes, as a pitch shift computed from the two velocities (OpenAL 1.1); checked here, see below | **not in the API**; the page computes the shift and sets `playbackRate`, as a game must |
| Distance | 1/d | inverse distance, clamped at 1 m (its default) | `inverse`, reference 1 m (its default) |
| Air absorption | yes, per band | optional: one high-frequency gain per source (`AL_AIR_ABSORPTION_FACTOR`, EFX); off in the main run, on in one case | **no** |
| Ears | time difference and head shadow of a sphere, computed | **measured** impulse responses, 64 taps per ear at 48 kHz with a delay per ear, one pair per source (`hrtf-mode = full`, its default); or plain panning | `HRTF`: "a convolution with measured impulse responses from human subjects"; or `equalpower` panning |
| Reflections | first order, each a path with its own delay | **none from geometry**; one case sends every source to one shared EFX reverb, which is a room effect, not reflections off the scene | **none**; a ConvolverNode could add a room's response, not measured |
| Edges (diffraction) | the engine's part | no | no |
| Sample-rate conversion | the delay line's interpolation | a resampler per source: cubic spline by default, nearest in one case | linear interpolation is not documented; whatever `playbackRate` does |

So the fair pairings are these. One OpenAL source with HRTF is one *direct path* of the prototype without its delay, its air and its reflections; a prototype source with K reflections should be set against K + 1 OpenAL sources. The reverb case shows what a shared room effect adds; it does not stand for reflections. And the reverse holds: both baselines convolve with measured responses, which give elevation and front-back cues that the prototype's sphere cannot; OpenAL Soft also has band-limited resamplers, a late reverberation, ambisonic and multichannel outputs, and twenty years of devices behind it. A lower cost per source does not make the prototype better at what they do.

## How to run

```sh
python3 bench/openal_soft.py            # about 25 minutes; --quick skips the runs beyond 256 sources
python3 bench/web_audio.py              # about 40 minutes; --quick keeps the two main cases
```

Each prints its table and writes a receipt (`--out`, by default `out/bench/openal_soft.json` and `out/bench/web_audio.json`; `out/` is not kept by git). Each stops with one line when the library or the browser is missing; `--library` and `--browser` name another one. `--help` lists the rest.

How they measure:

- **One core.** The program pins itself and everything it starts to one logical CPU (`--cpu`, by default the idlest performance core), and locks that core against a second baseline. The receipt holds the governor, the frequency before and after, whether turbo is on, the load, and how busy the hyper-thread sibling was during each case.
- **OpenAL Soft** is called through `ctypes` and renders through its loopback device (`ALC_SOFT_loopback`): no sound card, no clock. The mixing happens inside `alcRenderSamplesSOFT`, in the calling thread; only that call is timed, with `time.process_time_ns`, and `time.thread_time_ns` beside it shows that no other thread took part. The calls that move the sources are made from Python and timed apart ("updates"): they are mostly the interpreter's time. `resource.getrusage` gives the whole process's time and `ru_maxrss` its memory. Every case runs in a fresh process. Before measuring, the program checks that the scene does what it claims: a 1 kHz tone closing at a tenth of the speed of sound must be heard at 1111 Hz, and a source four times as far must be four times as weak.
- **Web Audio** runs in a headless Chromium with a fresh profile. The page `web_audio.html` renders with an `OfflineAudioContext` and times `startRendering()` with `performance.now()`. That clock is not taken on trust: the page logs a line before and after each render, and the program times those lines itself and reads the CPU clocks of the browser's processes at both moments. No virtual time is used. Chromium renders an offline context on a thread born for that render and gone after it, so the render's CPU time is what the page's process spent less what its lasting threads spent. To move a source every 1024 frames, the page suspends the context at each block, sets the positions, and resumes: the offline form of a game loop.

## Results

Measured on 2026-11-23: Intel Core i5-1335U, one performance core held at 1.3 GHz by the power setting (turbo off), the machine busy with other work (load 10 to 17). Medians, in processor time. OpenAL Soft was measured twice, an hour apart; both runs are given. The receipts are in `out/bench/` after a run.

| ms of one processor for a second of sound | 1 | 8 | 32 | 64 | 128 | 256 sources |
|---|---:|---:|---:|---:|---:|---:|
| prototype, low, speakers | 2.5 | 8.6 | 33.1 | 60.6 | 113 | 276 |
| prototype, low, headphones | 3.9 | 22.1 | 71.8 | 140 | 234 | 452 |
| prototype, medium, headphones | 4.9 | 17.4 | 60.0 | 123 | 235 | 529 |
| prototype, high, headphones | 6.0 | 26.6 | 93.6 | 181 | 361 | 803 |
| prototype, reference, headphones | 11.8 | 53.9 | 161 | 291 | 589 | 1249 |
| prototype, high, over a road | 9.0 | 49.9 | 195 | 382 | 721 | 1712 |
| OpenAL Soft, panning | 1.0 to 1.3 | 4.0 to 4.4 | 12.7 to 15.2 | 33.4 to 35.0 | 61.2 to 63.2 | 115 to 139 |
| OpenAL Soft, HRTF | 23.5 to 32.0 | 43.5 to 64.1 | 119 to 178 | 234 to 248 | 510 to 556 | 1023 to 1131 |
| Web Audio, equalpower | 13.8 | 99.3 | 324 | 682 | 1435 | 3277 |
| Web Audio, HRTF | 18.7 | 134 | 563 | 1030 | 2371 | 4145 |

As a line through the points, for one more source and one sample:

| For one source and one sample, ns | prototype | OpenAL Soft 1.25 | Web Audio, Chromium 155 |
|---|---:|---:|---:|
| Panned to two loudspeakers | 18 to 22 (low) | 9 to 11 | 266 |
| Heard by two ears | 37 to 47 (low) · 38 to 43 (medium) · 59 to 65 (high) · 95 to 105 (reference) | 83 to 89 | 342 |
| Heard by two ears, with its reflection off a road | 117 to 139 (high) | not done | not done |

OpenAL Soft grew by 55 MB with 256 sources, 47 MB of it their one-second buffers. Chromium held 926 to 1073 MB at most, all its processes together. The prototype held 43 MB at most over its whole run, 256 sources included.

Web Audio's figures include moving every source every 1 024 frames, which an offline context does by suspending and resuming the render; the page's own thread spends 3 to 83 ms a second on it beside the render. A live page would set positions from its main thread at its frame rate.

## The prototype's own figures

```sh
cargo build --release --manifest-path tools/paris-soundscape/Cargo.toml
taskset -c 0 tools/paris-soundscape/target/release/paris-soundscape bench --json prototype.json

cargo build --release --target wasm32-wasip1 --manifest-path tools/paris-soundscape/Cargo.toml
taskset -c 2 node tools/paris-soundscape/bench/run_wasm.mjs \
    tools/paris-soundscape/target/wasm32-wasip1/release/paris-soundscape.wasm bench
```

`bench` renders the bridge at each quality, then this same ring with 1 to 256 sources, then one way alone; `--part bridge`, `ring` or `way` runs one of them, `--quick` shortens them. It reads its own thread's processor time from `/proc`, so a busy machine changes its figures less than it changes a clock's. The second command runs the same program as WebAssembly in V8, the engine of Chromium.

## How to compare the prototype with these

1. Measure it on the same machine, in the same state, pinned the same way (`taskset -c N`), and close in time: the figures above move by a third with the load.
2. Divide its CPU time by sources × paths × samples, and say how many paths a source had.
3. Set its direct path against "HRTF" of OpenAL Soft, and its whole source, reflections included, against K + 1 of them.
4. Say what the two do not share, from the table above.
