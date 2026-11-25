# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Read all posed capsules at ordinary tick boundaries after a moving-deck impact."""
import csv
import json
import math
import subprocess
from pathlib import Path

from impact_geometry import basis, gap, read_poses

HERE = Path(__file__).resolve().parent


def sampled_peaks(directory, routes):
    run = json.loads((directory / "dynamics.json").read_bytes())["run"]
    parts = run["parts"]
    mass = sum(p["mass_kg"] for p in parts)
    outcomes = {o["id"]: o["lift_approach"] for o in run["outcomes"]}
    peaks = {name: {"depth_m": 0., "upward_speed_250ms_m_s": 0.} for name in routes}
    with (directory / "dynamics.csv").open() as stream:
        for row in csv.DictReader(stream):
            name = row["route"]
            box = json.loads(row["boxes_json"])[0]
            bodies = read_poses(row)
            axes = basis(box["rotation_xyzw"])
            depth = max(-gap(p, b, axes, box["center"], routes[name]["lift"]["half_extents_m"])
                        for p, b in zip(parts, bodies, strict=True))
            peak = peaks[name]
            peak["depth_m"] = max(peak["depth_m"], depth)
            hit = outcomes[name]["first_manifold"]
            t = int(row["tick"]) / run["tick_hz"]
            if hit and hit["end_s"] <= t < hit["end_s"] + .25:
                upward = sum(p["mass_kg"] * b["linear_velocity"][1]
                             for p, b in zip(parts, bodies, strict=True)) / mass - box["linear_velocity"][1]
                peak["upward_speed_250ms_m_s"] = max(peak["upward_speed_250ms_m_s"], upward)
    return run, outcomes, peaks


def check(binary, pages, out, replay, rig):
    routes = {r["id"]: dict(r, lift_contact_hz=1920, lift_response_travel_m=.001) for r in replay["routes"]}
    path = out / "routes-response.json"
    path.write_text(json.dumps(dict(replay, routes=list(routes.values()))))
    measurements = {}
    for engine in ("rapier", "jolt"):
        directory = out / (engine + "-response")
        args = [binary, "--scene", HERE/"fixtures/bir-hakeim.glb", "--physics", engine, "--actor-rig", rig,
                "--replay", path]
        subprocess.run([*args, "--bench", directory], check=True, stdout=subprocess.DEVNULL)
        subprocess.run([*args, "--verify", directory], check=True, stdout=subprocess.DEVNULL)
        run, outcomes, peaks = sampled_peaks(directory, routes)
        _, previous, reference = sampled_peaks(out / (engine + "-refined"), routes)
        per_tick = run["substeps_per_tick"]
        hz = run["tick_hz"] * per_tick
        measurements[engine] = {}
        for name, r in routes.items():
            report = outcomes[name]
            response = report["response"]
            first = report["first_manifold"]
            assert first == previous[name]["first_manifold"]
            limit = (r["lift_contact_hz"] + hz - 1) // hz
            spans = response["refined_spans"]
            extra = response["extra_intervals"]
            assert extra == sum(s["substeps"] * (s["divisions"] - 1) for s in spans)
            assert report["extra_intervals"] == previous[name]["extra_intervals"] + extra
            assert 0 <= extra <= per_tick * (limit - 1)
            if first is None:
                assert not spans and peaks[name]["depth_m"] == 0
            else:
                # Use the midpoint of the reporting slice to identify its unrefined base interval.
                first_index = math.floor((first["start_s"] + first["end_s"]) * .5 * hz)
                end = first_index + 1
                for span in spans:
                    assert 1 < span["divisions"] <= limit and span["substeps"] > 0
                    assert end <= span["start_substep"]
                    end = span["start_substep"] + span["substeps"]
                    assert end <= first_index + 1 + per_tick
            measurements[engine][name] = {"first_contact_only": reference[name], "response": peaks[name],
                                          "response_extra_intervals": extra}
        assert peaks["edge"]["depth_m"] < reference["edge"]["depth_m"] - .00005
        if engine == "rapier":
            assert peaks["four"]["depth_m"] < reference["four"]["depth_m"] - .001
            for name in ("four", "edge", "release"):
                page_output = out / (name + "-response-paged.json")
                subprocess.run([binary, "--paging", "--no-visuals", "--pages", pages, "--replay", path,
                                "--actor-rig", rig, "--route", name, "--out", page_output], check=True)
                assert json.loads(page_output.read_bytes())["lift_approach"] == outcomes[name]
    (out / "lift-response.json").write_text(json.dumps(measurements, indent=2))
