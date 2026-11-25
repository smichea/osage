#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Verify standalone, shared, and on-demand material bitmaps with an authored UV fixture."""

import argparse
import base64
import hashlib
import json
import runpy
import struct
import subprocess
import zlib
from pathlib import Path

from paging_checks import require
from paging_coarse import unpack_coarse

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
COOK = runpy.run_path(str(HERE / "cook-pages.py"))
SIZE = 32


def sha(data):
    return hashlib.sha256(data).hexdigest()


def png(pixels):
    def chunk(name, data):
        return (
            struct.pack(">I", len(data))
            + name
            + data
            + struct.pack(">I", zlib.crc32(name + data))
        )

    rows = b"".join(
        b"\0" + pixels[4 * SIZE * i : 4 * SIZE * (i + 1)] for i in range(SIZE)
    )
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">2I5B", SIZE, SIZE, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(rows, 9))
        + chunk(b"IEND", b"")
    )


def fixtures(out):
    rgba = bytearray()
    rgb = bytearray()
    noise = bytearray()
    for y in range(SIZE):
        for x in range(SIZE):
            colour = (
                (224, 43, 47, 255),
                (34, 199, 73, 255),
                (37, 67, 219, 255),
                (247, 200, 47, 0),
            )[(x >= 16) + 2 * (y >= 16)]
            rgba.extend(colour)
            rgb.extend(
                (
                    (x * 7 + y * 3) % 256,
                    (131 + x * 2 + y * 5) % 256,
                    (238 + y * 6) % 256,
                )
            )
            noise.extend((*hashlib.sha256(bytes((x, y))).digest()[:3], 255))
    jpeg = subprocess.check_output(
        [
            "ffmpeg",
            "-loglevel",
            "error",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgb24",
            "-video_size",
            f"{SIZE}x{SIZE}",
            "-i",
            "pipe:0",
            "-frames:v",
            "1",
            "-q:v",
            "3",
            "-threads",
            "1",
            "-f",
            "mjpeg",
            "pipe:1",
        ],
        input=rgb,
    )
    images = [png(rgba), jpeg, png(rgba), png(noise), png(bytes(reversed(rgba)))]

    def material(name, index, mode):
        return {
            "name": name,
            "alphaMode": mode,
            "doubleSided": True,
            "pbrMetallicRoughness": {
                "baseColorFactor": [1, 1, 1, 1],
                "metallicFactor": 0.0,
                "roughnessFactor": 1.0,
                "baseColorTexture": {"index": index, "texCoord": 0},
            },
        }

    table = [
        material("opaque-png", 0, "OPAQUE"),
        material("cutout-png", 2, "MASK"),
        material("opaque-jpg", 1, "OPAQUE"),
        material("cut-floor", 3, "MASK"),
    ]
    table[1]["alphaCutoff"] = 0.5
    # Use glTF slots, including two source image entries naming identical encoded bytes.
    tex = [
        {"source": 0, "sampler": 0},
        {"source": 1},
        {"source": 2, "sampler": 1},
        {"source": 3, "sampler": 0},
    ]
    samplers = [
        {"magFilter": 9728, "minFilter": 9728, "wrapS": 10497, "wrapT": 10497},
        {"magFilter": 9729, "minFilter": 9729, "wrapS": 33071, "wrapT": 33648},
    ]

    def scene():
        g = COOK["Glb"](table)
        g.doc["images"] = [
            g.embed_image(
                COOK["Bitmap"](img, "image/jpeg" if i == 1 else "image/png", sha(img))
            )
            for i, img in enumerate(images)
        ]
        g.doc["textures"] = tex
        g.doc["samplers"] = samplers
        return g

    uv = COOK["Channel"]("VEC2", 5126, [(0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0)])
    faces = [0, 1, 2, 0, 2, 3]
    quad = COOK["Part"](
        0,
        [(-4.0, -3.0, 0.0), (4.0, -3.0, 0.0), (4.0, 3.0, 0.0), (-4.0, 3.0, 0.0)],
        [(0.0, 0.0, 1.0)] * 4,
        faces,
        {"TEXCOORD_0": uv},
    )
    g = scene()
    for i, (x, m) in enumerate(((0, 0), (96, 0), (192, 1), (288, 2))):
        g.node(
            ("actor", m),
            [quad._replace(material=m)],
            {"name": f"panel-{i}", "translation": [x, 4, 0], "scale": [4, 2, 1]},
        )
    floor = quad._replace(
        material=3,
        positions=[
            (-108.0, 0.0, 3.0),
            (108.0, 0.0, 3.0),
            (108.0, 0.0, -3.0),
            (-108.0, 0.0, -3.0),
        ],
        normals=[(0.0, 1.0, 0.0)] * 4,
    )
    g.node("floor", [floor], {"name": "support/reference", "translation": [0, 0, 192]})
    (out / "textured.glb").write_bytes(g.bytes())
    isolated = scene()
    translucent = material("translucent-png", 2, "BLEND")
    translucent["pbrMetallicRoughness"]["baseColorFactor"][3] = 0.5
    isolated.doc["materials"] = [*table, translucent]
    isolated.node(
        "mask",
        [quad._replace(material=1)],
        {"name": "collider/masked", "translation": [0, 4, 0], "scale": [4, 2, 1]},
    )
    isolated.node(
        "blend",
        [quad._replace(material=len(table))],
        {"name": "secondary", "translation": [96, 4, 0], "scale": [4, 2, 1]},
    )
    (out / "only-cutouts.glb").write_bytes(isolated.bytes())
    mixed = scene()
    mixed.node(
        "panel",
        [quad._replace(material=1), quad._replace(material=0)],
        {"name": "collider/mixed", "scale": [4, 2, 1]},
    )
    (out / "mixed-parts.glb").write_bytes(mixed.bytes())
    # A camera in front of each authored panel looks along -Z.
    replay = json.loads((HERE / "fixtures/descent.json").read_bytes())
    replay["viewport"] = [320, 180]
    replay["routes"] = []
    for name, camera in [
        ("png", 0),
        ("alias", 96),
        ("mask", 192),
        ("jpeg", 288),
        ("away", 900),
    ]:
        replay["routes"].append(
            {
                "id": name,
                "ticks": 120,
                "start_m": [900, 300, 900],
                "velocity_m_s": [0, 0, 0],
                "expected_support": "reference",
                "camera": [
                    {"tick": k, "eye": [camera, 4, 12], "aim": [camera, 4, 0]}
                    for k in (0, 120)
                ],
            }
        )

    def camera_route(name, frames):
        replay["routes"].append(
            {
                "id": name,
                "ticks": frames[-1][0],
                "start_m": [900, 300, 900],
                "velocity_m_s": [0, 0, 0],
                "expected_support": "reference",
                "camera": [
                    {"tick": k, "eye": eye, "aim": aim} for k, eye, aim in frames
                ],
            }
        )

    floor_eye, floor_aim = [0, 11, 181], [0, 0, 192]
    far_eye, far_aim = [900, 8, 900], [910, 8, 900]
    camera_route("floor", [(k, floor_eye, floor_aim) for k in (0, 120)])
    camera_route("both", [(k, [48, 4, 12], [48, 4, 0]) for k in (0, 120)])
    camera_route(
        "return",
        [
            (0, floor_eye, floor_aim),
            (60, floor_eye, floor_aim),
            (61, far_eye, far_aim),
            (120, far_eye, far_aim),
            (121, floor_eye, floor_aim),
            (200, floor_eye, floor_aim),
        ],
    )
    (out / "replay.json").write_text(json.dumps(replay, indent=2) + "\n")
    return images, samplers


def glb(raw):
    require(
        raw[:4] == b"glTF" and len(raw) == struct.unpack_from("<I", raw, 8)[0],
        "invalid GLB",
    )
    size = struct.unpack_from("<I", raw, 12)[0]
    return json.loads(raw[20 : 20 + size]), raw[28 + size :]


def unpack(raw, tag):
    if tag == b"OVC1":
        return unpack_coarse(raw)
    require(raw[:4] == tag, "wrong content tag")
    result = zlib.decompress(raw[8:])
    require(len(result) == struct.unpack_from("<I", raw, 4)[0], "unpacked length")
    return result


def inventory(raw, remote=None):
    doc, blob = glb(raw)
    images = []
    for image in doc.get("images", []):
        uri = image.get("uri")
        if uri:
            require(
                remote and uri in remote and "bufferView" not in image,
                "unlisted local image reference",
            )
            data = remote[uri]
            require(
                uri
                == sha(data) + (".png" if image["mimeType"] == "image/png" else ".jpg"),
                "incorrect local image digest",
            )
        else:
            v = doc["bufferViews"][image["bufferView"]]
            s = v.get("byteOffset", 0)
            require(
                s % 4 == 0 and s + v["byteLength"] <= len(blob), "bad embedded bitmap"
            )
            data = blob[s : s + v["byteLength"]]
        images.append((sha(data), len(data), image["mimeType"], bool(uri)))
    used = set()
    bindings = []
    for model in doc.get("meshes", []):
        for part in model["primitives"]:
            m = doc["materials"][part["material"]]
            info = m.get("pbrMetallicRoughness", {}).get("baseColorTexture")
            if info is None:
                continue
            require(
                "TEXCOORD_0" in part["attributes"] and info.get("texCoord", 0) == 0,
                "lost UV binding",
            )
            tex = doc["textures"][info["index"]]
            used.add(tex["source"])
            bindings.append(
                (
                    m["name"],
                    images[tex["source"]][0],
                    doc["samplers"][tex["sampler"]] if "sampler" in tex else None,
                )
            )
    require(used == set(range(len(images))), "unreferenced embedded image")
    require(
        len(images) == len({i[0] for i in images}), "bitmap duplicated in a single GLB"
    )
    return doc, images, bindings


def audit(out, images, samplers, mode):
    directory = out / mode
    inline = mode == "embedded-local-v4"
    mode = "demand" if inline else mode
    COOK["cook"](
        out / "textured.glb", directory, 64, mode, "embedded" if inline else "demand"
    )
    data = json.loads((directory / "index.json").read_bytes())

    def read(entry):
        content = (directory / entry["file"]).read_bytes()
        require(
            len(content) == entry["bytes"] and sha(content) == entry["sha256"],
            "bad content address",
        )
        return content

    refs = {
        "opaque-png": (sha(images[0]), samplers[0]),
        "opaque-jpg": (sha(images[1]), None),
        "cutout-png": (sha(images[2]), samplers[1]),
        "cut-floor": (sha(images[3]), samplers[0]),
    }
    local = []
    clipped_pages = []
    models = []
    external_bytes = {
        img["file"]: read(img) for img in data.get("texture_resources", [])
    }
    resource_by_sha = {
        img["sha256"]: i for i, img in enumerate(data.get("texture_resources", []))
    }

    def checked(raw):
        doc, pics, bindings = inventory(raw, external_bytes)
        require(
            all(refs[name] == (key, sampler) for name, key, sampler in bindings),
            "material or sampler changed",
        )
        return doc, pics

    coarse_doc, coarse = checked(
        read(data["parent"])
        if mode == "embedded"
        else unpack(read(data["parent"]), b"OVC1")
    )
    initial = [sha(images[4])]
    require(all(x[0] not in initial for x in coarse), "unused source image emitted")
    if "models" in data:
        bank = unpack(read(data["models"]), b"OML1")
        _, models = checked(bank)
    for tile in data["tiles"]:
        raw = read(tile["visual"])
        if mode == "embedded":
            doc, pics = checked(raw)
        else:
            opened = unpack(raw, b"OVP1")
            length = struct.unpack_from("<I", opened)[0]
            records = json.loads(opened[4 : 4 + length])
            local_mesh = opened[4 + length :]
            doc, pics = checked(local_mesh) if local_mesh else ({}, [])
            if mode == "demand":
                needed = {resource_by_sha[p[0]] for p in pics if p[3]}
                for r in records:
                    if not isinstance(r, list):
                        continue
                    for j in data["model_variants"][r[0]]["materials"]:
                        mat = data["material_palette"][j]
                        binding = mat["pbrMetallicRoughness"]["baseColorTexture"]
                        t = data["textures"][binding["index"]]
                        needed.add(t["source"])
                        img = data["texture_resources"][t["source"]]
                        samp = (
                            data["samplers"][t["sampler"]] if "sampler" in t else None
                        )
                        require(
                            refs[mat["name"]] == (img["sha256"], samp),
                            "shared material binding changed",
                        )
                require(
                    needed == set(tile["visual"].get("uses_images", [])),
                    "page image requirements differ from placements",
                )
        owners = [n["extras"]["osage_source_node"] for n in doc.get("nodes", [])]
        local += [(tile["x"], tile["z"], p[0], p[1], owners, p[3]) for p in pics]
        if 4 in owners:
            clipped_pages.append(tile["visual"]["bytes"])
    source_owners = [
        n["extras"]["osage_source_node"] for n in coarse_doc.get("nodes", [])
    ]
    result = {
        "coarse_image_bytes": sum(i[1] for i in coarse),
        "local_image_references": len(local),
        "local_raw_image_bytes": sum(p[3] for p in local if not p[5]),
        "clipped_page_encoded_bytes": sum(clipped_pages),
        "raw_bank_image_bytes": sum(i[1] for i in models),
        "coarse_source_owners": source_owners,
    }
    if mode == "demand":
        remote = data["texture_resources"]
        expected = {sha(images[0]), sha(images[1])}
        if not inline:
            expected.add(sha(images[3]))
        require(
            {a["sha256"] for a in remote} == expected,
            "v4 should externalize only used images",
        )
        require(
            len(remote) == len(expected)
            and sum(read(a) == images[0] for a in remote) == 1,
            "duplicate content address",
        )
        for a in remote:
            require(
                a["file"].endswith(".png" if a["mimeType"] == "image/png" else ".jpg"),
                "MIME extension",
            )
        require(
            len(data["model_resources"]) == 1 and len(data["model_variants"]) == 3,
            "UV geometry sharing changed",
        )
        _, neutral, _ = inventory(unpack(read(data["model_resources"][0]), b"OMD1"))
        require(
            not neutral and not coarse and set(source_owners) == {0, 1, 3},
            "v4 fallback read a bitmap or covered a cutout",
        )
        clipped = [r for r in local if r[2] == sha(images[3])]
        require(
            len(clipped) >= 3 and all(4 in r[4] for r in clipped),
            "clipped owner or texture lost",
        )
        result.update(
            {
                "external_images": remote,
                "clipped_image_copies": len(clipped) if inline else 1,
                "local_uri_references": sum(bool(r[5]) for r in clipped),
                "cold_clipped_pages_and_bitmap_bytes": sum(clipped_pages)
                + (0 if inline else len(images[3])),
                "unique_clipped_image_bytes": len(images[3]),
                "repeated_clipped_image_bytes": (len(clipped) - 1) * len(images[3])
                if inline
                else 0,
            }
        )
    return result


def local_geometry(directory):
    index = json.loads((directory / "index.json").read_bytes())
    h = hashlib.sha256()
    for tile in index["tiles"]:
        page = unpack((directory / tile["visual"]["file"]).read_bytes(), b"OVP1")
        start = 4 + struct.unpack_from("<I", page)[0]
        local = page[start:]
        if not local:
            continue
        d, b = glb(local)
        h.update(COOK["encode"]([tile["x"], tile["z"], d["nodes"]]))
        for mesh in d["meshes"]:
            for part in mesh["primitives"]:
                for name, access in sorted(
                    {**part["attributes"], "indices": part["indices"]}.items()
                ):
                    a = d["accessors"][access]
                    v = d["bufferViews"][a["bufferView"]]
                    # The output writer has dense aligned accessors, one accessor per view.
                    require(
                        a.get("byteOffset", 0) == 0 and "byteStride" not in v,
                        "unexpected cooked view",
                    )
                    h.update(
                        COOK["encode"](
                            [
                                name,
                                a["count"],
                                a["type"],
                                a["componentType"],
                                a.get("normalized", False),
                            ]
                        )
                    )
                    off = v.get("byteOffset", 0)
                    h.update(b[off : off + v["byteLength"]])
    return h.hexdigest()


def optional_bitmap_fallback(path, images):
    doc, blob = glb(path.read_bytes())
    doc["extensionsUsed"] = ["KHR_texture_basisu"]
    alternate = len(doc["images"])
    doc["images"].append({"uri": "unused.ktx2", "mimeType": "image/ktx2"})
    for slot in (0, 1):
        doc["textures"][slot]["extensions"] = {
            "KHR_texture_basisu": {"source": alternate}
        }
    source = COOK["TextureSource"]
    picked = []
    table = COOK["MaterialTable"](
        source.read(doc, blob), lambda img: picked.append(img) or {}
    )
    for slot in (0, 1):
        table.add_texture(slot)
    require(
        [img.data for img in picked] == images[:2],
        "optional KTX2 did not retain PNG/JPEG fallback",
    )
    doc["extensionsRequired"] = ["KHR_texture_basisu"]
    try:
        source.read(doc, blob)
    except ValueError as e:
        require("required KHR_texture_basisu" in str(e), str(e))
    else:
        raise AssertionError("required KTX2 accepted by fallback-only cooker")
    del doc["extensionsRequired"]
    del doc["textures"][0]["source"]
    try:
        COOK["MaterialTable"](source.read(doc, blob), lambda img: {}).add_texture(0)
    except ValueError as e:
        require("fallback" in str(e), str(e))
    else:
        raise AssertionError("missing PNG/JPEG accepted by fallback-only cooker")


def source_encodings(path, images):
    doc, blob = glb(path.read_bytes())
    source = COOK["TextureSource"]
    doc["images"][0] = {
        "uri": "data:image/png;base64," + base64.b64encode(images[0]).decode()
    }
    require(
        source.read(doc, blob).bitmap(0).data == images[0],
        "embedded data URI changed its bytes",
    )
    for modified in (
        {"uri": "outside.png"},
        {"uri": doc["images"][0]["uri"], "mimeType": "image/jpeg"},
        {"uri": "data:image/png;base64,bad?encoding"},
    ):
        doc["images"][0] = modified
        try:
            source.read(doc, blob).bitmap(0)
        except ValueError:
            pass
        else:
            raise AssertionError("unsupported image source was accepted")
    info = doc["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"]
    info["texCoord"] = 1
    try:
        source.read(doc, blob)
    except ValueError:
        pass
    else:
        raise AssertionError("unsupported UV channel was accepted")


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/texture-storage"
    )
    a = cli.parse_args()
    out = a.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    images, samplers = fixtures(out)
    report = {
        mode: audit(out, images, samplers, mode)
        for mode in ("embedded", "shared", "embedded-local-v4", "demand")
    }
    prior, current = report["embedded-local-v4"], report["demand"]
    require(
        prior["clipped_image_copies"] == 5
        and current["local_uri_references"] == 5
        and current["clipped_image_copies"] == 1,
        "five independent pieces must now share one file",
    )
    require(
        current["cold_clipped_pages_and_bitmap_bytes"]
        < prior["cold_clipped_pages_and_bitmap_bytes"],
        "clipped cold transfer grew",
    )
    geom = local_geometry(out / "demand")
    require(
        geom == local_geometry(out / "embedded-local-v4"),
        "local model indices, attributes, transforms or owners changed",
    )
    current["local_geometry_sha256"] = prior["local_geometry_sha256"] = geom
    source_encodings(out / "textured.glb", images)
    optional_bitmap_fallback(out / "textured.glb", images)
    empty = out / "empty-parent"
    COOK["cook"](out / "only-cutouts.glb", empty, 64, "demand")
    idx = json.loads((empty / "index.json").read_bytes())
    parent, _ = glb(unpack((empty / idx["parent"]["file"]).read_bytes(), b"OVC1"))
    require(
        parent["scenes"] == [{}]
        and not parent.get("nodes")
        and not parent.get("images"),
        "invalid empty parent",
    )
    require(
        all(t["parent_count"] == 0 for t in idx["tiles"])
        and {m["alphaMode"] for m in idx["material_palette"]} == {"MASK", "BLEND"}
        and any(
            0 in t.get("physics", {}).get("source_nodes", []) for t in idx["tiles"]
        ),
        "masked/blended owners lost their detailed materials or physical piece",
    )
    report["empty_parent"] = {
        "coarse_instances": 0,
        "detail_alpha_modes": sorted(m["alphaMode"] for m in idx["material_palette"]),
        "detailed_owners": idx["source_instances"],
        "physics_triangles": idx["physics_triangles"],
    }
    mixed = out / "mixed-parent"
    COOK["cook"](out / "mixed-parts.glb", mixed, 64, "demand")
    idx = json.loads((mixed / "index.json").read_bytes())
    parent, _ = glb(unpack((mixed / idx["parent"]["file"]).read_bytes(), b"OVC1"))
    tile = idx["tiles"][0]
    vp = unpack((mixed / tile["visual"]["file"]).read_bytes(), b"OVP1")
    detail, _ = glb(vp[4 + struct.unpack_from("<I", vp)[0] :])
    require(
        len(idx["tiles"]) == tile["parent_count"] == 1
        and parent["nodes"][0]["extras"]
        == detail["nodes"][0]["extras"]
        == {"osage_source_node": 0}
        and [m["alphaMode"] for m in parent["materials"]] == ["OPAQUE"]
        and not parent.get("images")
        and [m["alphaMode"] for m in detail["materials"]] == ["MASK", "OPAQUE"]
        and tile["physics"]["source_nodes"] == [0],
        "mixed-material owner lost its later opaque part, textured detail or collider",
    )
    report["mixed_parent"] = {"owner": 0, "coarse_parts": 1, "fine_parts": 2}
    (out / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
