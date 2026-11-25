#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Qualify an independently paged district on the three existing fixed-tick routes."""

import argparse
import csv
import hashlib
import itertools
import json
import math
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from paging_checks import audit, qualify_guard, qualify_tree

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
ROUTES = ("rail", "road", "roof")


def require(condition, reason):
    if not condition:
        raise RuntimeError(reason)


def event_ticks(result, channel, cell, state):
    return [
        e["tick"]
        for e in result[channel + "_events"]
        if e["tile"] == cell and e["state"] == state
    ]


def layer_at(result, channel, cell, tick):
    state = False
    for e in result[channel + "_events"]:
        if (
            e["tile"] == cell
            and e["tick"] <= tick
            and e["state"] in ("ready", "evicted")
        ):
            state = e["state"] == "ready"
    return state


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    p.add_argument("--out", type=Path, default=ROOT / "bench/out/ville-paris/paging")
    p.add_argument("--delay-ms", type=int, nargs="+", default=[200, 600])
    p.add_argument("--mbps", type=float, default=20)
    p.add_argument("--physics-radii", type=float, nargs="+", default=[8, 32, 64, 120])
    p.add_argument("--visual-radii", type=float, nargs="+", default=[12, 48, 96, 200])
    p.add_argument("--margin-ticks", type=int, default=6)
    p.add_argument(
        "--visual-storage", choices=("demand", "shared", "embedded"), default="demand"
    )
    p.add_argument(
        "--frames",
        action="store_true",
        help="render a matched fine/held frame using the normal Bevy renderer",
    )
    args = p.parse_args()

    def executable_hash():
        with args.bin.open("rb") as executable:
            return hashlib.file_digest(executable, "sha256").hexdigest()

    binary_sha = executable_hash()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    pages = out / "pages"
    root = HERE / "fixtures"
    src = root / "bir-hakeim.glb"
    replay = json.loads((root / "descent.json").read_text())
    subprocess.run(
        [
            sys.executable,
            str(HERE / "cook-pages.py"),
            str(src),
            str(pages),
            "--visual-storage",
            args.visual_storage,
        ],
        check=True,
    )
    idx = json.loads((pages / "index.json").read_text())
    require(
        idx["source_sha256"] == hashlib.sha256(src.read_bytes()).hexdigest(),
        "source mismatch",
    )

    def run(
        id, route, phys=0, visual=0, delay=0, watch=None, extra=(), cli_defaults=False
    ):
        dest = out / "runs" / (id + ".json")
        cmd = [
            str(args.bin.resolve()),
            "--paging",
            "--pages",
            str(pages),
            "--out",
            str(dest),
            "--route",
            route,
        ]
        if not cli_defaults:
            cmd += [
                "--physics-radius",
                str(phys),
                "--visual-radius",
                str(visual),
                "--delay-ms",
                str(delay),
                "--mbps-per-kind",
                str(args.mbps),
            ]
        if watch is not None:
            cmd += ["--watch", ",".join(map(str, watch))]
        subprocess.run(cmd + list(map(str, extra)), check=True)
        return json.loads(dest.read_text())

    references = {}
    native_dir = out / "native-reference"
    native_dir.mkdir(exist_ok=True)
    subprocess.run(
        [
            str(args.bin.resolve()),
            "--physics",
            "rapier",
            "--actor",
            "capsule",
            "--bench",
            str(native_dir),
        ],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    mono = json.loads((native_dir / "dynamics.json").read_text())["run"]
    expected = {r["id"]: r["first_contacts"][0] for r in mono["outcomes"]}
    native_manifolds = {}
    with (native_dir / "dynamics.csv").open() as trace:
        previous_vy = {}
        for row in csv.DictReader(trace):
            route = row["route"]
            if route in expected and int(row["tick"]) == expected[route]["tick"]:
                candidates = [
                    c
                    for c in json.loads(row["contacts_json"])
                    if mono["surfaces"][c["surface"]] == expected[route]["surface"]
                    and c["normal"][1] > 0.5
                ]
                native_manifolds[route] = {
                    **min(candidates, key=lambda c: c["separation"]),
                    "vertical_speed_before_m_s": previous_vy[route],
                    "vertical_speed_after_m_s": float(row["vy"]),
                }
            previous_vy[route] = float(row["vy"])
    native_distances = {}
    for route in ROUTES:
        full = run("reference-" + route, route, extra=["--all-resident"])
        hit = full["first_contact"]
        require(
            hit
            and hit["support"]
            == full["expected_support"]
            == expected[route]["surface"],
            "reference landing on " + route,
        )
        require(
            abs(hit["tick"] - expected[route]["tick"]) <= 1,
            "tiled and monolithic reference contact differs: " + route,
        )
        native_distances[route] = math.dist(
            hit["point"], native_manifolds[route]["point"]
        )
        require(
            abs(hit["separation_m"]) < 0.05
            and abs(hit["vertical_speed_after_m_s"]) < 1
            and hit["vertical_speed_before_m_s"] < -5,
            "first paged manifold does not resolve the descent: " + route,
        )
        references[route] = hit
    rows = []
    passing = []
    for phys, visual in itertools.product(
        sorted(set(args.physics_radii)), sorted(set(args.visual_radii))
    ):
        eligible = True
        for delay, route in itertools.product(sorted(set(args.delay_ms)), ROUTES):
            ref = references[route]
            cell = ref["tile"]
            ident = f"{route}-d{delay}-p{phys:g}-v{visual:g}"
            r = run(ident, route, phys, visual, delay, cell)
            hit = r["first_contact"]
            reference_tick = ref["tick"]
            ok = bool(
                hit
                and hit["support"] == ref["support"]
                and hit["tick"] == reference_tick
                and math.dist(hit["point"], ref["point"]) <= 0.01
            )
            deadline = max(0, reference_tick - args.margin_ticks)
            ready = {
                ch: layer_at(r, ch, cell, deadline) for ch in ("physics", "visual")
            }
            passed = ok and all(ready.values())
            eligible &= passed
            metric = {
                "run": ident,
                "route": route,
                "delay_ms": delay,
                "physics_radius_m": phys,
                "visual_radius_m": visual,
                "correct_contact": ok,
                "qualified": passed,
                "deadline_tick": deadline,
            }
            for ch, on_time in ready.items():
                requested = event_ticks(r, ch, cell, "requested")
                loaded = event_ticks(r, ch, cell, "ready")
                model_io = r["storage"]["model_io"] if ch == "visual" else {}
                metric[ch] = {
                    "model_peak": model_io.get("peak_models", 0),
                    "model_bytes_read": model_io.get("read_bytes", 0),
                    "requested_tick": requested[0] if requested else None,
                    "ready_tick": loaded[0] if loaded else None,
                    "ready_by_deadline": on_time,
                    "peak_pages": r[ch]["peak_pages"],
                    "bytes_read": r[ch]["read_bytes"],
                    "evictions": r[ch]["evictions"],
                }
            rows.append(metric)
        if eligible:
            passing.append((phys, visual))
    (out / "sweep.json").write_text(json.dumps(rows, indent=2) + "\n")
    require(
        passing,
        "no radius pair meets contact and fine-mesh deadlines; inspect sweep.json",
    )

    # Prefer the smallest worst-case resident footprint, then the smaller request distances.
    def cost(pair):
        subset = [
            x for x in rows if (x["physics_radius_m"], x["visual_radius_m"]) == pair
        ]
        return (
            max(x["physics"]["peak_pages"] + x["visual"]["peak_pages"] for x in subset),
            sum(
                x["physics"]["bytes_read"]
                + x["visual"]["bytes_read"]
                + x["visual"]["model_bytes_read"]
                for x in subset
            ),
            sum(pair),
            pair,
        )

    phys, visual = min(passing, key=cost)
    slow = max(args.delay_ms)
    default_run = run("default-rail", "rail", cli_defaults=True)
    default_cfg = default_run["settings"]
    require(
        default_run["first_contact"] == references["rail"],
        "default CLI lost the rail contact",
    )
    if (
        set(args.physics_radii) == {8, 32, 64, 120}
        and set(args.visual_radii) == {12, 48, 96, 200}
        and set(args.delay_ms) == {200, 600}
        and args.mbps == 20
        and args.margin_ticks == 6
    ):
        require(
            (default_cfg["physics_radius_m"], default_cfg["visual_radius_m"])
            == (phys, visual),
            "default CLI radii no longer match the qualified envelope",
        )
    controls = {}
    for ch in ("visual", "physics"):
        ref = references["rail"]
        cell = ref["tile"]
        hold = ref["tick"] + 24
        r = run(
            "held-" + ch,
            "rail",
            phys,
            visual,
            slow,
            cell,
            ["--hold", ch, "--hold-ticks", hold, "--late-collision", "unprotected"],
        )
        other = "physics" if ch == "visual" else "visual"
        require(
            not layer_at(r, ch, cell, ref["tick"])
            and layer_at(r, other, cell, ref["tick"]),
            "hold must isolate " + ch,
        )
        hit = r["first_contact"]
        correct = bool(hit and hit["support"] == "rail" and hit["tick"] == ref["tick"])
        require(
            correct == (ch == "visual"), "negative control does not discriminate " + ch
        )
        controls[ch] = {
            "until_tick": hold,
            "reference_tick": ref["tick"],
            "first_contact": hit,
            "other_channel_ready": True,
            "held_channel_ready_at_reference_contact": False,
            "final_foot_m": r["final_foot_m"],
        }
    # A level traverse exercises installation/removal away from the three vertical landings.
    # The camera returns to its starting neighbourhood as the actor continues east.
    traverse = {
        **replay,
        "gravity_m_s2": 0.01,
        "routes": [
            {
                "id": "traverse",
                "ticks": 900,
                "start_m": [-500, 80, 0],
                "velocity_m_s": [70, 0, 0],
                "expected_support": "rail",
                "camera": [
                    {"tick": t, "eye": [x, 32, 45], "aim": [x + 24, 10, 0]}
                    for t, x in [(0, -500), (450, 500), (900, -500)]
                ],
            }
        ],
    }
    traverse_path = out / "traverse.json"
    traverse_path.write_text(json.dumps(traverse, indent=2) + "\n")
    tour = run("traverse", "traverse", 120, 96, slow, extra=["--replay", traverse_path])
    repeated_visual = sorted(
        {
            (e["tile"][0], e["tile"][1])
            for e in tour["visual_events"]
            if e["state"] == "evicted"
            and any(
                a > e["tick"] for a in event_ticks(tour, "visual", e["tile"], "ready")
            )
        }
    )
    require(
        all(tour[ch]["evictions"] for ch in ("physics", "visual")) and repeated_visual,
        "traverse must evict both channels and reread visual meshes on return",
    )
    residency = {
        "replay": traverse_path.name,
        "receipt": "runs/traverse.json",
        "physics": tour["physics"],
        "visual": tour["visual"],
        "visual_tiles_read_again_after_eviction": repeated_visual,
    }
    # A directory containing just the actually requested blobs must reproduce the road shot.
    # Corrupt one real support page in that isolated directory and demand it again.
    road = references["road"]
    deadline = road["tick"]
    trial = run(
        "road-working-set",
        "road",
        phys,
        visual,
        slow,
        road["tile"],
        ["--tick", deadline],
    )
    directories = {(t["x"], t["z"]): t for t in idx["tiles"]}
    files = {idx["parent"]["file"], idx["support"]["file"]}
    if "models" in idx:
        files.add(idx["models"]["file"])
    for e in trial["storage"].get("model_events", []):
        if e["state"] == "ready":
            files.add(idx["model_resources"][e["model"]]["file"])
    for ch in ("physics", "visual"):
        files.update(
            directories[tuple(e["tile"])][ch]["file"]
            for e in trial[ch + "_events"]
            if e["state"] in ("decoded", "ready")
        )
    with tempfile.TemporaryDirectory() as tmp:
        working = Path(tmp)
        shutil.copyfile(pages / "index.json", working / "index.json")
        for name in files:
            shutil.copyfile(pages / name, working / name)
        only = run(
            "road-working-set-only",
            "road",
            phys,
            visual,
            slow,
            road["tile"],
            ["--pages", working, "--tick", deadline],
        )
        require(
            only["first_contact"] == trial["first_contact"]
            and only["published"] == trial["published"],
            "withholding all off-route files changed the landing",
        )
        required = directories[tuple(road["tile"])]["physics"]["file"]
        broken = bytearray((working / required).read_bytes())
        broken[0] ^= 1
        (working / required).write_bytes(broken)
        bad = subprocess.run(
            [
                str(args.bin.resolve()),
                "--paging",
                "--pages",
                str(working),
                "--out",
                str(working / "bad.json"),
                "--route",
                "road",
                "--physics-radius",
                str(phys),
                "--visual-radius",
                str(visual),
                "--delay-ms",
                str(slow),
                "--mbps-per-kind",
                str(args.mbps),
                "--tick",
                str(deadline),
                "--late-collision",
                "unprotected",
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        require(
            bad.returncode
            and "bad tile content " + required in bad.stderr
            and not (working / "bad.json").exists(),
            "corrupted requested collider was accepted",
        )
        repaired_by_proxy = run(
            "road-corrupt-with-proxy",
            "road",
            phys,
            visual,
            slow,
            road["tile"],
            ["--pages", working, "--tick", deadline],
        )
        survivor = repaired_by_proxy["first_contact"]
        require(
            survivor
            and survivor["geometry"] == "certified_support"
            and survivor["source_node"] == road["source_node"]
            and survivor["tick"] == deadline
            and repaired_by_proxy["physics"]["rejected_reads"] >= 1
            and not layer_at(repaired_by_proxy, "physics", road["tile"], deadline),
            "bad full file displaced a verified support",
        )
        proxyfile = idx["support"]["file"]
        broken = bytearray((working / proxyfile).read_bytes())
        broken[0] ^= 1
        (working / proxyfile).write_bytes(broken)
        stopped = subprocess.run(
            [
                str(args.bin.resolve()),
                "--paging",
                "--pages",
                str(working),
                "--out",
                str(working / "unsafe.json"),
                "--route",
                "road",
            ],
            check=False,
            capture_output=True,
            text=True,
        )
        require(
            stopped.returncode
            and "bad tile content " + proxyfile in stopped.stderr
            and not (working / "unsafe.json").exists(),
            "simulation advanced with a corrupt support catalogue",
        )
    storage_control = {
        "road_tick": deadline,
        "data_files_available_in_isolation": len(files),
        "data_files_in_district": len(
            {idx["parent"]["file"], idx["support"]["file"]}
            | {r["file"] for r in idx.get("model_resources", [])}
            | ({idx["models"]["file"]} if "models" in idx else set())
            | {
                t[ch]["file"]
                for t in idx["tiles"]
                for ch in ("physics", "visual")
                if ch in t
            }
        ),
        "identical_contact_and_publication": True,
        "corrupted_required_physics_file_rejected": required,
        "protected_contact_with_corrupt_full_file": survivor,
        "corrupted_proxy_stopped_before_simulation": True,
    }
    safety = qualify_guard(run, replay, out, references, phys, visual, slow)
    ownership = audit(pages, idx)
    tree_check = qualify_tree(run, src, replay, out)
    photos = {}
    if args.frames:
        ref = references["road"]
        cell = ref["tile"]
        tick = ref["tick"]
        shots = []
        for name, extra in [
            ("fine", []),
            ("coarse", ["--hold", "visual", "--hold-ticks", tick + 1]),
            ("fine-repeat", []),
        ]:
            png = out / ("road-" + name + ".png")
            r = run(
                "frame-road-" + name,
                "road",
                phys,
                visual,
                slow,
                cell,
                [*extra, "--tick", tick, "--shot", png],
            )
            require(
                r["published"]["watch_is_fine"] == (name != "coarse"),
                "unexpected mesh in " + name + " frame",
            )
            shots.append(png)
            photos[name] = {
                "file": png.name,
                "sha256": hashlib.sha256(png.read_bytes()).hexdigest(),
                "tick": tick,
                "published": r["published"],
            }

        def pixels(path):
            return subprocess.check_output(
                [
                    "ffmpeg",
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-i",
                    str(path),
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "rgb24",
                    "-threads",
                    "1",
                    "-",
                ]
            )

        a, b, repeat = map(pixels, shots)
        w, h = replay["viewport"]
        require(len(a) == len(b) == len(repeat) == w * h * 3, "frame dimensions differ")

        def differences(other):
            return sum(a[i : i + 3] != other[i : i + 3] for i in range(0, len(a), 3))

        changed = differences(b)
        control = differences(repeat)
        require(
            changed > control, "fine/coarse difference does not exceed same-mesh repeat"
        )
        photos["changed_pixels"] = changed
        photos["repeat_changed_pixels"] = control
        photos["dimensions"] = [w, h]
    best = [
        r
        for r in rows
        if (r["physics_radius_m"], r["visual_radius_m"]) == (phys, visual)
    ]
    totals = {
        "visual_tiles": len(idx["tiles"]),
        "physics_tiles": sum("physics" in t for t in idx["tiles"]),
        "source_instances": idx["source_instances"],
        "source_triangles": idx["source_triangles"],
        "visual_triangles_after_cutting": idx["visual_triangles"],
        "physics_triangles_after_cutting": idx["physics_triangles"],
        "physics_area_audit": idx["physics_area_audit"],
        "parent_visual_bytes": idx["parent"]["bytes"],
        "model_library_bytes": idx.get("models", {}).get("bytes", 0),
        "model_resource_bytes": sum(r["bytes"] for r in idx.get("model_resources", [])),
        "fine_visual_bytes": sum(t["visual"]["bytes"] for t in idx["tiles"]),
        "physics_bytes": sum(
            t["physics"]["bytes"] for t in idx["tiles"] if "physics" in t
        ),
    }
    require(
        executable_hash() == binary_sha,
        "ville-paris binary changed during qualification",
    )
    summary = {
        "format": "ville-paris/paging-qualification/1",
        "binary_sha256": binary_sha,
        "tile_size_m": idx["edge_m"],
        "source_sha256": idx["source_sha256"],
        "replay_sha256": hashlib.sha256(
            (root / "descent.json").read_bytes()
        ).hexdigest(),
        "district": totals,
        "clock_hz": replay["tick_hz"],
        "delay_ms": args.delay_ms,
        "mbps_per_kind": args.mbps,
        "minimum_lead_ticks": args.margin_ticks,
        "tested_runs": len(rows),
        "passing_pairs": len(passing),
        "selected_radii_m": {"physics": phys, "visual": visual},
        "cli_defaults": {
            "settings": default_cfg,
            "rail_contact": default_run["first_contact"],
        },
        "reference": references,
        "monolithic_rapier_first_contact": expected,
        "monolithic_rapier_first_manifold": native_manifolds,
        "monolithic_contact_errors_m": native_distances,
        "monolithic_contact_threshold_m": 0.1,
        "monolithic_contacts_within_threshold": all(
            d < 0.1 for d in native_distances.values()
        ),
        "selected_runs": best,
        "holds": controls,
        "delayed_collision_protection": safety,
        "source_tree_audit": ownership,
        "nested_tree_probe": tree_check,
        "residency_control": residency,
        "independent_files_control": storage_control,
        "frames": photos,
    }
    (out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(
        json.dumps(
            {
                k: summary[k]
                for k in (
                    "tested_runs",
                    "passing_pairs",
                    "selected_radii_m",
                    "reference",
                    "holds",
                    "frames",
                )
            },
            indent=2,
        )
    )
    require(
        all(d < 0.1 for d in native_distances.values()),
        "tiled contact point differs from the monolithic solver by over 10 cm: "
        + ", ".join(f"{k} {v:.4f} m" for k, v in native_distances.items() if v >= 0.1),
    )


if __name__ == "__main__":
    main()
