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
generated: { by: "claude-code/claude-fable-5-2", at: "2026-11-10T06:45:00Z" }
---

# Problem

To say that a rendering change is faster or slower, or that a tier holds its budget, two runs must draw the same thing — on different machines, at different speeds, on different screens.

# Idea

Take time away from the machine. The simulation advances one fixed step per drawn frame, so frame 900 shows the same karts in the same places everywhere; frames are drawn as fast as the machine can and each one is timed[^own].

# Method

1. **No server, no input:** six karts on the grid are driven by the world's autopilot with the world's physics, both deterministic.
2. **One step per frame:** the application's clock is advanced by exactly 1/60 s each frame, whatever the real time.
3. **Off screen at a fixed size:** the scene is drawn into a picture of the tier's resolution and only shown in the window ([resolution and upscaling](/techniques/resolution-and-upscaling.md)); vertical sync is off.
4. **Shaders first:** pipelines are compiled synchronously, so that the first frames are complete; the stall this causes is reported apart as the worst warm-up frame.
5. **Timing:** the time between the ends of consecutive frames. The first 120 frames are warm-up. Frames around a screenshot are skipped, because reading the picture back and encoding it is not rendering.
6. **Screenshots** of the picture at four fixed frames.
7. **Report:** tier, commit, GPU, driver, CPU, output size and size drawn, mean, median, 95th and 99th percentiles, worst frame, share of frames over the tier's budget.
8. Nothing in the scene uses a random number generator; particles and scenery use repeatable arithmetic.

# Invariants

For a given commit and tier, frame N is the same scene on every machine. Only the times differ.

# Cost

2400 frames: 40 s of race, in the time the machine needs to draw them.

# Failure modes

- Other programs running skew the times; a comparison is valid only between runs made back to back.
- A window manager or a display scale factor changes the size drawn if the scene is drawn into the window: hence the off-screen picture.
- Drawing while shaders still compile gives empty first frames and screenshots that differ by machine speed.

# Alternatives

Timing a recorded play session depends on the network and on frame rate. Timing with vertical sync on measures the display.

# Oracle

It is one: see [the render bench](/oracles/render-bench.md).

# Reuse

Any engine with a deterministic simulation can do the same; the list above is the recipe.

[^own]: osage design
