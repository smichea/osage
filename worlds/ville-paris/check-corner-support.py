#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Compare persistent contact, member poses and the complete stereo recording during terrain handoff."""

import argparse
import hashlib
import json
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import zlib

from paging_support_storage import unpack_support
from paging_checks import require

HERE = Path(__file__).resolve().parent
BENCH = runpy.run_path(str(HERE / "check-paged-bonnet.py"))
AUDIO = runpy.run_path(str(HERE / "check-paged-impact.py"))
CONVERT = BENCH["RIG"]["CONVERTER"]


def variant(pages, directory, modify):
    index = json.loads((pages / "index.json").read_bytes())
    descriptor = index["support"]
    data = json.loads(unpack_support((pages / descriptor["file"]).read_bytes()))
    modify(data)
    raw = json.dumps(data, separators=(",", ":")).encode()
    packed = zlib.compress(raw, 9)
    digest = hashlib.sha256(packed).hexdigest()
    for path in pages.iterdir():
        if path.name != "index.json":
            (directory / path.name).symlink_to(path)
    name = digest + ".zlib"
    target = directory / name
    if not target.exists():
        target.write_bytes(packed)
    index["support"] = {"file": name, "sha256": digest, "bytes": len(packed), "decoded_bytes": len(raw)}
    (directory / "index.json").write_text(json.dumps(index))


def rejection(binary, directory, pages, fixture, name, edit, cause):
    with tempfile.TemporaryDirectory(dir=directory) as temp:
        changed = Path(temp)
        variant(pages, changed, edit)
        result = subprocess.run([binary, "--paging", "--all-resident", "--pages", changed, "--replay", fixture,
                                 "--route", "parked", "--out", directory / (name + ".json")], capture_output=True)
        require(result.returncode != 0 and cause in result.stderr.decode(), f"{name}: {result.stderr.decode()}")
    return cause


def sound(binary, out, source, fixture, pages):
    replay = json.loads(fixture.read_bytes())
    out.mkdir(exist_ok=True)
    results = {}
    for take in replay["routes"]:
        route = out / (take["id"] + "-route.json")
        route.write_text(json.dumps(dict(replay, routes=[dict(take, id="drop")])))
        for actor in ("capsule", "human"):
            label = take["id"] + "-" + actor
            reference, original = AUDIO["resident"](
                binary, out, source, route, "rapier", label=label + "-resident", actor=actor,
            )
            trace, rendered = AUDIO["paged"](
                binary, out, label + "-streamed", pages, route, "--actor", actor,
                "--acoustic-delay-ms", "50", all_resident=False,
            )
            events = len(reference["onsets"])
            require(events > 0 and len(trace["impact_audio"]["contacts"]) == events, f"{label}: missing sound")
            require(original == rendered and any(rendered), f"{label}: the full stereo recordings differ")
            results[label] = {"events": events, "stereo_frames": len(rendered) // 4,
                              "pcm_sha256": hashlib.sha256(rendered).hexdigest()}
    return results


def run(binary, out, pages):
    out.mkdir(parents=True, exist_ok=True)
    source = HERE / "fixtures" / "bir-hakeim.glb"
    fixture = HERE / "fixtures" / "bonnet.json"
    if pages is None:
        pages = out / "district"
        runpy.run_path(str(HERE / "cook-pages.py"))["cook"](source, pages, 128)
    pages = pages.resolve()
    rig = out / "pedestrian.glb"
    BENCH["RIG"]["run"](
        sys.executable, CONVERT / "convert.py", "--id", "pedestrian",
        "--fragment", CONVERT / "fixtures" / "pedestrian.yft.xml", "-o", rig,
    )
    summary = {}
    for name, actor in [("capsule", ["--actor", "capsule"]), ("human", ["--actor", "human"]),
                        ("imported", ["--actor-rig", rig])]:
        meta, ref = BENCH["resident"](binary, source, fixture, actor, out / (name + "-resident"))
        for case in ["parked", "moving"]:
            held = ["--watch", "0,0", "--hold", "physics", "--hold-ticks", "90", "--delay-ms", "0"]
            for mode, opts in [("all", ["--all-resident"]), ("streamed", []), ("held", held)]:
                label = f"{name}-{case}-{mode}"
                trace = BENCH["RIG"]["paged"](binary, pages, fixture, case, actor, opts, out / (label + ".json"))
                row = BENCH["inspect"](trace, ref[case], meta, case, name)
                stats = trace["support"]["stats"]
                require(stats["complete_corner_meshes"] > 0, "no complete support")
                require(row["actor_pose_sha256"] == row["monolithic_actor_sha256"]
                        and row["box_pose_sha256"] == row["monolithic_box_sha256"],
                        f"{label}: {row['peak_actor_center_error_m']} m actor, {row['peak_box_center_error_m']} m box")
                if mode == "held":
                    require(any(e["tile"] == [0, 0] and e["state"] == "ready" and e["tick"] > 90
                                for e in trace["physics_events"]), "expected terrain delivery after impact")
                    require(stats["retained_certified_colliders"] > 0 and stats["held_ticks"] == 0,
                            "delivery did not preserve existing support")
                summary[label] = {"actor_sha256": row["actor_pose_sha256"], "box_sha256": row["box_pose_sha256"],
                                  "first_contact": row["paged_first_bonnet"], "support": stats}
    with tempfile.TemporaryDirectory(dir=out) as temp:
        old = Path(temp)
        variant(pages, old, lambda data: data.pop("corner_meshes"))
        trace = BENCH["RIG"]["paged"](binary, old, fixture, "parked", ["--actor", "human"], [],
                                      out / "partial-human-parked.json")
        meta, reference = BENCH["read_resident"](out / "human-resident")
        row = BENCH["inspect"](trace, reference["parked"], meta, "parked", "human")
        require(trace["support"]["stats"]["complete_corner_meshes"] == 0
                and row["peak_actor_center_error_m"] > 0.01, "partial support control did not diverge")
        summary["partial-control"] = row
    def bad_index(data):
        data["corner_meshes"]["patterns"][0][0][0] = 8

    def reverse_face_order(data):
        pattern = data["corner_meshes"]["patterns"][0]
        # Two vertical triangles have the same omitted bounds before and after the reordering.
        sides = [i for i, face in enumerate(pattern[1])
                 if len({(pattern[0][v] >> 1) & 1 for v in face}) > 1]
        a, b = sides[:2]
        pattern[1][a], pattern[1][b] = pattern[1][b], pattern[1][a]
    summary["rejected"] = [
        rejection(binary, out, pages, fixture, "bad-index", bad_index, "invalid collision corner pattern"),
        rejection(binary, out, pages, fixture, "wrong-order", reverse_face_order,
                  "full collision differs from certified corner mesh"),
    ]
    summary["sound"] = sound(binary, out / "audio", source, fixture, pages)
    summary["binary_sha256"] = BENCH["RIG"]["sha"](binary)
    (out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print("18 exact four-second trajectories, 4 exact stereo recordings, partial-support control and 2 rejections")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--pages", type=Path)
    args = parser.parse_args()
    run(args.bin.resolve(), args.out.resolve(), args.pages)
