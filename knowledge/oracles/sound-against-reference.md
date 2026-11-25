---
type: Attested Computation
title: Sound against another engine
description: Measures osage's sound beside another engine's captures of the same controlled scenes, figure for figure against what each geometry asks, and judges both by a blind listening test whose protocol and analysis are fixed before it runs.
status: draft
tags: [oracle, sound, spatial-audio, listening-test, comparison]
runtime: shell
parameters:
  - { name: reference, type: string, required: false }
  - { name: hrir, type: string, required: false }
executor:
  resource: https://github.com/smichea/osage/blob/main/tools/paris-soundscape/src/against.rs
  receipt: [output]
attester:
  resource: https://github.com/smichea/osage/blob/main/tools/paris-soundscape/listening/analyse.py
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-25T12:00:00Z" }
---

# Computation

    cargo run --release -p paris-soundscape -- against [--reference {reference}] [--hrir {hrir}]
    python3 tools/paris-soundscape/listening/make_kit.py --plan PLAN.json --out KIT --key KEY.json --listeners 12 --seed SEED
    python3 tools/paris-soundscape/listening/analyse.py --key KEY.json answers-*.csv

`against` renders the controlled scenes (`tools/paris-soundscape/scenes/controlled-*.json`) with [osage-sound](/modules/osage-sound.md). Each is rendered raw at reference quality, in three ways: headphones with the sphere model, headphones with [measured ears](/algorithms/measured-ears.md), and loudspeakers. It reads every take, and every capture of the other engine found in the folder, alike with [osage-sound-analysis](/modules/osage-sound-analysis.md), and prints markdown tables.

`make_kit.py` makes the blind test's kit. It cuts each pair of takes to the same length and brings them to the same loudness, in the same kind of file and under random codes, with one page per listener. The key goes apart. `analyse.py` reads the answers against the key.

# What it proves

1. **Where the ears hear a noise going round:** the level of each ear, and the level difference in three bands and the time difference, ahead, right, behind and left.
2. **A passing tone's pitch:** at 0.5, 5 and 9.5 s, and when it comes back under 1 kHz, against the instant its nearest sound arrives.
3. **When a click arrives** and on which side, against the distance.
4. **What a screen and a floor do:** the scene with the surface over the scene without it, by octave. For the floor, the first three dips of the comb against those its delay predicts.
5. **The blind test:** for each question, the share of answers preferring osage, with an interval that holds across the questions and treats each listener's answers as one. It also reports the catch trials and how many listeners thought they knew which program made a recording.

`tools/paris-soundscape/tests/controlled.rs` checks osage's own answer to each scene against its geometry, on every run of the tests.

# What it does not prove

- **Absolute levels.** The two engines play their own voices, so their absolute levels are not compared.
- **Folding down for headphones.** It does not show how another engine folds its other output channels down for headphones, unless its captures do.
- **A test that has not run.** The kit and the analysis prove nothing until listeners have answered, as the protocol says.

# Used by

[Research 11](https://github.com/smichea/osage/blob/main/research/11-sound-against-reference.md), against the external reference of the city programme (issue #19, D-243). Reference A's captures stay in the evaluation track's private notes.
