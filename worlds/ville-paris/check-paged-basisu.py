#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Check compressed and core images on independent HTTP requests through the actual district renderer."""

import argparse
import copy
import hashlib
import json
import math
import os
import runpy
import shutil
import subprocess
import tempfile
import warnings
from pathlib import Path

HERE = Path(__file__).resolve().parent
HELPERS = runpy.run_path(str(HERE / "check-paged-lods.py"))
ORIGIN = runpy.run_path(str(HERE / "check-http.py"))["origin"]
COOK = HELPERS["COOK"]["cook"]
WRITE = HELPERS["write"]
READ = HELPERS["read"]
EDIT = HELPERS["with_document"]
POSITIONS = [13.15, 15.05, 16.95, 18.85]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def arrange(doc):
    root = doc["scenes"][doc["scene"]]["nodes"]
    assert len(root) == 1
    node = doc["nodes"][root[0]]
    assert not node.get("children") and not set(node).intersection(
        {"matrix", "rotation", "scale", "translation"}
    )
    nodes = []
    for i, x in enumerate(POSITIONS):
        placed = copy.deepcopy(node)
        placed["translation"] = [x, 0, 0]
        placed["extras"]["osage"]["id"] += f"-{i}"
        nodes.append(placed)
    doc["nodes"] = nodes
    root[:] = list(range(len(nodes)))


def capture(exe, out, label, url, records, cache, route, disabled):
    start = len(records)
    path = out / label
    gpu = path.with_suffix(".gpu.json")
    report = path.with_suffix(".json")
    image = path.with_suffix(".png")
    env = os.environ.copy()
    env["OSAGE_DISABLE_BC"] = str(int(disabled))
    command = [
        "flock",
        "-w",
        "450",
        "/tmp/osage-ville-paris-gpu.lock",
        exe,
        "--paging",
        "--pages",
        url,
        "--route",
        route,
        "--replay",
        out / "replay.json",
        "--visual-radius",
        "3000",
        "--tick",
        "95",
        "--gpu-stride",
        "5",
        "--gpu-trace",
        gpu,
        "--shot",
        image,
        "--model-fingerprints",
        "--out",
        report,
    ]
    if cache:
        command += ["--content-cache", cache]
    with path.with_suffix(".log").open("w") as log:
        subprocess.run(
            command,
            env=env,
            stdout=log,
            stderr=subprocess.STDOUT,
            check=True,
            timeout=480,
        )
    journal = records[start:]
    WRITE(path.with_suffix(".origin.json"), journal)
    result = READ(report)
    storage = result["storage"]
    render = READ(gpu)
    final = render["frames"][-1]
    events = storage["visual_lods"]["events"]
    return {
        "adapter": render["adapter"],
        "driver": render["driver"],
        "requests": [(e["name"], e["status"], e["bytes"]) for e in journal],
        "textureFormats": final["texture_formats"],
        "gpuPayloadBytes": final["texture_payload_bytes"],
        "namedGpuAllocations": final["memory"].get("named_base_color_allocations"),
        "imageIo": storage["image_io"],
        "transport": result["transport"]["io"],
        "fallbacks": storage["image_fallbacks"],
        "published": result["published"],
        "lodEvents": events,
        "modelFingerprints": storage["model_fingerprints"],
        "pixels": digest(HELPERS["rgb_pixels"](image)),
    }


def compare(out, left, right, empty):
    frames = [
        memoryview(HELPERS["rgb_pixels"](out / (label + ".png")))
        for label in (left, right, empty)
    ]
    a, b, bg = frames
    intersection = union = diff = error = count_a = count_b = max_delta = 0
    for j in range(0, len(bg), 3):
        x = max(abs(a[j + k] - bg[j + k]) for k in range(3)) >= 8
        y = max(abs(b[j + k] - bg[j + k]) for k in range(3)) >= 8
        count_a += x
        count_b += y
        union += x or y
        intersection += x and y
        deltas = [abs(a[j + k] - b[j + k]) for k in range(3)]
        delta = max(deltas)
        diff += delta > 0
        max_delta = max(max_delta, delta)
        if x or y:
            error += sum(deltas)
    if not union:
        return {
            "pixels": len(a) // 3,
            "foregroundThreshold": 8,
            "foregroundUnion": 0,
            "iou": None,
            "allChangedPixels": diff,
            "maxAbsoluteChannelError": max_delta,
        }
    return {
        "pixels": len(a) // 3,
        "foregroundThreshold": 8,
        "foregroundBC7": count_a,
        "foregroundCore": count_b,
        "foregroundUnion": union,
        "foregroundIntersection": intersection,
        "iou": intersection / union,
        "foregroundMeanAbsoluteChannelError": error / (3 * union),
        "maxAbsoluteChannelError": max_delta,
        "allChangedPixels": diff,
    }


def mixed_pages(assembled, out, width):
    """Clip both a core-only and a dual-source binding for the same raster into multiple pages."""

    def change(doc):
        for node in doc["nodes"]:
            node["extras"]["osage"].pop("visualLods")
            node["scale"] = [66 / width, 1, 1]
        mesh = copy.deepcopy(doc["meshes"][doc["nodes"][0]["mesh"]])
        for primitive in mesh["primitives"]:
            mat = copy.deepcopy(doc["materials"][primitive["material"]])
            binding = mat["pbrMetallicRoughness"]["baseColorTexture"]
            texture = copy.deepcopy(doc["textures"][binding["index"]])
            texture["extensions"].pop("KHR_texture_basisu")
            binding["index"] = len(doc["textures"])
            doc["textures"].append(texture)
            primitive["material"] = len(doc["materials"])
            doc["materials"].append(mat)
        for node in doc["nodes"][1::2]:
            node["mesh"] = len(doc["meshes"])
        doc["meshes"].append(mesh)

    source = out / "mixed.glb"
    EDIT(assembled, source, change)
    pages = out / "mixed"
    COOK(source, pages, 32)
    index = READ(pages / "index.json")
    assert len(index["tiles"]) >= 2
    assert not index.get("model_resources")
    assert any(
        t["visual"].get("image_choices") == [[0, None], [0, 0]] for t in index["tiles"]
    )
    return pages


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("--bin", required=True, type=Path)
    cli.add_argument(
        "--source",
        required=True,
        type=Path,
        help="Single-owner two-level GLB with embedded core and optional BasisU",
    )
    cli.add_argument(
        "--out",
        required=True,
        type=Path,
        help="Dedicated output directory for figures, cache and receipts",
    )
    opts = cli.parse_args()
    exe, source, out = (x.resolve() for x in (opts.bin, opts.source, opts.out))
    out.mkdir(exist_ok=True, parents=True)
    single = out / "one"
    COOK(source, single, 32)
    _, group, models = HELPERS["audit"](source, single)
    center = [(a + b) / 2 for a, b in zip(*group["bounds"])]
    center[0] += sum(POSITIONS) / len(POSITIONS)
    assembled = out / "placements.glb"
    EDIT(source, assembled, arrange)
    pages = out / "district"
    COOK(assembled, pages, 32)
    index = READ(pages / "index.json")
    assert len(index["tiles"]) > 1
    assert index["source_instances"] == len(POSITIONS)
    assert {(m["sha256"], m["triangles"]) for m in index["model_resources"]} == {
        (m["archive_sha256"], m["triangles"]) for m in models
    }
    (core,) = index["texture_resources"]
    (alternate,) = index["basisu_resources"]
    assert alternate["source"] == 0
    assert {t["basisu"] for t in index["textures"]} == {0}
    for entry in (core, alternate):
        content = (pages / entry["file"]).read_bytes()
        assert digest(content) == entry["sha256"] and len(content) == entry["bytes"]

    invalid_source = out / "wrong-image.glb"

    def wrong_image(doc):
        for texture in doc["textures"]:
            texture["extensions"]["KHR_texture_basisu"]["source"] = texture["source"]

    EDIT(source, invalid_source, wrong_image)
    with warnings.catch_warnings(record=True) as diagnostics:
        warnings.simplefilter("always")
        COOK(invalid_source, out / "core-only", 32)
    core_only = READ(out / "core-only" / "index.json")
    assert diagnostics and "expected a KTX2 image" in str(diagnostics[0].message)
    assert not core_only.get("basisu_resources") and core_only["texture_resources"] == [
        core
    ]

    # Zero alpha certifies the background with the same camera, placements, render settings and frame count.
    def invisible(doc):
        for mat in doc["materials"]:
            mat.setdefault("pbrMetallicRoughness", {})["baseColorFactor"] = [1, 1, 1, 0]
            mat.update(alphaMode="MASK", alphaCutoff=0.5)

    empty = out / "empty.glb"
    EDIT(assembled, empty, invisible)
    COOK(empty, out / "empty", 32)
    forced = out / "medium.glb"

    def medium(doc):
        for node in doc["nodes"]:
            levels = node["extras"]["osage"].pop("visualLods")
            node["mesh"] = next(x["mesh"] for x in levels if x["level"] == "medium")

    EDIT(assembled, forced, medium)
    COOK(forced, out / "medium", 32)

    def node(t, distance):
        return {
            "tick": t,
            "eye": [center[0], center[1], center[2] + distance],
            "aim": center,
        }

    def route(name, keys):
        return {
            "id": name,
            "ticks": 95,
            "start_m": [16.1, 6, -8],
            "velocity_m_s": [0, 0, 0],
            "expected_support": "none",
            "camera": [node(t, d) for t, d in keys],
        }

    far = max(
        270,
        math.ceil(
            1.1 * 180 / math.tan(math.radians(30)) * group["levels"][1]["error_m"]
        ),
    )
    distances = [6, far]
    WRITE(
        out / "replay.json",
        {
            "version": 1,
            "tick_hz": 60,
            "gravity_m_s2": 9.81,
            "terminal_m_s": 55,
            "viewport": [640, 360],
            "vertical_fov_deg": 60,
            "routes": [
                route("close", [(0, 6), (95, 6)]),
                route("far", [(0, far), (95, far)]),
                route(
                    "travel", [(0, 6), (30, 6), (31, far), (65, far), (66, 6), (95, 6)]
                ),
            ],
        },
    )
    data = {}
    for kind, disabled in (("bc", False), ("core", True)):
        cache = out / ("cache-" + kind)
        # New cold and warm runs always share only this invocation's content cache.
        with tempfile.TemporaryDirectory(dir=out, prefix=cache.name + "-") as cache_dir:
            with ORIGIN(pages, conditional=True) as (url, records):
                for temp in ("cold", "warm"):
                    tag = kind + "-" + temp
                    row = data[tag] = capture(
                        exe, out, tag, url, records, cache_dir, "travel", disabled
                    )
                    selected = core if disabled else alternate
                    imgs = [
                        x
                        for x in row["requests"]
                        if x[0].endswith((".png", ".jpg", ".ktx2"))
                    ]
                    assert imgs == (
                        [(selected["file"], 200, selected["bytes"])]
                        if temp == "cold"
                        else []
                    )
                    assert (
                        row["imageIo"]["reads"]
                        == row["imageIo"]["worker_prepared"]
                        == 1
                    )
                    assert (
                        row["imageIo"]["simulation_decode_us"] == 0
                        and row["fallbacks"] == []
                    )
                    assert row["transport"]["worker_image_decodes"] == 1
                    assert row["textureFormats"] == {
                        ("Rgba8UnormSrgb" if disabled else "Bc7RgbaUnormSrgb"): 1
                    }
                    for i in range(len(POSITIONS)):
                        assert [
                            e["level"] for e in row["lodEvents"] if e["owner"] == i
                        ] == ["high", "medium", "high"], (tag, i, row["lodEvents"])
                    if temp == "warm":
                        assert row["requests"] == [("index.json", 304, 0)]
                        assert row["pixels"] == data[kind + "-cold"]["pixels"]
            for tag, directory, camera in (
                ("medium", out / "medium", "close"),
                ("far", pages, "far"),
            ):
                with ORIGIN(directory) as (url, records):
                    label = kind + "-" + tag
                    data[label] = capture(
                        exe, out, label, url, records, None, camera, disabled
                    )
                    assert (
                        data[label]["imageIo"]["reads"]
                        == data[label]["imageIo"]["worker_prepared"]
                        == 1
                    )
    for label, camera in (("empty", "close"), ("empty-far", "far")):
        with ORIGIN(out / "empty") as (url, records):
            data[label] = capture(exe, out, label, url, records, None, camera, True)
    bad = out / "invalid-format"
    shutil.copytree(pages, bad, dirs_exist_ok=True)
    bad_index = copy.deepcopy(index)
    malformed = bytearray((pages / alternate["file"]).read_bytes())
    malformed[20:24] = bytes(
        4
    )  # KTX2 pixelWidth must be positive, even if the content hash is valid.
    invalid_sha = digest(malformed)
    bad_index["basisu_resources"][0].update(
        file=invalid_sha + ".ktx2", sha256=invalid_sha
    )
    (bad / (invalid_sha + ".ktx2")).write_bytes(malformed)
    WRITE(bad / "index.json", bad_index)
    for reason, directory, origin_args in (
        ("damaged", pages, {"corrupt_once": alternate["file"]}),
        ("unavailable", pages, {"fail_once": alternate["file"]}),
        ("invalid", bad, {}),
    ):
        with ORIGIN(directory, **origin_args) as (url, records):
            row = data[reason] = capture(
                exe, out, reason, url, records, None, "travel", False
            )
            assert (
                len(row["fallbacks"]) == 1
                and row["pixels"] == data["core-cold"]["pixels"]
            )
            assert row["imageIo"]["worker_prepared"] == 1 and row["textureFormats"] == {
                "Rgba8UnormSrgb": 1
            }
    mixed = mixed_pages(assembled, out, group["bounds"][1][0] - group["bounds"][0][0])
    for tag, options in (
        ("mixed", {}),
        ("mixed-core", {}),
        (
            "mixed-fallback",
            {"fail_once": alternate["file"], "delay": {alternate["file"]: 0.15}},
        ),
    ):
        with ORIGIN(mixed, **options) as (url, records):
            row = data[tag] = capture(
                exe, out, tag, url, records, None, "close", tag == "mixed-core"
            )
            assert row["published"]["fine_tiles"] >= 2
            images = [
                name
                for name, _, _ in row["requests"]
                if name.endswith((".png", ".jpg", ".ktx2"))
            ]
            assert images.count(core["file"]) == 1, (tag, images)
            assert images.count(alternate["file"]) == int(tag != "mixed-core")
            assert row["textureFormats"] == (
                {"Rgba8UnormSrgb": 1, "Bc7RgbaUnormSrgb": 1}
                if tag == "mixed"
                else {"Rgba8UnormSrgb": 1}
            )
            if tag == "mixed-fallback":
                assert row["pixels"] == data["mixed-core"]["pixels"]
                assert (
                    row["imageIo"]["resident_reuses"] == 1
                    and len(row["fallbacks"]) == 1
                )
    comparisons = {
        key: compare(out, "bc-" + tag, "core-" + tag, bg)
        for key, tag, bg in (
            ("high", "cold", "empty"),
            ("mediumAtNearCamera", "medium", "empty"),
            ("adaptiveFar", "far", "empty-far"),
        )
    }
    assert (
        comparisons["high"]["foregroundUnion"] > 0
        and comparisons["mediumAtNearCamera"]["foregroundUnion"] > 0
    )
    with exe.open("rb") as f:
        binary = hashlib.file_digest(f, "sha256").hexdigest()
    report = {
        "sourceSha256": digest(source.read_bytes()),
        "binarySha256": binary,
        "replaySha256": digest((out / "replay.json").read_bytes()),
        "placementsXMetres": POSITIONS,
        "gridMetres": 32,
        "cells": len(index["tiles"]),
        "distancesMetres": distances,
        "indexBytes": (pages / "index.json").stat().st_size,
        "core": core,
        "basisu": alternate,
        "modelOracle": models,
        "runs": data,
        "comparisons": comparisons,
    }
    WRITE(out / "summary.json", report)
    print(json.dumps(comparisons, indent=2))


if __name__ == "__main__":
    main()
