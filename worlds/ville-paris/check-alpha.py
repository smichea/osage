#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Render a wall and its visible neighbour with glTF alpha modes and vertex alpha."""

import argparse
import hashlib
import json
import runpy
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
COOK = runpy.run_path(str(HERE / "cook-pages.py"))
rgb = runpy.run_path(str(HERE / "check-pictures.py"))["rgb"]


def run(args, log):
    with log.open("w") as out:
        subprocess.run(args, stdout=out, stderr=subprocess.STDOUT, check=True)


def fixture(cube, mode, output):
    # Shared position and index data for front and rear, with independent materials.
    front = {
        "pbrMetallicRoughness": {
            "baseColorFactor": [0.85, 0.01, 0.01, 0.25],
            "metallicFactor": 0,
        }
    }
    rear = {
        "pbrMetallicRoughness": {
            "baseColorFactor": [0.01, 0.85, 0.01, 1],
            "metallicFactor": 0,
        }
    }
    if mode in ("cut", "kept", "holes"):
        front.update(alphaMode="MASK", alphaCutoff=0.25 if mode != "cut" else 0.5)
    if mode == "blend":
        front["alphaMode"] = "BLEND"
    scene = COOK["Glb"]([front, rear])
    channels = {}
    if mode == "holes":
        channels["COLOR_0"] = COOK["Channel"](
            "VEC4",
            5121,
            [(255, 255, 255, 255 if p[2] >= 0 else 0) for p in cube.positions],
        )
        front["pbrMetallicRoughness"]["baseColorFactor"][3] = 1
        front["alphaCutoff"] = 0.5
    part = cube._replace(material=0, channels=channels)
    if mode in ("single", "double"):
        # Just one quad. Its authored front points away from the camera.
        indices = [i for i in part.indices if part.normals[i][0] > 0.99]
        assert len(indices) == 6
        part = part._replace(indices=indices)
        front["doubleSided"] = mode == "double"
    scene.node(
        "front",
        [part],
        {"name": "front", "translation": [16, 10, 16], "scale": [12, 28, 34]},
    )
    scene.node(
        "rear",
        [cube._replace(material=1)],
        {"name": "rear", "translation": [100, 10, 16], "scale": [12, 18, 24]},
    )
    scene.node(
        "floor",
        [cube._replace(material=1)],
        {"name": "support/reference", "translation": [-80, 0, -80], "scale": [6, 1, 6]},
    )
    output.write_bytes(scene.bytes())


def different(a, b):
    if len(a) != len(b):
        raise ValueError("frame size changed")
    return sum(a[i : i + 3] != b[i : i + 3] for i in range(0, len(a), 3))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    p.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/material-alpha"
    )
    args = p.parse_args()
    out, exe = args.out.resolve(), args.bin.resolve()
    out.mkdir(exist_ok=True, parents=True)
    doc, meshes, _ = COOK["source"](HERE / "fixtures/bir-hakeim.glb")
    (cube,) = meshes[
        next(i for i, m in enumerate(doc["meshes"]) if m["name"] == "iron")
    ]
    replay = json.loads((HERE / "fixtures/descent.json").read_text())
    replay["viewport"] = [320, 180]
    replay["routes"] = [
        {
            "id": "street",
            "ticks": 8,
            "expected_support": "reference",
            "start_m": [-80, 200, -80],
            "velocity_m_s": [0, 0, 0],
            "camera": [
                {"tick": t, "eye": [-18, 10, 16], "aim": [100, 10, 16]} for t in (0, 8)
            ],
        }
    ]
    flight = out / "route.json"
    flight.write_text(json.dumps(replay))
    frames, records = {}, {}
    for mode in ("opaque", "cut", "kept", "holes", "blend", "single", "double"):
        case = out / mode
        case.mkdir(exist_ok=True)
        scene, pages = case / "scene.glb", case / "pages"
        fixture(cube, mode, scene)
        run(
            [
                sys.executable,
                HERE / "cook-pages.py",
                scene,
                pages,
                "--visual-storage",
                "demand",
            ],
            case / "cook.log",
        )
        records[mode] = {}
        for renderer in ("scene", "paged", "culled"):
            frame = case / f"{renderer}.png"
            if renderer == "scene":
                options = [
                    "--scene",
                    scene,
                    "--replay",
                    flight,
                    "--actor",
                    "capsule",
                    "--physics",
                    "rapier",
                    "--tick",
                    "8",
                ]
            else:
                options = [
                    "--paging",
                    "--pages",
                    pages,
                    "--replay",
                    flight,
                    "--route",
                    "street",
                    "--all-resident",
                    "--gpu-trace",
                    case / f"{renderer}.json",
                    "--out",
                    case / f"{renderer}-reads.json",
                    "--gpu-owner",
                    "1",
                    "--gpu-groups",
                    "on" if renderer == "culled" else "off",
                ]
            run([exe, *options, "--shot", frame], case / f"{renderer}.log")
            _, pixels = rgb(frame)
            frames[mode, renderer] = pixels
            records[mode][renderer] = {"rgb_sha256": hashlib.sha256(pixels).hexdigest()}
        count = different(frames[mode, "paged"], frames[mode, "culled"])
        assert count == 0, (mode, "culling changed the picture", count)
        assert frames[mode, "scene"] == frames[mode, "paged"], (
            mode,
            "tile colours differ from the scene",
        )
        groups = json.loads((case / "culled.json").read_text())["frames"][-1][
            "group_visibility"
        ]
        visible = groups["watched_primitives_after"]
        records[mode]["visible_rear_primitives"] = visible
        if mode in ("cut", "holes", "blend", "single", "double"):
            assert visible == 1, (
                mode,
                "neighbour behind non-solid wall was removed",
                groups,
            )
        else:
            assert visible == 0, (
                mode,
                "solid positive control did not occlude",
                groups,
            )
    for renderer in ("scene", "paged", "culled"):
        opaque = frames["opaque", renderer]
        for mode in ("kept", "cut", "holes", "blend"):
            changed = different(opaque, frames[mode, renderer])
            records[mode][renderer]["pixels_changed_from_opaque"] = changed
            if mode == "kept":
                assert changed == 0, (mode, renderer, changed)
            else:
                assert changed > 100, (mode, renderer, changed)
        for a, b in (("cut", "holes"), ("blend", "holes"), ("blend", "cut")):
            assert different(frames[a, renderer], frames[b, renderer]) > 100, (
                a,
                b,
                renderer,
            )
        single, double = (frames[m, renderer] for m in ("single", "double"))
        changed = different(single, double)
        records["double"][renderer]["pixels_changed_from_single"] = changed
        centre = (90 * 320 + 160) * 3
        assert (
            changed > 100
            and single[centre + 1] > single[centre]
            and double[centre] > double[centre + 1]
        ), renderer
    gpu = json.loads((out / "opaque/culled.json").read_text())
    with exe.open("rb") as file:
        binary = hashlib.file_digest(file, "sha256").hexdigest()
    report = {
        "binary_sha256": binary,
        "viewport": replay["viewport"],
        "adapter": {k: gpu[k] for k in ("adapter", "backend", "driver")},
        "cases": records,
    }
    (out / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    print(
        f"{len(records)} cases and {len(frames)} captures passed. See {out / 'summary.json'}"
    )


if __name__ == "__main__":
    main()
