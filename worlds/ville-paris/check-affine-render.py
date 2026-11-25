#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Compare native one-sided draws under full affine model-tree transforms with precomputed world-space triangles."""

import argparse
import copy
import hashlib
import json
import math
import runpy
import subprocess
from pathlib import Path

from paging_checks import require

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
COOK = runpy.run_path(str(HERE / "cook-pages.py"))
W, H = 360, 192
UNIT = ((1, 0, 0, 0), (0, 1, 0, 0), (0, 0, 1, 0), (0, 0, 0, 1))
MAT = [
    {
        "pbrMetallicRoughness": {
            "baseColorFactor": [0.74, 0.12, 0.04, 1],
            "metallicFactor": 0.0,
            "roughnessFactor": 0.76,
        },
        "doubleSided": False,
    }
]
POS = [(-1.0, 0.0, 0.875), (0.75, 0.0, 1.0), (1.0, 0.0, -0.625), (-0.625, 0.0, -1.0)]
IX = [0, 1, 2, 0, 2, 3]
COLOR = COOK["Channel"](
    "VEC4",
    5121,
    [
        (255, 255, 255, 255),
        (160, 190, 210, 255),
        (250, 210, 120, 255),
        (140, 220, 170, 255),
    ],
)
SHAPE = COOK["Part"](0, POS, [(0.0, 1.0, 0.0)] * len(POS), IX, {"COLOR_0": COLOR})


def mul(a, b):
    return tuple(
        tuple(
            sum(x * y for x, y in zip(row, col, strict=True))
            for col in zip(*b, strict=True)
        )
        for row in a
    )


def frame(x, linear):
    return tuple(tuple(row) + (x if i == 0 else 0,) for i, row in enumerate(linear)) + (
        UNIT[3],
    )


def scale(x, y, z):
    return ((x, 0, 0), (0, y, 0), (0, 0, z))


def transform(a, point, vector=False):
    v = (*point, float(not vector))
    return tuple(sum(x * y for x, y in zip(row, v, strict=True)) for row in a[:3])


def gltf(a):
    return [x for col in zip(*a, strict=True) for x in col]


def cross(a, b):
    return (
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    )


def dot(a, b):
    return sum(x * y for x, y in zip(a, b, strict=True))


def unit(v):
    n = math.hypot(*v)
    return tuple(x / n for x in v)


def baked(pose, fault):
    # The glTF stores local +Y artist normals. Use the transformed tangent plane in world
    # space as an independent geometric oracle for that normal, including its hemisphere.
    u, v, w = (
        transform(pose, axis, True) for axis in ((1, 0, 0), (0, 1, 0), (0, 0, 1))
    )
    normal = unit(cross(w, u))
    negative = dot(v, normal) < 0
    if negative:
        normal = tuple(-c for c in normal)
    if fault == "normals":
        normal = unit(v)
    ix = [0, 2, 1, 0, 3, 2] if negative and fault != "winding" else IX
    return SHAPE._replace(
        positions=[transform(pose, p) for p in POS], normals=[normal] * 4, indices=ix
    )


def placed(out, one_cell):
    # Row-major values here, written column-major only at the glTF boundary.
    shear = ((-1.25, 0.75, 0.1875), (0.3125, 1.0, 0.25), (0.25, -0.375, 1.25))
    cs = math.cos(math.pi / 4)
    sn = math.sin(math.pi / 4)
    poses = [
        (
            "rotated-child",
            frame(9, scale(-1.5, 0.625, 1.25)),
            frame(0, ((cs, -sn, 0), (sn, cs, 0), (0, 0, 1))),
        ),
        ("regular", frame(-6, scale(1, 1, 1)), UNIT),
        ("mirror-x", frame(-3, scale(-1.25, 1, 0.875)), UNIT),
        ("shear", frame(0, shear), frame(0, ((1, 0, -0.25), (0, 1, 0), (0.25, 0, 1)))),
        ("double-mirror", frame(3, scale(-1, 1, 1)), frame(0, scale(-1.25, 1, 0.875))),
        ("mirror-y", frame(6, scale(1, -1, 1)), UNIT),
        (
            "clipped",
            frame(70, ((-27, 0.75, 0.125), (0.125, 1.0, 0.125), (0.0625, -0.5, 4.0))),
            UNIT,
        ),
    ]
    if not one_cell:
        poses.append(("offscreen", frame(256, scale(1, 1, 1)), UNIT))
    result = {}
    for kind in ("source", "reference", "winding", "normals"):
        g = COOK["Glb"](copy.deepcopy(MAT))
        g.doc["asset"]["extras"] = {
            "osageObject": {
                "version": 1,
                "id": "panels",
                "collisionMaterials": {"solid": {"friction": 0.5, "restitution": 0.0}},
                "acousticMaterials": {},
            }
        }
        owners = []
        for label, parent, child in poses:
            key = "shared" if kind == "source" else label
            # Keep both the source-owner tree and the independently baked reference hierarchy.
            parent_id = len(g.doc["nodes"])
            g.doc["nodes"].append(
                {
                    "matrix": gltf(parent if kind == "source" else UNIT),
                    "children": [parent_id + 1],
                    "extras": {"osage": {"id": "parent/" + label}},
                }
            )
            part = SHAPE if kind == "source" else baked(mul(parent, child), kind)
            g.node(
                key,
                [part],
                {
                    "matrix": gltf(child if kind == "source" else UNIT),
                    "extras": {"osage": {"id": label}},
                },
            )
            n = g.doc["nodes"][-1]
            if kind == "source" and label == "rotated-child":
                # Both locals are glTF TRS. A rotation below anisotropic scale makes this world basis nonorthogonal.
                parent_node = g.doc["nodes"][parent_id]
                del parent_node["matrix"], n["matrix"]
                parent_node.update(translation=[9, 0, 0], scale=[-1.5, 0.625, 1.25])
                n["rotation"] = [0, 0, math.sin(math.pi / 8), math.cos(math.pi / 8)]
            n["extras"]["osage"]["collision"] = [
                {"id": label + "-floor", "mesh": n["mesh"], "material": "solid"}
            ]
            g.doc["scenes"][0]["nodes"][-1] = parent_id
            owners.append(parent_id + 1)
        path = out / (kind + ".glb")
        path.write_bytes(g.bytes())
        result[kind] = path
    routes = []
    for label, x, h, z in (
        ("above", 0, 13, 6),
        ("below", 0, -13, 6),
        ("seam", 67, 16, 8),
        ("seam-below", 67, -16, 8),
    ):
        camera = {"eye": [x, h, z], "aim": [x, 0, 0]}
        routes.append(
            {
                "id": label,
                "ticks": 8,
                "start_m": [350, 5, 350],
                "velocity_m_s": [0, 0, 0],
                "expected_support": "regular-floor",
                "camera": [dict(camera, tick=t) for t in (0, 8)],
            }
        )
    replay = out / "cameras.json"
    replay.write_text(
        json.dumps(
            {
                "version": 1,
                "tick_hz": 60,
                "gravity_m_s2": 9.81,
                "terminal_m_s": 55,
                "viewport": [W, H],
                "vertical_fov_deg": 45,
                "routes": routes,
            }
        )
    )
    pages = out / "pages"
    COOK["cook"](result["source"], pages, 32)
    idx = json.loads((pages / "index.json").read_bytes())
    require(
        all(idx["source_parents"][owner] == owner - 1 for owner in owners),
        "cooked spatial cells lost the parent of a visible primitive",
    )
    return result, pages, replay, owners


def stats(a, b):
    pixels = sum(a[i : i + 3] != b[i : i + 3] for i in range(0, len(a), 3))
    return {
        "different_pixels": pixels,
        "max_channel_delta": max(abs(x - y) for x, y in zip(a, b, strict=True)),
        "delta_over_2": sum(
            max(abs(a[i + k] - b[i + k]) for k in range(3)) > 2
            for i in range(0, len(a), 3)
        ),
    }


def run(args):
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    source, pages, replay, owners = placed(out, args.one_cell)
    exe = args.bin.resolve()
    cases = {}
    traces = {}

    def render(route, name):
        label = route + "-" + name
        png = out / (label + ".png")
        if name in source:
            cmd = [
                exe,
                "--scene",
                source[name],
                "--route",
                route,
                "--replay",
                replay,
                "--actor",
                "capsule",
                "--physics",
                "rapier",
                "--full-detail",
                "--tick",
                0,
                "--shot",
                png,
            ]
        else:
            receipt = out / (label + ".json")
            cmd = [
                exe,
                "--paging",
                "--pages",
                pages,
                "--route",
                route,
                "--replay",
                replay,
                "--tick",
                0,
                "--gpu-groups",
                "off",
                "--out",
                receipt,
                "--shot",
                png,
            ]
            if name != "coarse":
                cmd.append("--all-resident")
            if name != "capture":
                cmd += ["--gpu-trace", out / (label + "-gpu.json"), "--gpu-stride", 1]
            if name == "coarse":
                cmd += ["--delay-ms", 400]
        with (out / (label + ".log")).open("w") as log:
            proc = subprocess.run(
                ["flock", args.gpu_lock, *map(str, cmd)],
                check=False,
                stdout=log,
                stderr=subprocess.STDOUT,
                timeout=150,
            )
        if proc.returncode:
            error = (out / (label + ".log")).read_text().splitlines()[-1]
            raise RuntimeError(f"{label}: {error}")
        if name not in source:
            traces[label] = json.loads(receipt.read_bytes())["published"]
            if name != "capture":
                f = json.loads((out / (label + "-gpu.json")).read_bytes())["frames"][-1]
                traces[label].update(
                    gpu_geometry_assets=f["geometry_assets"],
                    prepared_geometry=f["prepared_assets"],
                    shared_gpu_primitives=f["shared_primitives"],
                    shared_gpu_vertex_bytes=f["shared_vertex_slice_bytes"],
                    shared_gpu_index_bytes=f["shared_index_slice_bytes"],
                )
        return subprocess.check_output(
            [
                "ffmpeg",
                "-loglevel",
                "error",
                "-i",
                png,
                "-threads",
                "1",
                "-pix_fmt",
                "rgb24",
                "-f",
                "rawvideo",
                "-",
            ]
        )

    for route in ("above", "below", "seam", "seam-below"):
        base = render(route, "reference")
        n = sum(
            base[i] - base[i + 1] > 15 and base[i] - base[i + 2] > 16
            for i in range(0, len(base), 3)
        )
        row = {
            "reference_red_pixels": n,
            "reference_sha256": hashlib.sha256(base).hexdigest(),
        }
        for name in ("source", "live", "capture", "coarse", "winding", "normals"):
            pixels = render(route, name)
            row[name] = stats(base, pixels)
        cases[route] = row
        print(route, json.dumps(row), flush=True)
    result = {
        "binary_sha256": hashlib.file_digest(exe.open("rb"), "sha256").hexdigest(),
        "owners": owners,
        "cases": cases,
        "pages": traces,
    }
    (out / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
    if not args.observe:
        for r, row in cases.items():
            for renderer in ("source", "live", "capture", "coarse"):
                require(
                    row[renderer]["max_channel_delta"] <= 1,
                    f"{r}/{renderer} winding, lighting or pose changed: {row[renderer]}",
                )
        require(
            cases["above"]["reference_red_pixels"] > 200
            and cases["below"]["reference_red_pixels"] > 40
            and cases["seam"]["reference_red_pixels"] > 1000,
            "front/back screens do not exercise visible material",
        )
        require(
            cases["above"]["winding"]["delta_over_2"] > 100
            and cases["below"]["winding"]["delta_over_2"] > 100,
            "the wrong-winding control did not detect a difference",
        )
        require(
            cases["above"]["normals"]["delta_over_2"] > 30,
            "the incorrect normal matrix went undetected",
        )
        require(
            traces["above-live"]["shared_gpu_primitives"] == (0 if args.one_cell else 1)
            and traces["above-live"]["shared_gpu_index_bytes"]
            == (0 if args.one_cell else 12)
            and traces["above-live"]["shared_gpu_vertex_bytes"]
            == (0 if args.one_cell else 160),
            "GPU geometry duplicated across positive and negative placements",
        )
        require(
            traces["above-live"]["shared_model_instances"]
            == (0 if args.one_cell else 7)
            and traces["above-live"]["distinct_shared_resource_meshes"]
            == (0 if args.one_cell else 1)
            and traces["above-coarse"]["fine_tiles"] == 0,
            "the shared or coarse control did not run",
        )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    parser.add_argument(
        "--out",
        type=Path,
        default=ROOT / "bench/out/ville-paris/mirrored-visual/qualified",
    )
    parser.add_argument(
        "--gpu-lock", type=Path, default=Path("/tmp/osage-ville-paris-gpu.lock")
    )
    parser.add_argument(
        "--observe",
        action="store_true",
        help="record comparisons without requiring correct pixels",
    )
    parser.add_argument(
        "--one-cell",
        action="store_true",
        help="keep the small placements in one cell to exercise local GLB meshes",
    )
    args = parser.parse_args()
    run(args)


if __name__ == "__main__":
    main()
