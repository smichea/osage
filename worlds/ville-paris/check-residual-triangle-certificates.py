#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Check conservative cooker fallback and invalid triangle clearance certificates in the normal player."""

import argparse
import json
import runpy
import subprocess
import tempfile
from pathlib import Path

from paging_checks import require
from paging_residual_triangles import observe

HERE = Path(__file__).resolve().parent
PACKED = runpy.run_path(str(HERE / "check-packed-collision-reader.py"))
RIG, CORNER = PACKED["RIG"], PACKED["CORNER"]


def cooking():
    key = (0, (0, 0, 0), (4, 0, 4))
    group = {}
    points = [(0, 0, 0), (4, 0, 0), (0, 0, 4)]
    observe(group, key, points)
    require(
        group[key] == (1, 264),
        "one source triangle did not retain its real three corners",
    )
    observe(group, key, [(4, 0, 4), points[1], points[2]])
    require(
        group[key] == (2, None),
        "a complete rectangle received a partial clearance certificate",
    )
    group.clear()
    observe(group, key, [(0, 0, 0), (4, 0, 2), (2, 0, 4)])
    require(
        group[key] == (1, None),
        "non-corner coordinates were rounded into an empty-space claim",
    )
    observe(group, key, points)
    require(
        group[key] == (2, None),
        "an unknown first triangle permitted a later clearance claim",
    )


def invalid(binary, pages, receipt, reason):
    r = subprocess.run(
        [
            binary,
            "--paging",
            "--all-resident",
            "--pages",
            pages,
            "--replay",
            HERE / "fixtures/bonnet.json",
            "--route",
            "parked",
            "--out",
            receipt,
        ],
        capture_output=True,
        check=False,
    )
    require(
        r.returncode != 0 and reason in r.stderr.decode(),
        f"{r.returncode}: {r.stderr.decode()}",
    )


def test(binary, source, mixed, out):
    cooking()
    result = {
        "binary_sha256": RIG["sha"](binary),
        "inputs": [RIG["sha"](p / "index.json") for p in [source, mixed]],
        "rejected": {},
    }
    physical = [
        t for t in PACKED["read"](source / "index.json")["tiles"] if t.get("physics")
    ]
    page = next(i for i, t in enumerate(physical) if (t["x"], t["z"]) == (-2, 0))

    def change(data, how):
        rows = data["residual_triangle_corners"]
        if how == "two-omitted-triangles":
            p, r = next(
                (p, j)
                for p, page_rows in enumerate(data["residual"])
                for j, (_, _, n) in enumerate(page_rows)
                if n > 1
            )
            rows.append([p, r, 264])
            rows.sort()
        elif how == "duplicate":
            rows.insert(0, rows[0])
        elif how == "bad-bits":
            rows[0][2] |= 512
        else:
            r = next(
                i
                for i, (low, high, _) in enumerate(data["residual"][page])
                if low == [-56, 3, 12] and high == [-52, 3, 16]
            )
            row = next(row for row in rows if row[:2] == [page, r])
            require(
                row[2] == 353,
                "fixture no longer holds the upper-right omitted triangle",
            )
            row[2] = 264  # Same box but opposite triangle.

    for name, original, cause in [
        ("two-omitted-triangles", mixed, "requires exactly one omitted face"),
        ("duplicate", source, "unordered residual triangle certificates"),
        ("bad-bits", source, "invalid residual triangle corners"),
        (
            "wrong-three-corners",
            source,
            "certificate differs from full collision geometry",
        ),
    ]:
        with tempfile.TemporaryDirectory(dir=out) as tmp:
            pages = Path(tmp)
            CORNER["variant"](
                original, pages, lambda data, kind=name: change(data, kind)
            )
            invalid(binary, pages, out / (name + "-all.json"), cause)
            result["rejected"][name] = cause
            if name != "wrong-three-corners":
                continue
            # The false half claims the landing point is occupied. Keep the page
            # delayed until contact, then require a verified full retry to release it.
            replay = PACKED["read"](HERE / "fixtures/descent.json")
            take = dict(
                replay["routes"][0],
                id="drop",
                ticks=175,
                start_m=[-54.666667, 5, 13.333333],
                velocity_m_s=[0, 0, 0],
                expected_support="ramp",
            )
            take["camera"] = [
                {"tick": n, "eye": [-55, 6, 18], "aim": [-54, 3, 13]} for n in (0, 175)
            ]
            route = out / "drop.json"
            route.write_text(
                json.dumps(dict(replay, tick_hz=60, gravity_m_s2=9.81, routes=[take]))
            )
            trace = RIG["paged"](
                binary,
                pages,
                route,
                "drop",
                ["--actor", "capsule"],
                [
                    "--watch",
                    "-2,0",
                    "--hold",
                    "physics",
                    "--hold-ticks",
                    "55",
                    "--delay-ms",
                    "0",
                ],
                out / "recovered.json",
            )
            denied = trace["support"]["rejected_full_pages"]
            require(
                len(denied) == 1 and cause in denied[0]["error"],
                "misbound certificate was not revoked",
            )
            ready = next(
                e["tick"]
                for e in trace["physics_events"]
                if e["tile"] == [-2, 0] and e["state"] == "ready"
            )
            held = [
                r["tick"]
                for r in trace["body_before_step"]
                if r["tick"] > 0 and not r["step"]
            ]
            require(
                len(held) == trace["support"]["stats"]["held_ticks"],
                "hold count differs from solver steps",
            )
            require(
                denied[0]["tick"] in held and max(held) == ready - 1,
                "simulation resumed before verified recovery",
            )
            hit = trace["first_contact"]
            require(
                hit["geometry"] == "full"
                and hit["tick"] == ready
                and hit["source_triangle"] == 4,
                f"unexpected recovered contact {hit}",
            )
            result["recovery"] = {
                "rejected_tick": denied[0]["tick"],
                "first_held_tick": min(held),
                "last_held_tick": max(held),
                "held_count": len(held),
                "verified_full_tick": ready,
                "source_triangle": hit["source_triangle"],
            }
            # Move to the genuinely omitted half. This time the bad certificate
            # claims it is clear. Arrange arrival at the first swept overlap itself.
            take["start_m"] = [-53.333332, 5, 14.666667]
            route = out / "crossed-drop.json"
            route.write_text(
                json.dumps(dict(replay, tick_hz=60, gravity_m_s2=9.81, routes=[take]))
            )
            crossed = RIG["paged"](
                binary,
                pages,
                route,
                "drop",
                ["--actor", "capsule"],
                [
                    "--watch",
                    "-2,0",
                    "--hold",
                    "physics",
                    "--hold-ticks",
                    "38",
                    "--delay-ms",
                    "0",
                ],
                out / "crossed-recovery.json",
            )
            (failure,) = crossed["support"]["rejected_full_pages"]
            attempted = failure["tick"]
            held = [
                r
                for r in crossed["body_before_step"]
                if r["tick"] > 0 and not r["step"]
            ]
            hit = crossed["first_contact"]
            require(
                attempted == 39
                and [r["tick"] for r in held] == list(range(attempted, hit["tick"])),
                "the invalidated empty-space forecast let the solver advance in the rejection tick",
            )
            require(
                hit["geometry"] == "full"
                and hit["source_triangle"] == 39
                and all(r["foot_m"] == held[0]["foot_m"] for r in held),
                "unexpected motion before the true missing triangle was available",
            )
            result["same_tick_revocation"] = {
                "rejected_tick": attempted,
                "held_ticks": len(held),
                "foot_y_during_hold_m": held[0]["foot_m"][1],
                "verified_full_tick": hit["tick"],
                "source_triangle": hit["source_triangle"],
            }
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("bin", "interleaved", "mixed", "out"):
        parser.add_argument("--" + name, type=Path, required=True)
    a = parser.parse_args()
    binary, pages, mixed, out = (
        p.resolve() for p in (a.bin, a.interleaved, a.mixed, a.out)
    )
    out.mkdir(parents=True, exist_ok=True)
    result = test(binary, pages, mixed, out)
    (out / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
    print(
        "4 malformed or inconsistent certificates rejected, 2 safe recoveries including same-tick revocation"
    )
