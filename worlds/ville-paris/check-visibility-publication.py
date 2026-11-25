#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Measure per-cell camera-index maintenance when district pages and an owner's visible LOD change."""

import argparse
import hashlib
import runpy
import statistics
import subprocess
import sys
from pathlib import Path

from paging_checks import require

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
camera = runpy.run_path(str(HERE / "check-descent-visibility.py"))
street = runpy.run_path(str(HERE / "check-visibility.py"))
read, save, digest, pixels = (camera[k] for k in ("read", "save", "digest", "pixels"))


def capture(exe, mode, case, trial, args, out):
    pages, replay, route, flags = case
    label = f"{route}-{mode}-{trial}"
    gpu, reader, image = [
        out / (label + ext) for ext in ("-gpu.json", "-reader.json", ".png")
    ]
    cmd = [
        "flock",
        args.gpu_lock,
        exe,
        "--paging",
        "--pages",
        pages,
        "--replay",
        replay,
        "--route",
        route,
        "--gpu-trace",
        gpu,
        "--gpu-owner",
        "1077",
        "--out",
        reader,
        *flags,
    ]
    if mode == "full":
        cmd += ["--gpu-scene-index", "full"]
    if trial == 0:
        cmd += ["--shot", image]
    with (out / (label + ".log")).open("w") as f:
        subprocess.run(cmd, check=True, stdout=f, stderr=subprocess.STDOUT, timeout=180)
    r, g = read(reader), read(gpu)
    frames = []
    updates = []
    for f in g["frames"]:
        v = f["group_visibility"]
        require(
            "scene_generation" in v and "highest_certified_roof_m" in v,
            "a reference with district camera publication diagnostics is required",
        )
        frame = {
            "tick": f["tick"],
            "eye": f["camera_eye"],
            "aim": f["camera_aim"],
            "frustum": v["frustum_candidates"],
            "submissions": f["render_world_camera_meshes"],
            "group_candidates": v["camera_candidates"],
            "removed_indices": v["omitted_triangle_indices"],
            "watched_before": v["watched_primitives_before"],
            "watched_after": v["watched_primitives_after"],
            "fine_cells": f["fine_cells"],
            "blockers": v["certified_building_boxes"],
            "highest": v["highest_certified_roof_m"],
            "placed": v["resident_index_placements"],
            "gpu_buffers": f["drawable_payload_bytes"],
            "uploads": f["mesh_uploads"],
            "evictions": f["evicted_assets"],
        }
        require(
            frame["submissions"] == frame["group_candidates"],
            "index failed to reach the render world",
        )
        frames.append(frame)
        if not updates or updates[-1]["generation"] != v["scene_generation"]:
            updates.append(
                {
                    "tick": f["tick"],
                    "generation": v["scene_generation"],
                    "dirty_cells": v.get("last_changed_cells"),
                    "indexed_cells": v.get("last_indexed_cells"),
                    "added": v.get(
                        "last_indexed_placements", v["resident_index_placements"]
                    ),
                    "removed": v.get("last_retired_placements"),
                    "resident": v["resident_index_placements"],
                    "resident_cell_entity_capacity_bytes": v.get(
                        "resident_cell_entity_capacity_bytes"
                    ),
                    "time_us": v["last_resident_index_build_ns"] / 1000,
                }
            )
    return {
        "source_sha256": g["source_sha256"],
        "replay_sha256": g["replay_sha256"],
        "renderer": [g[k] for k in ("adapter", "driver", "backend")],
        "rgb_sha256": hashlib.sha256(pixels(image)).hexdigest() if trial == 0 else None,
        "updates": updates,
        "frames": frames,
        "visual": r["visual"],
        "lod": r["storage"]["visual_lods"],
        "published": r["published"],
        "total_reindexed_after_first": sum(e["added"] for e in updates[1:]),
        "index_us_after_first": sum(e["time_us"] for e in updates[1:]),
        "index_us_start": updates[0]["time_us"],
    }


def check(run, reference, trial):
    keys = (
        "source_sha256",
        "replay_sha256",
        "renderer",
        "frames",
        "visual",
        "lod",
        "published",
    )
    require(
        all(run[k] == reference[k] for k in keys),
        "timed camera, source owner, residency, LOD or GPU submissions changed",
    )
    if trial == 0:
        require(run["rgb_sha256"] == reference["rgb_sha256"], "final RGB differs")
    require(
        [(e["tick"], e["generation"]) for e in run["updates"]]
        == [(e["tick"], e["generation"]) for e in reference["updates"]],
        "publication timeline changed",
    )


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    cli.add_argument(
        "--reference",
        type=Path,
        help="optionally compare the previous production binary's full rebuild",
    )
    cli.add_argument("--pages", type=Path, help="previously cooked Bir-Hakeim district")
    cli.add_argument(
        "--lod-root", type=Path, help="previous output of check-paged-lods.py"
    )
    cli.add_argument(
        "--out",
        type=Path,
        default=ROOT / "bench/out/ville-paris/visibility-publication",
    )
    cli.add_argument(
        "--trials", type=int, default=3, help="interleaved trials for each native mode"
    )
    cli.add_argument(
        "--cases",
        nargs="+",
        choices=["still", "street", "rail", "walk", "travel"],
        default=["still", "street", "rail", "walk", "travel"],
    )
    cli.add_argument(
        "--gpu-lock", type=Path, default=Path("/tmp/osage-ville-paris-gpu.lock")
    )
    args = cli.parse_args()
    require(args.trials >= 1, "positive trials required")
    out = args.out.resolve()
    out.mkdir(exist_ok=True, parents=True)
    pages = (args.pages or out / "pages").resolve()
    if not (pages / "index.json").exists():
        subprocess.run(
            [
                sys.executable,
                HERE / "cook-pages.py",
                HERE / "fixtures/bir-hakeim.glb",
                pages,
            ],
            check=True,
        )
    lod = (args.lod_root or out / "lod-fixture").resolve()
    if "travel" in args.cases and not (lod / "replay.json").exists():
        subprocess.run(
            [
                sys.executable,
                HERE / "check-paged-lods.py",
                "--bin",
                args.bin,
                "--out",
                lod,
            ],
            check=True,
        )
    replay = street["street_replay"]()
    fixed = next(r for r in replay["routes"] if r["id"] == "blocked")
    replay["routes"].extend(
        [
            {**fixed, "id": "still"},
            {
                **fixed,
                "id": "street",
                "ticks": 24,
                "camera": [
                    {**k, "tick": i * 24} for i, k in enumerate(fixed["camera"])
                ],
            },
            next(
                r
                for r in read(HERE / "fixtures/descent.json")["routes"]
                if r["id"] == "rail"
            ),
        ]
    )
    poses = out / "replay.json"
    save(poses, replay)
    plan = {
        "still": (pages, poses, "still", ["--all-resident"]),
        "street": (
            pages,
            poses,
            "street",
            ["--delay-ms", "0", "--visual-radius", "160", "--mbps-per-kind", "8"],
        ),
        "rail": (pages, poses, "rail", ["--gpu-stride", "28", "--delay-ms", "200"]),
        "walk": (pages, poses, "walk", ["--gpu-stride", "600", "--delay-ms", "200"]),
        "travel": (
            lod / "district",
            lod / "replay.json",
            "travel",
            [
                "--gpu-stride",
                "4",
                "--delay-ms",
                "200",
                "--visual-radius",
                "3000",
                "--tick",
                "160",
            ],
        ),
    }
    report = {"binary_sha256": digest(args.bin), "cases": {}}
    if args.reference:
        report["reference_binary_sha256"] = digest(args.reference)
    for name in args.cases:
        case = plan[name]
        observations = {}
        policies = ["full", "incremental"]
        if args.reference:
            policies.insert(0, "previous")
        for i in range(args.trials):
            shift = i % len(policies)
            for mode in policies[shift:] + policies[:shift]:
                executable = args.reference if mode == "previous" else args.bin
                observation = capture(executable, mode, case, i, args, out)
                observations.setdefault(mode, []).append(observation)
            check(observations["incremental"][i], observations["full"][i], i)
            if args.reference:
                for mode in ("full", "incremental"):
                    check(observations[mode][i], observations["previous"][i], i)
        a, b = observations["full"][0], observations["incremental"][0]
        require(
            all(
                x["dirty_cells"] == y["dirty_cells"]
                and y["indexed_cells"] == y["dirty_cells"]
                and y["indexed_cells"] <= x["indexed_cells"]
                and x["resident_cell_entity_capacity_bytes"]
                == y["resident_cell_entity_capacity_bytes"]
                for x, y in zip(a["updates"], b["updates"], strict=True)
            ),
            "indexed cell budget mismatch",
        )
        if name == "still":
            require(
                len(b["updates"]) == 1 and b["total_reindexed_after_first"] == 0,
                "unchanged scene was reindexed",
            )
        if name in ("street", "rail", "walk"):
            require(
                len(b["updates"]) > 1
                and 0
                < b["total_reindexed_after_first"]
                < a["total_reindexed_after_first"],
                "publication did not save placement work",
            )
        if name == "walk":
            require(
                b["visual"]["evictions"] > 0 and b["frames"][-1]["watched_after"] > 0,
                "walk failed to evict and restore the watched building",
            )
        if name == "travel":
            require(
                [x["level"] for x in b["lod"]["events"] if x["level"]]
                == ["high", "medium", "high"],
                "owner did not republish High/Medium/High",
            )
        modes = {}
        for mode, runs in observations.items():
            first = runs[0]
            modes[mode] = {
                "rgb_sha256": first["rgb_sha256"],
                "updates": first["updates"],
                "total_reindexed_after_first": first["total_reindexed_after_first"],
                "max_resident_cell_entity_capacity_bytes": max(
                    (e["resident_cell_entity_capacity_bytes"] or 0)
                    for e in first["updates"]
                )
                if mode != "previous"
                else None,
                "samples_us_start": [r["index_us_start"] for r in runs],
                "median_us_start": statistics.median(r["index_us_start"] for r in runs),
                "samples_us_after_first": [r["index_us_after_first"] for r in runs],
                "median_us_after_first": statistics.median(
                    r["index_us_after_first"] for r in runs
                ),
            }
        report["cases"][name] = {
            "modes": modes,
            "verified_frames": len(b["frames"]),
            "final_submissions": b["frames"][-1]["submissions"],
            "visual": b["visual"],
            "lod": b["lod"],
            "source_sha256": b["source_sha256"],
            "replay_sha256": b["replay_sha256"],
            "renderer": b["renderer"],
        }
        save(out / "summary.json", report)
        print(
            name,
            len(b["updates"]),
            a["total_reindexed_after_first"],
            b["total_reindexed_after_first"],
            round(modes["full"]["median_us_after_first"]),
            round(modes["incremental"]["median_us_after_first"]),
            flush=True,
        )


if __name__ == "__main__":
    main()
