#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Measure a standing capsule and its contact point through one full lift cycle."""

import argparse
import csv
import hashlib
import json
import math
from pathlib import Path
import subprocess

from impact_geometry import basis, difference, dot


HERE = Path(__file__).resolve().parent
SCENE = HERE / "fixtures/bir-hakeim.glb"
REPLAY = HERE / "fixtures/lift.json"
LIMIT_M = 0.02


def require(condition, message):
    if not condition:
        raise ValueError(message)


def vector(values, size):
    values = tuple(map(float, values))
    require(len(values) == size and all(map(math.isfinite, values)), f"invalid vector {values}")
    return values


def quaternion(values):
    q = vector(values, 4)
    require(abs(math.hypot(*q) - 1) < 1e-5, f"invalid quaternion {q}")
    return q


def resident_states(rows):
    for row in rows:
        pose = {key: [float(row[k]) for k in fields] for key, fields in (
            ("center", ("cx", "cy", "cz")), ("rotation_xyzw", ("qx", "qy", "qz", "qw")),
            ("linear_velocity", ("vx", "vy", "vz")), ("angular_velocity", ("wx", "wy", "wz")))}
        (platform,) = json.loads(row["boxes_json"])
        yield int(row["tick"]), row["route"], pose, platform, json.loads(row["contacts_json"])


def measure(run, rows, replay, engine):
    return measure_states(run, resident_states(rows), replay, engine)


def measure_states(run, states, replay, engine):
    (route,) = replay["routes"]
    lift = route["lift"]
    hz, steps = run["tick_hz"], route["ticks"]
    substeps = run["substeps_per_tick"]
    require(hz == replay["tick_hz"] == 60 and steps == 600 and substeps >= 1, "unexpected clock")
    require(lift["period_s"] == steps / hz and lift["travel_m"] == 3, "expected one full 3 m cycle")
    require(run["engine"] == engine and run["actor"] == "capsule", "unexpected solver or actor")
    require(run["surfaces"].count("lift") == 1 and route["expected_support"] == "lift", "missing lift surface")
    surface = run["surfaces"].index("lift")
    (part,) = run["parts"]
    radius, half = part["radius_m"], part["half_segment_m"]
    require(math.isclose(radius, 0.28, abs_tol=1e-6) and math.isclose(half, 0.60, abs_tol=1e-6), "capsule size")
    base = lift["center_m"]
    extents = lift["half_extents_m"]
    anchor = difference(route["start_m"], base)
    require(anchor[1] == extents[1] and all(0 < abs(anchor[i]) < extents[i] - radius for i in (0, 2)),
            "the capsule must start off-centre on the top face")
    maxima = {"platform_pose_m": 0.0, "foot_local_m": 0.0, "contact_local_m": 0.0}
    max_contact_from_prior_pose_m = 0.0
    phases = {"rising": 0, "descending": 0}
    previous = base
    heights = []
    for tick, (recorded_tick, route_id, capsule, body, contacts) in enumerate(states):
        require(recorded_tick == tick <= steps and route_id == route["id"], f"unexpected row at {tick}")
        center = vector(capsule["center"], 3)
        up = basis(quaternion(capsule["rotation_xyzw"]))[1]
        vector(capsule["linear_velocity"], 3)
        vector(capsule["angular_velocity"], 3)
        position = vector(body["center"], 3)
        q = quaternion(body["rotation_xyzw"])
        axes = basis(q)
        vector(body["linear_velocity"], 3)
        vector(body["angular_velocity"], 3)
        height = base[1] + 0.5 * lift["travel_m"] * (1 - math.cos(math.tau * tick / (hz * lift["period_s"])))
        expected = (base[0], height, base[2])
        # The rotational term bounds the displacement of every corner of the platform.
        pose_error = math.dist(position, expected) + 2 * math.hypot(*extents) * math.hypot(*q[:3])
        maxima["platform_pose_m"] = max(maxima["platform_pose_m"], pose_error)

        local = [dot(difference(center, position), axis) for axis in axes]
        local_up = [dot(up, axis) for axis in axes]
        arm = math.copysign(half, local_up[1])
        foot = [p - arm * u for p, u in zip(local, local_up)]
        foot[1] -= radius
        maxima["foot_local_m"] = max(maxima["foot_local_m"], math.dist(foot, anchor))

        require(tick == 0 or contacts, f"no lift contact at tick {tick}")
        for contact in contacts:
            require(contact["surface"] == surface and contact["part"] == 0, f"wrong contact at tick {tick}")
            substep = contact["substep"]
            require(1 <= substep <= substeps, f"bad contact clock at tick {tick}")
            normal = vector(contact["normal"], 3)
            require(dot(normal, axes[1]) > 0.99 and abs(contact["separation"]) <= LIMIT_M,
                    f"contact outside the top face at tick {tick}")
            # Gate the reported witness against the end of its substep. The comparison with
            # the starting pose independently exposes a solver that emits its manifold earlier.
            origin = [a + (b - a) * substep / substeps for a, b in zip(previous, position)]
            point = vector(contact["point"], 3)
            prior = [a + (b - a) * (substep - 1) / substeps for a, b in zip(previous, position)]
            prior_local = [dot(difference(point, prior), axis) for axis in axes]
            max_contact_from_prior_pose_m = max(max_contact_from_prior_pose_m, math.dist(prior_local, anchor))
            offset = difference(point, origin)
            contact_local = [dot(offset, axis) for axis in axes]
            maxima["contact_local_m"] = max(maxima["contact_local_m"], math.dist(contact_local, anchor))
        require(max(maxima.values()) <= LIMIT_M, f"2 cm tolerance exceeded at tick {tick}: {maxima}")
        if tick:
            phases["rising" if tick <= steps // 2 else "descending"] += 1
        heights.append(position[1])
        previous = position
    require(len(heights) == run["trace_rows"] == steps + 1, "incomplete lift trace")
    rise = heights[steps // 2] - heights[0]
    descent = heights[steps // 2] - heights[-1]
    require(min(rise, descent) >= lift["travel_m"] - LIMIT_M, "lift did not complete the full excursion")
    return {"rise_m": rise, "descent_m": descent, "ticks_with_contact": phases, "max_error": maxima,
            "max_contact_error_at_substep_start_m": max_contact_from_prior_pose_m}


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("out", type=Path)
    cli.add_argument("--binary", type=Path, default=HERE.parents[1] / "target/release/ville-paris")
    args = cli.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    replay = json.loads(REPLAY.read_bytes())
    inputs = {"scene_sha256": hashlib.sha256(SCENE.read_bytes()).hexdigest(),
              "replay_sha256": hashlib.sha256(REPLAY.read_bytes()).hexdigest()}
    results = {}
    for engine in ("rapier", "jolt"):
        folder = args.out / engine
        cmd = [args.binary.resolve(), "--scene", SCENE, "--actor", "capsule", "--physics", engine, "--replay", REPLAY]
        try:
            for action in ("--bench", "--verify"):
                subprocess.run([*cmd, action, folder], check=True)
            run = json.loads((folder / "dynamics.json").read_bytes())["run"]
            require(run["inputs"] == inputs, "scene or replay digest mismatch")
            with (folder / "dynamics.csv").open(newline="") as source:
                results[engine] = measure(run, csv.DictReader(source), replay, engine)
        except (ValueError, KeyError, TypeError, OSError, subprocess.CalledProcessError) as error:
            results[engine] = {"error": str(error)}
    report = json.dumps({"inputs": inputs, "limit_m": LIMIT_M, "engines": results}, indent=2)
    (args.out / "comparison.json").write_text(report + "\n")
    print(report)
    return int(any("error" in result for result in results.values()))


if __name__ == "__main__":
    raise SystemExit(main())
