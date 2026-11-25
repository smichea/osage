#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Compare district LOD decisions, actual reads and main-thread selection work on the same inputs."""

import argparse
import hashlib
import json
import runpy
import statistics
import subprocess
from pathlib import Path

from paging_checks import require

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
field = runpy.run_path(str(HERE / "check-dense-lods.py"))
read, write = (field[x] for x in ("read", "write"))


def digest(value):
    return hashlib.sha256(
        json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def route(name, keys, actor=(10000, 100, 10000), speed=(0, 0, 0)):
    return {
        "id": name,
        "ticks": keys[-1][0],
        "start_m": actor,
        "velocity_m_s": speed,
        "expected_support": "none",
        "camera": [{"tick": t, "eye": eye, "aim": aim} for t, eye, aim in keys],
    }


def cameras(out):
    poses = field["CAMERAS"]
    fixed = lambda pose, end: [(0, *pose), (end, *pose)]
    routes = [
        route("field", [(t, *poses[p]) for t, p in field["ITINERARY"]]),
        route(
            "sweep",
            [
                (0, *poses["distant"]),
                (95, *poses["near"]),
                (173, *poses["side"]),
                (265, *poses["far"]),
                (332, *poses["away"]),
            ],
        ),
        route("away", [(0, *poses["away"]), (180, [29, 216, 85], [45, 300, 128])]),
        route("stationary", fixed(poses["near"], 180)),
        route("actor", fixed(poses["distant"], 140), (-80, 45, 0), (85, -45, 0)),
        route(
            "near-vertical",
            [
                (0, [0, 200, 0], [0.01, 0, 0]),
                (96, [12, 29, 6], [12.01, 0, 6]),
                (150, [3, 100, 1], [3.01, 0, 1]),
            ],
        ),
    ]
    path = out / "replay.json"
    write(
        path,
        {
            "version": 1,
            "tick_hz": 60,
            "gravity_m_s2": 9.81,
            "terminal_m_s": 55,
            "viewport": [320, 180],
            "vertical_fov_deg": 60,
            "routes": routes,
        },
    )
    return path, {r["id"]: r for r in routes}


def evaluate(exe, pages, replay, case, flags, label, out, lock, stride):
    gpu = out / (label + "-gpu.json")
    receipt = out / (label + "-reader.json")
    cmd = [
        "flock",
        lock,
        exe,
        "--paging",
        "--pages",
        pages,
        "--route",
        case,
        "--replay",
        replay,
        "--gpu-trace",
        gpu,
        "--gpu-stride",
        str(stride),
        "--gpu-owner",
        "2",
        "--out",
        receipt,
        *flags,
    ]
    with (out / (label + ".log")).open("w") as log:
        subprocess.run(
            cmd, check=True, timeout=180, stdout=log, stderr=subprocess.STDOUT
        )
    g, s = read(gpu), read(receipt)
    store = s["storage"]
    lod = store["visual_lods"]
    ticks = g["lod_selection_cpu_ns"]["ticks"]
    # Sum how many owners the old loop projected over all simulation ticks.
    visible = 0
    desired = set()
    events = iter(lod["requests"])
    next_change = next(events, None)
    for tick in range(ticks):
        while next_change is not None and next_change["tick"] == tick:
            owner = next_change["owner"]
            if next_change["level"] is None:
                desired.discard(owner)
            else:
                desired.add(owner)
            next_change = next(events, None)
        visible += len(desired)
    index = read(pages / "index.json")
    # The dense field consists of pure adaptive pages, one draw slot per owner.
    # A page ready at tick n also gets its initial binding on that same tick.
    counts = {
        (t["x"], t["z"]): len(t["visual"].get("lods", [])) for t in index["tiles"]
    }
    earlier_slots = None
    if all(t["visual"].get("adaptive_only") for t in index["tiles"]):
        earlier_slots = sum(
            counts[tuple(e["tile"])]
            * (ticks - e["tick"])
            * (1 if e["state"] == "ready" else -1)
            for e in s["visual_events"]
            if e["state"] in ("ready", "evicted")
        )
    frames = [
        {
            k: f[k]
            for k in (
                "tick",
                "camera_eye",
                "camera_aim",
                "fine_cells",
                "draw_owners",
                "render_world_camera_meshes",
                "geometry_assets",
                "prepared_assets",
                "drawable_payload_bytes",
                "mesh_uploads",
                "evicted_assets",
            )
        }
        for f in g["frames"]
    ]
    compared = {
        "source": g["source_sha256"],
        "poses": g["replay_sha256"],
        "renderer": [g[k] for k in ("adapter", "driver", "backend")],
        "desired": digest(lod["requests"]),
        "published": digest(lod["events"]),
        "owners": digest(lod["owners"]),
        "visual_pages": s["visual"],
        "resources": digest(store["model_events"]),
        "placement_events": digest(s["visual_events"]),
        "readiness": digest(s["published"]),
        "frames": frames,
    }
    return {
        "compared": compared,
        "selection": g["lod_selection_cpu_ns"],
        "publication": g.get("lod_publication_cpu_ns"),
        "publication_work": g.get("lod_publication_work"),
        "work": g.get("lod_selection_work"),
        "ticks": ticks,
        "requests": len(lod["requests"]),
        "transitions": len(lod["events"]),
        "previous_owner_visits": ticks * len(lod["owners"]),
        "previous_projections": visible,
        "previous_draw_slots": earlier_slots,
        "earliest_request": next(
            (e["tick"] for e in lod["requests"] if e["level"]), None
        ),
        "earliest_release": next(
            (e["tick"] for e in lod["requests"] if not e["level"]), None
        ),
        "earliest_published": next(
            (e["tick"] for e in lod["events"] if e["level"]), None
        ),
    }


def compare_images(executables, pages, replay, out, lock, demand, controls):
    comparisons = {}
    for label, flags in (("ready", ["--all-resident"]), ("pending", demand)):
        hashes = {}
        for name, exe in executables.items():
            prefix = f"mixed-{label}-{name}"
            field["render"](
                exe, pages, replay, prefix, out, lock, 216, True, flags + controls[name]
            )
            hashes[name] = hashlib.sha256(
                field["pixels"](out / (prefix + ".png"))
            ).hexdigest()
        require(
            len(set(hashes.values())) == 1,
            f"mixed-view native pixels differ in {label} mode",
        )
        comparisons[label] = {"tick": 216, "rgb_sha256": hashes["current"]}
    return comparisons


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    cli.add_argument(
        "--reference",
        type=Path,
        help="previous executable with timing for the selected phase; publication defaults to the built-in full scan",
    )
    cli.add_argument(
        "--pages",
        type=Path,
        help="prebuilt multi-LOD field; default generates a public pair of mesh levels",
    )
    cli.add_argument(
        "--out",
        type=Path,
        help="receipt and image directory; defaults to the selected phase",
    )
    cli.add_argument("--trials", type=int, default=5)
    cli.add_argument(
        "--stage",
        choices=("selection", "publication"),
        default="selection",
        help="main-thread LOD phase to time",
    )
    cli.add_argument(
        "--gpu-lock", type=Path, default=Path("/tmp/osage-ville-paris-gpu.lock")
    )
    args = cli.parse_args()
    require(args.trials >= 1, "positive trial count required")
    require(
        args.reference or args.stage == "publication",
        "selection requires the previous executable",
    )
    phase_dir = "lod-publication" if args.stage == "publication" else "lod-evaluation"
    out = (args.out or ROOT / "bench/out/ville-paris" / phase_dir).resolve()
    out.mkdir(parents=True, exist_ok=True)
    pages = args.pages or out / "pages"
    if not args.pages:
        shape = field["example"](out)
        scene = out / "field.glb"
        field["build"](shape, scene, 20, 24, "adaptive")
        field["cook"](scene, pages, 32)
    replay, _ = cameras(out)
    full = ["--all-resident"]
    demand = ["--delay-ms", "80", "--visual-radius", "500", "--mbps-per-kind", "8"]
    cases = {
        "field": demand,
        "sweep": full,
        "away": demand,
        "stationary": demand,
        "actor": ["--delay-ms", "0", "--visual-radius", "12"],
        "near-vertical": full,
    }
    executables = {"previous": args.reference or args.bin, "current": args.bin}
    controls = {
        "previous": [] if args.reference else ["--lod-publication", "scan"],
        "current": [],
    }
    result = {
        "binary_sha256": {k: field["digest"](p) for k, p in executables.items()},
        "cases": {},
        "stage": args.stage,
        "reference": "earlier binary"
        if args.reference
        else "same binary, complete scan",
    }
    mode_order = list(executables)
    for case, options in cases.items():
        programs = {}
        for trial in range(args.trials):
            ordered = mode_order if trial % 2 == 0 else mode_order[::-1]
            for name in ordered:
                run = evaluate(
                    executables[name],
                    pages,
                    replay,
                    case,
                    options + controls[name],
                    f"{case}-{name}-{trial}",
                    out,
                    args.gpu_lock,
                    1000,
                )
                programs.setdefault(name, []).append(run)
                if trial:
                    require(
                        programs[name][0]["compared"] == run["compared"],
                        "repeated simulation changed data",
                    )
            require(
                programs["previous"][-1]["compared"]
                == programs["current"][-1]["compared"],
                f"{case} changed an owner, a read, a publication or extracted camera geometry",
            )
        new = programs["current"][0]
        w = new["work"]
        require(w is not None, "current district reader lacks work counters")
        if case == "sweep":
            require(
                not w["stationary_tiles"]
                and w["inside_tiles"]
                and w["boundary_tiles"]
                and w["outside_tiles"],
                "moving camera failed to exercise all tile tests",
            )
        if case == "actor":
            require(
                new["earliest_request"]
                and new["earliest_release"] > new["earliest_request"]
                and new["earliest_published"] is not None,
                "actor never entered and left the fixed camera's range",
            )
        if case == "away":
            require(
                w["projected_owners"] == 0
                and w["owner_bounds"] == 0
                and not new["compared"]["visual_pages"]["reads"],
                "back view tested or read offscreen geometry",
            )
        if case == "stationary":
            require(
                new["earliest_request"] == 0
                and new["earliest_published"] > 0
                and w["stationary_tiles"] > 0
                and w["resource_rebuilds"] > 0,
                "stationary view failed to publish delayed dependencies",
            )
        if args.stage == "publication":
            publication = new["publication_work"]
            require(publication is not None, "missing publication counters")
            require(
                publication["resolved_owners"] <= new["previous_owner_visits"],
                "publication repeated owner resolution in one tick",
            )
            if new["previous_draw_slots"] is not None:
                require(
                    publication["fine_slots"] + publication["new_page_slots"]
                    <= new["previous_draw_slots"],
                    "publication revisited more resident draw slots than a full scan",
                )
            if not args.reference:
                scanned = programs["previous"][0]["publication_work"]
                require(
                    scanned["resolved_owners"] == new["previous_owner_visits"],
                    "complete scan did not revisit every owner",
                )
                if new["previous_draw_slots"] is not None:
                    require(
                        scanned["fine_slots"] + scanned["new_page_slots"]
                        == new["previous_draw_slots"],
                        "complete scan differs from the placement read/eviction event accounting",
                    )
            if case == "away":
                require(
                    publication["resolved_owners"]
                    == publication["fine_slots"]
                    == publication["new_page_slots"]
                    == 0,
                    "an offscreen owner was prepared for publication",
                )
            if case == "stationary":
                require(
                    0 < publication["resolved_owners"] < new["previous_owner_visits"]
                    and publication["new_page_slots"] > 0,
                    "static late publication control did not install the ready owners",
                )
        detail = {
            k: v
            for k, v in new.items()
            if k not in ("selection", "publication", "compared")
        }
        detail["reads"] = new["compared"]["visual_pages"]
        detail["verified"] = new["compared"]
        for name, runs in programs.items():
            us = [r[args.stage]["total"] / r["ticks"] / 1000 for r in runs]
            detail[name] = {
                "mean_us_per_tick_by_trial": us,
                "median_us_per_tick": statistics.median(us),
                "max_tick_us_by_trial": [r[args.stage]["max"] / 1000 for r in runs],
            }
        result["cases"][case] = detail
        write(out / "summary.json", result)
        print(
            case,
            round(detail["previous"]["median_us_per_tick"], 2),
            round(detail["current"]["median_us_per_tick"], 2),
            new["publication_work"] if args.stage == "publication" else w,
            flush=True,
        )
    dense = {
        k: evaluate(
            exe,
            pages,
            replay,
            "field",
            demand + controls[k],
            f"field-{k}-camera",
            out,
            args.gpu_lock,
            4,
        )
        for k, exe in executables.items()
    }
    require(
        dense["previous"]["compared"] == dense["current"]["compared"],
        "sampled native camera differs",
    )
    result["native_camera_frames"] = len(dense["current"]["compared"]["frames"])
    result["mixed_native_images"] = compare_images(
        executables, pages, replay, out, args.gpu_lock, demand, controls
    )
    result["complete"] = True
    write(out / "summary.json", result)


if __name__ == "__main__":
    main()
