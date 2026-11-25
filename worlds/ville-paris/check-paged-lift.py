#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Compare the resident lift to the same ride while an independent observer streams terrain."""

import argparse
from collections import Counter
import csv
import hashlib
import json
from pathlib import Path
import runpy
import struct
import subprocess
import sys


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
LIFT = runpy.run_path(str(HERE / "check-lift.py"))
SCENE, REPLAY, require = LIFT["SCENE"], LIFT["REPLAY"], LIFT["require"]
POSE = ("center", "rotation_xyzw", "linear_velocity", "angular_velocity")


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def f32(n):
    return struct.unpack("<f", struct.pack("<f", n))[0]


def native_states(states):
    # The resident CSV emits shortest decimals; paged JSON can emit their exact f32 values as f64.
    def record(values):
        out = {}
        for key, value in values.items():
            if isinstance(value, list):
                value = list(map(f32, value))
            elif isinstance(value, float):
                value = f32(value)
            out[key] = value
        return out
    for tick, route, actor, lift, contacts in states:
        yield tick, route, record(actor), record(lift), [record(c) for c in contacts]


def observer(steps, hz):
    def key(tick):
        x = -610 + 36 * tick / hz
        return {"tick": tick, "foot_m": [x, 27, 100], "center_m": [x, 27.88, 100],
                "velocity_m_s": [36, 0, 0], "supported": True}
    return {"version": 1, "tracks": [{"slot": 1, "start": 0, "end": steps + 1, "keys": [key(0), key(steps)]}]}


def paged_states(raw):
    for row in raw["body_before_step"]:
        tick = row["tick"]
        require(row["step"] == (tick > 0), f"held or missing simulation step at tick {tick}")
        (passenger,) = row["parts_after_step"]
        (platform,) = row["boxes_after_step"]
        yield tick, raw["route"], passenger, platform, row["contacts_after_step"]


def contacts_by_identity(contacts, meta):
    def key(c):
        return (c["substep"], meta["parts"][c["part"]]["name"], meta["surfaces"][c["surface"]],
                c.get("source_triangle"), tuple(map(f32, c["point"])), tuple(map(f32, c["normal"])),
                f32(c["separation"]), f32(c["applied_offset_s"]) if "applied_offset_s" in c else None)
    # Native collider numbers and manifold traversal order can differ between the two worlds.
    return Counter(map(key, contacts))


def compare(reference, resident, states, meta):
    count = 0
    total_witnesses = 0
    contact_diff = {"ticks": 0, "multiplicity_only_ticks": 0}
    pose_diff = {}
    for before, after in zip(reference, states, strict=True):
        tick = before[0]
        require(before[:2] == after[:2], f"clock or route mismatch at {tick}")
        for label, a, b in zip(("passenger", "platform"), before[2:4], after[2:4], strict=True):
            for field in POSE:
                delta = max(abs(f32(x) - f32(y)) for x, y in zip(a[field], b[field], strict=True))
                if delta:
                    diff = pose_diff.setdefault(label + "." + field, {"first_tick": tick, "ticks": 0, "max_abs": 0})
                    diff["ticks"] += 1
                    diff["max_abs"] = max(diff["max_abs"], delta)
        expected = contacts_by_identity(before[4], resident)
        actual = contacts_by_identity(after[4], meta)
        total_witnesses += actual.total()
        if actual != expected:
            contact_diff["ticks"] += 1
            contact_diff["multiplicity_only_ticks"] += expected.keys() == actual.keys()
            if "first" not in contact_diff:
                contact_diff["first"] = {"tick": tick, "resident_only": list((expected - actual).items()),
                                         "paged_only": list((actual - expected).items())}
        count += 1
    return {"states": count, "contact_witnesses": total_witnesses, "changed_poses": pose_diff,
            "contact_differences": contact_diff}


def terrain(raw, directory, index, moving):
    layers = {(t["x"], t["z"]): t["physics"] for t in index["tiles"] if t.get("physics")}
    active = {}
    installs = []
    removals = []
    reads = 0
    for e in raw["physics_events"]:
        tile, tick = tuple(e["tile"]), e["tick"]
        require(e["kind"] == "physics", "unexpected event kind")
        if e["state"] == "ready":
            require(tile not in active, f"duplicate installation of {tile} at {tick}")
            layer = layers[tile]
            content = (directory / layer["file"]).read_bytes()
            require(len(content) == layer["bytes"] and hashlib.sha256(content).hexdigest() == layer["sha256"],
                    f"unexpected collision content for {tile}")
            active[tile] = tick
            installs.append((tick, tile))
            reads += len(content)
        elif e["state"] == "evicted":
            require(tile in active and active[tile] < tick, f"eviction without prior residence for {tile}")
            removals.append({"tile": tile, "ready_tick": active.pop(tile), "evicted_tick": tick})
    stats = raw["physics"]
    require((len(installs), len(removals), reads) == (stats["reads"], stats["evictions"], stats["read_bytes"]),
            "physical page counters disagree with the event log")
    require(raw["support"]["stats"]["held_ticks"] == raw["support"]["duplicate_layer_contact_ticks"] == 0,
            "held physics or duplicated support")
    case = {"reads": len(installs), "read_bytes": reads, "evictions": removals}
    if moving:
        pool = raw["physics_observers"]
        verified = sum(t["verified_bytes"] for t in pool["transfers"].values())
        require(verified == reads, "verified terrain bytes disagree with installed bytes")
        events = [e for e in pool["events"] if e["state"] == "installed"]
        require(Counter((e["tick"], tuple(e["tile"])) for e in events) == Counter(installs),
                "observer installations disagree with physical publications")
        secondary = [(e["tick"], tuple(e["tile"])) for e in events
                     if e["observers"] == [{"slot": 1, "generation": 1}]]
        retired = [v for v in removals if (v["ready_tick"], v["tile"]) in secondary]
        half = raw["tick"] // 2
        for lo, hi in ((0, half), (half, raw["tick"])):
            require(any(lo < t <= hi for t, _ in secondary) and any(lo < v["evicted_tick"] <= hi for v in retired),
                    f"no verified secondary installation and eviction during ticks {lo + 1}–{hi}")
        case["secondary_installations"] = secondary
        case["verified_bytes"] = verified
    else:
        require(installs and not removals, "the unaccompanied lift should retain its local terrain")
    return case


def inspect(raw, reference, run, replay, inputs, directory, index, moving):
    (route,) = replay["routes"]
    require(raw["source_sha256"] == inputs["scene_sha256"] and raw["replay_sha256"] == inputs["replay_sha256"],
            "scene or replay differs from the resident run")
    require(raw["clock_hz"] == run["tick_hz"] and raw["tick"] == route["ticks"], "different simulation clock")
    require(raw["settings"]["physics_plan"] == "trajectory" and raw["settings"]["physics_radius_m"] == 14,
            "unexpected terrain selection policy")
    actor = raw["actor"]
    require(actor["model"] == run["actor"] and actor["substeps_per_tick"] == run["substeps_per_tick"],
            "different actor or solver substep clock")
    (part,), (reference_part,) = actor["parts"], run["parts"]
    require(part["name"] == reference_part["name"] and part["parent"] == reference_part["parent"]
            and all(f32(part[k]) == f32(reference_part[k]) for k in ("mass_kg", "radius_m", "half_segment_m")),
            "different capsule")
    table = raw["contact_surfaces"]
    support = [s for s in table if s["id"] == "lift"]
    require(support == [{"id": "lift", "source_node": None, "source_path": []}], "unstable lift identity")
    states = list(native_states(paged_states(raw)))
    definition = raw["lift"]["definition"]
    for field in ("center_m", "half_extents_m"):
        require(tuple(map(f32, definition[field])) == tuple(map(f32, route["lift"][field])), f"different lift {field}")
    for field in ("travel_m", "period_s", "friction"):
        require(f32(definition[field]) == f32(route["lift"][field]), f"different lift {field}")
    require(states and all(tuple(map(f32, raw["lift"]["final_pose"][k])) == tuple(states[-1][3][k]) for k in POSE),
            "final lift pose differs from its trace")
    meta = {"engine": "rapier", "actor": actor["model"], "parts": actor["parts"], "surfaces": [s["id"] for s in table],
            "tick_hz": raw["clock_hz"], "substeps_per_tick": actor["substeps_per_tick"], "trace_rows": len(states)}
    first = raw["first_contact"]
    first_resident = next(s[0] for s in reference if s[4])
    require(raw["expected_support"] == first["support"] == "lift" and first["geometry"] == "kinematic_lift"
            and first["tile"] is None and first["source_node"] is None and first["tick"] == first_resident == 1,
            "unexpected first support, time or owner")
    physical = LIFT["measure_states"](meta, states, replay, "rapier")
    checked = raw["body_before_step"][2:]
    lost = [r["tick"] for r in checked if not r["supported"]]
    return {"physical": physical, "comparison": compare(reference, run, states, meta),
            "first_contact": {"id": first["support"], "geometry": first["geometry"], "tick": first["tick"],
                              "resident_surface_index": run["surfaces"].index("lift"),
                              "paged_surface_index": meta["surfaces"].index("lift")},
            "clock_hz": meta["tick_hz"], "substeps_per_tick": meta["substeps_per_tick"],
            "planner_support": {"checked_ticks": len(checked), "unsupported_ticks": len(lost),
                                "first_unsupported_tick": next(iter(lost), None)},
            "terrain": terrain(raw, directory, index, moving)}


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    cli.add_argument("--out", type=Path, default=ROOT / "bench/out/ville-paris/paged-lift")
    args = cli.parse_args()
    out, binary = args.out.resolve(), args.bin.resolve()
    binary_sha = sha(binary)
    directory = out / "district"
    out.mkdir(parents=True, exist_ok=True)
    subprocess.run([sys.executable, "-B", HERE / "cook-pages.py", SCENE, directory], check=True)
    replay = json.loads(REPLAY.read_bytes())
    (route,) = replay["routes"]
    tracks = out / "moving-observer.json"
    tracks.write_text(json.dumps(observer(route["ticks"], replay["tick_hz"]), indent=2) + "\n")
    inputs = {"scene_sha256": sha(SCENE), "replay_sha256": sha(REPLAY)}
    index = json.loads((directory / "index.json").read_bytes())
    require(index["source_sha256"] == inputs["scene_sha256"], "district was cooked from a different scene")
    control = out / "resident"
    for action in ("--bench", "--verify"):
        subprocess.run([binary, "--scene", SCENE, "--physics", "rapier", "--actor", "capsule",
                        "--replay", REPLAY, action, control], check=True)
    run = json.loads((control / "dynamics.json").read_bytes())["run"]
    require(run["inputs"] == inputs, "unexpected resident inputs")
    with (control / "dynamics.csv").open(newline="") as source:
        reference = list(native_states(LIFT["resident_states"](csv.DictReader(source))))
    results = {"inputs": inputs, "binary_sha256": binary_sha, "state_precision": "native float32 before measurement",
               "resident": LIFT["measure_states"](run, reference, replay, "rapier")}
    failed = False
    for name, extra in (("solo", []), ("secondary", ["--physics-observers", tracks])):
        dest = out / (name + ".json")
        subprocess.run([binary, "--paging", "--pages", directory, "--replay", REPLAY, "--route", route["id"],
                        "--actor", "capsule", "--physics-radius", "14", "--physics-plan", "trajectory",
                        "--delay-ms", "0", "--mbps-per-kind", "1000", "--trace-body", *extra, "--out", dest], check=True)
        result = inspect(json.loads(dest.read_bytes()), reference, run, replay, inputs, directory, index, bool(extra))
        results[name] = result
        compare_result = result["comparison"]
        failed |= bool(compare_result["changed_poses"] or compare_result["contact_differences"]["ticks"]
                       or result["planner_support"]["unsupported_ticks"])
    require(sha(binary) == binary_sha, "binary changed during the comparison")
    report = json.dumps(results, indent=2)
    (out / "comparison.json").write_text(report + "\n")
    print(report)
    return int(failed)


if __name__ == "__main__":
    raise SystemExit(main())
