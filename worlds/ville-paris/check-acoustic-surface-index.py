#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Compare full stereo impacts in a district of repeated tessellated acoustic screens."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import statistics
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
FIXTURE = runpy.run_path(str(HERE / "check-impact-diffraction.py"))
GLB = FIXTURE["common"]


def sha(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def prepare(out, placements):
    asset, replay = runpy.run_path(str(ROOT / "tools/object-convert/test_hierarchy.py"))["sample"](out)
    scene = out / "screens.glb"
    base, chunk = GLB["read_glb"](asset)
    # An eight-by-eight screen intercepts the source-to-camera path near its centre.
    faces = [FIXTURE["quad"](6.6+x/2, 7.1+x/2, 10.6+z/2, 11.1+z/2) for x in range(8) for z in range(8)]
    FIXTURE["write_geometry"](base, chunk, scene, faces, [0.4, 0.25, 0.1], True, False)
    doc, chunk = GLB["read_glb"](scene)
    parent = doc["nodes"][-2]
    mesh = doc["nodes"][-1]["extras"]["osage"]["acoustic"][0]["mesh"]
    for i in range(1, placements):
        name = f"remote-{i}"
        parent["children"].append(len(doc["nodes"]))
        doc["nodes"].append({
            "translation": [100 + 10 * (i % 32), 0, 100 + 10 * (i // 32)],
            "extras": {"osage": {"id": name, "acoustic": [
                {"id": name, "mesh": mesh, "source": "authored", "material": "wall"}
            ]}},
        })
    GLB["write_glb"](scene, doc, chunk)
    return scene, replay


def record(binary, scene, replay, out, label, direct, core):
    wav = out / f"{label}.wav"
    cmd = ["taskset", "-c", str(core), binary,
           "--scene", scene, "--replay", replay, "--route", "drop", "--physics", "jolt",
           "--actor", "capsule", "--listening", "headphones", "--impact-wav", wav]
    if direct:
        cmd.append("--direct-only")
    report_path = out / f"{label}.json"
    with report_path.open("wb") as sink, subprocess.Popen(cmd, stdout=sink) as proc:
        _, status, usage = os.wait4(proc.pid, 0)
        proc.returncode = os.waitstatus_to_exitcode(status)
        if proc.returncode:
            raise subprocess.CalledProcessError(proc.returncode, cmd)
    cost = {"process_cpu_ms": 1000 * (usage.ru_utime + usage.ru_stime), "peak_rss_kib": usage.ru_maxrss}
    report = json.loads(report_path.read_bytes())
    storage = report.pop("acoustic_index", None)
    payload = {"report": report, "wav_sha256": sha(wav)}
    return payload, storage, cost


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, required=True)
    parser.add_argument("--reference", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--placements", type=int, default=1024)
    parser.add_argument("--rounds", type=int, default=5)
    parser.add_argument("--cpu", type=int, default=min(os.sched_getaffinity(0)))
    args = parser.parse_args()
    if args.placements < 2 or args.rounds < 1:
        parser.error("at least two placements and one trial are required")
    out = args.out.resolve()
    out.mkdir(exist_ok=True, parents=True)
    scene, replay = prepare(out, args.placements)
    summary = {"scene_sha256": sha(scene), "placements":args.placements, "faces_per_screen":128,
               "reference_sha256":sha(args.reference), "current_sha256":sha(args.bin), "modes":{}}
    for direct, mode in [(True, "direct"), (False, "diffracted")]:
        expected = None
        samples = {"reference": [], "current": []}
        for i in range(args.rounds):
            builds = [("reference", args.reference), ("current", args.bin)]
            for name, binary in builds[::(-1 if i % 2 else 1)]:
                payload, storage, cost = record(binary.resolve(), scene, replay, out, f"{mode}-{name}", direct, args.cpu)
                if expected is None:
                    expected = payload
                if payload != expected:
                    raise AssertionError(f"{mode}/{name}: changed contacts, acoustic paths or stereo waveform")
                event, = payload["report"]["onsets"]
                if [h["id"] for h in event["direct_path"]["crossings"]] != ["surface-0"]:
                    raise AssertionError("screen should intersect the first sound path")
                if bool(event["diffracted_path"]) == direct:
                    raise AssertionError("unexpected diffracted path")
                if name == "current":
                    if not storage or storage["surfaces"] < args.placements or not 1 <= storage["triangle_trees"] <= 3:
                        raise AssertionError(f"inconsistent acoustic residency: {storage}")
                    if not 128 <= storage["indexed_triangles"] <= 132:
                        raise AssertionError(f"remote triangles were indexed: {storage}")
                    summary["modes"].setdefault(mode, {})["index"] = storage
                samples[name].append(cost)
        result = summary["modes"][mode]
        result.update(wav_sha256=expected["wav_sha256"], frames=expected["report"]["frames"], trials=samples,
                      median={k: {col: statistics.median(t[col] for t in series) for col in series[0]}
                              for k, series in samples.items()})
        (out / f"{mode}-receipt.json").write_text(json.dumps(expected["report"], indent=2)+"\n")
    (out / "summary.json").write_text(json.dumps(summary, indent=2)+"\n")
    print(json.dumps({k: {"index":v["index"], "median":v["median"]} for k,v in summary["modes"].items()}))


if __name__ == "__main__":
    main()
