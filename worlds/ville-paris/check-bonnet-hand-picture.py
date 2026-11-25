#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Compare hand positions and native screenshots for different Bir-Hakeim terrain catalogues."""

import argparse
import json
import math
import runpy
import subprocess
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
BENCH = runpy.run_path(str(HERE / "check-paged-bonnet-render.py"))
TICK = 80
MODES = ("resident", "all", "streamed")
HANDS = {"parked": "hand_l", "moving": "hand_r"}


def project(point, camera, viewport, fov):
    eye, aim = camera["eye"], camera["aim"]
    forward = BENCH["unit"]([b - a for a, b in zip(eye, aim, strict=True)])
    right = BENCH["unit"](BENCH["cross"](forward, [0, 1, 0]))
    up = BENCH["cross"](right, forward)
    ray = [v - e for v, e in zip(point, eye, strict=True)]
    depth = BENCH["dot"](ray, forward)
    assert depth > 0
    focal = viewport[1] / 2 / math.tan(math.radians(fov / 2))
    return [
        viewport[0] / 2 + focal * BENCH["dot"](ray, right) / depth,
        viewport[1] / 2 - focal * BENCH["dot"](ray, up) / depth,
    ]


def read_route(folder, route, cfg):
    label = f"{route}-{TICK:03}"
    box, actor = BENCH["stored_states"](folder / "resident-states/dynamics.csv")[
        route, TICK
    ]
    frame = BENCH["read"](folder / (label + "-all-gpu.json"))["frames"][-1]
    assert frame["tick"] == TICK and frame["actor_draw_primitives"] == 15
    camera = {"eye": frame["camera_eye"], "aim": frame["camera_aim"]}
    records = {
        mode: BENCH["read"](folder / (label + "-" + mode + ".json"))
        for mode in ("all", "streamed")
    }
    names = records["all"]["actor"]["parts"]
    hands = {
        p["name"]: i for i, p in enumerate(names) if p["name"] in ("hand_l", "hand_r")
    }
    assert len(hands) == 2
    result = {}
    extents = next(r for r in cfg["routes"] if r["id"] == route)["bonnet"][
        "half_extents_m"
    ]
    references = BENCH["box_projection"](
        box, extents, camera, cfg["viewport"], cfg["vertical_fov_deg"]
    )
    for mode in MODES:
        natural = mode == "resident"
        row = (
            {"boxes_after_step": [box], "parts_after_step": actor}
            if natural
            else records[mode]["body_before_step"][TICK]
        )
        state = BENCH["mode_motion"](box, actor, row, names)
        observations = {}
        for name, index in hands.items():
            center = row["parts_after_step"][index]["center"]
            origin = actor[index]["center"]
            point = project(center, camera, cfg["viewport"], cfg["vertical_fov_deg"])
            zero = project(origin, camera, cfg["viewport"], cfg["vertical_fov_deg"])
            observations[name] = {
                "center_m": center,
                "screen_center_px": point,
                "distance_from_resident_m": math.dist(center, origin),
                "screen_distance_from_resident_px": math.dist(point, zero),
            }
        path = folder / (label + "-" + mode + ".png")
        corners = BENCH["box_projection"](
            row["boxes_after_step"][0],
            extents,
            camera,
            cfg["viewport"],
            cfg["vertical_fov_deg"],
        )
        result[mode] = {
            "box_hull_px": BENCH["hull"](corners),
            "largest_box_corner_shift_px": max(
                math.dist(a, b) for a, b in zip(corners, references, strict=True)
            ),
            "state": state,
            "hands": observations,
            "image": path.name,
            "png_sha256": BENCH["digest"](path),
        }
        if not natural:
            rec = records[mode]
            result[mode].update(
                fine_tiles=rec["published"]["fine_tiles"],
                coarse_instances=rec["published"]["coarse_instances"],
                physical_reads=rec["physics"]["reads"],
                visual_reads=rec["visual"]["reads"],
                cold_read_bytes=rec["storage"]["cold_file_bytes"],
            )
    return {"camera": camera, "selected_hand": HANDS[route], "modes": result}


def french(value, places):
    return f"{value:.{places}f}".replace(".", ",")


def draw(folder, route, values, prior, viewport, suffix="hands"):
    width, height = viewport
    selected = values["selected_hand"]
    hands = [values["modes"][m]["hands"][selected] for m in MODES]
    coords = [v["screen_center_px"] for v in hands]
    xs, ys = zip(*coords, strict=True)
    # One crop in the original RGB coordinate system, including both extreme poses.
    crop_w = 2 * math.ceil(
        max(188.0, max(xs) - min(xs) + 80.0, 2 * (max(ys) - min(ys) + 66.0)) / 2
    )
    crop_h = crop_w // 2
    left = max(0, min(width - crop_w, round((min(xs) + max(xs) - crop_w) / 2)))
    top = max(0, min(height - crop_h, round((min(ys) + max(ys) - crop_h) / 2)))
    assert all(left <= x < left + crop_w and top <= y < top + crop_h for x, y in coords)
    values["source_crop_px"] = [left, top, crop_w, crop_h]
    columns = 480
    gutter = 18
    total_width = 3 * columns + 4 * gutter
    image_y = 119
    crop_y = 421
    height_out = 765
    paths = [folder / values["modes"][m]["image"] for m in MODES]
    native_reference = BENCH["rgb"](paths[0])
    crop_indices = [
        y * width + x
        for y in range(top, top + crop_h)
        for x in range(left, left + crop_w)
    ]
    ref_area = set(
        BENCH["inside_polygon"](values["modes"]["resident"]["box_hull_px"], viewport)
    )
    for mode, path in zip(MODES, paths, strict=True):
        pixels = native_reference if mode == "resident" else BENCH["rgb"](path)
        result = values["modes"][mode]
        area = set(BENCH["inside_polygon"](result["box_hull_px"], viewport))
        projected = ref_area | area
        blue = BENCH["visible_blue"](native_reference, projected) | BENCH[
            "visible_blue"
        ](pixels, projected)
        result["bonnet"] = {
            "projected_pixels": len(area),
            "coverage_symmetric_difference": len(ref_area ^ area),
            "projected_rgb": BENCH["compare"](native_reference, pixels, projected),
            "blue_rgb": BENCH["compare"](native_reference, pixels, blue),
        }
        values["modes"][mode]["crop_rgb_difference"] = BENCH["compare"](
            native_reference, pixels, crop_indices
        )
        values["modes"][mode]["full_rgb_difference"] = BENCH["compare"](
            native_reference, pixels, range(width * height)
        )
    font = subprocess.check_output(
        ["fc-match", "sans-serif", "-f", "%{file}"], text=True
    )
    bold = subprocess.check_output(
        ["fc-match", "sans-serif:style=Bold", "-f", "%{file}"], text=True
    )
    cmd = [
        "magick",
        "-respect-parentheses",
        "-size",
        f"{total_width}x{height_out}",
        "xc:#152128",
    ]

    def text(x, y, label, size=19, emphasis=False, color="#e9ece8"):
        cmd.extend(
            [
                "-gravity",
                "NorthWest",
                "-stroke",
                "none",
                "-strokewidth",
                "0",
                "-fill",
                color,
                "-font",
                bold if emphasis else font,
                "-pointsize",
                str(size),
                "-annotate",
                f"+{int(x)}+{int(y)}",
                label,
            ]
        )

    side = "gauche" if selected == "hand_l" else "droite"
    parked = route == "parked"
    text(gutter, 9, "Bir-Hakeim · capot " + ("garé" if parked else "mobile"), 32, True)
    text(
        gutter,
        54,
        f"Tick 80 à 60 Hz   |   Main {side}   |   Cercle clair pour le centre de la main, croix jaune pour le centre résident.",
        18,
        color="#b8c8cf",
    )
    text(gutter, 394, "Même recadrage des trois prises", 17, color="#b8c8cf")
    for j, (mode, hand, path) in enumerate(zip(MODES, hands, paths, strict=True)):
        x = gutter + (columns + gutter) * j
        text(
            x,
            86,
            ("Résident", "Quartier entièrement chargé", "Quartier chargé à la demande")[
                j
            ],
            23,
            True,
        )
        cmd.extend(
            [
                "(",
                str(path),
                "-resize",
                f"{columns}x270!",
                ")",
                "-gravity",
                "NorthWest",
                "-geometry",
                f"+{x}+{image_y}",
                "-composite",
            ]
        )
        # The reference marker is an open cross, keeping the actual contact pixels visible.
        ref_x = (coords[0][0] - left) * columns / crop_w
        ref_y = (coords[0][1] - top) * columns / crop_w
        hand_x = (coords[j][0] - left) * columns / crop_w
        hand_y = (coords[j][1] - top) * columns / crop_w
        reference = f"line {ref_x - 7:.2f},{ref_y:.2f} {ref_x + 7:.2f},{ref_y:.2f} line {ref_x:.2f},{ref_y - 7:.2f} {ref_x:.2f},{ref_y + 7:.2f}"
        target = f"circle {hand_x:.2f},{hand_y:.2f} {hand_x + 13:.2f},{hand_y:.2f}"
        cmd.extend(
            [
                "(",
                str(path),
                "-crop",
                f"{crop_w}x{crop_h}+{left}+{top}",
                "+repage",
                "-resize",
                f"{columns}x240!",
                "-fill",
                "none",
                "-stroke",
                "#12181f",
                "-strokewidth",
                "4",
                "-draw",
                reference,
                "-draw",
                target,
                "-stroke",
                "#ffce56",
                "-strokewidth",
                "2",
                "-draw",
                reference,
                "-stroke",
                "#d9faff",
                "-draw",
                target,
                ")",
                "-gravity",
                "NorthWest",
                "-geometry",
                f"+{x}+{crop_y}",
                "-composite",
            ]
        )
        cm = french(100 * hand["distance_from_resident_m"], 2)
        px = french(hand["screen_distance_from_resident_px"], 1)
        text(x, 675, f"Écart 3D {cm} cm   |   Image {px} px", 21, True)
        if prior:
            old = prior["modes"][mode]["hands"][selected]["distance_from_resident_m"]
            text(
                x,
                704,
                f"Catalogue précédent   {french(old * 100, 2)} cm",
                16,
                color="#b8c8cf",
            )
        sheet = values["modes"][mode]["bonnet"]
        d = sheet["blue_rgb"]["over_two"]
        area = sheet["projected_pixels"]
        text(
            x,
            733,
            f"Emprise de la tôle {area:,} px   |   {d:,} px bleus avec Δ > 2".replace(
                ",", " "
            ),
            15,
            color="#b8c8cf",
        )
    out = folder / (route + "-" + suffix + ".png")
    cmd.extend(["-alpha", "off", "PNG24:" + str(out)])
    subprocess.run(cmd, check=True)
    values["comparison_png"] = out.name
    values["comparison_sha256"] = BENCH["digest"](out)


def analyze(exe, pages, out, prior):
    cfg = BENCH["read"](HERE / "fixtures/bonnet.json")
    scene = HERE / "fixtures/bir-hakeim.glb"
    idx = BENCH["read"](pages / "index.json")
    assert idx["source_sha256"] == BENCH["digest"](scene)
    result = {
        "binary_sha256": BENCH["digest"](exe),
        "replay_sha256": BENCH["digest"](HERE / "fixtures/bonnet.json"),
        "scene_sha256": idx["source_sha256"],
        "page_index_sha256": BENCH["digest"](pages / "index.json"),
        "viewport": cfg["viewport"],
        "tick": TICK,
        "routes": {},
    }
    for route in HANDS:
        current = read_route(out, route, cfg)
        old = read_route(prior, route, cfg) if prior else None
        if old:
            assert all(current["camera"][k] == old["camera"][k] for k in ("eye", "aim"))
            current["resident_png_identical_to_previous"] = (
                current["modes"]["resident"]["png_sha256"]
                == old["modes"]["resident"]["png_sha256"]
            )
            current["previous_catalogue_hand_errors_m"] = {
                mode: {
                    name: v["distance_from_resident_m"]
                    for name, v in data["hands"].items()
                }
                for mode, data in old["modes"].items()
            }
        draw(out, route, current, old, cfg["viewport"])
        selected = current["selected_hand"]
        errors = current["modes"]["streamed"]["hands"]
        worst = max(errors, key=lambda name: errors[name]["distance_from_resident_m"])
        if (
            worst != selected
            and errors[worst]["distance_from_resident_m"]
            > errors[selected]["distance_from_resident_m"] + 0.001
        ):
            alternate = {
                **current,
                "selected_hand": worst,
                "modes": {k: dict(v) for k, v in current["modes"].items()},
            }
            draw(out, route, alternate, old, cfg["viewport"], "largest-hand-error")
            current["largest_error_detail"] = {
                k: alternate[k]
                for k in (
                    "selected_hand",
                    "comparison_png",
                    "comparison_sha256",
                    "source_crop_px",
                )
            }
        result["routes"][route] = current
    path = out / "hand-comparison.json"
    path.write_text(json.dumps(result, indent=2) + "\n")
    print(path)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    parser.add_argument("--pages", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument(
        "--previous",
        type=Path,
        help="Native captures and pose receipts from the previous catalogue, using the same inputs.",
    )
    parser.add_argument(
        "--gpu-lock", type=Path, default=Path("/tmp/osage-ville-paris-gpu.lock")
    )
    a = parser.parse_args()
    exe = a.bin.resolve()
    out = a.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    pages = a.pages.resolve()
    scene = HERE / "fixtures/bir-hakeim.glb"
    replay = HERE / "fixtures/bonnet.json"
    BENCH["resident_states"](exe, out, scene, replay)
    for route in HANDS:
        for mode in MODES:
            BENCH["capture"](
                exe,
                a.gpu_lock,
                out,
                f"{route}-{TICK:03}-{mode}",
                mode,
                scene,
                pages,
                replay,
                route,
                TICK,
            )
    analyze(exe, pages, out, a.previous.resolve() if a.previous else None)


if __name__ == "__main__":
    main()
