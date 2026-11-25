#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Replay the district's actor-only collision-request planner against a fixed radius."""

import argparse
import hashlib
import json
import runpy
import statistics
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def row(run):
    hit = run["first_contact"]
    cell = hit["tile"]
    events = [v for v in run["physics_events"] if v["tile"] == cell]

    def first(kind):
        return next((e["tick"] for e in events if e["state"] == kind), None)

    start, ready = first("started"), first("ready")
    stats = run["physics"]
    return {
        "contact": hit["tick"],
        "surface": hit["support"],
        "owner": hit["source_node"],
        "tile": cell,
        "geometry": hit["geometry"],
        "start": start,
        "ready": ready,
        "lead_ticks": hit["tick"] - ready if ready is not None else None,
        "service_ticks": ready - start if ready is not None else None,
        "landing_forecast": next(
            (e for e in run["physics_forecasts"] if e["tile"] == cell), None
        ),
        "forecast_requests": len(run["physics_forecasts"]),
        "started_pages": sum(e["state"] == "started" for e in run["physics_events"]),
        "ready_pages": stats["reads"],
        "read_bytes": stats["read_bytes"],
        "peak_in_flight": stats["peak_in_flight"],
        "peak_resident_pages": stats["peak_pages"],
        "peak_resident_bytes": stats["peak_page_bytes"],
        "held_ticks": run["support"]["held_ticks"]
        if "held_ticks" in run["support"]
        else run["support"]["stats"]["held_ticks"],
    }


def main():
    arg = argparse.ArgumentParser(description=__doc__)
    arg.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    arg.add_argument("--pages", type=Path, help="pre-cooked real Bir-Hakeim district")
    arg.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/physics-planning"
    )
    arg.add_argument(
        "--baseline",
        type=Path,
        help="frozen pre-planner directory with the twelve reference runs",
    )
    arg.add_argument(
        "--http",
        action="store_true",
        help="also measure real prompt and delayed loopback reads",
    )
    arg.add_argument(
        "--cpu",
        type=int,
        help="also time 15 alternating healthy-route pairs pinned to this Linux CPU",
    )
    a = arg.parse_args()
    out = a.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    pages = (a.pages or out / "pages").resolve()
    if not (pages / "index.json").exists():
        subprocess.run(
            [
                sys.executable,
                HERE / "cook-pages.py",
                HERE / "fixtures/bir-hakeim.glb",
                pages,
            ],
            check=True,
        )
    runs = out / "runs"
    runs.mkdir(exist_ok=True)
    fixture = json.loads((HERE / "fixtures/descent.json").read_bytes())
    routes = {r["id"]: r for r in fixture["routes"]}
    rail = routes["rail"]
    roof = routes["roof"]
    remote = {
        **rail,
        "id": "rail-remote",
        "camera": [
            {"tick": k, "eye": [920, 10, 895], "aim": [950, 5, 880]}
            for k in (0, rail["ticks"])
        ],
    }
    reversal = {
        **roof,
        "id": "roof-turn",
        "ticks": 560,
        "start_m": [188, 100, -72],
        "velocity_m_s": [0, 25, 0],
        "camera": [
            {"tick": k, "eye": [210, 140, -40], "aim": [188, 70, -72]} for k in (0, 560)
        ],
    }
    replay = out / "routes.json"
    replay.write_text(
        json.dumps({**fixture, "routes": [*fixture["routes"], remote, reversal]})
    )

    def run(name, route, delay, mode=None, extra=(), location=None):
        path = runs / (name + ".json")
        args = [
            a.bin,
            "--paging",
            "--pages",
            location or pages,
            "--out",
            path,
            "--replay",
            replay,
            "--route",
            route,
            "--trace-body",
        ]
        if delay is not None:
            args += ["--delay-ms", str(delay)]
        if mode == "radius":
            args += ["--physics-radius", "64"]
        elif mode is not None:
            args += ["--physics-plan", mode]
        subprocess.run([*args, *extra], check=True, capture_output=True)
        state = json.loads(path.read_bytes())
        assert state["settings"]["physics_plan"] == (mode or "trajectory")
        return state

    comparisons = []
    for route in routes:
        for delay in (200, 600, 1400, 2000):
            modes = {}
            for mode in ("radius", "trajectory"):
                full = run(
                    f"{route}-{delay}-{mode}",
                    route,
                    delay,
                    "radius" if mode == "radius" else None,
                )
                result = row(full)
                assert result["peak_in_flight"] <= 4 and result["held_ticks"] == 0
                if a.baseline and mode == "radius":
                    prior = json.loads(
                        (a.baseline / f"{route}-{delay}.json").read_bytes()
                    )
                    for key in (
                        "first_contact",
                        "physics_events",
                        "physics",
                        "final_foot_m",
                    ):
                        assert prior[key] == full[key], (route, delay, key)
                modes[mode] = result
            fixed, planned = modes["radius"], modes["trajectory"]
            assert (fixed["contact"], fixed["owner"], fixed["surface"]) == (
                planned["contact"],
                planned["owner"],
                planned["surface"],
            )
            assert planned["start"] <= fixed["start"]
            if route in ("rail", "roof") or delay <= 600:
                assert planned["geometry"] == "full" and planned["lead_ticks"] >= 0, (
                    route,
                    delay,
                    planned,
                )
            else:
                assert (
                    planned["start"] == 0 and planned["geometry"] == "certified_support"
                )
            comparisons.append({"route": route, "delay_ms": delay, **modes})
    rail_default = json.loads((runs / "rail-1400-trajectory.json").read_bytes())
    other = run("rail-remote", "rail-remote", 1400)
    assert all(
        rail_default[k] == other[k]
        for k in ("first_contact", "physics_events", "physics_forecasts", "physics")
    )
    assert other["visual_events"] != rail_default["visual_events"]
    modes = {
        m: run("roof-turn-" + m, "roof-turn", 2000, m) for m in ("radius", "trajectory")
    }
    up = modes["trajectory"]
    body = up["body_before_step"]
    apex = max(body, key=lambda f: f["center_m"][1])
    yy = lambda t: body[t]["center_m"][1]
    t = apex["tick"]
    assert yy(t - 3) < yy(t) > yy(t + 3) and yy(t) > yy(0) + 30
    assert (
        up["first_contact"]["tick"] == modes["radius"]["first_contact"]["tick"]
        and up["first_contact"]["geometry"] == "full"
    )
    explicit = run(
        "rail-explicit-plan", "rail", 1400, "trajectory", ("--physics-radius", "64")
    )
    assert explicit["physics_events"] == rail_default["physics_events"]
    def limited_bandwidth(label, mbps):
        return {
            mode: row(run(f"rail-{label}-{mode}", "rail", 200, mode, ("--mbps-per-kind", str(mbps))))
            for mode in ("trajectory", "radius")
        }

    slow_wire = limited_bandwidth("small-bandwidth", 0.025)
    assert slow_wire["trajectory"]["lead_ticks"] > 0
    # Keep the fixed-rate comparison when storage becomes smaller. Independently,
    # impose 1.28 seconds of transfer on the landing page so a radius-only request misses.
    directory = json.loads((pages / "index.json").read_bytes())
    landing = next(t["physics"] for t in directory["tiles"]
                   if [t["x"], t["z"]] == rail_default["first_contact"]["tile"])
    service_rate = landing["bytes"] * 8 / (1.28 * 1_000_000)
    slow_service = limited_bandwidth("fixed-service", service_rate)
    assert (
        slow_service["trajectory"]["lead_ticks"] > 0
        and slow_service["radius"]["lead_ticks"] < 0
    )
    http = {}
    if a.http:
        origin = runpy.run_path(str(HERE / "check-http.py"))["origin"]
        idx = json.loads((pages / "index.json").read_bytes())
        physical = {t["physics"]["file"]: 0.3 for t in idx["tiles"] if "physics" in t}
        for label, pauses in (("prompt", {}), ("delayed", physical)):
            with origin(pages, delay=pauses) as (url, records):
                r = run("http-road-" + label, "road", None, location=url)
            stats = row(r)
            assert (
                stats["geometry"] == "full"
                and r["transport"]["additional_link_profile"] is False
            )
            sample = next(
                s for s in r["physics_service_samples"] if s["tile"] == stats["tile"]
            )
            assert sample["elapsed_ticks"] == stats["service_ticks"]
            http[label] = {
                "landing": stats,
                "sample": sample,
                "physics_reads": sum(x["name"] in physical for x in records),
            }
        assert (
            http["delayed"]["sample"]["elapsed_ticks"]
            >= http["prompt"]["sample"]["elapsed_ticks"] + 8
        )
    cpu = None
    if a.cpu is not None:
        import resource

        rail_bytes = {
            mode: json.loads((runs / f"rail-200-{mode}.json").read_bytes())["physics"]["read_bytes"]
            for mode in ("radius", "trajectory")
        }

        samples = {"radius": [], "trajectory": []}
        for pair in range(16):
            for mode in (
                ("radius", "trajectory") if pair % 2 == 0 else ("trajectory", "radius")
            ):
                dest = runs / ("cpu-" + mode + ".json")
                cmd = [
                    "taskset",
                    "-c",
                    str(a.cpu),
                    a.bin,
                    "--paging",
                    "--pages",
                    pages,
                    "--route",
                    "rail",
                    "--delay-ms",
                    "200",
                    "--physics-plan",
                    mode,
                    "--out",
                    dest,
                ]

                def elapsed():
                    usage = resource.getrusage(resource.RUSAGE_CHILDREN)
                    return usage.ru_utime + usage.ru_stime

                before = elapsed()
                subprocess.run(cmd, check=True, capture_output=True)
                duration = elapsed() - before
                r = json.loads(dest.read_bytes())
                assert (
                    r["physics"]["read_bytes"] == rail_bytes[mode]
                    and r["first_contact"]["tick"] == 807
                )
                if pair:
                    samples[mode].append(duration * 1000)
        cpu = {
            "cpu": a.cpu,
            "ticks": 841,
            "samples_process_cpu_ms": samples,
            "medians_process_cpu_ms": {
                k: statistics.median(v) for k, v in samples.items()
            },
        }
    # An explicit radius keeps the historic admission order and deadlines across all twelve controls.
    with a.bin.open("rb") as binary:
        binary_hash = hashlib.file_digest(binary, "sha256").hexdigest()
    report = {
        "binary_sha256": binary_hash,
        "source_sha256": rail_default["source_sha256"],
        "clock_hz": fixture["tick_hz"],
        "matched_frozen_radius": bool(a.baseline),
        "pairs": comparisons,
        "bandwidth_25_kbit_s": slow_wire,
        "landing_payload_1280_ms": {"mbps": service_rate, **slow_service},
        "http": http,
        "healthy_route_cpu": cpu,
        "remote_camera": {
            "physical_sequence_equal": True,
            "visual_sequence_different": True,
            "visual_reads_actor_camera": rail_default["visual"]["reads"],
            "visual_reads_remote_camera": other["visual"]["reads"],
        },
        "roof_reversal": {
            "apex_tick": t,
            "apex_center_y_m": yy(t),
            "initial_center_y_m": yy(0),
            "radius": row(modes["radius"]),
            "trajectory": row(up),
        },
        "explicit_radius_with_trajectory_matches_default": True,
    }
    (out / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    print("route delay_ms fixed_lead predicted_lead fixed_bytes predicted_bytes")
    for v in comparisons:
        print(
            v["route"],
            v["delay_ms"],
            v["radius"]["lead_ticks"],
            v["trajectory"]["lead_ticks"],
            v["radius"]["read_bytes"],
            v["trajectory"]["read_bytes"],
        )


if __name__ == "__main__":
    main()
