#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Audit two independently cooked district directories, including every stored collider bit."""

import argparse
import hashlib
import json
from collections import Counter
from pathlib import Path
from struct import Struct

from paging_support_storage import unpack_support
from paging_collision_codec import encode_collision, read_collision

POINT = Struct("<3f")
FACE = Struct("<3I")
CELL = Struct("<2i")
COUNTS = Struct("<2I")
STORAGE_KEYS = {"file", "bytes", "sha256"}


def equal(left, right, description):
    if left != right:
        raise ValueError(f"{description} differs")


def without(doc, keys):
    return {key: value for key, value in doc.items() if key not in keys}


def inventory(pages, directory):
    """Verify content and count the bytes of every unique file reachable from this index."""
    files = {}

    def walk(value):
        if isinstance(value, dict):
            if STORAGE_KEYS <= value.keys():
                file = value["file"]
                expected = (value["bytes"], value["sha256"])
                if file in files:
                    equal(files[file], expected, "duplicate reference")
                else:
                    raw = (pages / file).read_bytes()
                    equal((len(raw), hashlib.sha256(raw).hexdigest()), expected, file)
                    files[file] = expected
            for child in value.values():
                walk(child)
        elif isinstance(value, list):
            for child in value:
                walk(child)

    walk(directory)
    return files


def support(pages, directory, binding):
    data = (pages / directory["support"]["file"]).read_bytes()
    expanded = unpack_support(data)
    equal(len(expanded), directory["support"]["decoded_bytes"], "support byte count")
    doc = json.loads(expanded)
    equal(doc.pop("directory_sha256"), binding.hexdigest(), "support directory binding")
    return doc


def shape_data(shape):
    vertices = b"".join(POINT.pack(*p) for p in shape["vertices"])
    faces = b"".join(FACE.pack(*f) for f in shape["triangles"])
    return (
        COUNTS.pack(len(shape["vertices"]), len(shape["triangles"])) + vertices + faces
    )


def audit(legacy, packed):
    paths = (legacy, packed)
    index_data = [(p / "index.json").read_bytes() for p in paths]
    dirs = [json.loads(raw) for raw in index_data]
    equal(
        without(dirs[0], {"tiles", "support"}),
        without(dirs[1], {"tiles", "support"}),
        "shared index fields",
    )
    files = [inventory(p, d) for p, d in zip(paths, dirs, strict=True)]
    bindings = [hashlib.sha256(), hashlib.sha256()]
    tags = Counter()
    physical_files = [{}, {}]
    resource_bytes = [0, 0]
    counts = Counter()
    geometry = hashlib.sha256()
    for t0, t1 in zip(dirs[0]["tiles"], dirs[1]["tiles"], strict=True):
        equal(
            without(t0, {"physics"}),
            without(t1, {"physics"}),
            "tile key and nonphysical channels",
        )
        if "physics" not in t0 and "physics" not in t1:
            continue
        layers = [t["physics"] for t in (t0, t1)]
        equal(
            without(layers[0], STORAGE_KEYS),
            without(layers[1], STORAGE_KEYS),
            "physical identities, materials, bounds and counts",
        )
        shapes = []
        cell = CELL.pack(t0["x"], t0["z"])
        for j, (p, layer, suffix) in enumerate(
            zip(paths, layers, (".json", ".cpage"), strict=True)
        ):
            if not layer["file"].endswith(suffix):
                raise ValueError(f"expected {suffix} physics at {layer['file']}")
            shapes.append(read_collision(p, layer))
            bindings[j].update(cell + layer["sha256"].encode("ascii"))
            resource_bytes[j] += layer["bytes"]
            physical_files[j][layer["file"]] = layer["bytes"]
        equal(
            encode_collision(shapes[1]),
            (packed / layers[1]["file"]).read_bytes(),
            "re-encoded page",
        )
        geometry.update(cell + COUNTS.pack(len(shapes[0]), layers[0]["triangles"]))
        for a, b in zip(*shapes, strict=True):
            equal(
                (a["id"], a["source_node"]),
                (b["id"], b["source_node"]),
                "ordered shape owner",
            )
            bits = shape_data(a)
            equal(
                bits,
                shape_data(b),
                "ordered binary32 positions and triangle index triplets",
            )
            geometry.update(bits)
            tags[encode_collision([b])[8]] += 1
            counts.update(
                shapes=1, vertices=len(a["vertices"]), triangles=len(a["triangles"])
            )
        counts["pages"] += 1
    certificates = [
        support(p, d, h) for p, d, h in zip(paths, dirs, bindings, strict=True)
    ]
    equal(*certificates, "certified and residual geometry")
    costs = []
    for j, d in enumerate(dirs):
        index = index_data[j]
        all_bytes = sum(size for size, _ in files[j].values())
        costs.append(
            {
                "index_sha256": hashlib.sha256(index).hexdigest(),
                "index_bytes": len(index),
                "unique_referenced_files": len(files[j]),
                "unique_referenced_bytes": all_bytes,
                "index_and_unique_resource_bytes": len(index) + all_bytes,
                "collision_references_bytes": resource_bytes[j],
                "unique_collision_files": len(physical_files[j]),
                "unique_collision_bytes": sum(physical_files[j].values()),
                "support_bytes": d["support"]["bytes"],
                "support_directory_binding": bindings[j].hexdigest(),
                "index_parent_support_bytes": len(index)
                + d["parent"]["bytes"]
                + d["support"]["bytes"],
            }
        )
    return {
        "source_sha256": dirs[0]["source_sha256"],
        "edge_m": dirs[0]["edge_m"],
        "tile_count": len(dirs[0]["tiles"]),
        "matched_collision": dict(counts),
        "ordered_geometry_sha256": geometry.hexdigest(),
        "ocp1_record_tags": dict(sorted(tags.items())),
        "identical_fields": [
            "directory except collision storage and support binding",
            "ordered binary32 geometry and triangle triplets",
            "support including residual and exact geometry",
        ],
        "storage": dict(zip(("legacy_json", "packed_ocp1"), costs, strict=True)),
    }


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--legacy", required=True, type=Path)
    ap.add_argument("--packed", required=True, type=Path)
    ap.add_argument("--out", required=True, type=Path)
    args = ap.parse_args()
    result = audit(args.legacy, args.packed)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result["matched_collision"]))


if __name__ == "__main__":
    main()
