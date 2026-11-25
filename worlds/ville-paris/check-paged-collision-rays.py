#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Compare the actual resident triangle sets and the unknown space along physical segment probes."""

import argparse
import json
import math
import runpy
import tempfile
from pathlib import Path

from paging_checks import require

HERE = Path(__file__).resolve().parent
FACE = runpy.run_path(str(HERE / "check-source-faces.py"))
PACKED = runpy.run_path(str(HERE / "check-packed-collision-reader.py"))
COOK = runpy.run_path(str(HERE / "cook-pages.py"))["cook"]
MIXED = runpy.run_path(str(HERE / "test_paging_face_order.py"))["mixed_ramp"]


def vertical(x, z, y=5, bottom=1):
    return [[x, y, z], [x, bottom, z]]


def execute(binary, pages, route, label, tick, flags, rays, out):
    args = []
    for start, end in rays:
        args.extend(
            ["--collision-ray", ",".join(map(str, start)), ",".join(map(str, end))]
        )
    path = out / (label + ".json")
    FACE["command"](
        binary,
        "--paging",
        "--pages",
        pages,
        "--replay",
        route,
        "--route",
        "drop",
        "--tick",
        str(tick),
        *flags,
        *args,
        "--out",
        path,
    )
    return PACKED["read"](path)


def observations(probe):
    return [h for h in probe["intersections"] if h["id"] == "ramp"]


def assigned(probe):
    return [h.get("source_triangle") for h in observations(probe)]


def run(binary, out):
    out.mkdir(parents=True, exist_ok=True)
    generated = out / "input"
    generated.mkdir(exist_ok=True)
    source, template, faces, owner, material = FACE["TOPOLOGY"]["prepare"](generated)
    scene, _ = FACE["interleave"](source, generated, faces)
    pages = out / "district"
    COOK(scene, pages, 32)
    x = -54.666667
    z = 13.333333
    missing = (-53.333332, 14.666667)
    centre = vertical(x, z)
    rays = {
        "certified": centre,
        "missing-face": vertical(*missing),
        "edge": vertical(-54, 14),
        "up": centre[::-1],
        "start": vertical(x, z, y=3),
        "end": vertical(x, z, bottom=3),
        "oblique": [[x - 0.25, 5, z], [x + 0.25, 1, z]],
        "remote": vertical(77.333333, 13.333333),
        "outside": vertical(1_000, 1_000),
        "coplanar-missing": [[-53.3, 3, 14.6], [-53.4, 3, 14.5]],
        "coplanar-clear": [[-54.7, 3, 13.3], [-54.6, 3, 13.4]],
        "repeat": centre,
    }
    route = out / "drop.json"
    take = dict(
        template["routes"][0],
        id="drop",
        ticks=90,
        start_m=[x, 5, z],
        velocity_m_s=[0, 0, 0],
        expected_support="ramp",
    )
    take["camera"] = [
        {"tick": n, "eye": [-56, 6, 18], "aim": [x, 3, z]} for n in (0, 90)
    ]
    route.write_text(
        json.dumps(dict(template, tick_hz=60, gravity_m_s2=9.81, routes=[take]))
    )
    summary = {
        "scene_sha256": PACKED["RIG"]["sha"](scene),
        "index_sha256": PACKED["RIG"]["sha"](pages / "index.json"),
    }
    delayed = [
        "--watch",
        "-2,0",
        "--hold",
        "physics",
        "--hold-ticks",
        "55",
        "--delay-ms",
        "0",
        "--physics-radius",
        "1",
        "--physics-plan",
        "trajectory",
    ]
    for mode, tick, flags in [
        ("all", 0, ["--all-resident"]),
        ("certified", 40, delayed),
        ("promoted", 60, delayed),
    ]:
        trace = execute(binary, pages, route, mode, tick, flags, rays.values(), out)
        baseline = execute(
            binary, pages, route, mode + "-without-rays", tick, flags, [], out
        )
        for key in (
            "physics",
            "visual",
            "first_contact",
            "physics_events",
            "support",
            "final_foot_m",
        ):
            require(trace[key] == baseline[key], f"{mode}: probes changed {key}")
        require(
            trace["storage"]["cold_file_bytes"]
            == baseline["storage"]["cold_file_bytes"],
            "physical ray caused I/O",
        )
        samples = dict(zip(rays, trace["collision_probes"], strict=True))
        require(
            samples["repeat"] == samples["certified"],
            "second identical request changed the snapshot",
        )
        is_proxy = mode == "certified"
        for key in ("certified", "up", "start", "end", "oblique"):
            sample = samples[key]
            require(
                assigned(sample) == [4] and sample["coverage"]["complete"],
                f"{mode}/{key}: {sample}",
            )
            (h,) = observations(sample)
            expected = {
                "start": 0.0,
                "end": 2.0,
                "oblique": math.sqrt(2.0**2 + 0.25**2),
            }.get(key, 2.0)
            require(
                abs(h["distance_m"] - expected) < 2e-6
                and abs(h["point_m"][1] - 3.0) < 1e-12,
                f"metric error {h}",
            )
            require(
                h["source_node"] == owner
                and h["material"] == "surface-7"
                and all(
                    math.isclose(
                        h["coefficients"][k], material[k], rel_tol=2e-7, abs_tol=1e-9
                    )
                    for k in ("friction", "restitution")
                ),
                "missing authored physics binding",
            )
            require(
                h["geometry"] == ("certified_support" if is_proxy else "full"),
                f"{mode}: wrong geometry",
            )
        edge, absent = samples["edge"], samples["missing-face"]
        require(
            assigned(edge) == ([4] if is_proxy else [4, 39]),
            f"{mode}: shared edge lost a face",
        )
        require(
            assigned(absent) == ([] if is_proxy else [39]),
            f"{mode}: a missing face was queried as resident",
        )
        require(
            edge["coverage"]["complete"] != is_proxy
            and absent["coverage"]["complete"] != is_proxy,
            "wrong partiality",
        )
        if is_proxy:
            require(
                all(
                    g["reason"] == "residual" and g["source_node"] == owner
                    for g in absent["coverage"]["gaps"]
                ),
                "wrong missing owner",
            )
        require(
            not samples["coplanar-missing"]["intersections"]
            and samples["coplanar-missing"]["coverage"]["complete"] != is_proxy,
            "coplanar missing triangle coverage",
        )
        require(
            samples["coplanar-clear"]["coverage"]["complete"],
            "separated coplanar ray reported missing data",
        )
        empty = samples["outside"]
        require(
            empty["coverage"]["complete"]
            and empty["stats"]["triangles_tested"] == 0
            and not empty["intersections"],
            "outside segment entered unrelated meshes",
        )
        remote = samples["remote"]
        if mode == "all":
            require(
                remote["coverage"]["complete"] and observations(remote),
                "the planned remote point missed the actual ramp",
            )
        else:
            require(
                not remote["coverage"]["complete"]
                and not observations(remote)
                and any(
                    g["reason"] == "missing_page" for g in remote["coverage"]["gaps"]
                ),
                "unloaded page hidden",
            )
        summary[mode] = {
            k: {
                "faces": assigned(s),
                "complete": s["coverage"]["complete"],
                "missing": [g["reason"] for g in s["coverage"]["gaps"]],
                "tested": s["stats"]["triangles_tested"],
            }
            for k, s in samples.items()
        }
        summary[mode]["cold_file_bytes"] = trace["storage"]["cold_file_bytes"]
        summary[mode]["edge_witnesses"] = observations(edge)
        summary[mode]["centre_bvh"] = samples["certified"]["stats"]
    with tempfile.TemporaryDirectory(dir=out) as tmp:
        legacy = Path(tmp)
        PACKED["duplicate_resources"](pages, legacy)
        idx = PACKED["read"](pages / "index.json")
        idx.pop("collision_face_order")
        (legacy / "index.json").write_text(json.dumps(idx))
        trace = execute(
            binary, legacy, route, "legacy", 0, ["--all-resident"], [rays["edge"]], out
        )
        (sample,) = trace["collision_probes"]
        require(
            sample["coverage"]["complete"]
            and assigned(sample) == [None, None]
            and sample["unknown_source_triangle_hits"] == 2,
            "legacy geometry acquired invented authored face numbers",
        )
        summary["legacy_edge"] = [h["local_triangle"] for h in observations(sample)]
    directory = out / "mixed-input"
    directory.mkdir(exist_ok=True)
    duplicate, expected = MIXED(directory)
    repeated = out / "mixed-pages"
    COOK(duplicate, repeated, 32)
    require(expected[0] == expected[1], "lost source duplicate")
    center = [sum(p[i] for p in expected[0]) / 3 for i in range(3)]
    d = execute(
        binary,
        repeated,
        route,
        "duplicates",
        0,
        ["--all-resident"],
        [vertical(center[0], center[2])],
        out,
    )
    (sample,) = d["collision_probes"]
    hits = observations(sample)
    require(
        [h["source_triangle"] for h in hits] == [0, 1]
        and hits[0]["point_m"] == hits[1]["point_m"]
        and hits[0]["distance_m"] == hits[1]["distance_m"],
        "distinct coincident authored faces merged",
    )
    summary["coincident_faces"] = [h["source_triangle"] for h in hits]
    summary["coincident_witnesses"] = hits
    summary["coincident_scene_sha256"] = PACKED["RIG"]["sha"](duplicate)
    summary["coincident_index_sha256"] = PACKED["RIG"]["sha"](repeated / "index.json")
    summary["binary_sha256"] = PACKED["RIG"]["sha"](binary)
    (out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(
        "38 segment probes with exact resident witnesses, missing coverage, ties, old provenance and zero additional reads"
    )


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--bin", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    args = ap.parse_args()
    run(args.bin.resolve(), args.out.resolve())
