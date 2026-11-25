#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Check exterior diffraction around closed meshes and isolation across a closed surface."""

import argparse
import itertools
import json
import math
import runpy
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
fixture = runpy.run_path(str(HERE / "check-impact-diffraction.py"))
common = fixture["common"]
SOURCE, EYE, RATE = (fixture[k] for k in ("SOURCE", "EYE", "RATE"))


def prism(polygon):
    lower = [(x, y, 0) for x, y in polygon]
    upper = [(x, y, 25) for x, y in polygon]
    return [lower[::-1], upper] + [
        [lower[i], lower[(i + 1) % len(lower)], upper[(i + 1) % len(lower)], upper[i]]
        for i in range(len(lower))
    ]


def rectangle(x0, x1):
    return prism([(x0, 2), (x1, 2), (x1, 3), (x0, 3)])


def exact(corners):
    xy = [SOURCE[:2], *corners, EYE[:2]]
    legs = [math.dist(a, b) for a, b in itertools.pairwise(xy)]
    dz = EYE[2] - SOURCE[2]
    points = [
        (*xy[i], SOURCE[2] + dz * sum(legs[:i]) / sum(legs))
        for i in range(1, len(xy) - 1)
    ]
    return math.hypot(sum(legs), dz), points


def rotate(p, angle=0.61):
    # Rotate the entire obstacle about the source-listener axis, preserving all lengths.
    axis = [(y - x) / math.dist(SOURCE, EYE) for x, y in zip(SOURCE, EYE, strict=True)]
    v = [x - y for x, y in zip(p, SOURCE, strict=True)]
    dot = sum(a * b for a, b in zip(axis, v, strict=True))
    cross = [
        axis[(i + 1) % 3] * v[(i + 2) % 3] - axis[(i + 2) % 3] * v[(i + 1) % 3]
        for i in range(3)
    ]
    c, s = math.cos(angle), math.sin(angle)
    return [
        SOURCE[i] + c * v[i] + s * cross[i] + (1 - c) * dot * axis[i] for i in range(3)
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    parser.add_argument(
        "--reference", type=Path, help="city binary with sheet diffraction only"
    )
    parser.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/acoustic-solid"
    )
    parser.add_argument(
        "--many",
        type=int,
        default=32,
        help="distant solids grouped under the same mesh binding",
    )
    args = parser.parse_args()
    if not 0 <= args.many <= 1800:
        parser.error("--many must be between 0 and 1800")
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    asset, replay = runpy.run_path(
        str(ROOT / "tools/object-convert/test_hierarchy.py")
    )["sample"](out)
    base, chunk = common["read_glb"](asset)
    r = json.loads(replay.read_text())
    for k in r["routes"][0]["camera"]:
        k["eye"] = list(EYE)
        k["aim"] = [EYE[0], EYE[1], EYE[2] - 1]
    route = out / "route.json"
    route.write_text(json.dumps(r))
    cases = {}

    def run(
        name,
        faces,
        corners=None,
        *,
        grouped=False,
        reverse=False,
        rotation=0.0,
        transparent=False,
    ):
        if rotation:
            faces = [[rotate(p, rotation) for p in face] for face in faces]
        scene = out / (name + ".glb")
        fixture["write_geometry"](
            base, chunk, scene, faces, [int(transparent)] * 3, grouped, reverse
        )
        opts = [
            "--scene",
            scene,
            "--replay",
            route,
            "--route",
            "drop",
            "--physics",
            "jolt",
            "--actor",
            "capsule",
            "--listening",
            "headphones",
        ]
        report, pcm = common["render"](args.bin, out, name, opts)
        (event,) = report["onsets"]
        edge = event["diffracted_path"]
        peak = max(map(abs, pcm))
        assert event["point_m"] == list(SOURCE) and event["emission_frame"] == 31200
        if corners:
            length, points = exact(corners)
            if rotation:
                points = [rotate(p, rotation) for p in points]
            assert edge and len(edge["bends"]) == len(points), (name, edge)
            assert peak > 0 and event["direct_path"]["transmission_energy"] == [0.0] * 3
            assert abs(edge["distance_m"] - length) < 1e-5, (
                name,
                edge["distance_m"],
                length,
            )
            for bend, point in zip(edge["bends"], points, strict=True):
                assert math.dist(bend["point_m"], point) < 3e-5, (name, bend, point)
                wedge = bend["exterior_wedge"]
                assert (
                    bend["owner_path"][0]
                    == wedge["adjacent_owner_path"][0]
                    == "room-screens"
                )
                assert all(abs(math.hypot(*n) - 1) < 1e-9 for n in wedge["normals"])
                if not grouped:
                    assert bend["surface"] != wedge["adjacent_surface"]
            at = 31200 + round(length * RATE / 343)
            assert edge["arrival_frame"] == event["first_path_frame"] == at
            assert at <= report["first_nonzero_pcm_frame"] < at + 100
        else:
            assert edge is None and ((peak > 0) == transparent), (name, edge)
        prior_peak, prior_length = None, None
        if args.reference:
            prior, old = common["render"](args.reference, out, name + "-previous", opts)
            prior_peak = max(map(abs, old))
            old_path = prior["onsets"][0]["diffracted_path"]
            prior_length = old_path["distance_m"] if old_path else None
            if name == "other-sheet-one-mesh":
                assert prior_peak > 0 and prior_length > edge["distance_m"]
            else:
                assert (prior_peak > 0) == transparent, name
        cases[name] = {
            "previous_peak": prior_peak,
            "previous_path_m": prior_length,
            "peak": peak,
            "first_pcm": report["first_nonzero_pcm_frame"],
            "edge": edge,
            "sha256": common["digest"](out / (name + ".wav")),
        }
        return pcm

    one, two = rectangle(7.4, 9.5), rectangle(4, 11)
    run("one-corner", one, [(9.5, 2)])
    expected = [(11, 2), (11, 3)]
    wave = run("two-corners", two, expected)
    assert run("two-corners-one-mesh", two, expected, grouped=True) == wave
    assert (
        run(
            "two-corners-inverted", [face[::-1] for face in two], expected, reverse=True
        )
        == wave
    )
    other = [[(x + 50, y, z) for x, y, z in face] for face in two]
    assert run("two-disjoint-one-mesh", two + other, expected, grouped=True) == wave
    # A separate sheet cuts the leg that follows the right face of the box.
    # Under one binding, its triangles still belong to a different component.
    blocker = fixture["quad"](10.5, 30, 0, 25, 2.5)
    run("other-sheet-on-side", two + [blocker], [(4, 2), (4, 3)])
    run("other-sheet-one-mesh", two + [blocker], [(4, 2), (4, 3)], grouped=True)
    run("two-corners-oblique", two, expected, rotation=0.61)
    for i, angle in enumerate((-1.41, -0.31, 0.14, 1.43, 2.46)):
        run(f"oblique-{i}", two, expected, rotation=angle, grouped=bool(i % 2))
    run(
        "triangular-prism",
        prism([(4, 2), (11, 2), (10.6, 3.2)]),
        [(11, 2), (10.6, 3.2)],
    )
    run("inside", fixture["room"]())
    run("inside-one-owner", fixture["room"](), grouped=True)
    run("transparent", two, transparent=True)
    if args.many:
        distant = [
            [(x + 50 * (i + 1), y, z) for x, y, z in face]
            for i in range(args.many)
            for face in two
        ]
        assert (
            run("distant-solids-one-mesh", two + distant, expected, grouped=True)
            == wave
        )
        alone = cases["two-corners-one-mesh"]["edge"]
        filled = cases["distant-solids-one-mesh"]["edge"]
        for field in ("single_edges_tested", "edge_pairs_tested"):
            assert filled[field] == alone[field], (field, alone[field], filled[field])
    summary = {
        "binary_sha256": common["digest"](args.bin),
        "reference_sha256": common["digest"](args.reference)
        if args.reference
        else None,
        "cases": cases,
    }
    (out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(
        f"{len(cases)} routes, two bends {cases['two-corners']['edge']['distance_m']:.6f} m, first PCM {cases['two-corners']['first_pcm']}, peak {cases['two-corners']['peak']}"
    )


if __name__ == "__main__":
    main()
