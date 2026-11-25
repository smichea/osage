#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Follow a real solver impact through authored acoustic surfaces to its stereo PCM."""

import argparse
import array
import cmath
import copy
import hashlib
import json
import math
import runpy
import struct
import subprocess
import sys
import wave
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
BANDS = (0.04, 0.0025, 0.0001)


def read_glb(path):
    raw = path.read_bytes()
    end = 20 + struct.unpack_from("<I", raw, 12)[0]
    return json.loads(raw[20:end]), raw[end:]


def write_glb(path, doc, binary_chunk):
    raw = json.dumps(doc, separators=(",", ":")).encode()
    raw += b" " * (-len(raw) % 4)
    payload = struct.pack("<I4s", len(raw), b"JSON") + raw + binary_chunk
    path.write_bytes(struct.pack("<4sII", b"glTF", 2, 12 + len(payload)) + payload)


def panel(name, transmission=BANDS, center=(8.5, 2.5, 12.5), proxy=False):
    return name, transmission, center, proxy


def add_panel(doc, spec):
    name, transmission, center, proxy = spec
    source = "collision-proxy" if proxy else "authored"
    item = {"id": name, "mesh": 2, "source": source, "material": name}
    binding = {"id": name, "acoustic": [item]}
    if proxy:
        binding["collision"] = [
            {"id": name + "-solid", "mesh": 2, "material": "surface-7"}
        ]
    header = doc["asset"]["extras"]["osageObject"]
    header["acousticMaterials"][name] = (
        {}
        if transmission is None
        else {
            "absorption": [0] * 3,
            "transmission": transmission,
            "scattering": 0,
        }
    )
    # Keep the panel on a translated branch in the same scene as the struck slab.
    doc["nodes"][4]["children"].append(len(doc["nodes"]))
    x, y, z = center
    doc["nodes"].append(
        {"translation": [x - 2, y - 1, z + 1], "extras": {"osage": binding}}
    )


def digest(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def render(binary, out, name, options):
    path = out / (name + ".wav")
    receipt = json.loads(
        subprocess.check_output([binary, "--impact-wav", path, *options])
    )
    (out / (name + ".json")).write_text(json.dumps(receipt, indent=2) + "\n")
    with wave.open(str(path)) as wav:
        assert (wav.getframerate(), wav.getnchannels(), wav.getsampwidth()) == (
            48000,
            2,
            2,
        )
        pcm = array.array("h", wav.readframes(wav.getnframes()))
        if sys.byteorder != "little":
            pcm.byteswap()
    return receipt, pcm


def components(pcm):
    # Full transient, both ears. Each authored frequency has an integer period at 48 kHz.
    result = []
    for hz in (250, 1000, 4000):
        period = 48000 // hz
        weights = [cmath.exp(-2j * math.pi * i / period) for i in range(period)]
        ears = [
            sum(x * weights[n % period] for n, x in enumerate(pcm[ear::2]))
            for ear in range(2)
        ]
        result.append(math.sqrt(sum(abs(x) ** 2 for x in ears)))
    return result


def band_energies(pcm):
    # A narrow band averages PCM quantization at low received levels. The 13
    # Goertzel frequencies span one twelfth of an octave around each material band.
    start = next(i // 2 for i, x in enumerate(pcm) if x)
    stop = (len(pcm) - 1 - next(i for i, x in enumerate(reversed(pcm)) if x)) // 2 + 1
    ears = [pcm[start * 2 + ear : stop * 2 : 2] for ear in range(2)]
    energies = []
    for hz in (250, 1000, 4000):
        power = 0.0
        for k in range(-6, 7):
            coefficient = 2 * math.cos(math.tau * hz * 2 ** (k / 144) / 48000)
            for ear in ears:
                a, b = 0.0, 0.0
                for sample in ear:
                    a, b = sample + coefficient * a - b, a
                power += a * a + b * b - coefficient * a * b
        energies.append(power)
    return energies


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    parser.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/acoustic-screen"
    )
    parser.add_argument(
        "--reference", type=Path, help="previous native renderer for paired comparisons"
    )
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    asset, replay = runpy.run_path(
        str(ROOT / "tools/object-convert/test_hierarchy.py")
    )["sample"](out)
    base, chunk = read_glb(asset)
    base["scenes"][base["scene"]]["nodes"].append(4)
    base["nodes"].append(
        {
            "translation": [2, 0, -1],
            "children": [],
            "extras": {"osage": {"id": "screens"}},
        }
    )

    def direct(name, options):
        return render(args.bin, out, name, ["--direct-only", *options])

    cases = {}
    reference_pcm = None
    first_event = None

    def run(name, specs, expected_labels, expected_energy, mode="headphones"):
        nonlocal first_event, reference_pcm
        doc = copy.deepcopy(base)
        for spec in specs:
            add_panel(doc, spec)
        if not specs:
            doc["nodes"].pop()
            doc["scenes"][doc["scene"]]["nodes"].pop()
        scene = out / (name + ".glb")
        write_glb(scene, doc, chunk)
        opts = [
            "--scene",
            scene,
            "--replay",
            replay,
            "--route",
            "drop",
            "--physics",
            "jolt",
            "--actor",
            "capsule",
            "--listening",
            mode,
        ]
        info, pcm = direct(name, opts)
        (event,) = info["onsets"]
        path = event["direct_path"]
        assert [h["id"] for h in path["crossings"]] == expected_labels, path
        for item in path["crossings"]:
            assert item["owner_path"] == ["screens", item["id"]]
        assert all(
            abs(a - b) < 1e-8
            for a, b in zip(path["transmission_energy"], expected_energy, strict=True)
        ), path
        contact = {
            k: event[k]
            for k in (
                "tick",
                "surface",
                "part",
                "emission_frame",
                "arrival_frame",
                "point_m",
                "listener_at_arrival_m",
                "closing_m_s",
            )
        }
        if first_event is None:
            first_event = contact
            reference_pcm = pcm
        assert contact == first_event
        peak = max(map(abs, pcm))
        if expected_energy == [1] * 3:
            assert pcm == reference_pcm
        elif not any(expected_energy):
            assert peak == 0 and info["first_nonzero_pcm_frame"] is None
        elif len(set(expected_energy)) == 1:
            gain = expected_energy[0] ** 0.5
            assert (
                max(abs(a - b * gain) for a, b in zip(pcm, reference_pcm, strict=True))
                <= 1
            )
        else:
            assert 0 < peak < max(map(abs, reference_pcm))
        if args.reference and name in ("clear", "screen"):
            _, prior = render(args.reference, out, name + "-previous", opts)
            assert prior == reference_pcm
        cases[name] = {
            "peak_pcm": peak,
            "wav_sha256": digest(out / (name + ".wav")),
            "direct_path": path,
            "first_frame": info["first_nonzero_pcm_frame"],
        }
        return pcm

    run("clear", [], [], [1] * 3)
    wet = run("screen", [panel("wall")], ["wall"], BANDS)
    band_db = [
        20 * math.log10(w / d)
        for w, d in zip(components(wet), components(reference_pcm), strict=True)
    ]
    cases["screen"]["pcm_band_change_db"] = band_db
    # At the original distance a -40 dB component of a short impact is near
    # the PCM16 noise floor. Retain the twelfth-octave check below at that
    # distance and check each single frequency with a stronger received signal.
    source = first_event["point_m"]
    listener = first_event["listener_at_arrival_m"]
    eye = [(a + b) / 2 for a, b in zip(source, listener, strict=True)]
    center = [(a + b) / 2 for a, b in zip(source, eye, strict=True)]
    near_replay = json.loads(replay.read_bytes())
    for camera in near_replay["routes"][0]["camera"]:
        camera["aim"] = [aim + new - old for aim, new, old in zip(camera["aim"], eye, camera["eye"], strict=True)]
        camera["eye"] = eye
    near_path = out / "near-listener.json"
    near_path.write_text(json.dumps(near_replay))
    near_doc = copy.deepcopy(base)
    add_panel(near_doc, panel("wall", center=center))
    near_screen = out / "near-screen.glb"
    write_glb(near_screen, near_doc, chunk)
    amplitudes = []
    for name, scene in [("clear", out / "clear.glb"), ("screen", near_screen)]:
        info, pcm = direct("near-" + name, [
            "--scene", scene, "--replay", near_path, "--route", "drop", "--physics", "jolt",
            "--actor", "capsule", "--listening", "headphones",
        ])
        event, = info["onsets"]
        assert event["point_m"] == source and event["emission_frame"] == first_event["emission_frame"]
        assert [c["id"] for c in event["direct_path"]["crossings"]] == ([] if name == "clear" else ["wall"])
        amplitudes.append(components(pcm))
    assert all(near > 1.8 * far for near, far in zip(amplitudes[0], components(reference_pcm), strict=True))
    calibrated_db = [20 * math.log10(w / d) for d, w in zip(*amplitudes, strict=True)]
    assert all(abs(value - 10 * math.log10(energy)) < 1.5
               for value, energy in zip(calibrated_db, BANDS, strict=True)), calibrated_db
    cases["screen"]["near_listener_single_frequency_db"] = calibrated_db
    run("opening", [panel("wall", center=(12.5, 2.5, 12.5))], [], [1] * 3)
    run("origin", [panel("wall", center=(7, 1, 10))], [], [1] * 3)
    run("receiver", [panel("wall", center=(10, 4, 15))], [], [1] * 3)
    run("beyond", [panel("wall", center=(13, 7, 20))], [], [1] * 3)
    run("unknown", [panel("wall", None)], ["wall"], [1] * 3)
    run("clear-material", [panel("wall", [1] * 3)], ["wall"], [1] * 3)
    run("opaque", [panel("wall", [0] * 3)], ["wall"], [0] * 3)
    run("quarter-energy", [panel("wall", [0.25] * 3)], ["wall"], [0.25] * 3)
    run(
        "two-sheets",
        [
            panel("second", [0.25] * 3, center=(9.25, 3.25, 13.75)),
            panel("first", [0.25] * 3, center=(7.75, 1.75, 11.25)),
        ],
        ["first", "second"],
        [0.0625] * 3,
    )
    assert run("proxy", [panel("wall", proxy=True)], ["wall"], BANDS) == wet
    remote = [panel(f"off-{n}", center=(30 + n * 3, 2.5, 12.5)) for n in range(300)]
    assert run("distant-panels", [panel("wall"), *remote], ["wall"], BANDS) == wet
    assert (
        cases["distant-panels"]["direct_path"]["candidates_tested"]
        == cases["screen"]["direct_path"]["candidates_tested"]
        <= 5
    )

    base_options = [
        "--replay",
        replay,
        "--route",
        "drop",
        "--physics",
        "jolt",
        "--actor",
        "capsule",
    ]
    stereo = {}
    speaker_bands = {}
    for name in ("clear", "screen"):
        info, waveform = direct(
            "speakers-" + name,
            [
                *base_options,
                "--scene",
                out / (name + ".glb"),
                "--listening",
                "speakers",
            ],
        )
        (event,) = info["onsets"]
        assert all(event[k] == value for k, value in first_event.items())
        assert event["direct_path"] == cases[name]["direct_path"]
        stereo[name] = components(waveform)
        speaker_bands[name] = band_energies(waveform)
        cases["speakers-" + name] = {
            "peak_pcm": max(map(abs, waveform)),
            "direct_path": event["direct_path"],
        }
    cases["speakers-screen"]["pcm_band_change_db"] = [
        20 * math.log10(w / d)
        for w, d in zip(stereo["screen"], stereo["clear"], strict=True)
    ]
    for name, wet_power, dry_power in (
        ("screen", band_energies(wet), band_energies(reference_pcm)),
        ("speakers-screen", speaker_bands["screen"], speaker_bands["clear"]),
    ):
        changes = [
            10 * math.log10(w / d) for w, d in zip(wet_power, dry_power, strict=True)
        ]
        assert all(
            abs(measured - 10 * math.log10(energy)) < 1.5
            for measured, energy in zip(changes, BANDS, strict=True)
        ), changes
        cases[name]["pcm_twelfth_octave_db"] = changes

    # The fine and coarse bindings share a physical crossing but belong to different
    # levels of an acoustic subtree. Changing sound detail must not change the fall.
    lod_doc = copy.deepcopy(base)
    add_panel(lod_doc, panel("assembly", (0.25,) * 3))
    assembly = lod_doc["nodes"][-1]
    assembly["children"] = [len(lod_doc["nodes"])]
    assembly["extras"]["osage"]["subtreeProxy"] = {"acoustic": "low"}
    fine = copy.deepcopy(assembly)
    fine.pop("children")
    fine["translation"] = [0, 0, 0]
    fine["extras"]["osage"] = {
        "id": "membrane",
        "acoustic": [
            {"id": "fine-sheet", "mesh": 2, "source": "authored", "material": "open"}
        ],
    }
    lod_doc["nodes"].append(fine)
    lod_doc["asset"]["extras"]["osageObject"]["acousticMaterials"]["open"] = {
        "absorption": [0] * 3,
        "transmission": [1] * 3,
        "scattering": 0,
    }
    lod_scene = out / "acoustic-lods.glb"
    write_glb(lod_scene, lod_doc, chunk)
    for level in ("high", "low"):
        info, waveform = direct(
            "acoustic-" + level,
            [
                *base_options,
                "--scene",
                lod_scene,
                "--acoustic-lod",
                level,
                "--listening",
                "headphones",
            ],
        )
        (event,) = info["onsets"]
        assert all(event[k] == value for k, value in first_event.items())
        path = event["direct_path"]
        (hit,) = path["crossings"]
        assert hit["owner_path"] == (
            ["screens", "assembly", "membrane"]
            if level == "high"
            else ["screens", "assembly"]
        )
        assert hit["id"] == ("fine-sheet" if level == "high" else "assembly")
        gain = 1.0 if level == "high" else 0.5
        assert (
            max(abs(a - b * gain) for a, b in zip(waveform, reference_pcm, strict=True))
            <= 1
        )
        cases["acoustic-" + level] = {
            "peak_pcm": max(map(abs, waveform)),
            "direct_path": path,
        }

    # The same physical point and camera in the bundled city; its rail deck has two authored faces.
    road = json.loads((HERE / "fixtures/descent.json").read_bytes())
    route = next(r for r in road["routes"] if r["id"] == "road")
    road["routes"] = [route]
    route.update(
        ticks=160,
        camera=[{"tick": n, "eye": [0, 18, 0], "aim": [0, 18, -1]} for n in (0, 160)],
    )
    rpath = out / "under-the-viaduct.json"
    rpath.write_text(json.dumps(road))
    opts = [
        "--route",
        "road",
        "--physics",
        "point",
        "--listening",
        "headphones",
        "--replay",
        rpath,
    ]
    blocked, pcm = direct("viaduct", opts)
    (ev,) = blocked["onsets"]
    viaduct_path = ev["direct_path"]
    assert [c["id"] for c in viaduct_path["crossings"]] == ["underside", "top"], (
        viaduct_path
    )
    assert all(
        c["owner_path"] == ["bridge", "upper-deck"] for c in viaduct_path["crossings"]
    )
    assert all(
        abs(a - b * b) < 1e-10
        for a, b in zip(
            viaduct_path["transmission_energy"], [0.02, 0.003, 0.0005], strict=True
        )
    )
    cases["viaduct"] = {
        "peak_pcm": max(map(abs, pcm)),
        "direct_path": viaduct_path,
        "first_frame": blocked["first_nonzero_pcm_frame"],
    }
    if args.reference:
        ref, clear = render(args.reference, out, "viaduct-previous", opts)
        assert ev["arrival_frame"] == ref["onsets"][0]["arrival_frame"]
        assert 0 < max(map(abs, pcm)) < max(map(abs, clear))
        cases["viaduct"]["previous_peak_pcm"] = max(map(abs, clear))
    inspections = 0
    if args.reference:
        # Compare inclusive intersections with the original exhaustive triangle scan.
        endpoints = [
            ((7, 1, 10), (10, 4, 15)),
            ((10, 4, 15), (7, 1, 10)),
            ((8.5, 2.5, 12.5), (8.5, 3.5, 12.5)),
            ((8.5, 3.5, 12.5), (8.5, 2.5, 12.5)),
            ((9.5, 4, 13.5), (9.5, 1, 13.5)),
            ((8.5, 2.5, 12.5), (10, 2.5, 12.5)),
            ((8, 2, 12), (8, 2, 12)),
            ((8.5, 2.50001, 12.5), (8.5, 2.49999, 12.5)),
        ]
        for name in ("proxy", "two-sheets", "distant-panels"):
            for start, end in endpoints:
                command = [
                    "--scene",
                    out / (name + ".glb"),
                    "--inspect-object",
                    "--ray",
                    ",".join(map(str, start)),
                    ",".join(map(str, end)),
                ]
                if start == end:
                    for executable in (args.bin, args.reference):
                        result = subprocess.run(
                            [executable, *command], check=False, capture_output=True
                        )
                        assert (
                            result.returncode
                            and b"invalid probe segment" in result.stderr
                        )
                    inspections += 1
                    continue
                new = json.loads(subprocess.check_output([args.bin, *command]))["ray"]
                old = json.loads(subprocess.check_output([args.reference, *command]))[
                    "ray"
                ]
                assert new == old, (name, start, end)
                inspections += 1
    # One listening comparison with a common gain. The measurement WAVs above retain their levels.
    demo = out / "clear-then-screen.wav"
    common_gain = min(8, 32767 // max(max(map(abs, reference_pcm)), max(map(abs, wet))))
    with wave.open(str(demo), "wb") as out_wav:
        out_wav.setparams((2, 2, 48000, 0, "NONE", "not compressed"))
        for recording in (reference_pcm, wet):
            scaled = array.array("h", (x * common_gain for x in recording))
            if sys.byteorder != "little":
                scaled.byteswap()
            out_wav.writeframes(scaled.tobytes())
    summary = {
        "binary_sha256": digest(args.bin),
        "reference_sha256": digest(args.reference) if args.reference else None,
        "listening_comparison": {
            "file": demo.name,
            "common_gain": common_gain,
            "screen_starts_at_frame": len(reference_pcm) // 2,
        },
        "inspection_ray_pairs": inspections,
        "contact": first_event,
        "cases": cases,
    }
    (out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(
        f"{len(cases)} paths, filtered panel peak {max(map(abs, wet))}, clear peak {max(map(abs, reference_pcm))}"
    )


if __name__ == "__main__":
    main()
