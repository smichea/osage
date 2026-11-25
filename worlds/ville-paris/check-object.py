#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Convert the original slab, query all three surfaces, then drop a capsule on it."""

import argparse
import csv
import hashlib
import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TOOL = ROOT / "tools/object-convert/convert.py"
FIXTURES = TOOL.parent / "fixtures"


def run(*args):
    return subprocess.run(
        [str(a) for a in args], cwd=ROOT, check=True, capture_output=True, text=True
    ).stdout


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def convert(out, drawable, bounds, *options):
    run(
        sys.executable,
        TOOL,
        "--id",
        "slab",
        "--drawable",
        drawable,
        "--bounds",
        bounds,
        *options,
        "-o",
        out,
    )


def inspect(binary, scene, lod="high", x=0):
    return json.loads(
        run(
            binary,
            "--scene",
            scene,
            "--inspect-object",
            "--visual-lod",
            lod,
            "--ray",
            f"{x},3,0",
            f"{x},0,0",
        )
    )


def distances(result):
    return [
        result["ray"][channel][0]["distanceM"]
        for channel in ("visual", "collision", "acoustic")
    ]


def drop(binary, scene, replay, folder, engine):
    args = (
        binary,
        "--scene",
        scene,
        "--replay",
        replay,
        "--physics",
        engine,
        "--actor",
        "capsule",
    )
    run(*args, "--bench", folder)
    run(*args, "--verify", folder)
    run(*args, "--visual-lod", "low", "--verify", folder)
    summary = json.loads((folder / "dynamics.json").read_text())["run"]
    outcome = summary["outcomes"][0]
    require(
        outcome["target_contacted"]
        and outcome["first_contacts"][0]["surface"] == "slab/bounds/0/m0",
        f"{engine} missed the imported floor",
    )
    require(
        abs(outcome["end_root_m"][1] - 1.88) < 0.02,
        f"{engine} used the wrong floor height",
    )
    with (folder / "dynamics.csv").open() as csvfile:
        first = next(
            c
            for row in csv.DictReader(csvfile)
            for c in json.loads(row["contacts_json"])
        )
    require(
        abs(first["point"][1] - 1.0) < 0.01 and first["normal"][1] > 0.99,
        f"{engine} contact point on wrong surface",
    )
    return {
        "firstTick": outcome["first_contacts"][0]["tick"],
        "firstPointM": first["point"],
        "firstNormal": first["normal"],
        "finalCentreM": outcome["end_root_m"],
        "traceSha256": summary["trace_sha256"],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("out", type=Path)
    parser.add_argument(
        "--binary", type=Path, default=ROOT / "target/release/ville-paris"
    )
    parser.add_argument("--resource-tool", type=Path)
    args = parser.parse_args()
    binary, out = args.binary.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    drawable, bounds = FIXTURES / "slab.ydr.xml", FIXTURES / "slab.ybn.xml"
    replay = out / "drop.json"
    camera = {"eye": [3, 3, 4], "aim": [0, 1, 0]}
    replay.write_text(
        json.dumps(
            {
                "version": 1,
                "tick_hz": 60,
                "gravity_m_s2": 9.81,
                "terminal_m_s": 55.0,
                "viewport": [640, 480],
                "vertical_fov_deg": 42,
                "routes": [
                    {
                        "id": "drop",
                        "ticks": 120,
                        "start_m": [0, 3, 0],
                        "velocity_m_s": [0, 0, 0],
                        "expected_support": "slab/bounds/0/m0",
                        "camera": [dict(camera, tick=0), dict(camera, tick=120)],
                    }
                ],
            }
        )
        + "\n"
    )
    sources = [("xml", drawable, bounds, [])]
    if args.resource_tool:
        bridge = args.resource_tool.resolve()
        for source, name in [(drawable, "slab.ydr"), (bounds, "slab.ybn")]:
            run(
                bridge,
                "resource",
                "build",
                "--strict",
                "--no-update-check",
                source,
                "-o",
                out / name,
            )
        sources.append(
            ("binary", out / "slab.ydr", out / "slab.ybn", ["--resource-tool", bridge])
        )
    results = {}
    for kind, visual, physical, options in sources:
        scene = out / f"{kind}.glb"
        convert(scene, visual, physical, "--acoustic-proxy", *options)
        high, low = inspect(binary, scene), inspect(binary, scene, "low")
        for sample in (high, low):
            require(
                all(
                    abs(a - b) < 0.001
                    for a, b in zip(distances(sample), [1.75, 2.0, 2.0])
                ),
                "wrong channel distances",
            )
            require(
                sample["ray"]["acoustic"][0]["coefficients"] == {},
                "unknown acoustic response became a number",
            )
            require(
                sample["collisionTriangles"] == 2,
                "visual level changed collision topology",
            )
        require(
            [high["visualTriangles"], low["visualTriangles"]] == [2, 1],
            "wrong number of visible triangles",
        )
        miss = inspect(binary, scene, x=3)["ray"]
        require(
            all(not miss[k] for k in ("visual", "collision", "acoustic")),
            "unexpected contact outside the slab",
        )
        results[kind] = {
            "glbSha256": hashlib.sha256(scene.read_bytes()).hexdigest(),
            "distancesM": distances(high),
            "visualTriangles": [2, 1],
            "drops": {
                engine: drop(binary, scene, replay, out / kind / engine, engine)
                for engine in ("rapier", "jolt")
            },
        }
    if "binary" in results:
        for engine in ("rapier", "jolt"):
            xml_trace = results["xml"]["drops"][engine]["traceSha256"]
            binary_trace = results["binary"]["drops"][engine]["traceSha256"]
            require(
                xml_trace == binary_trace,
                f"{engine} differed across the two input encodings",
            )
    authored = out / "authored.glb"
    convert(
        authored,
        drawable,
        bounds,
        "--acoustic-bounds",
        FIXTURES / "sound.ybn.xml",
        "--materials",
        FIXTURES / "surfaces.toml",
    )
    probe = inspect(binary, authored)
    require(
        distances(probe) == [1.75, 2.0, 2.25],
        "independent acoustic geometry did not move",
    )
    sound = probe["ray"]["acoustic"][0]
    require(
        sound["source"] == "authored"
        and all(
            abs(a - b) < 1e-6
            for a, b in zip(sound["coefficients"]["transmission"], [0.16, 0.09, 0.04])
        ),
        "independent acoustic coefficients were lost",
    )
    results["authoredAcoustic"] = {
        "distancesM": distances(probe),
        "coefficients": sound["coefficients"],
    }
    with tempfile.NamedTemporaryFile(dir=out, mode="w", delete=False) as tmp:
        json.dump(results, tmp, indent=2)
        tmp.write("\n")
    Path(tmp.name).replace(out / "comparison.json")
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
