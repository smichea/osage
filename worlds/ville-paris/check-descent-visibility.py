#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Compare native visibility, GPU primitive counts and pixels along the rail descent and two streets."""

import argparse
import hashlib
import json
import os
import statistics
import subprocess
import sys
from pathlib import Path

from paging_checks import require

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
TARGET = "1077"  # A real building visible at the end of the open street.


def digest(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def read(path):
    return json.loads(path.read_bytes())


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def pixels(image):
    return subprocess.check_output(
        [
            "ffmpeg",
            "-loglevel",
            "error",
            "-i",
            image,
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-threads",
            "1",
            "-",
        ]
    )


def snapshot_controls(exe, pages, replayfile, out, lock, cases):
    results = {}
    for pose in ("high", "blocked", "open", "descent"):
        if pose not in cases:
            continue
        normal = None
        entry = {}
        for withhold in [False, True] if pose in ("blocked", "open") else [False]:
            label = pose + ("-plain-withheld" if withhold else "-plain")
            image = out / (label + ".png")
            cmd = [
                "flock",
                lock,
                exe,
                "--paging",
                "--replay",
                replayfile,
                "--route",
                pose,
                "--pages",
                pages,
                "--shot",
                image,
                "--out",
                out / (label + "-reader.json"),
            ]
            cmd += ["--delay-ms", "200"] if pose == "descent" else ["--all-resident"]
            if withhold:
                cmd.extend(["--gpu-withhold-owner", TARGET])
            with (out / (label + ".log")).open("w") as log:
                subprocess.run(
                    cmd, check=True, stdout=log, stderr=subprocess.STDOUT, timeout=120
                )
            rgb = pixels(image)
            if withhold:
                require(normal is not None, "missing plain view")
                changed = sum(
                    normal[i : i + 3] != rgb[i : i + 3] for i in range(0, len(rgb), 3)
                )
                require(
                    (changed > 0) == (pose == "open"),
                    f"withholding the known target did not distinguish the street views: {pose}/{changed}",
                )
                entry.update(
                    {"withheld_owner": int(TARGET), "withheld_changed_pixels": changed}
                )
            else:
                normal = rgb
                checksum = hashlib.sha256(rgb).hexdigest()
                require(
                    checksum == cases[pose]["default"]["rgb_sha256"],
                    f"plain capture differs from traced camera at {pose}",
                )
                entry.update({"rgb_sha256": checksum, "identical_to_trace": True})
        results[pose] = entry
    return results


def stream_snapshot(exe, pages, replayfile, out, lock):
    values = {}
    # Hold the street camera still while fine pages replace the resident parent under it.
    for mode in ("on", "auto"):
        name = "reloading-" + mode
        gpu = out / (name + "-gpu.json")
        png = out / (name + ".png")
        cmd = [
            "flock",
            lock,
            exe,
            "--paging",
            "--pages",
            pages,
            "--replay",
            replayfile,
            "--route",
            "blocked",
            "--gpu-groups",
            mode,
            "--gpu-owner",
            TARGET,
            "--gpu-trace",
            gpu,
            "--shot",
            png,
            "--out",
            out / (name + "-reader.json"),
            "--visual-radius",
            "160",
            "--mbps-per-kind",
            "8",
            "--delay-ms",
            "0",
        ]
        with (out / (name + ".log")).open("w") as log:
            subprocess.run(
                cmd, check=True, stdout=log, stderr=subprocess.STDOUT, timeout=120
            )
        frames = read(gpu)["frames"]
        values[mode] = {
            "input": [
                [
                    f["tick"],
                    f["fine_cells"],
                    f["mesh_uploads"],
                    f["evicted_assets"],
                    f["render_world_camera_meshes"],
                    f["group_visibility"]["omitted_triangle_indices"],
                ]
                for f in frames
            ],
            "generations": [
                f["group_visibility"].get("scene_generation") for f in frames
            ],
            "reused": [
                f["group_visibility"].get("reused_visibility", False) for f in frames
            ],
            "rgb_sha256": hashlib.sha256(pixels(png)).hexdigest(),
        }
    a, b = values["on"], values["auto"]
    require(
        a["input"] == b["input"] and a["rgb_sha256"] == b["rgb_sha256"],
        "stationary streaming camera changed geometry or pixels",
    )
    g = b["generations"]
    require(
        g[0] is not None and len(set(g)) > 1 and b["input"][-1][1] > b["input"][0][1],
        "stationary test did not publish new detail",
    )
    require(
        all(not b["reused"][i] for i in range(1, len(g)) if g[i] != g[i - 1]),
        "resident publication reused older geometry",
    )
    return b


def frame_times(frames):
    # The render diagnostics expose an asynchronously completed pass, in milliseconds.
    settled = frames[4:]
    gpu = [
        f["latest_completed_opaque_pass"]["elapsed_gpu"]
        for f in settled
        if "elapsed_gpu" in f["latest_completed_opaque_pass"]
    ]
    return {
        "median_wall_to_completed_frame_ms": statistics.median(
            f["wall_to_finished_gpu_frame_ns"] for f in settled
        )
        / 1e6,
        "median_opaque_gpu_ms": statistics.median(gpu) if gpu else None,
    }


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    cli.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/descent-visibility"
    )
    cli.add_argument(
        "--pages", type=Path, help="previously cooked clean Bir-Hakeim district"
    )
    cli.add_argument(
        "--modes",
        nargs="+",
        choices=("off", "on", "default"),
        default=("off", "on", "default"),
    )
    cli.add_argument(
        "--poses",
        nargs="+",
        choices=("high", "middle", "approach", "blocked", "open", "cross", "descent"),
        default=("high", "middle", "approach", "blocked", "open", "cross", "descent"),
    )
    cli.add_argument(
        "--gpu-lock", type=Path, default=Path("/tmp/osage-ville-paris-gpu.lock")
    )
    cli.add_argument(
        "--controls", type=Path, help="reuse the off/on runs from an earlier directory"
    )
    cli.add_argument(
        "--snapshot-controls",
        action="store_true",
        help="also capture without a GPU trace and remove a known building as a negative control",
    )
    args = cli.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    exe = args.bin.resolve()
    pages = (args.pages or out / "pages").resolve()
    if not (pages / "index.json").is_file():
        subprocess.run(
            [
                sys.executable,
                HERE / "cook-pages.py",
                HERE / "fixtures/bir-hakeim.glb",
                pages,
            ],
            check=True,
        )
    replay = read(HERE / "fixtures/descent.json")
    rail = next(r for r in replay["routes"] if r["id"] == "rail")
    cameras = {
        k: (rail["camera"][n]["eye"], rail["camera"][n]["aim"])
        for k, n in (("high", 0), ("middle", 2), ("approach", 3))
    }
    cameras.update(
        {
            "blocked": ([217, 6.1, 90], [340, 6.1, 90]),
            "open": ([299, 6.1, 90], [340, 6.1, 90]),
            "cross": ([217, 6.1, 45], [340, 6.1, 45]),
        }
    )
    replay["viewport"] = [320, 180]
    replay["routes"] = [{**rail, "id": "descent"}] + [
        {
            **rail,
            "id": k,
            "ticks": 12,
            "camera": [{"tick": n, "eye": eye, "aim": aim} for n in (0, 12)],
        }
        for k, (eye, aim) in cameras.items()
    ]
    replayfile = out / "replay.json"
    save(replayfile, replay)
    study = {
        "binary_sha256": digest(exe),
        "source_sha256": digest(HERE / "fixtures/bir-hakeim.glb"),
        "index_sha256": digest(pages / "index.json"),
        "replay_sha256": digest(replayfile),
        "cases": {},
    }
    if args.controls:
        previous = read(args.controls / "summary.json")
        require(
            all(
                study[k] == previous[k]
                for k in ("source_sha256", "index_sha256", "replay_sha256")
            ),
            "visibility comparison uses different district or poses",
        )
        study["control_binary_sha256"] = previous["binary_sha256"]
    for pose in args.poses:
        modes = {}
        if args.controls:
            for name, reference in previous["cases"][pose].items():
                if name in ("off", "on"):
                    modes[name] = {
                        **reference,
                        "image": os.path.relpath(
                            args.controls / reference["image"], out
                        ),
                    }
        for mode in args.modes:
            label = pose + "-" + mode
            native = out / (label + "-gpu.json")
            reader = out / (label + "-reader.json")
            image = out / (label + ".png")
            command = [
                "flock",
                args.gpu_lock,
                exe,
                "--paging",
                "--pages",
                pages,
                "--replay",
                replayfile,
                "--route",
                pose,
                "--gpu-owner",
                TARGET,
                "--gpu-trace",
                native,
                "--out",
                reader,
                "--shot",
                image,
            ]
            command += (
                ["--gpu-stride", "28", "--delay-ms", "200"]
                if pose == "descent"
                else ["--all-resident"]
            )
            if mode != "default":
                command += ["--gpu-groups", mode]
            with (out / (label + ".log")).open("w") as log:
                subprocess.run(
                    command,
                    check=True,
                    stdout=log,
                    stderr=subprocess.STDOUT,
                    timeout=120,
                )
            g, r = read(native), read(reader)
            require(
                g["source_sha256"] == study["source_sha256"]
                and g["replay_sha256"] == study["replay_sha256"],
                "unexpected render input",
            )
            driver = [g[k] for k in ("adapter", "driver", "backend")]
            if args.controls:
                require(
                    driver == previous["renderer"], "GPU/driver differs from controls"
                )
            study["renderer"] = driver
            frames = g["frames"]
            require(
                all(
                    f["render_world_camera_meshes"]
                    == f["group_visibility"]["camera_candidates"]
                    and f["frustum_component"]
                    for f in frames
                ),
                "main camera output differs from render world",
            )
            raw = pixels(image)
            rows = []
            for f in frames:
                hardware = f["latest_completed_opaque_pass"]
                v = f["group_visibility"]
                rows.append(
                    {
                        "tick": f["tick"],
                        "eye": f["camera_eye"],
                        "aim": f["camera_aim"],
                        "frustum": v["frustum_candidates"],
                        "submitted_meshes": f["render_world_camera_meshes"],
                        "watched_before": v["watched_primitives_before"],
                        "watched_after": v["watched_primitives_after"],
                        "omitted_indices": v["omitted_triangle_indices"],
                        "face_tests": v["volume_tests"],
                        "tested_boxes": v["tested_boxes_limit"],
                        "automatic": v.get("automatic"),
                        "scene_generation": v.get("scene_generation"),
                        "reused": v.get("reused_visibility", False),
                        "cpu_us": v["cpu_ns"] / 1000,
                        "fine_cells": f["fine_cells"],
                        "uploaded": f["mesh_uploads"],
                        "evicted": f["evicted_assets"],
                        "resident_bytes": f["drawable_payload_bytes"],
                        "clipper_triangles": hardware.get("clipper_invocations"),
                        "vertex_shader_invocations": hardware.get(
                            "vertex_shader_invocations"
                        ),
                    }
                )
            settled = rows[4:]
            require(
                settled and settled[-1]["clipper_triangles"] is not None,
                "GPU did not report clipper work",
            )
            modes[mode] = {
                "rgb_sha256": hashlib.sha256(raw).hexdigest(),
                "frames": rows,
                **frame_times(frames),
                "median_visibility_cpu_us": statistics.median(
                    x["cpu_us"] for x in settled
                ),
                "visual_io": r["visual"],
                "lod": r["storage"]["visual_lods"],
                "published": r["published"],
                "last_clipper_triangles": rows[-1]["clipper_triangles"],
                "image": str(image.relative_to(out)),
            }
            print(
                label,
                rows[-1]["submitted_meshes"],
                rows[-1]["clipper_triangles"],
                round(modes[mode]["median_visibility_cpu_us"], 1),
                flush=True,
            )
        if args.controls:
            prior = previous["cases"][pose]
            for mode in args.modes:
                if mode in prior:
                    require(
                        modes[mode]["rgb_sha256"] == prior[mode]["rgb_sha256"],
                        f"reference pixels changed at {pose}/{mode}",
                    )
                    for new, old in zip(
                        modes[mode]["frames"], prior[mode]["frames"], strict=True
                    ):
                        require(
                            all(
                                new[k] == old[k]
                                for k in (
                                    "frustum",
                                    "submitted_meshes",
                                    "omitted_indices",
                                    "watched_after",
                                )
                            ),
                            f"spatial mask changed {pose}/{mode} visibility at tick {new['tick']}",
                        )
        if "default" in args.modes and "on" in modes and "off" in modes:
            prior = previous["cases"][pose] if args.controls else modes
            for i, frame in enumerate(modes["default"]["frames"]):
                required = prior["on" if frame["automatic"] else "off"]["frames"][i]
                require(
                    all(
                        frame[k] == required[k]
                        for k in (
                            "submitted_meshes",
                            "omitted_indices",
                            "watched_after",
                        )
                    ),
                    f"automatic policy differs from its forced control at {pose}/{frame['tick']}",
                )
            if pose in ("blocked", "open", "cross", "approach"):
                require(
                    any(f["reused"] for f in modes["default"]["frames"][4:]),
                    "stationary repeated poses did not reuse the proven camera selection",
                )
            if pose == "descent":
                require(
                    not any(f["reused"] for f in modes["default"]["frames"]),
                    "moving camera reused a different pose",
                )
            if pose in ("high", "middle"):
                require(
                    all(
                        f["automatic"] is False and f["face_tests"] == 0
                        for f in modes["default"]["frames"]
                    ),
                    "automatic tested buildings above the measured roofs",
                )
            if pose in ("blocked", "open", "cross", "approach"):
                require(
                    modes["default"]["last_clipper_triangles"]
                    < prior["off"]["last_clipper_triangles"],
                    "automatic failed to reduce hardware triangle processing",
                )
                require(
                    modes["default"]["last_clipper_triangles"]
                    == prior["on"]["last_clipper_triangles"],
                    "hardware pass differs from forced conservative control",
                )
        if "off" in modes:
            baseline = modes["off"]
            for name, candidate in modes.items():
                if name == "off":
                    continue
                require(
                    candidate["rgb_sha256"] == baseline["rgb_sha256"],
                    f"visible pixels changed at {pose}/{name}",
                )
                require(
                    all(
                        candidate[k] == baseline[k]
                        for k in ("visual_io", "lod", "published")
                    ),
                    f"streaming state changed at {pose}/{name}",
                )
                for a, b in zip(baseline["frames"], candidate["frames"], strict=True):
                    require(
                        all(
                            a[k] == b[k]
                            for k in (
                                "tick",
                                "eye",
                                "aim",
                                "frustum",
                                "fine_cells",
                                "uploaded",
                                "evicted",
                                "resident_bytes",
                            )
                        ),
                        "camera or resident geometry changed",
                    )
                    require(
                        b["submitted_meshes"] <= a["submitted_meshes"],
                        "visibility added a mesh",
                    )
        study["cases"][pose] = modes
        save(out / "summary.json", study)
    if args.snapshot_controls:
        require("default" in args.modes, "snapshot controls need the normal camera")
        study["untraced_views"] = snapshot_controls(
            exe, pages, replayfile, out, args.gpu_lock, study["cases"]
        )
        study["stationary_publication"] = stream_snapshot(
            exe, pages, replayfile, out, args.gpu_lock
        )
        save(out / "summary.json", study)


if __name__ == "__main__":
    main()
