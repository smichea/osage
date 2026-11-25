#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Measure sample timing, level, spectrum and direction on the stationary room-audio CLI."""

import argparse
import array
import hashlib
import json
import math
import os
import runpy
import struct
import subprocess
import sys
import wave
from pathlib import Path

from paging_checks import require

HERE = Path(__file__).resolve().parent
ROOMS = runpy.run_path(str(HERE / "check-room-portals.py"))
RATE = 48_000
SPEED = 343.0
EMIT = 317
OFFSET = 137
BANDS = (250, 1000, 4000)
MATERIAL = (0.25, 0.04, 0.0025)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def wav(path, samples, rate=RATE, channels=1):
    pcm = array.array("h", samples)
    if sys.byteorder != "little":
        pcm.byteswap()
    with wave.open(str(path), "wb") as f:
        f.setparams((channels, 2, rate, 0, "NONE", "not compressed"))
        f.writeframes(pcm.tobytes())


def ears(path):
    with wave.open(str(path), "rb") as f:
        require(
            (f.getnchannels(), f.getframerate(), f.getsampwidth()) == (2, RATE, 2),
            "incorrect stereo WAV format",
        )
        pcm = array.array("h", f.readframes(f.getnframes()))
    if sys.byteorder != "little":
        pcm.byteswap()
    return pcm[::2], pcm[1::2]


def float_wave(path, values):
    samples = struct.pack(f"<{len(values)}f", *values)
    fmt = struct.pack("<HHIIHHH", 3, 1, RATE, RATE * 4, 4, 32, 0)

    def chunk(tag, body):
        return tag + struct.pack("<I", len(body)) + body

    payload = (
        b"WAVE"
        + chunk(b"fmt ", fmt)
        + chunk(b"fact", struct.pack("<I", len(values)))
        + chunk(b"data", samples)
    )
    path.write_bytes(b"RIFF" + struct.pack("<I", len(payload)) + payload)


def float_ears(path):
    data = path.read_bytes()
    require(
        data[:4] == b"RIFF"
        and struct.unpack_from("<I", data, 4)[0] == len(data) - 8
        and data[8:12] == b"WAVE",
        "float RIFF length",
    )
    chunks = {}
    offset = 12
    while offset < len(data):
        tag, size = struct.unpack_from("<4sI", data, offset)
        chunks[tag] = data[offset + 8 : offset + 8 + size]
        offset += 8 + size + size % 2
    require(
        struct.unpack("<HHIIHHH", chunks[b"fmt "]) == (3, 2, RATE, RATE * 8, 8, 32, 0),
        "float WAV format",
    )
    (frames,) = struct.unpack("<I", chunks[b"fact"])
    require(len(chunks[b"data"]) == frames * 8, "fact sample length")
    values = struct.unpack(f"<{frames * 2}f", chunks[b"data"])
    return values[::2], values[1::2]


def commands(binary, scene, input_path, a, b, out, mode="speakers", emit=EMIT):
    return [
        binary,
        "--room-audio",
        "--scene",
        scene,
        "--input",
        input_path,
        "--source",
        ",".join(map(str, a)),
        "--listener",
        ",".join(map(str, b)),
        "--out",
        out,
        "--listening",
        mode,
        "--emit-frame",
        str(emit),
    ]


def rms(values):
    return math.sqrt(sum(x * x for x in values) / len(values))


def generate_receivers(out):
    fixture = out / "inputs"
    ROOMS["generate"](fixture, 1)
    doc, blur = ROOMS["build"](ROOMS["chain"]())
    raw = bytearray(blur)
    raw.extend(bytes(-len(raw) % 4))
    start = len(raw)
    raw.extend(struct.pack("<18f", 0, -20, -20, 0, 20, -20, 0, 0, 40, *([1, 0, 0] * 3)))
    at = len(doc["bufferViews"])
    doc["bufferViews"].extend(
        {"buffer": 0, "byteOffset": start + n * 36, "byteLength": 36, "target": 34962}
        for n in range(2)
    )
    ac = len(doc["accessors"])
    doc["accessors"].extend(
        [
            {
                "componentType": 5126,
                "type": "VEC3",
                "count": 3,
                "bufferView": at,
                "min": [0, -20, -20],
                "max": [0, 20, 40],
            },
            {"componentType": 5126, "type": "VEC3", "count": 3, "bufferView": at + 1},
        ]
    )
    doc["meshes"].append(
        {
            "primitives": [
                {"attributes": {"POSITION": ac, "NORMAL": ac + 1}, "indices": 2}
            ]
        }
    )
    doc["buffers"][0]["byteLength"] = len(raw)
    doc["nodes"][1]["children"].append(len(doc["nodes"]))
    doc["nodes"].append(
        ROOMS["node"](
            "glass",
            [10, 0, 0],
            acoustic=[
                {
                    "id": "glass",
                    "mesh": 1,
                    "source": "authored",
                    "material": "test-glass",
                }
            ],
        )
    )
    doc["asset"]["extras"]["osageObject"]["acousticMaterials"]["test-glass"] = {
        "absorption": [0] * 3,
        "scattering": 0,
        "transmission": MATERIAL,
    }
    ROOMS["write"](fixture / "sheet.glb", doc, raw)
    return fixture


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--bin", required=True, type=Path)
    ap.add_argument("--out", required=True, type=Path)
    args = ap.parse_args()
    binary = args.bin.resolve()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    fixtures = generate_receivers(out)
    # Original deterministic excitation. All output WAV fixtures can be used under CC0-1.0.
    register = 1
    pattern = []
    for _ in range(127):
        pattern.append(24000 if register & 1 else -24000)
        register = (register >> 1) | (((register ^ (register >> 1)) & 1) << 6)
    samples = [0] * OFFSET + pattern + [0] * 211
    cue = out / "source.wav"
    wav(cue, samples)
    results = {}
    failed = {}

    def render(name, scene, a, b, *, src=cue, mode="speakers", emit=EMIT, raw=False):
        path = out / (name + ".wav")
        cmd = commands(binary, fixtures / (scene + ".glb"), src, a, b, path, mode, emit)
        if raw:
            cmd.append("--float")
        p = subprocess.run(cmd, check=False, capture_output=True, text=True)
        require(p.returncode == 0, f"{name}: {p.stderr[-1300:]}")
        r = json.loads(p.stdout)
        channels = float_ears(path) if raw else ears(path)
        first = [next((i for i, x in enumerate(c) if x), None) for c in channels]
        require(
            r["frames"] == len(channels[0])
            and r["first_nonzero_pcm_frame"]
            == min((i for i in first if i is not None), default=None),
            f"{name}: publication metadata",
        )
        results[name] = {
            "sha256": digest(path),
            "frames": r["frames"],
            "source_first": r["first_nonzero_input_frame"],
            "emission_frame": r["emission_frame"],
            "arrival_frame": r["arrival_frame"],
            "first_per_ear": first,
            "output_format": r["output_format"],
            "peak": r["peak_preclip_per_ear"],
            "samples_over_full_scale": r["samples_over_full_scale"],
            "clipped_samples": r["clipped_samples"],
            "status": r["roomPath"]["status"],
            "length_m": r["roomPath"]["distance_m"],
            "path_sound": r["path_sound"],
        }
        return r, channels

    def timing(name, r, d):
        due = EMIT + math.floor(d / SPEED * RATE + 0.5)
        require(
            r["emission_frame"] == EMIT
            and r["arrival_frame"] == due
            and abs(r["roomPath"]["distance_m"] - d) < 1e-6,
            f"{name}: geometric reception time",
        )
        return due

    a = (2, 2, 0)
    b = (18, 2, 0)
    straight, lr = render("left", "chain", a, b)
    due = timing("left", straight, 16)
    require(
        straight["first_nonzero_input_frame"] == OFFSET
        and straight["first_nonzero_pcm_frame"] == due + OFFSET,
        "source onset confused with propagation",
    )
    # Determine the transport independently by matching a 127-sample code, excluding silence and file headers.
    best = max(
        range(max(0, due + OFFSET - 96), due + OFFSET + 97),
        key=lambda i: sum(
            x * y for x, y in zip(pattern, lr[0][i : i + len(pattern)], strict=True)
        ),
    )
    require(
        best - OFFSET == due and not any(lr[1]),
        "waveform correlation or left-only arrival",
    )
    results["left"]["matched_clip_start_frame"] = best - OFFSET
    x = lr[0][due : due + len(samples)]
    require(
        max(
            abs(y - round(s / 32768 * 32767 / 16))
            for s, y in zip(samples, x, strict=True)
        )
        <= 1,
        "inverse distance gain did not reach the actual PCM",
    )
    right, rl = render("right", "chain", b, a)
    timing("right", right, 16)
    require(lr == rl[::-1], "mirrored speaker signal")
    short, short_lr = render("near", "chain", (4, 2, 0), (4.5, 2, 0))
    t = timing("near", short, 0.5)
    require(
        short["path_sound"]["distance_gain"] == 1
        and max(
            abs(y - round(s / 32768 * 32767))
            for s, y in zip(samples, short_lr[0][t : t + len(samples)], strict=True)
        )
        <= 1,
        "near gain exceeded reference",
    )
    shifted, out_shifted = render("shift-emission", "chain", a, b, emit=EMIT + 47)
    require(
        shifted["arrival_frame"] == due + 47
        and all(c[47:] == ref for c, ref in zip(out_shifted, lr, strict=True)),
        "emission shift changed the sound",
    )
    silence = out / "silent-input.wav"
    wav(silence, [0] * len(samples))
    sr, empty = render("silent-source", "chain", a, b, src=silence)
    require(
        sr["arrival_frame"] == due
        and sr["first_nonzero_input_frame"] is None
        and not any(map(any, empty)),
        "silence erased the physical wavefront",
    )
    closed, mute = render("closed", "closed", a, b)
    require(
        closed["arrival_frame"] is None
        and closed["path_sound"] is None
        and not any(map(any, mute)),
        "closed room leaked a path",
    )

    hl, left = render("ears-left", "chain", a, b, mode="headphones")
    timing("ears-left", hl, 16)
    _, right = render("ears-right", "chain", b, a, mode="headphones")
    require(left == right[::-1], "headphone sides failed to mirror")
    near, far = results["ears-left"]["first_per_ear"]
    delay = (0.0875 / SPEED) * (1 + math.pi / 2) * RATE
    require(
        near == due + OFFSET and far - near == math.floor(delay),
        "headphone PCM arrival difference",
    )
    results["ears-left"]["broadside_expected_far_delay_frames"] = delay
    front, front_lr = render(
        "front", "rotated", (100, 6, 12), (100, 6, 28), mode="headphones"
    )
    require(
        front_lr[0] == front_lr[1]
        and abs(front["path_sound"]["ear_delay_frames"][0] - 0.0875 / SPEED * RATE)
        < 1e-8,
        "normal head frame/front ears",
    )

    one_a = (2, 2, 1.5)
    one_b = (18, 2, 1.5)
    (
        single,
        _,
    ) = render("one-bend", "chain", one_a, one_b)
    leg = math.hypot(8, 0.5)
    d = 2 * leg
    timing("one-bend", single, d)
    v = single["path_sound"]
    require(
        v["bends"] == 1
        and v["diffraction_model"] == "single_thin_screen"
        and v["direction_from_m"] == [10, 2, 1],
        "incorrect incoming direction",
    )
    pan_angle = (-8 / leg + 1) * math.pi / 4
    require(
        all(
            abs(x - y) < 1e-9
            for x, y in zip(
                v["speaker_pan"],
                [math.cos(pan_angle), math.sin(pan_angle)],
                strict=True,
            )
        ),
        "panning used original emitter bearing",
    )

    def loss(f):
        n = 2 * (d - 16) * f / SPEED
        return min(
            20,
            6 + 12 * math.sqrt(n)
            if n < 0.25
            else 8 + 8 * math.sqrt(n)
            if n < 1
            else 16 + 10 * math.log10(n),
        )

    require(
        all(
            abs(g - 10 ** (-loss(f) / 10)) < 1e-10
            for g, f in zip(v["diffraction_energy"], BANDS, strict=True)
        ),
        "single-screen independent band model",
    )
    p3 = (2, 0.5, 3)
    q3 = (28, 3.5, -1)
    between = math.dist((10, 3, 0), (20, 1, 2))
    distance3d = 2 * math.dist(p3, (10, 3, 0)) + between

    def two_pressure(f):
        wavelength = SPEED / f
        k = (5 * wavelength / between) ** 2
        ratio = 3 + 40 * (distance3d - math.dist(p3, q3)) / wavelength * (1 + k) / (
            1 / 3 + k
        )
        return 1 / math.sqrt(ratio)

    for f in BANDS:
        tone = out / f"tone-{f}.wav"
        inp = [int(23000 * math.sin(math.tau * f * n / RATE)) for n in range(RATE // 5)]
        wav(tone, inp)
        for scene, p, q, reference, expect in [
            ("chain", one_a, one_b, d, 10 ** (-loss(f) / 20)),
            ("sheet", a, b, 16, math.sqrt(MATERIAL[BANDS.index(f)])),
            ("staggered-height", p3, q3, distance3d, two_pressure(f)),
        ]:
            name = f"{scene}-{f}"
            t, stereo = render(name, scene, p, q, src=tone)
            arrived = timing(name, t, reference)
            start = RATE // 20
            stop = RATE // 8
            measured = (
                math.hypot(*(rms(c[arrived + start : arrived + stop]) for c in stereo))
                / rms(inp[start:stop])
                * reference
            )
            diff = 20 * math.log10(measured / expect)
            require(
                abs(diff) < 0.45,
                f"{name}: received pressure {measured} versus {expect}, {diff} dB",
            )
            results[name].update(
                expected_pressure=expect,
                measured_pressure=measured,
                level_error_db=diff,
            )
    sheet_sound = results["sheet-1000"]["path_sound"]
    crossings = sheet_sound["leg_transmission"][0]["crossings"]
    require(
        len(crossings) == 1 and crossings[0]["id"] == "glass",
        "glass on an aligned portal was lost or duplicated",
    )
    tangent, _ = render("sheet-at-bend", "sheet", one_a, one_b)
    require(
        tangent["path_sound"]["material_energy"] == [1, 1, 1]
        and all(
            not leg["crossings"] for leg in tangent["path_sound"]["leg_transmission"]
        ),
        "open-leg endpoint convention changed",
    )
    many, _ = render("two-bends", "staggered-height", (2, 0.5, 3), (28, 3.5, -1))
    timing("two-bends", many, distance3d)
    require(
        many["path_sound"]["bends"] == 2
        and many["path_sound"]["diffraction_model"] == "two_rigid_screens"
        and all(
            abs(math.sqrt(x) - two_pressure(f)) < 1e-9
            for x, f in zip(
                many["path_sound"]["diffraction_energy"], BANDS, strict=True
            )
        ),
        "double screen energy disagrees with the two-corner geometry",
    )
    third, _ = render("three-bends", "chain", (2, 2, 0), (48, 2, 0))
    require(
        third["path_sound"]["bends"] == 3
        and third["path_sound"]["diffraction_model"] == "unmodelled_multiple_bends"
        and third["path_sound"]["diffraction_energy"] == [1] * 3,
        "invented transfer beyond two screens",
    )
    opening, _ = render("unobstructed-half", "half", a, b)
    require(
        opening["path_sound"]["bends"] == 0
        and opening["path_sound"]["diffraction_energy"] == [1] * 3,
        "uncalibrated aperture-area filter",
    )

    raw_input = out / "original-float.wav"
    signal = [0.0] * 27 + [2.0**-40] + [0.0] * 12 + [1.75, -3.0] + [0.0] * 22
    float_wave(raw_input, signal)
    f32, values = render(
        "float-headroom", "chain", (4, 2, 0), (4.5, 2, 0), src=raw_input, raw=True
    )
    lag = timing("float-headroom", f32, 0.5)
    require(
        list(values[0][lag:]) == signal
        and not any(values[1])
        and f32["first_nonzero_output_frame"] == lag + 27
        and f32["samples_over_full_scale"] == 2
        and f32["clipped_samples"] == 0
        and f32["peak_preclip_per_ear"] == [3.0, 0.0],
        "float output quantized, normalized or clipped a source sample",
    )
    i16, values = render(
        "integer-headroom", "chain", (4, 2, 0), (4.5, 2, 0), src=raw_input
    )
    require(
        i16["first_nonzero_output_frame"] == lag + 40
        and i16["clipped_samples"] == 2
        and list(values[0][lag + 40 : lag + 42]) == [32767, -32767],
        "default PCM quantization or saturation changed",
    )
    for label, dist in (("tenth-second", 34.3), ("second", 343)):
        g, channels = render("float-" + label, "deep", a, (2 + dist, 2, 0), raw=True)
        travel = round(dist * RATE / SPEED)
        require(
            g["arrival_frame"] == EMIT + travel
            and g["first_nonzero_output_frame"] == EMIT + travel + OFFSET,
            "float clip front did not observe the acoustic delay",
        )
        peak = max(abs(x) for c in channels for x in c)
        require(
            abs(peak * dist - 24000 / 32768) < 2e-6,
            "float recording lost distance level",
        )
        results["float-" + label]["clip_start_from_waveform"] = (
            g["first_nonzero_output_frame"] - OFFSET
        )
    incomplete, _ = render("incomplete", "cycle-limit", (2, 2, 0.375), (138, 2, 0.375))
    require(
        not incomplete["roomPath"]["search_complete"]
        and incomplete["roomPath"]["lower_bound_m"]
        <= 136
        <= incomplete["roomPath"]["distance_m"],
        "bounded route presented as optimum",
    )

    def reject(name, cmd, match, protected):
        before = digest(protected)
        p = subprocess.run(cmd, check=False, capture_output=True, text=True)
        require(
            p.returncode != 0 and match in p.stderr and digest(protected) == before,
            f"{name}: {p.stderr[-900:]}",
        )
        failed[name] = match

    previous = out / "left.wav"
    scene = fixtures / "chain.glb"
    for name, p, match in [
        ("outside", (-2, 2, 0), "classification: outside"),
        ("ambiguous", (10, 2, 0), "classification: ambiguous"),
        ("nan", ("NaN", 2, 0), "finite"),
    ]:
        reject(name, commands(binary, scene, cue, p, b, previous), match, previous)
    for name, source, match in [
        ("stereo", out / "invalid-stereo.wav", "mono at 48000 Hz"),
        ("wrong-rate", out / "44k.wav", "mono at 48000 Hz"),
        ("truncated", out / "broken.wav", "truncated"),
    ]:
        if name == "stereo":
            wav(source, [0, 0] * len(samples), channels=2)
        if name == "wrong-rate":
            wav(source, samples, rate=44100)
        if name == "truncated":
            source.write_bytes(cue.read_bytes()[:-2])
        reject(name, commands(binary, scene, source, a, b, previous), match, previous)
    reject(
        "render-limit",
        commands(binary, scene, cue, a, b, previous, emit=120 * RATE - 1),
        "120-second output limit",
        previous,
    )
    for src in (scene, cue):
        alias = out / (src.name + ".alias")
        alias.unlink(missing_ok=True)
        alias.symlink_to(src)
        for kind, target in [("same", src), ("symlink", alias)]:
            reject(
                src.suffix + "-" + kind,
                commands(binary, scene, cue, a, b, target),
                "overwrite an input",
                src,
            )
        if os.name == "posix":
            link = out / (src.name + ".hard")
            link.unlink(missing_ok=True)
            os.link(src, link)
            reject(
                src.suffix + "-hardlink",
                commands(binary, scene, cue, a, b, link),
                "overwrite an input",
                src,
            )
    receipt = {
        "binary_sha256": digest(binary),
        "source_sha256": digest(cue),
        "sample_rate": RATE,
        "renders": results,
        "rejections": failed,
    }
    (out / "receipt.json").write_text(
        json.dumps(receipt, indent=2, allow_nan=False) + "\n"
    )
    print(
        f"{len(results)} stereo outputs, {len(failed)} atomic rejections; coded waveform locates sample 0 at {best - OFFSET}, "
        f"ear delay {far - near} frames; {out / 'receipt.json'}"
    )


if __name__ == "__main__":
    main()
