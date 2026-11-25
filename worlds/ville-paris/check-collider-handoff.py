#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Keep live contacts on matching collision pieces and install genuinely different full geometry."""

import argparse
import copy
import json
import runpy
import struct
from pathlib import Path

from paging_collision_codec import read_collision

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
ORDER = runpy.run_path(str(HERE / "check-ragdoll-contact-order.py"))
RIG = ORDER["RIG"]


def write_side(scene, dest):
    glb = runpy.run_path(str(HERE / "check-impact-transmission.py"))
    tree, chunk = glb["read_glb"](scene)
    assert chunk[4:8] == b"BIN\0"
    blob = bytearray(chunk[8 : 8 + tree["buffers"][0]["byteLength"]])

    def accessor(data, format_, component, kind, target):
        blob.extend(bytes(-len(blob) % 4))
        off = len(blob)
        for row in data:
            blob.extend(struct.pack(format_, *row))
        view = len(tree["bufferViews"])
        tree["bufferViews"].append(
            {
                "buffer": 0,
                "byteOffset": off,
                "byteLength": len(blob) - off,
                "target": target,
            }
        )
        i = len(tree["accessors"])
        tree["accessors"].append(
            {
                "bufferView": view,
                "componentType": component,
                "count": len(data),
                "type": kind,
                "min": list(map(min, zip(*data))),
                "max": list(map(max, zip(*data))),
            }
        )
        return i

    # The local -Z face becomes a world -X wall under the bridge node's rotation.
    verts = [(-1.0, 1.0, -1.0), (-1.0, -2.0, -1.0), (1.0, -2.0, -1.0), (1.0, 1.0, -1.0)]
    positions = accessor(verts, "<3f", 5126, "VEC3", 34962)
    normals = accessor([(0.0, 0.0, -1.0)] * 4, "<3f", 5126, "VEC3", 34962)
    indices = accessor([(i,) for i in (0, 2, 1, 0, 3, 2)], "<H", 5123, "SCALAR", 34963)
    owner = tree["nodes"][3]["extras"]["osage"]["collision"][0]
    mesh = copy.deepcopy(tree["meshes"][owner["mesh"]])
    mesh["name"] = "platform-side"
    mesh["primitives"].append(
        {"attributes": {"POSITION": positions, "NORMAL": normals}, "indices": indices}
    )
    owner["mesh"] = len(tree["meshes"])
    tree["nodes"][3]["extras"]["osage"]["acoustic"][0]["mesh"] = owner["mesh"]
    tree["meshes"].append(mesh)
    tree["buffers"][0]["byteLength"] = len(blob)
    blob.extend(bytes(-len(blob) % 4))
    glb["write_glb"](dest, tree, struct.pack("<I4s", len(blob), b"BIN\0") + blob)


def split_support(binary, base):
    receipt = json.loads((base / "order-receipt.json").read_bytes())
    assert receipt["bin_sha256"] == RIG["sha"](binary)
    rows = {}
    for model, trial in receipt["simultaneous"].items():
        for label in ("late-plus", "late-minus"):
            raw = json.loads((base / f"split-{model}-{label}.json").read_bytes())
            case = trial["cases"][label]
            stats = raw["support"]["stats"]
            assert (
                case["pose_sha256"] == trial["reference_pose_sha256"]
                and case["peak_member_center_difference_m"] == 0.0
            )
            assert (
                stats["retained_certified_colliders"] == 1
                and stats["replaced_certified_colliders"] == 0
            )
            rows[f"{model}/{label}"] = {
                "identical_native_trajectory": True,
                "retained": 1,
                "replaced": 0,
                "page_ready": case["page_ready"],
            }
    return rows


def mixed_page(city, out):
    scene, _, rig, template = RIG["prepare"](out / "source")
    source = out / "mixed.glb"
    write_side(scene, source)
    pages = out / "district"
    runpy.run_path(str(HERE / "cook-pages.py"))["cook"](source, pages, 32)
    idx = json.loads((pages / "index.json").read_bytes())
    layer = idx["tiles"][0]["physics"]
    full = read_collision(pages, layer)
    assert [(s["id"], len(s["triangles"])) for s in full] == [
        ("west-floor", 2),
        ("east-floor", 4),
    ]
    routes = []
    for name, position, velocity, target in [
        ("warm", [17.25, 3.0, 10.0], [0.0, 0.0, 0.0], "west-floor"),
        ("wall", [20.55, -0.7, 10.0], [4.0, 0.0, 0.0], "east-floor"),
        ("leave", [17.25, 1.02, 10.0], [8.0, 0.0, 0.0], "west-floor"),
    ]:
        routes.append(
            {
                "id": name,
                "ticks": 150 if name == "leave" else 78,
                "start_m": position,
                "velocity_m_s": velocity,
                "expected_support": target,
                "camera": [
                    {"tick": t, "eye": [16.0, 4.0, 15.0], "aim": [20.0, 1.0, 10.0]}
                    for t in (0, 150 if name == "leave" else 78)
                ],
            }
        )
    replay = out / "mixed-routes.json"
    replay.write_text(
        json.dumps(dict(template, routes=routes, tick_hz=60), indent=2) + "\n"
    )

    def held(t):
        return [
            "--watch",
            "1,0",
            "--hold",
            "physics",
            "--hold-ticks",
            str(t),
            "--delay-ms",
            "0",
            "--mbps-per-kind",
            "1000",
        ]

    def poses(receipt):
        return [r["parts_after_step"] for r in receipt["body_before_step"]]

    def wall_hits(receipt):
        table = receipt["contact_surfaces"]
        return [
            (s["tick"], receipt["actor"]["parts"][c["part"]]["name"])
            for s in receipt["body_before_step"]
            for c in s["contacts_after_step"]
            if table[c["surface"]]["id"] == "east-floor" and c["normal"][0] < -0.5
        ]

    result = {}
    for who, args in (
        ("human", ["--actor", "human"]),
        ("imported", ["--actor-rig", rig]),
    ):

        def read(case, name, flags, actor=args, label=who):
            return RIG["paged"](
                city,
                pages,
                replay,
                case,
                actor,
                flags,
                out / f"{label}-{case}-{name}.json",
            )

        baseline = read("warm", "full", ["--all-resident"])
        late = read("warm", "handoff", held(46))
        stats = late["support"]["stats"]
        assert (
            stats["retained_certified_colliders"] == 1
            and stats["replaced_certified_colliders"] == 1
        )
        assert (
            stats["full_handoffs"] == 1
            and late["first_contact"]["geometry"] == "certified_support"
        )
        assert ORDER["pose_hash"](poses(baseline)) == ORDER["pose_hash"](poses(late))
        assert late["support"]["duplicate_layer_contact_ticks"] == 0
        ref = read("wall", "full", ["--all-resident"])
        published = read("wall", "published", held(23))
        missing = read(
            "wall", "missing", [*held(120), "--late-collision", "unprotected"]
        )
        hits, arrived = wall_hits(ref), wall_hits(published)
        assert hits and arrived and not wall_hits(missing)
        assert published["support"]["stats"]["held_ticks"] > 0
        assert published["support"]["stats"]["replaced_certified_colliders"] == 1
        assert not any(
            row["contacts_after_step"] for row in missing["body_before_step"]
        )
        leave = read(
            "leave",
            "evicted",
            [
                "--physics-plan",
                "radius",
                "--physics-radius",
                "1",
                "--delay-ms",
                "0",
                "--mbps-per-kind",
                "1000",
            ],
        )
        evicted = [
            e["tick"] for e in leave["physics_events"] if e["state"] == "evicted"
        ]
        assert evicted and not any(
            row["contacts_after_step"]
            for row in leave["body_before_step"][evicted[-1] + 1 :]
        )
        result[who] = {
            "warm_reference_sha256": ORDER["pose_hash"](poses(baseline)),
            "warm_promoted_sha256": ORDER["pose_hash"](poses(late)),
            "retained": stats["retained_certified_colliders"],
            "replaced": stats["replaced_certified_colliders"],
            "first_vertical_contact_full": hits[0],
            "first_vertical_contact_after_delivery": arrived[0],
            "withheld_wall_touched": False,
            "clearance_held_ticks": published["support"]["stats"]["held_ticks"],
            "eviction_ticks": evicted,
        }
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    parser.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/collider-handoff"
    )
    parser.add_argument(
        "--order",
        type=Path,
        help="Existing check-ragdoll-contact-order output for this binary.",
    )
    args = parser.parse_args()
    city = args.bin.resolve()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    order = args.order
    if order is None:
        import sys

        order = out / "order"
        RIG["run"](
            sys.executable,
            HERE / "check-ragdoll-contact-order.py",
            "--bin",
            city,
            "--out",
            order,
        )
    report = {
        "bin_sha256": RIG["sha"](city),
        "two_supports": split_support(city, order.resolve()),
        "mixed": mixed_page(city, out),
    }
    dest = out / "handoff-receipt.json"
    dest.write_text(json.dumps(report, indent=2) + "\n")
    print(dest)


if __name__ == "__main__":
    main()
