#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Check the original model triangle of a solved contact before and after its file arrives."""
import argparse
import csv
import hashlib
import json
import math
import runpy
import struct
import subprocess
from pathlib import Path

from paging_checks import require

HERE = Path(__file__).resolve().parent
TOPOLOGY = runpy.run_path(str(HERE / "check-collision-topology.py"))
AUDIO = TOPOLOGY["AUDIO"]
GLB = AUDIO["EDGE"]["common"]


def command(*args):
    run = subprocess.run(args, capture_output=True)
    require(run.returncode == 0, run.stderr.decode()[-1000:])


def interleave(source, out, faces):
    """Interleave storage cells and insert downward faces at known slots in the actual GLB."""
    tree, chunk = GLB["read_glb"](source)
    mesh = tree["nodes"][-1]["extras"]["osage"]["collision"][0]["mesh"]
    primitive, = tree["meshes"][mesh]["primitives"]
    indices = tree["accessors"][primitive["indices"]]
    view = tree["bufferViews"][indices["bufferView"]]
    require(indices["componentType"] == 5123 and indices["count"] == 3 * len(faces), "unexpected fixture")
    offset = 8 + view.get("byteOffset", 0) + indices.get("byteOffset", 0)
    order = [(i * 37) % len(faces) for i in range(len(faces))]
    require(len(set(order)) == len(faces), "face permutation repeats")
    updated = bytearray(chunk)
    model_faces = []
    for i, original in enumerate(order):
        indices = struct.unpack_from("<3H", chunk, offset + 6 * original)
        face = faces[original]
        if original % 7 == 3 and original != len(faces) - 1:
            indices = (indices[0], indices[2], indices[1])
            face = (face[0], face[2], face[1])
        struct.pack_into("<3H", updated, offset + 6 * i, *indices)
        model_faces.append(face)
    scene = out / "interleaved.glb"
    GLB["write_glb"](scene, tree, updated)
    return scene, model_faces


def covered(faces, x, z, height):
    """Independent oriented area test on the authored triangles, including their common edge."""
    def cross(a, b, p):
        return (b[2] - a[2]) * (p[0] - a[0]) - (b[0] - a[0]) * (p[2] - a[2])
    return {i for i, (a, b, c) in enumerate(faces)
            if a[1] == b[1] == c[1] == height and cross(a, b, c) > 0
            and min(cross(a, b, (x, height, z)), cross(b, c, (x, height, z)), cross(c, a, (x, height, z))) >= -1e-7}


def verify_scene(binary, out, source, faces, replay, owner, legacy, coarse_storage):
    pages = out / "pages"
    AUDIO["COOK"](source, pages, 32, coarse_storage=coarse_storage)
    selections = {
        "east-terrace": (108, -3, 5), "west-terrace": (-65, -3.8, 5),
        "diagonal": (-74, 14, 3), "outside": (-85, -0.2, 5),
    }
    for i in (0, 1, 18, 37, 55, 67, 79):
        # Sample the same physical positions even after the fixture's face numbering changes.
        quad, side = divmod(i, 2)
        selections[f"ramp-{i}"] = (80 - 4 * quad - (4 if side == 0 else 8) / 3, 12 + (8 if side == 0 else 4) / 3, 3)
    expected = {k: covered(faces, *point) for k, point in selections.items()}
    require(len(expected["east-terrace"]) == 1 and expected["east-terrace"] == expected["west-terrace"], "terrace oracle")
    require(not expected["outside"] and len(expected["diagonal"]) in (1, 2), "edge oracle")
    require(all(expected[name] for name in selections if name != "outside"), "test lies on a reversed triangle")
    routes = []
    for name, (x, z, height) in selections.items():
        take = dict(replay["routes"][0], id=name, start_m=[x, height + 2, z], ticks=85, expected_support="ramp")
        take["camera"] = [dict(k, tick=t) for k, t in zip(take["camera"], (0,85), strict=True)]
        routes.append(take)
    fixture = out / "drops.json"
    fixture.write_text(json.dumps(dict(replay, routes=routes)))
    resident = out / "resident"
    command(binary, "--scene", source, "--physics", "rapier", "--replay", fixture, "--bench", resident)
    command(binary, "--scene", source, "--physics", "rapier", "--replay", fixture, "--verify", resident)
    meta = AUDIO["read"](resident / "dynamics.json")["run"]
    require(meta["format"] == "osage/dynamics-trace/3", "resident lacks triangle trace version")
    slot = meta["surfaces"].index("ramp")
    first = {}
    with (resident / "dynamics.csv").open() as stream:
        for row in csv.DictReader(stream):
            name = row["route"]
            contacts = [c for c in json.loads(row["contacts_json"]) if c["surface"] == slot]
            if not contacts or name in first:
                continue
            substep = min(c["substep"] for c in contacts)
            ids = {c["source_triangle"] for c in contacts if c["substep"] == substep}
            require(ids <= expected[name], f"resident {name}: {ids} outside {expected[name]}")
            first[name] = {"tick": int(row["tick"]), "triangles": sorted(ids)}
    require(set(first) == set(selections) - {"outside"}, "resident missed an intended landing")
    for take in meta["outcomes"]:
        name = take["id"]
        hits = [h for h in take["first_contacts"] if h["surface"] == "ramp"]
        require([h["source_triangles"] for h in hits] == ([first[name]["triangles"]] if name in first else []),
                f"resident summary disagrees with native contacts at {name}")
    runs = {}
    for name, point in selections.items():
        choices = expected[name]
        if choices:
            a, b, c = faces[min(choices)]
            cell = [math.floor((a[i] + b[i] + c[i]) / 96 + 0.5) for i in (0, 2)]
        else:
            cell = [1, 0]
        if name in ("east-terrace", "west-terrace"):
            require(cell[0] != math.floor(point[0] / 32 + 0.5), "distant triangle is in the observer cell")
        for mode, flags in [
            ("full", ["--all-resident"]), ("forecast", ["--delay-ms", "50"]),
            ("delayed", ["--watch", ",".join(map(str, cell)), "--hold", "physics", "--hold-ticks", "55",
                         "--delay-ms", "0"]),
        ]:
            path = out / f"{name}-{mode}.json"
            command(binary, "--paging", "--pages", pages, "--replay", fixture, "--route", name, "--out", path, *flags)
            trace = AUDIO["read"](path)
            hit = trace["first_contact"]
            require(bool(hit) == bool(choices), f"{name}/{mode}: hit or miss")
            if not hit:
                continue
            # The inverted neighbour of face 67 shares its whole AABB. The safety
            # catalogue holds this one drop until the exact triangles arrive.
            may_hold = source.name == "interleaved.glb" and name == "ramp-67" and mode == "delayed"
            held = trace["support"]["stats"]["held_ticks"]
            require(held == 0 or (may_hold and held == 17), f"{name}/{mode}: unexpected time held {held}")
            require(hit["source_triangle"] in choices and hit["source_node"] == owner and hit["support"] == "ramp"
                    and hit["tick"] == first[name]["tick"] + held, f"{name}/{mode}: incorrect triangle or owner {hit}")
            full = trace["support"]["first_full_contact"]
            require(full["source_triangle"] in choices, f"{name}/{mode}: page changed the attribution")
            if mode == "delayed":
                require(hit["geometry"] == ("full" if held else "certified_support")
                        and full["geometry"] == "full" and full["tick"] > 55,
                        f"{name}: delivery must occur after the first solved contact")
            runs[f"{name}-{mode}"] = {"first_triangle": hit["source_triangle"], "first_tick": hit["tick"],
                                        "full_triangle": full["source_triangle"], "full_tick": full["tick"], "held_ticks": held}
    if legacy:
        old = out / "old-resident"
        command(legacy, "--scene", source, "--physics", "rapier", "--replay", fixture, "--bench", old)
        require(AUDIO["read"](old / "dynamics.json")["run"]["format"] == "osage/dynamics-trace/2", "expected v2")
        command(binary, "--scene", source, "--physics", "rapier", "--replay", fixture, "--verify", old)
    return {"source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "source_faces": len(faces), "model_node": owner, "probe_positions_xzy_m": selections,
            "allowed_source_triangles": {k: sorted(ids) for k, ids in expected.items()},
            "resident": first, "paged": runs}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, required=True)
    parser.add_argument("--legacy-bin", type=Path)
    parser.add_argument("--coarse-storage", choices=("auto", "ovc1"), default="auto")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    source, replay, faces, owner, _ = TOPOLOGY["prepare"](out)
    results = {}
    for name, scene, model_faces in [("source", source, faces), ("interleaved", *interleave(source, out, faces))]:
        dest = out / name
        dest.mkdir(exist_ok=True)
        results[name] = verify_scene(args.bin.resolve(), dest, scene, model_faces, replay, owner, args.legacy_bin, args.coarse_storage)
    if args.legacy_bin:
        # The original flat fixture wrote v1 receipts without object paths.
        args0 = ["--scene", HERE / "fixtures/bir-hakeim.glb", "--physics", "rapier", "--replay", HERE / "fixtures/bonnet.json"]
        flat = out / "flat-v1"
        command(args.legacy_bin, *args0, "--bench", flat)
        require(AUDIO["read"](flat / "dynamics.json")["run"]["format"] == "osage/dynamics-trace/1", "expected v1")
        command(args.bin, *args0, "--verify", flat)
    results["legacy_resident_verified"] = [1,2] if args.legacy_bin else []
    with args.bin.open("rb") as f:
        results["binary_sha256"] = hashlib.file_digest(f, "sha256").hexdigest()
    (out / "summary.json").write_text(json.dumps(results, indent=2) + "\n")
    print("66 paged drops on two reflected 81-triangle meshes, resident first contacts and trace verification")


if __name__ == "__main__":
    main()
