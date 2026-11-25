# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

"""Write one embedded indexed glTF 2.0 object with distinct role meshes."""

import json
import math
import struct


def difference(a, b):
    return tuple(x - y for x, y in zip(a, b, strict=True))


def cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def unit(v):
    length = math.hypot(*v)
    if length == 0 or not math.isfinite(length):
        raise ValueError("expected a finite nonzero direction")
    return tuple(x / length for x in v)


def face_normals(positions, indices):
    """Area-weighted vertex normals, with a valid fallback only for unused vertices."""
    sums = [[0.0, 0.0, 0.0] for _ in positions]
    fallback = {}
    for a, b, c in zip(indices[::3], indices[1::3], indices[2::3], strict=True):
        n = cross(difference(positions[b], positions[a]), difference(positions[c], positions[a]))
        if not any(n):
            raise ValueError("zero-area triangle")
        for i in (a, b, c):
            fallback.setdefault(i, n)
            for j, value in enumerate(n):
                sums[i][j] += value
    return [unit(v if any(v) else fallback.get(i, (0, 1, 0))) for i, v in enumerate(sums)]


class Glb:
    def __init__(self, object_id):
        self.node_meta = {"id": object_id, "collision": [], "acoustic": []}
        self.profile = {"version": 1, "id": object_id, "collisionMaterials": {}, "acousticMaterials": {}}
        self.node = {"name": object_id, "extras": {"osage": self.node_meta}}
        self.doc = {
            "asset": {"version": "2.0", "extras": {"osageObject": self.profile}},
            "scene": 0, "scenes": [{"nodes": [0]}], "nodes": [self.node],
            "meshes": [], "bufferViews": [], "accessors": [],
        }
        self.binary = bytearray()

    def accessor(self, rows, scalar, width, *, bounds=False, indices=False):
        component_type = {"f": 5126, "B": 5121, "I": 5125}[scalar]
        element = struct.Struct("<" + scalar * width)
        self.binary.extend(bytes(-len(self.binary) % 4))
        start = len(self.binary)
        for row in rows:
            self.binary.extend(element.pack(*row))
        views = self.doc["bufferViews"]
        accessors = self.doc["accessors"]
        descriptor = {
            "bufferView": len(views), "componentType": component_type,
            "count": len(rows), "type": ("SCALAR", "VEC2", "VEC3", "VEC4")[width - 1],
        }
        if scalar == "B":
            descriptor["normalized"] = True
        if bounds:
            # glTF requires the extrema of the encoded float32 coordinates.
            axes = list(zip(*rows))
            for name, op in (("min", min), ("max", max)):
                descriptor[name] = [struct.unpack("<f", struct.pack("<f", op(axis)))[0] for axis in axes]
        views.append({"buffer": 0, "byteOffset": start, "byteLength": len(self.binary) - start,
                      "target": 34963 if indices else 34962})
        accessors.append(descriptor)
        return len(accessors) - 1

    def primitive(self, positions, indices, attributes=None, material=None):
        if not positions or not indices or len(indices) % 3:
            raise ValueError("expected nonempty indexed triangles")
        if min(indices) < 0 or max(indices) >= len(positions):
            raise ValueError("triangle vertex index outside this mesh")
        if not all(math.isfinite(v) for row in positions for v in row):
            raise ValueError("non-finite position")
        # Check face area after packing, including for meshes with explicit normals.
        f32 = struct.Struct("<3f")
        positions = [f32.unpack(f32.pack(*p)) for p in positions]
        generated_normals = face_normals(positions, indices)
        attributes = dict(attributes or {})
        attributes.setdefault("NORMAL", generated_normals)
        refs = {"POSITION": self.accessor(positions, "f", 3, bounds=True)}
        for semantic, rows in attributes.items():
            if len(rows) != len(positions):
                raise ValueError(f"{semantic} count differs from POSITION count")
            scalar = "B" if semantic.startswith("COLOR_") else "f"
            refs[semantic] = self.accessor(rows, scalar, len(rows[0]))
        primitive = {"attributes": refs, "indices": self.accessor([(i,) for i in indices], "I", 1, indices=True)}
        if material is not None:
            primitive["material"] = material
        return primitive

    def mesh(self, name, primitives):
        if not primitives:
            raise ValueError(f"{name}: empty mesh")
        self.doc["meshes"].append({"name": name, "primitives": primitives})
        return len(self.doc["meshes"]) - 1

    def image(self, name, mime, encoded):
        self.binary.extend(bytes(-len(self.binary) % 4))
        start = len(self.binary)
        self.binary.extend(encoded)
        views = self.doc["bufferViews"]
        images = self.doc.setdefault("images", [])
        views.append({"buffer": 0, "byteOffset": start, "byteLength": len(encoded)})
        images.append({"name": name, "mimeType": mime, "bufferView": len(views) - 1})
        return len(images) - 1

    def texture(self, name, mime, encoded):
        image = self.image(name, mime, encoded)
        textures = self.doc.setdefault("textures", [])
        textures.append({"source": image})
        return len(textures) - 1

    def encode(self):
        if not self.doc["meshes"]:
            raise ValueError("object has no geometry")
        self.doc["buffers"] = [{"byteLength": len(self.binary)}]
        doc = json.dumps(self.doc, ensure_ascii=False, allow_nan=False, separators=(",", ":")).encode("utf-8")
        doc += b" " * (-len(doc) % 4)
        payload = self.binary + bytes(-len(self.binary) % 4)
        return (struct.pack("<4sII", b"glTF", 2, 28 + len(doc) + len(payload))
                + struct.pack("<I4s", len(doc), b"JSON") + doc
                + struct.pack("<I4s", len(payload), b"BIN\0") + payload)
