#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Measure the native box and mannequin in the resident and on-demand Bir-Hakeim captures."""

import argparse
import csv
import hashlib
import itertools
import json
import math
import runpy
import struct
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
REF = runpy.run_path(str(HERE / "check-paged-actor-render.py"))
read = REF["read"]
rgb = REF["rgb"]
log_run = REF["log_run"]
cross, dot, unit = REF["cross"], REF["dot"], REF["unit"]
ROUTES = ("parked", "moving")
TICKS = (24, 80)
MODES = ("resident", "streamed", "all")


def digest(path):
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def capture(exe, lock, out, name, mode, source, pages, replay, route, tick):
    png = out / (name + ".png")
    cmd = ["flock", lock, exe]
    if mode == "resident":
        cmd += ["--scene", source, "--physics", "rapier", "--full-detail"]
    else:
        cmd += [
            "--paging",
            "--pages",
            pages,
            "--out",
            out / (name + ".json"),
            "--trace-body",
        ]
        if mode == "all":
            cmd += [
                "--all-resident",
                "--gpu-trace",
                out / (name + "-gpu.json"),
                "--gpu-stride",
                10000,
            ]
    cmd += [
        "--replay",
        replay,
        "--route",
        route,
        "--actor",
        "human",
        "--tick",
        tick,
        "--shot",
        png,
    ]
    log_run(out, name, cmd)
    return png


def hull(points):
    """Monotone convex hull of the eight perspective-projected box corners."""

    def lower(ordered):
        side = []
        for p in ordered:
            while len(side) >= 2:
                a, b = side[-2:]
                if (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]) > 0:
                    break
                side.pop()
            side.append(p)
        return side

    ordered = sorted(set(points))
    return lower(ordered)[:-1] + lower(reversed(ordered))[:-1]


def box_projection(pose, extents, camera, viewport, fov):
    eye, aim = camera["eye"], camera["aim"]
    forward = unit([b - a for a, b in zip(eye, aim, strict=True)])
    right = unit(cross(forward, [0, 1, 0]))
    up = cross(right, forward)
    focal = viewport[1] * 0.5 / math.tan(math.radians(fov * 0.5))
    qx, qy, qz, qw = pose["rotation_xyzw"]
    q = (qx, qy, qz)
    screen = []
    for signs in itertools.product((-1, 1), repeat=3):
        v = [s * x for s, x in zip(signs, extents, strict=True)]
        u = cross(q, v)
        uv = cross(q, u)
        relative = [
            origin - e + x + 2 * (qw * t + tt)
            for origin, e, x, t, tt in zip(pose["center"], eye, v, u, uv, strict=True)
        ]
        depth = dot(relative, forward)
        assert depth > 0
        screen.append(
            (
                viewport[0] / 2 + focal * dot(relative, right) / depth,
                viewport[1] / 2 - focal * dot(relative, up) / depth,
            )
        )
    return screen


def inside_polygon(polygon, viewport):
    width, height = viewport
    xs, ys = zip(*polygon, strict=True)
    sides = list(zip(polygon, polygon[1:] + polygon[:1], strict=True))
    result = []
    for y in range(max(0, int(min(ys)) - 1), min(height, math.ceil(max(ys)) + 1)):
        for x in range(max(0, int(min(xs)) - 1), min(width, math.ceil(max(xs)) + 1)):
            if all(
                (b[0] - a[0]) * (y + 0.5 - a[1]) - (b[1] - a[1]) * (x + 0.5 - a[0]) >= 0
                for a, b in sides
            ):
                result.append(y * width + x)
    return result


def compare(base, other, selected):
    different = severe = peak = 0
    for i in selected:
        j = 3 * i
        if base[j : j + 3] != other[j : j + 3]:
            different += 1
            delta = max(
                abs(x - y)
                for x, y in zip(base[j : j + 3], other[j : j + 3], strict=True)
            )
            severe += delta > 2
            peak = max(peak, delta)
    return {
        "pixels_compared": len(selected),
        "different_pixels": different,
        "over_two": severe,
        "max_channel_delta": peak,
    }


def visible_blue(base, inside):
    # Only count the blue-dominant bonnet faces in the projected box. The frozen
    # with/without render below independently measures this selection's precision.
    return {
        i
        for i in inside
        if base[3 * i + 2] > base[3 * i] + 10 and base[3 * i + 2] > base[3 * i + 1] + 8
    }


def frozen_replay(out, route, config, camera, row, selected, pose_source=""):
    names = [p["name"] for p in selected]
    current = row["parts_after_step"]
    bindings = []
    for name, pose in zip(names, current, strict=True):
        sample = {"center_m": pose["center"], "rotation_xyzw": pose["rotation_xyzw"]}
        bindings.append({"name": name, "previous": sample, "current": sample})
    box = row["boxes_after_step"][0]
    target = {
        **route,
        "ticks": 1,
        "start_m": [0, 0, 0],
        "velocity_m_s": [0, 0, 0],
        "camera": [dict(camera, tick=t) for t in (0, 1)],
        "ragdoll_activation": {
            "sample_interval_s": 1 / config["tick_hz"],
            "parts": bindings,
        },
    }
    paths = {}
    for use_box in (True, False):
        copy = dict(target)
        if use_box:
            copy["bonnet"] = {
                **route["bonnet"],
                "center_m": box["center"],
                "rotation_xyzw": box["rotation_xyzw"],
                "velocity_m_s": [0, 0, 0],
            }
        else:
            copy.pop("bonnet")
            copy["expected_support"] = "road"
        suffix = "box" if use_box else "empty"
        path = out / f"{route['id']}-frozen-{row['tick']:03}{pose_source}-{suffix}.json"
        path.write_text(json.dumps({**config, "routes": [copy]}))
        paths[suffix] = path
    return paths


def resident_states(exe, out, scene, replay):
    folder = out / "resident-states"
    log_run(
        out,
        "resident-states",
        [
            exe,
            "--scene",
            scene,
            "--replay",
            replay,
            "--physics",
            "rapier",
            "--actor",
            "human",
            "--bench",
            folder,
        ],
    )
    return stored_states(folder / "dynamics.csv")


def floating_bits(values):
    return struct.pack(f"<{len(values)}f", *values)


def solver_pose(pose):
    # serde serializes native f32 into the CSV with their shortest round-trip
    # decimals. Read back at the solver's precision before comparing to Value f64s.
    return {
        k: list(struct.unpack(f"<{len(v)}f", floating_bits(v))) for k, v in pose.items()
    }


def stored_states(path):
    states = {}
    with path.open() as stream:
        for r in csv.DictReader(stream):
            if (t := int(r["tick"])) not in TICKS:
                continue
            poses = json.loads(r["parts_json"])
            pelvis = {
                k: [float(r[name]) for name in fields]
                for k, fields in (
                    ("center", ("cx", "cy", "cz")),
                    ("rotation_xyzw", ("qx", "qy", "qz", "qw")),
                    ("linear_velocity", ("vx", "vy", "vz")),
                    ("angular_velocity", ("wx", "wy", "wz")),
                )
            }
            states[r["route"], t] = (
                solver_pose(json.loads(r["boxes_json"])[0]),
                [solver_pose(body) for body in (pelvis, *poses)],
            )
    return states


def mode_motion(box, actor, row, names):
    offsets = [
        math.dist(a["center"], b["center"])
        for a, b in zip(actor, row["parts_after_step"], strict=True)
    ]

    def equal(a, b):
        return all(
            floating_bits(a[k]) == floating_bits(b[k])
            for k in ("center", "rotation_xyzw", "linear_velocity", "angular_velocity")
        )

    exact = [equal(a, b) for a, b in zip(actor, row["parts_after_step"], strict=True)]
    largest = max(range(len(offsets)), key=offsets.__getitem__)
    return {
        "box_state_bit_exact": equal(box, row["boxes_after_step"][0]),
        "actor_states_bit_exact": sum(exact),
        "box_center_error_m": math.dist(
            box["center"], row["boxes_after_step"][0]["center"]
        ),
        "largest_actor_center_error_m": offsets[largest],
        "largest_actor_part": names[largest]["name"] if offsets[largest] else None,
    }


def compare_motion(route, config, summary, box, parts, row, stream, names):
    base = row["boxes_after_step"][0]
    summary["natural"]["all"]["state_vs_resident"] = mode_motion(box, parts, row, names)
    summary["natural"]["streamed"]["state_vs_resident"] = mode_motion(
        box, parts, stream, names
    )
    summary["natural"]["streamed"]["state_vs_preloaded"] = mode_motion(
        base, row["parts_after_step"], stream, names
    )
    summary["box_centers"] = {
        "resident": box["center"],
        "paged": base["center"],
        "error_m": math.dist(box["center"], base["center"]),
    }
    summary["max_actor_center_error_m"] = summary["natural"]["all"][
        "state_vs_resident"
    ]["largest_actor_center_error_m"]
    arguments = (
        route["bonnet"]["half_extents_m"],
        summary["camera"],
        config["viewport"],
        config["vertical_fov_deg"],
    )
    a, b = box_projection(box, *arguments), box_projection(base, *arguments)
    summary["max_box_corner_screen_error_px"] = max(
        math.dist(x, y) for x, y in zip(a, b, strict=True)
    )


def pose_swaps(exe, out, lock, report, numerical, config):
    """Change solved transforms in a single renderer with identical meshes and shadows."""
    road = out / "road-only.glb"
    local = out / "road-only"
    full = range(math.prod(report["viewport"]))
    results = report["routes"]
    for route in config["routes"]:
        name = route["id"]
        for tick in TICKS:
            label = f"{name}-{tick:03}"
            cfg = results[name]
            summary = cfg[tick] if tick in cfg else cfg[str(tick)]
            paged = read(out / (label + "-all.json"))
            row = paged["body_before_step"][tick]
            names = paged["actor"]["parts"]
            rep, parts = numerical[name, tick]
            normal = read(out / (label + "-streamed.json"))
            compare_motion(
                route,
                config,
                summary,
                rep,
                parts,
                row,
                normal["body_before_step"][tick],
                names,
            )
            daily = rgb(out / (label + "-frozen-resident-box.png"))
            inside = inside_polygon(summary["projected_hull"], report["viewport"])
            alternate = {}
            for tag, actor in (
                ("only-box", row["parts_after_step"]),
                ("all-parts", parts),
            ):
                other = {**row, "boxes_after_step": [rep], "parts_after_step": actor}
                path = frozen_replay(
                    out, route, config, summary["camera"], other, names, "-" + tag
                )["box"]
                probe = rgb(
                    capture(
                        exe,
                        lock,
                        out,
                        label + "-pose-swap-" + tag,
                        "resident",
                        road,
                        local,
                        path,
                        name,
                        0,
                    )
                )
                alternate[tag] = {
                    "full_frame": compare(daily, probe, full),
                    "projected_box": compare(daily, probe, inside),
                }
            summary["changed_pose_in_same_renderer"] = alternate
    path = out / "summary.json"
    path.write_text(json.dumps(report, indent=2) + "\n")
    return path


def run(exe, pages, out, lock):
    scene = HERE / "fixtures/bir-hakeim.glb"
    replay = HERE / "fixtures/bonnet.json"
    config = read(replay)
    assert read(pages / "index.json")["source_sha256"] == digest(scene)
    out.mkdir(parents=True, exist_ok=True)
    # One unchanged deck of the public district isolates the box and its
    # occlusion by the real mannequin from streaming of the background city.
    doc, packet = REF["GLB"]["read_glb"](scene)
    node = next(n for n in doc["nodes"] if n.get("name") == "support/road")
    doc["nodes"] = [node]
    doc["scenes"] = [{"nodes": [0]}]
    doc["scene"] = 0
    road = out / "road-only.glb"
    REF["GLB"]["write_glb"](road, doc, packet)
    local = out / "road-only"
    REF["COOK"](road, local, 512)
    numerical = resident_states(exe, out, scene, replay)
    size = config["viewport"]
    full = range(math.prod(size))
    report = {
        "binary_sha256": digest(exe),
        "replay_sha256": digest(replay),
        "scene_sha256": digest(scene),
        "pages_sha256": digest(pages / "index.json"),
        "viewport": size,
        "routes": {},
    }
    for route in config["routes"]:
        name = route["id"]
        assert name in ROUTES
        route_rows = {}
        for tick in TICKS:
            label = f"{name}-{tick:03}"
            images = {
                mode: rgb(
                    capture(
                        exe,
                        lock,
                        out,
                        label + "-" + mode,
                        mode,
                        scene,
                        pages,
                        replay,
                        name,
                        tick,
                    )
                )
                for mode in MODES
            }
            paged = read(out / (label + "-all.json"))
            normal = read(out / (label + "-streamed.json"))
            row = paged["body_before_step"][tick]
            box = row["boxes_after_step"][0]
            assert row["tick"] == tick and len(row["parts_after_step"]) == 15
            frame = read(out / (label + "-all-gpu.json"))["frames"][-1]
            camera = {"eye": frame["camera_eye"], "aim": frame["camera_aim"]}
            assert frame["tick"] == tick and frame["actor_draw_primitives"] == 15
            corners = box_projection(
                box,
                route["bonnet"]["half_extents_m"],
                camera,
                size,
                config["vertical_fov_deg"],
            )
            inside = inside_polygon(hull(corners), size)
            assert 25000 < len(inside) < 150000
            masks = {m: visible_blue(p, inside) for m, p in images.items()}
            frozen = frozen_replay(
                out, route, config, camera, row, paged["actor"]["parts"]
            )
            stable = {
                (mode, use): rgb(
                    capture(
                        exe,
                        lock,
                        out,
                        label + "-frozen-" + mode + "-" + use,
                        mode,
                        road,
                        local,
                        path,
                        name,
                        0,
                    )
                )
                for mode in ("resident", "all")
                for use, path in frozen.items()
            }
            true_masks = {
                m: {
                    i
                    for i in inside
                    if stable[m, "box"][3 * i : 3 * i + 3]
                    != stable[m, "empty"][3 * i : 3 * i + 3]
                }
                for m in ("resident", "all")
            }
            measured = true_masks["resident"] | true_masks["all"]
            assert len(measured) > 25000
            held = read(out / (label + "-frozen-all-box.json"))["body_before_step"][0]
            assert (
                math.dist(held["boxes_after_step"][0]["center"], box["center"]) < 1e-6
            )
            fixed_whole = compare(stable["resident", "box"], stable["all", "box"], full)
            fixed_box = compare(
                stable["resident", "box"], stable["all", "box"], measured
            )
            natural = {}
            for mode in ("all", "streamed"):
                target = images[mode]
                union = masks[mode] | masks["resident"]
                natural[mode] = {
                    "full_frame": compare(images["resident"], target, full),
                    "inside_projected_box": compare(images["resident"], target, inside),
                    "blue_surface": compare(images["resident"], target, union),
                    "blue_pixels": len(masks[mode]),
                    "blue_mask_symmetric_difference": len(
                        masks[mode] ^ masks["resident"]
                    ),
                }
            assert (
                fixed_box["different_pixels"] == 0
                and fixed_whole["different_pixels"] == 0
            )
            assert true_masks["resident"] == true_masks["all"]
            frozen_blue = visible_blue(stable["resident", "box"], inside)
            assert (
                len(frozen_blue) > 25000
                and len(frozen_blue & measured) / len(frozen_blue) > 0.95
            )
            route_rows[tick] = {
                "camera": camera,
                "projected_hull": hull(corners),
                "projected_box_pixels": len(inside),
                "first_contact": {
                    k: paged["first_contact"][k]
                    for k in ("tick", "substep", "part_name", "support")
                },
                "natural": natural,
                "resident_blue_pixels": len(masks["resident"]),
                "streamed_fine_tiles": normal["published"]["fine_tiles"],
                "preloaded_fine_tiles": paged["published"]["fine_tiles"],
                "frozen_match": {
                    "full_frame": fixed_whole,
                    "visible_box": fixed_box,
                    "mask_symmetric_difference": len(
                        true_masks["resident"] ^ true_masks["all"]
                    ),
                    "blue_pixels_also_changed_by_native_box": len(
                        frozen_blue & measured
                    ),
                    "native_blue_pixels": len(frozen_blue),
                },
            }
        report["routes"][name] = route_rows
    path = pose_swaps(exe, out, lock, report, numerical, config)
    print(path)
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    parser.add_argument("--pages", type=Path, required=True)
    parser.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/paged-bonnet/visual"
    )
    parser.add_argument(
        "--gpu-lock", type=Path, default=Path("/tmp/osage-ville-paris-gpu.lock")
    )
    args = parser.parse_args()
    run(args.bin.resolve(), args.pages.resolve(), args.out.resolve(), args.gpu_lock)
