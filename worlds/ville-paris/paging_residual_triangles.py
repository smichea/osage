# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Retain the exact corners of one omitted triangle when its AABB includes empty space."""


def observe(barriers, key, points):
    if key in barriers:
        count, _ = barriers[key]
        # Multiple missing faces, including two halves of a rectangular wall, keep
        # the original box. No partial description may prove the whole group clear.
        barriers[key] = (count + 1, None)
        return
    _, low, high = key
    masks = []
    for p in points:
        if any(x != lo and x != hi for x, lo, hi in zip(p, low, high, strict=True)):
            barriers[key] = (1, None)
            return
        masks.append(sum(1 << axis for axis in range(3) if p[axis] != low[axis]))
    a, b, c = sorted(masks)
    code = a | b << 3 | c << 6 if a < b < c else None
    barriers[key] = (1, code)
