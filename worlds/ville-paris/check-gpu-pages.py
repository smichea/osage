#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Measure the GPU allocation of the resident geometry on a moving paging replay."""

import argparse
import hashlib
import json
import runpy
import subprocess
import sys
import tempfile
from pathlib import Path

from paging_checks import require

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def digest(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    parser.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/paged-gpu"
    )
    parser.add_argument(
        "--placed",
        type=Path,
        help="optional original-to-Osage visual placement fixture",
    )
    parser.add_argument("--placed-replay", type=Path)
    parser.add_argument("--district-pages", type=Path)
    parser.add_argument("--index-mode", choices=("u32", "compact"), default="compact")
    parser.add_argument(
        "--compare",
        type=Path,
        help="compare matched frames to an earlier u32 summary from this binary",
    )
    a = parser.parse_args()
    require(
        bool(a.placed) == bool(a.placed_replay),
        "both the placed source and its replay are required",
    )
    exe = a.bin.resolve()
    out = a.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    sharing = runpy.run_path(str(HERE / "check-sharing.py"))
    fixture = HERE / "fixtures"
    source, replay = sharing["create_pair"](
        fixture / "bir-hakeim.glb",
        json.loads((fixture / "descent.json").read_bytes()),
        out / "pair",
    )

    def cook(asset, kind, mode, edge=128, base=None):
        target = (base or out / "pages") / kind / mode
        subprocess.run(
            [
                sys.executable,
                str(HERE / "cook-pages.py"),
                str(asset),
                str(target),
                "--edge",
                str(edge),
                "--visual-storage",
                mode,
            ],
            check=True,
        )
        return target

    def run(name, pages, route, timing, options):
        replay = json.loads(timing.read_bytes())
        replay["viewport"] = [320, 180]
        r = out / (name + "-replay.json")
        r.write_text(json.dumps(replay) + "\n")
        metrics = out / (name + "-gpu.json")
        disk = out / (name + "-cpu.json")
        png = out / (name + ".png")
        cmd = [
            str(exe),
            "--paging",
            "--pages",
            str(pages),
            "--route",
            route,
            "--replay",
            str(r),
            "--out",
            str(disk),
            "--gpu-trace",
            str(metrics),
            "--gpu-stride",
            "5",
            "--shot",
            str(png),
            *map(str, options),
        ]
        cmd.extend(["--gpu-indices", a.index_mode])
        with (out / (name + ".log")).open("w") as log:
            subprocess.run(cmd, stdout=log, stderr=subprocess.STDOUT, check=True)
        g = json.loads(metrics.read_bytes())
        c = json.loads(disk.read_bytes())
        require(
            g["source_sha256"] == c["source_sha256"] and g["route"] == route,
            "GPU and reader traces differ",
        )
        require(
            all(f["prepared_assets"] == f["geometry_assets"] for f in g["frames"]),
            "a submitted mesh was missing on GPU",
        )
        pixels = subprocess.check_output(
            [
                "ffmpeg",
                "-loglevel",
                "error",
                "-i",
                str(png),
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgb24",
                "-threads",
                "1",
                "-",
            ]
        )
        return g, c, pixels

    totals = {}
    observations = {}

    def record(tag, g, c, pixels):
        fs = g["frames"]
        f = fs[-1]
        heap = [
            q["memory"]["device_allocated_bytes"]
            for q in fs
            if q["memory"]["device_allocated_bytes"] is not None
        ]
        totals[tag] = {
            "shared_peak_vertex_bytes": max(q["shared_vertex_slice_bytes"] for q in fs),
            "shared_peak_index_bytes": max(q["shared_index_slice_bytes"] for q in fs),
            "scene_peak_mesh_payload_bytes": max(
                q["drawable_payload_bytes"] for q in fs
            ),
            "peak_wgpu_allocated_bytes": max(heap, default=None),
            "slab_capacity_peak": max(
                q["memory"]["mesh_slab_capacity_bytes"] for q in fs
            ),
            "asset_creations": f["mesh_uploads"],
            "uploaded_logical_geometry_bytes": f["mesh_payload_uploaded"],
            "released_assets": f["evicted_assets"],
            "max_live_draw_owners": max(q["draw_owners"] for q in fs),
            "u16_primitives_at_end": f["u16_primitives"],
            "u32_primitives_at_end": f["u32_primitives"],
            "fine_index_slice_bytes_at_end": f["index_slice_bytes"],
            "final_shared_owners": f["shared_draw_owners"],
            "final_shared_primitives": f["shared_primitives"],
            "final_shared_slice_bytes": f["shared_index_slice_bytes"]
            + f["shared_vertex_slice_bytes"],
            "first_physics_contact": c["first_contact"],
            "model_disk": c["storage"]["model_io"],
            "rgb_sha256": hashlib.sha256(pixels).hexdigest(),
            "frames": len(fs),
            "adapter": g["adapter"],
            "driver": g["driver"],
        }

    common = [
        "--visual-radius",
        20,
        "--physics-radius",
        0,
        "--delay-ms",
        0,
        "--mbps-per-kind",
        1000,
    ]
    comparison = {}
    for mode in ("embedded", "demand"):
        p = cook(source, "pair", mode)
        for rt in ("simultaneous", "leave-return"):
            args = common if rt == "leave-return" else [*common, "--tick", 20]
            runid = "pair-" + mode + "-" + rt
            g, c, px = run(runid, p, rt, replay, args)
            record(runid, g, c, px)
            if rt == "simultaneous":
                comparison[mode] = px
                if mode == "demand":
                    require(
                        g["frames"][-1]["shared_draw_owners"] == 2
                        and g["frames"][-1]["shared_primitives"] == 3,
                        "two facade owners require exactly one GPU handle per drawable primitive",
                    )
            elif mode == "demand":
                early = next(q for q in g["frames"] if q["tick"] == 10)
                gap = next(q for q in g["frames"] if q["tick"] == 60)
                late = g["frames"][-1]
                require(
                    early["shared_primitives"] == late["shared_primitives"] == 3
                    and gap["retired_shared_gpu_ids_pending"] == 0
                    and gap["shared_primitives"]
                    == gap["shared_index_slice_bytes"]
                    == gap["shared_vertex_slice_bytes"]
                    == 0,
                    "unused facade GPU handles or ranges survived the last owner",
                )
                require(
                    late["mesh_uploads"] > gap["mesh_uploads"]
                    and c["storage"]["model_io"]["reads"] == 2,
                    "return did not republish after eviction",
                )
                observations["facade"] = {
                    "t10_shared_geometry_bytes": early["shared_vertex_slice_bytes"]
                    + early["shared_index_slice_bytes"],
                    "t60_shared_geometry_bytes": 0,
                    "t160_shared_geometry_bytes": late["shared_vertex_slice_bytes"]
                    + late["shared_index_slice_bytes"],
                }
    require(
        comparison["demand"] == comparison["embedded"],
        "resident facade replay disagrees with embedded frame",
    )
    observations["facade"]["embedded_vs_demand_changed_pixels"] = 0
    # A high index must survive compact mode unchanged, even when the draw contains only three index values.
    cook_fns = runpy.run_path(str(HERE / "cook-pages.py"))
    shape = cook_fns["Glb"]([{}])
    vertices = [(0, 0, 0)] * 65535 + [(1, 0, 0), (0, 0, 1)]
    shape.node(
        0,
        [
            cook_fns["Part"](
                0, vertices, [(0, 1, 0)] * len(vertices), [0, 65536, 65535], {}
            )
        ],
        {},
    )
    with tempfile.TemporaryDirectory() as tmp:
        d = Path(tmp)
        asset = d / "high.glb"
        asset.write_bytes(shape.bytes())
        pp = cook(asset, "high", "demand", base=d)
        data = json.loads(replay.read_bytes())
        rr = data["routes"][0]
        data["routes"] = [{**rr, "expected_support": "none"}]
        r = d / "replay.json"
        r.write_text(json.dumps(data))
        g, c, px = run("high-index", pp, rr["id"], r, ["--all-resident", "--tick", 0])
        record("high-index", g, c, px)
        require(
            g["frames"][-1]["u32_primitives"] == 1
            and g["frames"][-1]["index_slice_bytes"] == 12,
            "an index above 65535 was narrowed",
        )
    if a.district_pages:
        g, c, px = run(
            "district-road",
            a.district_pages,
            "road",
            fixture / "descent.json",
            ["--tick", "66", "--gpu-stride", 6, "--delay-ms", 600],
        )
        record("district-road", g, c, px)
        require(
            c["at_contact"]["landing_tile_has_fine"],
            "district camera did not publish landing cell",
        )
    if a.placed:
        with tempfile.TemporaryDirectory() as private:
            p = cook(a.placed, "placed", "demand", 32, Path(private))
            rp = json.loads(a.placed_replay.read_bytes())
            rt = rp["routes"][0]["id"]
            opts = ["--physics-radius", 0, "--visual-radius", 3, "--delay-ms", 20]
            for name, args in (
                ("tour", opts),
                ("all", ["--all-resident", "--tick", 0]),
            ):
                g, c, px = run("placed-" + name, p, rt, a.placed_replay, args)
                record("placed-" + name, g, c, px)
                require(
                    g["frames"][-1]["shared_primitives"] == 2
                    and c["storage"]["model_io"]["reads"] == 1,
                    "original lamp did not keep its two primitive GPU allocations",
                )
                if name == "tour":
                    resident = set()
                    for event in c["visual_events"]:
                        tile = tuple(event["tile"])
                        if event["state"] == "ready":
                            resident.add(tile)
                        elif event["state"] == "evicted":
                            resident.discard(tile)
                    require(
                        len(resident) == 1,
                        "placed visibility control expects one final detail cell",
                    )
                    x, z = next(iter(resident))
                    control, _, empty = run(
                        "placed-withheld",
                        p,
                        rt,
                        a.placed_replay,
                        [
                            *opts,
                            "--hold",
                            "visual",
                            "--hold-ticks",
                            rp["routes"][0]["ticks"] + 1,
                            "--watch",
                            f"{x},{z}",
                        ],
                    )
                    changes = sum(
                        px[i : i + 3] != empty[i : i + 3] for i in range(0, len(px), 3)
                    )
                    require(
                        control["frames"][-1]["draw_owners"] == 0 and changes > 0,
                        "removing all visible lamps did not change the captured frame",
                    )
                    observations["placed"] = {
                        "withheld_cell": [x, z],
                        "visible_draw_owners": g["frames"][-1]["draw_owners"],
                        "withheld_draw_owners": 0,
                        "changed_pixels": changes,
                    }
                if name == "all":
                    require(
                        g["frames"][-1]["shared_draw_owners"] == 32,
                        "not all original owners were rendered",
                    )
    summary = {
        "format": "ville-paris/paged-gpu-check/1",
        "binary_sha256": digest(exe),
        "index_mode": a.index_mode,
        "cases": totals,
        "checks": observations,
    }
    if a.compare:
        ref = json.loads(a.compare.read_bytes())
        require(
            ref["binary_sha256"] == summary["binary_sha256"]
            and ref["index_mode"] == "u32"
            and set(ref["cases"]) == set(totals),
            "reference must have the same binary and cases with u32 indices",
        )
        comparisons = {}
        for k, t in totals.items():
            b = ref["cases"][k]
            require(
                t["rgb_sha256"] == b["rgb_sha256"],
                "index packing changed rendered pixels in " + k,
            )
            before = b["fine_index_slice_bytes_at_end"]
            after = t["fine_index_slice_bytes_at_end"]
            require(after <= before, "index buffer allocation grew for " + k)
            comparisons[k] = {
                "index_slice_before": before,
                "index_slice_after": after,
                "unchanged_rgb": True,
                "peak_mesh_slab_capacity_before": b["slab_capacity_peak"],
                "peak_mesh_slab_capacity_after": t["slab_capacity_peak"],
                "peak_device_allocated_before": b["peak_wgpu_allocated_bytes"],
                "peak_device_allocated_after": t["peak_wgpu_allocated_bytes"],
            }
        require(
            comparisons["pair-demand-simultaneous"]["index_slice_after"]
            < comparisons["pair-demand-simultaneous"]["index_slice_before"],
            "facade indices did not shrink",
        )
        summary["comparison_with_u32"] = comparisons
    (out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
