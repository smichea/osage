#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Compare native fixed triangle-shape placement, materials and full simulation traces on repeated original geometry."""

import argparse
import copy
import csv
import hashlib
import json
import os
import runpy
import statistics
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
RAYS = runpy.run_path(str(HERE / "check-object-ray-scaling.py"))
GLB = RAYS["GLB"]


def make_scene(out, count, varied=False):
    source, route, *_ = RAYS["TOPOLOGY"]["prepare"](out)
    target = out / "district.glb"
    width, _ = RAYS["scene"](source, count, target)
    doc, chunk = GLB["read_glb"](target)
    # Keep all 1,024 independent sheets inside the legacy Jolt shape-coordinate limit.
    doc["nodes"][0]["translation"] = [-width * 8, 7, -width * 8]
    if varied:
        doc["asset"]["extras"]["osageObject"]["collisionMaterials"]["elastic"] = {
            "friction": 0.2,
            "restitution": 0.55,
        }
    for node in doc["nodes"][1:]:
        node["translation"] = [n / 16 for n in node["translation"]]
        if "mesh" in node:
            node["scale"] = [1 / 16, 1, 1 / 16]
            surface = node["extras"]["osage"]["collision"][0]
            if varied and int(surface["id"].removeprefix("floor-")) % 2:
                surface["material"] = "elastic"
    GLB["write_glb"](target, doc, chunk)
    template = copy.deepcopy(route)
    routes = []
    for i in sorted({0, 1, count // 2, count - 1}):
        if i >= count:
            continue
        row, col = divmod(i, width)
        for tag, (local_x, local_z), drift in [
            ("face", (-78.75, 14.75), [0, 0, 0]),
            ("edge", (-78, 14), [0, 0, 0]),
            ("drift", (-78.25, 14.25), [0.09, -1, -0.07]),
            ("high", (-78.75, 14.75), [0, 0, 0]),
        ]:
            rt = copy.deepcopy(template["routes"][0])
            x = col * 16 - width * 8 + local_x / 16
            z = row * 16 - width * 8 + local_z / 16
            rt.update(
                id=f"{tag}-{i}",
                start_m=[x, 12, z],
                velocity_m_s=drift,
                ticks=90,
                expected_support=f"floor-{i}",
            )
            if tag == "high":
                rt["start_m"][1], rt["ticks"] = 45, 195
            rt["camera"][-1]["tick"] = rt["ticks"]
            routes.append(rt)
    template["routes"] = routes
    replays = out / "drops.json"
    replays.write_text(json.dumps(template, indent=2) + "\n")
    template["routes"] = routes[-1:]
    one = out / "first.json"
    one.write_text(json.dumps(template, indent=2) + "\n")
    if width >= 5:
        crossing = copy.deepcopy(template)
        r = crossing["routes"][0]
        r.update(
            id="cross-four",
            ticks=120,
            start_m=[-width * 8 - 78.75 / 16, 12, -width * 8 + 14.75 / 16],
            velocity_m_s=[100, 0, 0],
            expected_support="floor-4",
        )
        r["camera"][-1]["tick"] = 120
        (out / "cross.json").write_text(json.dumps(crossing, indent=2) + "\n")
    return target, replays, one


def trace_comparison(old, current):
    original = (old / "dynamics.csv").read_bytes()
    candidate = (current / "dynamics.csv").read_bytes()
    previous = csv.DictReader(original.decode().splitlines())
    now = csv.DictReader(candidate.decode().splitlines())
    rows = 0
    contacts = 0
    max_pos = 0.0
    differing = {}
    for a, b in zip(previous, now, strict=True):
        if (a["route"], a["tick"]) != (b["route"], b["tick"]):
            raise ValueError("different route order")
        rows += 1
        contacts += len(json.loads(a["contacts_json"]))
        if a != b:
            delta = (
                sum((float(a[k]) - float(b[k])) ** 2 for k in ("cx", "cy", "cz")) ** 0.5
            )
            max_pos = max(max_pos, delta)
            detail = differing.setdefault(
                a["route"],
                {
                    "first_tick": int(a["tick"]),
                    "rows": 0,
                    "first_fields": [k for k in a if a[k] != b[k]],
                },
            )
            detail["rows"] += 1
    data = [json.loads((path / "dynamics.json").read_text()) for path in (old, current)]
    if not all(
        out["target_contacted"] for run in data for out in run["run"]["outcomes"]
    ):
        raise ValueError("missing authored support")
    return {
        "state_rows": rows,
        "contact_witnesses": contacts,
        "trace_bytes_same": original == candidate,
        "run_metadata_same": data[0]["run"] == data[1]["run"],
        "trace_sha256": [hashlib.sha256(x).hexdigest() for x in (original, candidate)],
        "differing_routes": differing,
        "max_center_difference_m": max_pos,
        "median_world_build_ms": [
            statistics.median(q["timings"]["world_build_ns"]) / 1e6 for q in data
        ],
    }


def traversal(previous, current, scene, route, out):
    cases = {}
    for eng in ["rapier", "jolt"]:
        runs = []
        for key, exe in [("previous", previous), ("current", current)]:
            dest = out / ("traverse-" + key + "-" + eng)
            subprocess.run(
                [
                    exe.resolve(),
                    "--scene",
                    scene,
                    "--physics",
                    eng,
                    "--replay",
                    route,
                    "--bench",
                    dest,
                ],
                check=True,
                stdout=subprocess.DEVNULL,
            )
            runs.append(dest)
        cases[eng] = trace_comparison(*runs)
    return cases


def child_measure(arguments):
    # A separate small monitor keeps its high-water mark below that of the measured executable.
    with subprocess.Popen(arguments, stdout=subprocess.DEVNULL) as process:
        _, state, usage = os.wait4(process.pid, 0)
        process.returncode = os.waitstatus_to_exitcode(state)
        if process.returncode:
            raise subprocess.CalledProcessError(process.returncode, arguments)
    receipt = json.loads((Path(arguments[-1]) / "dynamics.json").read_text())
    times = receipt["timings"]
    assert len(times["world_build_ns"]) == 1
    return {
        "world_build_wall_ms": times["world_build_ns"][0] / 1e6,
        "process_cpu_ms": (usage.ru_utime + usage.ru_stime) * 1000,
        "peak_rss_kib": usage.ru_maxrss,
        "step_p95_us": times["step_ns"]["p95"] / 1000,
    }


def measure_pairs(previous, current, scene, one, out, rounds):
    core = min(os.sched_getaffinity(0))
    observations = {}
    conditions = [
        (eng, tag, b)
        for eng in ["rapier", "jolt"]
        for tag, b in [("previous", previous), ("current", current)]
    ]
    for i in range(rounds):
        for eng, tag, b in conditions if i % 2 == 0 else reversed(conditions):
            key = eng + "-" + tag
            result_dir = out / ("resource-" + key)
            command = [
                sys.executable,
                __file__,
                "--child-measure",
                "taskset",
                "-c",
                str(core),
                str(b.resolve()),
                "--scene",
                str(scene),
                "--physics",
                eng,
                "--replay",
                str(one),
                "--bench",
                str(result_dir),
            ]
            observations.setdefault(key, []).append(
                json.loads(subprocess.check_output(command, text=True))
            )
    return {
        "cpu": core,
        "rounds": rounds,
        "samples": observations,
        "medians": {
            k: {
                field: statistics.median(row[field] for row in rows)
                for field in rows[0]
            }
            for k, rows in observations.items()
        },
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--count", type=int, default=1024)
    parser.add_argument("--previous", type=Path)
    parser.add_argument("--current", type=Path)
    parser.add_argument("--varied-materials", action="store_true")
    parser.add_argument(
        "--negative",
        action="store_true",
        help="record the deliberately unprotected prototype's differences",
    )
    parser.add_argument(
        "--measure",
        type=int,
        default=0,
        help="alternate fresh profiled processes on the high translated drop",
    )
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    if args.count < 1:
        parser.error("count must be positive")
    scene, replays, one = make_scene(out, args.count, args.varied_materials)
    for name, binary in [("previous", args.previous), ("current", args.current)]:
        if binary:
            for engine in ["rapier", "jolt"]:
                file = out / (name + "-" + engine)
                subprocess.run(
                    [
                        binary.resolve(),
                        "--scene",
                        scene,
                        "--physics",
                        engine,
                        "--replay",
                        replays,
                        "--bench",
                        file,
                    ],
                    check=True,
                )
    if args.previous and args.current:
        comparisons = {
            engine: trace_comparison(
                out / ("previous-" + engine), out / ("current-" + engine)
            )
            for engine in ["rapier", "jolt"]
        }
        files = {}
        for version, exe in [("previous", args.previous), ("current", args.current)]:
            with exe.open("rb") as f:
                files[version] = hashlib.file_digest(f, "sha256").hexdigest()
        receipt = {
            "binary_sha256": files,
            "scene_sha256": hashlib.sha256(scene.read_bytes()).hexdigest(),
            "routes_sha256": hashlib.sha256(replays.read_bytes()).hexdigest(),
            "placements": args.count,
            "engines": comparisons,
        }
        cross = out / "cross.json"
        if args.count >= 17 and cross.is_file():
            receipt["traversal_100_m_s"] = traversal(
                args.previous, args.current, scene, cross, out
            )
            if not args.negative and not all(
                c["trace_bytes_same"] and c["run_metadata_same"]
                for c in receipt["traversal_100_m_s"].values()
            ):
                raise ValueError("crossing the four cells changed the trace")
        if args.measure:
            receipt["resources"] = measure_pairs(
                args.previous, args.current, scene, one, out, args.measure
            )
        (out / "comparison.json").write_text(json.dumps(receipt, indent=2) + "\n")
        for engine, values in comparisons.items():
            if args.negative:
                if values["trace_bytes_same"]:
                    raise ValueError(f"{engine}: negative control failed to diverge")
            else:
                if not (values["trace_bytes_same"] and values["run_metadata_same"]):
                    raise ValueError(
                        f"{engine}: differing native states or materials; see comparison.json"
                    )
                subprocess.run(
                    [
                        args.current.resolve(),
                        "--scene",
                        scene,
                        "--physics",
                        engine,
                        "--replay",
                        replays,
                        "--verify",
                        out / ("previous-" + engine),
                    ],
                    check=True,
                )


if __name__ == "__main__":
    if sys.argv[1:2] == ["--child-measure"]:
        print(json.dumps(child_measure(sys.argv[2:])))
    else:
        main()
