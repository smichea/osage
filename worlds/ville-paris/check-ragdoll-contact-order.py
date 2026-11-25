#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Check the ordering of static contacts on articulated actors, including two independent supporting pages."""

import argparse
import csv
import hashlib
import json
import math
import runpy
import struct
import sys
from pathlib import Path

from paging_collision_codec import read_collision

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
RIG = runpy.run_path(str(HERE / "check-paged-ragdoll.py"))


def pose_hash(rows):
    digest = hashlib.sha256()
    for frame in rows:
        for part in frame:
            for key in RIG["SOLUTION_KEYS"]:
                values = part[key]
                digest.update(struct.pack(f"<{len(values)}f", *values))
    return digest.hexdigest()


def district(out, scene):
    glb = runpy.run_path(str(HERE / "check-impact-transmission.py"))
    tree, binary = glb["read_glb"](scene)
    # Two separately owned faces end 2 cm either side of the 32 m tile boundary.
    # The feet each arrive on one side; the root is over the gap.
    for node, z, side in [(2, -3.23, "plus"), (3, -5.27, "minus")]:
        owner = tree["nodes"][node]
        owner["translation"][2] = z
        fields = owner["extras"]["osage"]
        fields["id"] = f"span/{side}"
        fields["collision"][0]["id"] = side + "-floor"
        fields["acoustic"][0]["id"] = side + "-sound"
    split = out / "split.glb"
    glb["write_glb"](split, tree, binary)
    pages = out / "split-district"
    runpy.run_path(str(HERE / "cook-pages.py"))["cook"](split, pages, 32)
    idx = json.loads((pages / "index.json").read_bytes())
    by_tile = {
        tuple(t["physics"]["ids"]): (t["x"], t["z"])
        for t in idx["tiles"]
        if t.get("physics")
    }
    assert by_tile == {("plus-floor",): (1, 0), ("minus-floor",): (0, 0)}
    edges = {
        s["id"]: (min(p[0] for p in s["vertices"]), max(p[0] for p in s["vertices"]))
        for t in idx["tiles"]
        for s in read_collision(pages, t["physics"])
    }
    assert (
        abs(edges["minus-floor"][1] - 15.98) < 1e-5
        and abs(edges["plus-floor"][0] - 16.02) < 1e-5
    )
    return split, pages


def single_surface_receipt(path, city):
    report = json.loads(path.read_bytes())
    assert report["bin_sha256"] == RIG["sha"](city)
    controls = []
    for hz, values in report["clocks"].items():
        for actor in ("human", "imported"):
            for route, modes in values[actor]["routes"].items():
                for name, metrics in modes.items():
                    if name == "unprotected_held":
                        continue
                    controls.append((hz, actor, route, name))
                    assert metrics["peak_member_center_difference_m"] == 0.0, (
                        controls[-1],
                        metrics,
                    )
    assert len(controls) == 36
    return {
        "compared_routes": len(controls),
        "maximum_member_center_difference_m": 0.0,
        "receipt_sha256": RIG["sha"](path),
    }


def trace(csv_file):
    result = []
    with csv_file.open() as stream:
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
            result.append([root, *json.loads(r["parts_json"])])
    return result


def split_tests(out, city, source, pages, rig, template):
    view = [
        {"tick": t, "eye": [14.0, 5.0, 15.0], "aim": [16.0, 1.0, 10.0]} for t in (0, 78)
    ]
    replay = out / "two-feet.json"
    replay.write_text(
        json.dumps(
            dict(
                template,
                tick_hz=60,
                routes=[
                    {
                        "id": "two-feet",
                        "ticks": 78,
                        "start_m": [16.0, 3.0, 10.0],
                        "velocity_m_s": [0.0, 0.0, 0.0],
                        "expected_support": "plus-floor",
                        "camera": view,
                    }
                ],
            ),
            indent=2,
        )
        + "\n"
    )
    results = {}
    baseline = ["--delay-ms", "0", "--mbps-per-kind", "1000"]

    def order(tile, t, protected):
        return [
            *baseline,
            "--watch",
            tile,
            "--hold",
            "physics",
            "--hold-ticks",
            str(t),
            "--late-collision",
            "proxy" if protected else "unprotected",
        ]

    options = (
        ("all", ["--all-resident"]),
        ("plus-first", order("0,0", 12, False)),
        ("minus-first", order("1,0", 12, False)),
        ("late-plus", order("1,0", 46, True)),
        ("late-minus", order("0,0", 46, True)),
    )
    for actor, selection in (
        ("human", ["--actor", "human"]),
        ("imported", ["--actor-rig", rig]),
    ):
        saved = out / f"split-{actor}-resident"
        RIG["run"](
            city,
            "--scene",
            source,
            "--replay",
            replay,
            "--physics",
            "rapier",
            *selection,
            "--bench",
            saved,
        )
        ref = json.loads((saved / "dynamics.json").read_bytes())["run"]
        first = ref["outcomes"][0]["first_contacts"]
        assert {
            (h["surface"], tuple(h["parts"]), h["tick"], h["substep"]) for h in first
        } == {("plus-floor", ("foot_l",), 39, 2), ("minus-floor", ("foot_r",), 39, 2)}
        poses = trace(saved / "dynamics.csv")
        controls = results[actor] = {
            "reference_pose_sha256": pose_hash(poses),
            "cases": {},
        }
        for name, flags in options:
            got = RIG["paged"](
                city,
                pages,
                replay,
                "two-feet",
                selection,
                flags,
                out / f"split-{actor}-{name}.json",
            )
            rows = got["body_before_step"]
            assert len(rows) == len(poses) and all(r["step"] for r in rows[1:])
            first_tick = next(r["tick"] for r in rows if r["contacts_after_step"])
            contacts = rows[first_tick]["contacts_after_step"]
            first_substep = min(c["substep"] for c in contacts)
            names = [p["name"] for p in got["actor"]["parts"]]
            surfaces = got["contact_surfaces"]
            participants = {
                (surfaces[c["surface"]]["id"], names[c["part"]])
                for c in contacts
                if c["substep"] == first_substep
            }
            assert (first_tick, first_substep, participants) == (
                39,
                2,
                {("plus-floor", "foot_l"), ("minus-floor", "foot_r")},
            )
            ready = [
                (e["tile"], e["tick"])
                for e in got["physics_events"]
                if e["state"] == "ready"
            ]
            assert len(ready) == 2
            if name in ("plus-first", "minus-first"):
                assert (
                    ready[0][0] == ([1, 0] if name == "plus-first" else [0, 0])
                    and ready[0][1] < ready[1][1] < 39
                )
            if name.startswith("late-"):
                assert (
                    ready[1][1] > 39 and got["support"]["stats"]["full_handoffs"] >= 1
                )
            solved = [r["parts_after_step"] for r in rows]
            assert pose_hash(solved[:first_tick]) == pose_hash(poses[:first_tick])
            deltas = [
                max(
                    math.dist(
                        [RIG["f32"](v) for v in a["center"]],
                        [RIG["f32"](v) for v in b["center"]],
                    )
                    for a, b in zip(original, actual, strict=True)
                )
                for original, actual in zip(poses, solved, strict=True)
            ]
            controls["cases"][name] = {
                "points_at_first_substep": len(
                    [c for c in contacts if c["substep"] == first_substep]
                ),
                "source_members": sorted(participants),
                "page_ready": ready,
                "full_handoffs": got["support"]["stats"]["full_handoffs"],
                "first_tick": first_tick,
                "first_substep": first_substep,
                "pose_sha256": pose_hash(solved),
                "peak_member_center_difference_m": max(deltas),
                "end_member_center_difference_m": deltas[-1],
                "first_position_difference_tick": next(
                    (t for t, d in enumerate(deltas) if d > 0.0), None
                ),
            }
    return results


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    ap.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/ragdoll-contact-order"
    )
    ap.add_argument(
        "--physics-receipt",
        type=Path,
        help="Reuse the ordinary single-plate oracle from this exact binary.",
    )
    opts = ap.parse_args()
    city = opts.bin.resolve()
    out = opts.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    receipt = opts.physics_receipt
    if receipt is None:
        RIG["run"](
            sys.executable,
            HERE / "check-paged-ragdoll.py",
            "--bin",
            city,
            "--out",
            out / "one",
        )
        receipt = out / "one/physics-receipt.json"
    one = single_surface_receipt(receipt, city)
    scene, _, rig, template = RIG["prepare"](out / "source")
    split, pages = district(out, scene)
    report = {
        "bin_sha256": RIG["sha"](city),
        "split_scene_sha256": RIG["sha"](split),
        "one_surface": one,
        "simultaneous": split_tests(out, city, split, pages, rig, template),
    }
    dest = out / "order-receipt.json"
    dest.write_text(json.dumps(report, indent=2) + "\n")
    print(dest)


if __name__ == "__main__":
    main()
