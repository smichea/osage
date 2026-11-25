<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Blind listening test: osage's sound against reference A

Written on 2026-11-25, before the test runs (issue #19, D-243). The test is run as written here. A change made after the first listener has answered is listed under **Changes** with its date and reason, and the results say which version they follow.

## Question

Do listeners prefer osage's sound or the sound of reference A (the external reference of the city programme, [design/07](../../../design/07-city-scale-quality.md))? The comparison is made scene by scene, on four stated attributes, without knowing which program made which recording.

## What is compared

Each **trial** is a pair of recordings of the same thing: one made by osage, one by reference A.

| Trial | osage | Reference A | Excerpt |
|---|---|---|---|
| bridge | `paris-soundscape render --hrir assets/sadie/d2-48k.hrir` (the made-up Pont de Bir-Hakeim, headphones) | the evaluation track's second 45 s capture on the bridge, a pedestrian standing still, music and radio off | 20 s holding each one's train passage |
| circle | `controlled-circle` | the same circle, captured by the evaluation track | the whole 20 s |
| passby | `controlled-passby` | the same pass, 1 kHz | the whole 10 s |
| screen | `controlled-screen` | the same screen | the whole 6 s |
| floor | `controlled-floor` | the same floor | the whole 6 s |

- **osage's recordings** are rendered with headphones, at reference quality, with SADIE II's measured ears (D-242).
- **Reference A's recordings** are the evaluation track's captures of its own output: stereo, 48 kHz, 32-bit float, kept unedited. They are observed from outside, as a player hears them. Nothing of reference A is opened, decompiled or copied ([clean sources](../../../CONTRIBUTING.md#clean-sources)).
- **These recordings are not committed.** Reference A's captures stay in the evaluation track's private notes. The plan file lists their paths and stays with them.
- **Clicks are left out.** Reference A's click captures are a single sample, which is no sound to judge.
- **The far tone is left out.** There is no capture of it from reference A.

## How the recordings are made alike

`make_kit.py` (beside this file) makes every excerpt alike:

- It cuts every excerpt to the same length as its pair, with the same 20 ms fades.
- It brings every excerpt to the same integrated loudness (ITU-R BS.1770, measured by `paris-soundscape measure`). The level is −23 LUFS, or lower if a take's true peak would pass −1 dBTP.
- It writes every excerpt as the same kind of file: stereo, 48 kHz, 16 bits, dithered.
- It names every excerpt with a random code.

On the page, the two recordings of a trial are X and Y:

- Which one is X is balanced: in every trial, osage is X for half of the listeners.
- Each listener gets the trials in an order of their own, drawn from the kit's seed.
- Each listener also gets one **catch trial**: the same recording twice, in a random place. "No difference" is the right answer there.

The **key** says which code is which program and gives each listener's order. It is written outside the kit's folder, and no listener sees it.

**Digital silence.** `make_kit.py` prints the digital silence in every take: runs of 1 ms or more where both channels are exactly zero. Such gaps can tell a listener which program made a recording.

- **Bridge.** Reference A's bridge capture has 221 gaps (1.4 s in 45 s). The excerpt keeps 78 of them (0.54 s). This is how its output reached the browser that captured it, and it is left as it is.
- **Circle.** Reference A's circle is fully silent for 5 s behind the listener. This is its output, as measured.
- **What follows.** Neither gap is mended, because mending would change one program's recording and not the other's. The last question asks every listener whether they thought they knew which program made a recording, and why. The results report the answers.

## Listeners and listening

- **Number.** At least 8 listeners, and 12 if they can be found. They are not authors of osage's sound, and have not heard these recordings before.
- **Equipment.** The same closed or open-back headphones and the same quiet room for everyone. The playback level is set once at the start, on a recording that is not in the test, to a comfortable level, and is not changed afterwards.
- **Procedure.** Each listener gets their own page (`listener-NN.html`) in the kit's folder. The page plays offline. The listener may replay each recording as often as they like, in any order. Every question must be answered before the answers can be saved. The page saves them as a CSV file.
- **Instructions.** The page's text is the only instruction. Listeners are not told that one program is osage, or what either program is.
- **Records.** For each listener the experimenter writes down only the listener number, the date, and the headphones used. Nothing that identifies a person is kept.

## Questions

For each trial, each of the four questions is answered with "X is better", "No difference" or "Y is better":

1. **place**: "Where the sounds are: which places each sound more clearly and steadily where it seems to be?"
2. **distance**: "Distance: which makes how near or far each sound is more believable?"
3. **natural**: "The sources: which sounds more like the real things it stands for?"
4. **space**: "The space: which makes the place the sounds are in more believable?"

The page ends with one more question: "Did you think you knew which program made a recording, in any trial?" If the answer is yes, the listener says which trials and why.

## Analysis

`analyse.py` (beside this file) computes:

- **The preference for osage, per question.** This is the share of answers over all trials and listeners that prefer osage, where an answer of "no difference" counts as half.
- **Its 95 % interval.** It comes from 10 000 bootstrap draws that resample listeners, not single answers, so each listener's answers stay together. The four intervals are made simultaneous by Bonferroni's correction (98.75 % each). The seed is 19.

Each question gets one verdict:

| Verdict | When |
|---|---|
| **osage preferred** | the interval's lower bound is above 50 % |
| **reference A preferred** | the upper bound is below 50 % |
| **no difference shown** | otherwise |

The results also report:

- the shares by trial, without intervals;
- the catch trials;
- how many listeners thought they knew which program made a recording.

**Nobody is excluded.** A listener who hears a difference in most catch answers is named in the results and kept in them.

**What follows from the results:**
- A verdict of "reference A preferred", or a trial whose share is under 40 % on a question, becomes a ticket of its own.
- The results are committed with the analysis as printed, the kit's seed, the date and the headphones.
- The key and the answers stay with the evaluation track's notes, because they name reference A's private captures.

## Running it

```sh
cargo build --release -p paris-soundscape
cargo run --release -p paris-soundscape -- render --hrir assets/sadie/d2-48k.hrir --out takes/bridge.wav
for s in circle passby screen floor; do
  cargo run --release -p paris-soundscape -- render --hrir assets/sadie/d2-48k.hrir \
    --scene tools/paris-soundscape/scenes/controlled-$s.json --out takes/$s.wav
done
python3 tools/paris-soundscape/listening/make_kit.py --plan PLAN.json --out KIT --key KEY.json --listeners 12 --seed SEED
# … the listeners answer; their CSV files come back …
python3 tools/paris-soundscape/listening/analyse.py --key KEY.json answers-*.csv
```

`PLAN.json` pairs each trial's two takes and gives each excerpt's start and length:

```json
{"trials": [{"id": "bridge", "osage": "takes/bridge.wav", "reference": "…/bridge-capture.wav", "from": 14.0, "seconds": 20.0}]}
```

The seed is drawn once, when the kit is made, and is written in the results.

## Changes

None yet.
