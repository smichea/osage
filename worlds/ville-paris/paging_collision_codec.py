# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Lossless little-endian OCP1 collider pages. Shape identities and materials live in the tile directory."""

import hashlib
import json
import math
from struct import Struct, error

MAGIC = b"OCP1"
U32 = Struct("<I")
COUNTS = Struct("<II")
POINT = Struct("<3f")
WORDS = Struct("<3I")
PLANES = Struct("<10I")
BOX = Struct("<6I")
INDEX = {1: Struct("<3B"), 2: Struct("<3H"), 4: Struct("<3I")}
CORNERS = (0, 1, 2, 3, 4, 5, 6, 7, 3, 2, 5, 4, 2, 1, 6, 5, 1, 0, 7, 6, 0, 3, 4, 7)
FACES = tuple(
    triangle
    for k in range(0, 24, 4)
    for triangle in ((k, k + 1, k + 2), (k, k + 2, k + 3))
)
REVERSED = tuple((a, c, b) for a, b, c in FACES)


def index_width(vertex_count):
    return 1 if vertex_count <= 256 else 2 if vertex_count <= 65536 else 4


def ring_points(planes):
    a, b, y, c, d, e, f, top, g, h = planes
    return (
        (a, y, c),
        (a, y, d),
        (b, y, d),
        (b, y, c),
        (e, top, g),
        (e, top, h),
        (f, top, h),
        (f, top, g),
    )


def compact_ends(coordinates, triangles):
    if triangles == FACES:
        winding = 0
    elif triangles == REVERSED:
        winding = 128
    else:
        return None
    points = tuple(WORDS.iter_unpack(coordinates))
    eight = points[:8]
    if tuple(eight[i] for i in CORNERS) != points:
        return None
    # All comparisons here use IEEE bit patterns, including the sign of zero.
    a, y, c = eight[0]
    b = eight[2][0]
    d = eight[1][2]
    e, top, g = eight[4]
    f = eight[6][0]
    h = eight[5][2]
    planes = (a, b, y, c, d, e, f, top, g, h)
    if ring_points(planes) != eight:
        return bytes([1 | winding]) + coordinates[: 8 * POINT.size]
    if (e, f, g, h) == (b, a, c, d):
        return bytes([3 | winding]) + BOX.pack(a, b, y, c, d, top)
    return bytes([2 | winding]) + PLANES.pack(*planes)


def encode_collision(shapes):
    if not shapes:
        raise ValueError("empty collision page")
    result = bytearray(MAGIC + U32.pack(len(shapes)))
    for shape in shapes:
        vertices = shape["vertices"]
        triangles = shape["triangles"]
        if not vertices or not triangles:
            raise ValueError("empty collision mesh")
        try:
            if not all(len(p) == 3 and all(map(math.isfinite, p)) for p in vertices):
                raise ValueError("collision vertices must be finite triples")
            positions = b"".join(POINT.pack(*p) for p in vertices)
        except (OverflowError, error) as ex:
            raise ValueError("collision vertex outside IEEE binary32") from ex
        # Only a matching complete face table may elide vertices and indices.
        if (
            len(vertices) == 24
            and len(triangles) == 12
            and all(type(i) is int for face in triangles for i in face)
        ):
            compact = compact_ends(positions, tuple(map(tuple, triangles)))
            if compact is not None:
                result.extend(compact)
                continue
        count = len(vertices)
        indices = INDEX[index_width(count)]
        result.append(0)
        result.extend(COUNTS.pack(count, len(triangles)))
        result.extend(positions)
        for face in triangles:
            if len(face) != 3 or not all(
                type(i) is int and 0 <= i < count for i in face
            ):
                raise ValueError("invalid collision triangle vertex index")
            result.extend(indices.pack(*face))
    return bytes(result)


def decode_collision(raw, layer):
    """Read the physical payload in directory order; legacy JSON retains its original shape fields."""
    suffix = layer["file"].rsplit(".", 1)[-1]
    ids = layer["ids"]
    nodes = layer.get("source_nodes", [])
    materials = layer.get("materials", [])
    if nodes and len(nodes) != len(ids) or materials and len(materials) != len(ids):
        raise ValueError("collision owner or material count differs from directory")
    if suffix == "json":
        shapes = json.loads(raw)
        if (
            not isinstance(shapes, list)
            or [s["id"] for s in shapes] != ids
            or (nodes and [s.get("source_node") for s in shapes] != nodes)
        ):
            raise ValueError("collision shape identity differs from directory")
    elif suffix == "cpage":
        if not raw.startswith(MAGIC) or len(raw) < 8:
            raise ValueError("invalid collision magic/version or header")
        count = U32.unpack_from(raw, 4)[0]
        if count == 0 or count != len(ids) or count > (len(raw) - 8) // 24:
            raise ValueError("collision record count differs from directory or payload")
        if any(type(n) is not int or n < 0 for n in nodes):
            raise ValueError("invalid collision owner")
        data = memoryview(raw)
        pos = 8

        def take(size):
            nonlocal pos
            if size > len(data) - pos:
                raise ValueError("truncated collision record")
            end = pos + size
            result = data[pos:end]
            pos = end
            return result

        shapes = []
        total = 0
        for slot, sid in enumerate(ids):
            tag = take(1)[0]
            if tag == 0:
                nv, nt = COUNTS.unpack(take(COUNTS.size))
                codec = INDEX[index_width(nv)]
                if nv == 0 or nt == 0 or 12 * nv + codec.size * nt > len(data) - pos:
                    raise ValueError("invalid collision mesh counts")
                if total + nt > layer["triangles"]:
                    raise ValueError("collision triangle count exceeds directory")
                vertex_rows = POINT.iter_unpack(take(nv * POINT.size))
                vertices = [list(p) for p in vertex_rows]
                triangles = [
                    list(face) for face in codec.iter_unpack(take(nt * codec.size))
                ]
                if any(i >= nv for face in triangles for i in face):
                    raise ValueError("invalid collision triangle vertex index")
            elif tag in (1, 2, 3, 129, 130, 131):
                if total + 12 > layer["triangles"]:
                    raise ValueError("collision triangle count exceeds directory")
                kind = tag & 127
                if kind == 1:
                    eight = list(POINT.iter_unpack(take(8 * POINT.size)))
                else:
                    if kind == 2:
                        planes = PLANES.unpack(take(PLANES.size))
                    else:
                        a, b, y, c, d, top = BOX.unpack(take(BOX.size))
                        planes = (a, b, y, c, d, b, a, top, c, d)
                    eight = [POINT.unpack(WORDS.pack(*p)) for p in ring_points(planes)]
                vertices = [list(eight[i]) for i in CORNERS]
                triangles = list(map(list, REVERSED if tag & 128 else FACES))
            else:
                raise ValueError("unknown collision record tag")
            if not all(all(map(math.isfinite, p)) for p in vertices):
                raise ValueError("nonfinite collision vertex")
            if "bounds" in layer:
                lo, hi = layer["bounds"]
                if any(
                    not all(lo[i] <= p[i] <= hi[i] for i in range(3)) for p in vertices
                ):
                    raise ValueError("collision vertex outside page bounds")
            total += len(triangles)
            shape = {"id": sid, "vertices": vertices, "triangles": triangles}
            if nodes:
                shape["source_node"] = nodes[slot]
            shapes.append(shape)
        if pos != len(raw):
            raise ValueError("trailing collision payload")
    else:
        raise ValueError("unknown collision file format")
    if sum(len(s["triangles"]) for s in shapes) != layer["triangles"]:
        raise ValueError("collision triangle count differs from directory")
    return shapes


def read_collision(directory, layer):
    path = directory / layer["file"]
    raw = path.read_bytes()
    if len(raw) != layer["bytes"] or hashlib.sha256(raw).hexdigest() != layer["sha256"]:
        raise ValueError("collision page size or SHA-256 differs from directory")
    return decode_collision(raw, layer)
