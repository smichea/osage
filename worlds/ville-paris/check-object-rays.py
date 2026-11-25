#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Probe reflected source surfaces, shared edges and coincident triangles through the object inspector."""
import argparse
import hashlib
import json
import runpy
import subprocess
from pathlib import Path

from paging_checks import require

HERE = Path(__file__).resolve().parent
SOURCE = runpy.run_path(str(HERE / "check-source-faces.py"))
GLB = SOURCE["GLB"]


def probe(binary, scene, rays, dest):
    args = [binary, "--scene", scene, "--inspect-object"]
    for _, start, end in rays:
        args.extend(["--ray", ",".join(map(str,start)), ",".join(map(str,end))])
    result = subprocess.run(args, capture_output=True)
    require(result.returncode == 0, result.stderr.decode()[-1000:])
    dest.write_bytes(result.stdout)
    doc = json.loads(result.stdout)
    rows = doc.get("rays", [doc["ray"]])
    require(len(rows) == len(rays) and rows[-1] == doc["ray"], "single and batch ray contracts differ")
    return rows


def coincident(source, out):
    tree, blob = GLB["read_glb"](source)
    node = tree["nodes"][-1]
    shape = node["extras"]["osage"]["collision"][0]
    node["mesh"] = shape["mesh"]
    primitive, = tree["meshes"][shape["mesh"]]["primitives"]
    raw = bytearray(blob)
    ix = tree["accessors"][primitive["indices"]]
    view = tree["bufferViews"][ix["bufferView"]]
    origin = 8 + view["byteOffset"] + ix.get("byteOffset", 0)
    raw[origin+6:origin+12] = raw[origin:origin+6]
    # Split the indices into two primitives. The second primitive starts at source face 1.
    second = dict(ix, byteOffset=ix.get("byteOffset",0) + 6, count=ix["count"]-3)
    ix["count"] = 3
    tree["accessors"].append(second)
    tree["meshes"][shape["mesh"]]["primitives"].append(dict(primitive, indices=len(tree["accessors"])-1))
    path = out/"coincident.glb"
    GLB["write_glb"](path, tree, raw)
    return path, {k: v for k,v in [("visual", node["extras"]["osage"]["id"]), ("collision",shape["id"]),
                                     ("acoustic",node["extras"]["osage"]["acoustic"][0]["id"])]}


def batch(binary, scene, faces, out):
    rays = []
    for i, (a,b,c) in enumerate(faces):
        x,y,z = [(a[j] + b[j] + c[j])/3 for j in range(3)]
        rays.append((f"face-{i}", [x,y+1,z], [x,y-1,z]))
    for name,x,z in [("east",108,-3),("west",-65,-3.8),("edge",-74,14),
                     ("left",-74.01,14),("right",-73.99,14),("miss",-85,-0.2)]:
        rays.append((name,[x,6,z],[x,2.9,z]))
    rows = probe(binary,scene,rays,out/"batch.json")
    receipts = {}
    for (name,start,end), row in zip(rays,rows,strict=True):
        expected = set()
        for y in (3,5):
            if end[1] <= y <= start[1]:
                expected.update(SOURCE["covered"](faces,start[0],start[2],y))
        binding = [s for s in row["collision"] if s["id"] == "ramp"]
        hits = binding[0]["intersections"] if binding else []
        ids = [v["source_triangle"] for v in hits]
        require(set(ids) == expected and len(ids) == len(expected), f"{name}: triangle IDs {ids} versus {expected}")
        for h in hits:
            y = faces[h["source_triangle"]][0][1]
            require(abs(h["distance_m"] - (start[1]-y)) < 1e-6 and abs(h["point_m"][1] - y) < 1e-6
                    and abs(h["point_m"][0]-start[0]) < 1e-5 and abs(h["point_m"][2]-start[2]) < 1e-5,
                    f"{name}: incorrect world-space intersection")
        require(row["testedTriangles"] < len(faces), f"{name}: broad phase failed to remove distant triangles")
        receipts[name] = {"triangles": ids, "tested": row["testedTriangles"]}
    return receipts


def main():
    arg = argparse.ArgumentParser(description=__doc__)
    arg.add_argument("--bin", type=Path, required=True)
    arg.add_argument("--out", type=Path, required=True)
    args = arg.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True,exist_ok=True)
    source, _, faces, _, _ = SOURCE["TOPOLOGY"]["prepare"](out)
    base = batch(args.bin,source,faces,out)
    # The injected duplicate shares all three vertices and its material with its neighbour.
    scene, names = coincident(source,out)
    a,b,c = faces[0]
    x,z = [(a[i]+b[i]+c[i])/3 for i in (0,2)]
    rays = [("down",[x,4,z],[x,2,z]), ("up",[x,2,z],[x,4,z]),
            ("endpoint",[x,4,z],[x,3,z]), ("short",[x,4,z],[x,3.001,z])]
    rows = probe(args.bin,scene,rays,out/"duplicates.json")
    result = {}
    for (name,_,_),row in zip(rays,rows,strict=True):
        result[name] = {}
        for channel, label in names.items():
            bindings = [g for g in row[channel] if g["id"] == label]
            hits = bindings[0]["intersections"] if bindings else []
            ids = [h["source_triangle"] for h in hits]
            require(ids == ([] if name == "short" else [0,1]), f"{name}/{channel}: {ids}")
            require(all(h["distance_m"] == 1. for h in hits), "wrong distance on tied faces")
            result[name][channel] = ids
    with args.bin.open("rb") as f:
        binary_hash=hashlib.file_digest(f,"sha256").hexdigest()
    summary = {"binary_sha256":binary_hash,"source_sha256":hashlib.sha256(source.read_bytes()).hexdigest(),
               "probes":base,"coincident_faces":result}
    (out/"summary.json").write_text(json.dumps(summary,indent=2)+"\n")
    print("81 face centres, 6 edge/extent probes, 4 two-sided and finite-segment probes across all 3 channels")


if __name__ == "__main__":
    main()
