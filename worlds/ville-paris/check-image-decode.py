#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Measure image arrival and simulation-thread time on identical HTTP replays."""

import argparse
import hashlib
import json
from pathlib import Path
import runpy
import statistics
import struct
import subprocess
import sys
import zlib

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
origin = runpy.run_path(str(HERE / "check-http.py"))["origin"]


def texture(size=1024):
    def chunk(tag, body):
        return (
            struct.pack(">I", len(body))
            + tag
            + body
            + struct.pack(">I", zlib.crc32(tag + body))
        )

    rows = []
    for y in range(128):
        pattern = bytearray()
        for x in range(128):
            pattern.extend(
                (
                    (3 * x + y) % 256,
                    (x + 5 * y) % 256,
                    (x * y) % 256,
                    255 if (x // 8 + y // 8) % 2 else 0,
                )
            )
        rows.append(b"\x00" + bytes(pattern) * (size // 128))
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(b"".join(rows) * (size // 128)))
        + chunk(b"IEND", b"")
    )


def district(textured, out):
    src = textured.read_bytes()
    js = struct.unpack_from("<I", src, 12)[0]
    doc = json.loads(src[20 : 20 + js])
    binary = bytearray(src[28 + js :])
    image = texture()
    doc["images"][0]["bufferView"] = len(doc["bufferViews"])
    doc["bufferViews"].append(
        {"buffer": 0, "byteOffset": len(binary), "byteLength": len(image)}
    )
    binary += image
    doc["buffers"][0]["byteLength"] = len(binary)
    binary += bytes(-len(binary) % 4)
    header = json.dumps(doc, separators=(",", ":")).encode()
    header += b" " * (-len(header) % 4)
    payload = (
        struct.pack("<I4s", len(header), b"JSON")
        + header
        + struct.pack("<I4s", len(binary), b"BIN\0")
        + binary
    )
    model = out / "panels.glb"
    model.write_bytes(struct.pack("<4sII", b"glTF", 2, len(payload) + 12) + payload)
    subprocess.run(
        [
            sys.executable,
            HERE / "cook-pages.py",
            model,
            out / "pages",
            "--visual-storage",
            "demand",
        ],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    return hashlib.sha256(image).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    parser.add_argument(
        "--source", type=Path, default=ROOT / "bench/out/ville-paris/texture-storage"
    )
    parser.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/http-image-decode"
    )
    parser.add_argument(
        "--frames",
        action="store_true",
        help="compare both modes and the original panel on the GPU",
    )
    opts = parser.parse_args()
    out = opts.out.resolve()
    out.mkdir(exist_ok=True, parents=True)
    source = opts.source.resolve()
    if not (source / "textured.glb").is_file():
        subprocess.run(
            [sys.executable, HERE / "check-texture-storage.py", "--out", source],
            check=True,
        )
    digest = district(source / "textured.glb", out)
    rows = {"worker": [], "inline": []}
    state = None
    with origin(out / "pages") as (url, records):
        for pair in range(6):
            for mode in ("worker", "inline") if pair % 2 == 0 else ("inline", "worker"):
                dest = out / f"{pair}-{mode}.json"
                start = len(records)
                subprocess.run(
                    [
                        opts.bin,
                        "--paging",
                        "--pages",
                        url,
                        "--route",
                        "png",
                        "--visual-radius",
                        "24",
                        "--replay",
                        source / "replay.json",
                        "--tick",
                        "45",
                        "--image-decode",
                        mode,
                        "--out",
                        dest,
                    ],
                    check=True,
                )
                r = json.loads(dest.read_bytes())
                assert any(x["sha256"] == digest for x in records[start:])
                actual = (r["published"], r["final_foot_m"])
                if state is None:
                    state = actual
                assert state == actual
                remote = r["transport"]["io"]
                io = r["storage"]["image_io"]
                assert io["reads"] == 1 and io["worker_prepared"] == remote[
                    "worker_image_decodes"
                ] == int(mode == "worker")
                times = [
                    e["tick"]
                    for e in r["storage"]["model_events"]
                    if e["sha256"] == digest and e["state"] == "ready"
                ]
                assert len(times) == 1
                if pair:
                    rows[mode].append(
                        {
                            "ready_tick": times[0],
                            "simulation_decode_us": io["simulation_decode_us"],
                            "worker_decode_us": remote["worker_image_decode_us"],
                            "simulation_worst_tick_us": r["transport"]["tick_work_us"][
                                "max"
                            ],
                            "network_content_bytes": remote["content_body_bytes"],
                        }
                    )
    medians = {
        m: {k: statistics.median(v[k] for v in samples) for k in samples[0]}
        for m, samples in rows.items()
    }
    report = {
        "png_sha256": digest,
        "width": 1024,
        "height": 1024,
        "tick_hz": 60,
        "warmup_pairs": 1,
        "alternating_pairs": 5,
        "samples": rows,
        "medians": medians,
        "published": state[0],
    }
    if opts.frames:
        rgb = runpy.run_path(str(HERE / "check-pictures.py"))["rgb"]
        frames = {}
        for mode, directory in (
            ("worker", out / "pages"),
            ("inline", out / "pages"),
            ("original", source / "demand"),
        ):
            with origin(directory) as (url, _):
                image = out / (mode + ".png")
                with (out / (mode + "-gpu.log")).open("w") as log:
                    subprocess.run(
                        [
                            "flock",
                            "-w",
                            "450",
                            "/tmp/osage-ville-paris-gpu.lock",
                            opts.bin,
                            "--paging",
                            "--pages",
                            url,
                            "--route",
                            "png",
                            "--visual-radius",
                            "24",
                            "--replay",
                            source / "replay.json",
                            "--tick",
                            "45",
                            "--gpu-stride",
                            "45",
                            "--image-decode",
                            "inline" if mode == "inline" else "worker",
                            "--gpu-trace",
                            out / (mode + "-gpu.json"),
                            "--shot",
                            image,
                            "--out",
                            out / (mode + "-pages.json"),
                        ],
                        stdout=log,
                        stderr=subprocess.STDOUT,
                        check=True,
                    )
                _, frames[mode] = rgb(image)
        assert frames["worker"] == frames["inline"]
        a, b = frames["worker"], frames["original"]
        changed = sum(a[i : i + 3] != b[i : i + 3] for i in range(0, len(a), 3))
        assert changed > 100
        report["gpu"] = {
            "identical_worker_inline": True,
            "pixels_different_from_original_texture": changed,
            "rgb_sha256": hashlib.sha256(a).hexdigest(),
        }
    (out / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(medians))


if __name__ == "__main__":
    main()
