# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

"""Bound symmetric Hausdorff distance between two indexed triangle surfaces in local metres."""

import heapq
import itertools
import math
import struct
import sys
from functools import lru_cache


def _sub(a, b):
    return a[0] - b[0], a[1] - b[1], a[2] - b[2]


def _dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def _cross(a, b):
    return (
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    )


def _lerp(a, b, t):
    t = max(0.0, min(1.0, t))
    return a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]), a[2] + t * (b[2] - a[2])


class _Face:
    def __init__(self, points):
        a, b, c = self.points = points
        self.edges = [(a, b, _sub(b, a)), (b, c, _sub(c, b)), (c, a, _sub(a, c))]
        self.ab = self.edges[0][2]
        self.ac = _sub(c, a)
        self.normal = _cross(self.ab, self.ac)
        self.normal2 = _dot(self.normal, self.normal)
        self.lo = tuple(min(axis) for axis in zip(*points))
        self.hi = tuple(max(axis) for axis in zip(*points))

    def distances(self, p):
        """A convex witness gives an upper bound. A separating plane gives a lower bound."""
        a, b, c = self.points
        if p in self.points:
            return 0.0, 0.0
        q = None
        if self.normal2:
            ap = _sub(p, a)
            u = _dot(_cross(ap, self.ac), self.normal) / self.normal2
            v = _dot(_cross(self.ab, ap), self.normal) / self.normal2
            if u >= 0 and v >= 0 and u + v <= 1:
                # Two clamped edge interpolations keep the witness inside even near a thin edge.
                q = _lerp(_lerp(a, b, u / (1.0 - v) if v < 1 else 0.0), c, v)
        if q is None:
            best = math.inf
            for start, end, edge in self.edges:
                length2 = _dot(edge, edge)
                t = _dot(_sub(p, start), edge) / length2 if length2 else 0.0
                candidate = _lerp(start, end, t)
                distance = math.dist(p, candidate)
                if distance < best:
                    q, best = candidate, distance
        upper = math.dist(p, q)
        if upper == 0:
            return 0.0, 0.0
        normal = _sub(p, q)
        lower = min(_dot(_sub(p, v), normal) for v in self.points) / upper
        return max(0.0, lower), upper


class _Tree:
    def __init__(self, faces, indices):
        self.lo = tuple(min(faces[i].lo[k] for i in indices) for k in range(3))
        self.hi = tuple(max(faces[i].hi[k] for i in indices) for k in range(3))
        self.leaves, self.children = indices, ()
        if len(indices) > 8:
            k = max(range(3), key=lambda k: self.hi[k] - self.lo[k])
            indices.sort(key=lambda i: faces[i].lo[k] + faces[i].hi[k])
            mid = len(indices) // 2
            self.children = _Tree(faces, indices[:mid]), _Tree(faces, indices[mid:])
            self.leaves = ()

    def distance(self, p):
        return math.hypot(
            *(
                max(lo - v, v - hi, 0.0)
                for lo, hi, v in zip(self.lo, self.hi, p, strict=True)
            )
        )


class _Surface:
    def __init__(self, triangles):
        self.faces = [_Face(t) for t in triangles]
        self.keys = {tuple(sorted(t)) for t in triangles}
        self.tree = _Tree(self.faces, list(range(len(triangles))))
        # Cache within this conversion alone. Adjacent source triangles and their children share vertices.
        self.distances = lru_cache(maxsize=32768)(
            lambda i, p: self.faces[i].distances(p)
        )
        self.nearest = lru_cache(maxsize=16384)(self._nearest)

    def _nearest(self, p):
        lower, upper, winner = math.inf, math.inf, None
        serial = itertools.count()
        queue = [(self.tree.distance(p), next(serial), self.tree)]
        while queue:
            box_distance, _, node = heapq.heappop(queue)
            if box_distance >= upper:
                lower = min(lower, box_distance)
                break
            for child in node.children:
                heapq.heappush(queue, (child.distance(p), next(serial), child))
            for i in node.leaves:
                lo, hi = self.distances(i, p)
                lower = min(lower, lo)
                if hi < upper:
                    upper, winner = hi, i
        return lower, upper, winner

    def enclose(self, points):
        # Distance to any one convex target face on a source triangle is bounded by its three corners.
        # The nearest target face can change within a source triangle. Corners and centre suggest witnesses.
        centre = tuple(sum(axis) / 3 for axis in zip(*points))
        samples = [self.nearest(p) for p in (*points, centre)]
        lower = max(s[0] for s in samples)
        candidates = {s[2] for s in samples}
        upper = min(max(self.distances(i, p)[1] for p in points) for i in candidates)
        return lower, upper


def _ceil_f32(value):
    if value > float.fromhex("0x1.fffffep127"):
        raise ValueError("visual deviation exceeds the finite float32 range")
    packed = struct.pack("<f", value)
    (nearest,) = struct.unpack("<f", packed)
    if nearest >= value:
        return nearest
    (bits,) = struct.unpack("<I", packed)
    (rounded,) = struct.unpack("<f", struct.pack("<I", bits + 1))
    return rounded


def deviation(high, lod, tolerance, *, max_splits=16384):
    """Enclose both directed distances. An exhausted refinement budget still returns a valid upper bound."""
    if not math.isfinite(tolerance) or tolerance <= 0:
        raise ValueError("LOD error tolerance must be finite and positive")
    if not high or not lod:
        raise ValueError("LOD deviation requires two nonempty triangle meshes")
    surfaces = _Surface(high), _Surface(lod)
    # All input coordinates are exact float32s; subsequent vector arithmetic uses binary64. A clamped
    # convex witness / supporting plane is valid for any parameter / direction. This guard covers the
    # rounded vector arithmetic, centre query and up to 48 midpoint interpolations of source vertices.
    scale = max(
        abs(v)
        for surface in surfaces
        for bound in (surface.tree.lo, surface.tree.hi)
        for v in bound
    )
    guard = scale * (2048 * sys.float_info.epsilon)
    serial, queue = itertools.count(), []
    lower = 0.0

    def enqueue(target, points, depth, inherited=math.inf):
        nonlocal lower
        if tuple(sorted(points)) in surfaces[target].keys:
            if depth == 0:
                return
            hi = 0.0
        else:
            lo, hi = surfaces[target].enclose(points)
            lower = max(lower, lo - guard)
        upper = min(inherited, hi + guard)
        heapq.heappush(queue, (-upper, next(serial), target, points, depth))

    for direction, triangles in enumerate((lod, high)):
        for points in triangles:
            enqueue(direction, points, 0)
    splits = 0
    while queue and _ceil_f32(-queue[0][0]) - lower > tolerance and splits < max_splits:
        neg_upper, _, target, tri, depth = queue[0]
        if depth >= 48:
            break
        heapq.heappop(queue)
        i = max(range(3), key=lambda i: math.dist(tri[i], tri[(i + 1) % 3]))
        a, b, c = tri[i], tri[(i + 1) % 3], tri[(i + 2) % 3]
        mid = tuple((x + y) * 0.5 for x, y in zip(a, b, strict=True))
        enqueue(target, (a, mid, c), depth + 1, -neg_upper)
        enqueue(target, (mid, b, c), depth + 1, -neg_upper)
        splits += 1
    upper = _ceil_f32(-queue[0][0]) if queue else 0.0
    # Drop bounded caches promptly when a multi-node object carries many independent mesh pairs.
    for surface in surfaces:
        del surface.distances, surface.nearest
    result = {
        "lowerM": lower,
        "upperM": upper,
        "requestedGapM": tolerance,
        "gapReached": upper - lower <= tolerance,
        "highTriangles": len(high),
        "lodTriangles": len(lod),
        "subdivisions": splits,
    }
    if not result["gapReached"]:
        result["stoppedAt"] = (
            "subdivision-limit" if splits >= max_splits else "depth-limit"
        )
    return result


def _accessor(glb, index):
    accessor = glb.doc["accessors"][index]
    view = glb.doc["bufferViews"][accessor["bufferView"]]
    scalar = {5121: "B", 5123: "H", 5125: "I", 5126: "f"}[accessor["componentType"]]
    width = {"VEC3": 3, "SCALAR": 1}[accessor["type"]]
    row = struct.Struct("<" + width * scalar)
    start = view.get("byteOffset", 0) + accessor.get("byteOffset", 0)
    stride = view.get("byteStride", row.size)
    return [
        row.unpack_from(glb.binary, start + i * stride)
        for i in range(accessor["count"])
    ]


def _triangles(glb, mesh):
    result = []
    for primitive in glb.doc["meshes"][mesh]["primitives"]:
        positions = _accessor(glb, primitive["attributes"]["POSITION"])
        indices = [i for (i,) in _accessor(glb, primitive["indices"])]
        result.extend(
            (positions[a], positions[b], positions[c])
            for a, b, c in zip(indices[::3], indices[1::3], indices[2::3], strict=True)
        )
    return result


def measure_lods(glb, tolerance, report):
    """Measure the surfaces from the exact encoded position/index accessors, in the node's common frame."""
    if not math.isfinite(tolerance) or tolerance <= 0:
        raise ValueError("LOD error tolerance must be finite and positive")
    measurements, pairs, meshes = [], {}, {}

    def triangles(index):
        if index not in meshes:
            meshes[index] = _triangles(glb, index)
        return meshes[index]

    for node in glb.doc["nodes"]:
        meta = node["extras"]["osage"]
        for lod in meta.get("visualLods", []):
            pair = node["mesh"], lod["mesh"]
            if pair not in pairs:
                pairs[pair] = deviation(
                    triangles(pair[0]), triangles(pair[1]), tolerance
                )
            result = pairs[pair]
            lod["maxDeviationM"] = result["upperM"]
            measurements.append({"node": meta["id"], "level": lod["level"], **result})
    report["visualDeviation"] = measurements
