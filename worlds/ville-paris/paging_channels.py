# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Static object-profile channels derived from the original node tree."""

from collections import defaultdict
from math import isfinite, prod
from struct import Struct

from paging_affine import apply
from paging_face_order import face_runs


def reverses_winding(pose):
    """Orientation of the effective world transform, independent of its translation."""
    if "matrix" in pose:
        m = pose["matrix"]
        a, b, c = (m[start : start + 3] for start in (0, 4, 8))
        determinant = (
            a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0])
        )
    else:
        determinant = prod(pose.get("scale", (1, 1, 1)))
    if not isfinite(determinant) or determinant == 0:
        raise ValueError("collision placement needs a finite, invertible transform")
    return determinant < 0


def high_channels(doc):
    """Same high-level subtree cut as the portable object profile."""
    channels = {"visual", "collision", "acoustic"}
    result = {}
    pending = [(n, channels) for n in doc["scenes"][doc["scene"]]["nodes"]]
    while pending:
        owner, allowed = pending.pop()
        node = doc["nodes"][owner]
        cuts = node.get("extras", {}).get("osage", {}).get("subtreeProxy") or {}
        result[owner] = {k for k in allowed if cuts.get(k) in (None, "high")}
        children = {k for k in allowed if cuts.get(k) != "high"}
        pending.extend((child, children) for child in node.get("children", []))
    return result


def geometry(parts):
    vertices, triangles = [], []
    for part in parts:
        offset = len(vertices)
        vertices.extend(part.positions)
        triangles.extend(
            [i + offset for i in face] for face in zip(*[iter(part.indices)] * 3)
        )
    return vertices, triangles


def collision_shape(owner, sid, world, faces, reflected):
    return {
        "id": sid,
        "source_node": owner,
        "vertices": world,
        "triangles": [[a, c, b] for a, b, c in faces] if reflected else faces,
    }


def collision_pages(shape, cell, bounds):
    """Keep each source face intact. Queries use the actual extent of the resulting page."""
    vertices, triangles = shape["vertices"], shape["triangles"]
    low, high = bounds(vertices)
    center = tuple(cell((low[a] + high[a]) * 0.5) for a in (0, 2))
    # A compact collider fits in one small read even when its area spans many visual cells.
    if len(triangles) <= 64 or all(cell(low[a]) == cell(high[a]) for a in (0, 2)):
        yield center, shape, None
        return
    groups = defaultdict(list)
    for ordinal, face in enumerate(triangles):
        key = tuple(cell(sum(vertices[i][a] for i in face) / 3) for a in (0, 2))
        groups[key].append(ordinal)
    if len(groups) == 1:
        yield next(iter(groups)), shape, None
        return
    for key, ordinals in groups.items():
        faces = [triangles[i] for i in ordinals]
        # Preserve source vertex order, material side and the relative order of faces.
        used = sorted({i for face in faces for i in face})
        remap = {i: j for j, i in enumerate(used)}
        yield key, {
            **shape,
            "vertices": [vertices[i] for i in used],
            "triangles": [[remap[i] for i in face] for face in faces],
        }, face_runs(ordinals)


def repeated_corner_meshes(tiles, bounds):
    """Share the exact vertex and face order of small solids made from eight box corners."""
    groups = defaultdict(list)
    point_key = Struct("<3f").pack
    for key, shapes in sorted(tiles.items()):
        for slot, (shape, _) in enumerate(shapes):
            points, faces = shape["vertices"], shape["triangles"]
            if len(faces) != 12 or not 8 <= len(points) <= 36:
                continue
            lo, hi = bounds(points)
            if any(a >= b for a, b in zip(lo, hi)):
                continue
            corners = {point_key(*(hi[a] if mask & 1 << a else lo[a] for a in range(3))): mask for mask in range(8)}
            keys = [point_key(*p) for p in points]
            if set(keys) != corners.keys():
                continue
            pattern = (tuple(corners[k] for k in keys), tuple(map(tuple, faces)))
            groups[pattern].append([key, slot, [lo, hi]])
    patterns, placements = [], []
    for pattern, rows in groups.items():
        if len(rows) >= 2 and closed_box_pattern(*pattern):
            index = len(patterns)
            patterns.append(pattern)
            placements.extend([key, slot, index, box] for key, slot, box in rows)
    return {"patterns": patterns, "placements": placements} if patterns else None


def closed_box_pattern(corners, faces):
    """Every side is an outward-facing pair sharing exactly one face diagonal."""
    sides = defaultdict(list)
    for face in faces:
        a, b, c = [corners[i] for i in face]
        axes = (a ^ b) | (a ^ c)
        if len({a, b, c}) != 3 or axes.bit_count() != 2:
            return False
        normal_axis = (7 ^ axes).bit_length() - 1
        u, v = (normal_axis + 1) % 3, (normal_axis + 2) % 3
        coord = lambda i, axis: (i >> axis) & 1
        cross = ((coord(b, u) - coord(a, u)) * (coord(c, v) - coord(a, v))
                 - (coord(b, v) - coord(a, v)) * (coord(c, u) - coord(a, u)))
        side = coord(a, normal_axis)
        if cross != 2 * side - 1:
            return False
        sides[normal_axis, side].append({a, b, c})
    if len(sides) != 6 or any(len(pair) != 2 for pair in sides.values()):
        return False
    for first, second in sides.values():
        diagonal = first & second
        if len(diagonal) != 2 or (min(diagonal) ^ max(diagonal)).bit_count() != 2:
            return False
    return True


def cook_channels(doc, meshes, poses, active, write, encode, bounds):
    header = doc["asset"].get("extras", {}).get("osageObject")
    physical = []
    if not header:
        return None, physical
    objects, acoustic_meshes, mesh_slots = [], [], {}
    solids, surfaces = set(), set()
    for owner, enabled in sorted(active.items()):
        node = doc["nodes"][owner]
        original = node.get("extras", {}).get("osage")
        if not original:
            raise ValueError("object profile node has no source identity")
        pose, matrix = poses[owner]
        reflected = (
            "collision" in enabled
            and bool(original.get("collision"))
            and reverses_winding(pose)
        )
        record = {"node": owner, "id": original["id"]}
        if "acousticRoom" in original or "acousticPortal" in original:
            record["indoor_pose"] = pose
            for channel in ("acousticRoom", "acousticPortal"):
                if channel in original:
                    record[channel] = original[channel]
        objects.append(record)
        for group in ("collision", "acoustic"):
            if group not in enabled:
                continue
            for shape in original.get(group, []):
                mi, material, sid = shape["mesh"], shape["material"], shape["id"]
                ids = solids if group == "collision" else surfaces
                if sid in ids or not 0 <= mi < len(meshes):
                    raise ValueError(f"invalid {group} surface {sid}")
                ids.add(sid)
                points, faces = geometry(meshes[mi])
                world = [apply(matrix, point) for point in points]
                box = bounds(world)
                if group == "collision":
                    entry = collision_shape(owner, sid, world, faces, reflected)
                    physical.append((entry, header["collisionMaterials"][material]))
                    record.setdefault(group, []).append(dict(shape))
                else:
                    if material not in header["acousticMaterials"] or (
                        shape["source"] == "collision-proxy"
                        and not any(
                            c["mesh"] == mi for c in original.get("collision", [])
                        )
                    ):
                        raise ValueError("unknown acoustic material or collision proxy")
                    if mi not in mesh_slots:
                        mesh_slots[mi] = len(acoustic_meshes)
                        acoustic_meshes.append(
                            write(
                                encode({"vertices": points, "triangles": faces}),
                                "acoustic",
                            )
                        )
                    record.setdefault(group, []).append(
                        {
                            **shape,
                            "resource": mesh_slots[mi],
                            "bounds": box,
                            "pose": pose,
                        }
                    )
    return {
        "id": header["id"],
        "nodes": objects,
        "acoustic_materials": header["acousticMaterials"],
        "acoustic_meshes": acoustic_meshes,
        "placed_bounds": True,
    }, physical
