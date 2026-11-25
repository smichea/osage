#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Exercise invalid authored face ranges, revoked proxy labels and legacy-to-full handoff."""

import argparse
import json
import runpy
import subprocess
import tempfile
from pathlib import Path

from paging_checks import require

HERE = Path(__file__).resolve().parent
PACKED = runpy.run_path(str(HERE / "check-packed-collision-reader.py"))
RIG, CORNER = PACKED["RIG"], PACKED["CORNER"]
REPLAY = HERE / "fixtures" / "bonnet.json"


def rejected(binary, pages, expected, out):
    run = subprocess.run(
        [
            binary,
            "--paging",
            "--all-resident",
            "--pages",
            pages,
            "--replay",
            REPLAY,
            "--route",
            "parked",
            "--out",
            out,
        ],
        capture_output=True,
        check=False,
    )
    require(
        run.returncode != 0 and expected in run.stderr.decode(),
        f"{run.returncode}: {run.stderr.decode()}",
    )
    return expected


def invalid_index(binary, ramps, out):
    controls = {}
    for label, expected in [
        ("empty-run", "source face ranges overlap, touch or exceed"),
        (
            "cross-page-overlap",
            "collision fragments overlap or leave missing source faces",
        ),
        ("unknown-format", "unknown collision face ordering"),
    ]:
        with tempfile.TemporaryDirectory(dir=out) as tmp:
            pages = Path(tmp)
            PACKED["duplicate_resources"](ramps, pages)
            idx = PACKED["read"](ramps / "index.json")
            layer = next(
                t["physics"]
                for t in idx["tiles"]
                if t.get("physics", {}).get("source_face_runs")
            )
            runs = layer["source_face_runs"][0][2]
            if label == "empty-run":
                runs[0][1] = 0
            elif label == "cross-page-overlap":
                require(
                    runs[0][0] > 0,
                    "need a fragment that begins after the first source face",
                )
                runs[0][0] -= 1
            else:
                idx["collision_face_order"] = 2
            (pages / "index.json").write_text(json.dumps(idx))
            controls[label] = rejected(binary, pages, expected, out / (label + ".json"))
    return controls


def reports(trace, owner):
    lookup = trace["contact_surfaces"]
    result = []
    for row in trace["body_before_step"]:
        for c in row["contacts_after_step"]:
            s = lookup[c["surface"]]
            if (s["id"], s["source_node"]) == ("road", owner):
                result.append((row["tick"], c.get("source_triangle")))
    require(result, "character did not contact the road")
    return result


def check_road(binary, source, out):
    idx = PACKED["read"](source / "index.json")
    physical = [t for t in idx["tiles"] if t.get("physics")]
    order = next(i for i, t in enumerate(physical) if (t["x"], t["z"]) == (0, 0))
    layer = physical[order]["physics"]
    slot = layer["ids"].index("road")
    owner = layer["source_nodes"][slot]
    result = {}
    for name in ("legacy-catalogue", "incorrect-certified-triangles"):

        def alter(catalogue, kind=name):
            if kind == "legacy-catalogue":
                catalogue.pop("face_order")
                return
            # The two upper road triangles are 2 and 3. Replace their labels with
            # two valid lower ordinals. Keep the upper-only proxy so comparison with
            # the full file is the first opportunity to disprove the supplied labels.
            table = catalogue["corner_meshes"]
            before = len(table["placements"])
            table["placements"] = [
                r for r in table["placements"] if (r[0], r[1]) != ([0, 0], slot)
            ]
            require(
                len(table["placements"]) == before - 1, "wrong complete-road placement"
            )
            selections = catalogue["face_order"]
            require(selections["start"] == 2, "unexpected certified top ordinal")
            rows = selections.setdefault("exceptions", [])
            rows.append([order, slot, [[0, 2]]])
            rows.sort()

        with tempfile.TemporaryDirectory(dir=out) as tmp:
            pages = Path(tmp)
            CORNER["variant"](source, pages, alter)
            trace = RIG["paged"](
                binary,
                pages,
                REPLAY,
                "parked",
                ["--actor", "human"],
                [
                    "--watch",
                    "0,0",
                    "--hold",
                    "physics",
                    "--hold-ticks",
                    "90",
                    "--delay-ms",
                    "0",
                ],
                out / (name + ".json"),
            )
            hits = reports(trace, owner)
            ready = next(
                e["tick"]
                for e in trace["physics_events"]
                if e["tile"] == [0, 0] and e["state"] == "ready"
            )
            denied = trace["support"]["rejected_full_pages"]
            if name == "legacy-catalogue":
                require(not denied, "legacy catalogue rejected a valid page")
                stages = [
                    (0, ready, {None}),
                    (ready, len(trace["body_before_step"]), {2, 3}),
                ]
            else:
                cause = "certified face ordinal differs from full collision geometry"
                require(
                    len(denied) == 1 and cause in denied[0]["error"],
                    "mismatched geometry did not revoke proxy labels",
                )
                reject = denied[0]["tick"]
                rejected(binary, pages, cause, out / "bad-certified-all.json")
                stages = [
                    (0, reject, {0, 1}),
                    (reject, ready, {None}),
                    (ready, len(trace["body_before_step"]), {2, 3}),
                ]
            timeline = []
            for lo, hi, allowed in stages:
                values = [face for tick, face in hits if lo <= tick < hi]
                require(
                    values and set(values) <= allowed,
                    f"{name} [{lo},{hi}): {set(values)} vs {allowed}",
                )
                timeline.append(
                    {
                        "ticks": [lo, hi],
                        "road_contacts": len(values),
                        "source_triangles": sorted(
                            set(values), key=lambda v: -1 if v is None else v
                        ),
                    }
                )
            result[name] = {
                "full_ready_tick": ready,
                "rejections": denied,
                "observations": timeline,
            }
    return result


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__)
    for name in ("bin", "district", "ramps", "out"):
        ap.add_argument("--" + name, type=Path, required=True)
    args = ap.parse_args()
    binary, district, ramps, out = (
        p.resolve() for p in (args.bin, args.district, args.ramps, args.out)
    )
    out.mkdir(parents=True, exist_ok=True)
    receipt = {
        "binary_sha256": RIG["sha"](binary),
        "district_index_sha256": RIG["sha"](district / "index.json"),
        "ramp_index_sha256": RIG["sha"](ramps / "index.json"),
        "invalid_directory": invalid_index(binary, ramps, out),
        "certified_and_full": check_road(binary, district, out),
    }
    (out / "summary.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(
        "3 rejected directory declarations, legacy handoff and revoked certified triangle labels"
    )
