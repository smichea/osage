# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Compare completed-frame receipts only when they describe the same workload and machine."""
import argparse
import json
from pathlib import Path
from statistics import median

MATCH = (
    "format", "inputs", "physics", "actor", "route", "viewport", "vertical_fov_deg", "gpu", "driver", "backend",
    "tier", "upscaling", "sync_pipelines", "tick_hz", "stride_ticks", "warmup_frames", "metric",
)


def read(path):
    data = json.loads(path.read_bytes())
    if data["format"] != "ville-paris/render/1":
        raise ValueError(f"Unexpected receipt format in {path}")
    data.setdefault("physics", "point")
    data.setdefault("actor", "capsule")
    data.setdefault("sync_pipelines", False)
    rows = data["tick_and_ns"]
    ticks = [tick for tick, _ in rows]
    times = sorted(ns for _, ns in rows)
    stride = data["stride_ticks"]
    if not times or times[0] <= 0 or stride <= 0 or ticks[0] != 0:
        raise ValueError(f"Invalid samples in {path}")
    # The last tick may complete a shorter stride.
    if ticks != [*range(0, ticks[-1], stride), ticks[-1]]:
        raise ValueError(f"Missing or repeated frame in {path}")
    quantiles = {f"p{p}": times[(len(times) * p + 99) // 100 - 1] for p in (50, 95, 99)}
    if data["ns"] != {**quantiles, "samples": len(times)}:
        raise ValueError(f"Frame statistics disagree with the raw times in {path}")
    return {field: data[field] for field in MATCH}, ticks, quantiles


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--control", type=Path, nargs="+", required=True)
    parser.add_argument("--trial", type=Path, nargs="+", required=True)
    args = parser.parse_args()
    if len(args.control) != len(args.trial):
        parser.error("Each trial needs its paired control")
    first, expected_ticks, _ = read(args.control[0])
    pairs = []
    for control, trial in zip(args.control, args.trial):
        pair = {}
        for label, path in (("control", control), ("trial", trial)):
            settings, ticks, quantiles = read(path)
            mismatch = [field for field in MATCH if settings[field] != first[field]]
            if mismatch or ticks != expected_ticks:
                raise ValueError(f"Different workload in {path}: {mismatch or 'frame ticks'}")
            pair[label] = quantiles
        pairs.append(pair)
    p95 = {label: median(pair[label]["p95"] for pair in pairs) for label in ("control", "trial")}
    print(json.dumps({
        "settings": first, "frames_per_run": len(expected_ticks), "paired_ns": pairs,
        "median_run_p95_ns": p95, "p95_reduction_fraction": 1 - p95["trial"] / p95["control"],
        "pairs_with_lower_trial_p95": sum(p["trial"]["p95"] < p["control"]["p95"] for p in pairs),
    }, indent=2))


if __name__ == "__main__":
    main()
