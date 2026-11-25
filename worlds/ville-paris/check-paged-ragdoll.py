#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Compare the real articulated actor, its first terrain contacts and a district's collision fallback."""

import argparse
import csv
import hashlib
import json
import math
import runpy
import struct
import subprocess
import sys
from pathlib import Path

from paging_collision_codec import read_collision

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
CONVERTER = ROOT / "tools/object-convert"
SOLUTION_KEYS = ("center", "rotation_xyzw", "linear_velocity", "angular_velocity")
SCENARIOS = {"feet": 17.25, "outside-hand": 15.49, "clear": 13.75}
EXPECTED = {"feet": {"foot_l", "foot_r"}, "outside-hand": {"hand_l"}, "clear": set()}
TILE = [1, 0]


def sha(path):
    with path.open("rb") as data:
        return hashlib.file_digest(data, "sha256").hexdigest()


def run(*args):
    proc = subprocess.run(args, capture_output=True, text=True, check=False)
    if proc.returncode:
        raise RuntimeError(f"{' '.join(map(str, args))}\n{proc.stderr}")


def prepare(out):
    out.mkdir(parents=True, exist_ok=True)
    scene, template = runpy.run_path(str(CONVERTER / "test_hierarchy.py"))["sample"](
        out
    )
    glb = runpy.run_path(str(HERE / "check-impact-transmission.py"))
    tree, binary = glb["read_glb"](scene)
    # Move the west ledge to x=16.25. A 32 m cell starts at x=16; the T-pose's hand
    # can reach across it while the pelvis and legs remain in the neighbouring cell.
    tree["nodes"][0]["translation"][0] += 10.25
    glb["write_glb"](scene, tree, binary)
    pages = out / "district"
    runpy.run_path(str(HERE / "cook-pages.py"))["cook"](scene, pages, 32)
    idx = json.loads((pages / "index.json").read_bytes())
    west = [
        t for t in idx["tiles"] if "west-floor" in t.get("physics", {}).get("ids", ())
    ]
    assert [[t["x"], t["z"]] for t in west] == [TILE]
    geometry = read_collision(pages, west[0]["physics"])
    slab = next(s for s in geometry if s["id"] == "west-floor")
    assert min(p[0] for p in slab["vertices"]) == 16.25
    rig = out / "pedestrian.glb"
    run(
        sys.executable,
        CONVERTER / "convert.py",
        "--id",
        "pedestrian",
        "--fragment",
        CONVERTER / "fixtures/pedestrian.yft.xml",
        "-o",
        rig,
    )
    return scene, pages, rig, json.loads(template.read_text())


def routes(out, hz, template):
    route_data = []
    ticks = math.ceil(hz * 1.3)
    for case, x in SCENARIOS.items():
        camera = [
            {"tick": tick, "eye": [14.0, 5.0, 15.0], "aim": [x, 1.0, 10.0]}
            for tick in (0, ticks)
        ]
        # The resident replay format references a real surface even on the clear route.
        # The oracle checks the contacts and flight independently.
        route_data.append(
            {
                "id": case,
                "ticks": ticks,
                "start_m": [x, 3.0, 10.0],
                "velocity_m_s": [0.0, 0.0, 0.0],
                "expected_support": "west-floor",
                "camera": camera,
            }
        )
    replay = out / f"routes-{hz}.json"
    replay.write_text(
        json.dumps(dict(template, tick_hz=hz, routes=route_data), indent=2) + "\n"
    )
    return replay


def resident(city, scene, replay, args, folder):
    run(
        city,
        "--scene",
        scene,
        "--physics",
        "rapier",
        "--replay",
        replay,
        *args,
        "--bench",
        folder,
    )
    receipt = json.loads((folder / "dynamics.json").read_bytes())["run"]
    traces = {k: [] for k in SCENARIOS}
    with (folder / "dynamics.csv").open() as stream:
        for r in csv.DictReader(stream):
            root = {
                key: [float(r[k]) for k in fields]
                for key, fields in zip(
                    SOLUTION_KEYS,
                    [
                        ("cx", "cy", "cz"),
                        ("qx", "qy", "qz", "qw"),
                        ("vx", "vy", "vz"),
                        ("wx", "wy", "wz"),
                    ],
                    strict=True,
                )
            }
            traces[r["route"]].append(
                ([root, *json.loads(r["parts_json"])], json.loads(r["contacts_json"]))
            )
    return receipt, traces


def paged(city, pages, replay, case, args, flags, path):
    run(
        city,
        "--paging",
        "--pages",
        pages,
        "--replay",
        replay,
        "--route",
        case,
        *args,
        *flags,
        "--trace-body",
        "--out",
        path,
    )
    return json.loads(path.read_bytes())


def f32(n):
    # Resident CSV writes the shortest round-tripping decimal for a float32.
    return struct.unpack("<f", struct.pack("<f", n))[0]


def compare_pose(a, b):
    return max(
        abs(f32(x) - f32(y))
        for key in SOLUTION_KEYS
        for x, y in zip(a[key], b[key], strict=True)
    )


def packet(contacts, names):
    if not contacts:
        return None
    step = min(c["substep"] for c in contacts)
    return step, {names[c["part"]] for c in contacts if c["substep"] == step}


def first_witnesses(reference, actual, substep):
    # A manifold can enumerate its anchors in either order. Check both directions
    # within the same member rather than assigning a left-foot point to the right.
    a, b = (
        [c for c in group if c["substep"] == substep] for group in (reference, actual)
    )
    position = separation = normal = 0.0
    for group, other in ((a, b), (b, a)):
        for c in group:
            candidates = [d for d in other if d["part"] == c["part"]]
            closest = min(candidates, key=lambda d: math.dist(c["point"], d["point"]))
            position = max(position, math.dist(c["point"], closest["point"]))
            normal = max(normal, math.dist(c["normal"], closest["normal"]))
            separation = max(separation, abs(c["separation"] - closest["separation"]))
    return {"point_m": position, "normal_vector": normal, "separation_m": separation}


def controls(receipt, trace, parts, case, path, hz):
    names = [p["name"] for p in parts]
    actor = receipt["actor"]
    for actual, expected in zip(actor["parts"], parts, strict=True):
        assert (
            actual["name"] == expected["name"]
            and actual["parent"] == expected["parent"]
        )
        for key in ("radius_m", "half_segment_m", "mass_kg"):
            assert f32(actual[key]) == f32(expected[key])
    assert actor["substeps_per_tick"] == max(4, math.ceil(240 / hz))
    rows = receipt["body_before_step"]
    assert len(rows) == len(trace)
    first = None
    witness = None
    full_end_error = 0.0
    peak_pose_error = 0.0
    first_error = 0.0
    flight_scalars = 0
    for tick, (row, (poses, ref)) in enumerate(zip(rows, trace, strict=True)):
        assert row["tick"] == tick and row["step"] == (tick > 0), (case, path, tick)
        got = row["parts_after_step"]
        assert len(poses) == len(got) == len(names)
        if first is None:
            cur = packet(row["contacts_after_step"], names)
            expected = packet(ref, names)
            assert cur == expected, (case, path, tick, cur, expected)
            if cur:
                assert cur[1] == EXPECTED[case], (case, cur)
                first = (tick, cur[0], sorted(cur[1]))
                witness = first_witnesses(ref, row["contacts_after_step"], cur[0])
                first_error = max(
                    math.dist(list(map(f32, p["center"])), list(map(f32, q["center"])))
                    for p, q in zip(poses, got)
                )
            else:
                assert all(compare_pose(p, q) == 0.0 for p, q in zip(poses, got)), (
                    case,
                    path,
                    "airborne",
                    tick,
                )
                flight_scalars += 13 * len(poses)
        full_end_error = max(
            math.dist(list(map(f32, p["center"])), list(map(f32, q["center"])))
            for p, q in zip(poses, got)
        )
        peak_pose_error = max(peak_pose_error, full_end_error)
    page = [evt for evt in receipt["physics_events"] if evt["tile"] == TILE]
    ready = next((event["tick"] for event in page if event["state"] == "ready"), None)
    requested = next(
        (event["tick"] for event in page if event["state"] == "requested"), None
    )
    if path == "full":
        assert ready == 0
    if path == "stream":
        assert ready is not None and ready > 0
    hit = receipt["first_contact"]
    assert bool(hit) == bool(EXPECTED[case]) == bool(first)
    if hit:
        assert [hit["tick"], hit["substep"]] == list(first[:2]) and hit[
            "part_name"
        ] in first[2]
        assert names[hit["part"]] == hit["part_name"] and hit["support"] == "west-floor"
        assert hit["tile"] == TILE and receipt["at_contact"]["source_path"] == [0, 1, 2]
        assert hit["normal"][1] > 0.5
        # Shape and integration order can change the contact anchors on the rounded
        # free edge. Record that difference and test both methods against the physical
        # slab boundary and downward flight, independently of their anchor placement.
        for group in (trace[first[0]][1], rows[first[0]]["contacts_after_step"]):
            for c in group:
                if c["substep"] != first[1]:
                    continue
                x, y, z = c["point"]
                assert (
                    16.247 <= x <= 18.253
                    and 8.997 <= z <= 11.003
                    and abs(y - 1.0) < 0.003
                )
                assert c["normal"][1] > 0.5
        if case == "outside-hand":
            pelvis = rows[first[0] - 1]["parts_after_step"][0]["center"][0]
            assert hit["point"][0] >= 16.249 and pelvis < 16.0
            assert pelvis + parts[0]["radius_m"] + parts[0]["half_segment_m"] < 16.25
        if path == "held":
            assert hit["geometry"] == "certified_support"
        if path == "full":
            assert hit["geometry"] == "full"
    assert receipt["support"]["stats"]["held_ticks"] == 0
    return {
        "first": first,
        "first_substep_witness_differences": witness,
        "geometry": hit["geometry"] if hit else None,
        "first_full_page_requested_tick": requested,
        "first_full_page_ready_tick": ready,
        "full_handoffs": receipt["support"]["stats"]["full_handoffs"],
        "first_substep_end_s": (first[0] - 1 + first[1] / actor["substeps_per_tick"])
        / hz
        if first
        else None,
        "first_point_m": hit["point"] if hit else None,
        "first_normal": hit["normal"] if hit else None,
        "source_path": receipt["at_contact"]["source_path"] if hit else None,
        "peak_member_center_difference_m": peak_pose_error,
        "first_pose_max_center_difference_m": first_error if first else None,
        "flight_f32_components_exact": flight_scalars,
        "end_max_member_center_difference_m": full_end_error,
        "final_root_m": rows[-1]["parts_after_step"][0]["center"],
        "held_terrain_ticks": receipt["support"]["stats"]["held_ticks"],
    }


def clearance(city, out, scene, replay, rig):
    glb = runpy.run_path(str(HERE / "check-impact-transmission.py"))
    tree, binary = glb["read_glb"](scene)
    # Reverse just the western face, keeping the physical square in precisely the same place.
    # Its upper side now needs the full collision data before the hand can enter its bounds.
    west = tree["nodes"][2]
    west["scale"] = [1.0, -1.0, 1.0]
    west["translation"][1] += 2.0
    surface = out / "uncertified.glb"
    glb["write_glb"](surface, tree, binary)
    pages = out / "uncertified-district"
    runpy.run_path(str(HERE / "cook-pages.py"))["cook"](surface, pages, 32)
    flags = [
        "--hold",
        "physics",
        "--watch",
        "1,0",
        "--hold-ticks",
        "60",
        "--delay-ms",
        "0",
        "--mbps-per-kind",
        "1000",
    ]
    results = {}
    for model, args in (
        ("human", ["--actor", "human"]),
        ("imported", ["--actor-rig", rig]),
        ("capsule", ["--actor", "capsule"]),
    ):
        data = paged(
            city,
            pages,
            replay,
            "outside-hand",
            args,
            flags,
            out / f"uncertified-{model}.json",
        )
        rows = data["body_before_step"]
        frozen = [r["tick"] for r in rows[1:] if not r["step"]]
        assert data["support"]["stats"]["held_ticks"] == len(frozen)
        if model == "capsule":
            assert not frozen
        else:
            assert frozen and frozen == list(range(frozen[0], frozen[-1] + 1))
            assert frozen[0] < 60 <= frozen[-1] + 1 < rows[-1]["tick"]
            prior = rows[frozen[0] - 1]["parts_after_step"]
            assert all(rows[t]["parts_after_step"] == prior for t in frozen)
            assert rows[frozen[-1] + 1]["parts_after_step"] != prior
            roles = data["actor"]["parts"]
            # At this moment only the stretched left hand can enter the tile's remaining geometry.
            reaching = [
                p["name"]
                for p, pose in zip(roles, prior, strict=True)
                if pose["center"][0] + p["radius_m"] + p["half_segment_m"] >= 16.25
            ]
            assert reaching == ["hand_l"], reaching
            events = [
                e for e in data["support"]["events"] if e["state"] == "clearance_hold"
            ]
            assert len(events) == 1 and events[0]["tile"] == TILE
            assert [s["id"] for s in events[0]["surfaces"]] == ["west-floor"]
        results[model] = {
            "held_ticks": len(frozen),
            "first_held_tick": frozen[0] if frozen else None,
            "resumed_tick": frozen[-1] + 1 if frozen else None,
        }
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    parser.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/paged-ragdoll/physics"
    )
    opts = parser.parse_args()
    out = opts.out.resolve()
    city = opts.bin.resolve()
    scene, pages, rig, template = prepare(out)
    summary = {
        "scene_sha256": sha(scene),
        "rig_sha256": sha(rig),
        "bin_sha256": sha(city),
        "clocks": {},
    }
    for hz in (20, 60):
        replay = routes(out, hz, template)
        results = summary["clocks"][str(hz)] = {}
        for kind, selection in (
            ("human", ["--actor", "human"]),
            ("imported", ["--actor-rig", rig]),
        ):
            base = out / f"{kind}-{hz}"
            base.mkdir(exist_ok=True)
            meta, traces = resident(city, scene, replay, selection, base / "resident")
            names = [p["name"] for p in meta["parts"]]
            assert (
                len(names) == 15
                and sum(p["parent"] is not None for p in meta["parts"]) == 14
            )
            assert abs(sum(p["mass_kg"] for p in meta["parts"]) - 75.0) < 0.001
            result = results[kind] = {
                "substeps_per_tick": meta["substeps_per_tick"],
                "routes": {},
            }
            for case in SCENARIOS:
                paths = result["routes"][case] = {}
                for mode, flags in (
                    ("full", ["--all-resident"]),
                    ("stream", []),
                    (
                        "held",
                        [
                            "--watch",
                            "1,0",
                            "--hold",
                            "physics",
                            "--hold-ticks",
                            str(math.ceil(hz * 1.05)),
                        ],
                    ),
                ):
                    row = paged(
                        city,
                        pages,
                        replay,
                        case,
                        selection,
                        flags,
                        base / f"{case}-{mode}.json",
                    )
                    if kind == "imported":
                        assert row["actor"]["rig_sha256"] == summary["rig_sha256"]
                    paths[mode] = controls(
                        row, traces[case], meta["parts"], case, mode, hz
                    )
                if case == "outside-hand":
                    missing = paged(
                        city,
                        pages,
                        replay,
                        case,
                        selection,
                        [
                            "--watch",
                            "1,0",
                            "--hold",
                            "physics",
                            "--hold-ticks",
                            str(2 * hz),
                            "--late-collision",
                            "unprotected",
                        ],
                        base / f"{case}-unprotected.json",
                    )
                    assert (
                        missing["first_contact"] is None
                        and missing["support"]["stats"]["held_ticks"] == 0
                    )
                    assert all(
                        not row["contacts_after_step"]
                        for row in missing["body_before_step"]
                    )
                    assert missing["final_foot_m"][1] < 0.0
                    paths["unprotected_held"] = {
                        "first": None,
                        "final_foot_m": missing["final_foot_m"],
                    }
        explicit = paged(
            city,
            pages,
            replay,
            "feet",
            ["--actor", "capsule"],
            [],
            out / f"capsule-{hz}.json",
        )
        default = paged(city, pages, replay, "feet", [], [], out / f"default-{hz}.json")
        for key in ("first_contact", "final_foot_m", "body_before_step", "at_contact"):
            assert explicit[key] == default[key], (key, hz)
        results["capsule"] = {
            "default_and_explicit_equal": True,
            "first_tick": default["first_contact"]["tick"],
        }
    summary["hand_clearance_without_certified_face"] = clearance(
        city, out, scene, out / "routes-60.json", rig
    )
    dest = out / "physics-receipt.json"
    dest.write_text(json.dumps(summary, indent=2, allow_nan=False) + "\n")
    print(dest)


if __name__ == "__main__":
    main()
