#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Compare the real moving support, rig contacts and continuous impact time over resident and streamed Paris."""

import argparse
import csv
import json
import math
import runpy
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
RIG = runpy.run_path(str(HERE / "check-paged-ragdoll.py"))
ORDER = runpy.run_path(str(HERE / "check-ragdoll-contact-order.py"))
FIX = HERE / "fixtures"


def make_routes(out):
    template = json.loads((FIX / "bonnet.json").read_bytes())
    free = {
        "id": "airborne",
        "ticks": 60,
        "start_m": [0.0, 203.0, 0.0],
        "velocity_m_s": [1.2, -5.0, 0.0],
        "bonnet": {
            "center_m": [0.0, 200.0, 0.0],
            "half_extents_m": [1.05, 0.4, 0.82],
            "rotation_xyzw": [0, 0, 0, 1],
            "velocity_m_s": [1.2, 0, 0],
            "mass_kg": 1250,
            "friction": 0.5,
        },
        "expected_support": "bonnet",
        "camera": [
            {"tick": t, "eye": [4.0, 202.0, 8.0], "aim": [0.0, 201.0, 0.0]}
            for t in [0, 60]
        ],
    }
    template["routes"].append(free)
    path = out / "bonnet-replay.json"
    path.write_text(json.dumps(template, indent=2) + "\n")
    return path, template


def resident(city, scene, replay, args, out):
    RIG["run"](
        city,
        "--scene",
        scene,
        "--replay",
        replay,
        "--physics",
        "rapier",
        *args,
        "--bench",
        out,
    )
    return read_resident(out)


def read_resident(out):
    meta = json.loads((out / "dynamics.json").read_bytes())["run"]
    result = {}
    with (out / "dynamics.csv").open() as stream:
        for r in csv.DictReader(stream):
            root = {
                key: [float(r[k]) for k in fields]
                for key, fields in zip(
                    RIG["SOLUTION_KEYS"],
                    [
                        ("cx", "cy", "cz"),
                        ("qx", "qy", "qz", "qw"),
                        ("vx", "vy", "vz"),
                        ("wx", "wy", "wz"),
                    ],
                    strict=True,
                )
            }
            result.setdefault(r["route"], []).append(
                (
                    [root, *json.loads(r["parts_json"])],
                    json.loads(r["boxes_json"]),
                    json.loads(r["contacts_json"]),
                )
            )
    return meta, result


def deltas(a, b):
    return [
        max(
            math.dist(
                tuple(map(RIG["f32"], p["center"])), tuple(map(RIG["f32"], q["center"]))
            )
            for p, q in zip(row, col, strict=True)
        )
        for row, col in zip(a, b, strict=True)
    ]


def inspect(raw, reference, meta, case, who):
    rows = raw["body_before_step"]
    names = [p["name"] for p in meta["parts"]]
    table = raw["contact_surfaces"]
    targets = {s["id"]: i for i, s in enumerate(table)}
    assert table[targets["bonnet"]]["source_node"] is None
    assert len([v for v in table if v["id"] == "bonnet"]) == 1
    native_box = meta["surfaces"].index("bonnet")
    assert len(rows) == len(reference)
    src, boxes, contacts = map(list, zip(*reference, strict=True))
    solved = [r["parts_after_step"] for r in rows]
    moving = [r["boxes_after_step"] for r in rows]
    assert all(len(x) == 1 for x in moving)

    def members(rows, box, body_names):
        for n, pos in enumerate(rows):
            hit = RIG["packet"]([c for c in pos if c["surface"] == box], body_names)
            if hit:
                return [n, hit[0], sorted(hit[1])]
        return None

    actual = members([r["contacts_after_step"] for r in rows], targets["bonnet"], names)
    control = members(contacts, native_box, names)
    assert actual and control and actual == control, (case, who, actual, control)
    earliest = raw["first_contact"]
    assert earliest["geometry"] == "dynamic_box" and earliest["tile"] is None
    assert earliest["support"] == "bonnet" and earliest["source_node"] is None
    outcome = next(x for x in meta["outcomes"] if x["id"] == case)
    times = [
        (r["tick"] - 1) / meta["tick_hz"] + r["swept_impact_offset_s"]
        for r in rows
        if r.get("swept_impact_offset_s") is not None
    ]
    first = times[0] if times else None
    reference_toi = outcome.get("first_swept_impact_s")
    if case == "airborne" and who == "capsule":
        assert first is not None and reference_toi is not None
        assert abs(first - 0.52) < 0.000015 and abs(first - reference_toi) < 1e-6, (
            first,
            reference_toi,
        )
    dr, db = deltas(src, solved), deltas(boxes, moving)
    result = {
        "monolithic_first_bonnet": control,
        "paged_first_bonnet": actual,
        "monolithic_swept_time_s": reference_toi,
        "paged_swept_time_s": first,
        "held_ticks": raw["support"]["stats"]["held_ticks"],
        "actor_pose_sha256": ORDER["pose_hash"](solved),
        "monolithic_actor_sha256": ORDER["pose_hash"](src),
        "box_pose_sha256": ORDER["pose_hash"](moving),
        "monolithic_box_sha256": ORDER["pose_hash"](boxes),
        "first_actor_center_difference_tick": next(
            (t for t, v in enumerate(dr) if v), None
        ),
        "first_box_center_difference_tick": next(
            (t for t, v in enumerate(db) if v), None
        ),
        "actor_at_first_center_error_m": dr[actual[0]],
        "box_at_first_center_error_m": db[actual[0]],
        "peak_actor_center_error_m": max(dr),
        "peak_box_center_error_m": max(db),
        "end_actor_center_error_m": dr[-1],
        "end_box_center_error_m": db[-1],
        "final_box_m": moving[-1][0]["center"],
        "owner_count": len(table),
    }
    if case == "airborne":
        assert result["actor_pose_sha256"] == result["monolithic_actor_sha256"]
        assert result["box_pose_sha256"] == result["monolithic_box_sha256"]
    return result


def intact_road(city, scene, replay, actors, out):
    glb = runpy.run_path(str(HERE / "check-impact-transmission.py"))
    doc, chunk = glb["read_glb"](scene)
    (road,) = [node for node in doc["nodes"] if node.get("name") == "support/road"]
    doc["nodes"] = [road]
    doc["scenes"][doc["scene"]]["nodes"] = [0]
    out.mkdir(exist_ok=True)
    source = out / "road.glb"
    glb["write_glb"](source, doc, chunk)
    pages = out / "pages"
    runpy.run_path(str(HERE / "cook-pages.py"))["cook"](source, pages, 512)
    directory = json.loads((pages / "index.json").read_bytes())
    (tile,) = directory["tiles"]
    assert tile["physics"]["triangles"] == 12 and tile["physics"]["ids"] == ["road"]
    result = {}
    for name, args in actors:
        meta, ref = resident(city, source, replay, args, out / f"{name}-resident")
        for case in ("parked", "moving"):
            trace = RIG["paged"](
                city,
                pages,
                replay,
                case,
                args,
                ["--all-resident"],
                out / f"{name}-{case}.json",
            )
            row = inspect(trace, ref[case], meta, case, name)
            assert row["actor_pose_sha256"] == row["monolithic_actor_sha256"], (
                name,
                case,
                row,
            )
            assert row["box_pose_sha256"] == row["monolithic_box_sha256"], (
                name,
                case,
                row,
            )
            result[f"{name}/{case}"] = row
    return {"source_sha256": RIG["sha"](source), "triangles": 12, "routes": result}


def remote_box(city, out):
    scene, _, _, template = RIG["prepare"](out / "source")
    wall = out / "wall.glb"
    runpy.run_path(str(HERE / "check-collider-handoff.py"))["write_side"](scene, wall)
    pages = out / "pages"
    runpy.run_path(str(HERE / "cook-pages.py"))["cook"](wall, pages, 32)
    ticks = 54
    route = {
        "id": "box-wall",
        "ticks": ticks,
        "start_m": [0, 18, 4],
        "velocity_m_s": [0, 0, 0],
        "expected_support": "bonnet",
        "camera": [
            {"tick": t, "eye": [0, 18, 12], "aim": [0, 18, 4]} for t in (0, ticks)
        ],
        "bonnet": {
            "center_m": [20, 0, 10],
            "half_extents_m": [0.5, 0.3, 0.4],
            "rotation_xyzw": [0, 0, 0, 1],
            "velocity_m_s": [6, 0, 0],
            "mass_kg": 1250,
            "friction": 0.5,
        },
    }
    replay = out / "remote-wall.json"
    replay.write_text(
        json.dumps(dict(template, tick_hz=60, routes=[route]), indent=2) + "\n"
    )
    delay = [
        "--watch",
        "1,0",
        "--hold",
        "physics",
        "--delay-ms",
        "0",
        "--mbps-per-kind",
        "1000",
        "--hold-ticks",
    ]
    result = {}
    for name, flags in [
        ("all", ["--all-resident"]),
        ("held", [*delay, "40"]),
        ("unprotected", [*delay, "55", "--late-collision", "unprotected"]),
    ]:
        raw = RIG["paged"](
            city,
            pages,
            replay,
            "box-wall",
            ["--actor", "human"],
            flags,
            out / f"{name}.json",
        )
        rows = raw["body_before_step"]
        boxes = [r["boxes_after_step"][0] for r in rows]
        assert not any(r["contacts_after_step"] for r in rows)
        held = raw["support"]["stats"]["held_ticks"]
        first = next(
            (n for n, box in enumerate(boxes) if box["linear_velocity"][0] < 5), None
        )
        farthest = max(box["center"][0] for box in boxes)
        result[name] = {
            "held_ticks": held,
            "first_box_slowing_tick": first,
            "maximum_box_center_x_m": farthest,
        }
        if name == "unprotected":
            assert held == 0 and first is None and farthest > 25
        else:
            assert first is not None and farthest < 21.8
        if name == "held":
            assert held > 0 and first == result["all"]["first_box_slowing_tick"] + held
            for row in rows[first - held : first]:
                assert (
                    row["parts_after_step"]
                    == rows[first - held - 1]["parts_after_step"]
                )
                assert (
                    row["boxes_after_step"]
                    == rows[first - held - 1]["boxes_after_step"]
                )
    return result, tick_boundary(city, wall, pages, template, out)


def tick_boundary(city, scene, pages, template, out):
    route = {
        "id": "boundary",
        "ticks": 36,
        "start_m": [7, 11, 10],
        "velocity_m_s": [0.5, -4, 0],
        "expected_support": "bonnet",
        "bonnet": {
            "center_m": [7, 9, 10],
            "half_extents_m": [1.05, 0.4, 0.82],
            "rotation_xyzw": [0, 0, 0, 1],
            "velocity_m_s": [1, 0, 0],
            "mass_kg": 1250,
            "friction": 0.5,
        },
        "camera": [
            {"tick": t, "eye": [10, 10.7, 10], "aim": [7, 9, 10]} for t in (0, 36)
        ],
    }
    replay = out / "tick-boundary.json"
    replay.write_text(
        json.dumps(dict(template, tick_hz=60, routes=[route]), indent=2) + "\n"
    )
    actor = ["--actor", "capsule"]
    meta, ref = resident(city, scene, replay, actor, out / "boundary-resident")
    paged = RIG["paged"](
        city, pages, replay, "boundary", actor, [], out / "boundary-streamed.json"
    )
    result = inspect(paged, ref["boundary"], meta, "boundary", "capsule")
    assert result["paged_first_bonnet"] == [24, meta["substeps_per_tick"], ["capsule"]]
    assert result["actor_pose_sha256"] == result["monolithic_actor_sha256"]
    assert result["box_pose_sha256"] == result["monolithic_box_sha256"]
    rows = paged["body_before_step"]
    start, end = rows[23:25]
    (witness,) = end["contacts_after_step"]
    assert (
        RIG["f32"](ref["boundary"][24][2][0]["applied_offset_s"])
        == witness["applied_offset_s"]
    )
    actor_delta = (
        end["parts_after_step"][0]["linear_velocity"][1]
        - start["parts_after_step"][0]["linear_velocity"][1]
        + 9.81 / 60
    ) * 75
    box_delta = (
        end["boxes_after_step"][0]["linear_velocity"][1]
        - start["boxes_after_step"][0]["linear_velocity"][1]
        + 9.81 / 60
    ) * 1250
    assert actor_delta > 250 and abs(actor_delta + box_delta) < 0.01
    assert 0 <= witness["applied_offset_s"] - end["swept_impact_offset_s"] < 0.000003
    result.update(
        geometry_offset_s=end["swept_impact_offset_s"],
        applied_offset_s=witness["applied_offset_s"],
        separation_m=witness["separation"],
        witness_m=witness["point"],
        vertical_reaction_ns=[actor_delta, box_delta],
    )
    return result


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    p.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/paged-bonnet"
    )
    p.add_argument(
        "--pages",
        type=Path,
        help="Already cooked pages from the public Bir-Hakeim GLB.",
    )
    a = p.parse_args()
    out = a.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    scene = FIX / "bir-hakeim.glb"
    city = a.bin.resolve()
    pages = a.pages
    if pages is None:
        pages = out / "district"
        runpy.run_path(str(HERE / "cook-pages.py"))["cook"](scene, pages, 128)
    pages = pages.resolve()
    idx = json.loads((pages / "index.json").read_bytes())
    assert idx["source_sha256"] == RIG["sha"](scene)
    replay, config = make_routes(out)
    imported = out / "pedestrian.glb"
    RIG["run"](
        sys.executable,
        ROOT / "tools/object-convert/convert.py",
        "--id",
        "pedestrian",
        "--fragment",
        ROOT / "tools/object-convert/fixtures/pedestrian.yft.xml",
        "-o",
        imported,
    )
    report = {
        "bin_sha256": RIG["sha"](city),
        "pages_sha256": RIG["sha"](pages / "index.json"),
        "source_sha256": RIG["sha"](scene),
        "replay_sha256": RIG["sha"](replay),
        "imported_sha256": RIG["sha"](imported),
        "actors": {},
    }
    actors = (
        ("capsule", ["--actor", "capsule"]),
        ("human", ["--actor", "human"]),
        ("imported", ["--actor-rig", imported]),
    )
    for who, args in actors:
        base = out / who
        base.mkdir(exist_ok=True)
        meta, ref = resident(city, scene, replay, args, base / "resident")
        results = report["actors"][who] = {
            "substeps_per_tick": meta["substeps_per_tick"],
            "routes": {},
        }
        for route in config["routes"]:
            case = route["id"]
            trials = results["routes"][case] = {}
            for name, options in (
                ("all", ["--all-resident"]),
                ("stream", []),
                (
                    "held",
                    [
                        "--watch",
                        "0,0",
                        "--hold",
                        "physics",
                        "--hold-ticks",
                        "36",
                        "--delay-ms",
                        "0",
                        "--mbps-per-kind",
                        "1000",
                    ],
                ),
            ):
                data = RIG["paged"](
                    city,
                    pages,
                    replay,
                    case,
                    args,
                    options,
                    base / f"{case}-{name}.json",
                )
                trials[name] = inspect(data, ref[case], meta, case, who)
    report["uncut_road"] = intact_road(city, scene, replay, actors, out / "whole-road")
    report["box_only_barrier"], report["tick_boundary"] = remote_box(
        city, out / "remote"
    )
    (out / "physics-receipt.json").write_text(
        json.dumps(report, indent=2, allow_nan=False) + "\n"
    )
    print(out / "physics-receipt.json")


if __name__ == "__main__":
    main()
