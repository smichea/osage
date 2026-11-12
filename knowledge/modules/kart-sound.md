---
type: Module
title: osage-kart-sound
description: A kart's sound made from its state — a two-stroke engine following the revs and the throttle, tyres squealing as they slide, kerbs drumming, grass, wind, knocks — from recordings where there are some, synthesised otherwise, sample by sample, deterministic.
resource: https://github.com/smichea/osage/tree/main/worlds/kart/components/sound
licence: MIT OR Apache-2.0
status: draft
tags: [kart, sound, synthesis, component]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-12T04:40:00Z" }
---

# Purpose

Every kart heard as it drives: from recordings when the content has them, played to follow the kart's state, and synthesised when it does not (too little bandwidth or storage) or for what has no recording (wind, kerbs). A component of the kart game (D-035).

# Concepts

- **Drive:** what the sound follows — revs (0 at idle, 1 at the limiter), throttle, how much the tyres slide, the ground (asphalt, kerb, grass), speed.
- **Voice:** one kart's sound, fed a `Drive` whenever the game knows more and eased towards it over 15 ms, and knocks as they come.
- **Recordings:** mono clips at 48 kHz, loops seamless: engine loops each with the engine speed it was recorded at, a squeal loop, a ground loop, knocks. Any missing layer is synthesised.
- **Revs:** a kart has no gearbox: above the centrifugal clutch (about 4 500 turns a minute) the engine turns with the wheels; below, it revs with the throttle (`revs`).

# Architecture

48 000 samples a second, mono:

- **Engine:** a single-cylinder two-stroke fires once a turn (47 to 242 firings a second from idle to 14 500 turns a minute), with a little jitter; off the throttle it skips firings now and then. Each firing is a burst, noisier under load, that rings the exhaust (a resonant band-pass at 380–640 Hz), a bark (1.4–2.3 kHz) and the body (a low-pass), driven through a soft saturation, louder and rougher with the throttle.
- **Tyres:** noise through a narrow band-pass (850 Hz rising with the slide and the speed), as loud as the slide to the power 1.5.
- **Ground:** kerbs drum at the rate their blocks pass (one a metre); grass is a low rumble.
- **Wind:** low-passed noise, rising with the square of the speed.
- **Knocks:** a 70 Hz thump, the 330 Hz ring of plastic and a crack of band-passed noise, decaying within 0.4 s.
- **Mix:** a soft limiter (a rational curve, as smooth as `tanh` and cheaper, 1 at 3 and beyond).

With recordings: the engine plays the two loops either side of the revs, crossfaded at equal power on a scale of ratios, each read faster or slower (×0.5 to ×2.5, interpolated) so that its note follows the revs; louder with the throttle. The squeal loop plays faster as the slide grows, the ground loop with the speed; a knock plays one of the knock clips, picked by a seeded hash, over the synthesised thump. Their levels are set to match the synthesised layers' (measured RMS).

Filters are a state-variable band-pass (topology preserving) and one-pole low-passes; the noise is a seeded xorshift. What changes slowly — the eased drive, filter coefficients, gains — is computed every 32 samples (0.7 ms), not every sample: a voice costs about 0.2 µs a sample on a laptop core, where the audio callback runs.

# Interfaces

`Drive`, `Recordings`, `Voice::{new, with, set, knock, sample, render}`, `revs(speed, top, throttle)`, `RATE`, `IDLE_RPM`, `MAX_RPM`. The example `lap` renders twelve seconds of a kart to a WAV file.

# Invariants

- The engine fires once a turn: its period is the revs' (autocorrelation). Tested.
- The throttle makes it louder; every sample is finite and within −1…1. Tested.
- Tyres squeal only when they slide; kerbs drum at the rate their blocks pass; a knock is heard, then dies back to the engine. Tested.
- The same voice twice is the same. Tested.
- A recorded engine plays at the revs' speed (one loop at 100 Hz recorded at 6 000 turns a minute plays at 150 Hz at 9 000); between two loops the level holds; recorded tyres and knocks play only when the kart slides or is hit. Tested.

# Knowledge

- Used by [osage-kart-native](/modules/kart-native.md): a voice per kart, placed where the kart is and heard from the camera, its drive read from the kart's state (the throttle of other karts guessed from how they gain speed, the slide and spin from the physics, a knock from a sudden spin or stop, the ground from where the wheels are).
- The clips come from CC0 recordings in the [asset manifest](https://github.com/smichea/osage/blob/main/assets/manifest.toml) (Freesound originals), cut, made seamless and evened by `tools/make-sounds.py`: about 0.3 MB of Ogg Vorbis for 27 MB of originals. A swell or a dip inside a loop repeats with it, and at full speed the ear hears it once a second: the engine loops are cut from the steadiest passages and their loudness evened within the loop (80 ms windows).
- Oracle: the crate's tests; listening to the example.

# Reuse

Any small single-cylinder engine (scooters, mowers, motocross): the firing rate, resonances and misfires are the parameters.

# Limits

The synthesis does not sound recorded; the recordings stretch further from their speed the further the revs are (×2.5 at most). Mono, one engine type; no gearbox, no doppler of its own (the client's spatial audio places it but does not shift it).
