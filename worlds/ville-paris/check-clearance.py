#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Hold a real collision page while a moving capsule approaches a jamb, its opening or the lintel."""

import argparse
import hashlib
import json
import math
import runpy
import struct
import subprocess
import sys
import tempfile
import zlib
from collections import Counter
from pathlib import Path

from paging_support_storage import unpack_support
from paging_collision_codec import read_collision

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def doorway(path):
    original = (HERE / "fixtures/bir-hakeim.glb").read_bytes()
    length = struct.unpack_from("<I", original, 12)[0]
    doc = json.loads(original[20 : 20 + length])
    binary = original[20 + length :]
    # The pilot supplies a one-metre cube with its actual glTF material and face winding.
    doc["meshes"] = [next(m for m in doc["meshes"] if m["name"] == "iron")]
    doc["nodes"] = [
        {"name": name, "mesh": 0, "translation": position, "scale": size}
        for name, position, size in [
            ("support/floor", [12, -0.25, 10], [24, 0.5, 18]),
            ("collider/left", [12, 2.5, 7], [0.4, 5, 3]),
            ("collider/right", [12, 2.5, 13], [0.4, 5, 3]),
            ("collider/lintel", [12, 5.2, 10], [0.4, 0.4, 9]),
        ]
    ]
    doc["nodes"].extend(
        [
            {"name": "entrance", "children": [1, 2, 3]},
            {"name": "building", "children": [0, 4]},
        ]
    )
    doc["scenes"] = [{"nodes": [5]}]
    doc["scene"] = 0
    head = json.dumps(doc, separators=(",", ":")).encode()
    head += b" " * (-len(head) % 4)
    payload = struct.pack("<I4s", len(head), b"JSON") + head + binary
    path.write_bytes(struct.pack("<4sII", b"glTF", 2, 12 + len(payload)) + payload)


def write_variant(source, dest, index, catalogue):
    """Give the same immutable physical files a different support catalogue."""
    dest.mkdir(exist_ok=True)
    for asset in source.iterdir():
        if asset.name != "index.json" and not (dest / asset.name).exists():
            (dest / asset.name).symlink_to(asset.resolve())
    raw = json.dumps(catalogue, separators=(",", ":")).encode()
    packed = zlib.compress(raw)
    digest = hashlib.sha256(packed).hexdigest()
    name = digest + ".zlib"
    if not (dest / name).exists():
        (dest / name).write_bytes(packed)
    descriptor = {"file": name, "sha256": digest, "bytes": len(packed), "decoded_bytes": len(raw)}
    (dest / "index.json").write_text(json.dumps({**index, "support": descriptor}))


def verify_catalogue(pages):
    idx = json.loads((pages / "index.json").read_bytes())
    ref = idx["support"]
    packed = (pages / ref["file"]).read_bytes()
    assert hashlib.sha256(packed).hexdigest() == ref["sha256"]
    cat = json.loads(unpack_support(packed))
    physics = [t["physics"] for t in idx["tiles"] if "physics" in t]
    certified = omitted = box_count = shape_count = 0
    for layer, support, boxes, slots in zip(
        physics, cat["pages"], cat["residual"], cat["residual_shapes"], strict=True
    ):
        shapes = read_collision(pages, layer)
        remaining = Counter()
        for slot, (shape, (owner, points, triangles)) in enumerate(
            zip(shapes, support, strict=True)
        ):
            assert owner == layer["source_nodes"][slot] == shape["source_node"]
            assert layer["ids"][slot] == shape["id"]
            actual = Counter(tuple(tuple(points[i]) for i in t) for t in triangles)
            for face in shape["triangles"]:
                tri = tuple(tuple(shape["vertices"][i]) for i in face)
                p, q, r = tri
                u = [b - a for a, b in zip(p, q)]
                v = [b - a for a, b in zip(p, r)]
                n = [
                    u[(i + 1) % 3] * v[(i + 2) % 3] - u[(i + 2) % 3] * v[(i + 1) % 3]
                    for i in range(3)
                ]
                if n[1] > math.hypot(*n) / 2:
                    assert actual[tri] > 0
                    actual[tri] -= 1
                    certified += 1
                else:
                    axes = list(zip(*tri))
                    key = (slot, tuple(map(min, axes)), tuple(map(max, axes)))
                    remaining[key] += 1
                    omitted += 1
            assert not +actual
        stored = Counter()
        for (lo, hi, count), slot in zip(boxes, slots, strict=True):
            stored[(slot, tuple(lo), tuple(hi))] += count
        assert stored == remaining
        box_count += len(boxes)
        shape_count += len(shapes)
    assert certified + omitted == sum(l["triangles"] for l in physics)
    return {
        "physics_pages": len(physics),
        "shape_pieces": shape_count,
        "support_faces": certified,
        "other_faces": omitted,
        "residual_boxes": box_count,
        "file_bytes": len(packed),
        "decoded_bytes": ref["decoded_bytes"],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    parser.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/clearance"
    )
    parser.add_argument(
        "--frames",
        action="store_true",
        help="test camera motion on Vulkan during the hold",
    )
    parser.add_argument(
        "--district",
        type=Path,
        help="also check every binding and face of this cooked district",
    )
    opts = parser.parse_args()
    out = opts.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    glb = out / "doorway.glb"
    doorway(glb)
    complete = out / "doorway-complete"
    subprocess.run(
        [
            sys.executable,
            HERE / "cook-pages.py",
            glb,
            complete,
            "--visual-storage",
            "demand",
        ],
        check=True,
    )
    idx = json.loads((complete / "index.json").read_bytes())
    tile = next(
        t for t in idx["tiles"] if "lintel" in t.get("physics", {}).get("ids", [])
    )
    watch = f"{tile['x']},{tile['z']}"
    cat = json.loads(unpack_support((complete / idx["support"]["file"]).read_bytes()))
    assert cat["format"] == "ville-paris/support-faces/2"

    # The partial catalogue describes missing walls. The current catalogue can
    # reconstruct those same four meshes, including both sides of the opening.
    pages = out / "doorway"
    cat.pop("corner_meshes")
    write_variant(complete, pages, idx, cat)
    idx = json.loads((pages / "index.json").read_bytes())
    replay = json.loads((HERE / "fixtures/descent.json").read_bytes())
    replay["gravity_m_s2"] = 0.02
    replay["routes"] = [
        {
            "id": route,
            "ticks": 140,
            "start_m": p,
            "velocity_m_s": v,
            "expected_support": "floor",
            "camera": [
                {"tick": t, "eye": [9, 3, 20], "aim": [12, 2, 10]} for t in (0, 140)
            ],
        }
        for route, p, v in [
            ("wall", [5, 2, 7], [12, 0, 0]),
            ("opening", [5, 2, 10], [12, 0, 0]),
            ("lintel", [12, 0.6, 10], [0, 6, 0]),
        ]
    ]
    flight = out / "routes.json"
    flight.write_text(json.dumps(replay))
    held_flags = ("--watch", watch, "--hold", "physics", "--hold-ticks", "80")

    def run(route, label, tick, *flags, directory=pages):
        receipt = out / f"{route}-{label}.json"
        subprocess.run(
            [
                opts.bin,
                "--paging",
                "--pages",
                directory,
                "--replay",
                flight,
                "--route",
                route,
                "--out",
                receipt,
                "--tick",
                str(tick),
                "--delay-ms",
                "0",
                "--mbps-per-kind",
                "1000",
                *flags,
            ],
            check=True,
        )
        return json.loads(receipt.read_bytes())

    with opts.bin.open("rb") as executable:
        binary_hash = hashlib.file_digest(executable, "sha256").hexdigest()
    results = {
        "source_sha256": idx["source_sha256"],
        "replay_sha256": hashlib.sha256(flight.read_bytes()).hexdigest(),
        "binary_sha256": binary_hash,
        "index_sha256": hashlib.sha256((pages / "index.json").read_bytes()).hexdigest(),
        "catalogue": verify_catalogue(pages),
    }
    if opts.district:
        results["district_catalogue"] = verify_catalogue(opts.district)
    for route in ("wall", "opening", "lintel"):
        all_detail = run(
            route, "complete-resident", 120, "--all-resident", "--trace-body", directory=complete
        )
        states = ("center_m", "foot_m", "velocity_m_s", "supported")
        expected = [[row[k] for k in states] for row in all_detail["body_before_step"]]
        assert len(expected) == 121
        ready_ticks = []
        for release in (16, 80, 115):
            flags = ("--watch", watch, "--hold", "physics", "--hold-ticks", str(release))
            replayed = run(route, f"complete-{release}", 120, *flags, "--trace-body", directory=complete)
            assert replayed["support"]["stats"]["held_ticks"] == 0
            assert replayed["support"]["stats"]["retained_certified_colliders"] == 4
            ready = next(e["tick"] for e in replayed["physics_events"] if e["state"] == "ready")
            assert ready == release + 1
            assert [[row[k] for k in states] for row in replayed["body_before_step"]] == expected
            ready_ticks.append(ready)
        results.setdefault("complete", {})[route] = {"matching_body_samples": len(expected), "ready_ticks": ready_ticks}
        guarded = run(route, "guarded", 75, *held_flags)
        bare = run(
            route, "unprotected", 75, *held_flags, "--late-collision", "unprotected"
        )
        final = run(route, "resumed", 120, *held_flags, "--trace-body")
        held = final["support"]["stats"]["held_ticks"]
        resident = run(route, "resident", 120 - held, "--all-resident")
        assert resident["support"]["stats"]["held_ticks"] == 0
        assert (
            max(
                abs(x - y)
                for x, y in zip(resident["final_foot_m"], final["final_foot_m"])
            )
            < 0.001
        )
        axis = 1 if route == "lintel" else 0
        boundary = 5.0 - 1.76 if route == "lintel" else 11.8 - 0.28
        coord = guarded["final_foot_m"][axis]
        events = [
            e for e in final["support"]["events"] if e["state"].startswith("clearance_")
        ]
        if route == "opening":
            assert held == 0 and not events and coord > 18.0
        else:
            assert (
                0 < boundary - coord < 0.21
                and guarded["support"]["stats"]["held_ticks"] > 0
            )
            assert bare["final_foot_m"][axis] > boundary + 1
            assert [e["state"] for e in events] == ["clearance_hold", "clearance_ready"]
            node = 1 if route == "wall" else 3
            surface = {
                "id": "left" if route == "wall" else "lintel",
                "source_node": node,
                "source_path": [5, 4, node],
            }
            assert all(e["surfaces"] == [surface] for e in events)
            ready = next(
                e["tick"]
                for e in final["physics_events"]
                if e["state"] == "ready" and e["tile"] == [tile["x"], tile["z"]]
            )
            assert (
                events[-1]["tick"] == ready
                and events[0]["foot_m"] == events[-1]["foot_m"]
            )
        results[route] = {
            "held_ticks": held,
            "guard_at_75": coord,
            "unprotected_at_75": bare["final_foot_m"][axis],
            "events": events,
            "resumed_foot_m": final["final_foot_m"],
            "resident_equal_steps_foot_m": resident["final_foot_m"],
        }

    with tempfile.TemporaryDirectory(dir=out) as test_dir:
        test_dir = Path(test_dir)
        bad = json.loads(json.dumps(cat))
        bad["residual_shapes"][0][0] = len(bad["pages"][0])
        write_variant(pages, test_dir, idx, bad)
        invalid = subprocess.run(
            [
                opts.bin,
                "--paging",
                "--pages",
                test_dir,
                "--route",
                "wall",
                "--replay",
                flight,
                "--tick",
                "0",
                "--out",
                test_dir / "invalid.json",
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        assert invalid.returncode != 0 and "unknown residual shape" in invalid.stderr
        results["invalid_binding_rejected"] = True

    # The first district format has embedded meshes and no resident support catalogue.
    legacy = out / "legacy-embedded"
    subprocess.run(
        [
            sys.executable,
            HERE / "cook-pages.py",
            glb,
            legacy,
            "--visual-storage",
            "embedded",
            "--physics-storage",
            "json",
        ],
        check=True,
    )
    manifest = json.loads((legacy / "index.json").read_bytes())
    manifest.pop("support")
    manifest["format"] = "ville-paris/pages/1"
    (legacy / "index.json").write_text(json.dumps(manifest))
    old_disk = run(
        "wall", "disk-no-catalogue", 120, *held_flags, "--wall-clock", directory=legacy
    )
    assert old_disk["support"]["mode"] == "wait_for_full"
    assert old_disk["support"]["stats"]["synchronous_legacy_bytes"] == 0
    assert (
        old_disk["support"]["stats"]["held_ticks"] > 0
        and old_disk["final_foot_m"][0] < 11.8
    )
    results["disk_without_catalogue_held_ticks"] = old_disk["support"]["stats"][
        "held_ticks"
    ]

    # Existing catalogues describe all omitted faces only by the whole page's bounds.
    old = out / "doorway-v1"
    first_catalogue = {key: cat[key] for key in ("directory_sha256", "pages")}
    first_catalogue["format"] = "ville-paris/support-faces/1"
    write_variant(pages, old, idx, first_catalogue)
    old_wall = run("wall", "legacy", 120, *held_flags, directory=old)
    assert (
        old_wall["support"]["stats"]["held_ticks"] > 0
        and old_wall["final_foot_m"][0] < 11.8
    )
    results["v1_held_ticks"] = old_wall["support"]["stats"]["held_ticks"]
    origin = runpy.run_path(str(HERE / "check-http.py"))["origin"]
    physical = tile["physics"]["file"]
    with origin(pages, delay={physical: 0.35}, corrupt_once=physical) as (url, records):
        recovered = run("wall", "http-retry", 140, directory=url)
    held = recovered["support"]["stats"]["held_ticks"]
    resident = run("wall", "http-reference", 140 - held, "--all-resident")
    logs = [r for r in records if r["name"] == physical]
    changes = [
        e for e in recovered["support"]["events"] if e["state"].startswith("clearance_")
    ]
    published = next(
        e["tick"] for e in recovered["physics_events"] if e["state"] == "ready"
    )
    assert (
        len(logs) == 2
        and logs[0]["sha256"] != logs[1]["sha256"] == tile["physics"]["sha256"]
    )
    assert recovered["transport"]["io"]["rejected_bodies"] == 1 and 0 < held < 140
    assert (
        changes[0]["foot_m"] == changes[-1]["foot_m"]
        and changes[-1]["tick"] == published
    )
    assert recovered["final_foot_m"] == resident["final_foot_m"]
    results["http_retry"] = {
        "held_ticks": held,
        "resumed_tick": published,
        "requests": logs,
        "foot_m": recovered["final_foot_m"],
    }
    if opts.frames:
        rgb = runpy.run_path(str(HERE / "check-pictures.py"))["rgb"]
        samples = {}
        positions = []
        replay["viewport"] = [640, 360]
        camera = replay["routes"][0]["camera"]
        camera[0]["eye"] = [1, 4, 15.5]
        for moving in (False, True):
            key = "moving" if moving else "fixed"
            camera[-1]["eye"] = [4, 6, 3] if moving else camera[0]["eye"]
            gpu_flight = out / f"camera-{key}.json"
            gpu_flight.write_text(json.dumps(replay))
            for tick in (40, 70):
                name = f"{key}-{tick}"
                png = out / (name + ".png")
                state = out / (name + "-state.json")
                cmd = [
                    "flock",
                    "-w",
                    "450",
                    "/tmp/osage-ville-paris-gpu.lock",
                    opts.bin,
                    "--paging",
                    "--pages",
                    pages,
                    "--replay",
                    gpu_flight,
                    "--route",
                    "wall",
                    "--out",
                    state,
                    "--tick",
                    str(tick),
                    "--delay-ms",
                    "0",
                    "--gpu-stride",
                    str(tick),
                    "--gpu-trace",
                    out / (name + "-gpu.json"),
                    "--shot",
                    png,
                    *held_flags,
                ]
                with (out / (name + ".log")).open("w") as log:
                    subprocess.run(
                        cmd, check=True, stdout=log, stderr=subprocess.STDOUT
                    )
                _, samples[name] = rgb(png)
                body = json.loads(state.read_bytes())
                assert body["support"]["stats"]["held_ticks"] > 0
                positions.append(body["final_foot_m"])
        assert positions.count(positions[0]) == len(positions)
        assert samples["fixed-40"] == samples["fixed-70"]
        left, right = samples["moving-40"], samples["moving-70"]
        changed = sum(
            left[i : i + 3] != right[i : i + 3] for i in range(0, len(left), 3)
        )
        assert changed > 1000
        results["camera"] = {
            "size": replay["viewport"],
            "fixed_camera_pixels_changed": 0,
            "moving_camera_pixels_changed": changed,
            "actor_foot_m": positions[0],
        }
    (out / "summary.json").write_text(json.dumps(results, indent=2) + "\n")
    print(
        json.dumps(
            {
                route: results[route]["held_ticks"]
                for route in ("wall", "opening", "lintel")
            }
        )
    )


if __name__ == "__main__":
    main()
