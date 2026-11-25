# Research 11 — osage's sound beside reference A, scene for scene

Date: **2026-11-25**. Issue #19, decision D-243. Like-for-like measurements of osage's sound engine and of the external reference of the city programme (reference A, [design/07](../design/07-city-scale-quality.md)), on small scenes that each ask one thing. The blind listening test that completes them has its protocol in [tools/paris-soundscape/listening/PROTOCOL.md](../tools/paris-soundscape/listening/PROTOCOL.md); it has not run yet.

## Question

Where a scene asks one thing — where a sound is, how its pitch moves as it passes, when a click arrives, what a screen or a floor does to it — what does each engine put in the two ears, and which is nearer what the geometry asks?

## Method

**The scenes** are `tools/paris-soundscape/scenes/controlled-*.json`, made by `make_controlled.py` beside them: one sound each, in a geometry any engine can play alike (metres, the listener at the origin, ears 1.6 m up, looking along −Z). Each renders in a test (`tools/paris-soundscape/tests/controlled.rs`), which checks its answer against the geometry.

**Reference A** was not opened by the author of this page. The evaluation track played the same geometries in reference A and captured its stereo output — 48 kHz, 32-bit float, music and ambience off, one voice — and answered with the captures and its own measurements; those captures stay in its private notes. They were made on reference A's own test bench, from a mono test signal fed to one voice; how its other output channels fold down for headphones in play was not measured. Reference A plays the geometries with its own voices, not osage's, so what is compared is what each engine does to a sound on its way: where the ears hear it, its pitch, its arrival, a surface's effect against no surface — never two engines' absolute levels.

**Every figure** is read alike from every take by `paris-soundscape against --reference DIR --hrir assets/sadie/d2-48k.hrir` (the figures below, as it printed them on this machine). osage's takes are rendered raw, at reference quality, three ways: headphones with the sphere model, headphones with SADIE II's measured ears ([D-242](../design/decisions.md)), and loudspeakers.

## Measurements

#### A noise going round the listener (`controlled-circle`)

Each second around the instant named: each ear's level under the take's loudest second, then the
right ear's level over the left's in three bands (50–500 Hz, 0.5–2 kHz, 2–20 kHz) and how much it leads
(µs, broadband). A channel more than 100 dB down is silent (—).

| | osage, headphones | osage, measured ears | osage, loudspeakers | reference A |
|---|---:|---:|---:|---:|
| ahead, 0.5 s: left / right, dB | -9.4 / -6.4 | -8.2 / -4.9 | -4.2 / -2.2 | -4.5 / -1.9 |
| ahead, 0.5 s: right − left | +0.1 / +1.4 / +3.3 dB, +89 µs | +1.0 / +2.1 / +3.5 dB, +77 µs | +2.0 / +2.0 / +2.0 dB, -0 µs | +2.5 / +2.7 / +2.6 dB, +0 µs |
| right, 5 s: left / right, dB | -14.1 / 0.3 | -16.2 / -1.2 | -47.1 / -0.0 | — / -3.2 |
| right, 5 s: right − left | +0.9 / +7.0 / +15.9 dB, +655 µs | +4.2 / +6.8 / +21.3 dB, +746 µs | +47.4 / +47.4 / +47.3 dB, -0 µs | — |
| behind, 10 s: left / right, dB | -8.0 / -7.8 | -8.9 / -10.2 | -3.1 / -3.0 | — / — |
| behind, 10 s: right − left | +0.0 / +0.1 / +0.2 dB, +8 µs | -0.2 / -0.0 / -1.8 dB, -11 µs | +0.2 / +0.2 / +0.2 dB, -0 µs | — |
| left, 15 s: left / right, dB | 0.3 / -14.0 | -2.2 / -16.5 | 0.0 / -47.1 | -3.4 / — |
| left, 15 s: right − left | -0.9 / -7.1 / -15.8 dB, -655 µs | -5.3 / -7.3 / -18.8 dB, -679 µs | -47.7 / -47.2 / -47.4 dB, -0 µs | — |

#### A tone passing at 72 km/h (`controlled-passby`)

The tone is 1 kHz from the start, 100 m to the left; it is level with the listener at 5 s, 5 m ahead.
Sound from the start takes 291.9 ms; at its nearest 14.577 ms, so its pitch comes back under 1 kHz at 5.01458 s.
The engine renders from 3 s before its first sample, so that what a source sent before reaches the ears
from the start: its first sound is at 0 by design. Reference A's voice starts with its capture.

| | osage, headphones | osage, measured ears | osage, loudspeakers | reference A |
|---|---:|---:|---:|---:|
| first sound, ms | 0.0 | 0.0 | 0.0 | 16.0 |
| pitch at 0.5 s, Hz | 1061.92 | 1061.92 | 1061.92 | 1062.49 |
| pitch at 5 s, Hz | 1003.15 | 1004.41 | 1003.35 | 1012.67 |
| pitch at 9.5 s, Hz | 945.08 | 945.08 | 945.08 | 944.60 |
| back under 1 kHz, s | 5.0151 | 5.0150 | 5.0149 | 5.0559 |

#### A tone 300 m ahead (`controlled-far`)

Sound takes 874.6 ms to come (the engine's renders start with it arrived, as for the passing tone); the level
is that of the steady part, 3 to 5 s.

| | osage, headphones | osage, measured ears | osage, loudspeakers |
|---|---:|---:|---:|
| first sound, ms | 0.0 | 0.0 | 0.0 |
| level, left / right, dB | -71.3 / -71.3 | -68.9 / -68.5 | -73.3 / -73.3 |

#### Clicks at a distance (`controlled-clicks`)

The engine's scene: a wheel on a plate at 1 s, 34.3 m to the right (heard at 1.100 s), another at 3 s, 137.2 m to
the left (3.400 s). The reference's captures are its own: one sample's click made at its first frame, 34.3 m and 343 m
away (`click-near.wav`, `click-far.wav`), when it arrives against when it should (100 and 1000 ms).

| | osage, headphones | osage, measured ears | osage, loudspeakers | reference A |
|---|---:|---:|---:|---:|
| nearer click (34.3 m) | 1.099 s, right by 3.4 dB | 1.099 s, right by 5.5 dB | 1.099 s, right by 61.7 dB | 16.0 ms |
| farther click (137.2 m here, 343 m there) | 3.399 s, left by 2.6 dB | 3.399 s, left by 6.4 dB | 3.399 s, left by 85.8 dB | 16.0 ms |

#### A screen 20 m wide and 4 m high halfway to a noise 20 m ahead (`controlled-screen` over `controlled-open`)

Mean power of both ears from 0.5 to 5.5 s, with the screen over without it.

| | osage, headphones | osage, measured ears | osage, loudspeakers | reference A |
|---|---:|---:|---:|---:|
| octave 250 Hz, dB | -12.60 | -13.38 | -12.61 | +0.00 |
| octave 1000 Hz, dB | -14.93 | -14.93 | -15.06 | +0.00 |
| octave 4000 Hz, dB | -18.14 | -18.71 | -18.07 | +0.00 |
| octave 8000 Hz, dB | -19.10 | -21.36 | -18.89 | +0.00 |

#### A hard floor under a noise 1 m up, the ears 2 m up and 4 m away (`controlled-floor`, with and without its floor)

The floor's reflection comes 2.557 ms later and should dig dips every 391.2 Hz, the first at 195.6 Hz.

| | osage, headphones | osage, measured ears | osage, loudspeakers | reference A |
|---|---:|---:|---:|---:|
| octave 250 Hz, dB | +0.74 | +0.19 | +0.84 | +0.00 |
| octave 1000 Hz, dB | +2.39 | +2.63 | +2.31 | +0.00 |
| octave 4000 Hz, dB | +2.01 | +1.89 | +2.22 | +0.00 |
| octave 8000 Hz, dB | +2.15 | +3.65 | +2.15 | +0.00 |
| first three dips, Hz (dB) | 198 (-15.1), 586 (-15.4), 978 (-17.0) | 214 (-8.1), 602 (-11.5), 994 (-15.9) | 194 (-14.9), 586 (-15.0), 978 (-15.1) | none under −6 dB |

## Reading

| Scene | What the geometry asks | osage | Reference A | |
|---|---|---|---|---|
| circle | the noise on each side in turn, behind as loud as ahead | the side's ear louder by 14 to 22 dB above 2 kHz and 655 to 746 µs sooner on headphones; heard all round | 9 dB louder on one channel at 27° (1 to 2 s), no time between channels, the far channel silent from about 55° to 135°, **both silent for 5 s behind** (135° to 225°) | osage better |
| passby | pitch back under 1 kHz at 5.0146 s; 1003.4 Hz at 5 s | 5.0149 to 5.0151 s; 1003.2 to 1004.4 Hz | 5.0559 s, 41 ms late; 1012.7 Hz | osage better |
| clicks | arrival after 100 ms at 34.3 m, 400 ms at 137.2 m, 1 s at 343 m | within 1 ms, on its side | 16 ms at 34.3 m and at 343 m alike | osage better |
| screen | quieter behind it, more so in the top octaves | −12.6 dB at 250 Hz to −19 dB at 8 kHz (no ground under the screen: its bottom edge is the way) | no change, its three files the same to the byte | osage better |
| floor | dips every 391.2 Hz from 195.6 Hz | 194 to 198, 586, 978 Hz, −15 dB (measured ears: 214, 602, 994 Hz, −8 to −16 dB) | no change, its files the same to the byte | osage better |
| far | a steady tone, on neither side | steady, on neither side | not captured | — |

The evaluation track's own measurements of the same captures agree with these: the circle second by second (`paris-soundscape measure`: +9.2, +9.3, +9.2 dB between 1 and 2 s against its 9.23, 9.30, 9.23), the pass's pitch within 0.15 Hz.

- **Re-sampling.** On the same pass with a 10 kHz tone, the strongest image of re-sampling stands 23.2 dB under the tone in reference A, whose voice reads its samples by straight lines between neighbours, and 51.8 dB under it in osage.
- **Clicks further away.** Reference A's click at 343 m is heard only when its range is stretched; at its usual range the click is silent.

**Nothing was found worse in osage** on these scenes. Two things are worth a second look:

- **Measured ears on the floor.** Through the measured ears, the floor's dips move up by 16 Hz and are less deep at the first. The direct sound and the reflection arrive from different heights, and a measured head colours them differently: real ears would too, so this is not a defect.
- **Loudspeakers.** They put a side source 47 dB more in one channel than in the other. That is what panning between two loudspeakers does, and a listener's own head then gives the time between the ears.

**What is not measured:**
- the far tone in reference A;
- clicks made by the same contact in both engines;
- reference A's other output channels folded down for headphones;
- whether listeners hear any of this: that is the blind test's question.

The blind test, written down before it runs, needs at least eight listeners and has not run yet.
