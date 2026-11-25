# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Analytic capsule versus moving box measurements in the local box frame."""

import json
import itertools
import math


def read_poses(row):
    root = {
        "center": [float(row[c]) for c in ("cx", "cy", "cz")],
        "rotation_xyzw": [float(row[c]) for c in ("qx", "qy", "qz", "qw")],
        "linear_velocity": [float(row[c]) for c in ("vx", "vy", "vz")],
        "angular_velocity": [float(row[c]) for c in ("wx", "wy", "wz")],
    }
    return [root, *json.loads(row["parts_json"])]


def inverse(q):
    return (-q[0], -q[1], -q[2], q[3])


def multiply(p, q):
    x, y, z, w = p
    a, b, c, d = q
    return (w*a + x*d + y*c - z*b, w*b - x*c + y*d + z*a,
            w*c + x*b - y*a + z*d, w*d - x*a - y*b - z*c)


def basis(quaternion):
    x, y, z, w = quaternion
    return (
        (1 - 2 * (y * y + z * z), 2 * (x * y + w * z), 2 * (x * z - w * y)),
        (2 * (x * y - w * z), 1 - 2 * (x * x + z * z), 2 * (y * z + w * x)),
        (2 * (x * z + w * y), 2 * (y * z - w * x), 1 - 2 * (x * x + y * y)),
    )


def dot(a, b):
    return sum(x * y for x, y in zip(a, b))


def difference(a, b):
    return tuple(x - y for x, y in zip(a, b, strict=True))


def closest_spine(center, half_segment, half_box):
    """Distance and parameter range of the segment points closest to the solid box."""
    cuts = {-1.0, 1.0}
    for c, d, h in zip(center, half_segment, half_box, strict=True):
        if d:
            for edge in (-h, h):
                t = (edge - c) / d
                if -1 < t < 1:
                    cuts.add(t)
    cuts = sorted(cuts)
    smallest = math.inf
    first = last = 0.0
    for lo, hi in itertools.pairwise(cuts):
        mid = (lo + hi) * 0.5
        curvature = slope = 0.0
        for c, d, h in zip(center, half_segment, half_box, strict=True):
            coord = c + d * mid
            if abs(coord) > h:
                curvature += d * d
                slope += d * (c - math.copysign(h, coord))
        t = min(hi, max(lo, -slope / curvature)) if curvature else mid
        residual = [
            max(abs(c + t * d) - h, 0.0)
            for c, d, h in zip(center, half_segment, half_box, strict=True)
        ]
        distance = math.hypot(*residual)
        start, end = (t, t) if curvature else (lo, hi)
        if distance < smallest:
            smallest, first, last = distance, start, end
        elif distance == smallest:
            first, last = min(first, start), max(last, end)
    return smallest, first, last


def separating_plane(center, half_segment, half_box):
    """Minimum separating depth and outward unit normal for an intersecting spine."""
    x, y, z = half_segment
    axes = [(1, 0, 0), (0, 1, 0), (0, 0, 1), (0, z, -y), (-z, 0, x), (y, -x, 0)]
    depth = math.inf
    normal = None
    for axis in axes:
        length = math.hypot(*axis)
        if length:
            support = dot(half_box, tuple(map(abs, axis))) + abs(
                dot(half_segment, axis)
            )
            projection = dot(center, axis)
            candidate = (support - abs(projection)) / length
            if candidate < depth:
                depth = candidate
                normal = tuple(
                    math.copysign(1.0, projection) * a / length for a in axis
                )
    return depth, normal


def segment_box_gap(center, half_segment, half_box, radius):
    """Signed surface distance outside, minimum separating translation inside."""
    distance, _, _ = closest_spine(center, half_segment, half_box)
    if distance:
        return distance - radius
    depth, _ = separating_plane(center, half_segment, half_box)
    return -(depth + radius)


def local_spine(part, pose, axes, box_center):
    local = [dot(difference(pose["center"], box_center), axis) for axis in axes]
    up = basis(pose["rotation_xyzw"])[1]
    arm = [part["half_segment_m"] * dot(up, axis) for axis in axes]
    return local, arm


def gap(part, pose, axes, box_center, half_box):
    local, arm = local_spine(part, pose, axes, box_center)
    return segment_box_gap(local, arm, half_box, part["radius_m"])


def world_vector(axes, vector):
    return tuple(dot(column, vector) for column in zip(*axes, strict=True))


def spin_speed(pose, point, normal):
    """Component of omega cross (point - centre) along the world normal."""
    x, y, z = difference(point, pose["center"])
    wx, wy, wz = pose["angular_velocity"]
    return dot((wy * z - wz * y, wz * x - wx * z, wx * y - wy * x), normal)


def normal_motion(pose, box, limb_point, box_point, normal):
    centres = dot(difference(pose["linear_velocity"], box["linear_velocity"]), normal)
    limb_spin = spin_speed(pose, limb_point, normal)
    box_spin = spin_speed(box, box_point, normal)
    return {
        "centres_m_s": centres,
        "limb_spin_m_s": limb_spin,
        "box_spin_m_s": box_spin,
        "relative_m_s": centres + limb_spin - box_spin,
    }


def surface_witnesses(part, pose, box, half_box):
    """Closest surface pair(s) and instantaneous normal material velocities in this pose.

    A parallel segment can have a line of closest points. Velocity along that line
    is affine, so its two ends bound all normal speeds. For an intersecting spine
    use a minimum-translation plane (first in axis order on an exact tie).
    """
    axes = basis(box["rotation_xyzw"])
    local, arm = local_spine(part, pose, axes, box["center"])
    radius = part["radius_m"]
    distance, first, last = closest_spine(local, arm, half_box)
    shifted = local

    def spine_point(center, t):
        return tuple(c + t * d for c, d in zip(center, arm, strict=True))

    def project(p):
        return tuple(max(-h, min(h, v)) for v, h in zip(p, half_box, strict=True))

    if distance:
        spine = spine_point(local, first)
        normal = tuple(v / distance for v in difference(spine, project(spine)))
        signed_gap = distance - radius
        kind = "closest"
    else:
        depth, normal = separating_plane(local, arm, half_box)
        signed_gap = -(depth + radius)
        # After this translation the capsule just touches the support plane.
        shifted = tuple(c - signed_gap * n for c, n in zip(local, normal, strict=True))
        _, first, last = closest_spine(shifted, arm, half_box)
        kind = "minimum_translation"
    n = world_vector(axes, normal)

    def world_point(p):
        return tuple(
            c + d for c, d in zip(box["center"], world_vector(axes, p), strict=True)
        )

    ends = (first, last) if first != last and any(arm) else (first,)
    pairs = []
    for t in ends:
        spine = spine_point(local, t)
        limb_point = world_point(
            tuple(v - radius * a for v, a in zip(spine, normal, strict=True))
        )
        box_point = world_point(project(spine_point(shifted, t)))
        pairs.append(
            {
                "limb_point_m": limb_point,
                "box_point_m": box_point,
                **normal_motion(pose, box, limb_point, box_point, n),
            }
        )
    return {"gap_m": signed_gap, "normal": n, "normal_kind": kind, "points": pairs}
