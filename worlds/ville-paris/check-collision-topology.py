#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Compare ordered source triangles to district geometry and drop capsules onto a distant triangle tip."""

import argparse
import json
import math
import runpy
import subprocess
from collections import Counter
from pathlib import Path

from paging_checks import require
from paging_collision_codec import read_collision

HERE = Path(__file__).resolve().parent
AUDIO = runpy.run_path(str(HERE / "check-paged-impact.py"))


def prepare(out):
    source, replay = AUDIO["SAMPLE"](out)
    edge = AUDIO["EDGE"]
    glb = edge["common"]
    base, chunk = glb["read_glb"](source)
    # Eighty small loading-ramp triangles share their mesh with one long triangular terrace.
    quads = [
        [(x, 3, 12), (x, 3, 16), (x + 4, 3, 16), (x + 4, 3, 12)]
        for x in range(-80, 80, 4)
    ]
    terrace = [(-113, 5, -4), (-113, 5, 0), (91, 5, -4)]
    scene = out / "ramps.glb"
    edge["write_geometry"](base, chunk, scene, [*quads, terrace], [0.5] * 3, True, False)
    tree, chunk = glb["read_glb"](scene)
    node = tree["nodes"][-1]
    node["scale"] = [-1, 1, 1]
    binding = node["extras"]["osage"]
    binding["collision"] = [
        {"id": "ramp", "material": "surface-7", "mesh": binding["acoustic"][0]["mesh"]}
    ]
    glb["write_glb"](scene, tree, chunk)
    faces = [
        face for a, b, c, d in quads for face in [(a, b, c), (a, c, d)]
    ] + [terrace]
    # The single reflection changes the triangle order once at publication.
    expected = [tuple((-x, y, z) for x, y, z in (a, c, b)) for a, b, c in faces]
    material = tree["asset"]["extras"]["osageObject"]["collisionMaterials"]["surface-7"]
    return scene, AUDIO["read"](replay), expected, len(tree["nodes"]) - 1, material


def audit(pages, faces, owner, material):
    index = AUDIO["read"](pages / "index.json")
    seen = []
    weights = {}
    terrace_page = None
    for tile in index["tiles"]:
        layer = tile.get("physics")
        if not layer:
            continue
        page = (tile["x"], tile["z"])
        for slot, shape in enumerate(read_collision(pages, layer)):
            if shape["id"] != "ramp":
                continue
            require(shape["source_node"] == layer["source_nodes"][slot] == owner, "lost source tree owner")
            require(layer["materials"][slot] == material,
                    "missing physical material")
            points = shape["vertices"]
            triangles = [tuple(tuple(points[i]) for i in tri) for tri in shape["triangles"]]
            require(triangles == sorted(triangles, key=faces.index), "source face order changed inside page")
            require(all(all(layer["bounds"][0][a] <= p[a] <= layer["bounds"][1][a] for a in range(3))
                        for p in points), "page excludes its own vertices")
            if faces[-1] in triangles:
                terrace_page = page
            weights[page] = len(triangles)
            seen.extend(triangles)
    require(len(weights) > 1 and Counter(seen) == Counter(faces), "missing, split or duplicated source triangles")
    require(terrace_page is not None, "missing distant terrace")
    return terrace_page, weights


def run(binary, out):
    out.mkdir(parents=True, exist_ok=True)
    source, replay, faces, owner, material = prepare(out)
    pages = out / "pages"
    AUDIO["COOK"](source, pages, 32)
    key, weights = audit(pages, faces, owner, material)
    results = {}
    for name, x, z, height, expected in [
        ("far-tip", 108, -3, 5, True), ("ramp-left", -74, 14, 3, True),
        ("ramp-right", 74, 14, 3, True), ("off-terrace", -85, -0.2, 5, False),
    ]:
        cell = tuple(math.floor(v / 32 + 0.5) for v in (x, z))
        if name == "far-tip":
            require(cell != key, "the probe must land beyond the triangle's storage cell")
        take = dict(replay["routes"][0], start_m=[x, height + 2, z],
                    expected_support="ramp", ticks=53)
        take["camera"] = [dict(k, tick=t) for k, t in zip(take["camera"], [0, 53], strict=True)]
        route = out / (name + "-route.json")
        route.write_text(json.dumps(dict(replay, routes=[take])))
        paths = {}
        for mode, args in [
            ("all", ["--all-resident"]), ("forecast", ["--delay-ms", "50"]),
            ("withheld", ["--watch", f"{key[0]},{key[1]}", "--hold", "physics", "--hold-ticks", "53"]),
        ]:
            receipt = out / f"{name}-{mode}.json"
            subprocess.run([binary, "--paging", "--pages", pages, "--replay", route, "--route", "drop",
                            "--out", receipt, *args], capture_output=True, check=True)
            row = AUDIO["read"](receipt)
            hit = row["first_contact"]
            require(bool(hit) == expected, f"{name}/{mode}: unexpected contact")
            if hit:
                require(hit["support"] == "ramp" and hit["source_node"] == owner, "false owner")
                require(abs(hit["point"][1] - height) < 0.01, "bad triangle height")
                if name == "far-tip":
                    require(tuple(hit["tile"]) == key, "resolved to another file")
                    if mode == "withheld":
                        require(hit["geometry"] == "certified_support", "not testing a late file")
            paths[mode] = hit
        if expected:
            require(len({h["tick"] for h in paths.values()}) == 1, "delivery changed first contact time")
        results[name] = {"occupied_cell": cell, "contacts": paths}
    result = {"source_faces": len(faces), "faces_per_page": [[*k, n] for k, n in sorted(weights.items())],
              "terrace_storage_cell": key, "runs": results}
    (out / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
    print(f"{len(faces)} exact source faces in {len(weights)} pages, 12 drop controls")


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--bin", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    args = ap.parse_args()
    run(args.bin.resolve(), args.out.resolve())
