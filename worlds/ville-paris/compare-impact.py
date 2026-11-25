#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Run both solvers and measure an imported pedestrian impacting a moving or parked bonnet."""

import argparse
import csv
import hashlib
import itertools
import json
import struct
import subprocess
from pathlib import Path
from tempfile import NamedTemporaryFile

from impact_geometry import (
    basis,
    difference,
    dot,
    gap,
    normal_motion,
    read_poses,
    surface_witnesses,
)

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


class BonnetRebound:
    """Measure clean lift-off relative to the contacted top face."""

    def __init__(self):
        self.contact_height = None
        self.release_tick = None
        self.best = None
        self.finished = False

    def observe(self, tick, height, over, clearance, contact, other_contact):
        if self.finished:
            return
        if self.contact_height is not None and (other_contact or not over):
            self.finished = True
            return
        if not over or other_contact:
            return
        if contact:
            self.contact_height = height
            self.release_tick = None
            return
        if self.contact_height is None:
            return
        if clearance <= 0.001:
            if self.release_tick is not None:
                self.contact_height = None
                self.release_tick = None
            return
        if self.release_tick is None:
            self.release_tick = tick
        rise = height - self.contact_height
        if rise > 0 and (self.best is None or rise > self.best["rise_m"]):
            self.best = {
                "start_tick": self.release_tick,
                "apex_tick": tick,
                "rise_m": rise,
            }


def pose_bytes(pose):
    return struct.pack(
        "<13f",
        *pose["center"],
        *pose["rotation_xyzw"],
        *pose["linear_velocity"],
        *pose["angular_velocity"],
    )


def native_response(capture, first, poses, box, parts, half, hz):
    hit = capture["first_contact"]
    samples = capture["samples"]
    by_step = {s["step"]: s for s in samples}
    if [s["step"] for s in samples] != list(
        range(samples[0]["step"], samples[-1]["step"] + 1)
    ):
        raise ValueError("non-contiguous native observation")
    step = hit["step"]
    if step != (first["tick"] - 1) * capture["substeps_per_tick"] + first["substep"]:
        raise ValueError("incorrect native step index")
    start, end = by_step[step - 1], by_step[step]
    if (hit["tick"], hit["substep"]) != (first["tick"], first["substep"]):
        raise ValueError("native and ordinary first-contact times differ")
    indices = sorted({row["part"] for row in hit["contacts"]})
    if sorted(parts[i]["name"] for i in indices) != first["parts"]:
        raise ValueError("native and ordinary first-contact parts differ")
    for observed, ordinary in zip(
        [*hit["tick_end"]["parts"], hit["tick_end"]["box"]], [*poses, box], strict=True
    ):
        if pose_bytes(observed) != pose_bytes(ordinary):
            raise ValueError("native observation changed the public trajectory")
    result = {}
    for i in indices:
        readings = {}
        for boundary, state in (("start", start), ("end", end)):
            readings[boundary] = surface_witnesses(
                parts[i], state["parts"][i], state["box"], half
            )
        readings["reported_points"] = [
            {
                "point_m": c["point"],
                "normal": c["normal"],
                "separation_m": c["separation"],
                "end_velocity_at_reported_point": normal_motion(
                    end["parts"][i], end["box"], c["point"], c["point"], c["normal"]
                ),
            }
            for c in hit["contacts"]
            if c["part"] == i
        ]
        result[parts[i]["name"]] = readings
    overlap = None
    depth_samples = []
    for sample in samples:
        state_box = sample["box"]
        axes = basis(state_box["rotation_xyzw"])
        gaps = [
            gap(part, pose, axes, state_box["center"], half)
            for part, pose in zip(parts, sample["parts"], strict=True)
        ]
        closest = min(range(len(parts)), key=gaps.__getitem__)
        depth_samples.append(
            {
                "step": sample["step"],
                "part": parts[closest]["name"],
                "depth_m": max(0.0, -gaps[closest]),
            }
        )
        if overlap is None and gaps[closest] <= 0:
            overlap = {
                "step": sample["step"],
                "time_s": sample["step"] / (hz * capture["substeps_per_tick"]),
                "already_overlapping_at_window_start": sample is samples[0]
                and sample["step"] > 0,
                "parts": {
                    parts[i]["name"]: surface_witnesses(
                        parts[i], sample["parts"][i], state_box, half
                    )
                    for i, separation in enumerate(gaps)
                    if separation <= 0
                },
            }
    return {
        "depth_samples": depth_samples,
        "reported_step": step,
        "reported_parts": result,
        "window_steps": [samples[0]["step"], samples[-1]["step"]],
        "first_sampled_touch": overlap,
    }


def common_depth_windows(readings, native_hz):
    """Sample each solver on the same clock from the earliest reported contact."""
    first = min(r["reported_step"] for r in readings.values())
    windows = []
    for ms in (25, 50):
        last = first + ms * native_hz // 1000
        peaks = {}
        for engine, report in readings.items():
            values = [v for v in report["depth_samples"] if first <= v["step"] <= last]
            # A late first contact or a short route can truncate one capture.
            peaks[engine] = (
                max(values, key=lambda v: v["depth_m"])
                if len(values) == last - first + 1
                else None
            )
        windows.append(
            {"duration_ms": ms, "first_step": first, "last_step": last, "peaks": peaks}
        )
    return windows


def route_measure(rows, route, run, capture=None):
    parts = run["parts"]
    target = run["surfaces"].index("bonnet")
    half = route["bonnet"]["half_extents_m"]
    hz, substeps = run["tick_hz"], run["substeps_per_tick"]
    mass = sum(part["mass_kg"] for part in parts)
    if len(parts) < 2 or mass <= 0:
        raise ValueError("an articulated actor is required")
    first = deepest = geometric = None
    response = None
    bounce = BonnetRebound()
    ticks = 0
    for tick, row in enumerate(rows):
        if int(row["tick"]) != tick:
            raise ValueError("non-contiguous replay")
        ticks += 1
        contacts = json.loads(row["contacts_json"])
        target_contacts = [c for c in contacts if c["surface"] == target]
        if first is None and target_contacts:
            substep = min(c["substep"] for c in target_contacts)
            ids = sorted(
                {
                    parts[c["part"]]["name"]
                    for c in target_contacts
                    if c["substep"] == substep
                }
            )
            first = {
                "tick": tick,
                "substep": substep,
                "parts": ids,
                "seconds": (tick - 1 + substep / substeps) / hz,
            }
        (box,) = json.loads(row["boxes_json"])
        poses = read_poses(row)
        if capture is not None and first is not None and first["tick"] == tick:
            hit = capture["first_contact"]
            if hit is None:
                raise ValueError("native observation missed the first contact")
            response = native_response(capture, first, poses, box, parts, half, hz)
        axes = basis(box["rotation_xyzw"])
        distances = [
            gap(p, pose, axes, box["center"], half)
            for p, pose in zip(parts, poses, strict=True)
        ]
        closest = min(distances)
        if closest <= 0 and geometric is None:
            geometric = tick
        if closest < 0 and (deepest is None or -closest > deepest["metres"]):
            deepest = {
                "metres": -closest,
                "tick": tick,
                "part": parts[distances.index(closest)]["name"],
            }
        # A rebound here is a whole-actor lift-off from the bonnet. Track the
        # mass centre along its moving top normal until re-contact or edge exit.
        com = [
            sum(
                p["mass_kg"] * pose["center"][i]
                for p, pose in zip(parts, poses, strict=True)
            )
            / mass
            for i in range(3)
        ]
        local = [dot(difference(com, box["center"]), axis) for axis in axes]
        height = local[1] - half[1]
        over = abs(local[0]) <= half[0] and abs(local[2]) <= half[2]
        bounce.observe(
            tick,
            height,
            over,
            closest,
            bool(target_contacts),
            any(c["surface"] != target for c in contacts),
        )
    if ticks != route["ticks"] + 1 or first is None:
        raise ValueError(f"incomplete impact for {route['id']}")
    return {
        "first_reported_contact": first,
        "first_sampled_overlap_tick": geometric,
        "maximum_sampled_penetration": deepest,
        "whole_body_rebound": bounce.best,
        "observations": ticks,
        **({"first_contact_surface_motion": response} if capture is not None else {}),
    }


def measure(directory, routes, observation=None):
    run = json.loads((directory / "dynamics.json").read_text())["run"]
    trace = directory / "dynamics.csv"
    with trace.open("rb") as raw:
        if hashlib.file_digest(raw, "sha256").hexdigest() != run["trace_sha256"]:
            raise ValueError("trace digest mismatch")
    captures = {}
    if observation is not None:
        if observation["format"] != "osage/bonnet-contact/1" or any(
            observation[k] != run[k]
            for k in ("inputs", "engine", "actor", "parts", "tick_hz")
        ):
            raise ValueError("native observation and ordinary replay inputs differ")
        records = observation["routes"]
        if [c["id"] for c in records] != [r["id"] for r in routes] or any(
            c["substeps_per_tick"] != run["substeps_per_tick"] for c in records
        ):
            raise ValueError("native observation routes or solver clock differ")
        captures = {c["id"]: c for c in records}
    result = {}
    with trace.open(newline="") as source:
        for (name, rows), route in zip(
            itertools.groupby(csv.DictReader(source), key=lambda r: r["route"]),
            routes,
            strict=True,
        ):
            if name != route["id"]:
                raise ValueError("route mismatch")
            result[name] = route_measure(rows, route, run, captures.get(name))
    return run, result


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("out", type=Path)
    cli.add_argument(
        "--actor-rig",
        type=Path,
        help="imported pedestrian, defaults to the built-in human",
    )
    cli.add_argument(
        "--native-response",
        action="store_true",
        help="measure the first reported bonnet contact on the configured solver clock",
    )
    cli.add_argument("--binary", type=Path, default=ROOT / "target/release/ville-paris")
    cli.add_argument("--replay", type=Path, default=HERE / "fixtures/bonnet.json")
    cli.add_argument("--scene", type=Path, default=HERE / "fixtures/bir-hakeim.glb")
    args = cli.parse_args()
    routes = json.loads(args.replay.read_bytes())["routes"]
    args.out.mkdir(parents=True, exist_ok=True)
    controls = {}
    shared_case = None
    for engine in ("rapier", "jolt"):
        folder = args.out / engine
        command = [
            args.binary.resolve(),
            *(
                ["--actor-rig", args.actor_rig.resolve()]
                if args.actor_rig
                else ["--actor", "human"]
            ),
            "--replay",
            args.replay.resolve(),
            "--scene",
            args.scene.resolve(),
            "--physics",
            engine,
        ]
        for mode in ("--bench", "--verify"):
            subprocess.run(
                [*command, mode, folder], check=True, stdout=subprocess.DEVNULL
            )
        observation = (
            json.loads(subprocess.check_output([*command, "--impact-response"]))
            if args.native_response
            else None
        )
        run, readings = measure(folder, routes, observation)
        for key, file in [
            *([("actor_sha256", args.actor_rig)] if args.actor_rig else []),
            ("scene_sha256", args.scene),
            ("replay_sha256", args.replay),
        ]:
            if hashlib.sha256(file.read_bytes()).hexdigest() != run["inputs"][key]:
                raise ValueError(f"incorrect {key} in {engine} receipt")
        if run["engine"] != engine or run["actor"] != (
            "imported" if args.actor_rig else "human"
        ):
            raise ValueError("incorrect backend or actor")
        case = {
            k: run[k]
            for k in ("inputs", "parts", "surfaces", "tick_hz", "substeps_per_tick")
        }
        if shared_case is not None and shared_case != case:
            raise ValueError("solvers received different cases")
        controls[engine] = {"trace_sha256": run["trace_sha256"], "routes": readings}
        shared_case = case
    summary = {"case": shared_case, "engines": controls}
    if args.native_response:
        summary["common_depth_windows"] = {
            route["id"]: common_depth_windows(
                {
                    e: c["routes"][route["id"]]["first_contact_surface_motion"]
                    for e, c in controls.items()
                },
                shared_case["tick_hz"] * shared_case["substeps_per_tick"],
            )
            for route in routes
        }
    with NamedTemporaryFile(mode="w", dir=args.out, delete=False) as output:
        json.dump(summary, output, indent=2, allow_nan=False)
        output.write("\n")
    Path(output.name).replace(args.out / "impact.json")
    print(json.dumps(controls, indent=2))


if __name__ == "__main__":
    main()
