#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Measure district acoustic reads, release, hierarchy and integrity on disk and an independent HTTP origin."""

import argparse
import copy
import hashlib
import json
import math
import runpy
import shutil
import struct
import subprocess
from pathlib import Path

from paging_checks import require

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
EDGES = runpy.run_path(str(HERE / "check-impact-diffraction.py"))
COOK = runpy.run_path(str(HERE / "cook-pages.py"))["cook"]
ORIGIN = runpy.run_path(str(HERE / "check-http.py"))["origin"]
sample = runpy.run_path(str(ROOT / "tools/object-convert/test_hierarchy.py"))["sample"]
read = lambda path: json.loads(path.read_bytes())
write = lambda p, obj: p.write_text(json.dumps(obj, indent=2) + "\n")
sha = lambda data: hashlib.sha256(data).hexdigest()
DIAGONAL = ((0, 5, 0), (100, -5, 100))
WEST = ((7, 5, 10), (7, -5, 10))
EAST = ((13, 5, 10), (13, -5, 10))
EMPTY = ((200, 1, 200), (201, -1, 201))
CORNER = ((49, 5, 49), (49, 0, 49))
NEAR_PARALLEL = ((49.2, 1e-8, 49.5), (50.8, -1e-8, 50.5))
COPLANAR = ((49, 0, 49.5), (51, 0, 49.5))
SHORT = ((50, 2, 50), (50, 0.1, 50))
NEXT = struct.unpack(
    "<f", struct.pack("<I", struct.unpack("<I", struct.pack("<f", 51))[0] + 1)
)[0]
OUTSIDE = ((NEXT, 5, 50), (NEXT, -5, 50))
TWO = ((7, 1, 10), (50, 0, 50))
DECOY = ((1, 1, 75.5), (1, -1, 75.5))
RAYS = (
    DIAGONAL,
    DIAGONAL[::-1],
    WEST,
    EMPTY,
    EAST,
    CORNER,
    NEAR_PARALLEL,
    COPLANAR,
    SHORT,
    OUTSIDE,
    TWO,
    DECOY,
)


def make(out):
    asset, replay = sample(out)
    base, chunk = EDGES["common"]["read_glb"](asset)
    quad = EDGES["quad"]
    faces = [quad(49, 51, 49, 51, 0)] + [
        quad(i - 0.4, i + 0.4, 75, 76, 0) for i in range(1, 65)
    ]
    source = out / "query.glb"
    EDGES["write_geometry"](
        base, chunk, source, faces, (0.04, 0.02, 0.005), False, False
    )
    doc, binary = EDGES["common"]["read_glb"](source)
    visible = {node["mesh"] for node in doc["nodes"] if "mesh" in node}
    collision = {
        s["mesh"]
        for n in doc["nodes"]
        for s in n["extras"]["osage"].get("collision", [])
    }
    acoustic = {
        s["mesh"]
        for n in doc["nodes"]
        for s in n["extras"]["osage"].get("acoustic", [])
    }
    no_normals = {
        i
        for i, m in enumerate(doc["meshes"])
        if all("NORMAL" not in p["attributes"] for p in m["primitives"])
    }
    require(
        (acoustic - collision) <= no_normals and visible.isdisjoint(no_normals),
        "source does not isolate channel-only geometry",
    )
    stripped = copy.deepcopy(doc)
    for mi in (collision | acoustic) - visible:
        for primitive in stripped["meshes"][mi]["primitives"]:
            primitive["attributes"].pop("NORMAL", None)
    normal_free = out / "all-channel-meshes-without-normals.glb"
    EDGES["common"]["write_glb"](normal_free, stripped, binary)
    normal_pages = out / "normal-free"
    COOK(normal_free, normal_pages, 32)
    bad = copy.deepcopy(doc)
    del bad["meshes"][min(visible)]["primitives"][0]["attributes"]["NORMAL"]
    EDGES["common"]["write_glb"](out / "visible-without-normal.glb", bad, binary)
    try:
        COOK(out / "visible-without-normal.glb", out / "invalid-visual", 32)
    except ValueError as err:
        require(
            "unsupported district primitive" in str(err),
            f"unexpected visual rejection: {err}",
        )
    else:
        raise ValueError("a visible mesh lacking its required normals was accepted")
    pages = out / "pages"
    COOK(source, pages, 32)
    a, b = (read(directory / "index.json") for directory in (pages, normal_pages))
    require(
        a["object_profile"] == b["object_profile"]
        and [t.get("physics") for t in a["tiles"]]
        == [t.get("physics") for t in b["tiles"]],
        "removing unused channel normals changed binding geometry or content keys",
    )
    return (
        source,
        replay,
        pages,
        {
            "channel_only_without_normals": len(no_normals),
            "physical_meshes": len(collision),
            "normal_free_physics_and_acoustic_bytes_equal": True,
            "acoustic_meshes": len(acoustic),
            "visible_without_normals": "rejected",
        },
    )


def invoke(exe, pages, replay, rays, out, label, *, cache=None, error=None):
    path = out / (label + ".json")
    args = [
        exe,
        "--paging",
        "--pages",
        pages,
        "--route",
        "drop",
        "--replay",
        replay,
        "--tick",
        "0",
        "--out",
        path,
    ]
    if cache is not None:
        args += ["--content-cache", cache]
    for ends in rays:
        args += ["--acoustic-ray", *(",".join(map(str, p)) for p in ends)]
    proc = subprocess.run(
        args,
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=90,
    )
    (out / (label + ".log")).write_text(proc.stdout)
    if error is not None:
        require(
            proc.returncode and error in proc.stdout,
            f"{label} should reject {error!r}: {proc.stdout[-500:]}",
        )
        return proc.stdout
    require(not proc.returncode, f"{label} failed: {proc.stdout[-600:]}")
    return read(path)


def footprint(rays, index):
    """Independent count for the previous envelope-only reader."""
    previous = []
    profile = index["object_profile"]
    for a, b in rays:
        files = {}
        for node in profile["nodes"]:
            for binding in node.get("acoustic", []):
                lo, hi = binding["bounds"]
                if all(
                    min(x, y) <= h and max(x, y) >= l
                    for x, y, h, l in zip(a, b, hi, lo, strict=True)
                ):
                    f = profile["acoustic_meshes"][binding["resource"]]
                    files[f["file"]] = f["bytes"]
        previous.append({"mesh_reads": len(files), "read_bytes": sum(files.values())})
    return previous


def native_reference(exe, source, rays):
    hits = []
    for a, b in rays:
        points = [",".join(map(str, p)) for p in (a, b)]
        obj = json.loads(
            subprocess.check_output(
                [exe, "--scene", source, "--inspect-object", "--ray", *points]
            )
        )
        hits.append(obj["ray"]["acoustic"])
    return hits


def checks(result, source_hits):
    probes, io = result["acoustic_probes"], result["acoustic_io"]
    require(
        probes[-1] == result["acoustic_probe"] and len(probes) == len(RAYS),
        "legacy alias or ray order changed",
    )
    for ray, probe, ref in zip(RAYS, probes, source_hits, strict=True):
        expect = {h["id"]: h for h in ref}
        require(
            len(expect) == len(probe["hits"]),
            f"native source hit count changed at {ray}",
        )
        for hit in probe["hits"]:
            h = expect[hit["id"]]
            require(
                math.isclose(
                    h["distanceM"], hit["distance_m"], rel_tol=1e-6, abs_tol=1e-6
                )
                and h["nodePath"][-1] == hit["owner_id"]
                and h["coefficients"] == hit["coefficients"],
                f"native source owner/material/distance changed at {ray}",
            )
    require(
        [h["id"] for h in probes[10]["hits"]] == ["west-sound", "surface-0"],
        "closed segment endpoints were lost",
    )
    require(
        [h["source_path"] for h in probes[2]["hits"]] == [[0, 1, 2]]
        and [h["source_path"] for h in probes[4]["hits"]] == [[0, 1, 3]],
        "shared mesh lost its source parents",
    )
    require(
        io["mesh_reads"] == 3
        and io["peak_meshes"] == 2
        and io["resident_meshes_after"] == io["resident_payload_bytes_after"] == 0,
        "mesh acquisition or retention bound changed",
    )
    acquired = set()
    for index, (p, stats) in enumerate(zip(probes, io["rays"], strict=True)):
        for rid in stats["read_resources"]:
            require(rid not in acquired, "a payload was decoded twice")
            acquired.add(rid)
        for rid in stats["released_resources"]:
            require(
                all(
                    rid not in later["candidate_resources"]
                    for later in io["rays"][index + 1 :]
                ),
                "premature release",
            )
        require(
            p["mesh_reads"] == len(stats["read_resources"])
            and p["read_bytes"] == stats["read_bytes"],
            "per-ray IO mismatch",
        )
    require(
        io["rays"][0]["box_tests"] < io["bindings"]
        and io["rays"][0]["candidate_bindings"] == 1,
        "diagonal lacks spatial pruning",
    )
    require(
        [p["hits"] for p in probes[7:10]] == [[], [], []]
        and io["rays"][7]["candidate_bindings"] == 1
        and not io["rays"][9]["candidate_bindings"],
        "coplanar, short or one-ULP exterior control changed",
    )
    return io


def copy_pages(src, out):
    shutil.copytree(src, out, dirs_exist_ok=True)
    return out


def failure_controls(exe, pages, replay, out, slot):
    d = copy_pages(pages, out / "incomplete")
    index = read(d / "index.json")
    meshes = index["object_profile"]["acoustic_meshes"]
    wanted = meshes[slot]
    for f in set(d.glob("*.acoustic")) - {d / wanted["file"]}:
        f.unlink()
    isolated = invoke(exe, d, replay, [DIAGONAL], out, "isolated")["acoustic_probe"]
    require(
        isolated["mesh_reads"] == 1 and isolated["hits"][0]["id"] == "surface-0",
        "unrelated files required",
    )
    file = d / wanted["file"]
    data = file.read_bytes()
    file.write_bytes(bytes([data[0] ^ 1]) + data[1:])
    invoke(exe, d, replay, [DIAGONAL], out, "wrong-digest", error="bad tile content")
    for label, mutation, error in [
        (
            "wrong-index",
            lambda f: f["triangles"][0].__setitem__(0, 10000),
            "invalid acoustic geometry",
        ),
        (
            "wrong-bounds",
            lambda f: f["vertices"][0].__setitem__(0, 10000),
            "outside binding bounds",
        ),
    ]:
        faces = json.loads(data)
        mutation(faces)
        packed = json.dumps(faces, separators=(",", ":")).encode()
        key = sha(packed)
        (d / (key + ".acoustic")).write_bytes(packed)
        index["object_profile"]["acoustic_meshes"][slot] = {
            "file": key + ".acoustic",
            "sha256": key,
            "bytes": len(packed),
        }
        write(d / "index.json", index)
        invoke(exe, d, replay, [DIAGONAL], out, label, error=error)
    return {
        "unrelated_files_removed": len(meshes) - 1,
        "surviving_hit": isolated["hits"][0]["id"],
        "rejected": [
            "wrong digest",
            "invalid index with valid digest",
            "geometry outside declared bounds with valid digest",
        ],
    }


def alias_controls(exe, pages, replay, out):
    dest = copy_pages(pages, out / "aliases")
    idx = read(dest / "index.json")
    profile = idx["object_profile"]
    node = next(n for n in profile["nodes"] if n["id"] == "surface-0")
    original = node["acoustic"][0]
    f = profile["acoustic_meshes"][original["resource"]]
    alias = len(profile["acoustic_meshes"])
    profile["acoustic_meshes"].append(dict(f))
    node["acoustic"].append({**original, "id": "coincident-second", "resource": alias})
    write(dest / "index.json", idx)
    r = invoke(exe, dest, replay, [DIAGONAL, DIAGONAL[::-1]], out, "alias")
    for q in r["acoustic_probes"]:
        require(
            [h["id"] for h in q["hits"]] == ["surface-0", "coincident-second"],
            "source order lost at an equal distance",
        )
    require(
        r["acoustic_io"]["mesh_reads"] == 1
        and r["acoustic_io"]["rays"][1]["binding_reuses"] == 2,
        "equal digests used different decoded meshes",
    )
    return r["acoustic_io"]


def singular_control(exe, pages, replay, out):
    dest = copy_pages(pages, out / "singular")
    idx = read(dest / "index.json")
    profile = idx["object_profile"]
    for node in profile["nodes"]:
        if node["id"] in ("surface-0", "surface-1"):
            node["acoustic"][0]["pose"] = {"scale": [1, 0, 1]}
    # Both local meshes lie at y=0, so their placed bounds remain exact.
    checks = {}
    for label, loose, reads in (("placed", 0, 1), ("legacy", 2, 2)):
        if label == "legacy":
            profile.pop("placed_bounds")
        write(dest / "index.json", idx)
        report = invoke(exe, dest, replay, [DIAGONAL], out, "singular-" + label)
        io = report["acoustic_io"]
        require(
            io["loose_bindings"] == loose
            and io["mesh_reads"] == reads
            and [h["id"] for h in report["acoustic_probe"]["hits"]] == ["surface-0"],
            f"singular placement in {label} directory lost a hit or loaded extra geometry",
        )
        checks[label] = io
    return checks


def affine_controls(exe, out):
    sample_scene, replay = sample(out)
    common = EDGES["common"]
    base, chunk = common["read_glb"](sample_scene)
    scene = out / "boundary.glb"

    def f32(value):
        return struct.unpack("<f", struct.pack("<f", value))[0]

    left, factor, shift = map(f32, (99.95, -1.2345, 120.01))
    EDGES["write_geometry"](
        base,
        chunk,
        scene,
        [EDGES["quad"](left, 101, -1, 1, 0)],
        [0.04] * 3,
        False,
        False,
    )
    doc, chunk = common["read_glb"](scene)
    panel = doc["nodes"].pop()
    root = doc["nodes"][-1]
    root["children"] = []
    edge = f32(f32(factor * left) + shift)
    rays, expected = [], []

    for n, label in enumerate(("mirror", "nested", "double", "shear", "rotation")):
        z = n * 16
        parents = [{"translation": [0, 0, z]}]
        if label == "nested":
            # At 65536 the tiny child translation is below half a source float step.
            parents.extend({"translation": [x, 0, 0]} for x in (65536, 0.003, -65536))
        if label == "double":
            parents[0]["scale"] = [-1, 1, 1]
        if label == "rotation":
            parents[0]["rotation"] = [0, math.sqrt(0.5), 0, math.sqrt(0.5)]
        parent = root
        for depth, pose in enumerate(parents):
            parent["children"].append(len(doc["nodes"]))
            parent = {
                **pose,
                "children": [],
                "extras": {"osage": {"id": f"{label}-parent-{depth}"}},
            }
            doc["nodes"].append(parent)
        parent["children"].append(len(doc["nodes"]))
        part = copy.deepcopy(panel)
        part["extras"]["osage"]["id"] = label
        part["extras"]["osage"]["acoustic"][0]["id"] = label
        shear = 2 if label == "shear" else 0
        part["matrix"] = [factor, 0, 0, 0, 0, 1, 0, 0, shear, 0, 1, 0, shift, 0, 0, 1]
        doc["nodes"].append(part)
        if label == "rotation":
            for x, hit in ((0, True), (2, False)):
                rays.append(((x, 1, z + 4), (x, -1, z + 4)))
                expected.append([label] if hit else [])
            continue
        boundary = f32(f32(f32(factor * left) + shear) + shift)
        sign = -1 if label == "double" else 1
        for dx, hit in ((-0.001, True), (0.0, True), (0.0001, False)):
            x = sign * (boundary + dx)
            rays.append(((x, 1, z + 1), (x, -1, z + 1)))
            expected.append([label] if hit else [])
    common["write_glb"](scene, doc, chunk)
    dest = out / "boundary-pages"
    COOK(scene, dest, 128)
    source_hits = native_reference(exe, scene, rays)
    require(
        [[h["id"] for h in p] for p in source_hits] == expected,
        "unexpected source boundary",
    )
    index = read(dest / "index.json")
    actual = {}
    for label in ("placed", "legacy"):
        if label == "legacy":
            # Replay the previous cook's double-precision enclosure of the first panel.
            profile = index["object_profile"]
            profile.pop("placed_bounds")
            mirror = next(n for n in profile["nodes"] if n["id"] == "mirror")
            old = factor * left + shift
            require(f32(old) < edge, "control lost the rounding difference")
            mirror["acoustic"][0]["bounds"][1][0] = old
            dest = copy_pages(dest, out / "legacy-boundary")
            write(dest / "index.json", index)
        report = invoke(exe, dest, replay, rays, out, "boundary-" + label)
        probes = report["acoustic_probes"]
        require(
            [[h["id"] for h in p["hits"]] for p in probes] == expected,
            f"{label} directory differs at the transformed boundary",
        )
        for p, ref in zip(probes, source_hits, strict=True):
            for h, r in zip(p["hits"], ref, strict=True):
                require(
                    h["distance_m"] == r["distanceM"] == 1.0
                    and h["owner_id"] == r["nodePath"][-1]
                    and h["coefficients"] == r["coefficients"],
                    "wrong transformed witness",
                )
        require(
            report["acoustic_io"]["mesh_reads"] == 1,
            "the transformed instances must share one acoustic payload",
        )
        actual[label] = {"rays": len(rays), "hits": sum(map(len, expected)), "reads": 1}
    return actual


def http(exe, pages, replay, out, local):
    cache = out / "http-cache"
    cache.mkdir(exist_ok=True)
    # Repeated invocations of the oracle start with new content and validator directories.
    import tempfile

    with (
        tempfile.TemporaryDirectory(dir=cache) as tmp,
        ORIGIN(pages, conditional=True) as (url, records),
    ):
        logs = {}
        for label in ("cold", "warm"):
            before = len(records)
            value = invoke(exe, url, replay, RAYS, out, "http-" + label, cache=tmp)
            require(
                value["acoustic_probes"] == local["acoustic_probes"]
                and value["acoustic_io"] == local["acoustic_io"],
                "transport changed probe data",
            )
            all_rows = records[before:]
            rows = [v for v in all_rows if v["name"].endswith(".acoustic")]
            require(
                len(rows) == (3 if label == "cold" else 0),
                "unexpected HTTP acoustic GET",
            )
            if label == "cold":
                require(
                    sum(v["bytes"] for v in rows) == local["acoustic_io"]["read_bytes"],
                    "origin acoustic body budget",
                )
            else:
                require(
                    any(
                        v["name"] == "index.json" and v["status"] == 304
                        for v in all_rows
                    ),
                    "warm index was not revalidated",
                )
            logs[label] = {
                "acoustic_gets": len(rows),
                "acoustic_body_bytes": sum(v["bytes"] for v in rows),
                "requests": all_rows,
            }
    idx = read(pages / "index.json")
    slot = local["acoustic_io"]["rays"][0]["read_resources"][0]
    name = idx["object_profile"]["acoustic_meshes"][slot]["file"]
    with (
        tempfile.TemporaryDirectory(dir=cache) as tmp,
        ORIGIN(pages, corrupt_once=name) as (url, records),
    ):
        invoke(
            exe,
            url,
            replay,
            [DIAGONAL],
            out,
            "http-corrupt",
            cache=tmp,
            error="bad tile content",
        )
        require(
            not (Path(tmp) / name).exists(),
            "corrupt HTTP body entered the persistent cache",
        )
        healed = invoke(exe, url, replay, [DIAGONAL], out, "http-healed", cache=tmp)
        require(
            healed["acoustic_probe"] == local["acoustic_probes"][0],
            "verified HTTP recovery lost the hit",
        )
        logs["corrupt_then_healthy"] = [r for r in records if r["name"] == name]
    return logs


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    cli.add_argument(
        "--reference",
        type=Path,
        help="earlier envelope-only reader for identical per-ray comparisons",
    )
    cli.add_argument(
        "--out",
        type=Path,
        default=ROOT / "bench/out/ville-paris/acoustic-reads/qualified",
    )
    args = cli.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    source, replay, pages, channels = make(out)
    previous = footprint(RAYS, read(pages / "index.json"))
    local = invoke(args.bin, pages, replay, RAYS, out, "local")
    io = checks(local, native_reference(args.bin, source, RAYS))
    without = invoke(args.bin, pages, replay, [], out, "no-probe")
    require(
        local["storage"]["cold_file_bytes"]
        == without["storage"]["cold_file_bytes"] + io["read_bytes"],
        "cold file budget omitted the verified acoustic bytes",
    )
    invoke(
        args.bin,
        pages,
        replay,
        [DIAGONAL, (WEST[0], WEST[0])],
        out,
        "invalid-segment",
        error="invalid acoustic ray",
    )

    reference = {}
    if args.reference:
        for i, ray in enumerate(RAYS):
            before = invoke(
                args.reference, pages, replay, [ray], out, f"reference-{i}"
            )["acoustic_probe"]
            after = local["acoustic_probes"][i]
            require(
                before["hits"] == after["hits"]
                and all(before[k] == v for k, v in previous[i].items()),
                "previous reader differs from geometry or acquisition reference",
            )
        reference["binary_sha256"] = sha(args.reference.read_bytes())
    require(
        previous[0] == {"mesh_reads": 66, "read_bytes": 10676}
        and io["rays"][0]["read_bytes"] == 108,
        "unexpected measured diagonal inputs",
    )
    summary = {
        "binary_sha256": sha(args.bin.read_bytes()),
        "source_sha256": sha(source.read_bytes()),
        "cook": channels,
        "previous": previous,
        "reference": reference,
        "probes": local["acoustic_probes"],
        "io": io,
        "failures": failure_controls(
            args.bin, pages, replay, out, io["rays"][0]["read_resources"][0]
        ),
        "aliases": alias_controls(args.bin, pages, replay, out),
        "affine": affine_controls(args.bin, out),
        "singular": singular_control(args.bin, pages, replay, out),
        "http": http(args.bin, pages, replay, out, local),
    }
    write(out / "summary.json", summary)
    print(
        json.dumps(
            {
                "diagonal_previous": previous[0],
                "diagonal_current": local["acoustic_probes"][0],
                "series_reads": io["mesh_reads"],
                "series_bytes": io["read_bytes"],
                "http_cold_acoustic_GET": summary["http"]["cold"]["acoustic_gets"],
                "http_warm_acoustic_GET": summary["http"]["warm"]["acoustic_gets"],
            }
        )
    )


if __name__ == "__main__":
    main()
