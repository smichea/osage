#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Cook independent visual GLB and collision pages from the authored Bir-Hakeim district."""

import argparse
import base64
import binascii
import hashlib
import json
import math
import struct
import tempfile
import warnings
import zlib
from collections import defaultdict
from pathlib import Path
from typing import NamedTuple

import paging_affine
import paging_channels
import paging_coarse
import paging_collision_codec
import paging_physics_directory
import paging_face_order
import paging_residual_triangles
import paging_support_regions
from paging_support_storage import pack_catalogue


def encode(value):
    return json.dumps(value, allow_nan=False, separators=(",", ":")).encode()


def digest(data):
    return hashlib.sha256(data).hexdigest()


class Channel(NamedTuple):
    kind: str
    component: int
    values: list


class Part(NamedTuple):
    material: int
    positions: list
    normals: list
    indices: list
    channels: dict


FORMATS = {5121: "B", 5123: "H", 5125: "I", 5126: "f"}
WIDTHS = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}


def source(path, *, with_textures=False):
    raw = path.read_bytes()
    size = struct.unpack_from("<I", raw, 12)[0]
    doc = json.loads(raw[20 : 20 + size])
    blob = raw[28 + size :]
    if raw[:4] != b"glTF":
        raise ValueError("expected a district GLB")

    def accessor(index):
        a = doc["accessors"][index]
        v = doc["bufferViews"][a["bufferView"]]
        fmt = f"<{WIDTHS[a['type']]}{FORMATS[a['componentType']]}"
        if "sparse" in a:
            raise ValueError("expected dense district accessor")
        at = v.get("byteOffset", 0) + a.get("byteOffset", 0)
        return [
            struct.unpack_from(
                fmt, blob, at + i * v.get("byteStride", struct.calcsize(fmt))
            )
            for i in range(a["count"])
        ]

    parts = []
    default_material = None
    visible = {node["mesh"] for node in doc["nodes"] if "mesh" in node}
    bound = set()
    for node in doc["nodes"]:
        geometry = node.get("extras", {}).get("osage", {})
        visible.update(lod["mesh"] for lod in geometry.get("visualLods", []))
        for channel in ("collision", "acoustic"):
            bound.update(surface["mesh"] for surface in geometry.get(channel, []))
    geometry_only = bound - visible
    for mi, mesh in enumerate(doc["meshes"]):
        decoded = []
        for p in mesh["primitives"]:
            attrs = p["attributes"]
            if (
                "POSITION" not in attrs
                or (mi not in geometry_only and "NORMAL" not in attrs)
                or not attrs.keys()
                <= {
                    "POSITION",
                    "NORMAL",
                    "COLOR_0",
                    "COLOR_1",
                    "_OSAGE_SOURCE_COLOR_0",
                    "TEXCOORD_0",
                }
                or p.get("mode", 4) != 4
            ):
                raise ValueError("unsupported district primitive")
            positions = accessor(attrs["POSITION"])
            channels = {}
            for semantic, kinds in (
                ("COLOR_0", ("VEC3", "VEC4")),
                ("COLOR_1", ("VEC3", "VEC4")),
                ("_OSAGE_SOURCE_COLOR_0", ("VEC4",)),
                ("TEXCOORD_0", ("VEC2",)),
            ):
                if semantic not in attrs:
                    continue
                a = doc["accessors"][attrs[semantic]]
                kind, component = a["type"], a["componentType"]
                if (
                    kind not in kinds
                    or component not in (5121, 5123, 5126)
                    or bool(a.get("normalized")) != (component != 5126)
                ):
                    raise ValueError(f"unsupported {semantic} encoding")
                values = accessor(attrs[semantic])
                if len(values) != len(positions):
                    raise ValueError(f"{semantic} count differs from POSITION")
                channels[semantic] = Channel(kind, component, values)
            material = p.get("material")
            if material is None:
                if default_material is None:
                    default_material = len(doc.setdefault("materials", []))
                    doc["materials"].append({})
                material = default_material
            decoded.append(
                Part(
                    material,
                    positions,
                    accessor(attrs["NORMAL"]) if "NORMAL" in attrs else [],
                    [i for (i,) in accessor(p["indices"])],
                    channels,
                )
            )
        parts.append(decoded)
    if with_textures:
        textures = TextureSource.read(doc, blob)
        if textures:
            for mi, mesh in enumerate(parts):
                if mi in geometry_only:
                    continue
                for part in mesh:
                    if (
                        textures.binding(part.material) is not None
                        and "TEXCOORD_0" not in part.channels
                    ):
                        raise ValueError(
                            "baseColorTexture requires TEXCOORD_0 on the primitive"
                        )
        return doc, parts, digest(raw), textures
    return doc, parts, digest(raw)


def transform(p, node, normal=False):
    if "matrix" in node:
        m = node["matrix"]
        # glTF stores affine transforms in column-major order.
        a, b, c = (m[i : i + 3] for i in (0, 4, 8))
        if normal:

            def cross(u, v):
                return (
                    u[1] * v[2] - u[2] * v[1],
                    u[2] * v[0] - u[0] * v[2],
                    u[0] * v[1] - u[1] * v[0],
                )

            cof = (cross(b, c), cross(c, a), cross(a, b))
            det = sum(x * y for x, y in zip(a, cof[0]))
            if not det:
                raise ValueError("singular source node")
            n = [sum(p[j] * cof[j][i] for j in range(3)) / det for i in range(3)]
            length = math.hypot(*n)
            return tuple(v / length for v in n)
        return tuple(
            m[12 + i] + a[i] * p[0] + b[i] * p[1] + c[i] * p[2] for i in range(3)
        )
    scale = node.get("scale", [1] * 3)
    tr = node.get("translation", [0] * 3)
    a, b, c = (p[i] / scale[i] if normal else p[i] * scale[i] for i in range(3))
    x, y, z, w = node.get("rotation", [0, 0, 0, 1])
    u, v, s = 2 * (y * c - z * b), 2 * (z * a - x * c), 2 * (x * b - y * a)
    p = [
        a + w * u + y * s - z * v,
        b + w * v + z * u - x * s,
        c + w * s + x * v - y * u,
    ]
    if normal:
        length = math.hypot(*p)
        return tuple(k / length for k in p)
    return tuple(k + t for k, t in zip(p, tr))


def bounds(points):
    return [[f(v) for v in zip(*points)] for f in (min, max)]


def tree(doc):
    """An original glTF node index names its owner across every derived representation."""
    nodes = doc["nodes"]
    parents = [None] * len(nodes)
    placed = []
    visited = set()
    stack = [(i, None) for i in reversed(doc["scenes"][doc["scene"]]["nodes"])]
    while stack:
        i, parent = stack.pop()
        if i in visited or not 0 <= i < len(nodes):
            raise ValueError("district scene must be a rooted forest")
        visited.add(i)
        parents[i] = parent
        if "mesh" in nodes[i]:
            placed.append(i)
        stack.extend((child, i) for child in reversed(nodes[i].get("children", [])))
    return parents, placed


def support_faces(shape, slot, barriers):
    # Keep the original facing and exact vertex positions, including clipped hips and deck edges.
    vertices, indices, remap = [], [], {}
    selected = []
    for ordinal, tri in enumerate(shape["triangles"]):
        a, b, c = (shape["vertices"][i] for i in tri)
        u, v = ([p - q for p, q in zip(point, a)] for point in (b, c))
        normal = (
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        )
        if normal[1] <= 0.5 * math.hypot(*normal):
            # A missing side or soffit can block any approach. Keep separate
            # bounds for the two jambs so an opening remains passable.
            lo, hi = bounds([a, b, c])
            key = (slot, tuple(lo), tuple(hi))
            paging_residual_triangles.observe(barriers, key, (a, b, c))
            continue
        face = []
        for point in (a, b, c):
            key = tuple(point)
            if key not in remap:
                remap[key] = len(vertices)
                vertices.append(point)
            face.append(remap[key])
        indices.append(face)
        paging_face_order.append_face(selected, ordinal)
    return [shape["source_node"], vertices, indices], selected


def surface_area(vertices, triangles):
    total = 0.0
    for ia, ib, ic in triangles:
        a, b, c = vertices[ia], vertices[ib], vertices[ic]
        u = [x - y for x, y in zip(b, a)]
        v = [x - y for x, y in zip(c, a)]
        total += 0.5 * math.hypot(
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        )
    return total


def clip(poly, axis, edge, greater):
    out = []
    for a, b in zip(poly[-1:] + poly, poly):
        inside_a, inside_b = (
            (a[axis] >= edge, b[axis] >= edge)
            if greater
            else (a[axis] <= edge, b[axis] <= edge)
        )
        if inside_a != inside_b:
            t = (edge - a[axis]) / (b[axis] - a[axis])
            out.append(tuple(x + t * (y - x) for x, y in zip(a, b)))
        if inside_b:
            out.append(b)
    return out


def triangles_in_cell(parts, cell, edge):
    answer = []
    for mat, vertices, normals, indices, channels in parts:
        points, ns, ix = [], [], []
        # Preserve endpoints exactly in uncut meshes. At a seam, interpolate normalized
        # integer colours/UVs in their linear domain and emit float32 to avoid re-rounding.
        extra = {}
        cursor = 6
        corners = [p + n for p, n in zip(vertices, normals)]
        for name, channel in channels.items():
            width = WIDTHS[channel.kind]
            scale = {5121: 255, 5123: 65535, 5126: 1}[channel.component]
            corners = [
                v + tuple(c / scale for c in row)
                for v, row in zip(corners, channel.values)
            ]
            extra[name] = (
                slice(cursor, cursor + width),
                Channel(channel.kind, 5126, []),
            )
            cursor += width
        for i in range(0, len(indices), 3):
            poly = [corners[k] for k in indices[i : i + 3]]
            for axis, coord in zip((0, 2), cell):
                poly = clip(poly, axis, (coord - 0.5) * edge, True)
                poly = clip(poly, axis, (coord + 0.5) * edge, False)
            for k in range(1, len(poly) - 1):
                a, b, c = poly[0], poly[k], poly[k + 1]
                u = [b[j] - a[j] for j in range(3)]
                v = [c[j] - a[j] for j in range(3)]
                if (
                    math.hypot(
                        u[1] * v[2] - u[2] * v[1],
                        u[2] * v[0] - u[0] * v[2],
                        u[0] * v[1] - u[1] * v[0],
                    )
                    < 1e-8
                ):
                    continue
                for vert in (a, b, c):
                    ix.append(len(points))
                    points.append(vert[:3])
                    ns.append(vert[3:6])
                    for span, channel in extra.values():
                        channel.values.append(vert[span])
        if ix:
            answer.append(
                Part(mat, points, ns, ix, {name: c for name, (_, c) in extra.items()})
            )
    return answer


class Bitmap(NamedTuple):
    data: bytes
    mime: str
    sha256: str


class TextureSource:
    def __init__(self, doc, blob):
        self.doc = doc
        self.blob = blob
        self.loaded = {}

    @classmethod
    def read(cls, doc, blob):
        if "KHR_texture_basisu" in doc.get("extensionsRequired", []):
            raise ValueError("required KHR_texture_basisu has no supported fallback")
        used = False
        for m in doc["materials"]:
            pbr = m.get("pbrMetallicRoughness", {})
            if (
                any(
                    k in m
                    for k in ("normalTexture", "emissiveTexture", "occlusionTexture")
                )
                or "metallicRoughnessTexture" in pbr
            ):
                raise ValueError(
                    "only baseColorTexture is supported by district materials"
                )
            if "baseColorTexture" in pbr:
                info = pbr["baseColorTexture"]
                if info.get("texCoord", 0) != 0 or info.get("extensions"):
                    raise ValueError(
                        "baseColorTexture requires untransformed TEXCOORD_0"
                    )
                used = True
        return cls(doc, blob) if used else None

    def binding(self, material):
        return (
            self.doc["materials"][material]
            .get("pbrMetallicRoughness", {})
            .get("baseColorTexture")
        )

    def entry(self, table, index):
        entries = self.doc.get(table, [])
        if type(index) is not int or not 0 <= index < len(entries):
            raise ValueError(f"invalid glTF {table} index")
        return entries[index]

    def bitmap(self, i):
        if i in self.loaded:
            return self.loaded[i]
        image = self.entry("images", i)
        mime = image.get("mimeType")
        if "bufferView" in image:
            if "uri" in image:
                raise ValueError("glTF image has both URI and bufferView")
            v = self.entry("bufferViews", image["bufferView"])
            start = v.get("byteOffset", 0)
            end = start + v["byteLength"]
            if v["buffer"] != 0 or start < 0 or end > len(self.blob):
                raise ValueError("embedded image outside source buffer")
            data = self.blob[start:end]
        else:
            header, sep, payload = image.get("uri", "").partition(",")
            if not sep or header not in (
                "data:image/png;base64",
                "data:image/jpeg;base64",
                "data:image/ktx2;base64",
            ):
                raise ValueError("expected an embedded PNG, JPEG or KTX2 image")
            media = header[5:-7]
            if mime is not None and mime != media:
                raise ValueError("image URI MIME type mismatch")
            mime = media
            try:
                data = base64.b64decode(payload, validate=True)
            except binascii.Error as exc:
                raise ValueError("invalid base64 image") from exc
        png = data.startswith(b"\x89PNG\r\n\x1a\n") and data[12:16] == b"IHDR"
        jpeg = data.startswith(b"\xff\xd8\xff") and data.endswith(b"\xff\xd9")
        ktx = len(data) >= 80 and data.startswith(b"\xabKTX 20\xbb\r\n\x1a\n")
        if not (
            (mime == "image/png" and png)
            or (mime == "image/jpeg" and jpeg)
            or (mime == "image/ktx2" and ktx)
        ):
            raise ValueError("invalid or unsupported PNG, JPEG or KTX2 image payload")
        self.loaded[i] = Bitmap(data, mime, digest(data))
        return self.loaded[i]

    def optional_basisu(self, binding):
        try:
            if not isinstance(binding, dict):
                raise ValueError("missing texture extension object")
            index = binding.get("source")
            image = self.entry("images", index)
            if not isinstance(image, dict):
                raise ValueError("invalid image entry")
            uri = image.get("uri", "")
            if "bufferView" not in image and not (
                isinstance(uri, str) and uri.startswith("data:")
            ):
                raise ValueError(
                    "the portable object does not embed the optional image"
                )
            candidate = self.bitmap(index)
            if candidate.mime != "image/ktx2":
                raise ValueError("expected a KTX2 image")
            return candidate
        except (ValueError, KeyError, TypeError) as error:
            warnings.warn(
                f"Keeping core texture for optional BasisU binding {binding!r}: {error}",
                stacklevel=2,
            )
            return None


class MaterialTable:
    """Remap the materials, samplers and images actually referenced by one output."""

    def __init__(self, source, store_image, coarse=False, basisu=None):
        self.source = source
        self.store_image = store_image
        self.basisu = basisu
        self.image_choices = set()
        self.coarse = coarse
        self.materials, self.images, self.textures, self.samplers = [], [], [], []
        self.material_ids, self.image_ids, self.texture_ids, self.sampler_ids = (
            {},
            {},
            {},
            {},
        )

    def add_texture(self, index):
        t = self.source.entry("textures", index)
        extensions = t.get("extensions", {})
        if extensions.keys() - {"KHR_texture_basisu"}:
            raise ValueError("unsupported texture source extension")
        if "source" not in t:
            raise ValueError("texture needs an explicit PNG or JPEG fallback")

        img = self.source.bitmap(t["source"])
        if img.mime not in ("image/png", "image/jpeg"):
            raise ValueError("texture needs an explicit PNG or JPEG fallback")
        if img.sha256 not in self.image_ids:
            self.image_ids[img.sha256] = len(self.images)
            self.images.append(self.store_image(img))
        tex = {"source": self.image_ids[img.sha256]}
        alt = None
        if self.basisu and "KHR_texture_basisu" in extensions:
            alt = self.source.optional_basisu(extensions["KHR_texture_basisu"])
            if alt is not None:
                tex.update(self.basisu(self, img, alt))
        self.image_choices.add((img.sha256, alt.sha256 if alt else ""))
        if "sampler" in t:
            sampler = self.source.entry("samplers", t["sampler"])
            if sampler.get("extensions"):
                raise ValueError("unsupported sampler extension")
            for field, valid in (
                ("magFilter", (9728, 9729)),
                ("minFilter", (9728, 9729, 9984, 9985, 9986, 9987)),
                ("wrapS", (33071, 33648, 10497)),
                ("wrapT", (33071, 33648, 10497)),
            ):
                if field in sampler and sampler[field] not in valid:
                    raise ValueError(f"unsupported glTF sampler {field}")
            key = json.dumps(sampler, sort_keys=True)
            if key not in self.sampler_ids:
                self.sampler_ids[key] = len(self.samplers)
                self.samplers.append(sampler)
            tex["sampler"] = self.sampler_ids[key]
        key = (tex["source"], tex.get("sampler"), alt.sha256 if alt else "")
        if key not in self.texture_ids:
            self.texture_ids[key] = len(self.textures)
            self.textures.append(tex)
        return self.texture_ids[key]

    def add(self, i):
        if i not in self.material_ids:
            m = self.source.doc["materials"][i]
            info = self.source.binding(i)
            if info is not None:
                pbr = dict(m["pbrMetallicRoughness"])
                if self.coarse:
                    del pbr["baseColorTexture"]
                else:
                    pbr["baseColorTexture"] = {
                        **info,
                        "index": self.add_texture(info["index"]),
                    }
                m = {**m, "pbrMetallicRoughness": pbr}
            self.material_ids[i] = len(self.materials)
            self.materials.append(m)
        return self.material_ids[i]


class Glb:
    def __init__(
        self, materials, textures=None, coarse=False, image_uri=None, basisu=None
    ):
        self.doc = {
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": []}],
            "nodes": [],
            "meshes": [],
            "materials": materials,
            "accessors": [],
            "bufferViews": [],
            "buffers": [{}],
        }
        self.bin = bytearray()
        self.models = {}
        self.table = (
            MaterialTable(textures, image_uri or self.embed_image, coarse, basisu)
            if textures
            else None
        )
        if self.table:
            self.doc["materials"] = self.table.materials

    def embed_image(self, image):
        start = len(self.bin)
        self.bin.extend(image.data)
        self.bin.extend(bytes(-len(self.bin) % 4))
        views = self.doc["bufferViews"]
        views.append({"buffer": 0, "byteOffset": start, "byteLength": len(image.data)})
        return {"bufferView": len(views) - 1, "mimeType": image.mime}

    def accessor(self, data, fmt, kind, component, is_position=False, normalized=False):
        start = len(self.bin)
        binary = b"".join(struct.pack("<" + fmt, *v) for v in data)
        self.bin.extend(binary)
        self.bin.extend(bytes((-len(self.bin)) % 4))
        views = self.doc["bufferViews"]
        entries = self.doc["accessors"]
        entries.append(
            {
                "bufferView": len(views),
                "componentType": component,
                "count": len(data),
                "type": kind,
            }
        )
        if normalized:
            entries[-1]["normalized"] = True
        if is_position:
            entries[-1]["min"], entries[-1]["max"] = bounds(data)
        views.append({"buffer": 0, "byteOffset": start, "byteLength": len(binary)})
        return len(entries) - 1

    def node(self, key, parts, node):
        if key not in self.models:
            prim = []
            for material, pos, norm, ix, channels in parts:
                prim.append(
                    {
                        "attributes": {
                            "POSITION": self.accessor(pos, "3f", "VEC3", 5126, True),
                            "NORMAL": self.accessor(norm, "3f", "VEC3", 5126),
                        },
                        "indices": self.accessor(
                            [(i,) for i in ix], "I", "SCALAR", 5125
                        ),
                        "material": self.table.add(material)
                        if self.table
                        else material,
                    }
                )
                for name, channel in channels.items():
                    prim[-1]["attributes"][name] = self.accessor(
                        channel.values,
                        f"{WIDTHS[channel.kind]}{FORMATS[channel.component]}",
                        channel.kind,
                        channel.component,
                        normalized=channel.component != 5126,
                    )
            self.models[key] = len(self.doc["meshes"])
            self.doc["meshes"].append({"primitives": prim})
        self.doc["scenes"][0]["nodes"].append(len(self.doc["nodes"]))
        self.doc["nodes"].append({**node, "mesh": self.models[key]})

    def bytes(self):
        if self.table and self.table.images:
            self.doc.update(
                {"images": self.table.images, "textures": self.table.textures}
            )
            if any(t.get("extensions") for t in self.table.textures):
                self.doc["extensionsUsed"] = ["KHR_texture_basisu"]
            if self.table.samplers:
                self.doc["samplers"] = self.table.samplers
        doc, binary = self.doc, self.bin
        if not self.doc["nodes"]:
            # Empty active scene. Keep the single embedded buffer expected by the district reader.
            doc = {
                "asset": {"version": "2.0"},
                "scene": 0,
                "scenes": [{}],
                "buffers": [{}],
            }
            binary = bytes(4)
        doc["buffers"][0]["byteLength"] = len(binary)
        text = encode(doc)
        text += b" " * ((-len(text)) % 4)
        return (
            struct.pack(
                "<4sIIII",
                b"glTF",
                2,
                28 + len(text) + len(binary),
                len(text),
                0x4E4F534A,
            )
            + text
            + struct.pack("<I4s", len(binary), b"BIN\0")
            + binary
        )


def pack(tag, raw):
    return tag + struct.pack("<I", len(raw)) + zlib.compress(raw, 6)


def visual_levels(node, meshes, storage):
    if storage != "demand":
        return []
    high = node["mesh"]
    primary_count = sum(len(p.indices) // 3 for p in meshes[high])
    levels = []
    seen = {"high"}
    for lod in node.get("extras", {}).get("osage", {}).get("visualLods", []):
        level, index, deviation = lod["level"], lod["mesh"], lod.get("maxDeviationM")
        if (
            level not in ("medium", "low", "veryLow")
            or level in seen
            or not 0 <= index < len(meshes)
        ):
            raise ValueError("invalid visual LOD on source node")
        seen.add(level)
        if deviation is None:
            continue
        if not math.isfinite(deviation) or deviation < 0:
            raise ValueError("invalid geometric deviation on source node")
        if sum(len(p.indices) // 3 for p in meshes[index]) < primary_count:
            levels.append({"level": level, "mesh": index, "error_m": deviation})
    return [{"level": "high", "mesh": high, "error_m": 0}, *levels] if levels else []


def cook(path, out, edge, storage="demand", local_images="demand", physics_storage="packed", coarse_storage="auto", support_storage="auto", pin_index=False, support_region_tiles=None, physics_pin=False, regional_pin=False, bundled_pin=False, visual_regional_pin=False, first_frame_pin=False, first_frame_cell=None, previous_first_frame=None):
    if previous_first_frame is not None and not first_frame_pin:
        raise ValueError("a previous package requires --pin-first-frame")
    if first_frame_cell is not None and not first_frame_pin:
        raise ValueError("first-frame tile requires --pin-first-frame")
    regional_pin = regional_pin or bundled_pin
    if (regional_pin or visual_regional_pin or first_frame_pin) and support_region_tiles is None:
        support_region_tiles = 4
    if physics_storage not in ("packed", "json"):
        raise ValueError("unknown collision storage")
    if not 32 <= edge <= 512:
        raise ValueError("tile size must be between 32 and 512 metres")
    doc, meshes, fingerprint, textures = source(path, with_textures=True)
    cells = defaultdict(list)
    source_area = defaultdict(float)
    source_colliders = []
    tiled_area = defaultdict(float)
    local_bounds = [bounds([p for part in m for p in part.positions]) for m in meshes]
    parents, placed = tree(doc)
    poses = paging_affine.world_placements(doc)
    lods = {}
    profile = storage == "demand" and doc["asset"].get("extras", {}).get("osageObject")
    active = paging_channels.high_channels(doc) if profile else {}
    if profile:
        placed = [i for i in placed if "visual" in active[i]]

    def cell(p):
        return math.floor(p / edge + 0.5)

    def emit(c, key, parts, node, shape, box, owner):
        cells[c].append(
            (
                key,
                parts,
                {**node, "extras": {"osage_source_node": owner}},
                shape,
                box,
                owner,
            )
        )

    for inode in placed:
        node = doc["nodes"][inode]
        mesh = node["mesh"]
        geometry = meshes[mesh]
        levels = visual_levels(node, meshes, storage)
        low, high = (
            bounds([point for lod in levels for point in local_bounds[lod["mesh"]]])
            if levels
            else local_bounds[mesh]
        )
        original_name = node.get("name", "")
        key = mesh
        node, matrix = poses[inode]
        box = bounds(
            [
                paging_affine.apply(matrix, (x, y, z))
                for x in (low[0], high[0])
                for y in (low[1], high[1])
                for z in (low[2], high[2])
            ]
        )
        a, b = box
        label = original_name
        shape = (
            label.split("/", 1)[1]
            if not profile and label.startswith(("support/", "collider/"))
            else None
        )
        if shape is not None:
            points, faces = paging_channels.geometry(geometry)
            world = [paging_affine.apply(matrix, point) for point in points]
            collider = paging_channels.collision_shape(
                inode, shape, world, faces, paging_channels.reverses_winding(node)
            )
            source_colliders.append((collider, None))
        placement = {
            k: v
            for k, v in node.items()
            if k in ("rotation", "scale", "translation", "matrix")
        }
        if levels:
            lods[inode] = {
                "owner": inode,
                "bounds": [low, high],
                "pose": placement,
                "levels": levels,
            }
        # Keep an authored object and its alternate meshes as a single placement with union bounds.
        if levels or max(b[0] - a[0], b[2] - a[2]) <= edge:
            owner = (cell((a[0] + b[0]) * 0.5), cell((a[2] + b[2]) * 0.5))
            emit(owner, key, geometry, placement, shape, box, inode)
        else:
            # Baked cell fragments have identity transforms. Keep the source front side after a reflection.
            reflected = paging_channels.reverses_winding(node)
            transformed = [
                (
                    m,
                    [transform(p, node) for p in ps],
                    [transform(n, node, True) for n in ns],
                    [i for a, b, c in zip(*[iter(ix)] * 3) for i in (a, c, b)]
                    if reflected
                    else ix,
                    attrs,
                )
                for m, ps, ns, ix, attrs in geometry
            ]
            for cx in range(cell(a[0]), cell(math.nextafter(b[0], -math.inf)) + 1):
                for cz in range(cell(a[2]), cell(math.nextafter(b[2], -math.inf)) + 1):
                    fragment = triangles_in_cell(transformed, (cx, cz), edge)
                    if fragment:
                        emit(
                            (cx, cz),
                            ("piece", inode, cx, cz),
                            fragment,
                            {},
                            shape,
                            bounds([p for part in fragment for p in part.positions]),
                            inode,
                        )
    out.mkdir(parents=True, exist_ok=True)

    def publish(dest, data):
        with tempfile.TemporaryDirectory(dir=out) as staging:
            path = Path(staging) / dest.name
            path.write_bytes(data)
            path.replace(dest)

    def write(data, suffix):
        h = digest(data)
        filename = h + "." + suffix
        dest = out / filename
        if not dest.exists() or dest.read_bytes() != data:
            publish(dest, data)
        return {"file": filename, "sha256": h, "bytes": len(data)}

    bindings = None
    if profile:
        bindings, source_colliders = paging_channels.cook_channels(
            doc, meshes, poses, active, write, encode, bounds
        )
    collision_tiles = defaultdict(list)
    fragments = defaultdict(list)
    for collider, material in source_colliders:
        source_area[collider["source_node"]] += surface_area(collider["vertices"], collider["triangles"])
        for key, piece, runs in paging_channels.collision_pages(collider, cell, bounds):
            if runs is not None:
                fragments[key].append([len(collision_tiles[key]), len(collider["triangles"]), runs])
            collision_tiles[key].append((piece, material))
            cells.setdefault(key, [])

    parent = Glb(doc["materials"], textures, coarse=storage == "demand")
    shared_keys = []
    bank_file = None
    resources = []
    variants = []
    model_indices = {}
    if storage != "embedded":
        across = defaultdict(set)
        for cell_key, entries in cells.items():
            for mesh, *_ in entries:
                if isinstance(mesh, int):
                    across[mesh].add(cell_key)
        canonical_models = {}
        if storage == "demand":
            # Distinct source draw IDs can bind different materials to identical vertices.
            locations_by_digest = defaultdict(set)
            model_payloads = {}
            adaptive_meshes = {
                v["mesh"] for group in lods.values() for v in group["levels"]
            }
            for key in adaptive_meshes:
                across.setdefault(key, set())
            for key, locations in across.items():
                model = Glb([{}])
                model.node(key, [part._replace(material=0) for part in meshes[key]], {})
                content = pack(b"OMD1", model.bytes())
                h = digest(content)
                model_payloads[h] = content
                canonical_models[key] = h
                locations_by_digest[h].update(locations)
            shared_keys = sorted(
                k
                for k, h in canonical_models.items()
                if k in adaptive_meshes or len(locations_by_digest[h]) > 1
            )
        else:
            shared_keys = sorted(
                k for k, locations in across.items() if len(locations) > 1
            )
        if shared_keys and storage == "shared":
            bank = Glb(doc["materials"], textures)
            for key in shared_keys:
                bank.node(key, meshes[key], {})
            bank_file = {
                **write(pack(b"OML1", bank.bytes()), "models"),
                "source_meshes": shared_keys,
            }
            model_indices = {key: i for i, key in enumerate(shared_keys)}
        elif shared_keys:
            by_digest = {}
            for key in shared_keys:
                h = canonical_models[key]
                if h not in by_digest:
                    content = write(model_payloads[h], "model")
                    by_digest[h] = len(resources)
                    resources.append(
                        {
                            **content,
                            "triangles": sum(
                                len(part.indices) // 3 for part in meshes[key]
                            ),
                            "primitives": len(meshes[key]),
                        }
                    )
                slot = by_digest[h]
                model_indices[key] = len(variants)
                variants.append(
                    {
                        "source_mesh": key,
                        "resource": slot,
                        "materials": [part.material for part in meshes[key]],
                    }
                )
    external = None
    image_resources = []
    image_ids = {}

    def register_image(img):
        if img.sha256 not in image_ids:
            image_ids[img.sha256] = len(image_resources)
            image_resources.append(
                {
                    **write(img.data, "png" if img.mime == "image/png" else "jpg"),
                    "mimeType": img.mime,
                }
            )
        return image_resources[image_ids[img.sha256]]

    basisu_resources = []
    basisu_ids = {}

    def register_basisu(core, candidate):
        key = (core.sha256, candidate.sha256)
        if key not in basisu_ids:
            register_image(core)
            basisu_ids[key] = len(basisu_resources)
            basisu_resources.append(
                {
                    **write(candidate.data, "ktx2"),
                    "source": image_ids[core.sha256],
                }
            )
        return basisu_ids[key]

    def palette_basisu(table, core, candidate):
        return {"basisu": register_basisu(core, candidate)}

    def page_basisu(table, core, candidate):
        slot = register_basisu(core, candidate)
        if candidate.sha256 not in table.image_ids:
            table.image_ids[candidate.sha256] = len(table.images)
            table.images.append(
                {"uri": basisu_resources[slot]["file"], "mimeType": "image/ktx2"}
            )
        return {
            "extensions": {
                "KHR_texture_basisu": {"source": table.image_ids[candidate.sha256]}
            }
        }

    def image_uri(img):
        return {"uri": register_image(img)["file"], "mimeType": img.mime}

    external_local = (
        storage == "demand" and local_images == "demand" and textures is not None
    )
    if storage == "demand" and textures:
        external = MaterialTable(textures, register_image, basisu=palette_basisu)
        for variant in variants:
            variant["materials"] = [external.add(m) for m in variant["materials"]]
    tiled = []
    proxy_pages = []
    partial_face_runs = []
    residual_pages = []
    residual_shapes = []
    residual_triangle_corners = []
    binding = hashlib.sha256()
    total_v = total_c = 0
    for (x, z), entries in sorted(cells.items()):
        visual = Glb(
            doc["materials"],
            textures,
            image_uri=image_uri if external_local else None,
            basisu=page_basisu if external_local else None,
        )
        physical = []
        ids = []
        owners = []
        collision_points = []
        vt = ct = 0
        first = len(parent.doc["nodes"])
        boxes = []
        draw = []
        for key, parts, node, shape, box, owner in entries:
            boxes.extend(box)
            if key in model_indices:
                draw.append(
                    [
                        model_indices[key],
                        owner,
                        {k: v for k, v in node.items() if k != "extras"},
                    ]
                )
            else:
                draw.append(len(visual.doc["nodes"]))
                visual.node(key, parts, node)
            dimensions = [hi - lo for lo, hi in zip(*box)]
            # Major forms, roofs, tower and decks survive in the distant model.
            # Fine bridge furniture and window/trim primitives come from the tile.
            support_owner = (
                profile
                and "collision" in active[owner]
                and doc["nodes"][owner]
                .get("extras", {})
                .get("osage", {})
                .get("collision")
            )
            if (
                shape
                or support_owner
                or min(dimensions) >= 0.5
                or max(dimensions) >= 24
            ):
                coarse = parts[:1]
                if storage == "demand" and textures:
                    coarse = next(
                        (
                            [part]
                            for part in parts
                            if textures.binding(part.material) is None
                            or doc["materials"][part.material].get(
                                "alphaMode", "OPAQUE"
                            )
                            == "OPAQUE"
                        ),
                        [],
                    )
                if coarse:
                    parent.node(key, coarse, node)
            vt += sum(len(ix) // 3 for _, _, _, ix, _ in parts)
        authored_materials = []
        for s, material in collision_tiles.get((x, z), []):
            physical.append(s)
            if material is not None:
                authored_materials.append(material)
            ids.append(s["id"])
            owners.append(s["source_node"])
            collision_points.extend(s["vertices"])
            tiled_area[s["source_node"]] += surface_area(s["vertices"], s["triangles"])
            ct += len(s["triangles"])
        if not boxes:
            boxes.extend(collision_points)
        if storage != "embedded":
            placements = encode(draw)
            local = visual.bytes() if visual.doc["nodes"] else b""
            encoded_visual = write(
                pack(b"OVP1", struct.pack("<I", len(placements)) + placements + local),
                "vpage",
            )
        else:
            encoded_visual = write(visual.bytes(), "glb")
        t = {
            "x": x,
            "z": z,
            "visual": {
                **encoded_visual,
                "bounds": bounds(boxes),
                "triangles": vt,
            },
            "parent_first": first,
            "parent_count": len(parent.doc["nodes"]) - first,
        }
        if storage == "demand":
            adaptive = [
                lods[owner] for _, _, _, _, _, owner in entries if owner in lods
            ]
            if adaptive:
                t["visual"]["lods"] = [
                    {
                        **g,
                        "levels": [
                            {
                                "level": v["level"],
                                "variant": model_indices[v["mesh"]],
                                "error_m": v["error_m"],
                            }
                            for v in g["levels"]
                        ],
                    }
                    for g in adaptive
                ]
                t["visual"]["adaptive_only"] = len(adaptive) == len(entries)
            t["visual"]["uses"] = sorted(
                {
                    variants[r[0]]["resource"]
                    for r in draw
                    if isinstance(r, list) and r[1] not in lods
                }
            )
        if external and image_resources:
            choices = (
                {
                    (image_ids[h], basisu_ids[(h, alt)] if alt else None)
                    for h, alt in visual.table.image_choices
                }
                if external_local
                else set()
            )
            for record in draw:
                if isinstance(record, list) and record[1] not in lods:
                    for m in variants[record[0]]["materials"]:
                        info = (
                            external.materials[m]
                            .get("pbrMetallicRoughness", {})
                            .get("baseColorTexture")
                        )
                        if info:
                            tex = external.textures[info["index"]]
                            choices.add((tex["source"], tex.get("basisu")))
            if choices:
                t["visual"]["uses_images"] = sorted({core for core, _ in choices})
                if any(alt is not None for _, alt in choices):
                    t["visual"]["image_choices"] = sorted(
                        choices,
                        key=lambda item: (item[0], -1 if item[1] is None else item[1]),
                    )
        if physical:
            if physics_storage == "packed":
                body = write(paging_collision_codec.encode_collision(physical), "cpage")
            else:
                body = write(encode(physical), "json")
            t["physics"] = {
                **body,
                "bounds": bounds(collision_points),
                "triangles": ct,
                "ids": ids,
                "source_nodes": owners,
            }
            if authored_materials:
                t["physics"]["materials"] = authored_materials
            if (x, z) in fragments:
                t["physics"]["source_face_runs"] = fragments[x, z]
            binding.update(struct.pack("<ii", x, z))
            binding.update(t["physics"]["sha256"].encode("ascii"))
            barriers = {}
            page = []
            for slot, shape in enumerate(physical):
                proxy, selected = support_faces(shape, slot, barriers)
                if 0 < len(proxy[2]) < len(shape["triangles"]):
                    partial_face_runs.append([len(proxy_pages), slot, selected])
                page.append(proxy)
            proxy_pages.append(page)
            ordered = sorted(barriers.items())
            residual_triangle_corners.extend(
                [len(residual_pages), row, code]
                for row, (_, (_, code)) in enumerate(ordered)
                if code is not None
            )
            residual_pages.append([[lo, hi, count] for (_, lo, hi), (count, _) in ordered])
            residual_shapes.append([slot for (slot, _, _), _ in ordered])
        tiled.append(t)
        total_v += vt
        total_c += ct
    if source_area.keys() != tiled_area.keys() or any(
        not math.isclose(area, tiled_area[shape], rel_tol=1e-8, abs_tol=1e-7)
        for shape, area in source_area.items()
    ):
        raise ValueError("collision surface area changed during tiling")
    catalogue = {
        "format": "ville-paris/support-faces/2",
        "residual": residual_pages,
        "residual_shapes": residual_shapes,
        "directory_sha256": binding.hexdigest(),
        "pages": proxy_pages,
    }
    if residual_triangle_corners:
        catalogue["residual_triangle_corners"] = residual_triangle_corners
    exact = paging_channels.repeated_corner_meshes(collision_tiles, bounds)
    if exact:
        catalogue["corner_meshes"] = exact
    if total_c:
        catalogue["face_order"] = paging_face_order.support_order(partial_face_runs)
        paging_face_order.validate_face_order({"collision_face_order": 1, "tiles": tiled}, catalogue)
    proxy = encode(catalogue)
    support_bytes, support_extension = pack_catalogue(proxy, support_storage)
    manifest = {
        "format": "ville-paris/pages/"
        + {"demand": "4", "shared": "3", "embedded": "2"}[storage],
        "source_sha256": fingerprint,
        "edge_m": edge,
        "parent": write(paging_coarse.pack_coarse(parent.bytes(), coarse_storage), "coarse")
        if storage != "embedded"
        else write(parent.bytes(), "glb"),
        "support": {
            **write(support_bytes, support_extension),
            "decoded_bytes": len(proxy),
        },
        "source_parents": parents,
        "tiles": tiled,
        "source_instances": len(placed),
        "source_triangles": sum(
            sum(len(p.indices) // 3 for p in meshes[doc["nodes"][i]["mesh"]])
            for i in placed
        ),
        "visual_triangles": total_v,
        "physics_triangles": total_c,
        "physics_area_audit": {
            "source_shapes": len(source_area),
            "source_area_m2": sum(source_area.values()),
            "tiled_area_m2": sum(tiled_area.values()),
            "max_absolute_error_per_shape_m2": max(
                (abs(a - tiled_area[k]) for k, a in source_area.items()), default=0
            ),
        },
    }
    if total_c:
        manifest["collision_face_order"] = 1
    if bindings is not None:
        manifest["object_profile"] = bindings
    if bank_file is not None:
        manifest["models"] = bank_file
    if storage == "demand":
        manifest["model_resources"] = resources
        manifest["model_variants"] = variants
        manifest["material_palette"] = (
            external.materials if external else doc["materials"]
        )
        if external and image_resources:
            manifest["texture_resources"] = image_resources
            if basisu_resources:
                manifest["basisu_resources"] = basisu_resources
            if external.textures:
                manifest["textures"] = external.textures
            if external.samplers:
                manifest["samplers"] = external.samplers
    if support_region_tiles is not None:
        manifest["support_regions"] = paging_support_regions.partition(
            tiled, catalogue, edge, support_region_tiles, encode, write, support_storage
        )
    directory = encode(manifest)
    pin = None
    if pin_index or physics_pin or regional_pin or visual_regional_pin or first_frame_pin:
        if first_frame_pin:
            wire = paging_physics_directory.pack_first_frame(manifest, out, encode, write, support_storage, first_frame_cell)
        elif visual_regional_pin:
            wire = paging_physics_directory.pack_visual_district(manifest, out, encode, write, support_storage)
        elif regional_pin:
            wire = paging_physics_directory.pack_regional(manifest, out, encode, write, support_storage, bundle=bundled_pin)
        elif physics_pin:
            wire = paging_physics_directory.pack_physics(manifest, encode)
        else:
            wire = paging_physics_directory.envelope(b"ODI1", directory)
            if wire is None:
                raise ValueError("district directory exceeds the pinned reader limit")
        entry = write(wire, "district")
        pin = f"{entry['sha256']}:{entry['bytes']}"
    if previous_first_frame is not None:
        from paging_launch_update import update_reference
        pin = update_reference(out, previous_first_frame, wire, write)
    publish(out / "index.json", directory)
    print(
        f"{len(tiled)} tiles, {total_v} visual triangles, {total_c} collision triangles"
    )

    return pin


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("out", type=Path)
    parser.add_argument("--edge", type=int, default=128)
    parser.add_argument(
        "--visual-storage", choices=("demand", "shared", "embedded"), default="demand"
    )
    parser.add_argument(
        "--local-images",
        choices=("demand", "embedded"),
        default="demand",
        help="image storage in v4 local GLBs (embedded reproduces the first textured v4 writer)",
    )
    parser.add_argument(
        "--physics-storage",
        choices=("packed", "json"),
        default="packed",
        help="use lossless OCP1 pages or the historical JSON payload",
    )
    parser.add_argument(
        "--coarse-storage", choices=("auto", "ovc1"), default="auto",
        help="store the smaller lossless parent, or force the historical zlib GLB",
    )
    parser.add_argument("--support-storage", choices=("auto", "zlib"), default="auto",
                        help="choose the smaller support catalogue, or force the original zlib encoding")
    pins = parser.add_mutually_exclusive_group()
    pins.add_argument("--pin-physics", action="store_true",
                      help="publish a headless index with the physics and acoustic bindings")
    pins.add_argument("--pin-regional-physics", action="store_true",
                      help="keep detailed collision addresses in regional support files (four tiles wide by default)")
    pins.add_argument("--pin-bundled-physics", action="store_true",
                      help="include full packed collision pages when smaller than a regional support catalogue")
    pins.add_argument("--pin-first-frame", action="store_true",
                      help="bundle the visual directory and the coarse parent in a single immutable launch file")
    parser.add_argument("--previous-first-frame", help="offer an update from this previously published first-frame pin")
    parser.add_argument("--first-frame-cell", type=int, nargs=2, metavar=("X", "Z"),
                        help="also bundle the collision region for this spawn tile with --pin-first-frame")
    pins.add_argument("--pin-visual-district", action="store_true",
                      help="publish the full visual directory with regionally streamed collision data")
    pins.add_argument("--pin-index", action="store_true",
                        help="also publish a compressed, immutable directory and print its --index-pin value")
    parser.add_argument("--support-region-tiles", type=int, metavar="N",
                        help="also publish collision support in groups N tiles wide")
    args = parser.parse_args()
    pin = cook(args.source, args.out, args.edge, args.visual_storage, args.local_images, args.physics_storage, args.coarse_storage, args.support_storage, args.pin_index, args.support_region_tiles, args.pin_physics, args.pin_regional_physics, args.pin_bundled_physics, args.pin_visual_district, args.pin_first_frame, args.first_frame_cell, args.previous_first_frame)
    if pin:
        print(f"--index-pin {pin}")
