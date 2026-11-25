# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""World placements of a glTF source forest on the native reader's float grid."""

import math
import struct

_FLOAT = struct.Struct("<f")
IDENTITY = (
    1.0,
    0.0,
    0.0,
    0.0,
    0.0,
    1.0,
    0.0,
    0.0,
    0.0,
    0.0,
    1.0,
    0.0,
    0.0,
    0.0,
    0.0,
    1.0,
)


def f32(value):
    return _FLOAT.unpack(_FLOAT.pack(value))[0]


def local_matrix(node):
    if "matrix" in node:
        matrix = tuple(map(f32, node["matrix"]))
        if len(matrix) != 16 or matrix[3::4] != (0.0, 0.0, 0.0, 1.0):
            raise ValueError("invalid affine matrix")
        return matrix
    sx, sy, sz = map(f32, node.get("scale", (1, 1, 1)))
    x, y, z, w = map(f32, node.get("rotation", (0, 0, 0, 1)))
    tx, ty, tz = map(f32, node.get("translation", (0, 0, 0)))
    # Form the three basis vectors on the source float grid before applying scale.
    xx, yy, zz = (f32(2 * v * v) for v in (x, y, z))
    xy, xz, yz = (f32(2 * a * b) for a, b in ((x, y), (x, z), (y, z)))
    wx, wy, wz = (f32(2 * w * v) for v in (x, y, z))

    def diagonal(a, b):
        return f32(f32(1.0 - a) - b)

    a = (diagonal(yy, zz), f32(xy + wz), f32(xz - wy))
    b = (f32(xy - wz), diagonal(xx, zz), f32(yz + wx))
    c = (f32(xz + wy), f32(yz - wx), diagonal(xx, yy))
    return (
        *(f32(v * sx) for v in a),
        0.0,
        *(f32(v * sy) for v in b),
        0.0,
        *(f32(v * sz) for v in c),
        0.0,
        tx,
        ty,
        tz,
        1.0,
    )


def apply(matrix, vector, is_point=True):
    """Apply the linear part first. Translation affects only a point."""
    if matrix == IDENTITY:
        return tuple(vector)
    x, y, z = vector

    def component(i):
        xy = f32(f32(matrix[i] * x) + f32(matrix[4 + i] * y))
        xyz = f32(xy + f32(matrix[8 + i] * z))
        return f32(xyz + matrix[12 + i]) if is_point else xyz

    return tuple(component(i) for i in range(3))


def compose(parent, local):
    if parent == IDENTITY:
        return local
    if local == IDENTITY:
        return parent
    world = []
    for col in range(4):
        start = 4 * col
        world.extend(apply(parent, local[start : start + 3], is_point=col == 3))
        world.append(local[start + 3])
    return tuple(world)


def world_placements(doc):
    """Compose each active node once; lightweight root instances keep compact poses."""
    nodes = doc["nodes"]
    authored = bool(doc["asset"].get("extras", {}).get("osageObject"))
    result = {}
    pending = [(i, None) for i in doc["scenes"][doc["scene"]]["nodes"]]
    while pending:
        index, parent = pending.pop()
        node = nodes[index]
        matrix = local_matrix(node)
        if parent is not None:
            matrix = compose(parent, matrix)
        if not all(map(math.isfinite, matrix)):
            raise ValueError("non-finite district placement")
        if parent is None and "matrix" not in node and (not authored or "rotation" not in node):
            pose = {k: node[k] for k in ("scale", "rotation", "translation") if k in node}
        else:
            pose = {"matrix": matrix}
        result[index] = (pose, matrix)
        pending.extend((i, matrix) for i in node.get("children", []))
    return result
