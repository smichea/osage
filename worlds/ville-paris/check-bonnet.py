#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Run matched pedestrian drops on a stationary and a rolling body with each solver."""

import argparse
import copy
import csv
import hashlib
import json
import math
from pathlib import Path
import subprocess

from impact_geometry import basis, dot, inverse, multiply


def cases(template, size):
    replay = dict(template, routes=[])
    for row in range(size):
        for column in range(size):
            x = (column + 0.5) / size - 0.5
            z = (row + 0.5) / size - 0.5
            for original in template["routes"]:
                route = copy.deepcopy(original)
                route["id"] = f'{original["id"]}-{row}-{column}'
                route["start_m"] = [round(0.5 * x, 6), round(10.3 + 0.5 * z, 6), round(6.65 + 0.4 * z, 6)]
                route["velocity_m_s"] = [round(0.5 + 0.6 * z, 6), round(-2.0 + 0.7 * x, 6), 0]
                replay["routes"].append(route)
    return replay


def top_depth_mm(box, half, limb, part):
    """Post-step capsule depth on the top face where its lowest point lies over the face."""
    axes = basis(box["rotation_xyzw"])
    offset = [a - b for a, b in zip(limb["center"], box["center"])]
    center = [dot(offset, axis) for axis in axes]
    up = basis(limb["rotation_xyzw"])[1]
    direction = [dot(up, axis) for axis in axes]
    end = -math.copysign(part["half_segment_m"], direction[1])
    x, y, z = [c + end * d for c, d in zip(center, direction)]
    if abs(x) <= half[0] and abs(z) <= half[2]:
        return 1000 * max(0, half[1] - (y - part["radius_m"]))
    return None


def hinge_error_deg(model, rotations):
    """Final swing off the hinge axis or twist outside its permitted interval, whichever is larger."""
    changes = [multiply(q, inverse(p["rotation_xyzw"])) for p, q in zip(model, rotations)]
    errors = []
    for i, p in enumerate(model):
        free = [axis for axis, limits in enumerate(p["limits_deg"]) if limits != [0, 0]]
        if p["parent"] is None or len(free) != 1:
            continue
        axis = free[0]
        q = multiply(inverse(changes[p["parent"]]), changes[i])
        if q[3] < 0:
            q = [-v for v in q]
        twist = math.degrees(2 * math.atan2(q[axis], q[3]))
        swing = math.degrees(2 * math.atan2(math.hypot(*(v for j, v in enumerate(q[:3]) if j != axis)),
                                            math.hypot(q[3], q[axis])))
        lo, hi = p["limits_deg"][axis]
        errors.append(max(swing, lo - twist, twist - hi))
    return max(errors)


def evaluate(directory):
    receipt = json.loads((directory / "dynamics.json").read_text())["run"]
    bonnet = receipt["surfaces"].index("bonnet")
    outcomes = {o["id"]: o for o in receipt["outcomes"]}
    depths = {name: {"samples": 0, "max_mm": 0.0, "consecutive_over_20mm": 0} for name in outcomes}
    last_rows = {}
    previous = None
    with (directory / "dynamics.csv").open(newline="") as source:
        for row in csv.DictReader(source):
            name = row["route"]
            last_rows[name] = row
            if name != previous:
                streak = {}
                previous = name
            contacts = json.loads(row["contacts_json"])
            box = json.loads(row["boxes_json"])[0]
            up = basis(box["rotation_xyzw"])[1]
            on_top = {c["part"] for c in contacts if c["surface"] == bonnet and dot(c["normal"], up) > 0.9}
            next_streak = {}
            if on_top:
                parts = [{"center": [float(row[k]) for k in ("cx", "cy", "cz")],
                          "rotation_xyzw": [float(row[k]) for k in ("qx", "qy", "qz", "qw")]}]
                parts.extend(json.loads(row["parts_json"]))
                half = outcomes[name]["bonnet"]["half_extents_m"]
                stats = depths[name]
                for i in on_top:
                    depth = top_depth_mm(box, half, parts[i], receipt["parts"][i])
                    if depth is None:
                        continue
                    stats["samples"] += 1
                    stats["max_mm"] = max(stats["max_mm"], depth)
                    if depth > 20:
                        next_streak[i] = streak.get(i, 0) + 1
                        stats["consecutive_over_20mm"] = max(stats["consecutive_over_20mm"], next_streak[i])
            streak = next_streak
    angles = {}
    for name, row in last_rows.items():
        rotations = [[float(row[k]) for k in ("qx", "qy", "qz", "qw")]]
        rotations.extend(p["rotation_xyzw"] for p in json.loads(row["parts_json"]))
        angles[name] = hinge_error_deg(receipt["parts"], rotations)
    groups = {}
    for kind in ("parked", "moving"):
        group = [o for o in outcomes.values() if o["id"].startswith(kind + "-")]
        hits = [o["first_contacts"][0]["tick"] for o in group if o["first_contacts"] and o["first_contacts"][0]["surface"] == "bonnet"]
        failures = [o["id"] for o in group if not o["target_contacted"] or not o["first_contacts"]
                    or o["first_contacts"][0]["surface"] != "bonnet" or o["final_anchor_gap_mm"] > 20
                    or angles[o["id"]] > 5 or not depths[o["id"]]["samples"] or depths[o["id"]]["consecutive_over_20mm"] >= 3]
        groups[kind] = {"drops": len(group), "bonnet_first": len(hits), "impact_tick_range": [min(hits), max(hits)] if hits else [],
                        "max_final_anchor_gap_mm": max(o["final_anchor_gap_mm"] for o in group),
                        "max_final_hinge_error_deg": round(max(angles[o["id"]] for o in group), 2),
                        "max_top_depth_mm": round(max(depths[o["id"]]["max_mm"] for o in group), 2),
                        "max_consecutive_ticks_over_20mm": max(depths[o["id"]]["consecutive_over_20mm"] for o in group),
                        "failures": failures}
    return {"input": receipt["inputs"], "trace_sha256": receipt["trace_sha256"], "groups": groups}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("out", type=Path)
    parser.add_argument("--binary", type=Path, default=Path("target/release/ville-paris"))
    parser.add_argument("--size", type=int, default=10, help="grid side, giving size squared drops per initial box speed")
    args = parser.parse_args()
    if not 1 <= args.size <= 30:
        parser.error("--size must be in 1..30")
    args.out.mkdir(parents=True, exist_ok=True)
    template = json.loads(Path(__file__).with_name("fixtures").joinpath("bonnet.json").read_text())
    replay = args.out / "replay.json"
    replay.write_text(json.dumps(cases(template, args.size), indent=2) + "\n")
    result = {}
    binary = args.binary.resolve()
    for engine in ("rapier", "jolt"):
        directory = args.out / engine
        directory.mkdir(exist_ok=True)
        cmd = [binary, "--actor", "human", "--physics", engine, "--replay", replay]
        with (directory / "run.log").open("w") as log:
            existing = all((directory / name).is_file() for name in ("dynamics.csv", "dynamics.json"))
            if not existing or subprocess.run([*cmd, "--verify", directory], stdout=log, stderr=subprocess.STDOUT).returncode:
                for mode in ("--bench", "--verify"):
                    subprocess.run([*cmd, mode, directory], stdout=log, stderr=subprocess.STDOUT, check=True)
        result[engine] = evaluate(directory)
    if result["rapier"]["input"] != result["jolt"]["input"]:
        raise ValueError("different scene or replay")
    if result["rapier"]["input"]["replay_sha256"] != hashlib.sha256(replay.read_bytes()).hexdigest():
        raise ValueError("different replay digest")
    output = json.dumps(result, indent=2)
    (args.out / "comparison.json").write_text(output + "\n")
    print(output)
    return int(any(g["failures"] for r in result.values() for g in r["groups"].values()))


if __name__ == "__main__":
    raise SystemExit(main())
