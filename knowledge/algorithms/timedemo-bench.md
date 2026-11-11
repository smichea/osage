---
type: Algorithm
title: Timedemo bench
description: A benchmark that draws the same frames on every machine by stepping a deterministic simulation once per frame and drawing off screen.
resource: https://github.com/smichea/osage/blob/main/worlds/kart/native/src/bench.rs
licence: MIT OR Apache-2.0
status: draft
tags: [benchmark, rendering, determinism]
sources:
  - id: own
    resource: "osage's own design and measurements, 2026-11-09 and 2026-11-10"
    title: osage design
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-11T15:30:00Z" }
---

# Problem

To say that a rendering change is faster or slower, or that a tier holds its budget, two runs must draw the same thing — on different machines, at different speeds, on different screens.

# Idea

Take time away from the machine. The simulation advances one fixed step per drawn frame, so frame 900 shows the same karts in the same places everywhere; frames are drawn as fast as the machine can and each one is timed[^own].

# Method

1. **No server, no input:** six karts on the grid are driven by the world's autopilot with the world's physics, both deterministic. The autopilot does not see other karts, so the bench adds one rule: a kart that finds another less than 16 m ahead within 2.6 m of its line moves to the side with more room, and returns to its line once that is clear (version `kart-lap/2`; in version 1 the followed kart ran into the karts ahead and was slowed).
2. **One step per frame:** the application's clock is advanced by exactly 1/60 s each frame, whatever the real time.
3. **Off screen at a fixed size:** the scene is drawn into a picture of the tier's resolution and only shown in the window ([resolution and upscaling](/techniques/resolution-and-upscaling.md)); vertical sync is off.
4. **Shaders and content first:** pipelines are compiled synchronously, so that the first frames are complete; the stall this causes is reported apart as the worst warm-up frame. The race starts only once the circuit is built and every model it places has loaded, three frames in a row with nothing waiting: loading takes its own time on each machine, the race must not.
5. **Paced by the GPU:** with a window, drawing waits for the screen and the screen for the GPU. Without one (`--headless`), nothing would stop the processor from queuing frames far ahead of the GPU and then stalling until all are drawn — frames of a few milliseconds, then one of seconds, with only the mean meaningful. The render thread therefore keeps one frame in flight, as a window does: once it has handed frame N to the GPU, it waits until frame N − 1 is done (waiting for frame N itself would leave the GPU idle while N + 1 is prepared: 26 ms at T2 on the Iris Xe against 18).
6. **Timing:** the time between the ends of consecutive frames. The first 120 frames are warm-up. Frames around a screenshot are skipped, because reading the picture back and encoding it is not rendering.
7. **Screenshots** of the picture at four fixed frames.
8. **Report:** tier, sky, commit, GPU, driver, CPU, output size and size drawn, mean, median, 95th and 99th percentiles, worst frame, the numbers of the three slowest frames (where a stall is, to replay it), share of frames over the tier's budget.
9. Nothing in the scene uses a random number generator; particles and scenery use repeatable arithmetic.

# Invariants

For a given commit and tier, frame N is the same scene on every machine. Only the times differ.

# Cost

2400 frames: 40 s of race, in the time the machine needs to draw them.

# Failure modes

- Other programs running skew the times; a comparison is valid only between runs made back to back.
- A window manager or a display scale factor changes the size drawn if the scene is drawn into the window: hence the off-screen picture.
- Drawing while shaders still compile, or while models still load, gives empty first frames and screenshots that differ by machine speed.
- Without a window and without pacing, a GPU-bound scene gives a bimodal series (seen at T3 on the Iris Xe, 2026-11-11: median 16 ms, 95th percentile 1.5 s, mean 145 ms); paced, the same run gives about 150 ms at every percentile.

# Alternatives

Timing a recorded play session depends on the network and on frame rate. Timing with vertical sync on measures the display.

# Oracle

It is one: see [the render bench](/oracles/render-bench.md).

# Reuse

Any engine with a deterministic simulation can do the same; the list above is the recipe.

[^own]: osage design
