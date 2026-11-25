#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Compare the native resident and paged images of a falling rigid mannequin and an imported actor."""

import argparse
import copy
import hashlib
import json
import math
import runpy
import struct
import subprocess
import sys
import zlib
from pathlib import Path

from paging_checks import glb, require

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
CONVERTER = ROOT / "tools/object-convert"
SAMPLE = runpy.run_path(str(CONVERTER / "test_hierarchy.py"))["sample"]
COOK = runpy.run_path(str(HERE / "cook-pages.py"))["cook"]
GLB = runpy.run_path(str(HERE / "check-impact-transmission.py"))
SIZE = (432, 288)
FOV = 43.0


def read(path):
    return json.loads(path.read_bytes())


def log_run(out, name, cmd, timeout=160):
    log = out / (name + ".log")
    with log.open("wb") as f:
        result = subprocess.run(
            list(map(str, cmd)),
            check=False,
            stdout=f,
            stderr=subprocess.STDOUT,
            timeout=timeout,
        )
    require(
        result.returncode == 0,
        f"{name} exited {result.returncode}: {log.read_text()[-900:]}",
    )


def write_replay(out, doc, key, target):
    # Compose a fixed observer independent of subsequent body and render updates.
    eye = [v + d for v, d in zip(target, (1.15, 2.30, 3.10), strict=True)]
    route = doc["routes"][0]
    camera = {"eye": eye, "aim": target}
    route["camera"] = [dict(camera, tick=t) for t in (0, route["ticks"])]
    path = out / (key + "-route.json")
    path.write_text(json.dumps(doc))
    return path, camera


def align_visual_slab(source, out):
    # The original sample deliberately puts its visual surface at 1.25 m and its
    # collider at 1 m. For this contact camera, lower only the two visual vertex sets.
    doc, chunk = GLB["read_glb"](source)
    binary = bytearray(chunk)
    levels = set()
    solids = set()
    for node in doc["nodes"]:
        if "mesh" in node:
            levels.add(node["mesh"])
        geom = node.get("extras", {}).get("osage", {})
        levels.update(v["mesh"] for v in geom.get("visualLods", []))
        solids.update(
            v["mesh"] for k in ("collision", "acoustic") for v in geom.get(k, [])
        )

    def positions(meshes):
        return {
            p["attributes"]["POSITION"]
            for m in meshes
            for p in doc["meshes"][m]["primitives"]
        }

    render_pos, solid_pos = positions(levels), positions(solids)
    require(
        render_pos.isdisjoint(solid_pos),
        "visual/control surface accessors unexpectedly overlap",
    )
    require(
        all(
            doc["accessors"][p]["min"][1] == doc["accessors"][p]["max"][1] == 1
            for p in solid_pos
        ),
        "expected the original flat support at 1 m",
    )
    for i in render_pos:
        accessor = doc["accessors"][i]
        view = doc["bufferViews"][accessor["bufferView"]]
        require(
            accessor["componentType"] == 5126 and accessor["type"] == "VEC3",
            "unexpected visual position encoding",
        )
        first = 8 + view.get("byteOffset", 0) + accessor.get("byteOffset", 0) + 4
        for j in range(accessor["count"]):
            offset = first + j * view.get("byteStride", 12)
            require(
                struct.unpack_from("<f", binary, offset)[0] == 1.25,
                "unexpected visible slab elevation",
            )
            struct.pack_into("<f", binary, offset, 1.0)
        accessor["min"][1] = accessor["max"][1] = 1.0
    path = out / "aligned-visual.glb"
    GLB["write_glb"](path, doc, bytes(binary))
    return path


def setup(out, place, paris_pages=None):
    out.mkdir(parents=True, exist_ok=True)
    pages = out / "district"
    if place == "ledge":
        terrain = runpy.run_path(str(HERE / "check-paged-ragdoll.py"))
        source, _, _, template = terrain["prepare"](out)
        doc = read(terrain["routes"](out, 60, template))
        doc["routes"] = [next(r for r in doc["routes"] if r["id"] == "outside-hand")]
    elif place == "bir-hakeim":
        source = HERE / "fixtures/bir-hakeim.glb"
        doc = read(HERE / "fixtures/deck-edge.json")
        doc["routes"] = [next(r for r in doc["routes"] if r["id"] == "inside")]
    else:
        source, initial = SAMPLE(out)
        doc = read(initial)
    if place != "bir-hakeim":
        source = align_visual_slab(source, out)
        pages = out / "aligned-district"
    if place == "bir-hakeim" and paris_pages:
        pages = paris_pages
        require(
            read(pages / "index.json")["source_sha256"]
            == hashlib.sha256(source.read_bytes()).hexdigest(),
            "Bir-Hakeim source and cached district do not match",
        )
    else:
        COOK(source, pages, 128 if place == "span" else 32)
    doc.update(viewport=SIZE, vertical_fov_deg=FOV)
    route = doc["routes"][0]
    replay, _ = write_replay(out, doc, "initial", route["start_m"])
    return source, pages, doc, replay


def rig_files(out):
    rig = out / "pedestrian.glb"
    log_run(
        out,
        "convert",
        [
            sys.executable,
            CONVERTER / "convert.py",
            "--id",
            "pedestrian",
            "--fragment",
            CONVERTER / "fixtures/pedestrian.yft.xml",
            "-o",
            rig,
        ],
    )
    return rig


def without_draw(out, source, part_name):
    doc, chunk = GLB["read_glb"](source)
    originals = copy.deepcopy(doc["asset"]["extras"]["osageObject"]["articulation"])
    (node,) = (n for n in doc["nodes"] if n.get("name") == part_name)
    node.pop("mesh")
    require(
        not node["extras"]["osage"].get("visualLods"),
        "unexpected secondary draw on removed visual",
    )
    require(
        doc["asset"]["extras"]["osageObject"]["articulation"] == originals,
        "rig control changed the rig",
    )
    path = out / ("without-" + part_name + ".glb")
    GLB["write_glb"](path, doc, chunk)
    return path


def actor_args(rig):
    return ["--actor-rig", rig] if rig else ["--actor", "human"]


def motion(exe, out, pages, replay, route, rig, label):
    target = out / (label + "-motion.json")
    log_run(
        out,
        label + "-motion",
        [
            exe,
            "--paging",
            "--all-resident",
            "--pages",
            pages,
            "--replay",
            replay,
            "--route",
            route,
            "--trace-body",
            "--out",
            target,
            *actor_args(rig),
        ],
    )
    r = read(target)
    require(len(r["actor"]["parts"]) == 15, f"{label} lost rigid bodies")
    return r


def rgb(path):
    return subprocess.check_output(
        [
            "ffmpeg",
            "-loglevel",
            "error",
            "-i",
            path,
            "-threads",
            "1",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "-",
        ]
    )


def comparison(base, other):
    require(
        len(base) == len(other) == 3 * math.prod(SIZE), "unexpected native pixel count"
    )
    count = high = peak = 0
    for i in range(0, len(base), 3):
        delta = max(abs(base[i + k] - other[i + k]) for k in range(3))
        count += delta > 0
        high += delta > 2
        peak = max(peak, delta)
    return {"different_pixels": count, "over_two": high, "max_channel_delta": peak}


def cross(a, b):
    return [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]


def unit(v):
    length = math.hypot(*v)
    return [x / length for x in v]


def dot(a, b):
    return sum(x * y for x, y in zip(a, b, strict=True))


def projected_bounds(pose, camera, radius):
    eye = camera["eye"]
    forward = unit([v - e for v, e in zip(camera["aim"], eye, strict=True)])
    right = unit(cross(forward, (0, 1, 0)))
    up = cross(right, forward)
    rel = [p - e for p, e in zip(pose["center"], eye, strict=True)]
    x, y, z = dot(rel, right), dot(rel, up), dot(rel, forward)
    require(z > radius, "the camera intersects the body sphere")
    focal = SIZE[1] / (2 * math.tan(math.radians(FOV) / 2))
    cx, cy = SIZE[0] / 2 + focal * x / z, SIZE[1] / 2 - focal * y / z
    # A sphere changes both numerator and denominator of the perspective divide.
    bound = focal * radius * (1 + max(abs(x), abs(y)) / z) / (z - radius)
    return cx, cy, bound


def actor_region(poses, parts, camera, base, other):
    bounds = [
        projected_bounds(pose, camera, part["radius_m"] + part["half_segment_m"])
        for pose, part in zip(poses, parts, strict=True)
    ]
    x0 = max(0, math.floor(min(x - r for x, _, r in bounds)) - 2)
    y0 = max(0, math.floor(min(y - r for _, y, r in bounds)) - 2)
    x1 = min(SIZE[0], math.ceil(max(x + r for x, _, r in bounds)) + 2)
    y1 = min(SIZE[1], math.ceil(max(y + r for _, y, r in bounds)) + 2)
    changed = 0
    for y in range(y0, y1):
        for x in range(x0, x1):
            i = 3 * (y * SIZE[0] + x)
            changed += base[i : i + 3] != other[i : i + 3]
    return {"bounds_px": [x0, y0, x1, y1], "different_pixels": changed}


def difference_near_body(base, removed, pose, camera):
    # 30 cm covers each tested limb's half-length and radius. Require a real local colour change.
    cx, cy, radius = projected_bounds(pose, camera, 0.30)
    changed = 0
    for y in range(max(0, int(cy - radius)), min(SIZE[1], math.ceil(cy + radius))):
        for x in range(max(0, int(cx - radius)), min(SIZE[0], math.ceil(cx + radius))):
            if (x + 0.5 - cx) ** 2 + (y + 0.5 - cy) ** 2 < radius**2:
                i = 3 * (x + y * SIZE[0])
                changed += max(abs(base[i + k] - removed[i + k]) for k in range(3)) > 7
    return {
        "projected_center_px": [cx, cy],
        "radius_px": radius,
        "pixels_over_seven_inside": changed,
    }


def render(
    exe,
    lock,
    out,
    name,
    mode,
    source,
    pages,
    replay,
    route,
    tick,
    rig,
    expected_draws=15,
):
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
            out / (name + "-paging.json"),
        ]
        if mode != "streamed":
            cmd.append("--all-resident")
    if mode == "traced":
        cmd += ["--gpu-trace", out / (name + "-gpu.json"), "--gpu-stride", 10000]
    if mode == "groups-off":
        cmd += ["--gpu-groups", "off"]
    cmd += [
        "--replay",
        replay,
        "--route",
        route,
        "--tick",
        tick,
        "--shot",
        png,
        *actor_args(rig),
    ]
    log_run(out, name, cmd)
    if mode == "traced":
        gpu = read(out / (name + "-gpu.json"))
        frame = gpu["frames"][-1]
        require(
            frame["tick"] == tick and frame["actor_bodies"] == 15,
            "render used the wrong solved tick or body count",
        )
        require(
            frame["actor_draw_primitives"] == expected_draws,
            "the published draw count differs from the authored visual bindings",
        )
    return rgb(png)


def contact(rows, part_id):
    return next(
        row
        for row in rows
        if any(
            c["part"] == part_id and c["separation"] <= 0
            for c in row["contacts_after_step"]
        )
    )


def freeze_pose(folder, key, replay, names, poses):
    # The animation handoff accepts centre of mass and orientation in actor coordinates.
    # At zero origin the world trace can be replayed directly, with no solver step.
    frozen = read(replay)
    route = frozen["routes"][0]
    camera = route["camera"][0]
    parts = []
    for name, pose in zip(names, poses, strict=True):
        body = {"center_m": pose["center"], "rotation_xyzw": pose["rotation_xyzw"]}
        parts.append({"name": name, "previous": body, "current": body})
    route.update(
        ticks=1,
        start_m=[0, 0, 0],
        velocity_m_s=[0, 0, 0],
        camera=[dict(camera, tick=t) for t in (0, 1)],
        ragdoll_activation={"sample_interval_s": 1 / 60, "parts": parts},
    )
    path = folder / (key + "-frozen-route.json")
    path.write_text(json.dumps(frozen))
    return path


def page_for_resident(folder, pages):
    # Keep the exact static matrices, vertices, material and draw order from the local
    # visual page. A flat collider name allows the resident CLI to open this same scene.
    index = read(pages / "index.json")
    (tile,) = index["tiles"]
    visual = tile["visual"]
    raw = (pages / visual["file"]).read_bytes()
    require(
        hashlib.sha256(raw).hexdigest() == visual["sha256"] and raw[:4] == b"OVP1",
        "expected a verified local placement page",
    )
    decoded = zlib.decompress(raw[8:])
    end = 4 + struct.unpack_from("<I", decoded)[0]
    doc, chunk = glb(decoded[end:])
    require(
        json.loads(decoded[4:end]) == list(range(len(doc["nodes"]))),
        "unexpected local draw order",
    )
    (owner,) = [n for n in doc["nodes"] if n["extras"]["osage_source_node"] == 2]
    owner["name"] = "collider/west-floor"
    path = folder / "exact-fine-page.glb"
    GLB["write_glb"](path, doc, chunk)
    return path


def raised_control(exe, lock, folder, route, frozen, rig, hidden):
    # This original visual plane stands 25 cm above the physical slab and hides the forearm.
    images = [
        render(
            exe,
            lock,
            folder,
            "raised-arm-" + label,
            "resident",
            folder / "object.glb",
            folder / "district",
            frozen,
            route,
            0,
            actor,
        )
        for label, actor in (("full", rig), ("without", hidden))
    ]
    delta = comparison(*images)
    require(
        images[0] == images[1],
        "removing the occluded forearm changed the original elevated surface",
    )
    (folder / "raised-arm-control.json").write_text(json.dumps(delta, indent=2) + "\n")
    return delta


def inspect_model(
    exe, lock, folder, model, source, pages, doc, initial, rig, place, old
):
    route = doc["routes"][0]["id"]
    solved = motion(exe, folder, pages, initial, route, rig, model)
    exact_static = page_for_resident(folder, pages) if place != "bir-hakeim" else None
    names = [p["name"] for p in solved["actor"]["parts"]]
    rows = solved["body_before_step"]
    first = next(i for i, row in enumerate(rows) if row["contacts_after_step"])
    targets = [("first-contact", rows[first], None)]
    if place == "ledge":
        hit = solved["first_contact"]
        require(
            hit["part_name"] == "hand_l" and hit["tile"] == [1, 0],
            "expected an isolated left-hand terrain hit",
        )
        targets = [("hand", rows[first], "hand_l")]
    if place == "span":
        targets = [
            ("in-air", rows[first // 2], None),
            *targets,
            ("arm", contact(rows, names.index("lowerarm_l")), "lowerarm_l"),
            ("torso", contact(rows, names.index("spine_01")), "spine_01"),
        ]
    cases = {}
    for phase, row, part in targets:
        tick = row["tick"]
        key = f"{model}-{phase}"
        poses = row["parts_after_step"]
        center = list(poses[0]["center"])
        center[1] += 0.12
        replay, camera = write_replay(folder, doc, key, center)
        args = (source, pages, replay, route, tick)
        baseline = render(exe, lock, folder, key + "-traced", "traced", *args, rig)
        direct = render(exe, lock, folder, key + "-plain", "plain", *args, rig)
        reference = render(exe, lock, folder, key + "-resident", "resident", *args, rig)
        result = {
            "tick": tick,
            "camera": camera,
            "draws": 15,
            "bodies": 15,
            "contacts": sorted({names[c["part"]] for c in row["contacts_after_step"]}),
            "traced_vs_plain": comparison(baseline, direct),
            "traced_vs_resident": comparison(baseline, reference),
            "rgb_sha256": hashlib.sha256(baseline).hexdigest(),
        }
        require(
            baseline == direct, f"{key} differs with and without GPU tracing: {result}"
        )
        if phase == "in-air":
            result["projected_actor_vs_resident"] = actor_region(
                poses, solved["actor"]["parts"], camera, baseline, reference
            )
            require(
                result["projected_actor_vs_resident"]["different_pixels"] == 0
                and result["traced_vs_resident"]["max_channel_delta"] <= 1,
                f"airborne source actor differs from its paged image: {result}",
            )
        if place != "bir-hakeim" and phase != "in-air":
            frozen = freeze_pose(folder, key, replay, names, poses)
            same_pose = render(
                old or exe,
                lock,
                folder,
                key + "-frozen",
                "resident",
                source,
                pages,
                frozen,
                route,
                0,
                rig,
            )
            result["traced_vs_same_pose_resident"] = comparison(baseline, same_pose)
            exact = render(
                old or exe,
                lock,
                folder,
                key + "-exact-frozen",
                "resident",
                exact_static,
                pages,
                frozen,
                route,
                0,
                rig,
            )
            result["traced_vs_same_pose_and_page"] = comparison(baseline, exact)
            require(
                baseline == exact,
                f"{key} changes the native rendering of the captured poses and fine geometry: {result}",
            )
        if old and place != "bir-hakeim":
            historical = render(
                old, lock, folder, key + "-previous", "resident", *args, rig
            )
            result["resident_vs_previous_resident"] = comparison(reference, historical)
            require(
                reference == historical,
                "shared actor construction changed the existing native mannequin",
            )
        if phase in ("first-contact", "hand"):
            ungrouped = render(
                exe, lock, folder, key + "-groups-off", "groups-off", *args, rig
            )
            result["traced_vs_groups_off"] = comparison(baseline, ungrouped)
            require(
                baseline == ungrouped,
                f"{key} differs through the explicit ungrouped entry",
            )
        if place == "bir-hakeim":
            streamed = render(
                exe, lock, folder, key + "-streamed", "streamed", *args, rig
            )
            area = read(folder / (key + "-streamed-paging.json"))
            fine = area["published"]["fine_tiles"]
            require(
                0 < fine < len(read(pages / "index.json")["tiles"]),
                "the city demand control read no fine pages or kept all",
            )
            require(
                area["first_contact"]["support"] == "rail"
                and area["first_contact"]["tick"] == tick,
                "on-demand terrain lost the rail impact",
            )
            result["streamed_city"] = {
                "fine_tiles": fine,
                "cold_file_bytes": area["storage"]["cold_file_bytes"],
                "first_contact": area["first_contact"],
                "vs_all_resident": comparison(streamed, baseline),
            }
        if part and rig:
            hidden = without_draw(folder, rig, part)
            control = motion(
                exe, folder, pages, initial, route, hidden, "without-" + part
            )
            require(
                rows == control["body_before_step"]
                and solved["actor"]["parts"] == control["actor"]["parts"],
                f"removing {part}'s visual changed a pose, contact or rigid body",
            )
            other = render(
                exe,
                lock,
                folder,
                key + "-without",
                "traced",
                *args,
                hidden,
                expected_draws=14,
            )
            camera_meshes = [
                read(folder / (key + "-" + tag + "-gpu.json"))["frames"][-1][
                    "render_world_camera_meshes"
                ]
                for tag in ("traced", "without")
            ]
            require(
                camera_meshes[0] == camera_meshes[1] + 1,
                "the removed actor draw still reached the camera",
            )
            result["removed_part"] = dict(
                part=part,
                camera_meshes_full_and_hidden=camera_meshes,
                full_image=comparison(baseline, other),
                **difference_near_body(
                    baseline, other, poses[names.index(part)], camera
                ),
            )
            require(
                result["removed_part"]["pixels_over_seven_inside"] >= 5,
                f"{key} did not reveal its own contacting drawable: {result['removed_part']}",
            )
            if place == "span" and phase == "arm":
                result["original_raised_slab"] = raised_control(
                    old or exe, lock, folder, route, frozen, rig, hidden
                )
        cases[phase] = result
        print(key, json.dumps(result), flush=True)
    return cases


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    parser.add_argument(
        "--paris-pages",
        type=Path,
        help="already cooked Bir-Hakeim pages with the same source hash",
    )
    parser.add_argument(
        "--case",
        choices=("span", "bir-hakeim", "ledge"),
        action="append",
        help="run a single location or repeat to choose several",
    )
    parser.add_argument(
        "--resident-reference",
        type=Path,
        help="previous native executable for a resident pixel control",
    )
    parser.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/paged-ragdoll/visual"
    )
    parser.add_argument(
        "--gpu-lock", type=Path, default=Path("/tmp/osage-ville-paris-gpu.lock")
    )
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    exe = args.bin.resolve()
    rig = rig_files(out)
    report = {
        "binary_sha256": hashlib.file_digest(exe.open("rb"), "sha256").hexdigest(),
        "viewport": SIZE,
    }
    old = args.resident_reference.resolve() if args.resident_reference else None
    if old:
        report["resident_reference_sha256"] = hashlib.file_digest(
            old.open("rb"), "sha256"
        ).hexdigest()
    for place in args.case or ("span", "bir-hakeim", "ledge"):
        folder = out / place
        source, pages, doc, initial = setup(folder, place, args.paris_pages)
        report[place] = {}
        for name, model in (("human", None), ("imported", rig)):
            report[place][name] = inspect_model(
                exe,
                args.gpu_lock,
                folder,
                name,
                source,
                pages,
                doc,
                initial,
                model,
                place,
                old,
            )
        for phase in report[place]["human"]:
            require(
                report[place]["human"][phase]["rgb_sha256"]
                != report[place]["imported"][phase]["rgb_sha256"],
                f"{place}/{phase} drew both models as the same shape",
            )
    (out / "summary.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
