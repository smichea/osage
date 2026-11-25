#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Generate original room layouts and qualify finite-aperture acoustic paths via the object inspector."""

import argparse
import copy
import hashlib
import json
import math
import runpy
import shutil
import struct
import subprocess
from dataclasses import dataclass, replace
from itertools import pairwise
from pathlib import Path

from paging_checks import require


@dataclass(frozen=True)
class Door:
    key: str
    left: str
    right: str
    axis: int  # 0 for an X wall, 1 for a Z wall in the horizontal oracle
    plane: float
    lo: float
    hi: float
    closure: float = 0.0

    def bounds(self):
        center = (self.lo + self.hi) / 2
        half = (self.hi - self.lo) / 2 * math.sqrt(1 - self.closure)
        return center - half, center + half

    def at(self, t):
        return (self.plane, t) if self.axis == 0 else (t, self.plane)

    def corners(self):
        return [
            [0, y, w] if self.axis == 0 else [w, y, 0]
            for y, w in [(0.5, self.lo), (3.5, self.lo), (3.5, self.hi), (0.5, self.hi)]
        ]


@dataclass
class Layout:
    rooms: dict
    doors: list


def chain():
    rooms = {chr(65 + i): (10 * i, -6, 10 * i + 10, 6) for i in range(6)}
    intervals = [(-1, 1), (2, 4), (-4, -2), (1, 3), (-1, 1)]
    doors = [
        Door(f"door-{i + 1}", chr(65 + i), chr(66 + i), 0, 10 * (i + 1), lo, hi)
        for i, (lo, hi) in enumerate(intervals)
    ]
    return Layout(rooms, doors)


def branch(closed=False):
    rooms = {
        f"{row}{i}": (10 * i, z, 10 * i + 10, z + 10)
        for row, z in (("L", 0), ("U", 10))
        for i in range(3)
    }
    return Layout(
        rooms,
        [
            Door("lower-west", "L0", "L1", 0, 10, 4, 6, 1.0 if closed else 0.0),
            Door("lower-east", "L1", "L2", 0, 20, 4, 6),
            Door("turn-west", "L0", "U0", 1, 10, 4, 6),
            Door("upper-west", "U0", "U1", 0, 10, 12, 14),
            Door("upper-east", "U1", "U2", 0, 20, 12, 14),
            Door("turn-east", "U2", "L2", 1, 10, 24, 26),
        ],
    )


def ranked():
    layout = branch()
    layout.doors = [
        replace(d, lo=0.125, hi=0.5)
        if d.key.startswith("lower")
        else replace(d, lo=10.125, hi=10.875)
        if d.key.startswith("upper")
        else d
        for d in layout.doors
    ]
    return layout


def cyclic_limit():
    rooms = {f"r{i}": (10 * i, -2, 10 * i + 10, 2) for i in range(14)}
    doors = [
        Door(f"{i:02d}-{side}", f"r{i}", f"r{i + 1}", 0, 10 * i + 10, lo, hi)
        for i in range(13)
        for side, lo, hi in [("a", -0.625, -0.125), ("b", 0.125, 0.625)]
    ]
    return Layout(rooms, doors)


def long_corridor():
    n = 192
    rooms = {f"r{i}": (10 * i, -2, 10 * (i + 1), 2) for i in range(n)}
    doors = [
        Door(f"gate-{i}", f"r{i}", f"r{i + 1}", 0, 10 * (i + 1), -1, 1)
        for i in range(n - 1)
    ]
    return Layout(rooms, doors)


def glb_template():
    # A single original marker on the district root keeps the normal renderable-object route exercised.
    positions = [(-8.0, 0.0, -8.0), (-8.0, 0.0, -7.0), (-7.0, 0.0, -8.0)]
    blob = struct.pack(
        "<18f3H", *[v for p in positions + [(0.0, 1.0, 0.0)] * 3 for v in p], 0, 1, 2
    )
    doc = {
        "asset": {
            "version": "2.0",
            "extras": {
                "osageObject": {
                    "version": 1,
                    "id": "quartier-interieurs",
                    "collisionMaterials": {},
                    "acousticMaterials": {},
                }
            },
        },
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [],
        "buffers": [{"byteLength": len(blob)}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": o, "byteLength": n, "target": t}
            for o, n, t in [(0, 36, 34962), (36, 36, 34962), (72, 6, 34963)]
        ],
        "accessors": [
            {
                "bufferView": 0,
                "componentType": 5126,
                "type": "VEC3",
                "count": 3,
                "min": [-8, 0, -8],
                "max": [-7, 0, -7],
            },
            {"bufferView": 1, "componentType": 5126, "type": "VEC3", "count": 3},
            {"bufferView": 2, "componentType": 5123, "type": "SCALAR", "count": 3},
        ],
        "meshes": [
            {"primitives": [{"attributes": {"POSITION": 0, "NORMAL": 1}, "indices": 2}]}
        ],
    }
    return doc, blob


def node(ident, position=None, **metadata):
    data = {"name": ident, "extras": {"osage": {"id": ident, **metadata}}}
    if position:
        data["translation"] = position
    return data


def building_offset(i):
    return [80 * (i % 32), 0, 40 * (i // 32)]


def build(layout, count=1):
    doc, blob = glb_template()
    nodes = doc["nodes"]
    nodes.append(dict(node("quartier"), children=[], mesh=0))
    for i in range(count):
        prefix = f"b{i:04d}."
        parent = len(nodes)
        nodes[0]["children"].append(parent)
        nodes.append(dict(node(prefix + "interieur", building_offset(i)), children=[]))
        for name, (left, near, right, far) in layout.rooms.items():
            nodes[parent]["children"].append(len(nodes))
            nodes.append(
                node(
                    prefix + name,
                    [left, 0, near],
                    acousticRoom={
                        "boundsM": [[0, 0, 0], [right - left, 4, far - near]]
                    },
                )
            )
        for door in layout.doors:
            nodes[parent]["children"].append(len(nodes))
            translate = [door.plane, 0, 0] if door.axis == 0 else [0, 0, door.plane]
            nodes.append(
                node(
                    prefix + door.key,
                    translate,
                    acousticPortal={
                        "rooms": [prefix + door.left, prefix + door.right],
                        "cornersM": door.corners(),
                        "closureFraction": door.closure,
                    },
                )
            )
    return doc, blob


def write(path, doc, blob):
    header = json.dumps(doc, separators=(",", ":"), allow_nan=False).encode()
    header += b" " * (-len(header) % 4)
    binary = blob + b"\0" * (-len(blob) % 4)
    chunks = (
        struct.pack("<I4s", len(header), b"JSON")
        + header
        + struct.pack("<I4s", len(binary), b"BIN\0")
        + binary
    )
    path.write_bytes(struct.pack("<4sII", b"glTF", 2, 12 + len(chunks)) + chunks)
    return hashlib.sha256(path.read_bytes()).hexdigest()


def generate(out, count):
    out.mkdir(parents=True, exist_ok=True)
    long = chain()
    fixtures = {
        "chain": (long, 1),
        "district": (long, count),
        "branch": (branch(), 1),
        "detour": (branch(True), 1),
        "ranked": (ranked(), 1),
        "cycle-limit": (cyclic_limit(), 1),
        "deep": (long_corridor(), 1),
    }
    for suffix, closure in [("half", 0.5), ("closed", 1)]:
        doors = long.doors.copy()
        original = doors[0]
        doors[0] = Door(
            original.key,
            original.left,
            original.right,
            original.axis,
            original.plane,
            original.lo,
            original.hi,
            closure,
        )
        fixtures[suffix] = (Layout(long.rooms, doors), 1)
    digests = {}
    for ident, (layout, copies) in fixtures.items():
        digests[ident] = write(out / (ident + ".glb"), *build(layout, copies))
    basic, blob = build(long)

    def changed(portal):
        return next(
            n["extras"]["osage"]["acousticPortal"]
            for n in portal["nodes"]
            if n["name"] == "b0000.door-1"
        )

    rotated = copy.deepcopy(basic)
    parent = rotated["nodes"][1]
    parent.pop("translation", None)
    parent["matrix"] = [0, 0, -1, 0, 0, 1, 0, 0, 1, 0, 0, 0, 100, 4, 30, 1]
    digests["rotated"] = write(out / "rotated.glb", rotated, blob)
    diamond = copy.deepcopy(basic)
    changed(diamond)["cornersM"] = [
        [0, 2, -1.5],
        [0, 0.75, 0],
        [0, 2, 1.5],
        [0, 3.25, 0],
    ]
    digests["diamond"] = write(out / "diamond.glb", diamond, blob)
    staggered = copy.deepcopy(basic)
    changed(staggered)["cornersM"] = [[0, 3, -1], [0, 3.5, -1], [0, 3.5, 0], [0, 3, 0]]
    gate = next(
        n["extras"]["osage"]["acousticPortal"]
        for n in staggered["nodes"]
        if n["name"] == "b0000.door-2"
    )
    gate["cornersM"] = [[0, 0.5, 2], [0, 1, 2], [0, 1, 3], [0, 0.5, 3]]
    digests["staggered-height"] = write(out / "staggered-height.glb", staggered, blob)
    overlap = copy.deepcopy(basic)
    overlap["nodes"][1]["children"].append(len(overlap["nodes"]))
    overlap["nodes"].append(
        node(
            "b0000.inner", [3, 1, -1], acousticRoom={"boundsM": [[0, 0, 0], [2, 2, 2]]}
        )
    )
    digests["overlap"] = write(out / "overlap.glb", overlap, blob)

    def same_side(doc):
        next(n for n in doc["nodes"] if n["name"] == "b0000.B")["translation"] = [
            0,
            0,
            -6,
        ]

    for ident, alter in [
        ("nonplanar", lambda d: changed(d)["cornersM"][2].__setitem__(0, 0.125)),
        ("wrong-neighbour", lambda d: changed(d)["rooms"].__setitem__(1, "b0000.C")),
        ("missing-neighbour", lambda d: changed(d)["rooms"].__setitem__(1, "missing")),
        ("self-neighbour", lambda d: changed(d)["rooms"].__setitem__(1, "b0000.A")),
        ("bad-closure", lambda d: changed(d).__setitem__("closureFraction", 1.125)),
        ("same-side", same_side),
        (
            "bad-bounds",
            lambda d: d["nodes"][2]["extras"]["osage"]["acousticRoom"]["boundsM"][
                1
            ].__setitem__(1, 0),
        ),
        ("outside-wall", lambda d: changed(d)["cornersM"][2].__setitem__(1, 5)),
        (
            "unordered",
            lambda d: changed(d).__setitem__(
                "cornersM", [changed(d)["cornersM"][i] for i in (0, 2, 1, 3)]
            ),
        ),
    ]:
        variant = copy.deepcopy(basic)
        alter(variant)
        digests[ident] = write(out / (ident + ".glb"), variant, blob)
    return fixtures, digests


def interval_on_segment(a, b, door):
    """Parameter interval in which a closed segment intersects the aperture at ear height."""
    axis, cross = door.axis, 1 - door.axis
    delta = [y - x for x, y in zip(a, b, strict=True)]
    lo, hi = door.bounds()
    eps = 1e-10
    if delta[axis]:
        t = (door.plane - a[axis]) / delta[axis]
        w = a[cross] + t * delta[cross]
        if -eps <= t <= 1 + eps and lo - eps <= w <= hi + eps:
            return (max(0.0, min(1.0, t)),) * 2
        return None
    if abs(a[axis] - door.plane) > eps:
        return None
    if not delta[cross]:
        return (0.0, 1.0) if lo - eps <= a[cross] <= hi + eps else None
    s, t = sorted((limit - a[cross]) / delta[cross] for limit in (lo, hi))
    return (max(0.0, s), min(1.0, t)) if max(0.0, s) <= min(1.0, t) + eps else None


def visible(a, b, doors):
    earliest = 0.0
    for door in doors:
        interval = interval_on_segment(a, b, door)
        if interval is None or interval[1] < earliest - 1e-10:
            return False
        earliest = max(earliest, interval[0])
    return True


def corridor(a, b, doors):
    """At mid-height, every bend in a shortest path lies at a gate endpoint."""
    # Dynamic programming on the ordered visibility graph of the gate endpoints.
    # An interior crossing cannot bend a shortest straight segment in a convex room.
    stages = [[a]] + [[d.at(w) for w in d.bounds()] for d in doors] + [[b]]
    previous = [(-1, a, 0.0)]
    for stage, points in enumerate(stages[1:]):
        reached = []
        for q in points:
            reached.append(
                (
                    stage,
                    q,
                    min(
                        cost + math.dist(p, q)
                        for i, p, cost in previous
                        if visible(p, q, doors[i + 1 : stage])
                    ),
                )
            )
        previous.extend(reached)
    return previous[-1][2]


def candidates(layout, source, target, a, b):
    routes = []

    def visit(rooms, doors):
        here = rooms[-1]
        if here == target:
            routes.append(
                (corridor(a, b, doors), tuple(rooms), tuple(d.key for d in doors))
            )
            return
        for door in layout.doors:
            if door.closure == 1 or here not in (door.left, door.right):
                continue
            there = door.right if door.left == here else door.left
            if there not in rooms:
                visit([*rooms, there], [*doors, door])

    visit([source], [])
    return sorted(routes)


def polygon_nearest(point, polygon):
    """Project a test point outside a convex polygon onto its boundary."""
    best = None
    for a, b in zip(polygon, polygon[1:] + polygon[:1], strict=True):
        step = [y - x for x, y in zip(a, b, strict=True)]
        t = sum((v - x) * d for v, x, d in zip(point, a, step, strict=True)) / sum(
            d * d for d in step
        )
        p = [x + min(1.0, max(0.0, t)) * d for x, d in zip(a, step, strict=True)]
        if best is None or math.dist(point, p) < math.dist(point, best):
            best = p
    return best


@dataclass(frozen=True)
class Query:
    name: str
    a: tuple
    b: tuple
    room_a: str = ""
    room_b: str = ""
    status: str = "connected"
    building: int = 0


def checks(count):
    close = 2.0**-15
    base = [
        Query("same-room", (2, 2, 0), (8, 2, 4), "A", "A"),
        Query("aligned", (2, 2, 0), (18, 2, 0), "A", "B"),
        Query("grazing-edge", (2, 2, 1), (18, 2, 1), "A", "B"),
        Query("one-offset", (2, 2, 3), (18, 2, 3), "A", "B"),
        Query("two-offset", (2, 2, -2), (28, 2, -2), "A", "C"),
        Query("three", (2, 2, 0), (38, 2, 0), "A", "D"),
        Query("four", (2, 2, 0), (48, 2, 0), "A", "E"),
        Query("five", (2, 2, 0), (58, 2, 0), "A", "F"),
        Query("upper-corner", (2, 3.875, 3), (18, 3.875, 3), "A", "B"),
        Query("shared-wall", (10, 2, 0), (18, 2, 0), status="ambiguous"),
        Query("wall-sides", (10 - close, 2, 0), (10 + close, 2, 0), "A", "B"),
        Query("outside", (-close, 2, 0), (8, 2, 0), status="outside"),
        Query("outer-boundary", (0, 4, 0), (8, 4, 0), "A", "A"),
    ]
    district = [base[6], replace(base[6], name="last-building", building=count - 1)]
    if count > 1:
        dx, dy, dz = building_offset(count - 1)
        district.append(
            Query(
                "separate-buildings",
                (2, 2, 0),
                (dx + 8, dy + 2, dz),
                status="disconnected",
            )
        )
    return {
        "chain": base,
        "rotated": [base[2], base[4], base[6], base[9], base[10], base[11]],
        "half": [base[1], base[3], base[8]],
        "closed": [replace(base[1], status="disconnected"), base[0]],
        "branch": [Query("direct", (2, 2, 5), (28, 2, 5), "L0", "L2")],
        "detour": [Query("diverted", (2, 2, 5), (28, 2, 5), "L0", "L2")],
        "ranked": [Query("four-beat-two", (2, 2, 9.5), (28, 2, 9.5), "L0", "L2")],
        "diamond": [Query("sloping-edge", (2, 3.5, 2), (18, 3.5, 2), "A", "B")],
        "staggered-height": [
            Query("two-corners-3d", (2, 0.5, 3), (28, 3.5, -1), "A", "C")
        ],
        "overlap": [
            Query("nested-overlap", (4, 2, 0), (8, 2, 0), status="ambiguous"),
            base[1],
        ],
        "cycle-limit": [
            Query("early-direct", (2, 2, -0.375), (138, 2, -0.375), "r0", "r13"),
            Query("late-direct", (2, 2, 0.375), (138, 2, 0.375), "r0", "r13"),
        ],
        "deep": [Query("191-openings", (2, 2, 0), (1918, 2, 0), "r0", "r191")],
        "district": district,
    }


def world_point(p, offset, rotated):
    x, y, z = p
    if rotated:
        return (100 + z, 4 + y, 30 - x)
    return tuple(a + b for a, b in zip(p, offset, strict=True))


def local_point(p, offset, rotated):
    x, y, z = p
    if rotated:
        return (30 - z, y - 4, x - 100)
    return tuple(a - b for a, b in zip(p, offset, strict=True))


METRE_TOLERANCE = 2e-6
DIAMOND = [(2.0, -1.5), (0.75, 0.0), (2.0, 1.5), (3.25, 0.0)]


def connected(scene, layout, q, path, offset):
    prefix = f"b{q.building:04d}."
    ids = path["rooms"] + path["portals"]
    require(
        all(v.startswith(prefix) for v in ids), f"{q.name} crossed a different building"
    )
    rooms = tuple(s.removeprefix(prefix) for s in path["rooms"])
    doors = tuple(s.removeprefix(prefix) for s in path["portals"])
    points = [local_point(p, offset, scene == "rotated") for p in path["points_m"]]
    require(
        len(points) == len(doors) + 2
        and len(rooms) == len(doors) + 1
        and rooms[0] == q.room_a
        and rooms[-1] == q.room_b
        and math.dist(q.a, points[0]) < 1e-9
        and math.dist(q.b, points[-1]) < 1e-9,
        f"{scene}/{q.name} endpoints, owners or crossing count",
    )
    by_id = {d.key: d for d in layout.doors}
    for i, key in enumerate(doors):
        d = by_id[key]
        require(
            d.closure < 1 and {d.left, d.right} == set(rooms[i : i + 2]),
            f"{q.name} invalid crossing {key}",
        )
        x, y, z = points[i + 1]
        if scene == "diamond":
            require(
                abs(x - 10) < 1e-9 and abs(y - 2) / 1.25 + abs(z) / 1.5 <= 1 + 1e-9,
                f"{q.name} outside diamond",
            )
        elif scene == "staggered-height":
            ylo, yhi, zlo, zhi = (3, 3.5, -1, 0) if key == "door-1" else (0.5, 1, 2, 3)
            require(
                abs(x - d.plane) < 1e-9
                and ylo - 1e-8 <= y <= yhi + 1e-8
                and zlo - 1e-8 <= z <= zhi + 1e-8,
                f"{q.name} outside staggered rectangle",
            )
        else:
            h = (x, z)
            lo, hi = d.bounds()
            require(
                abs(h[d.axis] - d.plane) < 1e-7
                and lo - 1e-7 <= h[1 - d.axis] <= hi + 1e-7
                and abs(y - 2) <= 1.5 * math.sqrt(1 - d.closure) + 1e-7,
                f"{scene}/{q.name} outside finite aperture",
            )
    length = sum(math.dist(a, b) for a, b in pairwise(points))
    require(abs(length - path["distance_m"]) < 1e-8, f"{q.name} polyline length")
    runner_up = None
    if scene == "cycle-limit":
        # A straight line through all lower or all upper openings gives the exact optimum
        # independently of the number and order of graph branches that were explored.
        expected = math.dist(q.a, q.b)
        require(len(doors) == 13, "budgeted route skipped a dividing wall")
    elif scene == "deep":
        expected = math.dist(q.a, q.b)
        require(
            len(doors) == 191 and path["routes_evaluated"] == 1,
            "unique long corridor was truncated",
        )
    elif scene == "staggered-height":
        # Each coordinate distance of all three segments is individually minimized
        # by these two extreme corners in the disjoint horizontal and vertical intervals.
        middle = [(10, 3, 0), (20, 1, 2)]
        if q.a[0] > q.b[0]:
            middle.reverse()
        expected = sum(
            math.dist(a, b) for a, b in zip([q.a, *middle], [*middle, q.b], strict=True)
        )
        require(
            all(
                math.dist(p, c) < 1e-7
                for p, c in zip(points[1:-1], middle, strict=True)
            ),
            f"{scene}/{q.name} doubly staggered corners",
        )
    elif q.name in ("upper-corner", "sloping-edge"):
        requires = by_id["door-1"]
        if scene == "diamond":
            yz = polygon_nearest(q.a[1:], DIAMOND)
        else:
            yz = (2 + 1.5 * math.sqrt(1 - requires.closure), requires.bounds()[1])
        crossing = (10.0, *yz)
        expected = math.dist(q.a, crossing) + math.dist(q.b, crossing)
        require(
            doors == ("door-1",) and math.dist(crossing, points[1]) < 1e-7,
            f"{scene}/{q.name} edge or corner",
        )
    elif not doors and q.room_a == q.room_b:
        expected = math.dist(q.a, q.b)
    else:
        require(
            q.a[1] == q.b[1] == 2,
            "planar oracle requires ear height at opening midpoint",
        )
        routes = candidates(
            layout, q.room_a, q.room_b, (q.a[0], q.a[2]), (q.b[0], q.b[2])
        )
        expected = routes[0][0]
        require(
            any(
                rs == rooms and ds == doors and abs(v - expected) < METRE_TOLERANCE
                for v, rs, ds in routes
            ),
            f"{scene}/{q.name}: {doors} differs from oracle {routes}",
        )
        if len(routes) > 1:
            runner_up = routes[1][0]
    if scene != "cycle-limit":
        require(
            path["search_complete"], f"{scene}/{q.name} exceeded its exploration budget"
        )
    require(
        (
            abs(length - expected) < METRE_TOLERANCE
            if path["search_complete"]
            else length >= expected - 1e-8
        )
        and path["lower_bound_m"] <= expected + 1e-8
        and expected - path["lower_bound_m"] < METRE_TOLERANCE,
        f"{scene}/{q.name} lower={path['lower_bound_m']}, length={length}, independent={expected}",
    )
    result = {
        "oracle_m": expected,
        "length_m": length,
        "lower_m": path["lower_bound_m"],
        "portals": list(doors),
        "crossings_m_local": [list(p) for p in points[1:-1]],
    }
    if runner_up is not None:
        result["second_route_m"] = runner_up
    return result


def qualified_query(scene, layout, q, path, offset):
    require(
        path["status"] == q.status,
        f"{scene}/{q.name}: status {path['status']} expected {q.status}",
    )
    result = {
        "status": path["status"],
        "search_complete": path["search_complete"],
        **{
            key: path[key]
            for key in (
                "room_bounds_tested",
                "room_candidates_tested",
                "rooms_visited",
                "routes_evaluated",
            )
        },
    }
    if q.status == "connected":
        result.update(connected(scene, layout, q, path, offset))
    else:
        require(
            path["search_complete"]
            and path["distance_m"] is None
            and path["lower_bound_m"] is None
            and not any(path[k] for k in ("rooms", "portals", "points_m")),
            f"{q.name} fabricated a path",
        )
    return result


def rays(binary, path, pairs):
    cmd = [binary, "--scene", path, "--inspect-object"]
    for a, b in pairs:
        cmd.extend(["--ray", ",".join(map(str, a)), ",".join(map(str, b))])
    res = subprocess.run(cmd, check=False, capture_output=True)
    require(res.returncode == 0, f"{path.name}: {res.stderr.decode()[-1800:]}")
    doc = json.loads(res.stdout)
    nodes = {n["channels"]["id"]: n for n in doc["nodes"]}
    require(doc["roots"] == ["quartier"], f"{path.name} district root")
    for child in nodes["quartier"]["children"]:
        building = nodes[child]
        require(
            building["parent"] == "quartier" and child.endswith(".interieur"),
            f"{path.name} building",
        )
        require(
            all(
                nodes[leaf]["parent"] == child and not nodes[leaf]["children"]
                for leaf in building["children"]
            ),
            f"{path.name} interior ownership",
        )
    rows = doc.get("rays", [doc["ray"]])
    require(
        len(rows) == len(pairs) and rows[-1] == doc["ray"],
        f"{path.name} batch rays reordered",
    )
    require(
        all(
            isinstance(row["roomPath"], dict)
            and all(
                isinstance(row[key], list)
                for key in ("visual", "collision", "acoustic")
            )
            for row in rows
        ),
        f"{path.name} channel or room path missing",
    )
    return [row["roomPath"] for row in rows]


INVALID = {
    "nonplanar": "nonplanar aperture",
    "wrong-neighbour": "wall of both rooms",
    "missing-neighbour": "unknown second room",
    "self-neighbour": "invalid acoustic portal",
    "bad-closure": "invalid acoustic portal",
    "same-side": "same side",
    "bad-bounds": "room bounds",
    "outside-wall": "wall of both rooms",
    "unordered": "convex ordered quadrilateral",
}


def verify(binary, out, layouts, hashes):
    count = layouts["district"][1]
    with binary.open("rb") as exe:
        binary_sha = hashlib.file_digest(exe, "sha256").hexdigest()
    receipt = {
        "binary_sha256": binary_sha,
        "scene_sha256": hashes,
        "buildings": count,
        "distance_tolerance_m": METRE_TOLERANCE,
        "scenes": {},
        "rejections": {},
    }
    tests = checks(count)
    packed_references = {}
    for scene, queries in tests.items():
        layout = layouts.get(scene, layouts["chain"])[0]
        cases = []
        pairs = []
        for q in queries:
            offset = building_offset(q.building)
            a = world_point(q.a, offset, scene == "rotated")
            b = world_point(q.b, offset, scene == "rotated")
            pairs.extend([(a, b), (b, a)])
            cases.append((q, offset))
        results = rays(binary, out / (scene + ".glb"), pairs)
        if scene in ("chain", "rotated"):
            packed_references[scene] = (pairs, results)
        records = {}
        for (q, offset), (forward, back) in zip(
            cases, zip(results[::2], results[1::2], strict=True), strict=True
        ):
            record = qualified_query(scene, layout, q, forward, offset)
            rev = replace(q, a=q.b, b=q.a, room_a=q.room_b, room_b=q.room_a)
            qualified_query(scene, layout, rev, back, offset)
            require(
                forward["portals"] == back["portals"][::-1]
                and forward["rooms"] == back["rooms"][::-1]
                and all(
                    math.dist(a, b) < 1e-7
                    for a, b in zip(
                        forward["points_m"], reversed(back["points_m"]), strict=True
                    )
                ),
                f"{scene}/{q.name} reversal changed the physical path",
            )
            records[q.name] = record
        receipt["scenes"][scene] = records
    for scene, match in INVALID.items():
        cmd = [
            binary,
            "--scene",
            out / (scene + ".glb"),
            "--inspect-object",
            "--ray",
            "2,2,0",
            "18,2,0",
        ]
        res = subprocess.run(cmd, check=False, capture_output=True, text=True)
        require(
            res.returncode and match in res.stderr.lower(),
            f"{scene}: expected '{match}' in {res.stderr[-800:]}",
        )
        receipt["rejections"][scene] = match
    ordinary = receipt["scenes"]["chain"]["four"]
    for key in ("four", "last-building"):
        sample = receipt["scenes"]["district"][key]
        require(
            abs(ordinary["length_m"] - sample["length_m"]) < 1e-8
            and ordinary["rooms_visited"] == sample["rooms_visited"]
            and ordinary["routes_evaluated"] == sample["routes_evaluated"]
            and sample["room_candidates_tested"]
            == ordinary["room_candidates_tested"]
            == 2,
            f"independent buildings changed local routing or containment, {key}",
        )
        # Tests include leaves visited and rejected. A balanced partition needs logarithmic work per endpoint.
        require(
            sample["room_bounds_tested"] < 12 * (1 + math.ceil(math.log2(6 * count))),
            "room lookup scanned distant buildings",
        )
    ranked_case = receipt["scenes"]["ranked"]["four-beat-two"]
    require(
        len(ranked_case["portals"]) == 4
        and ranked_case["routes_evaluated"] > 1
        and ranked_case["second_route_m"] > ranked_case["length_m"],
        "shorter alternative route went unranked",
    )
    cycles = receipt["scenes"]["cycle-limit"].values()
    require(
        any(not c["search_complete"] for c in cycles),
        "stress graph did not exercise the exploration budget",
    )
    receipt["query_count"] = 2 * sum(len(v) for v in tests.values())
    receipt["max_length_error_m"] = max(
        abs(c["length_m"] - c["oracle_m"])
        for group in receipt["scenes"].values()
        for c in group.values()
        if "oracle_m" in c and c["search_complete"]
    )
    receipt["max_budgeted_gap_m"] = max(c["length_m"] - c["oracle_m"] for c in cycles)
    (out / "receipt.json").write_text(
        json.dumps(receipt, indent=2, allow_nan=False) + "\n"
    )
    print(
        f"{receipt['query_count']} ray queries, {len(INVALID)} invalid inputs; worst distance error "
        f"{receipt['max_length_error_m']:.3g} m; {out / 'receipt.json'}"
    )
    verify_paging(binary, binary_sha, out, packed_references)


def verify_paging(binary, binary_sha, out, references):
    cook = runpy.run_path(str(Path(__file__).with_name("cook-pages.py")))["cook"]
    root = out / "paging"
    root.mkdir(exist_ok=True)
    route = root / "no-floor.json"
    route.write_text(
        json.dumps(
            {
                "version": 1,
                "tick_hz": 60,
                "gravity_m_s2": 9.81,
                "terminal_m_s": 55,
                "viewport": [640, 480],
                "vertical_fov_deg": 45,
                "routes": [
                    {
                        "id": "drop",
                        "ticks": 49,
                        "start_m": [16, 3, 10],
                        "velocity_m_s": [0, 0, 0],
                        "expected_support": "none",
                        "camera": [
                            {
                                "tick": t,
                                "eye": [16, 1.78, 10.06],
                                "aim": [16, 1.78, 9.06],
                            }
                            for t in (0, 49)
                        ],
                    }
                ],
            }
        )
    )
    receipt = {"binary_sha256": binary_sha, "scenes": {}, "rejections": {}}
    for scene, (pairs, paths) in references.items():
        pages = root / scene
        cook(out / (scene + ".glb"), pages, 128)
        index = json.loads((pages / "index.json").read_bytes())
        profile = index["object_profile"]["nodes"]
        require(
            sum("acousticRoom" in n for n in profile) == 6
            and sum("acousticPortal" in n for n in profile) == 5,
            f"{scene}: authored nodes lost in paged district",
        )
        output = root / (scene + ".json")
        base = [binary, "--paging", "--replay", route, "--route", "drop", "--tick", "1"]
        rays_args = [
            s
            for a, b in pairs
            for s in ("--acoustic-ray", ",".join(map(str, a)), ",".join(map(str, b)))
        ]
        res = subprocess.run(
            [*base, "--pages", pages, "--out", output, *rays_args],
            capture_output=True,
            check=False,
        )
        require(res.returncode == 0, f"{scene}: {res.stderr.decode()[-1000:]}")
        trace = json.loads(output.read_bytes())
        rows = trace["acoustic_probes"]
        require(
            len(rows) == len(paths)
            and trace["acoustic_probe"] == rows[-1]
            and [row["roomPath"] for row in rows] == paths,
            f"{scene}: paged room paths differ from resident",
        )
        io = trace["acoustic_io"]
        require(
            io["mesh_reads"] == io["read_bytes"] == 0
            and all(row["mesh_reads"] == row["read_bytes"] == 0 for row in rows),
            f"{scene}: room traversal fetched a surface mesh",
        )
        receipt["scenes"][scene] = {
            "queries": len(paths),
            "identical_room_paths": len(paths),
            "room_paths_sha256": hashlib.sha256(
                json.dumps(paths, sort_keys=True, separators=(",", ":")).encode()
            ).hexdigest(),
            "index_sha256": hashlib.sha256(
                (pages / "index.json").read_bytes()
            ).hexdigest(),
            "format": trace["storage"]["format"],
            "index_bytes": trace["storage"]["index_bytes"],
            "cold_file_bytes": trace["storage"]["cold_file_bytes"],
            "acoustic_mesh_reads": io["mesh_reads"],
            "acoustic_mesh_bytes": io["read_bytes"],
        }
        for defect, expected in [
            ("missing-pose", "missing indoor node placement"),
            ("unknown-room", "unknown second room"),
        ]:
            changed = copy.deepcopy(index)
            gate = next(
                n for n in changed["object_profile"]["nodes"] if "acousticPortal" in n
            )
            if defect == "missing-pose":
                del gate["indoor_pose"]
            else:
                gate["acousticPortal"]["rooms"][1] = "b0000.absent"
            directory = root / (scene + "-" + defect)
            shutil.copytree(pages, directory, dirs_exist_ok=True)
            (directory / "index.json").write_text(
                json.dumps(changed, separators=(",", ":"))
            )
            rejected = subprocess.run(
                [
                    *base,
                    "--pages",
                    directory,
                    "--out",
                    root / (directory.name + ".json"),
                    *rays_args[:3],
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            require(
                rejected.returncode != 0 and expected in rejected.stderr.lower(),
                f"{scene}/{defect}: {rejected.stderr[-1000:]}",
            )
            receipt["rejections"][directory.name] = expected
    (out / "paging-receipt.json").write_text(
        json.dumps(receipt, indent=2, allow_nan=False) + "\n"
    )
    print(
        f"{sum(row['queries'] for row in receipt['scenes'].values())} paged room paths identical to the GLB, "
        f"zero acoustic mesh bytes, {len(receipt['rejections'])} damaged indexes rejected; {out / 'paging-receipt.json'}"
    )


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--buildings", type=int, default=1024)
    ap.add_argument("--generate-only", action="store_true")
    ap.add_argument("--bin", type=Path)
    args = ap.parse_args()
    if args.buildings < 1 or (not args.generate_only and args.bin is None):
        ap.error("give a positive building count and either --bin or --generate-only")
    layouts, hashes = generate(args.out.resolve(), args.buildings)
    if args.generate_only:
        print(
            f"{len(hashes)} original GLBs generated, largest district {args.buildings} buildings"
        )
        return
    verify(args.bin.resolve(), args.out.resolve(), layouts, hashes)


if __name__ == "__main__":
    main()
