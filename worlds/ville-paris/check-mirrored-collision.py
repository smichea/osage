#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Compare the winding, support classification, material and landing of instances of one authored slab."""

import argparse
import copy
import csv
import json
import math
import runpy
import struct
import subprocess
from pathlib import Path

from paging_support_storage import unpack_support
from paging_collision_codec import read_collision

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
NAMES = ("regular", "mirror-x", "sheared-mirror", "double-mirror", "mirror-y")


def cross(a, b):
    return [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]


def normal(a, b, c):
    n = cross([x - y for x, y in zip(b, a)], [x - y for x, y in zip(c, a)])
    return [x / math.hypot(*n) for x in n]


def fixture(out):
    make = runpy.run_path(str(ROOT / "tools/object-convert/test_hierarchy.py"))[
        "sample"
    ]
    path, replay = make(out)
    raw = path.read_bytes()
    old = struct.unpack_from("<I", raw, 12)[0]
    doc = json.loads(raw[20 : 20 + old])
    piece = doc["nodes"][2]
    doc["nodes"] = []
    roots = []
    routes = []
    expected = {}
    replay_doc = json.loads(replay.read_bytes())
    template = replay_doc["routes"][0]
    for i, name in enumerate(NAMES):
        x = i * 64.0
        z = 0.0
        node = copy.deepcopy(piece)
        for k in ("translation", "rotation", "scale", "matrix", "children"):
            node.pop(k, None)
        fields = node["extras"]["osage"]
        fields["id"] = name
        material = name + "-material"
        surface = name + "-floor"
        fields["collision"][0].update(id=surface, material=material)
        fields["acoustic"][0]["id"] = name + "-sound"
        coef = {"friction": 0.25 + i * 0.125, "restitution": 0.0}
        doc["asset"]["extras"]["osageObject"]["collisionMaterials"][material] = coef
        parent = None
        center = [x, 1.0, z]
        signed = [0.0, 1.0, 0.0]
        if name == "sheared-mirror":
            # Column-major affine basis with shear, a tilted floor and a negative determinant.
            u, v, w = [-1.2, 0.26, 0.15], [0.1, 1.0, 0.06], [0.18, 0.12, 1.1]
            parent = {"matrix": [*u, 0.0, *v, 0.0, *w, 0.0, x, 0.0, z, 1.0]}
            center = [x + v[0], v[1], z + v[2]]
            n = cross(w, u)
            signed = [-y / math.hypot(*n) for y in n]
        elif name == "double-mirror":
            parent = {"translation": [x, 0.0, z], "scale": [-1.0, 1.0, 1.0]}
            node["scale"] = [-1.5, 1.0, 0.8]
        else:
            node["translation"] = [x, 2.0 if name == "mirror-y" else 0.0, z]
            if name == "mirror-x":
                node["scale"] = [-1.25, 1.0, 0.75]
            if name == "mirror-y":
                node["scale"] = [1.0, -1.0, 1.0]
                signed = [0.0, -1.0, 0.0]
        root = len(doc["nodes"])
        roots.append(root)
        if parent is not None:
            parent.update(
                children=[root + 1], extras={"osage": {"id": name + "-parent"}}
            )
            doc["nodes"].append(parent)
        owner = len(doc["nodes"])
        doc["nodes"].append(node)
        r = copy.deepcopy(template)
        r.update(
            id=name,
            ticks=160,
            start_m=[center[0], center[1] + 2.0, center[2]],
            velocity_m_s=[0.0, 0.0, 0.0],
            expected_support=surface,
        )
        r["camera"] = [
            {"tick": t, "eye": [x + 4.0, 6.0, 8.0], "aim": center} for t in (0, 160)
        ]
        routes.append(r)
        expected[surface] = {
            "normal": signed,
            "center": center,
            "sourceNode": owner,
            "material": material,
            "coefficients": coef,
            "tile": [i, 0],
        }
    doc["scenes"][doc["scene"]]["nodes"] = roots
    js = json.dumps(doc, separators=(",", ":")).encode()
    js += b" " * (-len(js) % 4)
    tail = raw[20 + old :]
    path = out / "mirror.glb"
    path.write_bytes(
        raw[:8]
        + struct.pack("<III", 20 + len(js) + len(tail), len(js), 0x4E4F534A)
        + js
        + tail
    )
    replay = out / "mirror-routes.json"
    replay.write_text(json.dumps(dict(replay_doc, routes=routes)) + "\n")
    return path, replay, expected


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    ap.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/mirrored-collision"
    )
    args = ap.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    city = args.bin.resolve()
    scene, replay, expected = fixture(out)
    pages = out / "district"
    runpy.run_path(str(HERE / "cook-pages.py"))["cook"](scene, pages, 64)
    index = json.loads((pages / "index.json").read_bytes())
    catalogue = json.loads(
        unpack_support((pages / index["support"]["file"]).read_bytes())
    )
    report = {"geometry": {}, "routes": {}}
    shapes = [t for t in index["tiles"] if t.get("physics")]
    assert len(shapes) == len(expected)
    for tile, faces in zip(shapes, catalogue["pages"], strict=True):
        layer = tile["physics"]
        data = read_collision(pages, layer)
        for shape, cert, coef in zip(data, faces, layer["materials"], strict=True):
            sid = shape["id"]
            exp = expected[sid]
            values = [
                normal(*(shape["vertices"][j] for j in tri))
                for tri in shape["triangles"]
            ]
            report["geometry"][sid] = {
                "expectedNormal": exp["normal"],
                "worldNormals": values,
                "proxyFaces": len(cert[2]),
                "coefficient": coef,
                "sourceNode": shape["source_node"],
                "vertexCount": len(shape["vertices"]),
                "triangleCount": len(shape["triangles"]),
            }
    (out / "geometry.json").write_text(json.dumps(report["geometry"], indent=2) + "\n")

    def run(args):
        result = subprocess.run([city, *args], capture_output=True, check=False)
        if result.returncode:
            raise RuntimeError(result.stderr.decode())
        return result

    trace = out / "resident"
    run(
        [
            "--scene",
            scene,
            "--replay",
            replay,
            "--physics",
            "rapier",
            "--actor",
            "capsule",
            "--bench",
            trace,
        ]
    )
    mono = json.loads((trace / "dynamics.json").read_bytes())["run"]
    witnesses = {}
    with (trace / "dynamics.csv").open() as stream:
        for row in csv.DictReader(stream):
            contacts = json.loads(row["contacts_json"])
            if contacts and row["route"] not in witnesses:
                witnesses[row["route"]] = contacts[0]
    for sid, exp in expected.items():
        name = sid[:-6]
        c = exp["center"]
        q = json.loads(
            run(
                [
                    "--scene",
                    scene,
                    "--inspect-object",
                    "--ray",
                    ",".join(map(str, [c[0], c[1] + 3.0, c[2]])),
                    ",".join(map(str, [c[0], c[1] - 3.0, c[2]])),
                ]
            ).stdout
        )
        binding = next(v for v in q["ray"]["collision"] if v["id"] == sid)
        resident = next(v for v in mono["outcomes"] if v["id"] == name)
        case = {
            "residentBinding": binding,
            "residentFirstContacts": resident["first_contacts"],
            "residentWitness": witnesses[name],
            "residentEndCenterM": resident["end_root_m"],
        }
        for mode, flags in [
            ("full", ["--all-resident"]),
            (
                "held",
                [
                    "--watch",
                    ",".join(map(str, exp["tile"])),
                    "--hold",
                    "physics",
                    "--hold-ticks",
                    "90",
                ],
            ),
        ]:
            f = out / f"{name}-{mode}.json"
            run(
                [
                    "--paging",
                    "--pages",
                    pages,
                    "--route",
                    name,
                    "--replay",
                    replay,
                    "--out",
                    f,
                    *flags,
                ]
            )
            r = json.loads(f.read_bytes())
            case[mode] = {
                "firstContact": r["first_contact"],
                "heldTicks": r["support"]["stats"]["held_ticks"],
                "finalFootM": r["final_foot_m"],
            }
        report["routes"][sid] = case
    (out / "receipt.json").write_text(json.dumps(report, indent=2) + "\n")
    for sid, geom in report["geometry"].items():
        exp = expected[sid]
        case = report["routes"][sid]
        assert all(math.dist(n, exp["normal"]) < 2e-5 for n in geom["worldNormals"]), (
            sid,
            geom,
        )
        assert geom["proxyFaces"] == (2 if exp["normal"][1] > 0.5 else 0), (sid, geom)
        assert (
            geom["sourceNode"] == exp["sourceNode"]
            and geom["coefficient"] == exp["coefficients"]
        )
        assert (
            case["residentBinding"]["material"] == exp["material"]
            and abs(case["residentBinding"]["distanceM"] - 3.0) < 1e-5
        )
        m = case["residentFirstContacts"]
        f = case["full"]["firstContact"]
        h = case["held"]["firstContact"]
        witness = case["residentWitness"]
        assert (
            m
            and m[0]["surface"] == sid
            and math.dist(witness["normal"], exp["normal"]) < 2e-5
        )
        if exp["normal"][1] > 0.5:
            assert f and h and f["support"] == h["support"] == sid
            assert f["source_node"] == h["source_node"] == exp["sourceNode"]
            assert f["tick"] == h["tick"] == m[0]["tick"]
            assert math.dist(f["point"], witness["point"]) < 2e-4
            assert math.dist(f["normal"], witness["normal"]) < 2e-5
            assert (
                h["geometry"] == "certified_support" and case["held"]["heldTicks"] == 0
            ), (sid, case)
        else:
            # The reversed vertical scale turns the authored upper face into an underside.
            # The full solver can contact that side while falling through, without landing.
            assert f is None and h is None and case["held"]["heldTicks"] > 0, (
                sid,
                case,
            )
            assert (
                case["residentEndCenterM"][1] < 0.0
                and case["full"]["finalFootM"][1] < 0.0
            )
    print(
        json.dumps(
            {
                "instances": len(expected),
                "signedTriangles": sum(
                    q["triangleCount"] for q in report["geometry"].values()
                ),
                "certifiedFaces": sum(
                    q["proxyFaces"] for q in report["geometry"].values()
                ),
            }
        )
    )


if __name__ == "__main__":
    main()
