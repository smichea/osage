#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Drop a capsule on either side of a missing triangular face while its page is delayed."""
import argparse
import json
import math
import runpy
from pathlib import Path

from paging_checks import require

HERE = Path(__file__).resolve().parent
FACES = runpy.run_path(str(HERE / "check-source-faces.py"))
AUDIO = FACES["AUDIO"]


def setup(out):
    source, replay, faces, _, _ = FACES["TOPOLOGY"]["prepare"](out)
    mixed, _ = FACES["interleave"](source, out, faces)
    pages = out / "pages"
    # Both generations must read the very same fixture to isolate the clearance test.
    AUDIO["COOK"](mixed, pages, 32, coarse_storage="ovc1")
    # The diagonal in the plane of this cell has equation x + z = -40.
    # The capsule has a horizontal radius of 0.28 m, plus the 1 mm sweep margin.
    distances = [-0.7, -0.1, 0.0, 0.1, 0.30, 0.394, 0.401, 0.45, 0.7, 1.2]
    routes = []
    for i, distance in enumerate(distances):
        offset = distance / math.sqrt(2)
        route = dict(replay["routes"][0], id=f"probe-{i}", start_m=[-54-offset, 5, 14-offset],
                     ticks=85, expected_support="ramp")
        route["camera"] = [dict(k, tick=t) for k,t in zip(route["camera"], (0,85), strict=True)]
        routes.append(route)
    path = out / "drops.json"
    path.write_text(json.dumps(dict(replay, routes=routes)))
    return pages, path, distances


def measure(binary, out, pages, replay, distances):
    out.mkdir(exist_ok=True)
    result = []
    for i, distance in enumerate(distances):
        row = {"horizontal_clearance_to_diagonal_m": distance,
               "swept_bounds_cross_omitted_triangle": distance <= math.sqrt(2) * 0.281}
        for mode, flags in [("resident", ["--all-resident"]),
                            ("delayed", ["--watch", "-2,0", "--hold", "physics", "--hold-ticks", "55", "--delay-ms", "0"])]:
            path = out / f"probe-{i}-{mode}.json"
            FACES["command"](binary, "--paging", "--pages", pages, "--replay", replay,
                             "--route", f"probe-{i}", "--trace-body", "--out", path, *flags)
            run = AUDIO["read"](path)
            hit = run["first_contact"]
            parked = [b["tick"] for b in run["body_before_step"] if b["tick"] and not b["step"]]
            require(len(parked) == run["support"]["stats"]["held_ticks"], "hold count disagrees with the actual clock")
            row[mode] = {"held_ticks": len(parked), "held_from": parked[0] if parked else None,
                         "hit": hit, "final_foot_m": run["final_foot_m"]}
        result.append(row)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, required=True)
    parser.add_argument("--reference", type=Path)
    parser.add_argument("--guard", choices=("box", "capsule"), default="capsule",
                        help="box selects the earlier swept-AABB reader for a historical replay")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    pages, replay, distances = setup(out)
    current = measure(args.bin, out/"current", pages, replay, distances)
    old = measure(args.reference, out/"reference", pages, replay, distances) if args.reference else None
    for i, row in enumerate(current):
        held = row["delayed"]["held_ticks"]
        require(row["resident"]["held_ticks"] == 0, "all-resident scene stalled")
        intersects = row["swept_bounds_cross_omitted_triangle"] if args.guard == "box" else distances[i] <= .281
        if intersects:
            require(held == 17, f"omitted triangle unprotected at {distances[i]} m")
        else:
            require(held == 0 and row["delayed"]["hit"]["source_triangle"] == row["resident"]["hit"]["source_triangle"]
                    and row["delayed"]["hit"]["tick"] == row["resident"]["hit"]["tick"] == 39,
                    f"unnecessary stall or changed landing at {distances[i]} m")
        if old:
            require(old[i]["resident"] == row["resident"] and old[i]["delayed"]["held_ticks"] == 17,
                    "control lacks old false hold or resident physics changed")
    (out/"summary.json").write_text(json.dumps({"routes": current, "reference": old}, indent=2) + "\n")
    print("10 diagonal clearances: intersecting sweeps protected, separated sweeps advanced")


if __name__ == "__main__":
    main()
