#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Compare seeded, cold page reads to fully resident Rapier contacts on the same Paris inputs."""

import argparse
import hashlib
import json
import math
import random
import subprocess
import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
DEFAULT_REPLAY = HERE / "fixtures/descent.json"
HEIGHTS = {"rail": 16.0, "road": 8.0, "roof-grenelle": 32.0}


def digest(path):
    with path.open("rb") as src:
        return hashlib.file_digest(src, "sha256").hexdigest()


def fall_time(distance, down, gravity, terminal):
    t = (terminal - down) / gravity
    to_terminal = down * t + 0.5 * gravity * t * t
    if distance > to_terminal:
        return t + (distance - to_terminal) / terminal
    return 2 * distance / (down + math.sqrt(down * down + 2 * gravity * distance))


def route_for(base, index, seed):
    rng = random.Random((seed << 32) + index)
    support = tuple(HEIGHTS)[index % 3]
    level = HEIGHTS[support]
    if support == "roof-grenelle":
        x, z = 188 + rng.uniform(-10, 10), -72 + rng.uniform(-4, 4)
        vx, vz = rng.uniform(-1.8, 1.8), rng.uniform(-0.7, 0.7)
    else:
        # Include deliberate approaches to both physical page boundaries across the deck.
        x = (
            (rng.choice([-64, 64]) + rng.uniform(-0.08, 0.08))
            if index % 5 == 0
            else rng.uniform(-105, 105)
        )
        z = (
            rng.uniform(-1.5, 1.5)
            if support == "rail"
            else rng.choice([-1, 1]) * rng.uniform(5, 9)
        )
        vx, vz = rng.uniform(-4, 4), rng.uniform(-0.45, 0.45)
    if support == "road":
        distance, down = rng.uniform(2, 4.8), rng.uniform(0, 4)
    else:
        distance, down = rng.uniform(60, 600), rng.uniform(0, 45)
    t = fall_time(distance, down, base["gravity_m_s2"], base["terminal_m_s"])
    hz = base["tick_hz"]
    expected = round(t * hz)
    last = expected + 170
    camera = [
        {"tick": tick, "eye": [x + side * 28, y, z + 42], "aim": [x, level, z]}
        for tick, side, y in [
            (0, 1, level + distance + 25),
            (max(1, expected // 2), -1, level + 40),
            (last, 1, level + 16),
        ]
    ]
    return {
        "id": str(index),
        "expected_support": support,
        "ticks": last,
        "start_m": [x - vx * t, level + distance, z - vz * t],
        "velocity_m_s": [vx, -down, vz],
        "camera": camera,
    }


def measure(binary, pages, base, index, seed):
    route = route_for(base, index, seed)
    support = route["expected_support"]
    with tempfile.TemporaryDirectory(prefix="osage-approach-") as tmp:
        here = Path(tmp)
        replay = here / "replay.json"
        receipt = here / "receipt.json"
        replay.write_text(
            json.dumps({**base, "routes": [route]}, separators=(",", ":"))
        )
        cmd = [
            binary,
            "--paging",
            "--pages",
            pages,
            "--out",
            receipt,
            "--route",
            route["id"],
            "--replay",
            replay,
        ]

        def run(*flags):
            subprocess.run(
                [*cmd, *map(str, flags)], check=True, capture_output=True, text=True
            )
            return json.loads(receipt.read_bytes())

        resident = run("--all-resident")
        control = resident["first_contact"]
        if (
            not control
            or control["support"] != support
            or abs(control["point"][1] - HEIGHTS[support]) > 0.01
        ):
            return {
                "index": index,
                "support": support,
                "failures": ["resident surface or height"],
                "control": control,
            }
        tick, cell = control["tick"], control["tile"]
        profile = ("healthy", "late-physics", "cut", "late-visual")[index % 4]
        flags = [
            "--delay-ms",
            80,
            "--jitter-ms",
            20,
            "--seed",
            seed + index,
            "--mbps-per-kind",
            20,
        ]
        if profile.startswith("late"):
            flags.extend(
                [
                    "--watch",
                    ",".join(map(str, cell)),
                    "--hold",
                    profile[5:],
                    "--hold-ticks",
                    tick + 30,
                ]
            )
        elif profile == "cut":
            # A two-second disconnection straddles demand for the landing page, even
            # for the low road where the capsule begins inside the request radius.
            start = max(0, tick - 100)
            flags.extend(["--outage", f"{start},{start + 120}"])
        cold = run(*flags)
    hit = cold["first_contact"]
    guard = cold["support"]
    failures = []
    valid_owner = hit and (hit["support"], hit["source_node"]) == (
        support,
        control["source_node"],
    )
    if not valid_owner:
        failures.append("missing or wrong physical owner")
    errors = {}
    if hit:
        errors = {
            "tick_error": hit["tick"] - tick,
            "point_error_m": math.dist(hit["point"], control["point"]),
            "downward_excursion_m": max(
                0,
                HEIGHTS[support]
                - guard.get(
                    "minimum_foot_y_m", guard["minimum_foot_y_after_contact_m"]
                ),
            ),
        }
        if abs(errors["tick_error"]) > 1 or errors["point_error_m"] > 0.01:
            failures.append("contact differs from resident control")
        if errors["downward_excursion_m"] > 0.04:
            failures.append("fell below support")
        if not cold["at_contact"]["visual_has_same_source"]:
            failures.append("visible surface has a different owner")
    if hit and profile == "healthy" and hit["geometry"] != "full":
        failures.append("healthy link missed full support")
    if hit and profile == "late-physics" and hit["geometry"] != "certified_support":
        failures.append("withheld support did not exercise fallback")
    if guard["duplicate_layer_contact_ticks"]:
        failures.append("full and fallback both generated contacts on the same piece")
    ready = [
        e["tick"]
        for e in cold["physics_events"]
        if e["state"] == "ready" and e["tile"] == cell
    ]
    transferred = [e for e in guard["events"] if e["state"] == "full_handoff"]
    ready_keys = {
        (tuple(e["tile"]), e["tick"])
        for e in cold["physics_events"]
        if e["state"] == "ready"
    }
    if (
        hit
        and hit["geometry"] == "certified_support"
        and ready
        and not any(
            e["tile"] == hit["tile"] and e["tick"] >= hit["tick"] for e in transferred
        )
    ):
        failures.append("contacted fallback never handed off to a verified page")
    if any((tuple(e["tile"]), e["tick"]) not in ready_keys for e in transferred):
        failures.append("handoff differs from verified full publication tick")
    if profile == "cut":
        for channel in ("physics", "visual"):
            if any(
                e["state"] == "ready" and start <= e["tick"] < start + 120
                for e in cold[channel + "_events"]
            ):
                failures.append("content published during disconnection")
    if max(cold[k]["peak_in_flight"] for k in ("physics", "visual")) > 4:
        failures.append("wire slots exceeded")
    return {
        "index": index,
        "support": support,
        "profile": profile,
        "seed": seed + index,
        "replay_sha256": cold["replay_sha256"],
        "resident_tick": tick,
        "resident_owner": control["source_node"],
        "cell": cell,
        "geometry": hit and hit["geometry"],
        "handoffs": len(transferred),
        "layer_overlap_ticks": guard["duplicate_layer_contact_ticks"],
        "full_slack_ticks": tick - ready[0] if ready else None,
        "cold_bytes": cold["storage"]["cold_file_bytes"],
        "peak_physics_bytes": cold["physics"]["peak_page_bytes"],
        "peak_visual_bytes": cold["visual"]["peak_page_bytes"],
        **errors,
        "failures": failures,
    }


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    p.add_argument("--pages", type=Path, required=True)
    p.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/approaches"
    )
    p.add_argument("--seed", type=int, default=231123)
    p.add_argument("--count", type=int, default=1000)
    p.add_argument("--workers", type=int, default=3)
    args = p.parse_args()
    if min(args.count, args.workers) < 1 or not 0 <= args.seed <= 2**64 - args.count:
        p.error("invalid sample count, worker count or seed")
    base = json.loads(DEFAULT_REPLAY.read_bytes())
    binary, pages = args.bin.resolve(), args.pages.resolve()
    inputs = {
        "binary": digest(binary),
        "index": digest(pages / "index.json"),
        "base_replay": digest(DEFAULT_REPLAY),
    }
    args.out.mkdir(parents=True, exist_ok=True)
    rows = []
    with (
        (args.out / "cases.ndjson").open("w", buffering=1) as out,
        ThreadPoolExecutor(args.workers) as pool,
    ):
        for r in pool.map(
            lambda i: measure(binary, pages, base, i, args.seed), range(args.count)
        ):
            out.write(json.dumps(r, separators=(",", ":")) + "\n")
            rows.append(r)
    if inputs["binary"] != digest(binary) or inputs["index"] != digest(
        pages / "index.json"
    ):
        raise RuntimeError("inputs changed during qualification")
    profiles = {}
    for profile in ("healthy", "late-physics", "cut", "late-visual"):
        batch = [r for r in rows if r.get("profile") == profile]
        if not batch:
            continue
        slack = sorted(
            r["full_slack_ticks"] for r in batch if r["full_slack_ticks"] is not None
        )
        profiles[profile] = {
            "cases": len(batch),
            "fallback_contact": sum(
                r["geometry"] == "certified_support" for r in batch
            ),
            "slack_p01_ticks": slack[math.ceil(len(slack) / 100) - 1]
            if slack
            else None,
            "slack_p99_ticks": slack[math.ceil(0.99 * len(slack)) - 1]
            if slack
            else None,
            "max_cold_bytes": max(r["cold_bytes"] for r in batch),
            "max_physics_bytes": max(r["peak_physics_bytes"] for r in batch),
            "max_visual_bytes": max(r["peak_visual_bytes"] for r in batch),
        }
    summary = {
        "format": "ville-paris/approaches/1",
        "inputs_sha256": inputs,
        "seed": args.seed,
        "cases": len(rows),
        "failed": sum(bool(r["failures"]) for r in rows),
        "profiles": profiles,
        "max_tick_error": max((abs(r.get("tick_error", 0)) for r in rows), default=0),
        "max_point_error_m": max((r.get("point_error_m", 0) for r in rows), default=0),
        "max_downward_excursion_m": max(
            (r.get("downward_excursion_m", 0) for r in rows), default=0
        ),
        "cases_sha256": digest(args.out / "cases.ndjson"),
    }
    (args.out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary))
    return bool(summary["failed"])


if __name__ == "__main__":
    raise SystemExit(main())
