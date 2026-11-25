# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

"""Rigid frames represented as three column vectors followed by the origin."""

import math

from glb import cross, unit

I = ((1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0))
ZERO = (0.0, 0.0, 0.0)
Y_UP = ((1.0, 0.0, 0.0), (0.0, 0.0, -1.0), (0.0, 1.0, 0.0))
Z_UP = tuple(zip(*Y_UP))


def add(a, b):
    return tuple(x + y for x, y in zip(a, b, strict=True))


def dot(a, b):
    return sum(x * y for x, y in zip(a, b, strict=True))


def rotate(r, v):
    return tuple(dot(row, v) for row in zip(*r))


def compose(r, s):
    return tuple(rotate(r, v) for v in s)


def body_basis(direction):
    y = unit(direction)
    reference = I[2] if abs(y[2]) < 0.9 else I[0]
    x = unit(cross(y, reference))
    return (x, y, cross(x, y))


def quaternion(r):
    # Choose the largest quaternion component before dividing (works at pi).
    m = list(zip(*r))
    m00, m11, m22 = (m[i][i] for i in range(3))
    d = [1 + m00 - m11 - m22, 1 - m00 + m11 - m22, 1 - m00 - m11 + m22, 1 + m00 + m11 + m22]
    i = max(range(4), key=d.__getitem__)
    q = [0.0] * 4
    q[i] = math.sqrt(d[i]) / 2
    k = 4 * q[i]
    if i == 3:
        q[:3] = [(m[2][1] - m[1][2]) / k, (m[0][2] - m[2][0]) / k, (m[1][0] - m[0][1]) / k]
    else:
        j, l = (i + 1) % 3, (i + 2) % 3
        q[j], q[l], q[3] = (m[i][j] + m[j][i]) / k, (m[i][l] + m[l][i]) / k, (m[l][j] - m[j][l]) / k
    return tuple(q)


def axes_from_quaternion(q):
    x, y, z, w = q
    return (
        (1 - 2 * (y * y + z * z), 2 * (x * y + z * w), 2 * (x * z - y * w)),
        (2 * (x * y - z * w), 1 - 2 * (x * x + z * z), 2 * (y * z + x * w)),
        (2 * (x * z + y * w), 2 * (y * z - x * w), 1 - 2 * (x * x + y * y)),
    )


def assemble(parent, child):
    r, p = parent
    s, t = child
    return compose(r, s), add(p, rotate(r, t))


def relative_pose(parent, target, *, node=False):
    """Represent an authored bind pose in the orientation actually carried by its parent."""
    axes, origin = parent
    q = quaternion(axes)
    q_unit = tuple(v / math.hypot(*q) for v in q)
    # glTF node matrices are reduced to rigid rotations, while an authored body keeps its input quaternion.
    q = q_unit if node else q
    x, y, z, w = (-q[0], -q[1], -q[2], q[3])
    i, j, k, s = quaternion(target[0])
    product = (
        w * i + x * s + y * k - z * j,
        w * j - x * k + y * s + z * i,
        w * k + x * j - y * i + z * s,
        w * s - x * i - y * j - z * k,
    )
    length2 = dot(q, q)
    inverse_axes = tuple(zip(*axes_from_quaternion(q_unit)))
    delta = tuple(v - o for v, o in zip(target[1], origin, strict=True))
    return tuple(v / length2 for v in product), rotate(inverse_axes, delta)
