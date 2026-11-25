#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
"""Compare pedestrian_crowd full/view/clip CSVs from the same route and camera."""
import argparse
import csv
import json
import math
import statistics
from pathlib import Path

MODES = ("full", "view", "clip")
FRAMES = 240
WALKERS = 24
TURN = 120


def read(path):
    with path.open() as stream:
        rows = list(csv.DictReader(stream))
    keys = [(int(r["frame"]), int(r["actor"])) for r in rows]
    if keys != [(f, i) for f in range(FRAMES) for i in range(WALKERS)]:
        raise ValueError(f"{path}: expected all 24 actors on 240 frames in order")
    return rows


def project(point, direction):
    x, y, z = point
    focal = 540 / math.tan(math.radians(30))
    depth = direction * z
    return x * focal / depth, (y - 1.65) * focal / depth


def foot(row, name):
    return tuple(float(row[f"{name}_{axis}"]) for axis in "xyz")


def check_poses(data):
    largest = dict(near_before_turn_m=0., near_after_turn_m=0.,
                   visible_distant_px=0., inactive_vs_clip_m=0.)
    for full, view, clip in zip(*(data[m] for m in MODES), strict=True):
        actor, frame = int(view["actor"]), int(view["frame"])
        for key in ("clock", "root_y", "root_z"):
            assert full[key] == view[key] == clip[key], (frame, actor, key)
        direction = 1 if frame < TURN else -1
        visible = direction * float(full["root_z"]) > 0
        near = actor // 6 == (0 if frame < TURN else 2)
        assert full["planned"] == "true" and clip["planned"] == "false"
        assert (view["planned"] == "true") == near, (frame, actor)
        for name in ("left", "right"):
            a, b = foot(full, name), foot(view, name)
            if near:
                key = "near_before_turn_m" if frame < TURN else "near_after_turn_m"
                largest[key] = max(largest[key], math.dist(a, b))
            else:
                raw_error = math.dist(b, foot(clip, name))
                largest["inactive_vs_clip_m"] = max(largest["inactive_vs_clip_m"], raw_error)
                if visible:
                    pixels = math.dist(project(a, direction), project(b, direction))
                    largest["visible_distant_px"] = max(largest["visible_distant_px"], pixels)
    assert largest["near_before_turn_m"] <= 1e-5 and largest["inactive_vs_clip_m"] <= 1e-5, largest
    assert largest["near_after_turn_m"] <= .001 and largest["visible_distant_px"] <= 2, largest
    return largest


def timing_summary(runs, column):
    samples = [float(r[column]) / 1000 for frames in runs for r in frames[12:] if r.get(column)]
    if not samples:
        return None
    turns = [float(frames[TURN][column]) / 1000 for frames in runs if frames[TURN].get(column)]
    return dict(frames=len(samples), median_ms=statistics.median(samples),
                p95_ms=sorted(samples)[math.ceil(.95 * len(samples)) - 1],
                turn_median_ms=statistics.median(turns))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("folder", type=Path)
    folder = parser.parse_args().folder
    data = {mode: read(folder / f"{mode}-count.csv") for mode in MODES}
    calls = {mode: sum(int(r["queries"]) for r in data[mode][::WALKERS]) for mode in MODES}
    assert 0 < calls["view"] < calls["full"] and calls["clip"] == 0, calls
    result = dict(actors=WALKERS, frames=FRAMES, ground_queries=calls, maximum=check_poses(data))
    runs = {mode: [] for mode in MODES}
    maxima = []
    for path in sorted(folder.glob("full-time-*.csv")):
        suffix = path.name.removeprefix("full")
        data = {mode: read(folder / (mode + suffix)) for mode in MODES}
        maxima.append(check_poses(data))
        for mode in MODES:
            runs[mode].append(data[mode][::WALKERS])
    if maxima:
        result["timed_replay_maximum"] = {k: max(m[k] for m in maxima) for k in maxima[0]}
        result["update_wall_time"] = {m: timing_summary(runs[m], "frame_us") for m in MODES}
        result["process_cpu_time"] = {m: timing_summary(runs[m], "cpu_us") for m in MODES}
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
