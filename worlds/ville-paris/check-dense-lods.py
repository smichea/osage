#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Measure native camera submissions and GPU work for many instances of a converted multi-LOD model."""

import argparse
import copy
import hashlib
import json
import runpy
import statistics
import subprocess
from pathlib import Path

from paging_checks import require

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
lod = runpy.run_path(str(HERE / "check-paged-lods.py"))
cook = lod["COOK"]["cook"]
read, write, pixels = (lod[k] for k in ("read", "write", "rgb_pixels"))
CAMERAS = {
    "distant": ([18, 220, 80], [0, 0, 0]),
    "near": ([-21, 30, 12], [-20, 0, -1]),
    "side": ([21, 42, 14], [19, 0, 0]),
    "far": ([-24, 215, 86], [4, 0, 0]),
    "away": ([-24, 215, 86], [-24, 290, 130]),
}
ITINERARY = [
    (0, "distant"),
    (40, "distant"),
    (88, "near"),
    (128, "near"),
    (156, "side"),
    (196, "side"),
    (244, "far"),
    (284, "far"),
    (292, "away"),
    (332, "away"),
]
PLATEAUS = {"distant": 40, "near": 128, "side": 196, "far": 284, "away": 332}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def example(directory):
    """Build the independent, public two-level shape used by the paging tests."""
    original = directory / "generated-panel.glb"
    source = directory / "visual-panel.glb"
    lod["recipe"](original)

    def single_visual_node(doc):
        node = doc["nodes"][1]
        node["scale"] = [0.26] * 3
        node["translation"] = [0, 0, 0]
        node["extras"]["osage"]["collision"] = []
        node["extras"]["osage"]["acoustic"] = []
        doc["nodes"] = [node]
        doc["scenes"][0]["nodes"] = [0]

    lod["with_document"](original, source, single_visual_node)
    return source


def build(source, dest, rows, cols, policy):
    details = {}

    def multiply(doc):
        (root,) = doc["scenes"][doc.get("scene", 0)]["nodes"]
        master = doc["nodes"][root]
        require(
            not master.get("children") and "mesh" in master,
            "expected a single placed mesh node",
        )
        config = master["extras"]["osage"]
        medium = next(v for v in config["visualLods"] if v["level"] == "medium")
        count = lambda mesh: sum(
            doc["accessors"][p["indices"]]["count"] // 3
            for p in doc["meshes"][mesh]["primitives"]
        )
        require(
            all(
                len(doc["meshes"][mesh]["primitives"]) == 1
                for mesh in (master["mesh"], medium["mesh"])
            ),
            "the exact native entity budget expects one primitive per level",
        )
        details.update(
            {
                "source_node": config["id"],
                "high_triangles": count(master["mesh"]),
                "medium_triangles": count(medium["mesh"]),
                "deviation_m": medium["maxDeviationM"],
            }
        )
        tree = [{"name": "field", "children": [], "extras": {"osage": {"id": "field"}}}]
        for row in range(rows):
            row_node = len(tree)
            tree[0]["children"].append(row_node)
            tree.append(
                {
                    "name": f"row-{row}",
                    "children": [],
                    "translation": [0, 0, (row - (rows - 1) / 2) * 3],
                    "extras": {"osage": {"id": f"row-{row}"}},
                }
            )
            for col in range(cols):
                leaf = copy.deepcopy(master)
                owner = len(tree)
                tree[row_node]["children"].append(owner)
                sid = f"row-{row}-plant-{col}"
                cfg = leaf["extras"]["osage"]
                cfg["id"] = sid
                for channel in ("collision", "acoustic"):
                    for surface in cfg.get(channel, []):
                        surface["id"] = sid + "/" + surface["id"]
                if policy != "adaptive":
                    del cfg["visualLods"]
                    if policy == "medium":
                        leaf["mesh"] = medium["mesh"]
                x = (col - (cols - 1) / 2) * 3
                if "matrix" in leaf:
                    leaf["matrix"][12] += x
                else:
                    leaf.setdefault("translation", [0, 0, 0])[0] += x
                leaf["name"] = sid
                tree.append(leaf)
        doc["nodes"] = tree
        doc["scenes"][doc.get("scene", 0)]["nodes"] = [0]

    lod["with_document"](source, dest, multiply)
    return details


def render(exe, pages, replay, label, out, lock, tick, capture, extra=()):
    command = [
        "flock",
        lock,
        exe,
        "--paging",
        "--route",
        "field",
        "--replay",
        replay,
        "--pages",
        pages,
        *extra,
        "--gpu-owner",
        "2",
        "--gpu-trace",
        out / (label + "-gpu.json"),
        "--gpu-stride",
        "4",
        "--tick",
        str(tick),
        "--out",
        out / (label + "-reader.json"),
    ]
    if capture:
        command.extend(("--shot", out / (label + ".png")))
    with (out / (label + ".log")).open("w") as log:
        subprocess.run(
            command, check=True, timeout=180, stdout=log, stderr=subprocess.STDOUT
        )


def plain_capture(exe, out, replay, tick, lock, expected):
    name = f"plain-t{tick}"
    shot = out / (name + ".png")
    cmd = [
        "flock",
        lock,
        exe,
        "--paging",
        "--all-resident",
        "--pages",
        out / "adaptive",
        "--replay",
        replay,
        "--route",
        "field",
        "--tick",
        str(tick),
        "--out",
        out / (name + "-reader.json"),
        "--shot",
        shot,
    ]
    with (out / (name + ".log")).open("w") as log:
        subprocess.run(
            cmd, check=True, timeout=180, stdout=log, stderr=subprocess.STDOUT
        )
    raw = pixels(shot)
    require(
        raw == expected, "ordinary untraced capture differs from tracked native camera"
    )
    return {"different_pixels": 0, "rgb_sha256": hashlib.sha256(raw).hexdigest()}


def material_witnesses(exe, out, replay, ticks, lock, pictures):
    """Measure final-image contributions using the same camera and full postprocess."""
    against_background, opaque = {}, {}
    used = [t for t in (40, 128, 216, 284) if t in ticks]
    if not used:
        return against_background, opaque

    def off_camera(doc):
        doc["nodes"][0]["translation"] = [10000, 0, 0]

    def opaque_material(doc):
        for mat in doc["materials"]:
            mat["alphaMode"] = "OPAQUE"
            mat.pop("alphaCutoff", None)

    for control, field, edit in (
        ("offscreen", "adaptive", off_camera),
        ("opaque-medium", "medium", opaque_material),
    ):
        samples = (
            used
            if control == "offscreen"
            else [
                t
                for t in (40, 284)
                if str(t) in against_background
                and against_background[str(t)]["over_background_pixels"] == 0
            ]
        )
        if not samples:
            continue
        glb, pages = out / (control + ".glb"), out / control
        lod["with_document"](out / (field + ".glb"), glb, edit)
        cook(glb, pages, 32)
        for tick in samples:
            label = f"{control}-t{tick}"
            render(exe, pages, replay, label, out, lock, tick, True, ["--all-resident"])
            frames = read(out / (label + "-gpu.json"))["frames"]
            raw = pixels(out / (label + ".png"))
            rgb = hashlib.sha256(raw).hexdigest()
            if control == "offscreen":
                require(
                    all(f["render_world_camera_meshes"] == 0 for f in frames),
                    "background control has visible geometry",
                )
                against_background[str(tick)] = {
                    "submitted_meshes": 0,
                    "rgb_sha256": rgb,
                    "over_background_pixels": lod["different"](
                        raw, pictures["adaptive", tick]
                    ),
                }
            else:
                bg = pixels(out / f"offscreen-t{tick}.png")
                f = frames[-1]
                opaque[str(tick)] = {
                    "meshes": f["render_world_camera_meshes"],
                    "clipper": f["latest_completed_opaque_pass"]["clipper_invocations"],
                    "rgb_sha256": rgb,
                    "over_background_pixels": lod["different"](raw, bg),
                }
    return against_background, opaque


def summarize(out, label):
    report, stream = [
        read(out / (label + suffix)) for suffix in ("-gpu.json", "-reader.json")
    ]
    frames = report["frames"]
    events = stream["storage"]["visual_lods"]["events"]
    by_tick = {}
    for e in events:
        by_tick.setdefault(e["tick"], []).append((e["owner"], e["level"]))
    active, prior = {}, {}
    ages = {}
    prior_tick = -1
    output = []
    generation = -1
    for f in frames:
        tick = f["tick"]
        for t in range(prior_tick + 1, tick + 1):
            for owner, level in by_tick.get(t, []):
                if level is None:
                    active.pop(owner, None)
                else:
                    active[owner] = level
        change = {
            owner
            for owner in set(active) | set(prior)
            if active.get(owner) != prior.get(owner)
        }
        v = f["group_visibility"]
        hw = f["latest_completed_opaque_pass"]
        output.append(
            {
                "tick": tick,
                "eye": f["camera_eye"],
                "aim": f["camera_aim"],
                "high": sum(x == "high" for x in active.values()),
                "medium": sum(x == "medium" for x in active.values()),
                "min_spawn": sum(owner in active for owner in change),
                "min_retire": sum(owner in prior for owner in change),
                "indexed": v["last_indexed_placements"]
                if v["scene_generation"] != generation
                else 0,
                "retired_from_index": v["last_retired_placements"]
                if v["scene_generation"] != generation
                else 0,
                "camera_meshes": f["render_world_camera_meshes"],
                "vertices": hw.get("vertex_shader_invocations"),
                "clipper": hw.get("clipper_invocations"),
                "fragments": hw.get("fragment_shader_invocations"),
                "opaque_ms": hw.get("elapsed_gpu"),
                "frame_ms": f["wall_to_finished_gpu_frame_ns"] / 1e6,
                "prepared": f["prepared_assets"],
                "geometry_bytes": f["drawable_payload_bytes"],
                "uploads": f["mesh_uploads"],
                "gpu_bytes_uploaded": f["mesh_payload_uploaded"],
                "spawned": f.get("placement_entities_spawned"),
                "despawned": f.get("placement_entities_retired"),
                "reused": f.get("placement_entities_reused"),
                "generation": v["scene_generation"],
                "publish_us": f.get("placement_publication_ns", 0) / 1000,
            }
        )
        prior = active.copy()
        prior_tick = tick
        generation = v["scene_generation"]
    for name, end in PLATEAUS.items():
        subset = [f for f in output if end - 12 <= f["tick"] <= end]
        if not subset:
            continue
        stable = subset[-1]
        ages[name] = {
            k: stable[k]
            for k in (
                "tick",
                "high",
                "medium",
                "camera_meshes",
                "clipper",
                "vertices",
                "fragments",
                "prepared",
                "geometry_bytes",
                "uploads",
            )
        }
        for metric in ("opaque_ms", "frame_ms"):
            values = [f[metric] for f in subset if f[metric] is not None]
            ages[name][metric] = statistics.median(values) if values else None
    return {
        "frames": output,
        "plateaus": ages,
        "lod_events": len(events),
        "visual_reads": stream["visual"],
        "model_io": {
            k: v
            for k, v in stream["storage"]["model_io"].items()
            if not k.endswith("_us")
        },
        "image_io": {
            k: v
            for k, v in stream["storage"]["image_io"].items()
            if not k.endswith("_us")
        },
        "renderer": [report[k] for k in ("adapter", "driver", "backend")],
        "poses_sha256": report["replay_sha256"],
        "choice_timing": report.get("lod_selection_cpu_ns"),
        "event_digest": hashlib.sha256(
            json.dumps(events, sort_keys=True).encode()
        ).hexdigest(),
        "requests_digest": hashlib.sha256(
            json.dumps(
                stream["storage"]["visual_lods"]["requests"], sort_keys=True
            ).encode()
        ).hexdigest(),
    }


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument(
        "--model",
        type=Path,
        help="converted single-root GLB with a certified medium alternate; default uses a generated two-level panel",
    )
    cli.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    cli.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/dense-lods"
    )
    cli.add_argument("--rows", type=int, default=20)
    cli.add_argument("--columns", type=int, default=24)
    cli.add_argument(
        "--lock", type=Path, default=Path("/tmp/osage-ville-paris-gpu.lock")
    )
    cli.add_argument(
        "--reference",
        type=Path,
        help="optional previous executable without binding reuse",
    )
    cli.add_argument("--trials", type=int, default=3)
    cli.add_argument(
        "--capture-ticks", nargs="*", type=int, default=[40, 128, 216, 284, 332]
    )
    args = cli.parse_args()
    require(
        args.rows > 0 and args.columns > 1, "expected at least two repeated placements"
    )
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    model = args.model or example(out)
    replay = out / "camera.json"
    write(
        replay,
        {
            "version": 1,
            "tick_hz": 60,
            "gravity_m_s2": 9.81,
            "terminal_m_s": 55,
            "viewport": [320, 180],
            "vertical_fov_deg": 60,
            "routes": [
                {
                    "id": "field",
                    "ticks": 332,
                    "start_m": [10000, 100, 10000],
                    "velocity_m_s": [0, 0, 0],
                    "expected_support": "none",
                    "camera": [
                        {"tick": t, "eye": CAMERAS[name][0], "aim": CAMERAS[name][1]}
                        for t, name in ITINERARY
                    ],
                }
            ],
        },
    )
    result = {
        "binary_sha256": digest(args.bin),
        "model_sha256": digest(model),
        "input": "provided" if args.model else "generated panel",
        "rows": args.rows,
        "columns": args.columns,
        "cases": {},
    }
    require(args.trials >= 1, "at least one native trial required")
    meta = {}
    for policy in ("adaptive", "high", "medium"):
        glb = out / (policy + ".glb")
        pages = out / policy
        result["model"] = build(model, glb, args.rows, args.columns, policy)
        cook(glb, pages, 32)
        index = read(pages / "index.json")
        require(
            sum(p is not None for p in index["source_parents"])
            == args.rows * (args.columns + 1),
            "source hierarchy changed",
        )
        meta[policy] = {
            "parent_digest": hashlib.sha256(
                json.dumps(index["source_parents"], separators=(",", ":")).encode()
            ).hexdigest(),
            "index_sha256": digest(pages / "index.json"),
            "cell_count": len(index["tiles"]),
        }
    require(
        len({m["parent_digest"] for m in meta.values()}) == 1,
        "source parent tree changed",
    )
    resident = ["--all-resident"]
    demand = ["--delay-ms", "80", "--visual-radius", "500", "--mbps-per-kind", "8"]
    rebuild = ["--gpu-bindings", "rebuild"]
    configs = {
        "adaptive": (args.bin, "adaptive", resident),
        "rebuild": (args.bin, "adaptive", [*resident, *rebuild]),
        "high": (args.bin, "high", resident),
        "medium": (args.bin, "medium", resident),
        "demand": (args.bin, "adaptive", demand),
        "demand-rebuild": (args.bin, "adaptive", [*demand, *rebuild]),
    }
    if args.reference:
        result["reference_binary_sha256"] = digest(args.reference)
        configs["previous"] = (args.reference, "adaptive", resident)
    modes = list(configs)
    pictures = {}
    timings = {name: [] for name in modes}
    for trial in range(args.trials):
        step = trial % len(modes)
        for name in modes[step:] + modes[:step]:
            exe, policy, options = configs[name]
            pages = out / policy
            label = f"{name}-{trial}"
            render(
                exe,
                pages,
                replay,
                label,
                out,
                args.lock,
                332,
                trial == 0 and 332 in args.capture_ticks,
                options,
            )
            run = summarize(out, label)
            frames = run["frames"]
            timings[name].append(
                {
                    "placements_published_us": sum(f["publish_us"] for f in frames[1:]),
                    "choice_timing": run["choice_timing"],
                    "plateaus_opaque_gpu_ms": {
                        k: v["opaque_ms"] for k, v in run["plateaus"].items()
                    },
                }
            )
            if trial == 0:
                run.update(meta[policy])
                run["snapshots"] = {}
                for tick in args.capture_ticks:
                    image_label = label if tick == 332 else f"{name}-t{tick}"
                    if tick != 332:
                        render(
                            exe,
                            pages,
                            replay,
                            image_label,
                            out,
                            args.lock,
                            tick,
                            True,
                            options,
                        )
                    raw = pixels(out / (image_label + ".png"))
                    pictures[name, tick] = raw
                    run["snapshots"][str(tick)] = hashlib.sha256(raw).hexdigest()
                result["cases"][name] = run
            else:
                same_submission(run, result["cases"][name])
            result["cases"][name]["trials"] = timings[name]
            write(out / "summary.json", result)
            print(
                name,
                trial + 1,
                frames[-1]["spawned"],
                frames[-1]["despawned"],
                run["plateaus"]["far"]["clipper"],
                flush=True,
            )
    cases = result["cases"]
    a, h, m = [cases[k] for k in ("adaptive", "high", "medium")]
    for left, right in (("adaptive", "rebuild"), ("demand", "demand-rebuild")):
        p, q = cases[left], cases[right]
        same_submission(p, q)
        require(
            p["snapshots"] == q["snapshots"],
            "retaining native entity changed final pixels",
        )
        end = p["frames"][-1]
        ref = q["frames"][-1]
        require(
            end["spawned"] == sum(f["min_spawn"] for f in p["frames"])
            and end["despawned"] == sum(f["min_retire"] for f in p["frames"]),
            "ordinary source-slot changes disagree with published native entities",
        )
        require(
            ref["spawned"] > end["spawned"]
            and ref["despawned"] > end["despawned"]
            and end["reused"] > 0,
            "whole-page witness found no redundant submissions",
        )
        require(
            ref["spawned"] == sum(f["indexed"] for f in q["frames"]),
            "complete rebuild count differs",
        )
    if args.reference:
        same_submission(a, cases["previous"])
        require(
            a["snapshots"] == cases["previous"]["snapshots"],
            "prior production pixels differ",
        )
    require(
        all(
            [f["eye"], f["aim"]] == [g["eye"], g["aim"]] == [j["eye"], j["aim"]]
            for f, g, j in zip(a["frames"], h["frames"], m["frames"], strict=True)
        ),
        "camera inputs differ",
    )
    far, near = a["plateaus"]["far"], a["plateaus"]["near"]
    require(
        far["medium"] == args.rows * args.columns
        and far["high"] == 0
        and near["high"] > 10
        and near["medium"] == 0,
        "representative LOD choices not exercised",
    )
    for pose in ("distant", "far"):
        d, full, low = (mode["plateaus"][pose] for mode in (a, h, m))
        require(
            d["clipper"] == low["clipper"] < full["clipper"]
            and d["vertices"] == low["vertices"] < full["vertices"],
            "distant geometry did not reach GPU",
        )
        require(
            d["clipper"]
            == args.rows * args.columns * result["model"]["medium_triangles"]
            and full["clipper"]
            == args.rows * args.columns * result["model"]["high_triangles"],
            "resident draw/triangle budget diverges from actual clipped primitives",
        )
    require(
        near["clipper"]
        == h["plateaus"]["near"]["clipper"]
        > m["plateaus"]["near"]["clipper"],
        "near control failed to retain the high geometry",
    )
    require(
        all(
            x["plateaus"]["away"]["camera_meshes"] == 0
            and x["plateaus"]["away"]["clipper"] == 0
            for x in cases.values()
        ),
        "off-camera scene submitted visible geometry",
    )
    require(
        any(f["high"] and f["medium"] for f in a["frames"]),
        "moving path never mixed genuine LODs",
    )
    require(
        cases["demand"]["visual_reads"]["reads"]
        and cases["demand"]["model_io"]["reads"]
        and cases["demand"]["image_io"]["reads"],
        "demand control did not read real content",
    )
    result["image_differences"] = {}
    for tick in args.capture_ticks:
        result["image_differences"][str(tick)] = {
            other: lod["different"](pictures["adaptive", tick], pictures[other, tick])
            for other in ("high", "medium", "demand")
        }
    if 128 in args.capture_ticks:
        require(
            pictures["adaptive", 128] == pictures["high", 128]
            and pictures["adaptive", 128] != pictures["medium", 128],
            "close silhouette negative control failed",
        )
    for name, case in cases.items():
        case["median_publication_after_first_us"] = statistics.median(
            v["placements_published_us"] for v in timings[name]
        )
        case["median_gpu_ms"] = {
            pose: statistics.median(
                t["plateaus_opaque_gpu_ms"][pose] for t in timings[name]
            )
            for pose in PLATEAUS
        }
    result["plain_captures"] = {
        str(tick): plain_capture(
            args.bin, out, replay, tick, args.lock, pictures["adaptive", tick]
        )
        for tick in (128, 216)
        if tick in args.capture_ticks
    }
    result["background"], result["opaque_control"] = material_witnesses(
        args.bin, out, replay, args.capture_ticks, args.lock, pictures
    )
    result["complete"] = True
    write(out / "summary.json", result)


def same_submission(left, right):
    keys = (
        "tick",
        "eye",
        "aim",
        "high",
        "medium",
        "camera_meshes",
        "min_spawn",
        "min_retire",
        "geometry_bytes",
        "prepared",
        "uploads",
        "gpu_bytes_uploaded",
    )
    require(
        all(
            all(x[k] == y[k] for k in keys)
            for x, y in zip(left["frames"], right["frames"], strict=True)
        ),
        "native submissions, resource lifetimes or owner selection changed",
    )
    require(
        all(
            left[k] == right[k]
            for k in (
                "renderer",
                "poses_sha256",
                "event_digest",
                "requests_digest",
                "visual_reads",
                "model_io",
                "image_io",
            )
        ),
        "LOD decisions or demand reads changed",
    )
    for p in PLATEAUS:
        # Vulkan returns the last completed counter sample asynchronously. Use settled camera poses for numerical work.
        require(
            all(
                left["plateaus"][p][k] == right["plateaus"][p][k]
                for k in ("clipper", "vertices", "camera_meshes")
            ),
            "settled GPU geometry changed",
        )


if __name__ == "__main__":
    main()
