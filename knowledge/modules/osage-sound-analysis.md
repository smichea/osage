---
type: Module
title: osage-sound-analysis
description: What a stereo recording tells of where its sounds are, measured the same way whatever made it — peaks between samples, loudness by ITU-R BS.1770-4, octave levels, level and time differences between the ears by band and second by second, a tone followed through its Doppler shift, a passage — so that two renders of a like scene are compared by numbers before they are compared by ear.
resource: https://github.com/smichea/osage/tree/main/crates/osage-sound-analysis
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [sound, measurement, oracle, loudness]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-24T09:00:00Z" }
---

# Purpose

The oracle of osage's sound. The tests of [osage-sound](/modules/osage-sound.md) and [osage-sound-voices](/modules/osage-sound-voices.md), the command's `verify` and `measure`, and comparisons with other engines and with captures of a reference all read a recording through this crate, so that a claim about where a sound is, how loud it is or how it moves is a number and not an impression.

# Concepts

- **Signal:** a slice of `f32` samples within −1…1 and its rate; measures are computed and given in `f64`.
- **Level:** decibels of mean squares relative to full scale (a full-scale sine reads −3.01 dB); silence reads −180 dB, never minus infinity.
- **Cue:** what two ears compare in a band and a stretch of time — the level difference (ILD), the time difference (ITD) and the coherence. Differences are right minus left: a source on the right reads positive on both.
- **Every sample counts alike** in a measure over a file: its last tenth of a second as any other, and the click of a file cut in the middle of a sound as a bank of filters would read it.

# Architecture

- **level:** sample peak, true peak (four points a sample through a 48-tap filter, as BS.1770 describes; good to a tenth of a decibel or two), mean square, offset, samples at the limits of a 16-bit file, correlation between channels.
- **loudness:** integrated loudness by ITU-R BS.1770-4 — K-weighting (a shelf and a high-pass), mean squares over 400 ms blocks overlapping by 75 %, an absolute gate at −70 LUFS and a relative gate 10 LU under the gated mean. One channel counts once: a mono file reads 3.01 LU under the same signal in both channels of a stereo one.
- **bands:** levels by octave and third of octave, over a file and over time, from an FFT of the whole; the spectrum's centre.
- **cues:** ILD and coherence by band from cross-spectra; ITD from the phase of the cross-spectrum in a band, and a broadband second opinion by generalised cross-correlation with phase transform (Knapp and Carter, 1976) between 100 Hz and 8 kHz.
- **pitch:** a tone followed over time within a band, and the shift of one that passes (its Doppler ratio before and after an instant).
- **envelope:** a level over time, smoothed; its peak, and a passage — how far it rises before and falls after.
- **fft, resample, wav, hash:** what the others stand on; `sha256` of a file's bytes.
- **report:** `measure(audio)` gathers everything into a `Report`: length, peaks, clipping, loudness, octaves, correlation, and second by second the level, the ILD, the ITD, the coherence and the broadband ITD.

Nothing is random and nothing reads a clock: the same samples give the same figures.

# Interfaces

`measure`, `Report`, `Channel`, `Second`; `loudness`, `true_peak`, `sample_peak`, `rms`, `dc`, `clipping`, `correlation`, `db`; `Band`, `octaves`, `third_octaves`, `band_levels`, `band_levels_over_time`, `spectral_centroid`; `cue`, `cues`, `cue_bands`, `gcc_phat`, `gcc_phat_between`; `tone_track`, `tone_shift`; `envelope`, `band_envelope`, `Passage`; `resample`; `wav::{parse, Audio}`; `sha256`.

# Invariants

Tested against signals whose answer is known (about eighty tests):

- The K-weighting's coefficients are the standard's table; a 997 Hz sine at −20 dBFS in both channels reads −20 LKFS, and 3 dB less in one channel; loudness follows a gain decibel for decibel; silence around a sound does not change it; a file at 44.1 kHz reads what it reads at 48 kHz.
- A full-scale sine at a quarter of the rate peaks 3 dB between its samples; a true peak is never under the sample peak.
- A copy half a millisecond late and 6 dB lower on the right reads −500 µs and −6 dB; a delay between two samples is read to a microsecond; a source that crosses changes the sign of both differences.
- A steady tone is read to a cent and at its level, under noise as loud as itself; a tone that passes reads the shift its speed makes.

# Knowledge

- Used by [osage-sound](/modules/osage-sound.md) for its offline mastering ([loudness mastering](/algorithms/loudness-mastering.md)) and its tests, and by [soundscape verify](/oracles/soundscape-verify.md).
- `paris-soundscape measure FILE.wav` prints a report of any stereo file, with `--json` for a receipt: the same figures for this engine's renders, another engine's and a binaural capture.

# Reuse

A small dependency-free toolkit for asserting where a sound is in a binaural file; the conventions (right minus left, every sample counts alike, silence as −180 dB) make tests readable.

# Limits

- Cues are energetic and phase measures, not a model of hearing: they say what the ears receive, not what a person perceives.
- No measure of height or of front and back: a sphere-free binaural file carries those in its spectrum, which no single figure here reads.
