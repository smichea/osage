#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Exercise demand-paged per-owner LOD, delayed publication, geometry and channel identity."""

import argparse
import hashlib
import json
import runpy
import shutil
import struct
import subprocess
import tempfile
import zlib
from pathlib import Path

from paging_checks import require

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
COOK = runpy.run_path(str(HERE / "cook-pages.py"))
PNG = runpy.run_path(str(HERE / "check-texture-storage.py"))["png"]


def read(path):
    return json.loads(path.read_bytes())


def write(path, obj):
    path.write_text(json.dumps(obj, indent=2) + "\n")


def sha(data):
    return hashlib.sha256(data).hexdigest()


def recipe(out):
    colours = [[70, 200, 92, 255], [229, 63, 55, 255]]
    materials = [
        {
            "name": name,
            "alphaMode": "MASK",
            "doubleSided": True,
            "alphaCutoff": 0.25,
            "pbrMetallicRoughness": {
                "metallicFactor": 0,
                "roughnessFactor": 1,
                "baseColorTexture": {"index": i},
            },
        }
        for i, name in enumerate(("near", "far"))
    ]
    g = COOK["Glb"](materials + [{}])
    g.doc["images"] = []
    for colour in colours:
        encoded = PNG(bytes(colour) * 1024)
        g.doc["images"].append(
            g.embed_image(COOK["Bitmap"](encoded, "image/png", sha(encoded)))
        )
    g.doc["textures"] = [{"source": 0}, {"source": 1}]
    g.doc["asset"]["extras"] = {
        "osageObject": {
            "version": 1,
            "id": "floor",
            "collisionMaterials": {"stone": {"friction": 0.6, "restitution": 0.0}},
            "acousticMaterials": {
                "slate": {
                    "absorption": [0.1, 0.2, 0.3],
                    "transmission": [0.2, 0.1, 0],
                    "scattering": 0.3,
                },
                "felt": {},
            },
        }
    }

    def mesh(y, indices, material, colours):
        p = [(-2, y, -2), (-2, y, 2), (2, y, 2), (2, y, -2)]
        channels = {}
        if colours:
            ch = COOK["Channel"]
            channels = {
                "COLOR_0": ch("VEC4", 5121, [(255, 255, 255, 255)] * 4),
                "COLOR_1": ch("VEC4", 5121, [(13, 57, 3 * i, 0) for i in range(4)]),
                "_OSAGE_SOURCE_COLOR_0": ch(
                    "VEC4", 5121, [(255, 255, 255, 156 + 30 * i) for i in range(4)]
                ),
                "TEXCOORD_0": ch("VEC2", 5126, [(0, 0), (0, 1), (1, 1), (1, 0)]),
            }
        return COOK["Part"](material, p, [(0, 1, 0)] * 4, indices, channels)

    quad = [0, 1, 2, 0, 2, 3]
    for i, part in enumerate(
        (
            mesh(1.25, quad, 0, True),
            mesh(1.25, quad[:3], 1, True),
            mesh(1.25, quad, 2, False),
            mesh(0.75, quad, 2, False),
        )
    ):
        g.node(i, [part], {})
    g.doc["nodes"] = [
        {
            "extras": {"osage": {"id": "building"}},
            "children": [1],
            "translation": [0.25, 0, 0.5],
        },
        {
            "mesh": 0,
            "translation": [-0.25, 0, -0.5],
            "extras": {
                "osage": {
                    "id": "slab",
                    "visualLods": [
                        {"level": "medium", "mesh": 1, "maxDeviationM": 3.0}
                    ],
                    "collision": [{"id": "landing", "mesh": 2, "material": "stone"}],
                    "acoustic": [
                        {
                            "id": "stone-echo",
                            "mesh": 2,
                            "source": "collision-proxy",
                            "material": "slate",
                        },
                        {
                            "id": "lining",
                            "mesh": 3,
                            "source": "authored",
                            "material": "felt",
                        },
                    ],
                }
            },
        },
    ]
    g.doc["scenes"][0]["nodes"] = [0]
    out.write_bytes(g.bytes())


def audit(source, directory):
    original, meshes, _ = COOK["source"](source)
    index = read(directory / "index.json")
    (group,) = [g for t in index["tiles"] for g in t["visual"].get("lods", [])]
    variants = index["model_variants"]
    details = []
    for item in group["levels"]:
        v = variants[item["variant"]]
        mesh = v["source_mesh"]
        packed = (
            directory / index["model_resources"][v["resource"]]["file"]
        ).read_bytes()
        require(packed[:4] == b"OMD1", "model container")
        binary = zlib.decompress(packed[8:])
        with tempfile.NamedTemporaryFile(dir=directory, suffix=".glb") as tmp:
            tmp.write(binary)
            tmp.flush()
            doc, actual, _ = COOK["source"](Path(tmp.name))
        src = meshes[mesh]
        require(
            len(actual) == 1 and len(actual[0]) == len(src), "primitive count changed"
        )
        channel_digests = []
        for p, q, prim, src_prim in zip(
            src,
            actual[0],
            doc["meshes"][0]["primitives"],
            original["meshes"][mesh]["primitives"],
        ):
            require(
                (p.positions, p.normals, p.indices, p.channels)
                == (q.positions, q.normals, q.indices, q.channels),
                "model lost geometry or a channel",
            )
            require(
                set(prim["attributes"]) == set(src_prim["attributes"]),
                "source semantics differ",
            )
            for name, ia in src_prim["attributes"].items():
                before, after = (
                    original["accessors"][ia],
                    doc["accessors"][prim["attributes"][name]],
                )
                require(
                    all(
                        before.get(k) == after.get(k)
                        for k in ("componentType", "type", "normalized", "count")
                    ),
                    "attribute encoding changed",
                )
            channel_digests.append(
                {
                    k: sha(
                        b"".join(
                            struct.pack(
                                "<" + COOK["FORMATS"][c.component] * len(val), *val
                            )
                            for val in c.values
                        )
                    )
                    for k, c in q.channels.items()
                }
            )
        details.append(
            {
                "level": item["level"],
                "triangles": sum(len(p.indices) // 3 for p in src),
                "vertices": sum(len(p.positions) for p in src),
                "materials": v["materials"],
                "model_bytes": len(packed),
                "archive_sha256": sha(packed),
                "attribute_digests": channel_digests,
            }
        )
    return index, group, details


def with_document(original, dest, edit):
    raw = original.read_bytes()
    length = struct.unpack_from("<I", raw, 12)[0]
    doc = json.loads(raw[20 : 20 + length])
    edit(doc)
    text = COOK["encode"](doc)
    text += b" " * (-len(text) % 4)
    payload = raw[20 + length :]
    dest.write_bytes(
        struct.pack(
            "<4sIIII", b"glTF", 2, 20 + len(text) + len(payload), len(text), 0x4E4F534A
        )
        + text
        + payload
    )


def forced_source(original, dest, level, owner):
    def select(doc):
        node = doc["nodes"][owner]
        alternatives = node["extras"]["osage"].pop("visualLods")
        if level != "high":
            node["mesh"] = next(v["mesh"] for v in alternatives if v["level"] == level)

    with_document(original, dest, select)


def rgb_pixels(path):
    return subprocess.check_output(
        [
            "ffmpeg",
            "-loglevel",
            "error",
            "-i",
            str(path),
            "-threads",
            "1",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "-",
        ]
    )


def different(a, b):
    return sum(a[i : i + 3] != b[i : i + 3] for i in range(0, len(a), 3))


def run(exe, out, source, gpu, lock, fixture):
    pages = out / "district"
    COOK["cook"](source, pages, 32)
    idx, group, models = audit(source, pages)
    require(
        [x["level"] for x in models] == ["high", "medium"],
        "oracle expects one certified high/medium pair",
    )
    original_owner = group["owner"]
    error = group["levels"][1]["error_m"]
    focal = 90 / (3**-0.5)
    d = focal * error
    near = 5
    itinerary = [
        (0, near),
        (40, near),
        (41, d * 1.6 + 8),
        (85, d * 1.6 + 8),
        (86, d * 0.57 + 4),
        (116, d * 0.57 + 4),
        (117, near),
        (160, near),
    ]
    center = [(a + b) / 2 for a, b in zip(*group["bounds"])]
    trace = []

    def camera(t, z, away=False):
        eye = [center[0], center[1] + (2 if fixture else 0), center[2] + z]
        aim = [center[0], center[1], center[2] + 2 * z if away else center[2]]
        return {"tick": t, "eye": eye, "aim": aim}

    for name, keys, away in (
        ("travel", itinerary, False),
        (
            "band",
            [(0, near), (40, near), (41, d * 0.57 + 4), (160, d * 0.57 + 4)],
            False,
        ),
        ("back", [(0, near), (160, near)], True),
        ("cold-medium", [(0, d * 1.6 + 8), (160, d * 1.6 + 8)], False),
    ):
        trace.append(
            {
                "id": name,
                "ticks": 160,
                "start_m": [1.25, 7, -1],
                "velocity_m_s": [0, 0, 0],
                "expected_support": "landing" if fixture else "none",
                "camera": [camera(t, z, away) for t, z in keys],
            }
        )
    mixed_route = {
        **trace[0],
        "id": "reentry",
        "camera": [
            camera(0, near, True),
            camera(40, near, True),
            camera(41, near),
            camera(160, near),
        ],
    }
    trace.append(mixed_route)
    replays = out / "replay.json"
    write(
        replays,
        {
            "version": 1,
            "tick_hz": 60,
            "gravity_m_s2": 9.81,
            "terminal_m_s": 55,
            "viewport": [320, 180],
            "vertical_fov_deg": 60,
            "routes": trace,
        },
    )

    def invoke(
        tag,
        route="travel",
        tick=160,
        directory=pages,
        extra=(),
        picture=False,
        frames=False,
        ray=fixture,
    ):
        path = out / (tag + ".json")
        command = [
            exe,
            "--paging",
            "--pages",
            directory,
            "--route",
            route,
            "--replay",
            replays,
            "--out",
            path,
            "--visual-radius",
            "3000",
            "--delay-ms",
            "200",
            "--tick",
            str(tick),
            *extra,
        ]
        if ray:
            command.extend(("--acoustic-ray", "1.25,4,-1", "1.25,-1,-1"))
        if frames:
            command.extend(
                ("--gpu-trace", out / (tag + "-gpu.json"), "--gpu-stride", "5")
            )
        if picture:
            command.extend(("--shot", out / (tag + ".png")))
        if picture or frames:
            command = ["flock", lock, *command]
        with (out / (tag + ".log")).open("w") as log:
            subprocess.run(
                list(map(str, command)),
                stdout=log,
                stderr=subprocess.STDOUT,
                check=True,
            )
        return read(path)

    travel = invoke("travel", frames=gpu)
    storage = travel["storage"]
    lod = storage["visual_lods"]
    require(
        [(e["tick"], e["level"]) for e in lod["requests"]]
        == [(0, "high"), (41, "medium"), (117, "high")],
        "projected decision or hysteresis differs",
    )
    require(
        [e["level"] for e in lod["events"]] == ["high", "medium", "high"],
        "published level sequence differs",
    )
    require(
        all(e["owner"] == original_owner for e in lod["events"])
        and all(a["tick"] < b["tick"] for a, b in zip(lod["requests"], lod["events"])),
        "lost ownership or bypassed latency",
    )
    band = invoke("band", "band")
    require(
        [e["level"] for e in band["storage"]["visual_lods"]["requests"]] == ["high"],
        "hysteresis must preserve the opposite history",
    )
    back = invoke("back", "back")
    require(
        back["visual"]["reads"]
        == back["storage"]["model_io"]["reads"]
        == back["storage"]["image_io"]["reads"]
        == 0,
        "cold invisible object causes read",
    )
    far = invoke("cold-medium", "cold-medium")
    require(
        far["storage"]["model_io"]["reads"] == 1
        and far["storage"]["model_io"]["read_bytes"] == models[1]["model_bytes"]
        and far["storage"]["image_io"]["reads"] == 1,
        "cold distant object pulled other levels",
    )
    bad = out / "damaged"
    bad.mkdir(exist_ok=True)
    for path in pages.iterdir():
        if path.is_file():
            shutil.copyfile(path, bad / path.name)
    mid = idx["model_variants"][group["levels"][1]["variant"]]["resource"]
    damaged = bad / idx["model_resources"][mid]["file"]
    value = bytearray(damaged.read_bytes())
    value[-1] ^= 128
    damaged.write_bytes(value)
    repair = invoke(
        "unavailable",
        tick=113,
        directory=bad,
        picture=gpu and fixture,
        frames=gpu and fixture,
    )
    require(
        repair["storage"]["model_io"]["rejected"] > 0
        and [e["level"] for e in repair["storage"]["visual_lods"]["events"]] == ["high"]
        and repair["storage"]["visual_lods"]["owners"][0]["desired"]["level"]
        == "medium",
        "corrupt candidate displaced verified draw",
    )
    report = {
        "source_sha256": idx["source_sha256"],
        "source_owner": original_owner,
        "source_tree": idx["source_parents"],
        "models": models,
        "decisions": lod["requests"],
        "published": lod["events"],
        "io": {key: storage[key] for key in ("model_io", "image_io")},
        "offscreen_cold": {
            "pages": back["visual"]["read_bytes"],
            "models": back["storage"]["model_io"]["read_bytes"],
            "images": back["storage"]["image_io"]["read_bytes"],
        },
        "cold_medium": {
            "model_bytes": far["storage"]["model_io"]["read_bytes"],
            "image_bytes": far["storage"]["image_io"]["read_bytes"],
        },
        "damaged": {
            "rejected": repair["storage"]["model_io"]["rejected"],
            "published": repair["storage"]["visual_lods"]["owners"][0]["published"],
        },
    }
    if fixture:
        cuts = [
            invoke("near-contact", tick=40),
            invoke("mid-contact", tick=100),
            travel,
            back,
        ]
        for result in cuts:
            if result["tick"] >= 100:
                require(
                    result["first_contact"]["support"] == "landing"
                    and result["first_contact"]["source_node"] == 1
                    and result["at_contact"]["source_path"] == [0, 1],
                    "actor lost authored support",
                )
            probe = result["acoustic_probe"]
            require(
                [
                    (h["id"], h["distance_m"], h["owner"], h["source_path"])
                    for h in probe["hits"]
                ]
                == [("stone-echo", 2.75, 1, [0, 1]), ("lining", 3.25, 1, [0, 1])],
                "acoustic surface changed across selection",
            )
        expected = read(out / "travel.json")["acoustic_probe"]
        require(
            all(r["acoustic_probe"] == expected for r in cuts),
            "acoustic coefficients or distance changed",
        )
        require(
            travel["first_contact"] == back["first_contact"] == far["first_contact"],
            "physical contact depends on visual selection",
        )
        standalone = json.loads(
            subprocess.check_output(
                [
                    exe,
                    "--scene",
                    source,
                    "--inspect-object",
                    "--ray",
                    "1.25,4,-1",
                    "1.25,-1,-1",
                ]
            )
        )
        require(
            [
                (h["id"], h["distanceM"], h["coefficients"])
                for h in standalone["ray"]["acoustic"]
            ]
            == [
                (h["id"], h["distance_m"], h["coefficients"]) for h in expected["hits"]
            ],
            "acoustic query differs from standalone",
        )
        require(
            [(h["id"], h["distanceM"]) for h in standalone["ray"]["collision"]]
            == [("landing", 2.75)],
            "source collision mismatch",
        )
        report.update(
            {"first_contact": travel["first_contact"], "acoustic_ray": expected}
        )
        mixed_source = out / "mixed.glb"

        def add_neighbor(doc):
            for material in doc["materials"][:2]:
                material["alphaMode"] = "OPAQUE"
                material.pop("alphaCutoff")
            doc["nodes"][0]["children"].append(2)
            doc["nodes"].append(
                {
                    "mesh": 3,
                    "translation": [5, 0, 0],
                    "extras": {"osage": {"id": "neighbour"}},
                }
            )

        with_document(source, mixed_source, add_neighbor)
        mixed_dir = out / "mixed"
        COOK["cook"](mixed_source, mixed_dir, 32)
        during = invoke("reentry-loading", "reentry", 47, mixed_dir, frames=gpu)
        after = invoke("reentry-verified", "reentry", 75, mixed_dir, frames=gpu)
        require(
            during["published"]["fine_tiles"] == 1
            and during["published"]["coarse_instances"] == 1
            and during["published"]["shared_model_instances"] == 0
            and after["published"]["coarse_instances"] == 0
            and after["published"]["shared_model_instances"] == 1,
            "mixed tile did not retain the visible owner's parent in transit",
        )
        if gpu:
            frames = [
                read(out / (k + "-gpu.json"))["frames"][-1]
                for k in ("reentry-loading", "reentry-verified")
            ]
            require(
                [f["draw_owners"] for f in frames] == [2, 2]
                and [f["shared_draw_owners"] for f in frames] == [0, 1],
                "mixed tile fallback never reached the persistent renderer",
            )
        report["mixed_tile"] = {
            "during": during["published"],
            "after": after["published"],
        }
        miss = invoke(
            "no-nearby-acoustics",
            tick=60,
            ray=False,
            extra=["--acoustic-ray", "85,1,80", "85,-1,80"],
        )
        require(
            miss["acoustic_probe"]["mesh_reads"] == 0
            and not miss["acoustic_probe"]["hits"],
            "distant ray fetched unrelated geometry",
        )
        siblings = out / "siblings.glb"

        def add_sibling(doc):
            child = doc["nodes"][1]
            doc["nodes"][0]["children"].append(2)
            doc["nodes"].append(
                {
                    "mesh": child["mesh"],
                    "translation": [0, 1.25, 0],
                    "scale": [0.001] * 3,
                    "extras": {
                        "osage": {
                            "id": "miniature",
                            "visualLods": child["extras"]["osage"]["visualLods"],
                        }
                    },
                }
            )

        with_document(source, siblings, add_sibling)
        sibling_dir = out / "siblings"
        COOK["cook"](siblings, sibling_dir, 32)
        sibling_index = read(sibling_dir / "index.json")
        require(
            sibling_index["tiles"][0]["visual"]["adaptive_only"],
            "expected only independently selected owners",
        )
        broken = sibling_dir / sibling_index["model_resources"][mid]["file"]
        invalid = bytearray(broken.read_bytes())
        invalid[-1] ^= 128
        broken.write_bytes(invalid)
        independent = invoke(
            "sibling-failure",
            tick=35,
            directory=sibling_dir,
            ray=False,
            picture=gpu,
            frames=gpu,
        )
        owners = independent["storage"]["visual_lods"]["owners"]
        require(
            independent["published"]["shared_model_instances"] == 1
            and owners[0]["published"]["level"] == "high"
            and owners[1]["desired"]["level"] == "medium"
            and owners[1]["published"] is None,
            "one failed owner blocked its verified sibling",
        )
        report["sibling_failure"] = {
            "owners": owners,
            "rejected": independent["storage"]["model_io"]["rejected"],
        }
        detached = out / "detached.glb"

        def move_channels(doc):
            geo = doc["nodes"][1]["extras"]["osage"]
            doc["nodes"][0]["children"].append(2)
            doc["nodes"].append(
                {
                    "translation": [100, 0, 0],
                    "extras": {
                        "osage": {
                            "id": "detached",
                            "collision": geo.pop("collision"),
                            "acoustic": geo.pop("acoustic"),
                        }
                    },
                }
            )

        with_document(source, detached, move_channels)
        detached_dir = out / "detached"
        COOK["cook"](detached, detached_dir, 32)
        separate = invoke(
            "detached",
            tick=80,
            directory=detached_dir,
            ray=False,
            extra=["--all-resident", "--acoustic-ray", "101,4,-1", "101,-1,-1"],
        )
        require(
            not separate["first_contact"]
            and [
                (h["source_path"], h["distance_m"])
                for h in separate["acoustic_probe"]["hits"]
            ]
            == [([0, 2], 2.75), ([0, 2], 3.25)],
            "channel-only owner changed under tiling",
        )
        report["detached"] = {
            "hits": separate["acoustic_probe"]["hits"],
            "visual_reads": separate["visual"]["reads"],
            "physics_reads": separate["physics"]["reads"],
        }
    if gpu:
        gpu_report = read(out / "travel-gpu.json")
        samples = []
        for frame in gpu_report["frames"]:
            t = frame["tick"]
            published = next(
                (ev for ev in reversed(lod["events"]) if ev["tick"] <= t), None
            )
            tier = published["level"] if published else None
            if not samples or samples[-1]["level"] != tier:
                actual = (
                    frame["shared_vertex_slice_bytes"]
                    + frame["shared_index_slice_bytes"]
                )
                require(
                    (tier is None and actual == 0)
                    or (
                        actual > 0
                        and frame["prepared_assets"] >= 1
                        and frame["shared_primitives"] == 1
                        and frame["prepared_textures"] == 1
                    ),
                    "chosen mesh and texture not on GPU",
                )
                samples.append(
                    {
                        "tick": t,
                        "level": tier,
                        "shared_gpu_bytes": actual,
                        "vertex_bytes": frame["shared_vertex_slice_bytes"],
                        "index_bytes": frame["shared_index_slice_bytes"],
                        "texture_bytes": frame["texture_payload_bytes"],
                    }
                )
        require(
            samples[1]["shared_gpu_bytes"] > samples[2]["shared_gpu_bytes"]
            and samples[1]["shared_gpu_bytes"] == samples[3]["shared_gpu_bytes"],
            "GPU did not exchange fine and coarse meshes",
        )
        report["gpu"] = {
            "adapter": gpu_report.get("adapter"),
            "device": gpu_report.get("device"),
            "changes": samples,
        }
        for level in ("high", "medium"):
            fixed = out / ("forced-" + level + ".glb")
            forced_source(source, fixed, level, original_owner)
            COOK["cook"](fixed, out / ("forced-" + level), 32)
        for level, tick in (("high", 35), ("medium", 105)):
            pictures = {}
            other = "medium" if level == "high" else "high"
            for tag, ds in (
                ("adaptive", pages),
                ("forced", out / ("forced-" + level)),
                ("other", out / ("forced-" + other)),
            ):
                label = f"{tag}-{level}"
                invoke(label, tick=tick, directory=ds, picture=True, ray=False)
                pictures[tag] = rgb_pixels(out / (label + ".png"))
            ref = pictures["forced"]
            require(
                ref == pictures["adaptive"],
                f"actual {level} pixels differ from forced topology",
            )
            wrong = different(ref, pictures["other"])
            if level == "high":
                require(wrong > 0, "the close-up cannot distinguish the wrong level")
            report["gpu"].setdefault("images", {})[level] = {
                "changed_rgb_pixels": 0,
                "wrong_level_changed_rgb_pixels": wrong,
                "width": 320,
                "height": 180,
                "rgb_sha256": sha(ref),
            }
        if fixture:
            invoke(
                "unavailable-forced-high",
                tick=113,
                directory=out / "forced-high",
                picture=True,
                ray=False,
            )
            checks = {}
            for case, reference in (
                ("unavailable", "unavailable-forced-high"),
                ("sibling-failure", "forced-high"),
            ):
                image = rgb_pixels(out / (case + ".png"))
                target = rgb_pixels(out / (reference + ".png"))
                frame = read(out / (case + "-gpu.json"))["frames"][-1]
                require(
                    image == target
                    and frame["shared_primitives"] == 1
                    and frame["shared_index_slice_bytes"] == samples[1]["index_bytes"],
                    "a corrupt candidate changed the drawn High mesh or its native image",
                )
                checks[case] = {
                    "tick": frame["tick"],
                    "rgb_sha256": sha(image),
                    "changed_rgb_pixels": 0,
                    "shared_high_index_bytes": frame["shared_index_slice_bytes"],
                }
            wrong = different(
                rgb_pixels(out / "sibling-failure.png"),
                rgb_pixels(out / "other-high.png"),
            )
            require(
                wrong > 0,
                "the close surviving sibling must be visually distinguishable from the wrong mesh",
            )
            checks["sibling-failure"]["wrong_level_changed_rgb_pixels"] = wrong
            report["gpu"]["failed_candidates"] = checks
    with exe.open("rb") as binary:
        report["binary_sha256"] = hashlib.file_digest(binary, "sha256").hexdigest()
    write(out / "summary.json", report)
    return report


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    cli.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/paged-object-lods"
    )
    cli.add_argument(
        "--source",
        type=Path,
        help="Optional independent two-level, single-owner textured model without a collider",
    )
    cli.add_argument("--gpu", action="store_true")
    cli.add_argument(
        "--gpu-lock", type=Path, default=Path("/tmp/osage-ville-paris-gpu.lock")
    )
    a = cli.parse_args()
    out = a.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    source = a.source.resolve() if a.source else out / "fixture.glb"
    if not a.source:
        recipe(source)
    result = run(a.bin.resolve(), out, source, a.gpu, a.gpu_lock, a.source is None)
    print(
        json.dumps(
            {k: result[k] for k in ("source_sha256", "published", "offscreen_cold")}
        )
    )


if __name__ == "__main__":
    main()
