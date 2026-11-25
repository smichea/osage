#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Compare the scalar reference with Pixels::read, alternating fresh pinned processes."""

import argparse
import hashlib
import json
import os
import random
import runpy
import statistics
import subprocess
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
TEST = "texture::verification::measure_pixels_read"


def test_binary():
    cmd = [
        "cargo",
        "test",
        "--offline",
        "--release",
        "--locked",
        "-p",
        "osage-ville-paris",
        "--bin",
        "ville-paris",
        "--no-run",
        "--message-format=json",
    ]
    rows = subprocess.check_output(cmd, cwd=ROOT, text=True).splitlines()
    return Path(
        next(
            r["executable"]
            for r in map(json.loads, rows)
            if r.get("executable")
            and r.get("profile", {}).get("test")
            and r["target"]["name"] == "ville-paris"
        )
    )


def inputs(output):
    large = output / "1024x1024.png"
    large.write_bytes(runpy.run_path(str(HERE / "check-image-decode.py"))["texture"]())
    small = output / "32x32.png"
    small.write_bytes(
        runpy.run_path(str(HERE / "check-texture-storage.py"))["png"](
            bytes((100, 140, 100, 255)) * 1024
        )
    )
    return [("large", large), ("small", small)]


def describe(samples):
    ordered = sorted(samples)

    def percentile(p):
        at = (len(ordered) - 1) * p
        left = int(at)
        right = min(left + 1, len(ordered) - 1)
        return round(
            (ordered[left] + (at - left) * (ordered[right] - ordered[left])) / 1000, 3
        )

    return {
        "count": len(samples),
        "median_us": round(statistics.median(samples) / 1000, 3),
        "p10_us": percentile(0.1),
        "p90_us": percentile(0.9),
    }


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument(
        "--test-bin", type=Path, help="compiled Ville-Paris release test executable"
    )
    cli.add_argument(
        "--input",
        nargs=2,
        action="append",
        metavar=("NAME", "IMAGE"),
        help="encoded PNG or JPEG; may repeat",
    )
    cli.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/mip-prepare"
    )
    cli.add_argument(
        "--rounds", type=int, default=15, help="paired fresh processes per image"
    )
    cli.add_argument(
        "--cpu",
        type=int,
        default=min(os.sched_getaffinity(0)),
        help="Linux CPU affinity for both implementations",
    )
    opt = cli.parse_args()
    out = opt.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    if opt.rounds < 1 or opt.cpu not in os.sched_getaffinity(0):
        cli.error("expected positive rounds and an available CPU")
    exe = (opt.test_bin or test_binary()).resolve()
    for name in (
        "texture::mipmap::all_linear_u16_codes_match_the_bevy_transfer_function",
        "texture::verification::rgba_and_level_count_match_for_odd_even_and_one_dimensional_inputs",
    ):
        p = subprocess.check_output([exe, "--exact", name], text=True)
        if "1 passed" not in p:
            raise RuntimeError(f"test missing: {name}")
    chosen = (
        [(name, Path(p).resolve()) for name, p in opt.input]
        if opt.input
        else inputs(out)
    )
    if len({name for name, _ in chosen}) != len(chosen):
        cli.error("input names must be unique")
    samples = {}
    results = {}
    metadata = {}
    tasks = [
        (name, path, round) for name, path in chosen for round in range(opt.rounds)
    ]
    rng = random.Random(139)
    rng.shuffle(tasks)
    for name, path, round_index in tasks:
        modes = ["scalar", "current"] if round_index % 2 == 0 else ["current", "scalar"]
        for mode in modes:
            # Keep startup, file reads and the independent reference outside the measured interval.
            count = (
                4
                if path.stat().st_size >= 100_000
                else (15 if path.stat().st_size >= 10_000 else 51)
            )
            env = {
                **os.environ,
                "OSAGE_MIP_INPUT": str(path),
                "OSAGE_MIP_METHOD": mode,
                "OSAGE_MIP_COUNT": str(count),
            }
            log = subprocess.check_output(
                [
                    "taskset",
                    "-c",
                    str(opt.cpu),
                    exe,
                    "--exact",
                    TEST,
                    "--ignored",
                    "--nocapture",
                ],
                env=env,
                text=True,
            )
            line = next(l for l in log.splitlines() if "OSAGE_MIP_MEASUREMENT " in l)
            receipt = json.loads(line.partition("OSAGE_MIP_MEASUREMENT ")[2])
            timing = receipt.pop("samples_ns")
            cpu = receipt.pop("cpu_ns")
            e = samples.setdefault(name, {}).setdefault(mode, [])
            e.append(
                {
                    "round": round_index,
                    "cold_ns": timing[0],
                    "warm_ns": timing[1:],
                    "cpu_cold_ns": cpu[0],
                    "cpu_warm_ns": cpu[1:],
                }
            )
            if metadata.setdefault(name, receipt) != receipt:
                raise RuntimeError("input or RGBA receipt differs between processes")
    for name, _ in chosen:
        row = {**metadata[name], "timings": {}}
        for method, s in samples[name].items():
            row["timings"][method] = {
                "cold": describe([r["cold_ns"] for r in s]),
                "warm": describe([n for r in s for n in r["warm_ns"]]),
                "cpu_cold": describe([r["cpu_cold_ns"] for r in s]),
                "cpu_warm": describe([n for r in s for n in r["cpu_warm_ns"]]),
            }
        a, b = row["timings"]["scalar"], row["timings"]["current"]
        row["speedup"] = {
            kind: round(a[kind]["median_us"] / b[kind]["median_us"], 3)
            for kind in ("cold", "warm", "cpu_cold", "cpu_warm")
        }
        results[name] = row
    with exe.open("rb") as f:
        binary = hashlib.file_digest(f, "sha256").hexdigest()
    cpuinfo = Path("/proc/cpuinfo").read_text().splitlines()
    name = next(
        (i.split(":", 1)[1].strip() for i in cpuinfo if i.startswith("model name")),
        None,
    )
    result = {
        "format": "ville-paris/mip-prepare/1",
        "methods": ["original scalar sRGB and imageops", "Pixels::read"],
        "timing": "Wall and decoding-thread CPU clocks include image decompression, minimum alpha, RGBA mip generation, and encoded digest. The encoded input is in memory. Byte parity is checked outside the measured interval.",
        "cpu": name,
        "pinned_cpu": opt.cpu,
        "rounds": opt.rounds,
        "binary_sha256": binary,
        "linear_u16_verified": 65536,
        "dimension_pairs_verified": 28 * 28 + 4,
        "inputs": results,
    }
    (out / "samples.json").write_text(json.dumps(samples) + "\n")
    (out / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({k: v["speedup"] for k, v in results.items()}))


if __name__ == "__main__":
    main()
