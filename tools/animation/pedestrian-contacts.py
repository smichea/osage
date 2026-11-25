# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
"""Score the matched, planted and planned native actors in pedestrian_stride --planned --soles CSVs."""
import argparse
import csv
import json
import math
from pathlib import Path
import struct

def clip_durations():
    asset = Path(__file__).resolve().parents[2] / "assets/quaternius/moves/UAL1_Standard.glb"
    with asset.open("rb") as source:
        source.seek(12)
        length, kind = struct.unpack("<I4s", source.read(8))
        assert kind == b"JSON"
        doc = json.loads(source.read(length))
    return {clip["name"]: max(doc["accessors"][s["input"]]["max"][0] for s in clip["samplers"])
            for clip in doc["animations"]}

def score(path, durations):
    with path.open() as stream:
        rows = list(csv.DictReader(stream))
    metrics = {}
    for lane in ("Walk_Loop", "Jog_Fwd_Loop"):
        modes = {mode: [r for r in rows if r["gait"] == lane and r["mode"] == mode]
                 for mode in ("matched", "planted", "planned")}
        reference = modes["matched"]
        assert len(reference) == 480
        for mode, track in modes.items():
            slip = {foot: [] for foot in ("left", "right")}
            depth = {"single": [], "blend": []}
            max_phase_error = 0.
            previous = {}
            reentries = []
            prior = None
            for src, row in zip(reference, track, strict=True):
                assert (row["frame"], row["root_z"], row["active_move"]) == (src["frame"], src["root_z"], src["active_move"])
                assert all(math.isfinite(float(value)) for field, value in row.items()
                           if field not in {"gait", "mode", "active_move"} and value != "")
                frame = int(row["frame"])
                single = int(row["playing_clips"]) == 1
                assert single == (int(src["playing_clips"]) == 1)
                assert bool(row["planned_phase"]) == (mode == "planned" and single and src["active_move"] in ("Walk_Loop", "Jog_Fwd_Loop"))
                if row["planned_phase"]:
                    max_phase_error = max(max_phase_error, abs(float(row["planned_phase"])
                                          - float(src["phase_seconds"]) / durations[src["active_move"]]))
                channel = {"Walk_Loop": "walk_time_s", "Jog_Fwd_Loop": "jog_time_s"}.get(row["active_move"])
                if (prior is not None and channel in row and prior["active_move"] != row["active_move"]
                        and prior[channel] != "" and float(prior[channel.replace("time_s", "weight")]) > 0.):
                    duration = durations[row["active_move"]]
                    expected = float(prior[channel]) + float(row["pace"]) / 60
                    reentries.append(abs(math.remainder(float(row[channel]) - expected, duration)))
                prior = row
                for foot in slip:
                    position = tuple(float(row[foot+"_"+axis]) for axis in ("x", "z"))
                    band = {"Walk_Loop": .035, "Jog_Fwd_Loop": .0225}.get(src["active_move"], -math.inf)
                    low = float(src[foot+"_y"]) - float(src.get("root_y", 0)) <= band
                    key = src["active_move"]
                    if single and low and not 361 <= frame <= 420:
                        if foot in previous and previous[foot][0] == key:
                            slip[foot].append(60 * math.dist(position, previous[foot][1]))
                        previous[foot] = (key, position)
                    else:
                        previous.pop(foot, None)
                    depth["single" if single else "blend"].append(max(0., -float(row[foot+"_sole_clearance"])))
            if "walk_time_s" in track[0]:
                assert reentries and max(reentries) < 2e-6, (path, lane, mode, reentries)
            assert all(slip.values())
            assert max_phase_error < 2e-6, (path, lane, max_phase_error)
            metrics.setdefault(lane, {})[mode] = {
                "max_foot_p95_slip_m_s": max(sorted(v)[int(.95*(len(v)-1))] for v in slip.values()),
                "max_foot_slip_m_s": max(max(v) for v in slip.values()),
                "stance_pairs": {side: len(v) for side, v in slip.items()},
                "single_worst_penetration_mm": 1000 * max(depth["single"]),
                "blend_worst_penetration_mm": 1000 * max(depth["blend"], default=0),
                "blend_frames": len(depth["blend"]) // 2, "max_phase_error_cycles": max_phase_error}
            if reentries:
                metrics[lane][mode]["reentry_max_clock_error_s"] = max(reentries)
    return metrics

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("captures", type=Path, nargs="+")
    parser.add_argument("--check", action="store_true", help="enforce the measured Paris replay budgets")
    args = parser.parse_args()
    durations = clip_durations()
    reports = {str(p): score(p, durations) for p in args.captures}
    if args.check:
        for path, lanes in reports.items():
            for lane, modes in lanes.items():
                planned = modes["planned"]
                failed = (planned["max_foot_p95_slip_m_s"] > 0.005
                          or planned["single_worst_penetration_mm"] > 0.05
                          or planned["blend_worst_penetration_mm"] > 0.05)
                if failed:
                    raise SystemExit(f"foot contact budget exceeded for {path} {lane}: {planned}")
    print(json.dumps(reports, indent=2))
