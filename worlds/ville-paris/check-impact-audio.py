#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Check contact-to-wave timing against camera distance and the city's live solver."""

import argparse
import array
import hashlib
import json
import math
import runpy
import struct
import subprocess
import sys
import tempfile
import wave
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    parser.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/impact-audio"
    )
    parser.add_argument(
        "--speakers-reference-bin",
        type=Path,
        help="compare default WAVs with a previous build",
    )
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    reports = {}

    def run(label, route, *options):
        wav = args.out / (label + ".wav")
        result = subprocess.run(
            [
                str(args.bin.resolve()),
                "--impact-wav",
                str(wav),
                "--route",
                route,
                *map(str, options),
            ],
            check=True,
            capture_output=True,
        )
        report = json.loads(result.stdout)
        (args.out / (label + ".json")).write_bytes(result.stdout)
        clear = all(
            e["direct_path"]["transmission_energy"] == [1.0] * 3
            for e in report["onsets"]
        )
        if args.speakers_reference_bin and report["listening"] == "speakers" and clear:
            with tempfile.TemporaryDirectory(dir=args.out) as temp:
                previous = Path(temp) / "reference.wav"
                subprocess.run(
                    [
                        str(args.speakers_reference_bin.resolve()),
                        "--impact-wav",
                        str(previous),
                        "--route",
                        route,
                        *map(str, options),
                    ],
                    check=True,
                    capture_output=True,
                )
                assert digest(previous) == digest(wav), (
                    f"{label} changed the default speaker mix"
                )
        with wave.open(str(wav), "rb") as sound:
            assert (
                sound.getframerate(),
                sound.getnchannels(),
                sound.getsampwidth(),
            ) == (48000, 2, 2)
            assert sound.getnframes() == report["frames"]
            samples = array.array("h")
            samples.frombytes(sound.readframes(sound.getnframes()))
            if sys.byteorder != "little":
                samples.byteswap()
        first = next((i // 2 for i, x in enumerate(samples) if x), None)
        assert first == report["first_nonzero_pcm_frame"]
        arrivals = [
            frame
            for event in report["onsets"]
            if (
                frame := event.get(
                    "first_path_frame",
                    event["arrival_frame"]
                    if any(event["direct_path"]["transmission_energy"])
                    else None,
                )
            )
            is not None
        ]
        if arrivals:
            earliest = min(arrivals)
            assert earliest <= first < earliest + 240
        else:
            assert first is None
        for event in report["onsets"]:
            if report.get("listening") == "speakers":
                assert event["ear_delay_frames"] == [0.0, 0.0]
            travel = event["distance_m"] / 343 * 48000
            assert (
                abs(event["arrival_frame"] - event["emission_frame"] - travel)
                <= 0.500001
            )
        peaks = [max(abs(x) for x in samples[i::2]) for i in range(2)]
        reports[label] = {
            "onsets": len(report["onsets"]),
            "first_frame": first,
            "peaks": peaks,
            "sha256": digest(wav),
            "first_event": report["onsets"][0] if report["onsets"] else None,
        }
        return report, samples

    replay = json.loads((HERE / "fixtures/descent.json").read_bytes())
    road = next(r for r in replay["routes"] if r["id"] == "road")
    point, _ = run("road-point", "road", "--physics", "point")
    assert len(point["onsets"]) == 1
    # The road is the tagged horizontal surface at y=8. The point's constant lateral
    # velocity makes its impact location independent of the observer.
    dt = point["onsets"][0]["emission_frame"] / 48000
    origin = [
        road["start_m"][0] + road["velocity_m_s"][0] * dt,
        8.0,
        road["start_m"][2] + road["velocity_m_s"][2] * dt,
    ]
    assert math.dist(origin, point["onsets"][0]["point_m"]) < 0.001
    origin = point["onsets"][0]["point_m"]
    replay["routes"] = [road]

    # A single road deck keeps the free-field direction controls independent of the
    # viaduct and its underside. The landing and the camera still use city coordinates.
    source = (HERE / "fixtures/bir-hakeim.glb").read_bytes()
    split = 20 + struct.unpack_from("<I", source, 12)[0]
    doc = json.loads(source[20:split])
    road_node = next(
        i for i, n in enumerate(doc["nodes"]) if n.get("name") == "support/road"
    )
    doc["scenes"][doc["scene"]]["nodes"] = [road_node]
    raw = json.dumps(doc, separators=(",", ":")).encode()
    raw += b" " * (-len(raw) % 4)
    payload = struct.pack("<I4s", len(raw), b"JSON") + raw + source[split:]
    free_field = args.out / "single-road.glb"
    free_field.write_bytes(
        struct.pack("<4sII", b"glTF", 2, len(payload) + 12) + payload
    )

    def fixed(label, dx, dz, ticks=160, dy=0, headphones=False):
        eye = [origin[0] + dx, origin[1] + dy, origin[2] + dz]
        aim = [eye[0], eye[1], eye[2] - 1]
        road.update(
            ticks=ticks,
            camera=[{"tick": t, "eye": eye, "aim": aim} for t in (0, ticks)],
        )
        path = args.out / (label + "-route.json")
        path.write_text(json.dumps(replay))
        mode = ["--listening", "headphones"] if headphones else []
        return run(
            label,
            "road",
            "--physics",
            "point",
            "--replay",
            path,
            "--scene",
            free_field,
            *mode,
        )

    near, _ = fixed("near", 0, 2)
    far, _ = fixed("far", 0, 22)
    delta = far["first_nonzero_pcm_frame"] - near["first_nonzero_pcm_frame"]
    assert abs(delta - 20 / 343 * 48000) < 6
    ratio = reports["near"]["peaks"][0] / reports["far"]["peaks"][0]
    assert 10.8 < ratio < 11.2
    fixed("right-10m", -10, 0)
    fixed("right-20m", -20, 0)
    radial_db = 20 * math.log10(
        reports["right-20m"]["peaks"][1] / reports["right-10m"]["peaks"][1]
    )
    assert abs(radial_db - 20 * math.log10(0.5)) < 0.05
    _, left = fixed("left", 8, 8)
    _, right = fixed("right", -8, 8)
    assert max(abs(a - b) for a, b in zip(left[::2], right[1::2], strict=True)) <= 2
    assert max(abs(a - b) for a, b in zip(left[1::2], right[::2], strict=True)) <= 2
    assert reports["left"]["peaks"][0] > reports["left"]["peaks"][1] * 2
    for side, height in (("above", -10), ("below", 10)):
        vertical, _ = fixed(side, 0, 0, dy=height)
        event = vertical["onsets"][0]
        assert event["distance_m"] == 10
        assert event["point_m"][0] == event["listener_at_arrival_m"][0]
        assert event["point_m"][2] == event["listener_at_arrival_m"][2]
        l, r = reports[side]["peaks"]
        assert l == r and l > 0
    assert reports["above"]["sha256"] == reports["below"]["sha256"]
    # The impact, height and sound path stay identical while only the final
    # listening mode changes. Check the signal actually sent to each ear.
    headphone_vertical = {}
    for name, dy in (("above", -10), ("below", 10)):
        receipt, stereo = fixed("headphones-" + name, 0, 0, dy=dy, headphones=True)
        assert receipt["listening"] == "headphones"
        event = receipt["onsets"][0]
        speaker_event = reports[name]["first_event"]
        for field in (
            "point_m",
            "material",
            "arrival_frame",
            "emission_frame",
            "distance_m",
        ):
            assert event[field] == speaker_event[field]
        assert abs(event["ear_delay_frames"][0] - 0.0875 / 343 * 48000) < 1e-6
        assert event["ear_delay_frames"][0] == event["ear_delay_frames"][1]
        assert stereo[::2] == stereo[1::2]
        headphone_vertical[name] = stereo[::2]
    above, below = headphone_vertical["above"], headphone_vertical["below"]
    # Compare normalised signals so a pure gain change cannot pass.
    similarity = sum(a * b for a, b in zip(above, below, strict=True)) / math.sqrt(
        sum(a * a for a in above) * sum(b * b for b in below)
    )
    assert 0 < similarity < 0.99999
    right_report, right_ears = fixed("headphones-right", -10, 0, headphones=True)
    left_report, left_ears = fixed("headphones-left", 10, 0, headphones=True)
    assert right_ears[::2] == left_ears[1::2] and right_ears[1::2] == left_ears[::2]
    delays = right_report["onsets"][0]["ear_delay_frames"]
    assert delays == left_report["onsets"][0]["ear_delay_frames"][::-1]
    expected_samples = (1 + math.pi / 2) * 0.0875 / 343 * 48000
    assert abs(delays[0] - expected_samples) < 1e-6 and delays[1] == 0
    first_ear = [
        next(i for i, v in enumerate(right_ears[ear::2]) if v) for ear in range(2)
    ]
    assert abs(first_ear[0] - first_ear[1] - expected_samples) < 8
    quiet, _ = fixed("air", 0, 2, ticks=40)
    assert not quiet["onsets"] and quiet["first_nonzero_pcm_frame"] is None
    for engine in ("jolt", "rapier"):
        city, _ = run(
            "road-" + engine, "road", "--physics", engine, "--actor", "capsule"
        )
        first_road = city["onsets"][0]
        assert (
            first_road["surface"] == "road"
            and first_road["tick"] == {"jolt": 65, "rapier": 66}[engine]
        )
        still = json.loads((HERE / "fixtures/descent.json").read_bytes())
        still["routes"] = [next(r for r in still["routes"] if r["id"] == "road")]
        still["routes"][0].update(start_m=[0, 8, 0], velocity_m_s=[0, 0, 0])
        still_path = args.out / "rest-route.json"
        still_path.write_text(json.dumps(still))
        rest, _ = run(
            "rest-" + engine,
            "road",
            "--physics",
            engine,
            "--actor",
            "capsule",
            "--replay",
            still_path,
        )
        assert rest["onsets"] == [] and rest["first_nonzero_pcm_frame"] is None
        for actor in ("capsule", "human"):
            for route in ("moving", "parked"):
                name = f"{route}-{engine}-{actor}"
                report, _ = run(
                    name,
                    route,
                    "--physics",
                    engine,
                    "--actor",
                    actor,
                    "--replay",
                    HERE / "fixtures/bonnet.json",
                )
                first = report["onsets"][0]
                assert (
                    first["surface"] == "bonnet" and first["material"] == "steel-plate"
                )
                assert 0.36 < first["emission_frame"] / 48000 < 0.43
                keys = [(e["surface"], e["emission_frame"]) for e in report["onsets"]]
                assert len(keys) == len(set(keys))
    for engine in ("rapier", "jolt"):
        label = f"moving-{engine}-human"
        report, _ = run(
            f"headphones-{label}",
            "moving",
            "--physics",
            engine,
            "--actor",
            "human",
            "--replay",
            HERE / "fixtures/bonnet.json",
            "--listening",
            "headphones",
        )
        assert (
            report["onsets"][0]["arrival_frame"]
            == reports[label]["first_event"]["arrival_frame"]
        )
        assert len(report["onsets"]) == reports[label]["onsets"]
    # A modern profile supplies material and ownership through the collision binding.
    owned = args.out / "owned"
    owned.mkdir(exist_ok=True)
    asset, drop = runpy.run_path(str(ROOT / "tools/object-convert/test_hierarchy.py"))[
        "sample"
    ](owned)
    config = json.loads(drop.read_bytes())
    route = config["routes"][0]["id"]
    data, _ = run(
        "owned",
        route,
        "--replay",
        drop,
        "--scene",
        asset,
        "--physics",
        "jolt",
        "--actor",
        "capsule",
    )
    onset = data["onsets"][0]
    assert len(onset["owner_path"]) >= 2
    assert onset["surface"] == config["routes"][0]["expected_support"]
    raw = asset.read_bytes()
    size = struct.unpack_from("<I", raw, 12)[0]
    doc = json.loads(raw[20 : 20 + size])
    materials = doc["asset"]["extras"]["osageObject"]["collisionMaterials"]
    physical = next(iter(materials.values()))
    for key in ("steel-plate", "asphalt"):
        materials[key] = physical
        for node in doc["nodes"]:
            for shape in node.get("extras", {}).get("osage", {}).get("collision", []):
                if shape["id"] == onset["surface"]:
                    shape["material"] = key
        head = json.dumps(doc).encode()
        head += b" " * (-len(head) % 4)
        payload = struct.pack("<I4s", len(head), b"JSON") + head + raw[20 + size :]
        variant = owned / (key + ".glb")
        variant.write_bytes(
            struct.pack("<4sII", b"glTF", 2, 12 + len(payload)) + payload
        )
        changed, _ = run(
            "owned-" + key,
            route,
            "--replay",
            drop,
            "--scene",
            variant,
            "--physics",
            "jolt",
            "--actor",
            "capsule",
        )
        event = changed["onsets"][0]
        assert (
            event["owner_path"] == onset["owner_path"]
            and event["emission_frame"] == onset["emission_frame"]
        )
        assert event["material"] == key
    assert reports["owned-steel-plate"]["sha256"] != reports["owned-asphalt"]["sha256"]
    # Keep the point trajectory fixed and move the receiver along the wave normal.
    # The camera crosses the last key only after these two receptions.
    for velocity in (-30.0, 30.0):
        road["ticks"] = 160
        v = velocity

        def camera_position(tick, speed):
            z = origin[2] + 35 + speed * (tick / 60 - dt)
            return {
                "tick": tick,
                "eye": [origin[0], 8, z],
                "aim": [origin[0], 8, z - 1],
            }

        road["camera"] = [camera_position(t, v) for t in (0, 160)]
        path = args.out / ("camera-motion-" + str(v) + ".json")
        path.write_text(json.dumps(replay))
        label = "receding" if v > 0 else "approaching"
        result, _ = run(label, "road", "--replay", path, "--physics", "point")
        event = result["onsets"][0]
        expected = 35 / (343 - v) * 48000
        assert abs(event["arrival_frame"] - event["emission_frame"] - expected) < 1
    repeated, _ = run("road-repeat", "road", "--physics", "point")
    assert (
        point == repeated
        and reports["road-repeat"]["sha256"] == reports["road-point"]["sha256"]
    )
    receipt = {
        "binary_sha256": digest(args.bin),
        "speakers_reference_sha256": digest(args.speakers_reference_bin)
        if args.speakers_reference_bin
        else None,
        "sample_rate": 48000,
        "point_20m_delay_frames": delta,
        "point_2m_to_22m_amplitude_ratio": ratio,
        "right_10m_to_20m_db": radial_db,
        "headphone_vertical_correlation": similarity,
        "headphone_lateral_first_pcm_frames": first_ear,
        "headphone_lateral_model_frames": delays,
        "cases": reports,
    }
    (args.out / "summary.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(
        f"{len(reports)} recordings, 20 m change: {delta} frames, level ratio {ratio:.3f}"
    )


if __name__ == "__main__":
    main()
