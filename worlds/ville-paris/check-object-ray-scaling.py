#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Measure resident ray queries over repeated placements of the authored 81-triangle ramp, with a single-ray baseline."""

import argparse
import hashlib
import json
import math
import os
import resource
import runpy
import statistics
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
TOPOLOGY = runpy.run_path(str(HERE / "check-collision-topology.py"))
GLB = TOPOLOGY["AUDIO"]["EDGE"]["common"]
TEST = "object::rays::measurement::measure_instanced_rays"
KINDS = ("hit", "remote_miss", "coplanar_row")


def sha(data):
    return hashlib.sha256(data).hexdigest()


def scene(source, count, dest):
    doc, chunk = GLB["read_glb"](source)
    mesh = doc["nodes"][-1]["extras"]["osage"]["collision"][0]["mesh"]
    columns = math.ceil(math.sqrt(count))
    nodes = [
        {
            "translation": [100, 7, -30],
            "children": [],
            "extras": {"osage": {"id": "district"}},
        }
    ]
    for row in range(math.ceil(count / columns)):
        section = {
            "translation": [0, 0, row * 256],
            "children": [],
            "extras": {"osage": {"id": f"row-{row}"}},
        }
        nodes[0]["children"].append(len(nodes))
        nodes.append(section)
        for col in range(columns):
            index = row * columns + col
            if index >= count:
                break
            section["children"].append(len(nodes))
            nodes.append(
                {
                    "translation": [col * 256, 0, 0],
                    "mesh": mesh,
                    "extras": {
                        "osage": {
                            "id": f"placement-{index}",
                            "collision": [
                                {
                                    "id": f"floor-{index}",
                                    "mesh": mesh,
                                    "material": "surface-7",
                                }
                            ],
                            "acoustic": [
                                {
                                    "id": f"sound-{index}",
                                    "mesh": mesh,
                                    "material": "wall",
                                    "source": "authored"
                                    if index % 2
                                    else "collision-proxy",
                                }
                            ],
                        }
                    },
                }
            )
    doc["nodes"], doc["scenes"], doc["scene"] = nodes, [{"nodes": [0]}], 0
    GLB["write_glb"](dest, doc, chunk)
    if GLB["read_glb"](dest)[1] != chunk:
        raise ValueError("model geometry changed")
    return columns, sha(chunk)


def rays(count, columns):
    probes = []
    for kind in KINDS:
        for j in range(16):
            idx = (j * (count - 1)) // 15
            row, col = divmod(idx, columns)
            ox, oy, oz = 100 + col * 256, 7, -30 + row * 256
            if kind == "hit":
                # Strict interior of the first triangle at x=-80...-76 and z=12...16.
                x, z = ox - 78.75, oz + 14.75
                start, end = [x, oy + 6, z], [x, oy - 2, z]
            elif kind == "remote_miss":
                x, z = ox + 42.75, oz + 90.25
                start, end = [x, oy + 6, z], [x, oy - 2, z]
            else:
                last = min(columns, count - row * columns) - 1
                # This finite horizontal segment meets AABBs of the flat row but no triangle.
                start, end = (
                    [100 - 85, oy + 3, oz + 14.25],
                    [100 + last * 256 + 84, oy + 3, oz + 14.25],
                )
            probes.append(
                {"kind": kind, "start": start, "end": end, "placement": idx, "row": row}
            )
    return probes


def cpu_us():
    usage = resource.getrusage(resource.RUSAGE_CHILDREN)
    return 1e6 * (usage.ru_utime + usage.ru_stime)


def command(binary, source, probes, cpu):
    args = [
        "taskset",
        "-c",
        str(cpu),
        binary.resolve(),
        "--scene",
        source.resolve(),
        "--inspect-object",
    ]
    for row in probes:
        args.extend(
            ["--ray", ",".join(map(str, row["start"])), ",".join(map(str, row["end"]))]
        )
    return args


def invoke(binary, source, probes, cpu):
    args = command(binary, source, probes, cpu)
    before = cpu_us()
    clock = time.perf_counter_ns()
    raw = subprocess.check_output(args)
    elapsed = time.perf_counter_ns() - clock
    return json.loads(raw), {
        "process_cpu_ms": (cpu_us() - before) / 1000,
        "wall_ms": elapsed / 1e6,
        "stdout_bytes": len(raw),
        "stdout_sha256": sha(raw.rstrip(b"\n")),
    }


def child_rss(args):
    # Spawn from a fresh small monitor. Forking from the benchmarking process would charge
    # its GLB, serialized scene and binary hashing buffers to the child's initial high-water mark.
    with subprocess.Popen(args, stdout=subprocess.DEVNULL) as child:
        _, status, usage = os.wait4(child.pid, 0)
        child.returncode = os.waitstatus_to_exitcode(status)
        if child.returncode:
            raise subprocess.CalledProcessError(child.returncode, child.args)
    return usage.ru_maxrss


def peak_rss(binary, source, probes, cpu):
    args = [
        sys.executable,
        __file__,
        "--child-peak-rss",
        *command(binary, source, probes, cpu),
    ]
    return int(subprocess.check_output(args, text=True))


def paired_rss(previous, current, source, probes, cpu, rounds):
    cases = [
        ("d173_profile", previous, []),
        ("current_profile", current, []),
        ("d173_batch", previous, probes),
        ("current_batch", current, probes),
    ]
    records = {name: [] for name, _, _ in cases}
    for n in range(rounds):
        for name, binary, rays_to_test in cases if n % 2 == 0 else reversed(cases):
            records[name].append(peak_rss(binary, source, rays_to_test, cpu))
    return {
        name: {
            "peak_kib": values,
            "median_kib": statistics.median(values),
            "min_kib": min(values),
            "max_kib": max(values),
        }
        for name, values in records.items()
    }


def signatures(row):
    return {
        k: [(h["id"], tuple(h["nodePath"]), h["distanceM"]) for h in row[k]]
        for k in ("visual", "collision", "acoustic")
    }


def inspect_native(test, source, paths, rounds, cpu):
    env = {
        **os.environ,
        "OSAGE_OBJECT_RAY_SCENE": str(source.resolve()),
        "OSAGE_OBJECT_RAY_PATHS": str(paths.resolve()),
        "OSAGE_OBJECT_RAY_ROUNDS": str(rounds),
    }
    raw = subprocess.check_output(
        [
            "taskset",
            "-c",
            str(cpu),
            test.resolve(),
            "--exact",
            TEST,
            "--ignored",
            "--nocapture",
        ],
        env=env,
        text=True,
    )
    line = next(x for x in raw.splitlines() if "OSAGE_OBJECT_RAY_TIMING " in x)
    return json.loads(line.partition("OSAGE_OBJECT_RAY_TIMING ")[2])


def median(rows, field):
    return statistics.median(x[field] for x in rows)


def summary(probes, internal):
    metrics, samples = internal["metrics"], internal["rounds"]
    if len(samples) > 1:
        samples = samples[1:]
    per_kind = {}
    for kind in KINDS:
        selected = [
            v
            for p, v in zip(probes, metrics["pruning"], strict=True)
            if p["kind"] == kind
        ]
        per_kind[kind] = {
            "rays": len(selected),
            "tested_triangles_min_max": [
                min(v["tested"] for v in selected),
                max(v["tested"] for v in selected),
            ],
            "tested_sound_triangles_min_max": [
                min(v["sound_tested"] for v in selected),
                max(v["sound_tested"] for v in selected),
            ],
            "bindings_hit_each_channel": [
                max(v["binding_hits"][i] for v in selected) for i in range(3)
            ],
            "median_query_us_per_ray": statistics.median(
                r["query_and_hit_value_cpu_ns"][kind] for r in samples
            )
            / len(selected)
            / 1000,
            "median_sound_query_us_per_ray": statistics.median(
                r["standalone_acoustic_query_cpu_ns"][kind] for r in samples
            )
            / len(selected)
            / 1000,
        }
    return {
        "median_stage_ms": {
            k: median(samples, k) / 1e6
            for k in (
                "load_and_hierarchy_cpu_ns",
                "build_cpu_ns",
                "profile_json_value_cpu_ns",
                "assemble_json_cpu_ns",
                "pretty_print_cpu_ns",
            )
        },
        "rays": per_kind,
    }


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--previous", type=Path, required=True)
    ap.add_argument("--current", type=Path, required=True)
    ap.add_argument("--test-bin", type=Path)
    ap.add_argument(
        "--reference-test-bin",
        type=Path,
        help="repeat native D173/current on the largest fixture in alternating order",
    )
    ap.add_argument(
        "--batch-reference",
        type=Path,
        help="compare every detailed hit with the previous batched inspector",
    )
    ap.add_argument("--counts", type=int, nargs="+", default=[1, 16, 64, 256, 1024])
    ap.add_argument("--rounds", type=int, default=8)
    ap.add_argument(
        "--rss-rounds",
        type=int,
        default=0,
        help="paired fresh process peak RSS on the largest input, using --batch-reference",
    )
    ap.add_argument("--cpu", type=int, default=min(os.sched_getaffinity(0)))
    ap.add_argument("--prepare", action="store_true")
    args = ap.parse_args()
    if (
        any(v < 1 for v in args.counts)
        or args.rounds < 2
        or args.rss_rounds < 0
        or (args.rss_rounds and not args.batch_reference)
        or (args.reference_test_bin and not args.test_bin)
        or args.cpu not in os.sched_getaffinity(0)
    ):
        ap.error("invalid counts, rounds or CPU")
    out = args.out.resolve()
    out.mkdir(exist_ok=True, parents=True)
    sources = out / "source"
    sources.mkdir(exist_ok=True)
    src, *_ = TOPOLOGY["prepare"](sources)
    results = {
        "reference": sha(args.previous.read_bytes()),
        "current": sha(args.current.read_bytes()),
        "source": sha(src.read_bytes()),
        "cpu": args.cpu,
        "records": {},
    }
    if args.batch_reference:
        results["batch_reference"] = sha(args.batch_reference.read_bytes())
    if args.reference_test_bin:
        results["reference_test_binary"] = sha(args.reference_test_bin.read_bytes())
    if args.test_bin:
        results["test_binary"] = sha(args.test_bin.read_bytes())
    for count in args.counts:
        p = out / f"p{count}"
        p.mkdir(exist_ok=True)
        asset = p / "district.glb"
        columns, digest = scene(src, count, asset)
        probes = rays(count, columns)
        paths = p / "rays.json"
        paths.write_text(json.dumps(probes, indent=2) + "\n")
        if args.prepare:
            continue
        batch, timing = invoke(args.current, asset, probes, args.cpu)
        if args.batch_reference:
            earlier, _ = invoke(args.batch_reference, asset, probes, args.cpu)
            for old, new in zip(earlier["rays"], batch["rays"], strict=True):
                if {k: v for k, v in old.items() if k != "testedTriangles"} != {
                    k: v for k, v in new.items() if k != "testedTriangles"
                }:
                    raise ValueError(
                        "source triangle, world point, distance, owner or sort order changed"
                    )

        if len(batch["rays"]) != len(probes) or batch["ray"] != batch["rays"][-1]:
            raise ValueError("batch order or compatibility result differs")
        # Both historical CLIs are sampled separately. The full hierarchy is serialized on every separate execution.
        sample_slots = [0, 7, 15, 16, 31, 32, 47]
        checks = []
        for selected in sample_slots:
            pair = [None, None]
            for idx in (0, 1) if selected % 2 == 0 else (1, 0):
                binary = (args.previous, args.current)[idx]
                current, stamp = invoke(binary, asset, [probes[selected]], args.cpu)
                pair[idx] = stamp
                if signatures(current["ray"]) != signatures(batch["rays"][selected]):
                    raise ValueError(
                        f"legacy single-ray semantics differ for {selected}"
                    )
            checks.append(
                {
                    "slot": selected,
                    "kind": probes[selected]["kind"],
                    "single_legacy": pair[0],
                    "single_current": pair[1],
                }
            )
        empty = {}
        for key, exe in [("previous", args.previous), ("current", args.current)]:
            _, empty[key] = invoke(exe, asset, [], args.cpu)
        row = {
            "asset_sha256": sha(asset.read_bytes()),
            "source_shared_bin_sha256": digest,
            "glb_bytes": asset.stat().st_size,
            "batch": timing,
            "single_controls": checks,
            "profiling_empty_cli": empty,
        }
        for probe, hit in zip(probes, batch["rays"], strict=True):
            if probe["kind"] == "hit":
                i, r = probe["placement"], probe["row"]
                expected = [f"placement-{i}", f"floor-{i}", f"sound-{i}"]
                for name, k in zip(
                    expected, ("visual", "collision", "acoustic"), strict=True
                ):
                    (v,) = hit[k]
                    if (
                        v["id"] != name
                        or v["nodePath"] != ["district", f"row-{r}", f"placement-{i}"]
                        or v["distanceM"] != 3
                        or [h["source_triangle"] for h in v["intersections"]] != [0]
                    ):
                        raise ValueError(
                            f"unexpected owner, distance or ordinal for {k}: {v}"
                        )
            elif any(hit[k] for k in ("visual", "collision", "acoustic")):
                raise ValueError("a negative control intersected a triangle")
        if args.test_bin:
            internal = inspect_native(
                args.test_bin, asset, paths, args.rounds, args.cpu
            )
            m = internal["metrics"]
            if (
                m["pretty_sha256"] != timing["stdout_sha256"]
                or m["pretty_bytes"] + 1 != timing["stdout_bytes"]
            ):
                raise ValueError(
                    "isolated stages did not reconstruct the exact normal inspector stdout"
                )
            if "outer_tree_leaves" in m:
                if (
                    m["outer_tree_leaves"],
                    m["local_tree_leaves"],
                    m["world_fallback_trees"],
                ) != (count, 81, 0):
                    raise ValueError(
                        "new traversal failed to share the proven source mesh"
                    )
            elif m["bvh_leaves"] != 81 * (2 * count + count // 2):
                raise ValueError("unexpected legacy leaf count")
            if (
                m["visual_meshes"] != 1
                or m["owned_acoustic_world_meshes"] != count // 2
            ):
                raise ValueError("unexpected source reuse")
            (p / "native-stages.json").write_text(json.dumps(internal, indent=2) + "\n")
            row.update(
                native_metrics={k: v for k, v in m.items() if k != "pruning"},
                **summary(probes, internal),
            )
        if args.reference_test_bin and count == max(args.counts):
            trials = {"d173": [], "current": []}
            for name, test in [
                ("d173", args.reference_test_bin),
                ("current", args.test_bin),
                ("current", args.test_bin),
                ("d173", args.reference_test_bin),
            ]:
                native = inspect_native(test, asset, paths, args.rounds, args.cpu)
                j = len(trials[name])
                (p / f"native-{name}-{j}.json").write_text(
                    json.dumps(native, indent=2) + "\n"
                )
                trials[name].append(summary(probes, native))
            row["paired_native"] = trials
        if args.rss_rounds and count == max(args.counts):
            row["paired_rss"] = paired_rss(
                args.batch_reference,
                args.current,
                asset,
                probes,
                args.cpu,
                args.rss_rounds,
            )
        results["records"][count] = row
        (out / "summary.json").write_text(json.dumps(results, indent=2) + "\n")
        print(
            f"{count} placements, {len(probes)} rays, {len(checks)} matched standalone pairs",
            flush=True,
        )


if __name__ == "__main__":
    if sys.argv[1:2] == ["--child-peak-rss"]:
        print(child_rss(sys.argv[2:]))
    else:
        main()
