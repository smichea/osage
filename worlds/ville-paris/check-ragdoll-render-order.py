#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Replay the same airborne mannequin and compare the resident and streamed image at first reaction."""

import argparse
import csv
import hashlib
import json
import math
import runpy
import struct
from pathlib import Path

from paging_checks import require

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
REFERENCE = runpy.run_path(str(HERE / "check-paged-actor-render.py"))
TICK = 39


def digest(path):
    with path.open("rb") as data:
        return hashlib.file_digest(data, "sha256").hexdigest()


def f32(values):
    return struct.pack(f"<{len(values)}f", *values)


def position_error(a, b):
    groups = ("center", "rotation_xyzw", "linear_velocity", "angular_velocity")
    different = [
        i
        for i, (p, q) in enumerate(zip(a, b, strict=True))
        if any(f32(p[k]) != f32(q[k]) for k in groups)
    ]
    return {
        "different_bodies": different,
        "max_center_error_m": max(
            math.dist(
                struct.unpack("<3f", f32(p["center"])),
                struct.unpack("<3f", f32(q["center"])),
            )
            for p, q in zip(a, b, strict=True)
        ),
    }


def resident_poses(exe, out, tag, source, replay):
    folder = out / (tag + "-states")
    REFERENCE["log_run"](
        out,
        tag + "-states",
        [
            exe,
            "--scene",
            source,
            "--replay",
            replay,
            "--actor",
            "human",
            "--physics",
            "rapier",
            "--bench",
            folder,
        ],
    )
    with (folder / "dynamics.csv").open() as stream:
        row = next(r for r in csv.DictReader(stream) if int(r["tick"]) == TICK)
    keys = ("center", "rotation_xyzw", "linear_velocity", "angular_velocity")
    fields = (
        ("cx", "cy", "cz"),
        ("qx", "qy", "qz", "qw"),
        ("vx", "vy", "vz"),
        ("wx", "wy", "wz"),
    )
    # CSV emits the shortest decimal round-tripping to its binary32 state.
    root = {
        k: list(struct.unpack(f"<{len(ff)}f", f32([float(row[s]) for s in ff])))
        for k, ff in zip(keys, fields, strict=True)
    }
    return [root, *json.loads(row["parts_json"])]


def run(previous, current, out, lock):
    out.mkdir(parents=True, exist_ok=True)
    source, first_route = REFERENCE["SAMPLE"](out)
    pages = out / "district"
    REFERENCE["COOK"](source, pages, 128)
    route = REFERENCE["read"](first_route)
    route.update(viewport=REFERENCE["SIZE"], vertical_fov_deg=REFERENCE["FOV"])
    initial, _ = REFERENCE["write_replay"](
        out, route, "initial", route["routes"][0]["start_m"]
    )
    old = REFERENCE["motion"](
        previous, out, pages, initial, "drop", None, "initial-old"
    )
    center = old["body_before_step"][TICK]["parts_after_step"][0]["center"].copy()
    center[1] += 0.12
    replay, camera = REFERENCE["write_replay"](out, route, "matched", center)
    report = {
        "tick": TICK,
        "number_of_pixels": math.prod(REFERENCE["SIZE"]),
        "camera": camera,
        "source_sha256": digest(source),
        "index_sha256": digest(pages / "index.json"),
        "replay_sha256": digest(replay),
    }
    images = {}
    for tag, exe in (("before", previous), ("after", current)):
        solved = REFERENCE["motion"](exe, out, pages, replay, "drop", None, tag)
        hit = solved["first_contact"]
        require(
            hit["tick"] == TICK
            and hit["support"] == "west-floor"
            and hit["substep"] == 2,
            "first contact changed in the order control",
        )
        poses = solved["body_before_step"][TICK]["parts_after_step"]
        resident = resident_poses(exe, out, tag, source, replay)
        for mode in ("resident", "traced", "plain"):
            images[tag, mode] = REFERENCE["render"](
                exe,
                lock,
                out,
                tag + "-" + mode,
                mode,
                source,
                pages,
                replay,
                "drop",
                TICK,
                None,
            )
        same_time = REFERENCE["comparison"](
            images[tag, "resident"], images[tag, "traced"]
        )
        report[tag] = {
            "binary_sha256": digest(exe),
            "first_contact": hit,
            "paged_vs_resident_pose": position_error(poses, resident),
            "paged_vs_resident_pixels": same_time,
            "actor_region": REFERENCE["actor_region"](
                poses,
                solved["actor"]["parts"],
                camera,
                images[tag, "traced"],
                images[tag, "resident"],
            ),
        }
        require(
            images[tag, "traced"] == images[tag, "plain"],
            "traced and normal shots at the same tick diverge",
        )
    report["before_after"] = {
        k: REFERENCE["comparison"](images["before", k], images["after", k])
        for k in ("resident", "traced")
    }
    (out / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    require(
        report["before"]["paged_vs_resident_pixels"]["over_two"] > 20,
        "the previous native discrepancy is absent",
    )
    require(
        not report["after"]["paged_vs_resident_pose"]["different_bodies"],
        "the fifteen post-solve states differ",
    )
    require(
        report["after"]["paged_vs_resident_pixels"]["max_channel_delta"] <= 1,
        "natural poses at first contact still change the image beyond quantization",
    )
    print(json.dumps(report, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--previous", type=Path, required=True)
    parser.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    parser.add_argument(
        "--out",
        type=Path,
        default=ROOT / "bench/out/ville-paris/terrain-order/natural-picture",
    )
    parser.add_argument(
        "--gpu-lock", type=Path, default=Path("/tmp/osage-ville-paris-gpu.lock")
    )
    a = parser.parse_args()
    run(a.previous.resolve(), a.bin.resolve(), a.out.resolve(), a.gpu_lock)


if __name__ == "__main__":
    main()
