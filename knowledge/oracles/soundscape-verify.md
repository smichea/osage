---
type: Attested Computation
title: Soundscape verify
description: Proves that a render of a scene is reproducible, does not clip, is as loud as asked, places a side source on its side and swaps it when the head turns, keeps the river on its side throughout, and hears the train come, pass on the right and go.
status: draft
tags: [oracle, sound, spatial-audio, determinism]
runtime: shell
parameters:
  - { name: quality, type: string, required: false }
  - { name: listening, type: string, required: false }
  - { name: seconds, type: number, required: false }
executor:
  resource: https://github.com/smichea/osage/blob/main/tools/paris-soundscape/src/main.rs
  receipt: [exit_status, output]
attester:
  resource: https://github.com/smichea/osage/blob/main/tools/paris-soundscape/src/main.rs
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-24T09:00:00Z" }
---

# Computation

    cargo run --release -p paris-soundscape -- verify [--quality {quality}] [--listening {listening}] [--seconds {seconds}] [--scene FILE]

The command renders the scene several times with [osage-sound](/modules/osage-sound.md), measures each render with [osage-sound-analysis](/modules/osage-sound-analysis.md), and prints one line per check, `ok` or `FAIL`, with what it found. It exits with an error if any check failed; a check whose sound is missing (no emitter of that name) has failed, never passed.

# What it proves

1. **The same seed gives the same bytes**, twice; **another seed gives other bytes**.
2. **Nothing clips:** no sample held at the limits by the mastering, none at the limits of the file, and a true peak within 0.1 dB of the −1 dBTP ceiling.
3. **As loud as the scene asks:** the file as written, ends faded, reads within 0.3 LU of the loudness the mastering reached; if that is under what the scene asks, its true peak is at the ceiling ([loudness mastering](/algorithms/loudness-mastering.md)).
4. **Turning the listener half round moves the river to the other side:** the river alone reads a level difference (right minus left, 500 Hz to 8 kHz) under −1.5 dB ahead and over +1.5 dB turned, the two within 0.5 dB of each other's opposite; **and each ear then hears what the other heard**, sample for sample.
5. **The river stays on the left:** a second at a time, and over the render's last second whatever its length, the level difference never rises above −1 dB.
6. **The train comes, passes and goes:** heard alone, its smoothed envelope peaks within 5 s of the instant it is level with the listener, at least 10 dB above its start and 8 dB above its end; **it passes on the right:** a level difference above +1 dB over the 3 s each side of that instant.

Checks 4 to 6 use the bridge's names (`river`, `upper-deck`); another scene without them fails them, as it should.

# What it does not prove

That the scene sounds right: that is for listening (issue #20). That the physics of each way is right: the crate's tests measure that one effect at a time (`crates/osage-sound/tests/physics.rs`). What it costs: `paris-soundscape bench`.

# Used by

Continuous integration does not run it: its `sound` job runs the crates' tests. `verify` was run by hand on every change during the prototype's review, at the qualities and listenings the change concerned; its output is the receipt.
