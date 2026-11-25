#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Measure native worker/inline OMD1 publication on an already installed, real shared mesh."""

import argparse
import array
import hashlib
import json
import os
import runpy
import statistics
import struct
import subprocess
import sys
import zlib
from contextlib import ExitStack
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
ASSET = ROOT / "assets/polyhaven/pine_sapling_small/pine_sapling_small_1k.gltf"
origin = runpy.run_path(str(HERE / "check-http.py"))["origin"]
rgb = runpy.run_path(str(HERE / "check-pictures.py"))["rgb"]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def store(path, data):
    path.write_text(json.dumps(data, indent=2) + "\n")


def glb(doc, binary):
    head = json.dumps(doc, separators=(",", ":")).encode()
    head += b" " * (-len(head) % 4)
    binary = bytes(binary) + bytes(-len(binary) % 4)
    return (
        struct.pack(
            "<4sIII4s", b"glTF", 2, 28 + len(head) + len(binary), len(head), b"JSON"
        )
        + head
        + struct.pack("<I4s", len(binary), b"BIN\0")
        + binary
    )


def source_geometry(out, src):
    doc = json.loads(src.read_bytes())
    raw = (src.parent / doc["buffers"][0]["uri"]).read_bytes()
    mesh = next(m for m in doc["meshes"] if m["name"] == "pine_sapling_small_a")
    binary = bytearray()
    access, views, primitives, reference = [], [], [], []

    def copy(index):
        a = doc["accessors"][index]
        v = doc["bufferViews"][a["bufferView"]]
        width = {"SCALAR": 1, "VEC2": 2, "VEC3": 3}[a["type"]]
        component = {5123: 2, 5125: 4, 5126: 4}[a["componentType"]]
        item = width * component
        assert v.get("buffer", 0) == 0 and v.get("byteStride", item) == item
        start = v.get("byteOffset", 0) + a.get("byteOffset", 0)
        values = raw[start : start + a["count"] * item]
        assert len(values) == a["count"] * item
        binary.extend(bytes(-len(binary) % 4))
        views.append(
            {"buffer": 0, "byteOffset": len(binary), "byteLength": len(values)}
        )
        access.append({**a, "bufferView": len(views) - 1, "byteOffset": 0})
        binary.extend(values)
        return len(access) - 1, values

    for number, p in enumerate(mesh["primitives"]):
        assert set(p["attributes"]) == {"POSITION", "NORMAL", "TEXCOORD_0"}
        part = {"material": number, "attributes": {}}
        hashes = dict.fromkeys(("COLOR_0", "COLOR_1", "_OSAGE_SOURCE_COLOR_0"))
        for key, index in p["attributes"].items():
            slot, values = copy(index)
            part["attributes"][key] = slot
            assert access[slot]["componentType"] == 5126
            hashes[key] = sha(values)
        i, values = copy(p["indices"])
        part["indices"] = i
        if access[i]["componentType"] == 5123:
            converted = array.array(
                "I", (x for (x,) in struct.iter_unpack("<H", values))
            )
            assert converted.itemsize == 4
            if sys.byteorder != "little":
                converted.byteswap()
            values = converted.tobytes()
        else:
            assert access[i]["componentType"] == 5125
        hashes.update(
            {
                "indices": sha(values),
                "vertices": access[part["attributes"]["POSITION"]]["count"],
                "triangles": access[i]["count"] // 3,
            }
        )
        primitives.append(part)
        reference.append(hashes)
    mats = [
        {
            "doubleSided": True,
            "pbrMetallicRoughness": {
                "baseColorFactor": c,
                "metallicFactor": 0,
                "roughnessFactor": 0.9,
            },
        }
        for c in ([0.22, 0.10, 0.05, 1.0], [0.10, 0.32, 0.13, 1.0])
    ]
    scene = {
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0, 1]}],
        "nodes": [
            {"mesh": 0, "translation": [x, 0, 0], "scale": [2, 2, 2]} for x in (14, 18)
        ],
        "meshes": [{"name": mesh["name"], "primitives": primitives}],
        "materials": mats,
        "accessors": access,
        "bufferViews": views,
        "buffers": [{"byteLength": len(binary)}],
    }
    file = out / "two-pines.glb"
    file.write_bytes(glb(scene, binary))
    subprocess.run(
        [sys.executable, HERE / "cook-pages.py", file, out / "pages", "--edge", "32"],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    replay = json.loads((HERE / "fixtures/descent.json").read_bytes())
    replay["viewport"] = [320, 180]
    replay["routes"] = [
        {
            "id": "pines",
            "ticks": 60,
            "start_m": [16, 1.2, 1],
            "velocity_m_s": [0, 0, 0],
            "expected_support": "none",
            "camera": [
                {"tick": t, "eye": [16, 2.1, 8.5], "aim": [16, 1.35, 0]}
                for t in (0, 60)
            ],
        }
    ]
    store(out / "replay.json", replay)
    return {
        "source_name": mesh["name"],
        "source_gltf_sha256": sha(src.read_bytes()),
        "source_bin_sha256": sha(raw),
        "parts": reference,
    }


def altered_pages(out, name, idx, resource):
    target = out / ("invalid-" + name)
    target.mkdir(exist_ok=True)
    # Only this directory's index and newly addressed resources change.
    for f in (out / "pages").iterdir():
        if f.name != "index.json" and not (target / f.name).exists():
            os.link(f, target / f.name)
    altered = json.loads(json.dumps(idx))
    if name == "slots":
        altered["model_resources"][0]["primitives"] += 1
        for v in altered["model_variants"]:
            v["materials"].append(v["materials"][0])
    elif name in ("root", "triangles"):
        model = altered["model_resources"][0]
        inner = zlib.decompress((target / resource["file"]).read_bytes()[8:])
        chunks = struct.unpack_from("<I", inner, 12)[0]
        scene = json.loads(inner[20 : 20 + chunks])
        if name == "root":
            scene["nodes"][0]["translation"] = [1, 0, 0]
        else:
            indices = scene["meshes"][0]["primitives"][0]["indices"]
            scene["accessors"][indices]["count"] -= 3
        data = glb(scene, inner[28 + chunks :])
        rekey(target, model, b"OMD1", data, "model")
    elif name in ("owner", "reference"):
        for tile in altered["tiles"]:
            v = tile["visual"]
            plain = zlib.decompress((target / v["file"]).read_bytes()[8:])
            cut = struct.unpack_from("<I", plain)[0] + 4
            records = json.loads(plain[4:cut])
            assert records and all(isinstance(entry, list) for entry in records)
            for entry in records:
                if name == "owner":
                    entry[1] = len(idx["source_parents"])
                else:
                    entry[0] = len(idx["model_variants"])
            data = json.dumps(records, separators=(",", ":")).encode()
            rekey(
                target,
                v,
                b"OVP1",
                struct.pack("<I", len(data)) + data + plain[cut:],
                "vpage",
            )
    else:
        raise ValueError(name)
    store(target / "index.json", altered)
    return target


def rekey(target, meta, magic, data, ext):
    encoded = magic + struct.pack("<I", len(data)) + zlib.compress(data, 9)
    h = sha(encoded)
    name = f"{h}.{ext}"
    (target / name).write_bytes(encoded)
    meta.update({"bytes": len(encoded), "sha256": h, "file": name})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    parser.add_argument("--source", type=Path, default=ASSET)
    parser.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/model-decode"
    )
    parser.add_argument("--pairs", type=int, default=5)
    parser.add_argument(
        "--cpus", help="optional Linux CPU list passed to taskset for both processes"
    )
    parser.add_argument(
        "--frames",
        action="store_true",
        help="capture both modes and the rejected-model control on the GPU",
    )
    a = parser.parse_args()
    if a.pairs < 1:
        parser.error("--pairs must be positive")
    out = a.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    fixture = source_geometry(out, a.source)
    pages = out / "pages"
    idx = json.loads((pages / "index.json").read_bytes())
    [resource] = idx["model_resources"]
    assert len(idx["tiles"]) == 2 and len(idx["model_variants"]) == 1
    assert resource["triangles"] == sum(p["triangles"] for p in fixture["parts"])
    assert resource["primitives"] == len(fixture["parts"])
    expected_bytes = sum(
        p["vertices"] * 32 + p["triangles"] * 12 for p in fixture["parts"]
    )
    exe = a.bin.resolve()

    def run(label, directory, mode, *, frames=False, fingerprints=False):
        logfile = out / (label + ".log")
        receipt = out / (label + ".json")
        cmd = [
            exe,
            "--paging",
            "--wall-clock",
            "--pages",
            directory,
            "--route",
            "pines",
            "--replay",
            out / "replay.json",
            "--visual-radius",
            "24",
            "--tick",
            "40",
            "--model-decode",
            mode,
            "--out",
            receipt,
        ]
        if a.cpus:
            cmd = ["taskset", "-c", a.cpus, *cmd]
        if fingerprints:
            cmd += ["--model-fingerprints"]
        if frames:
            cmd = [
                "flock",
                "-w",
                "450",
                "/tmp/osage-ville-paris-gpu.lock",
                *cmd,
                "--gpu-stride",
                "40",
                "--gpu-trace",
                out / (label + "-gpu.json"),
                "--shot",
                out / (label + ".png"),
            ]
        with logfile.open("w") as log:
            subprocess.run(cmd, stdout=log, stderr=subprocess.STDOUT, check=True)
        return json.loads(receipt.read_bytes())

    def verify(r, transport, mode, check_arrays=False):
        published = r["published"]
        assert (
            published["shared_model_instances"] == 2
            and published["fine_triangles"] == 2 * resource["triangles"]
        )
        assert (
            published["distinct_shared_resource_meshes"] == 1
            and published["fine_tiles"] == 2
        )
        m = r["storage"]["model_io"]
        wire = r["transport"]["io"]
        assert r["transport"]["kind"] == transport and wire[
            "worker_model_decodes"
        ] == m["worker_prepared"] == int(mode == "worker")
        assert (
            m["reads"] == 1
            and m["read_bytes"] == resource["bytes"]
            and m["rejected"] == 0
        )
        assert (
            m["resident_models"] == 1 and m["resident_buffer_bytes"] == expected_bytes
        )
        assert (m["simulation_decode_us"] == 0) == (mode == "worker")
        events = [
            e
            for e in r["storage"]["model_events"]
            if e["sha256"] == resource["sha256"] and e["state"] == "ready"
        ]
        assert len(events) == 1
        if check_arrays:
            [actual] = r["storage"]["model_fingerprints"]
            assert (
                actual["resource_sha256"] == resource["sha256"]
                and actual["parts"] == fixture["parts"]
            )
            assert actual["owner_variants"] == [[0, 0], [1, 0]]
        amount = (
            wire["disk_content_bytes"]
            if transport == "disk"
            else wire["content_body_bytes"]
        )
        return published, {
            "model_ready_tick": events[0]["tick"],
            "transport_content_bytes": amount,
            "simulation_decode_us": m["simulation_decode_us"],
            "worker_decode_us": wire["worker_model_decode_us"],
            "simulation_publish_us": m["simulation_publish_us"],
            "simulation_worst_tick_us": r["transport"]["tick_work_us"]["max"],
            "tick_p99_us": r["transport"]["tick_work_us"]["p99"],
        }

    samples = {t: {m: [] for m in ("worker", "inline")} for t in ("disk", "http")}
    state = None
    with ExitStack() as stack:
        url, requests = stack.enter_context(origin(pages))
        for pair in range(a.pairs + 1):
            for transport, location in (("disk", pages), ("http", url)):
                order = (
                    ("worker", "inline")
                    if (pair + int(transport == "http")) % 2 == 0
                    else ("inline", "worker")
                )
                for mode in order:
                    label = f"{transport}-{pair}-{mode}"
                    before = len(requests)
                    r = run(label, location, mode, fingerprints=pair == 0)
                    published, times = verify(r, transport, mode, pair == 0)
                    if state is None:
                        state = published
                    assert state == published
                    if transport == "http":
                        assert (
                            sum(
                                q["sha256"] == resource["sha256"]
                                for q in requests[before:]
                            )
                            == 1
                        )
                    if pair:
                        samples[transport][mode].append(times)
    totals = {
        x["transport_content_bytes"]
        for t in samples.values()
        for group in t.values()
        for x in group
    }
    assert len(totals) == 1
    negative = {}
    for case in ("slots", "root", "triangles", "owner", "reference"):
        directory = altered_pages(out, case, idx, resource)
        for mode in ("worker", "inline"):
            r = run("invalid-" + case + "-" + mode, directory, mode)
            errors = [
                x["error"]
                for x in (
                    r["transport"]["rejected_visual_pages"]
                    if case in ("owner", "reference")
                    else r["storage"]["rejected_models"]
                )
            ]
            phrase = {
                "slots": "incompatible material slots",
                "root": "invalid shared model asset",
                "triangles": "invalid shared model asset",
                "owner": "unknown owner",
                "reference": "unknown shared model",
            }[case]
            assert errors and all(phrase in e for e in errors), (case, errors)
            assert r["published"] != state, (case, r["published"])
            negative[case + "-" + mode] = errors
    stats = {}
    for t, groups in samples.items():
        stats[t] = {}
        for mode, rows in groups.items():
            stats[t][mode] = {
                k: statistics.median(row[k] for row in rows) for k in rows[0]
            }
            stats[t][mode]["worst_over_all_ticks_us"] = max(
                row["simulation_worst_tick_us"] for row in rows
            )
    with exe.open("rb") as executable:
        executable_hash = hashlib.file_digest(executable, "sha256").hexdigest()
    result = {
        "fixture": fixture,
        "resource": resource,
        "source_placements": 2,
        "buffer_bytes": expected_bytes,
        "published": state,
        "input_content_bytes": totals.pop(),
        "warmup_pairs": 1,
        "pairs": a.pairs,
        "tick_hz": 60,
        "cpus": a.cpus,
        "executable_sha256": executable_hash,
        "negative_controls": negative,
        "samples": samples,
        "medians": stats,
    }
    if a.frames:
        frames = {}
        for mode in ("worker", "inline", "absent"):
            directory = out / "invalid-root" if mode == "absent" else pages
            label = "frame-" + mode
            r = run(
                label,
                directory,
                "inline" if mode == "inline" else "worker",
                frames=True,
            )
            first, last = json.loads((out / (label + "-gpu.json")).read_bytes())[
                "frames"
            ]
            assert first["fine_cells"] == 0 and first["draw_owners"] == 2
            if mode != "absent":
                verify(r, "disk", mode)
                assert (
                    last["shared_draw_owners"] == 2 and last["shared_primitives"] == 2
                )
            else:
                assert last["fine_cells"] == 0 and last["draw_owners"] == 2
            size, frames[mode] = rgb(out / (label + ".png"))
        assert frames["worker"] == frames["inline"]
        actual, control = frames["worker"], frames["absent"]
        changed = sum(
            actual[i : i + 3] != control[i : i + 3] for i in range(0, len(actual), 3)
        )
        assert changed > 100
        result["gpu"] = {
            "size": size,
            "rgb_sha256": sha(actual),
            "absent_rgb_sha256": sha(control),
            "worker_inline_changed_pixels": 0,
            "changed_pixels_from_absent": changed,
        }
    store(out / "summary.json", result)
    print(json.dumps(stats, indent=2))


if __name__ == "__main__":
    main()
