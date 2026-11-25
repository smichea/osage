#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Check embedded, startup-bank and demand-loaded geometry using actual file reads."""

import argparse
import hashlib
import json
import runpy
import shutil
import struct
import subprocess
import sys
import tempfile
import zlib
from collections import Counter
from pathlib import Path

from paging_checks import glb, require

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


def pack_json(obj):
    return json.dumps(obj, separators=(",", ":"), allow_nan=False).encode()


def payload(raw):
    length = struct.unpack_from("<I", raw, 4)[0]
    body = zlib.decompress(raw[8:])
    require(len(body) == length, "packed length mismatch")
    return body


def mesh_table(raw, materials=None):
    doc, tail = glb(raw)
    blob = tail[8:]
    models = []
    for mesh in doc["meshes"]:
        h = hashlib.sha256()
        total = 0
        for slot, prim in enumerate(mesh["primitives"]):
            mat = pack_json(
                materials[slot]
                if materials is not None
                else doc["materials"][prim["material"]]
            )
            h.update(struct.pack("<I", len(mat)) + mat)
            for name, a in [
                *sorted(prim["attributes"].items()),
                ("indices", prim["indices"]),
            ]:
                h.update(name.encode())
                accessor = doc["accessors"][a]
                v = doc["bufferViews"][accessor["bufferView"]]
                start = v.get("byteOffset", 0) + accessor.get("byteOffset", 0)
                size = v["byteLength"]
                data = blob[start : start + size]
                h.update(
                    pack_json(
                        [
                            accessor["type"],
                            accessor["componentType"],
                            accessor["count"],
                            accessor.get("normalized", False),
                            len(data),
                        ]
                    )
                )
                h.update(data)
                total += size
        models.append((h.hexdigest(), total))
    return doc, models


def owner_meshes(raw):
    doc, meshes = mesh_table(raw)
    placed = []
    for i in doc["scenes"][doc["scene"]]["nodes"]:
        n = doc["nodes"][i]
        pose = {
            k: v
            for k, v in n.items()
            if k in ("matrix", "rotation", "translation", "scale")
        }
        placed.append((n["extras"]["osage_source_node"], meshes[n["mesh"]][0], pose))
    return placed, meshes


def mesh_receipt(directory):
    idx = json.loads((directory / "index.json").read_bytes())
    demand = idx["format"].endswith("/4")
    shared = demand or idx["format"].endswith("/3")
    bank = []
    if "models" in idx:
        _, bank = mesh_table(payload((directory / idx["models"]["file"]).read_bytes()))
    stored_models = list(bank)
    if demand:
        resources = [
            payload((directory / r["file"]).read_bytes())
            for r in idx["model_resources"]
        ]
        stored_models = [mesh_table(raw)[1][0] for raw in resources]
        for v in idx["model_variants"]:
            palette = [idx["material_palette"][i] for i in v["materials"]]
            bank.append(mesh_table(resources[v["resource"]], palette)[1][0])
    same_codec_bytes = 0
    size_by_hash = {}
    records = Counter()
    render = {}
    glb_json = 0
    for tile in idx["tiles"]:
        raw = (directory / tile["visual"]["file"]).read_bytes()
        if shared:
            body = payload(raw)
            n = struct.unpack_from("<I", body)[0]
            order = json.loads(body[4 : 4 + n])
            local = body[4 + n :]
            arr, meshes = owner_meshes(local) if local else ([], [])
            if demand:
                uses = sorted(
                    {
                        idx["model_variants"][r[0]]["resource"]
                        for r in order
                        if isinstance(r, list)
                    }
                )
                require(
                    tile["visual"]["uses"] == uses, "dependency list differs from draws"
                )
            placed = [
                arr[r] if isinstance(r, int) else (r[1], bank[r[0]][0], r[2])
                for r in order
            ]
        else:
            placed, meshes = owner_meshes(raw)
            glb_json += struct.unpack_from("<I", raw, 12)[0]
            same_codec_bytes += 8 + len(zlib.compress(raw, 6))
        render[tile["x"], tile["z"]] = placed
        for h, s in meshes:
            records[h] += 1
            size_by_hash[h] = s
    for h, s in stored_models:
        records[h] += 1
        size_by_hash[h] = s
    duplicate = sum((n - 1) * size_by_hash[h] for h, n in records.items())
    files = {
        idx["parent"]["file"]: idx["parent"]["bytes"],
        idx["support"]["file"]: idx["support"]["bytes"],
    }
    if "models" in idx:
        files[idx["models"]["file"]] = idx["models"]["bytes"]
    for resource in idx.get("model_resources", []):
        files[resource["file"]] = resource["bytes"]
    for t in idx["tiles"]:
        for k in ("visual", "physics"):
            if k in t:
                files[t[k]["file"]] = t[k]["bytes"]
    return (
        render,
        idx,
        {
            "indexed_file_bytes": (directory / "index.json").stat().st_size
            + sum(files.values()),
            "fine_page_bytes": sum(t["visual"]["bytes"] for t in idx["tiles"]),
            "model_files": len(stored_models) if demand else int(bool(bank)),
            "model_variants": len(bank),
            "model_resource_bytes": sum(
                r["bytes"] for r in idx.get("model_resources", [])
            ),
            "embedded_fine_same_codec_bytes": None if shared else same_codec_bytes,
            "coarse_bytes": idx["parent"]["bytes"],
            "model_bank_bytes": idx.get("models", {}).get("bytes", 0),
            "index_bytes": (directory / "index.json").stat().st_size,
            "physical_bytes": sum(
                t["physics"]["bytes"] for t in idx["tiles"] if "physics" in t
            ),
            "support_bytes": idx["support"]["bytes"],
            "fine_draw_placements": sum(map(len, render.values())),
            "mesh_records": sum(records.values()),
            "unique_meshes": len(records),
            "unique_geometry_buffer_bytes": sum(size_by_hash.values()),
            "duplicate_geometry_buffer_bytes": duplicate,
            "geometry_buffer_bytes": sum(
                records[h] * s for h, s in size_by_hash.items()
            ),
            "embedded_page_json_bytes": glb_json if not shared else None,
            "most_duplicated": [
                {
                    "mesh_sha256": h,
                    "copies": records[h],
                    "single_buffer_bytes": size_by_hash[h],
                }
                for h in sorted(
                    records,
                    key=lambda k: (records[k] - 1) * size_by_hash[k],
                    reverse=True,
                )[:3]
            ],
        },
    )


def create_pair(src, replay, out):
    doc, tail = glb(src.read_bytes())
    mesh = next(i for i, m in enumerate(doc["meshes"]) if "facade" in m.get("name", ""))
    floor = next(n for n in doc["nodes"] if n.get("name") == "support/rail")["mesh"]
    facade = {
        "name": "facade",
        "mesh": mesh,
        "scale": [36, 36, 22],
        "translation": [0, 24, 0],
    }
    doc["nodes"] = [
        {"children": [1, 3, 5]},
        {"name": "block", "translation": [-128, 0, 0], "children": [2]},
        facade,
        {"name": "block", "translation": [128, 0, 0], "children": [4]},
        facade,
        {
            "name": "support/reference",
            "mesh": floor,
            "translation": [800, 0, 800],
            "scale": [4, 1, 4],
        },
    ]
    doc["scenes"] = [{"nodes": [0]}]
    doc["scene"] = 0
    desc = pack_json(doc)
    desc += b" " * (-len(desc) % 4)
    out.mkdir(parents=True, exist_ok=True)
    pair = out / "pair.glb"
    pair.write_bytes(
        struct.pack(
            "<4sIIII", b"glTF", 2, 20 + len(desc) + len(tail), len(desc), 0x4E4F534A
        )
        + desc
        + tail
    )
    camera = [
        {"tick": t, "eye": [x, 24, 12], "aim": [x, 24, 0]}
        for t, x in [(0, -128), (20, -128), (21, 128), (40, 128)]
    ]
    common = {
        "id": "pair",
        "ticks": 40,
        "start_m": [0, 400, 0],
        "velocity_m_s": [0, 0, 0],
        "expected_support": "reference",
        "camera": camera,
    }
    render = {
        **common,
        "id": "both",
        "camera": [
            {"tick": t, "eye": [0, 85, 260], "aim": [0, 25, 0]} for t in [0, 40]
        ],
    }

    def camera_route(id, positions, start=None):
        return {
            **common,
            "id": id,
            "ticks": positions[-1][0],
            "start_m": start or common["start_m"],
            "camera": [
                {"tick": t, "eye": [x, y, 12], "aim": [x, y, 0]}
                for t, x, y in positions
            ],
        }

    sky = camera_route("sky", [(0, 0, 400), (160, 0, 400)])
    simultaneous = camera_route(
        "simultaneous", [(0, 128, 24), (160, 128, 24)], [-128, 24, 0]
    )
    leave = camera_route(
        "leave-return",
        [
            (0, -128, 24),
            (20, -128, 24),
            (21, 0, 400),
            (100, 0, 400),
            (101, 128, 24),
            (160, 128, 24),
        ],
    )
    path = out / "pair.json"
    path.write_text(
        json.dumps(
            {
                **replay,
                "gravity_m_s2": 0.01,
                "routes": [common, render, sky, simultaneous, leave],
            },
            indent=2,
        )
        + "\n"
    )
    return pair, path


def qualify_demand(run, directory, idx, replay, traces, opts):
    model = idx["model_resources"][0]

    def io(r):
        return r["storage"]["model_io"]

    def events(r, kind):
        return [e for e in r["storage"]["model_events"] if e["state"] == kind]

    require(
        len(idx["model_resources"]) == 1
        and all(
            io(traces[k])["reads"] == 0 and io(traces[k])["peak_buffer_bytes"] == 0
            for k in ("sky", "bootstrap")
        ),
        "an unused model was read or decoded",
    )
    both = traces["simultaneous"]
    requested = [
        e for e in both["visual_events"] if e["state"] == "requested" and e["tick"] == 0
    ]
    require(
        len(requested) == 2
        and len(events(both, "requested")) == 1
        and io(both)["reads"] == 1
        and io(both)["read_bytes"] == model["bytes"],
        "two simultaneously demanded pages did not share one actual model read",
    )
    require(
        both["published"]["shared_model_instances"] == 2
        and both["published"]["distinct_shared_resource_meshes"] == 1,
        "shared resource was copied for two live draws",
    )
    leave = traces["leave-return"]
    require(
        io(leave)["reads"] == 2
        and io(leave)["evictions"] == 1
        and io(leave)["read_bytes"] == 2 * model["bytes"],
        "return after eviction did not reread geometry",
    )
    gap = run(
        "demand-no-resident", directory, "leave-return", replay, [*opts, "--tick", 60]
    )
    require(
        io(gap)["resident_buffer_bytes"] == 0 and io(gap)["resident_models"] == 0,
        "model remained in RAM after last owner left",
    )
    # Both arm the service at tick zero. On a narrow link a placement arrives first.
    slow = [
        *opts,
        "--delay-ms",
        600,
        "--mbps-per-kind",
        1,
        "--watch",
        "-1,0",
        "--outage",
        "38,48",
    ]
    resolved = run("demand-delayed-model", directory, "simultaneous", replay, slow)
    ready = events(resolved, "ready")[0]["tick"]
    decoded = next(
        e["tick"]
        for e in resolved["visual_events"]
        if e["state"] == "decoded" and e["tile"] == [-1, 0]
    )
    require(
        decoded < ready and ready >= 48 and resolved["visual"]["peak_in_flight"] <= 4,
        "model must occupy the same bounded transfer service as its placement",
    )
    staged = run(
        "demand-delayed-before",
        directory,
        "simultaneous",
        replay,
        [*slow, "--tick", ready - 1],
    )
    require(
        staged["storage"]["decoded_pages_waiting"] > 0
        and not staged["published"]["watch_is_fine"]
        and staged["published"]["coarse_instances"] > 0
        and resolved["published"]["watch_is_fine"],
        "partial model was visible during transfer",
    )
    with tempfile.TemporaryDirectory() as tmp:
        dest = Path(tmp)
        for f in directory.iterdir():
            if f.is_file():
                shutil.copyfile(f, dest / f.name)
        asset = dest / model["file"]
        broken = bytearray(asset.read_bytes())
        broken[-1] ^= 1
        asset.write_bytes(broken)
        bad = run(
            "demand-rejected-model",
            dest,
            "simultaneous",
            replay,
            [*opts, "--watch", "-1,0"],
        )
        require(
            io(bad)["rejected"] >= 2
            and io(bad)["reads"] == io(bad)["rejected"]
            and io(bad)["read_bytes"] == io(bad)["reads"] * model["bytes"]
            and not bad["published"]["watch_is_fine"]
            and bad["published"]["coarse_instances"] > 0
            and io(bad)["resident_buffer_bytes"] == 0,
            "corrupt model was installed or escaped metering/retry",
        )
    return {
        "two_page_requests_tick_0": len(requested),
        "simultaneous_model_reads": io(both)["reads"],
        "model_bytes": model["bytes"],
        "live_cpu_geometry_bytes": io(both)["peak_buffer_bytes"],
        "evicted_cpu_geometry_bytes_remaining": io(gap)["resident_buffer_bytes"],
        "reads_after_return": io(leave)["reads"],
        "placement_decoded_tick": decoded,
        "model_ready_tick": ready,
        "outage": [38, 48],
        "combined_peak_in_flight": resolved["visual"]["peak_in_flight"],
        "corrupt_attempts": io(bad)["reads"],
        "corrupt_bytes_read": io(bad)["read_bytes"],
    }


def qualify_vertex_channels():
    helpers = runpy.run_path(str(HERE / "cook-pages.py"))
    glb_type, part, channel = (helpers[k] for k in ("Glb", "Part", "Channel"))
    palette = [
        {"pbrMetallicRoughness": {"baseColorFactor": [1, 0, 0, 1]}},
        {"pbrMetallicRoughness": {"baseColorFactor": [0, 1, 0, 1]}},
    ]
    source = glb_type(palette)
    points = [(0, 0, 0), (1, 0, 0), (0, 0, 1)]
    channels = {
        "COLOR_0": channel(
            "VEC4", 5121, [(0, 0, 0, 255), (255, 0, 0, 255), (0, 255, 0, 255)]
        ),
        "TEXCOORD_0": channel("VEC2", 5126, [(0, 0), (1, 0), (0, 1)]),
    }
    tri = part(0, points, [(0, 1, 0)] * 3, [0, 2, 1], channels)
    # Only the other source mesh ID shares these bytes across the two distinct cells.
    for i, x in enumerate((-128, 128)):
        source.node(i, [tri._replace(material=i)], {"translation": [x, 0, 0]})
    source.node(2, [tri._replace(positions=[(0, 0, 0), (128, 0, 0), (0, 0, 32)])], {})
    with tempfile.TemporaryDirectory() as tmp:
        wd = Path(tmp)
        src = wd / "attributes.glb"
        src.write_bytes(source.bytes())
        pages = wd / "pages"
        helpers["cook"](src, pages, 32)
        index = json.loads((pages / "index.json").read_bytes())
        require(
            len(index["model_resources"]) == 1 and len(index["model_variants"]) == 2,
            "equal geometry with two single-use source IDs was duplicated",
        )
        variants = index["model_variants"]
        require(
            [index["material_palette"][v["materials"][0]] for v in variants] == palette,
            "per-instance appearance lost",
        )
        mesh = wd / "part.glb"
        mesh.write_bytes(
            payload((pages / index["model_resources"][0]["file"]).read_bytes())
        )
        _, saved, _ = helpers["source"](mesh)
        require(
            saved[0][0].channels == channels,
            "unclipped normalized integer colours or UV changed",
        )
        count = 0
        seams = set()
        for t in index["tiles"]:
            blob = payload((pages / t["visual"]["file"]).read_bytes())
            end = 4 + struct.unpack_from("<I", blob)[0]
            if end == len(blob):
                continue
            mesh.write_bytes(blob[end:])
            doc, parts, _ = helpers["source"](mesh)
            for node in doc["nodes"]:
                if node["extras"]["osage_source_node"] != 2:
                    continue
                for chunk in parts[node["mesh"]]:
                    color, uv = (chunk.channels[k] for k in ("COLOR_0", "TEXCOORD_0"))
                    require(
                        color.component == uv.component == 5126,
                        "clipped attributes were requantized",
                    )
                    for (x, _, z), c, u in zip(
                        chunk.positions, color.values, uv.values, strict=True
                    ):
                        values = (*c, *u)
                        expected = (x / 128, z / 32, 0, 1, x / 128, z / 32)
                        require(
                            all(abs(a - b) < 1e-6 for a, b in zip(values, expected)),
                            "incorrect UV/colour interpolation at cell boundary",
                        )
                        count += 1
                        if 0 < x < 128:
                            seams.add(x)
        require(
            count > 3 and len(seams) > 1, "wide attributed triangle was not clipped"
        )
    return {
        "equal_resource_distinct_source_mesh_ids": 2,
        "uncut_color_component": "normalized u8x4",
        "uncut_uv_component": "f32x2",
        "checked_clipped_vertices": count,
        "interior_x_coordinates_m": sorted(seams),
    }


def main():
    a = argparse.ArgumentParser(description=__doc__)
    a.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    a.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/model-demand"
    )
    a.add_argument("--district-embedded", type=Path)
    a.add_argument("--district-shared", type=Path)
    a.add_argument("--district-demand", type=Path)
    a.add_argument("--frames", action="store_true")
    args = a.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    exe = args.bin.resolve()

    def fingerprint():
        with exe.open("rb") as f:
            return hashlib.file_digest(f, "sha256").hexdigest()

    binary = fingerprint()
    src = HERE / "fixtures/bir-hakeim.glb"
    rp = HERE / "fixtures/descent.json"
    pair, replay_pair = create_pair(src, json.loads(rp.read_bytes()), out / "pair")

    def cook(name, mode, asset, prepared=None):
        if prepared:
            d = prepared.resolve()
        else:
            d = out / name / mode
            subprocess.run(
                [
                    sys.executable,
                    str(HERE / "cook-pages.py"),
                    str(asset),
                    str(d),
                    "--visual-storage",
                    mode,
                ],
                check=True,
            )
        idx = json.loads((d / "index.json").read_bytes())
        require(
            idx["source_sha256"] == hashlib.sha256(asset.read_bytes()).hexdigest(),
            "wrong source digest",
        )
        return d

    def run(name, d, route, replay, opts):
        dest = out / "runs" / (name + ".json")
        subprocess.run(
            [
                str(exe),
                "--paging",
                "--pages",
                str(d),
                "--out",
                str(dest),
                "--replay",
                str(replay),
                "--route",
                route,
                *map(str, opts),
            ],
            check=True,
        )
        return json.loads(dest.read_bytes())

    data = {}
    metrics = {"vertex_channels": qualify_vertex_channels()}
    for name, asset in (("district", src), ("pair", pair)):
        per = {}
        traces = {}
        imports = {}
        for mode, prepared in (
            ("embedded", args.district_embedded),
            ("shared", args.district_shared),
            ("demand", args.district_demand),
        ):
            d = cook(name, mode, asset, prepared if name == "district" else None)
            render, idx, receipt = mesh_receipt(d)
            imports[mode] = (render, idx)
            if name == "district":
                traces[mode] = {
                    f"{rt}-{radius}m": run(
                        f"{mode}-{rt}-{radius}m",
                        d,
                        rt,
                        rp,
                        [
                            "--delay-ms",
                            600,
                            "--physics-radius",
                            64,
                            "--visual-radius",
                            radius,
                        ],
                    )
                    for rt in ("rail", "road", "roof")
                    for radius in (48, 96)
                }
                traces[mode]["all"] = run(
                    f"{mode}-all", d, "road", rp, ["--all-resident", "--tick", 0]
                )
                if mode == "demand":
                    everything = traces[mode]["all"]["storage"]["model_io"]
                    require(
                        everything["reads"] == len(idx["model_resources"])
                        and everything["read_bytes"] == receipt["model_resource_bytes"],
                        "all-model control did not read exactly the directory",
                    )
            else:
                opts = [
                    "--visual-radius",
                    20,
                    "--physics-radius",
                    0,
                    "--delay-ms",
                    0,
                    "--mbps-per-kind",
                    1000,
                ]
                traces[mode] = {
                    shot: run(
                        f"{mode}-pair-{shot}",
                        d,
                        "pair",
                        replay_pair,
                        [*opts, "--tick", tick],
                    )
                    for shot, tick in [("bootstrap", 0), ("west", 10), ("east", 35)]
                }
                shotopts = ["--all-resident", "--tick", 15]
                if args.frames:
                    shotopts += ["--shot", out / (mode + "-pair.png")]
                traces[mode]["both"] = run(
                    f"{mode}-both", d, "both", replay_pair, shotopts
                )
                for route in ("sky", "simultaneous", "leave-return"):
                    traces[mode][route] = run(
                        f"{mode}-{route}", d, route, replay_pair, opts
                    )
                if mode == "demand":
                    metrics["on_demand"] = qualify_demand(
                        run, d, idx, replay_pair, traces[mode], opts
                    )
            per[mode] = receipt
        left, li = imports["embedded"]
        right, ri = imports["shared"]
        demand, di = imports["demand"]
        require(
            left == right == demand and li["source_parents"] == di["source_parents"],
            "model ancestry, draw order, material, mesh or transform changed",
        )
        require(
            li["support"]["sha256"] == di["support"]["sha256"]
            and [t.get("physics") for t in li["tiles"]]
            == [t.get("physics") for t in di["tiles"]],
            "demand changed collision pages",
        )
        require(
            li["source_parents"] == ri["source_parents"]
            and left.keys() == right.keys(),
            "source tree or spatial keys changed",
        )
        require(
            {(t["x"], t["z"]): t.get("physics", {}).get("sha256") for t in li["tiles"]}
            == {
                (t["x"], t["z"]): t.get("physics", {}).get("sha256")
                for t in ri["tiles"]
            },
            "physical triangles changed",
        )
        require(
            li["support"]["sha256"] == ri["support"]["sha256"],
            "safety catalogue changed",
        )
        if name == "district":
            require(
                left == right,
                "owner, local draw model, material, order or placement changed in district",
            )
            for rt in traces["embedded"]:
                a, b, c = (traces[k][rt] for k in ("embedded", "shared", "demand"))
                require(
                    a["first_contact"] == b["first_contact"] == c["first_contact"]
                    and a["final_foot_m"] == b["final_foot_m"] == c["final_foot_m"],
                    "contact changed on " + rt,
                )
        else:
            require(
                [n for n, _, _ in right[-1, 0]] == [2]
                and [n for n, _, _ in right[1, 0]] == [4]
                and right[-1, 0][0][1] == right[1, 0][0][1],
                "two owners do not share the same draw model",
            )
            require(
                ri["source_parents"] == [None, 0, 1, 0, 3, 0],
                "source hierarchy changed",
            )
            for mode in traces:
                require(
                    [
                        traces[mode][s]["visual"]["reads"]
                        for s in ("bootstrap", "west", "east")
                    ]
                    == [0, 1, 2],
                    "extra/missing tile reads in pair",
                )
            shared = traces["shared"]
            require(
                shared["both"]["published"]["shared_model_instances"] == 2
                and shared["both"]["published"]["distinct_shared_draw_models"] == 1,
                "both live instances must refer to one renderer mesh set",
            )
            require(
                shared["east"]["storage"]["model_library_reads"]
                == shared["west"]["storage"]["model_library_reads"]
                == 1,
                "second placement reread library",
            )
            if args.frames:
                # Compare decoded pixels, independent of image container metadata.
                def pixels(path):
                    return subprocess.check_output(
                        [
                            "ffmpeg",
                            "-loglevel",
                            "error",
                            "-i",
                            str(path),
                            "-f",
                            "rawvideo",
                            "-pix_fmt",
                            "rgb24",
                            "-threads",
                            "1",
                            "-",
                        ]
                    )

                l, r, d = (
                    pixels(out / (mode + "-pair.png"))
                    for mode in ("embedded", "shared", "demand")
                )
                different = sum(
                    l[i : i + 3] != r[i : i + 3] for i in range(0, len(r), 3)
                )
                require(l == r == d, "same camera/objects produced different pixels")
                metrics["pair_frame"] = {
                    "changed_pixels": different,
                    "rgb_sha256": hashlib.sha256(r).hexdigest(),
                    "encoded_files": [
                        "embedded-pair.png",
                        "shared-pair.png",
                        "demand-pair.png",
                    ],
                }
        for mode, rs in traces.items():
            per[mode]["cold_replay_bytes"] = {
                k: r["storage"]["cold_file_bytes"] for k, r in rs.items()
            }
            per[mode]["model_library_reads"] = {
                k: r["storage"]["model_library_reads"] for k, r in rs.items()
            }
            per[mode]["visual_cpu_buffers"] = {
                k: r["storage"]["visual_cpu_buffers"] for k, r in rs.items()
            }
            per[mode]["model_io"] = {k: r["storage"]["model_io"] for k, r in rs.items()}
            if name == "pair":
                per[mode]["second_instance_additional_bytes"] = (
                    rs["east"]["storage"]["cold_file_bytes"]
                    - rs["west"]["storage"]["cold_file_bytes"]
                )
        data[name] = per
    require(fingerprint() == binary, "binary changed during qualification")
    summary = {
        "format": "ville-paris/model-sharing/2",
        "binary_sha256": binary,
        "data": data,
        "checked": metrics,
        "cold_definition": "Application starts empty. Counts actual encoded reads of index, parent, safety catalogue, placements, collision and model resources. Model memory is live normalized CPU vertex, attribute and index payload. It excludes allocator, GPU and metadata overhead. No OS cache flush.",
    }
    (out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
