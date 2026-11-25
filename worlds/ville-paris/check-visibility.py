#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Compare main-camera visibility and real Vulkan pixels on two Bir-Hakeim streets."""

import argparse
import hashlib
import json
import statistics
import struct
import subprocess
import sys
from pathlib import Path

from paging_checks import require

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
FRONT = 1033
TARGET = 1077
CAMERAS = {
    "blocked": ([217, 6.1, 90], [340, 6.1, 90]),
    "open": ([299, 6.1, 90], [340, 6.1, 90]),
    "cross_street": ([217, 6.1, 45], [340, 6.1, 45]),
    "above_roofs": ([217, 52, 90], [340, 13, 90]),
}


def file_digest(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def differ(a, b):
    require(len(a) == len(b), "RGB dimensions differ")
    return sum(a[i : i + 3] != b[i : i + 3] for i in range(0, len(a), 3))


def street_replay():
    replay = json.loads((HERE / "fixtures/descent.json").read_bytes())
    replay["viewport"] = [320, 180]
    replay["routes"] = [
        {
            "id": name,
            "ticks": 8,
            "start_m": [0, 600, 0],
            "velocity_m_s": [0, 0, 0],
            "expected_support": "road",
            "camera": [{"tick": k, "eye": eye, "aim": aim} for k in (0, 8)],
        }
        for name, (eye, aim) in CAMERAS.items()
    ]
    walk = [
        (0, 217, 90),
        (600, 217, 90),
        (1200, 217, 45),
        (2400, 299, 45),
        (5400, 545, 45),
        (8400, 299, 45),
        (9000, 299, 90),
        (9600, 299, 90),
    ]
    replay["routes"].append(
        {
            "id": "walk",
            "ticks": walk[-1][0],
            "start_m": [0, 600, 0],
            "velocity_m_s": [0, 0, 0],
            "expected_support": "road",
            "camera": [
                {"tick": tick, "eye": [x, 6.1, z], "aim": [x + 120, 6.1, z]}
                for tick, x, z in walk
            ],
        }
    )
    return replay


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    cli.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/group-visibility"
    )
    cli.add_argument(
        "--pages", type=Path, help="previously cooked Bir-Hakeim demand pages"
    )
    cli.add_argument(
        "--gpu-lock",
        type=Path,
        help="optional flock path shared with other GPU benchmarks",
    )
    cli.add_argument(
        "--compare",
        type=Path,
        help="compare pixels and costs against an earlier visibility summary",
    )
    a = cli.parse_args()
    out = a.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    exe = a.bin.resolve()
    source = HERE / "fixtures/bir-hakeim.glb"
    pages = a.pages or (out / "pages")
    if not a.pages:
        subprocess.run(
            [
                sys.executable,
                str(HERE / "cook-pages.py"),
                str(source),
                str(pages),
                "--visual-storage",
                "demand",
            ],
            check=True,
        )
    b = source.read_bytes()
    doc = json.loads(b[20 : 20 + struct.unpack_from("<I", b, 12)[0]])
    objects = {}
    for key, n, pos in (
        ("occluder", FRONT, [258, 13.2, 90]),
        ("target", TARGET, [340, 12.2, 90]),
    ):
        node = doc["nodes"][n]
        require(node["translation"] == pos, "authored building moved")
        mesh = doc["meshes"][node["mesh"]]
        objects[key] = {
            "node": n,
            "mesh": mesh["name"],
            "center_m": pos,
            "scale_m": node["scale"],
            "primitives": len(mesh["primitives"]),
        }
    replay = street_replay()
    replayfile = out / "replay.json"
    replayfile.write_text(json.dumps(replay, indent=2) + "\n")
    district_hash = file_digest(source)
    cases = {}
    adapter = None

    def capture(name, mode):
        nonlocal adapter
        label = f"{name}-{mode}"
        gpu, cpu, png = [
            out / (label + ext) for ext in ("-gpu.json", "-reader.json", ".png")
        ]
        cmd = [
            str(exe),
            "--paging",
            "--pages",
            str(pages),
            "--replay",
            str(replayfile),
            "--route",
            name,
            *(
                ["--gpu-stride", "600", "--delay-ms", "200"]
                if name == "walk"
                else ["--all-resident"]
            ),
            "--gpu-trace",
            str(gpu),
            "--out",
            str(cpu),
            "--gpu-groups",
            "on" if mode == "on" else "off",
            "--gpu-owner",
            str(TARGET),
            "--shot",
            str(png),
        ]
        if mode == "withheld":
            cmd.extend(["--gpu-withhold-owner", str(TARGET)])
        if a.gpu_lock:
            cmd = ["flock", str(a.gpu_lock), *cmd]
        with (out / (label + ".log")).open("w") as log:
            subprocess.run(cmd, check=True, stdout=log, stderr=subprocess.STDOUT)
        data = json.loads(gpu.read_bytes())
        require(
            data["source_sha256"] == district_hash, "unexpected source sector in pages"
        )
        adapter = {k: data[k] for k in ("adapter", "driver", "backend")}
        rgb = subprocess.check_output(
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
        reader = json.loads(cpu.read_bytes())
        frames = data["frames"]
        f = frames[-1]
        v = f["group_visibility"]
        require(
            f["frustum_component"] and not f["bevy_hierarchical_z_component"],
            "T2 visibility baseline changed",
        )
        require(
            f["prepared_assets"] == f["geometry_assets"], "incomplete geometry upload"
        )
        require(
            f["render_world_camera_meshes"] == v["camera_candidates"],
            "main-world selection failed to reach renderer",
        )
        return (
            {
                "rgb_sha256": hashlib.sha256(rgb).hexdigest(),
                "latest_completed_opaque_pass": f.get(
                    "latest_completed_opaque_pass", {}
                ),
                "visual_page_io": reader["visual"],
                "stream_samples": [
                    [
                        item["tick"],
                        item["fine_cells"],
                        item["mesh_uploads"],
                        item["evicted_assets"],
                        item["group_visibility"]["watched_primitives_after"],
                        item["render_world_camera_meshes"],
                    ]
                    for item in frames
                ]
                if name == "walk"
                else [],
                "visibility": v,
                "render_world_camera_meshes": f["render_world_camera_meshes"],
                "resident_gpu_meshes": f["prepared_assets"],
                "gpu_payload_bytes": f["drawable_payload_bytes"],
                "source_draw_owners": f["draw_owners"],
                "fine_cells": f["fine_cells"],
                "median_cpu_visibility_us": round(
                    statistics.median(
                        item["group_visibility"]["cpu_ns"] for item in frames[1:]
                    )
                    / 1000,
                    1,
                ),
                "median_wall_to_gpu_finished_ms": round(
                    statistics.median(
                        item["wall_to_finished_gpu_frame_ns"] for item in frames[1:]
                    )
                    / 1e6,
                    3,
                ),
            },
            rgb,
        )

    for name in (*CAMERAS, "walk"):
        off, base = capture(name, "off")
        on, filtered = capture(name, "on")
        diff = differ(base, filtered)
        obs = {"off": off, "on": on, "changed_pixels": diff}
        require(
            diff == 0, f"visible pixels changed with camera filtering at {name}: {diff}"
        )
        require(
            on["gpu_payload_bytes"] == off["gpu_payload_bytes"]
            and on["resident_gpu_meshes"] == off["resident_gpu_meshes"],
            "camera visibility changed scene residency",
        )
        require(
            on["visibility"]["frustum_candidates"]
            == off["visibility"]["frustum_candidates"],
            "Bevy frustum changed",
        )
        if name == "blocked":
            reference = off["latest_completed_opaque_pass"]
            test = on["latest_completed_opaque_pass"]
            if "vertex_shader_invocations" in reference:
                require(
                    test["vertex_shader_invocations"]
                    < reference["vertex_shader_invocations"]
                    and test["clipper_invocations"] < reference["clipper_invocations"],
                    "opaque GPU pass processed as much geometry with groups enabled",
                )
        if name in ("blocked", "open"):
            held, without = capture(name, "withheld")
            changed = differ(base, without)
            obs.update({"withheld": held, "withheld_changed_pixels": changed})
            require(
                off["visibility"]["watched_primitives_before"]
                == objects["target"]["primitives"]
                and held["visibility"]["watched_primitives_after"] == 0,
                "target control failed",
            )
            if name == "blocked":
                require(
                    changed == 0
                    and on["visibility"]["watched_primitives_after"] == 0
                    and on["visibility"]["rejected_cells_with_multiple_owners"] > 0,
                    "occluded target or multi-owner cell remained",
                )
            else:
                require(
                    changed > 0
                    and on["visibility"]["watched_primitives_after"]
                    == objects["target"]["primitives"],
                    "open-street target lost",
                )
        if name == "walk":
            baseline, filtered = off["stream_samples"], on["stream_samples"]
            require(
                [r[:4] for r in baseline] == [r[:4] for r in filtered]
                and off["visual_page_io"] == on["visual_page_io"],
                "a camera decision affected the real file/mesh lifecycle",
            )
            require(
                baseline[0][1] == 0
                and baseline[-1][1] > 0
                and off["visual_page_io"]["evictions"] > 0,
                "walk did not publish and evict detail",
            )
            require(
                filtered[1][4] == 0
                and filtered[-1][4] == objects["target"]["primitives"],
                "streaming walk did not recover the target facade",
            )
            require(
                all(r[5] <= ref[5] for ref, r in zip(baseline, filtered)),
                "filtered render list grew",
            )
        cases[name] = obs
        print(
            name,
            off["render_world_camera_meshes"],
            on["render_world_camera_meshes"],
            diff,
            flush=True,
        )
    result = {
        "format": "ville-paris/group-visibility/1",
        "binary_sha256": file_digest(exe),
        "source_sha256": district_hash,
        "replay_sha256": file_digest(replayfile),
        "renderer": adapter,
        "viewport": replay["viewport"],
        "objects": objects,
        "cases": cases,
    }
    if a.compare:
        prior = json.loads(a.compare.read_bytes())
        require(
            all(
                prior[k] == result[k]
                for k in (
                    "source_sha256",
                    "replay_sha256",
                    "renderer",
                    "viewport",
                    "objects",
                )
            )
            and set(prior["cases"]) == set(cases),
            "camera comparison uses different inputs",
        )
        comparison = {}
        for name, current in cases.items():
            old = prior["cases"][name]
            rows = {}
            for mode in ("off", "on", "withheld"):
                if mode not in current:
                    continue
                before, after = old[mode], current[mode]
                require(
                    before["rgb_sha256"] == after["rgb_sha256"],
                    f"indexed camera changed {name}/{mode} pixels",
                )
                require(
                    before["resident_gpu_meshes"] == after["resident_gpu_meshes"]
                    and before["visual_page_io"] == after["visual_page_io"],
                    "index changed residency",
                )
                rows[mode] = {
                    "cpu_median_before_us": before["median_cpu_visibility_us"],
                    "cpu_median_after_us": after["median_cpu_visibility_us"],
                    "camera_submissions_before": before["render_world_camera_meshes"],
                    "camera_submissions_after": after["render_world_camera_meshes"],
                    "unchanged_rgb": True,
                }
            comparison[name] = rows
        result["comparison"] = {
            "reference_binary_sha256": prior["binary_sha256"],
            "cases": comparison,
        }
    (out / "summary.json").write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
