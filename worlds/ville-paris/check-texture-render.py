#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Qualify texture pixels, page reads and residency on procedural panels and ground."""

import argparse
import hashlib
import json
import subprocess
import sys
import tempfile
from pathlib import Path

from paging_checks import require

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def changes(a, b):
    return sum(a[i : i + 3] != b[i : i + 3] for i in range(0, len(a), 3))


def colours(raw):
    return [
        list(raw[3 * (320 * y + x) : 3 * (320 * y + x) + 3])
        for x, y in ((80, 50), (240, 50), (80, 130), (240, 130))
    ]


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    cli.add_argument(
        "--sources", type=Path, default=ROOT / "bench/out/ville-paris/texture-storage"
    )
    cli.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/texture-render"
    )
    cli.add_argument("--gpu-lock", type=Path)
    a = cli.parse_args()
    out, src, exe = a.out.resolve(), a.sources.resolve(), a.bin.resolve()
    out.mkdir(parents=True, exist_ok=True)
    if not (src / "summary.json").exists():
        subprocess.run(
            [sys.executable, str(HERE / "check-texture-storage.py"), "--out", str(src)],
            check=True,
        )
    recipe = json.loads((src / "summary.json").read_bytes())
    replay = src / "replay.json"

    def invoke(label, cmd, gpu):
        if gpu and a.gpu_lock:
            cmd = ["flock", str(a.gpu_lock), *cmd]
        with (out / (label + ".log")).open("w") as log:
            subprocess.run(
                list(map(str, cmd)), stdout=log, stderr=subprocess.STDOUT, check=True
            )

    checks = {}
    bitmaps = {}

    def render(
        route,
        mode,
        options=None,
        tick=8,
        label=None,
        pages=None,
        input_replay=None,
        scene=None,
    ):
        name = label or f"{route}-{mode}"
        picture = out / (name + ".png")
        if mode == "source":
            opts = [
                "--scene",
                scene or src / "textured.glb",
                "--route",
                route,
                "--replay",
                input_replay or replay,
                "--actor",
                "capsule",
                "--physics",
                "rapier",
                "--tick",
                tick,
                "--shot",
                picture,
            ]
        else:
            trace, outfile = out / (name + "-gpu.json"), out / (name + "-reader.json")
            opts = [
                "--paging",
                "--pages",
                pages or src / mode,
                "--replay",
                input_replay or replay,
                "--route",
                route,
                "--out",
                outfile,
                "--gpu-trace",
                trace,
                "--gpu-stride",
                30,
                "--gpu-groups",
                "off",
                "--tick",
                tick,
                *(options or ["--all-resident"]),
                "--shot",
                picture,
            ]
        invoke(name, [exe, *opts], True)
        pixels = subprocess.check_output(
            [
                "ffmpeg",
                "-loglevel",
                "error",
                "-i",
                str(picture),
                "-threads",
                "1",
                "-pix_fmt",
                "rgb24",
                "-f",
                "rawvideo",
                "-",
            ]
        )
        row = {"rgb_sha256": digest(pixels), "samples_rgb": colours(pixels)}
        if mode != "source":
            gpu = json.loads(trace.read_bytes())
            cpu = json.loads(outfile.read_bytes())
            frames = gpu["frames"]
            require(
                frames[-1]["prepared_textures"] == frames[-1]["texture_assets"],
                "texture handle unprepared",
            )
            row.update(
                {
                    "adapter": gpu["adapter"],
                    "driver": gpu["driver"],
                    "visual": cpu["visual"],
                    "image_io": cpu["storage"]["image_io"],
                    "GPU_samples": [
                        [f["tick"], f["prepared_textures"], f["texture_payload_bytes"]]
                        for f in frames
                    ],
                    "rejected_resources": cpu["storage"]["rejected_models"],
                }
            )
        checks[name] = row
        return pixels

    # Standalone, v2/v3, the first textured v4 and its current on-demand image layout.
    for route in ("png", "mask", "jpeg", "floor"):
        for mode in ("source", "embedded", "shared", "embedded-local-v4", "demand"):
            bitmaps[route, mode] = render(route, mode)
        levels = {}
        base = bitmaps[route, "demand"]
        for mode in ("source", "embedded", "shared", "embedded-local-v4"):
            diff = changes(base, bitmaps[route, mode])
            levels[mode] = diff
            require(diff == 0, f"{route}/{mode}: {diff} pixels differ from v4")
        checks[route + "-layout-differences"] = levels
        print(route, levels, flush=True)
    for mode in ("source", "embedded", "shared", "embedded-local-v4", "demand"):
        red, green, blue, yellow = colours(bitmaps["png", mode])
        require(
            red[0] > red[1]
            and red[0] > red[2]
            and green[1] > green[0]
            and green[1] > green[2]
            and blue[2] > blue[0]
            and blue[2] > blue[1]
            and yellow[0] > yellow[2]
            and yellow[1] > yellow[2],
            "UV orientation or base colour incorrect in " + mode,
        )
        require(
            changes(bitmaps["png", mode], bitmaps["mask", mode]) > 7000,
            "alpha opening missing",
        )
        require(
            changes(bitmaps["png", mode], bitmaps["jpeg", mode]) > 10000,
            "JPEG gradient missing",
        )
    for route in ("png", "mask", "floor"):
        parent = render(
            route,
            "demand",
            ["--visual-radius", 130 if route == "floor" else 24, "--delay-ms", 5000],
            8,
            route + "-coarse",
        )
        row = checks[route + "-coarse"]
        require(
            row["image_io"]["reads"] == 0 and row["GPU_samples"][-1][1] == 0,
            "coarse scene loaded a detail bitmap",
        )
        delta = changes(parent, bitmaps[route, "demand"])
        require(delta > 8000, "coarse and textured are visually identical")
        row["changed_from_detail_pixels"] = delta
        bitmaps[route, "coarse"] = parent
        if route == "mask":
            require(
                colours(parent)[3] == colours(bitmaps["mask", "demand"])[3],
                "transparent quadrant covered the background",
            )
        if route == "floor":
            offset = (90 * 320 + 160) * 3
            row["centre_coarse"] = list(parent[offset : offset + 3])
            row["centre_detail"] = list(bitmaps[route, "demand"][offset : offset + 3])
            require(
                row["centre_coarse"] != row["centre_detail"],
                "textured floor missing at the centre",
            )

    def cpu_run(route, mode, radius, tick=120):
        name = f"read-{route}-{mode}"
        dest = out / (name + ".json")
        invoke(
            name,
            [
                exe,
                "--paging",
                "--pages",
                src / mode,
                "--replay",
                replay,
                "--route",
                route,
                "--visual-radius",
                radius,
                "--delay-ms",
                0,
                "--mbps-per-kind",
                1000,
                "--tick",
                tick,
                "--out",
                dest,
            ],
            False,
        )
        data = json.loads(dest.read_bytes())
        summary = {
            "visual": data["visual"],
            "image_io": data["storage"]["image_io"],
            "model_events": data["storage"]["model_events"],
        }
        checks[name] = summary
        return summary

    png_bytes = recipe["demand"]["external_images"][0]["bytes"]
    for route, count, encoded, radius in (
        ("away", 0, 0, 24),
        ("png", 1, png_bytes, 24),
        ("both", 1, png_bytes, 56),
        ("mask", 1, png_bytes, 24),
        ("jpeg", 1, recipe["demand"]["external_images"][1]["bytes"], 24),
        ("floor", 1, recipe["demand"]["unique_clipped_image_bytes"], 130),
    ):
        row = cpu_run(route, "demand", radius)
        require(
            row["image_io"]["reads"] == count
            and row["image_io"]["read_bytes"] == encoded,
            f"unexpected image reads for {route}: {row['image_io']}",
        )
    legacy = cpu_run("floor", "embedded-local-v4", 130)
    current = checks["read-floor-demand"]
    before = legacy["visual"]["read_bytes"] + legacy["image_io"]["read_bytes"]
    after = current["visual"]["read_bytes"] + current["image_io"]["read_bytes"]
    require(
        before == recipe["embedded-local-v4"]["cold_clipped_pages_and_bitmap_bytes"]
        and after == recipe["demand"]["cold_clipped_pages_and_bitmap_bytes"],
        "encoded costs and real reader disagree",
    )
    render(
        "return",
        "demand",
        ["--visual-radius", 130, "--delay-ms", 0, "--mbps-per-kind", 1000],
        200,
        "evict-return",
    )
    row = checks["evict-return"]
    samples = {q[0]: q[1:] for q in row["GPU_samples"]}
    require(
        row["image_io"]["reads"] == 2
        and row["image_io"]["evictions"] == 1
        and samples[60][0] == 1
        and samples[90][0] == 0
        and samples[200][0] == 1,
        "leaving and returning did not release and reacquire the image",
    )
    # Fail the content hash on a private copy; all unmodified files remain read-only.
    key = recipe["demand"]["external_images"][0]["file"]
    with tempfile.TemporaryDirectory(dir=out) as copy:
        temp = Path(copy)
        for original in (src / "demand").iterdir():
            local = temp / original.name
            if original.name == key:
                altered = bytearray(original.read_bytes())
                altered[-1] ^= 1
                local.write_bytes(altered)
            else:
                local.symlink_to(original)
        damaged = render(
            "png",
            "demand",
            ["--visual-radius", 24, "--delay-ms", 0],
            8,
            "corrupt-image",
            temp,
        )
    failure = checks["corrupt-image"]
    require(
        failure["image_io"]["rejected"] >= 1
        and failure["GPU_samples"][-1][1] == 0
        and damaged == bitmaps["png", "coarse"],
        "corrupt image reached the camera",
    )
    empty_replay = json.loads(replay.read_bytes())
    empty_replay["routes"] = [
        {**route, "expected_support": "masked"}
        for route in empty_replay["routes"]
        if route["id"] in ("png", "alias")
    ]
    empty_route = out / "empty-replay.json"
    empty_route.write_text(json.dumps(empty_replay) + "\n")
    empty_visual = render(
        "png",
        "demand",
        ["--visual-radius", 24, "--delay-ms", 5000],
        8,
        "empty-parent-coarse",
        src / "empty-parent",
        empty_route,
    )
    empty_fine = render(
        "png",
        "demand",
        ["--all-resident"],
        8,
        "empty-parent-detail",
        src / "empty-parent",
        empty_route,
    )
    empty_receipt = json.loads((out / "empty-parent-coarse-gpu.json").read_bytes())[
        "frames"
    ][-1]
    require(
        empty_receipt["draw_owners"] == 0
        and checks["empty-parent-detail"]["GPU_samples"][-1][1] == 1
        and changes(empty_visual, empty_fine) > 5000,
        "empty coarse scene or its textured descendants failed",
    )
    checks["empty-parent-detail"]["changed_from_empty_parent"] = changes(
        empty_visual, empty_fine
    )
    blend_source = render(
        "alias",
        "source",
        label="blend-source",
        input_replay=empty_route,
        scene=src / "only-cutouts.glb",
    )
    blend_fine = render(
        "alias",
        "demand",
        label="blend-detail",
        pages=src / "empty-parent",
        input_replay=empty_route,
    )
    require(
        blend_source == blend_fine and changes(blend_fine, empty_fine) > 5000,
        "translucent panel differs from its source or duplicates the MASK output",
    )
    checks["blend-detail"]["changed_from_source"] = changes(blend_fine, blend_source)
    checks["blend-detail"]["changed_from_mask"] = changes(blend_fine, empty_fine)
    report = {
        "format": "ville-paris/texture-render/1",
        "source_sha256": digest((src / "textured.glb").read_bytes()),
        "binary_sha256": digest(exe.read_bytes()),
        "replay_sha256": digest(replay.read_bytes()),
        "cases": checks,
        "five_clipped_pages": {"cold_before": before, "cold_after": after},
    }
    (out / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    print("cold clipped pages", before, after, flush=True)


if __name__ == "__main__":
    main()
