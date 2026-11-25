# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Sparse triangle ordinals for physical fragments and partial support meshes."""

from collections import Counter

U32_MAX = (1 << 32) - 1


def unsigned(value):
    return type(value) is int and 0 <= value <= U32_MAX


def append_face(runs, index):
    """Append one strictly increasing source ordinal, joining adjacent triangles."""
    if not unsigned(index):
        raise ValueError("invalid face ordinal")
    if runs:
        end = sum(runs[-1])
        if index < end:
            raise ValueError("face ordinals must increase")
        if index == end:
            runs[-1][1] += 1
            return
    runs.append([index, 1])


def face_runs(ordinals):
    runs = []
    for ordinal in ordinals:
        append_face(runs, ordinal)
    return runs


def support_order(partials):
    """Store a single common start for contiguous subsets, with exceptions keyed by page and slot."""
    frequencies = Counter(runs[0][0] for _, _, runs in partials if len(runs) == 1)
    start = min(frequencies, key=lambda i: (-frequencies[i], i)) if frequencies else 0
    exceptions = [row for row in partials if len(row[2]) != 1 or row[2][0][0] != start]
    return {"start": start, **({"exceptions": exceptions} if exceptions else {})}


def checked_count(runs, limit):
    total = 0
    end = -1
    if not runs:
        raise ValueError("empty face runs")
    for begin, length in runs:
        if (
            not unsigned(begin)
            or not unsigned(length)
            or length == 0
            or begin <= end
            or begin + length > limit
        ):
            raise ValueError("invalid or nonmaximal face runs")
        total += length
        end = begin + length
    return total


def local_counts(layer, certified, residual, slots):
    counts = [len(faces) for _, _, faces in certified]
    if len(counts) != len(layer["ids"]) or len(counts) != len(layer["source_nodes"]):
        raise ValueError("face order owner count mismatch")
    for (_, _, triangles), slot in zip(residual, slots, strict=True):
        if (
            not unsigned(slot)
            or slot >= len(counts)
            or not unsigned(triangles)
            or triangles == 0
        ):
            raise ValueError("invalid residual shape")
        counts[slot] += triangles
    if sum(counts) != layer["triangles"] or not all(
        0 < count <= U32_MAX for count in counts
    ):
        raise ValueError("face order differs from directory triangle count")
    return counts


def checked_slots(layer, counts):
    by_slot = {}
    previous = -1
    for slot, total, runs in layer.get("source_face_runs", []):
        if (
            not unsigned(slot)
            or not previous < slot < len(counts)
            or not unsigned(total)
        ):
            raise ValueError("invalid fragment slot or source count")
        if counts[slot] >= total or checked_count(runs, total) != counts[slot]:
            raise ValueError("fragment length differs from directory")
        by_slot[slot] = (total, runs)
        previous = slot
    return by_slot


def validate_face_order(index, support):
    """Validate both mappings and the global partition. An old index supplies no source identity."""
    if "collision_face_order" not in index:
        return False
    if (
        type(index["collision_face_order"]) is not int
        or index["collision_face_order"] != 1
    ):
        raise ValueError("unknown collision face order")
    layout = support["face_order"]
    start = layout["start"]
    if not unsigned(start):
        raise ValueError("invalid default support face ordinal")
    exceptions = {}
    previous = (-1, -1)
    for page, slot, runs in layout.get("exceptions", []):
        key = (page, slot)
        if not unsigned(page) or not unsigned(slot) or key <= previous:
            raise ValueError("invalid support exception slot")
        exceptions[key] = runs
        previous = key
    owners = {}
    layers = (t["physics"] for t in index["tiles"] if "physics" in t)
    groups = zip(
        layers,
        support["pages"],
        support["residual"],
        support["residual_shapes"],
        strict=True,
    )
    for page_index, (layer, proxies, residual, residual_slots) in enumerate(groups):
        counts = local_counts(layer, proxies, residual, residual_slots)
        fragments = checked_slots(layer, counts)
        for slot, count in enumerate(counts):
            owner = (layer["source_nodes"][slot], layer["ids"][slot])
            total, runs = fragments.get(slot, (count, [[0, count]]))
            expected, combined = owners.setdefault(owner, (total, []))
            if expected != total:
                raise ValueError("inconsistent source triangle count")
            combined.extend(runs)
            ncert = len(proxies[slot][2])
            key = (page_index, slot)
            if key in exceptions:
                selected = exceptions.pop(key)
                if not 0 < ncert < count or checked_count(selected, count) != ncert:
                    raise ValueError("invalid support exception length")
                if len(selected) == 1 and selected[0][0] == start:
                    raise ValueError("redundant support exception")
            elif 0 < ncert < count and start + ncert > count:
                raise ValueError("default support ordinals exceed the full mesh")
    if exceptions:
        raise ValueError("support exception names a missing page or slot")
    for total, combined in owners.values():
        next_face = 0
        for begin, length in sorted(combined):
            if begin != next_face:
                raise ValueError("source triangle partition overlaps or has a gap")
            next_face += length
        if next_face != total:
            raise ValueError("incomplete source triangle partition")
    return True
