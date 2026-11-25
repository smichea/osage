#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Compare silent and audible routes with separated screens and split convex rooms."""
import argparse
import copy
import json
import os
import runpy
import statistics
from pathlib import Path

HERE = Path(__file__).resolve().parent
AUDIO = runpy.run_path(str(HERE / "check-acoustic-surface-index.py"))
SOLID = runpy.run_path(str(HERE / "check-impact-solids.py"))
GEOMETRY = SOLID["fixture"]
GLB = GEOMETRY["common"]


def rooms(out, count):
    base, chunk = GLB["read_glb"](out / "object.glb")
    path = out / "rooms.glb"
    GEOMETRY["write_geometry"](base, chunk, path, SOLID["rectangle"](4, 11), [0.] * 3, False, False)
    doc, chunk = GLB["read_glb"](path)
    root = doc["scenes"][doc["scene"]]["nodes"][-1]
    walls = [doc["nodes"][i] for i in doc["nodes"][root]["children"]]
    for i in range(1, count):
        room = {
            "translation": [100 + 32 * (i % 32), 0, 100 + 32 * (i // 32)],
            "extras": {"osage": {"id": f"room-{i}"}},
            "children": [],
        }
        doc["nodes"][root]["children"].append(len(doc["nodes"]))
        doc["nodes"].append(room)
        for source in walls:
            node = copy.deepcopy(source)
            profile = node["extras"]["osage"]
            profile["id"] = f"room-{i}-{profile['id']}"
            profile["acoustic"][0]["id"] = profile["id"]
            room["children"].append(len(doc["nodes"]))
            doc["nodes"].append(node)
    GLB["write_glb"](path, doc, chunk)
    # Keep the floor and impact as before and move every acoustic wall away.
    doc["nodes"][root]["translation"] = [10000, 0, 0]
    clear = out / "clear.glb"
    GLB["write_glb"](clear, doc, chunk)
    return path, clear


def two_boxes(out):
    def box(center):
        corners = [[center[0] + dx * .18, center[1] + dy * .16, center[2] + dz * .2]
                   for dz in (-1, 1) for dy in (-1, 1) for dx in (-1, 1)]
        return [[corners[i] for i in side] for side in
                [[0,1,3,2], [4,6,7,5], [0,4,5,1], [2,3,7,6], [0,2,6,4], [1,5,7,3]]]
    # Interleaved bindings test shell ordering independently of region discovery.
    pairs = zip(box([7.75, 1.75, 11.25]), box([9.25, 3.25, 13.75]), strict=True)
    faces = [wall for pair in pairs for wall in pair]
    base, chunk = GLB["read_glb"](out / "object.glb")
    path = out / "two-boxes.glb"
    GEOMETRY["write_geometry"](base, chunk, path, faces, [0.05] * 3, False, False)
    return path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, required=True)
    parser.add_argument("--reference", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--rounds", type=int, default=5)
    parser.add_argument("--rooms", type=int, default=256)
    parser.add_argument("--cpu", type=int, default=min(os.sched_getaffinity(0)))
    args = parser.parse_args()
    if args.rounds < 1 or args.rooms < 2:
        parser.error("at least one trial and two rooms required")
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    screen, route = AUDIO["prepare"](out, 1024)
    room, clear = rooms(out, args.rooms)
    boxes = two_boxes(out)
    data = json.loads(route.read_bytes())
    silent = out / "one-tick.json"
    data["routes"][0]["ticks"] = data["routes"][0]["camera"][-1]["tick"] = 1
    silent.write_text(json.dumps(data))
    # The impact is confirmed in each audible case. Its floor intersects an endpoint
    # and can enter the triangle broad phase without being an interior crossing.
    cases = [
        ("silent", screen, silent, False, (0, 0, 0)),
        ("clear", clear, route, False, (0, 0, 0)),
        ("direct", room, route, True, (0, 0, 0)),
        ("screen", screen, route, False, (1, 1, 128)),
        ("room", room, route, False, (1, 6, 12)),
        ("two-boxes", boxes, route, False, (2, 12, 24)),
    ]
    summary = {"reference_sha256": AUDIO["sha"](args.reference), "binary_sha256": AUDIO["sha"](args.bin),
               "rooms": args.rooms, "rounds": args.rounds, "cases": {}}
    for label, scene, replay, direct, counts in cases:
        costs = {"prior": [], "current": []}
        expected = None
        for i in range(args.rounds):
            builds = [("prior", args.reference), ("current", args.bin)]
            for name, binary in (builds if i % 2 == 0 else reversed(builds)):
                payload, index, cost = AUDIO["record"](binary.resolve(), scene, replay, out, label+"-"+name, direct, args.cpu)
                if expected is None:
                    expected = payload
                if payload != expected:
                    raise AssertionError(f"{label}/{name}: different contact, path, frame or stereo PCM")
                if name == "current":
                    topology = index["solid_topology"]
                    if tuple(topology[k] for k in ("regions", "surfaces", "triangles")) != counts:
                        raise AssertionError(f"{label}: unexpected certified topology {topology}")
                costs[name].append(cost)
        report = expected["report"]
        if label == "silent":
            assert not report["onsets"] and report["first_nonzero_pcm_frame"] is None and report["frames"] == 800
        else:
            event, = report["onsets"]
            if label == "room":
                assert len(event["diffracted_path"]["bends"]) == 2 and report["first_nonzero_pcm_frame"] is not None
            if label == "two-boxes":
                assert len({c["id"] for c in event["direct_path"]["crossings"]}) == 4
            if label == "clear":
                assert event["direct_path"]["crossings"] == [] and report["first_nonzero_pcm_frame"] is not None
        medians = {name: {k: statistics.median(s[k] for s in series) for k in series[0]} for name, series in costs.items()}
        summary["cases"][label] = {"scene_sha256": AUDIO["sha"](scene), "solid_topology":topology,
                                   "wav_sha256": expected["wav_sha256"], "frames": report["frames"],
                                   "median": medians, "trials": costs}
        (out / "summary.json").write_text(json.dumps(summary, indent=2)+"\n")
        print(label, topology, medians, flush=True)


if __name__ == "__main__":
    main()
