#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Compare source-triangle ownership and the earlier clipped pages on the same resident replay."""

import argparse
import json
import math
import runpy
from pathlib import Path

from paging_collision_codec import read_collision

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
BENCH = runpy.run_path(str(HERE / "check-paged-bonnet.py"))
RIG = BENCH["RIG"]
FIX = HERE / "fixtures"


def physical_modes():
    return {
        "all": ["--all-resident"],
        "stream": [],
        "held": [
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
    }


def coverage(pages):
    directory = json.loads((pages / "index.json").read_bytes())
    owners = []
    for tile in directory["tiles"]:
        if (layer := tile.get("physics")) and "road" in layer["ids"]:
            body = read_collision(pages, layer)
            (road,) = [shape for shape in body if shape["id"] == "road"]
            vertices = road["vertices"]
            owners.append(
                {
                    "tile": [tile["x"], tile["z"]],
                    "source_node": road.get("source_node"),
                    "faces": len(road["triangles"]),
                    "vertices": len(vertices),
                    "road_bounds": [
                        list(map(min, zip(*vertices))),
                        list(map(max, zip(*vertices))),
                    ],
                    "layer_bounds": layer["bounds"],
                }
            )
    return {
        "source_sha256": directory["source_sha256"],
        "index_sha256": RIG["sha"](pages / "index.json"),
        "tile_edge_m": directory["edge_m"],
        "road_owners": owners,
    }


def compare_city(city, pages, base, out, previous):
    replay = base / "bonnet-replay.json"
    rig = base / "pedestrian.glb"
    out.mkdir(exist_ok=True)
    result = {}
    for name, actor in (
        ("capsule", ["--actor", "capsule"]),
        ("human", ["--actor", "human"]),
        ("imported", ["--actor-rig", rig]),
    ):
        meta, ref = BENCH["read_resident"](base / name / "resident")
        for case in ("parked", "moving", "airborne"):
            for mode, flags in physical_modes().items():
                raw = RIG["paged"](
                    city,
                    pages,
                    replay,
                    case,
                    actor,
                    flags,
                    out / f"{name}-{case}-{mode}.json",
                )
                row = BENCH["inspect"](raw, ref[case], meta, case, name)
                old = previous["actors"][name]["routes"][case][mode]
                assert old["monolithic_actor_sha256"] == row["monolithic_actor_sha256"]
                assert old["monolithic_box_sha256"] == row["monolithic_box_sha256"]
                stats = raw["support"]["stats"]
                result[f"{name}/{case}/{mode}"] = {
                    "previous_peak_actor_error_m": old["peak_actor_center_error_m"],
                    "previous_peak_box_error_m": old["peak_box_center_error_m"],
                    "previous_actor_sha256": old["actor_pose_sha256"],
                    "previous_box_sha256": old["box_pose_sha256"],
                    "retained": stats.get("retained_certified_colliders", 0),
                    "replaced": stats.get("replaced_certified_colliders", 0),
                    "handoffs": [
                        [v["tick"], v["tile"]]
                        for v in raw["support"]["events"]
                        if v["state"] == "full_handoff"
                    ],
                    **row,
                }
                if mode == "all":
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
    return result


def ends(city, pairs, out):
    out.mkdir(exist_ok=True)
    scene = FIX / "bir-hakeim.glb"
    conf = json.loads((FIX / "bonnet.json").read_bytes())
    routes = []
    for case, x in (("west", -111.0), ("east", 111.0)):
        routes.append(
            {
                "id": case,
                "ticks": 60,
                "start_m": [x, 10.0, 8.0],
                "velocity_m_s": [0.0, -2.0, 0.0],
                "expected_support": "road",
                "camera": [
                    {"tick": n, "eye": [x + 4, 12, 14], "aim": [x, 9, 8]}
                    for n in (0, 60)
                ],
            }
        )
    replay = out / "ends.json"
    replay.write_text(json.dumps(dict(conf, routes=routes), indent=2) + "\n")
    result = {}
    for who, actor in (
        ("capsule", ["--actor", "capsule"]),
        ("human", ["--actor", "human"]),
    ):
        meta, _ = BENCH["resident"](city, scene, replay, actor, out / f"{who}-resident")
        for route in routes:
            case = route["id"]
            x = route["start_m"][0]
            ref = next(
                s["first_contacts"][0] for s in meta["outcomes"] if s["id"] == case
            )
            for name, pages, index in pairs:
                owners = [
                    t
                    for t in index["road_owners"]
                    if t["road_bounds"][0][0] <= x <= t["road_bounds"][1][0]
                ]
                assert len(owners) == 1
                home = owners[0]["tile"]
                occupied = [math.floor(x / index["tile_edge_m"] + 0.5), 0]
                if name == "source":
                    assert home != occupied and home == [0, 0]
                for mode, flags in physical_modes().items():
                    flags = [
                        ",".join(map(str, home)) if v == "0,0" else v for v in flags
                    ]
                    data = RIG["paged"](
                        city,
                        pages,
                        replay,
                        case,
                        actor,
                        flags,
                        out / f"{name}-{case}-{who}-{mode}.json",
                    )
                    hit = data["first_contact"]
                    assert (
                        hit["tick"] == ref["tick"]
                        and hit["support"] == ref["surface"] == "road"
                    ), (name, who, case, mode, hit, ref)
                    assert hit["tile"] == home and abs(hit["point"][1] - 8.0) < 0.0001
                    if who == "human":
                        assert hit["substep"] == ref["substep"]
                        names = [p["name"] for p in meta["parts"]]
                        row = data["body_before_step"][hit["tick"]]
                        tick_parts = RIG["packet"](
                            [
                                c
                                for c in row["contacts_after_step"]
                                if data["contact_surfaces"][c["surface"]]["id"]
                                == "road"
                            ],
                            names,
                        )[1]
                        assert tick_parts == set(ref["parts"])
                    result[f"{name}/{who}/{case}/{mode}"] = {
                        "physical_owner": home,
                        "occupied_cell": occupied,
                        "tick": hit["tick"],
                        "substep": hit.get("substep"),
                        "parts": ref["parts"],
                        "geometry": hit["geometry"],
                        "contact_point": hit["point"],
                        "held_ticks": data["support"]["stats"]["held_ticks"],
                    }
    return result


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    ap.add_argument("--old-pages", type=Path, required=True)
    ap.add_argument("--pages", type=Path, required=True)
    ap.add_argument(
        "--previous",
        type=Path,
        help="Existing check-paged-bonnet output from this binary on --old-pages.",
    )
    ap.add_argument(
        "--out",
        type=Path,
        default=ROOT / "bench/out/ville-paris/source-terrain/physics",
    )
    a = ap.parse_args()
    city = a.bin.resolve()
    out = a.out.resolve()
    out.mkdir(exist_ok=True, parents=True)
    previous = a.previous.resolve() if a.previous else out / "old"
    if a.previous is None:
        import sys

        RIG["run"](
            sys.executable,
            HERE / "check-paged-bonnet.py",
            "--bin",
            city,
            "--pages",
            a.old_pages,
            "--out",
            previous,
        )
    before = json.loads((previous / "physics-receipt.json").read_bytes())
    old, new = coverage(a.old_pages), coverage(a.pages)
    assert (
        before["bin_sha256"] == RIG["sha"](city)
        and before["pages_sha256"] == old["index_sha256"]
    )
    assert (
        old["source_sha256"]
        == new["source_sha256"]
        == RIG["sha"](FIX / "bir-hakeim.glb")
    )
    assert old["tile_edge_m"] == new["tile_edge_m"] == 128
    assert len(old["road_owners"]) > 1 and len(new["road_owners"]) == 1
    (lone,) = new["road_owners"]
    assert lone["faces"] == 12 and lone["vertices"] == 24
    result = {
        "binary_sha256": before["bin_sha256"],
        "previous_receipt_sha256": RIG["sha"](previous / "physics-receipt.json"),
        "previous_pages": old,
        "source_pages": new,
        "city": compare_city(city, a.pages, previous, out / "city", before),
        "ends": ends(
            city,
            [("clipped", a.old_pages, old), ("source", a.pages, new)],
            out / "ends",
        ),
    }
    receipt = out / "source-terrain-physics.json"
    receipt.write_text(json.dumps(result, indent=2, allow_nan=False) + "\n")
    print(receipt)


if __name__ == "__main__":
    main()
