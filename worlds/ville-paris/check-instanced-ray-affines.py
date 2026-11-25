#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Compare complete D173 and instanced-index ray hits under exact edges, affine placements and a different collision topology."""

import argparse
import copy
import json
import math
import runpy
import struct
import subprocess
from pathlib import Path

from paging_affine import apply, world_placements

HERE = Path(__file__).resolve().parent
BENCH = runpy.run_path(str(HERE / "check-object-ray-scaling.py"))
TOPOLOGY, GLB = BENCH["TOPOLOGY"], BENCH["GLB"]
ID = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]


def filter_hits(values):
    return [
        {s: row[s] for s in ("from", "to", "visual", "collision", "acoustic")}
        for row in values
    ]


def altered_mesh(doc, chunk, mesh, change):
    """Copy one stream and keep all other authored streams intact."""
    primitive = copy.deepcopy(doc["meshes"][mesh]["primitives"][0])
    binding, field = (
        (primitive, "indices")
        if change == "reorder"
        else (primitive["attributes"], "POSITION")
    )
    accessor = copy.copy(doc["accessors"][binding[field]])
    view = doc["bufferViews"][accessor["bufferView"]]
    offset = view["byteOffset"] + accessor.get("byteOffset", 0)
    index_change = change == "reorder"
    expected = 5123 if index_change else 5126
    assert accessor["componentType"] == expected and "byteStride" not in view
    length = (2 if index_change else 12) * accessor["count"]
    raw = bytearray(chunk[8:])
    stream = bytearray(raw[offset : offset + length])
    if index_change:
        stream[:6], stream[-6:] = stream[-6:], stream[:6]
    else:
        # Move each positive y up by exactly one representable binary32 step. Equal topology
        # and counts must not cause this surface to inherit its sibling's intersections.
        coords = []
        for off in range(0, length, 12):
            bits = struct.unpack_from("<I", stream, off + 4)[0]
            assert bits < 0x7F7FFFFF
            struct.pack_into("<I", stream, off + 4, bits + 1)
            coords.append(struct.unpack_from("<fff", stream, off))
        accessor["min"] = list(map(min, zip(*coords, strict=True)))
        accessor["max"] = list(map(max, zip(*coords, strict=True)))
    raw.extend(bytes(-len(raw) % 4))
    new_start = len(raw)
    raw.extend(stream)
    raw.extend(bytes(-len(raw) % 4))
    accessor["bufferView"] = len(doc["bufferViews"])
    accessor["byteOffset"] = 0
    doc["bufferViews"].append(
        {
            "buffer": 0,
            "byteOffset": new_start,
            "byteLength": len(stream),
            "target": 34963 if index_change else 34962,
        }
    )
    binding[field] = len(doc["accessors"])
    doc["accessors"].append(accessor)
    idx = len(doc["meshes"])
    doc["meshes"].append({"primitives": [primitive]})
    doc["buffers"][0]["byteLength"] = len(raw)
    return idx, struct.pack("<I4s", len(raw), b"BIN\0") + raw


def cases():
    shear = [1, 0.25, 0, 0, 0.5, 1, 0, 0, 0, 0.125, 1, 0, 0, 0, 0, 1]
    mirror = copy.copy(ID)
    mirror[0] = -1
    return {
        "identity": ({}, {}, False),
        "translation": (
            {"translation": [42, -8, 16]},
            {"translation": [5, 2, -3]},
            False,
        ),
        "quarter-turn": (
            {"rotation": [0, math.sqrt(0.5), 0, math.sqrt(0.5)]},
            {"translation": [4, 1, -2]},
            False,
        ),
        "oblique": (
            {"rotation": [0, math.sin(0.31), 0, math.cos(0.31)]},
            {"scale": [0.25, 8, 1.5]},
            False,
        ),
        "reflection": ({}, {"scale": [-1, 1, 1]}, False),
        "double-reflection": ({"matrix": mirror}, {"scale": [-1, 1, 1]}, False),
        "shear-reflection": ({"matrix": shear}, {"scale": [-1, 1, 2]}, False),
        "large-translation": (
            {"translation": [1048576, 1024, -131072]},
            {"scale": [1, 2, 0.5]},
            False,
        ),
        "thin-affine": ({}, {"scale": [2**-12, 1, 2**8]}, False),
        "collapsed": ({}, {"scale": [1, 0, 1]}, False),
        "different-collision-order": ({}, {}, "reorder"),
        "one-ulp-collision-height": ({}, {}, "one-ulp"),
    }


def run(binary, source, probes):
    args = [binary.resolve(), "--scene", source, "--inspect-object"]
    for p in probes:
        args.extend(
            ["--ray", ",".join(map(str, p["start"])), ",".join(map(str, p["end"]))]
        )
    r = subprocess.run(args, capture_output=True, text=True, check=False)
    return (
        r.returncode,
        r.stderr,
        None if r.returncode else json.loads(r.stdout)["rays"],
    )


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--previous", type=Path, required=True)
    ap.add_argument("--current", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    a = ap.parse_args()
    out = a.out.resolve()
    out.mkdir(exist_ok=True, parents=True)
    source, *_ = TOPOLOGY["prepare"](out)
    normal = out / "panel.glb"
    BENCH["scene"](source, 1, normal)
    rows = {}
    for name, (parent, local, change) in cases().items():
        doc, chunk = GLB["read_glb"](normal)
        nodes = doc["nodes"]
        node = nodes[2]
        nodes[0]["translation"] = [0, 0, 0]
        nodes[1].pop("translation", None)
        node.pop("translation", None)
        nodes[1].update(parent)
        node.update(local)
        if change:
            replacement, chunk = altered_mesh(doc, chunk, node["mesh"], change)
            geom = node["extras"]["osage"]
            geom["collision"][0]["mesh"] = replacement
            geom["acoustic"][0]["mesh"] = replacement
        file = out / (name + ".glb")
        GLB["write_glb"](file, doc, chunk)
        _, pose = world_placements(doc)[2]
        probes = []
        for tag, p in [
            ("interior", [-78.75, 3, 14.75]),
            ("vertex", [-80, 3, 12]),
            ("diagonal", [-78, 3, 14]),
            ("edge", [-80, 3, 14]),
            ("terrace", [0, 5, -3.5]),
            ("miss", [0, 3, 70]),
        ]:
            # Transform both ends of each test with the complete parent chain. A collapsed y axis
            # is also probed by a nonzero world vertical segment through the collapsed point.
            lo = apply(pose, [p[0], p[1] - 4, p[2]])
            point = apply(pose, p)
            hi = apply(pose, [p[0], p[1] + 4, p[2]])
            if hi == lo:
                lo = [point[0], point[1] - 4, point[2]]
                hi = [point[0], point[1] + 4, point[2]]
            probes += [
                {"id": tag + "/down", "start": hi, "end": lo},
                {"id": tag + "/up", "start": lo, "end": hi},
                {"id": tag + "/endpoint", "start": hi, "end": point},
                {
                    "id": tag + "/short",
                    "start": hi,
                    "end": [
                        b + (v - b) * 0.002 for v, b in zip(hi, point, strict=True)
                    ],
                },
            ]
        before, out0, old = run(a.previous, file, probes)
        after, out1, new = run(a.current, file, probes)
        if before != after or (before and out0 != out1):
            raise ValueError(f"{name}: different input acceptance {out0} / {out1}")
        if not before:
            if filter_hits(old) != filter_hits(new):
                raise ValueError(
                    f"{name}: different hit ordering, node, material, ordinal, point or distance"
                )
            hits = sum(
                len(g["intersections"])
                for r in new
                for k in ("visual", "collision", "acoustic")
                for g in r[k]
            )
            if not hits:
                raise ValueError(f"{name}: no positive ray")
            if name == "different-collision-order" and (
                [h["source_triangle"] for h in new[0]["visual"][0]["intersections"]]
                != [0]
                or [
                    h["source_triangle"]
                    for h in new[0]["collision"][0]["intersections"]
                ]
                != [80]
            ):
                raise ValueError("fallback lost authored face order")
            if name == "one-ulp-collision-height" and not (
                new[0]["visual"][0]["distanceM"] == 4
                and new[0]["collision"][0]["distanceM"] == 4 - 2.0**-22
                and new[0]["acoustic"][0]["distanceM"]
                == new[0]["collision"][0]["distanceM"]
            ):
                raise ValueError("one-ULP geometry change lost its true intersection")
            rows[name] = {
                "rays": len(probes),
                "intersections": hits,
                "old_triangle_tests": sum(v["testedTriangles"] for v in old),
                "new_triangle_tests": sum(v["testedTriangles"] for v in new),
                "geometry_sha256": BENCH["sha"](
                    json.dumps(
                        filter_hits(new), sort_keys=True, separators=(",", ":")
                    ).encode()
                ),
            }
        else:
            rows[name] = {"rejected_by_both": out0.strip()}
    (out / "summary.json").write_text(json.dumps(rows, indent=2) + "\n")
    print({k: v.get("rays", "refused") for k, v in rows.items()})


if __name__ == "__main__":
    main()
