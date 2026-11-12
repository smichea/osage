#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Makes the kart world's sound clips from the recordings in the asset manifest.

    python3 tools/make-sounds.py

Reads the originals (assets/freesound/originals/, downloaded by a person: see
assets/manifest.toml), cuts the passages below, makes the loops seamless,
evens their loudness, and writes Ogg Vorbis, mono, 48 kHz, to
assets/freesound/kart/. Needs ffmpeg with libvorbis; Python standard library
only. The game plays these clips when they are there and synthesises its
sound otherwise (osage-kart-sound).
"""

import array
import math
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
ORIGINALS = ROOT / "assets" / "freesound" / "originals"
OUT = ROOT / "assets" / "freesound" / "kart"
RATE = 48_000
# Loops are evened to this loudness (RMS), one-shots to this peak.
LOOP_RMS = 0.1
SHOT_PEAK = 0.8

ENGINE_IDLE = "175964__kristian321__motocross-bike-starting-and-ticking-over.wav"
ENGINE_DIRT = "675214__craigsmith__s22-11-2-stroke-dirt-bike-small-motorcycle.wav"
HANDBRAKE = "76804__audible-edge__ae0090-volvo-740-gle-handbrake-turn-01.flac"
GRAVEL = "545828__erwil__car-turning-on-gravel-with-a-bit-of-birds-megan-renault.flac"
PLASTIC = "151470__bellick__plastic-hit-1.wav"
CROWD = "486215__craigsmith__r07-32-crowd-egging-on-team.wav"

# name: (original, start s, end s, crossfade s). Passages chosen from their
# loudness and pitch over time: the engine's steady, the tyres' two tones.
LOOPS = {
    "engine-idle": (ENGINE_IDLE, 6.0, 9.0, 0.25),
    "engine-mid": (ENGINE_DIRT, 7.0, 8.8, 0.2),
    "engine-high": (ENGINE_DIRT, 15.5, 16.9, 0.2),
    "squeal": (HANDBRAKE, 7.85, 9.25, 0.25),
    "gravel": (GRAVEL, 2.8, 4.6, 0.3),
    "crowd": (CROWD, 2.0, 22.0, 1.0),
}
# name: (original, about when the hit is, s): cut from just before its
# peak, 0.35 s long.
SHOTS = {
    "knock-1": (PLASTIC, 1.0),
    "knock-2": (PLASTIC, 4.0),
    "knock-3": (PLASTIC, 6.0),
}


def decode(name: str, start: float, end: float) -> array.array:
    path = ORIGINALS / name
    raw = subprocess.run(
        ["ffmpeg", "-v", "error", "-ss", str(start), "-to", str(end), "-i", str(path),
         "-ac", "1", "-ar", str(RATE), "-f", "f32le", "-"],
        check=True, capture_output=True,
    ).stdout
    samples = array.array("f")
    samples.frombytes(raw)
    return samples


def encode(samples, name: str) -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    data = array.array("f", samples).tobytes()
    subprocess.run(
        ["ffmpeg", "-v", "error", "-y", "-f", "f32le", "-ar", str(RATE), "-ac", "1", "-i", "-",
         "-c:a", "libvorbis", "-q:a", "4", str(OUT / f"{name}.ogg")],
        input=data, check=True,
    )


def seamless(segment, crossfade: int):
    """The segment minus its last `crossfade` samples, whose sound is faded
    into its beginning at equal power: its end runs into its start."""
    n = len(segment) - crossfade
    out = list(segment[:n])
    for i in range(crossfade):
        t = i / crossfade
        out[i] = segment[i] * math.sin(t * math.pi / 2) + segment[n + i] * math.cos(t * math.pi / 2)
    return out


def scaled(samples, gain: float):
    return [max(-1.0, min(1.0, s * gain)) for s in samples]


def main() -> int:
    missing = [n for n in {v[0] for v in [*LOOPS.values(), *SHOTS.values()]} if not (ORIGINALS / n).exists()]
    if missing:
        for name in missing:
            print(f"missing  assets/freesound/originals/{name}: download it as assets/manifest.toml says")
        return 1
    for name, (original, start, end, fade) in LOOPS.items():
        segment = decode(original, start, end)
        loop = seamless(segment, int(fade * RATE))
        rms = math.sqrt(sum(s * s for s in loop) / len(loop))
        encode(scaled(loop, LOOP_RMS / max(rms, 1e-6)), name)
        print(f"{name}.ogg  {len(loop) / RATE:.2f} s loop")
    for name, (original, around) in SHOTS.items():
        window = decode(original, around - 0.4, around + 0.8)
        peak = max(range(len(window)), key=lambda i: abs(window[i]))
        start = max(0, peak - int(0.01 * RATE))
        shot = list(window[start:start + int(0.35 * RATE)])
        fade = int(0.1 * RATE)
        for i in range(fade):
            shot[-fade + i] *= 1.0 - i / fade
        top = max(abs(s) for s in shot)
        encode(scaled(shot, SHOT_PEAK / max(top, 1e-6)), name)
        print(f"{name}.ogg  {len(shot) / RATE:.2f} s")
    return 0


if __name__ == "__main__":
    sys.exit(main())
