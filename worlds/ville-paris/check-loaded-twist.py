#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Measure signed twist and directional tilt when a moving deck catches all, two or one of the paws."""
import argparse
import csv
from copy import deepcopy
import json
import math
from pathlib import Path
import runpy
import subprocess
import sys
import xml.etree.ElementTree as ET

from impact_geometry import basis, gap, inverse, multiply, read_poses, segment_box_gap

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT / "tools/object-convert"))
sys.path.insert(0, str(ROOT / "tools/object-convert/fixtures"))
from convert import convert
from make_quadruped import write


def joint_error(q, limits):
    """Signed twist and tilt in the bind frame, with the angular ellipse as the admissible set."""
    q = tuple(v / math.hypot(*q) for v in q)
    x, y, z, w = q
    twist = (math.degrees(2 * math.atan2(x, w)) + 180) % 360 - 180
    tilt = math.degrees(2 * math.atan2(math.hypot(y, z), math.hypot(w, x)))
    (lo, hi), (_, y_width), (_, z_width) = limits
    _, dy, dz = basis(q)[0]
    radius = math.hypot(dy, dz)
    width = radius / math.hypot(dz / y_width, dy / z_width) if radius else min(y_width, z_width)
    return max(0., tilt - width, twist - hi, lo - twist), twist, tilt


def measure(folder):
    run = json.loads((folder / "dynamics.json").read_bytes())["run"]
    parts = run["parts"]
    routes = {}
    passing = {}
    with (folder / "dynamics.csv").open() as trace:
        for row in csv.DictReader(trace):
            state = routes.setdefault(row["route"], dict(peak_deg=0., errors=[], contact_ticks=0))
            state["contact_ticks"] += bool(json.loads(row["contacts_json"]))
            bodies = read_poses(row)
            if row["route"] in ("glance", "clear"):
                passing.setdefault(row["route"], []).append(bodies)
            if int(row["tick"]) == 0:
                continue
            delta = [multiply(b["rotation_xyzw"], inverse(p["rotation_xyzw"]))
                     for b,p in zip(bodies, parts, strict=True)]
            worst = 0.
            for child, part in enumerate(parts[1:], start=1):
                frame = part["joint_frame_xyzw"]
                relative = multiply(inverse(delta[part["parent"]]), delta[child])
                q = multiply(inverse(frame), multiply(relative, frame))
                error, twist, swing = joint_error(q, part["limits_deg"])
                worst = max(worst, error)
                if error > state["peak_deg"]:
                    state.update(peak_deg=error, part=part["name"], tick=int(row["tick"]),
                                 twist_deg=twist, swing_deg=swing)
            state["errors"].append(worst)
    for name, state in routes.items():
        errors = state.pop("errors")
        assert len(errors) == 120
        if name != "glance":
            assert (state["contact_ticks"] > 0) == (name not in ("release", "clear"))
        state["mean_max_deg"] = sum(errors) / len(errors)
    if passing:
        # A positive speculative witness is allowed. Every limb must follow
        # the same trajectory with the pad nearby as with the pad ten metres away.
        assert passing["glance"] == passing["clear"], folder
    return routes


def sliding_response(directory, slides):
    """Observe a quarter second from the first nonpositive deck witness on each route."""
    run = json.loads((directory / "dynamics.json").read_bytes())["run"]
    parts = run["parts"]
    mass = sum(part["mass_kg"] for part in parts)
    lift = run["surfaces"].index("lift")
    peaks = {}
    with (directory / "dynamics.csv").open() as trace:
        for row in csv.DictReader(trace):
            name, tick = row["route"], int(row["tick"])
            if name not in slides:
                continue
            if name not in peaks:
                hits = json.loads(row["contacts_json"])
                if not any(h["surface"] == lift and h["separation"] <= 0 for h in hits):
                    continue
                peaks[name] = dict(first_tick=tick, samples=0, depth_m=0., peak_relative_vy_m_s=-math.inf)
            peak = peaks[name]
            if 4 * (tick - peak["first_tick"]) >= run["tick_hz"]:
                continue
            box = json.loads(row["boxes_json"])[0]
            axes = basis(box["rotation_xyzw"])
            bodies = read_poses(row)
            distances = [gap(part, body, axes, box["center"], slides[name]["half_extents_m"])
                         for part, body in zip(parts, bodies, strict=True)]
            vy = sum(part["mass_kg"] * body["linear_velocity"][1]
                     for part, body in zip(parts, bodies, strict=True)) / mass - box["linear_velocity"][1]
            peak["samples"] += 1
            peak["depth_m"] = max(peak["depth_m"], -min(distances))
            peak["peak_relative_vy_m_s"] = max(peak["peak_relative_vy_m_s"], vy)
    assert peaks.keys() == slides.keys()
    assert all(p["samples"] == math.ceil(run["tick_hz"] / 4) for p in peaks.values())
    return peaks


def certify_clearance(parts, route, gravity):
    """Lower-bound free-flight separation on a regular time grid, then prove the rig has passed the pad."""
    deck = route["lift"]
    half = deck["half_extents_m"]
    velocity = route["velocity_m_s"]
    hz, end = 2400, .5
    omega = 2 * math.pi / deck["period_s"]
    speed_bound = math.hypot(*velocity) + gravity * end + .5 * deck["travel_m"] * omega
    closest = math.inf
    for part in parts:
        arm = [v * part["half_segment_m"] for v in basis(part["rotation_xyzw"])[1]]
        center = [x + y - z for x, y, z in zip(route["start_m"], part["center_m"], deck["center_m"], strict=True)]
        # At end time the entire limb is past the left face and receding.
        assert velocity[0] < 0 and center[0] + end * velocity[0] + abs(arm[0]) + part["radius_m"] < -half[0]
        for i in range(round(hz * end) + 1):
            t = i / hz
            c = [x + t * v for x, v in zip(center, velocity, strict=True)]
            c[1] -= .5 * gravity * t*t + .5 * deck["travel_m"] * (1 - math.cos(omega*t))
            closest = min(closest, segment_box_gap(c, arm, half, part["radius_m"]))
    lower = closest - speed_bound / (2*hz)
    assert lower > .002, lower
    return lower


def run(binary, out, variant="wide"):
    out.mkdir(parents=True, exist_ok=True)
    source = out / "quadruped.yft.xml"
    write(source)
    if variant != "narrow":
        widths = {"wide": (30, 30, 30), "anisotropic": (20, 5, 45)}[variant]
        tree = ET.parse(source)
        for limit in tree.findall("Physics/LOD1/ArticulatedBody/Joints/Item/UnknownA0"):
            for axis, degrees in zip("xyz", widths, strict=True):
                limit.set(axis, str(math.radians(degrees)))
        tree.write(source, encoding="utf-8")
    replay = runpy.run_path(str(HERE / "check-quadruped-support.py"))["quadruped_replay"]()
    if variant != "anisotropic":
        replay["routes"] = [r for r in replay["routes"] if r["id"] in ("edge", "four", "slide-4", "release")]
    if variant != "wide":
        edge = next(r for r in replay["routes"] if r["id"] == "edge")
        for name, offset in (("left", .15), ("right", -.15)):
            pad = deepcopy(edge)
            pad["id"] = name
            pad["lift"]["center_m"][0] += offset
            pad["lift"]["half_extents_m"][0] = .08
            replay["routes"].append(pad)
        glance = deepcopy(next(r for r in replay["routes"] if r["id"] == "left"))
        glance.update(id="glance", velocity_m_s=[-1.5, 0., 0.])
        clear = deepcopy(glance)
        clear["id"] = "clear"
        clear["lift"]["center_m"][0] += 10.
        replay["routes"].extend([glance, clear])
    if variant == "narrow":
        frictionless = deepcopy(next(r for r in replay["routes"] if r["id"] == "slide-4"))
        frictionless["id"] = "slide-4-frictionless"
        frictionless["lift"]["friction"] = 0.
        replay["routes"].append(frictionless)
    if variant == "anisotropic":
        offset = deepcopy(next(r for r in replay["routes"] if r["id"] == "slide-0"))
        offset["id"] = "slide-0-offset"
        offset["start_m"][0] -= .05
        replay["routes"].append(offset)
    routes = out / "routes.json"
    routes.write_text(json.dumps(replay))
    results = {}
    for mode, options in (("independent", {}), ("ellipse", {"swing_envelope":"angular-ellipse"})):
        model, _ = convert(object_id="quadruped", fragment=source, **options)
        path = out / (mode + ".glb")
        path.write_bytes(model)
        for engine in ("rapier", "jolt"):
            directory = out / f"{engine}-{mode}"
            command = [binary, "--physics", engine, "--actor-rig", path, "--scene", HERE / "fixtures/bir-hakeim.glb",
                       "--replay", routes]
            for action in ("--bench", "--verify"):
                subprocess.run([*command, action, directory], check=True, stdout=subprocess.DEVNULL)
            results[directory.name] = measure(directory)
    if variant == "narrow":
        slides = {r["id"]: r["lift"] for r in replay["routes"] if r["id"] in ("slide-4", "slide-4-frictionless")}
        peaks = sliding_response(out / "rapier-ellipse", slides)
        assert peaks["slide-4"]["first_tick"] == peaks["slide-4-frictionless"]["first_tick"]
        for name, vertical_limit in (("slide-4", .4), ("slide-4-frictionless", 0.)):
            case = results["rapier-ellipse"][name]
            case["early_support"] = peak = peaks[name]
            assert case["peak_deg"] < 2. and peak["depth_m"] < .008 and peak["peak_relative_vy_m_s"] < vertical_limit, case
    (out / "limits.json").write_text(json.dumps(results, indent=2))
    for name, cases in results.items():
        print(name, {case:round(v["mean_max_deg"], 3) for case,v in cases.items()})
    for engine in ("rapier", "jolt"):
        coupled = results[engine+"-ellipse"]
        assert coupled["release"]["peak_deg"] < .01
        for name in ("four", "edge", "slide-4"):
            assert coupled[name]["mean_max_deg"] < results[engine+"-independent"][name]["mean_max_deg"]
    if variant != "wide":
        report = json.loads((out / "rapier-ellipse/dynamics.json").read_bytes())["run"]
        print("glancing clearance lower bound (m)", certify_clearance(report["parts"], glance, replay["gravity_m_s2"]))
        events = {r["id"]: r["first_contacts"] for r in report["outcomes"]}
        for name in ("left", "right"):
            assert events[name][0]["parts"] == [f"rear_{name}_paw"]
        if variant == "anisotropic":
            for part in report["parts"][1:]:
                for interval, degrees in zip(part["limits_deg"], (45, 20, 5), strict=True):
                    assert math.isclose(interval[0], -degrees, abs_tol=1e-4)
                    assert math.isclose(interval[1], degrees, abs_tol=1e-4)
            # Four loaded paws can push twist beyond its stop late in this drop.
            small_pad = results["rapier-ellipse"]["slide-0"]
            assert small_pad["peak_deg"] < 2.4, small_pad
            # Shifting the actor by 5 cm loads an oblique direction of the same ellipse.
            offset = results["rapier-ellipse"]["slide-0-offset"]
            assert offset["peak_deg"] < 2.4, offset
            # The contact normals cease to carry load as the actor leaves the ledge.
            edge_peak = results["rapier-ellipse"]["edge"]["peak_deg"]
            assert edge_peak < 2.4, edge_peak
            for name, case in results["rapier-ellipse"].items():
                assert case["mean_max_deg"] < 1. and case["peak_deg"] < 6., (name, case)
        else:
            for name in ("edge", "left", "right"):
                case = results["rapier-ellipse"][name]
                assert case["mean_max_deg"] < .25 and case["peak_deg"] < 1.5, (name, case)
    else:
        for name in ("four", "edge", "slide-4"):
            case = results["rapier-ellipse"][name]
            assert case["mean_max_deg"] < 1. and case["peak_deg"] < 8., (name, case)
    return results


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    run(args.bin.resolve(), args.out)
    run(args.bin.resolve(), args.out / "narrow", variant="narrow")
    run(args.bin.resolve(), args.out / "anisotropic", variant="anisotropic")
