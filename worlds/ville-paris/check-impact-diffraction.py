#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Measure the route, delay, ear direction and recorded sound around exposed edges."""

import argparse
import array
import copy
import json
import math
import runpy
import struct
import sys
import wave
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
common = runpy.run_path(str(HERE / "check-impact-transmission.py"))
SOURCE = (7.0, 1.0, 10.0)
EYE = (10.0, 4.0, 15.0)
RATE = 48000


def quad(l, r, n, f, y=2.5):
    return [(l, y, n), (r, y, n), (r, y, f), (l, y, f)]


def rim(quad):
    return list(zip(quad, quad[1:] + quad[:1], strict=True))


def best_bend(sides, eye):
    # Independent numerical minimization on each finite side.
    candidates = []
    for a, b in sides:

        def path(t, a=a, b=b):
            p = tuple(v + (w - v) * t for v, w in zip(a, b, strict=True))
            return math.dist(SOURCE, p) + math.dist(p, eye), p

        lo, hi = 0.0, 1.0
        for _ in range(70):
            u, v = (2 * lo + hi) / 3, (lo + 2 * hi) / 3
            if path(u)[0] < path(v)[0]:
                hi = v
            else:
                lo = u
        candidates.extend([path(0.0), path(1.0), path((lo + hi) * 0.5)])
    return min(candidates)


def aperture(x, y=2.5, z=12.5):
    return [
        quad(-100, x - 1, -100, 100, y),
        quad(x + 1, 100, -100, 100, y),
        quad(x - 1, x + 1, -100, z - 1, y),
        quad(x - 1, x + 1, z + 1, 100, y),
    ]


def room():
    top = quad(2, 14, 4, 18)
    base = [(x, -5.0, z) for x, _, z in top]
    return [top, base] + [
        [top[i], top[(i + 1) % 4], base[(i + 1) % 4], base[i]] for i in range(4)
    ]


def write_geometry(base, chunk, path, faces, energy, grouped, reverse):
    doc = copy.deepcopy(base)
    binary = bytearray(chunk[8 : 8 + doc["buffers"][0]["byteLength"]])
    profile = doc["asset"]["extras"]["osageObject"]
    profile["acousticMaterials"]["wall"] = {
        "absorption": [0] * 3,
        "transmission": energy,
        "scattering": 0,
    }
    root = doc["scenes"][doc["scene"]]["nodes"]
    root.append(len(doc["nodes"]))
    children = []
    doc["nodes"].append(
        {"children": children, "extras": {"osage": {"id": "room-screens"}}}
    )

    def accessor(values, shape, field, target):
        while len(binary) % 4:
            binary.append(0)
        count = len(values)
        raw = struct.pack(
            "<" + field * (count * len(values[0])), *(v for p in values for v in p)
        )
        view = len(doc["bufferViews"])
        doc["bufferViews"].append(
            {
                "buffer": 0,
                "byteOffset": len(binary),
                "byteLength": len(raw),
                "target": target,
            }
        )
        binary.extend(raw)
        a = {
            "bufferView": view,
            "componentType": 5126 if field == "f" else 5123,
            "count": count,
            "type": shape,
        }
        if shape == "VEC3":
            a["min"] = [min(c) for c in zip(*values, strict=True)]
            a["max"] = [max(c) for c in zip(*values, strict=True)]
        doc["accessors"].append(a)
        return len(doc["accessors"]) - 1

    groups = [faces] if grouped else [[face] for face in faces]
    for i, group in enumerate(groups):
        vertices = []
        indices = []
        for corners in group:
            offset = len(vertices)
            vertices.extend(corners)
            layout = [0, 1, 3, 1, 2, 3] if reverse else [0, 1, 2, 0, 2, 3]
            if len(corners) == 3:
                layout = [0, 1, 2]
            indices.extend((offset + k,) for k in layout)
        p = accessor(vertices, "VEC3", "f", 34962)
        ix = accessor(indices, "SCALAR", "H", 34963)
        mesh = len(doc["meshes"])
        doc["meshes"].append(
            {"primitives": [{"attributes": {"POSITION": p}, "indices": ix}]}
        )
        label = f"surface-{i}"
        children.append(len(doc["nodes"]))
        doc["nodes"].append(
            {
                "extras": {
                    "osage": {
                        "id": label,
                        "acoustic": [
                            {
                                "id": label,
                                "mesh": mesh,
                                "source": "authored",
                                "material": "wall",
                            }
                        ],
                    }
                }
            }
        )
    doc["buffers"][0]["byteLength"] = len(binary)
    binary.extend(b"\0" * (-len(binary) % 4))
    common["write_glb"](path, doc, struct.pack("<I4s", len(binary), b"BIN\0") + binary)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    parser.add_argument("--reference", type=Path, help="earlier city binary")
    parser.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/acoustic-edge"
    )
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    asset, replay = runpy.run_path(
        str(ROOT / "tools/object-convert/test_hierarchy.py")
    )["sample"](out)
    base, chunk = common["read_glb"](asset)
    cases = {}
    waves = {}

    def run(
        name,
        faces,
        energy=(0.0, 0.0, 0.0),
        grouped=False,
        reverse=False,
        speed=0.0,
        flipped=False,
        mode="headphones",
    ):
        scene = out / (name + ".glb")
        write_geometry(base, chunk, scene, faces, energy, grouped, reverse)
        route = json.loads(replay.read_text())
        keys = route["routes"][0]["camera"]
        for key in keys:
            x = EYE[0] + speed * (key["tick"] / route["tick_hz"] - 0.65)
            key["eye"] = [x, EYE[1], EYE[2]]
            key["aim"] = [x, EYE[1], EYE[2] + (1 if flipped else -1)]
        rpath = out / (name + "-route.json")
        rpath.write_text(json.dumps(route))
        options = [
            "--scene",
            scene,
            "--replay",
            rpath,
            "--physics",
            "jolt",
            "--actor",
            "capsule",
            "--route",
            "drop",
            "--listening",
            mode,
        ]
        report, pcm = common["render"](args.bin, out, name, options)
        direct, dry = common["render"](
            args.bin, out, name + "-direct", [*options, "--direct-only"]
        )
        (hit,) = report["onsets"]
        (old,) = direct["onsets"]
        for field in (
            "emission_frame",
            "arrival_frame",
            "direct_path",
            "point_m",
            "surface",
            "material",
            "part",
        ):
            assert hit[field] == old[field], field
        assert (
            hit["point_m"] == list(SOURCE)
            and hit["emission_frame"] == 31200
            and old["diffracted_path"] is None
        )
        edge = hit["diffracted_path"]
        if edge:
            assert 0 < max(map(abs, pcm))
            at = edge["arrival_frame"]
            assert pcm[: 2 * at] == dry[: 2 * at] and pcm[2 * at :] != dry[2 * at :]
            assert (
                abs((at - hit["emission_frame"]) - edge["distance_m"] / 343 * RATE)
                <= 0.501
            )
            assert abs(sum(edge["legs_m"]) - edge["distance_m"]) < 1e-9
            points = [
                SOURCE,
                *(b["point_m"] for b in edge["bends"]),
                edge["listener_at_arrival_m"],
            ]
            for length, a, b in zip(
                edge["legs_m"], points[:-1], points[1:], strict=True
            ):
                assert abs(length - math.dist(a, b)) < 1e-9
            straight = math.dist(points[0], points[-1])
            assert abs(straight - edge["reference_distance_m"]) < 1e-9
            excess = sum(edge["legs_m"]) - straight
            separation = math.dist(*points[1:-1]) if len(points) == 4 else 0
            for i, hz in enumerate((250, 1000, 4000)):
                wavelength = 343 / hz
                corr = 1
                if separation > 0.3:
                    k = (5 * wavelength / separation) ** 2
                    corr = (1 + k) / (1 / 3 + k)
                loss = 10 * math.log10(3 + 40 * excess * corr / wavelength)
                assert abs(loss - edge["edge_loss_db"][i]) < 1e-8
                available = (
                    1 - hit["direct_path"]["transmission_energy"][i]
                ) * math.prod(leg[i] for leg in edge["leg_transmission_energy"])
                assert (
                    abs(available * 10 ** (-loss / 10) - edge["received_energy"][i])
                    < 1e-10
                )
            if max(map(abs, dry)) == 0:
                assert (
                    hit["first_path_frame"] == at
                    and at <= report["first_nonzero_pcm_frame"] < at + 100
                )
            else:
                assert hit["first_path_frame"] == hit["arrival_frame"]
                first_pcm = report["first_nonzero_pcm_frame"]
                direct_pcm = direct["first_nonzero_pcm_frame"]
                if direct_pcm < at:
                    assert first_pcm == direct_pcm
                else:
                    assert at <= first_pcm < at + 240
        else:
            assert pcm == dry
        if args.reference and name in ("aligned", "displaced", "enclosed"):
            _, prior = common["render"](
                args.reference, out, name + "-previous", options
            )
            assert prior == dry
        cases[name] = {
            "direct_frame": hit["arrival_frame"],
            "first_path": hit["first_path_frame"],
            "first_pcm": report["first_nonzero_pcm_frame"],
            "direct_peak": max(map(abs, dry)),
            "peak": max(map(abs, pcm)),
            "edge": edge,
            "sha256": common["digest"](out / (name + ".wav")),
        }
        waves[name] = pcm
        return edge

    assert run("aligned", aperture(8.5)) is None
    displaced = run("displaced", aperture(12.5))
    assert displaced["owner_path"][0] == "room-screens"
    assert run("enclosed", room()) is None and cases["enclosed"]["peak"] == 0
    assert (
        run("enclosed-one-mesh", room(), grouped=True) is None
        and cases["enclosed-one-mesh"]["peak"] == 0
    )
    assert run("transparent", aperture(12.5), energy=[1] * 3) is None
    independent, point = best_bend(rim(quad(11.5, 13.5, 11.5, 13.5)), EYE)
    assert (
        abs(displaced["distance_m"] - independent) < 1e-6
        and math.dist(displaced["point_m"], point) < 1e-5
    )
    for label, grouped, reverse in (
        ("other-diagonal", False, True),
        ("one-mesh", True, False),
        ("one-mesh-flipped", True, True),
    ):
        edge = run(label, aperture(12.5), grouped=grouped, reverse=reverse)
        assert (
            abs(edge["distance_m"] - independent) < 1e-6
            and waves[label] == waves["displaced"]
        )
    center = quad(7.5, 9.5, 11.5, 13.5)
    finite = run("finite", [center])
    assert abs(finite["distance_m"] - best_bend(rim(center), EYE)[0]) < 1e-6
    assert run("through-and-around", [center], energy=(0.04, 0.0025, 0.0001))
    mirror = run("turned", aperture(12.5), flipped=True)
    assert mirror["point_m"] == displaced["point_m"]
    assert (
        waves["turned"][0::2] == waves["displaced"][1::2]
        and waves["turned"][1::2] == waves["displaced"][0::2]
    )
    speakers = run("speakers", aperture(12.5), mode="speakers")
    assert speakers["arrival_frame"] == displaced["arrival_frame"] and speakers[
        "ear_delay_frames"
    ] == [0.0, 0.0]
    for speed in (-12.0, 12.0):
        edge = run("moving-" + str(int(speed)), aperture(12.5), speed=speed)

        def length(t, speed=speed):
            return best_bend(
                rim(quad(11.5, 13.5, 11.5, 13.5)), (EYE[0] + speed * t, EYE[1], EYE[2])
            )[0]

        lo, hi = 0.0, 0.1
        for _ in range(42):
            t = (lo + hi) / 2
            if length(t) > 343 * t:
                lo = t
            else:
                hi = t
        assert abs(edge["arrival_frame"] - 31200 - round(hi * RATE)) <= 1
        assert (
            math.dist(
                edge["listener_at_arrival_m"], [EYE[0] + speed * hi, EYE[1], EYE[2]]
            )
            < 2e-5
        )

    # For these parallel cuts, project to XY and unfold each constrained leg.
    # A straight line along Z in the unfolded plane minimizes the full 3D sum.
    def two_screens(a, b):
        return aperture(a, 2, 35 / 3) + aperture(b, 3, 40 / 3)

    def exact_pair(ax, bx, eye):
        xy = (
            math.hypot(ax - SOURCE[0], 1),
            math.hypot(bx - ax, 1),
            math.hypot(eye[0] - bx, 1),
        )
        length = math.hypot(sum(xy), eye[2] - SOURCE[2])

        def z(d):
            return SOURCE[2] + (eye[2] - SOURCE[2]) * d / sum(xy)

        return length, [(ax, 2, z(xy[0])), (bx, 3, z(xy[0] + xy[1]))]

    assert run("two-aligned-openings", two_screens(8, 9)) is None
    for name, a, b, ax, bx in (
        ("first-offset", 4, 9, 5, 8),
        ("second-offset", 8, 13, 9, 12),
        ("two-opposed-openings", 4, 13, 5, 12),
    ):
        length, points = exact_pair(ax, bx, EYE)
        edge = run(name, two_screens(a, b))
        assert len(edge["bends"]) == 2 and len(edge["legs_m"]) == 3, (name, edge)
        assert abs(edge["distance_m"] - length) < 1e-6, (
            name,
            edge["distance_m"],
            length,
        )
        assert all(
            math.dist(p, bend["point_m"]) < 1e-5
            for p, bend in zip(points, edge["bends"], strict=True)
        )
        assert edge["arrival_frame"] == 31200 + round(length * RATE / 343)
    # Rebinding or changing a triangle diagonal must preserve the wave.
    run("two-opposed-one-mesh", two_screens(4, 13), grouped=True, reverse=True)
    assert waves["two-opposed-one-mesh"] == waves["two-opposed-openings"]
    # Retarded time for a moving receiver behind both openings.
    speed = -12
    edge = run("two-opposed-moving", two_screens(4, 13), speed=speed)
    lo, hi = 0.0, 0.1
    for _ in range(48):
        t = (lo + hi) / 2
        length, _ = exact_pair(5, 12, (EYE[0] + speed * t, EYE[1], EYE[2]))
        if length > 343 * t:
            lo = t
        else:
            hi = t
    assert abs(edge["arrival_frame"] - 31200 - round(hi * RATE)) <= 1
    assert len(edge["bends"]) == 2
    assert run("two-opposed-enclosed", room() + two_screens(4, 13)) is None
    assert cases["two-opposed-enclosed"]["peak"] == 0

    for filename, labels in (
        ("aligned-then-displaced", ("aligned", "displaced")),
        ("two-aligned-then-opposed", ("two-aligned-openings", "two-opposed-openings")),
    ):
        with wave.open(str(out / (filename + ".wav")), "wb") as wav:
            wav.setparams((2, 2, RATE, 0, "NONE", "not compressed"))
            for label in labels:
                scaled = array.array("h", (8 * x for x in waves[label]))
                if sys.byteorder != "little":
                    scaled.byteswap()
                wav.writeframes(scaled.tobytes())
    summary = {
        "binary_sha256": common["digest"](args.bin),
        "reference_sha256": common["digest"](args.reference)
        if args.reference
        else None,
        "independent_displaced_length_m": independent,
        "cases": cases,
    }
    (out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(
        f"{len(cases)} routes, displaced path {independent:.6f} m, first PCM {cases['displaced']['first_pcm']}, peak {cases['displaced']['peak']}"
    )


if __name__ == "__main__":
    main()
