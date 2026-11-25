# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""OVC2 transfers a lossless GLB as glTF text columns and binary view dictionaries."""

import json
import struct
import zlib

from paging_coarse_columns import (
    LIMIT,
    Input,
    SizeLimit,
    blob,
    json_bytes,
    metadata,
    numbers,
    read_metadata,
    read_numbers,
    smallest,
    u32,
)

HEADER = struct.Struct("<4sIII")
GLB_HEAD = struct.Struct("<4sIIII")
POINT = struct.Struct("<4s4s4s")


def sequence_dictionary(views):
    unique = list(dict.fromkeys(views))
    ids = {data: i for i, data in enumerate(unique)}
    return (
        blob(numbers([ids[v] for v in views]))
        + u32(len(unique))
        + b"".join(blob(v) for v in unique)
    )


def point_views(views):
    # A clipped triangulated roof repeats its local corners in a common order.
    # Keep that order once and store every coordinate as its exact four source bytes.
    patterns = {}
    pattern_ids = []
    coords = [[], [], []]
    for view in views:
        ordered = {}
        pattern = []
        for point in POINT.iter_unpack(view):
            pattern.append(ordered.setdefault(point, len(ordered)))
        pattern_ids.append(patterns.setdefault(tuple(pattern), len(patterns)))
        for point in ordered:
            for col, value in zip(coords, point, strict=True):
                col.append(value)
    out = bytearray(u32(len(patterns)))
    for pattern in patterns:
        out.extend(blob(numbers(pattern)))
    out.extend(blob(numbers(pattern_ids)))
    for axis in coords:
        table = dict.fromkeys(axis)
        indices = {v: i for i, v in enumerate(table)}
        out.extend(blob(b"".join(table)))
        out.extend(blob(numbers([indices[v] for v in axis])))
    return bytes(out)


def grouped_binary(doc, binary):
    if len(doc.get("buffers", [])) != 1:
        return None
    names = {}
    for mesh in doc.get("meshes", []):
        for primitive in mesh["primitives"]:
            refs = dict(primitive["attributes"])
            if "indices" in primitive:
                refs["indices"] = primitive["indices"]
            for name, idx in refs.items():
                a = doc["accessors"][idx]
                if "bufferView" not in a:
                    continue
                vi = a["bufferView"]
                key = (name, a["componentType"], a["type"])
                if names.setdefault(vi, key) != key:
                    names[vi] = None
    streams = {}
    sizes, groups = [], []

    def add(key, data):
        if data:
            i, buffers = streams.setdefault(key, (len(streams), []))
            sizes.append(len(data))
            groups.append(i)
            buffers.append(data)

    end = 0
    for i, view in enumerate(doc.get("bufferViews", [])):
        start = view.get("byteOffset", 0)
        count = view["byteLength"]
        if view.get("buffer", 0) != 0 or start < end or start + count > len(binary):
            return None
        add(None, binary[end:start])
        end = start + count
        key = names.get(i) if "byteStride" not in view else None
        add(key, binary[start:end])
    add(None, binary[end:])
    out = bytearray(blob(numbers(groups)) + blob(numbers(sizes)) + u32(len(streams)))
    for key, (_, views) in streams.items():
        direct = b"\0" + b"".join(views)
        alternatives = [direct, b"\1" + sequence_dictionary(views)]
        if key == ("POSITION", 5126, "VEC3") and all(
            len(v) % POINT.size == 0 for v in views
        ):
            alternatives.append(b"\2" + point_views(views))
        out.extend(blob(smallest(alternatives)))
    return bytes(out)


def read_points(r, sizes):
    count = sum(sizes) // POINT.size
    if any(size % POINT.size for size in sizes):
        raise ValueError("coarse position span is not an xyz triple")
    patterns = []
    for _ in range(r.count(len(sizes))):
        pattern = read_numbers(r.blob(), count)
        if not pattern or set(pattern) != set(range(len(set(pattern)))):
            raise ValueError("invalid coarse local corner dictionary")
        patterns.append(pattern)
    ids = read_numbers(r.blob(), len(sizes))
    if len(ids) != len(sizes) or any(not 0 <= i < len(patterns) for i in ids):
        raise ValueError("invalid coarse position templates")
    ordered = [patterns[i] for i in ids]
    if any(len(p) * POINT.size != size for p, size in zip(ordered, sizes, strict=True)):
        raise ValueError("coarse position count mismatch")
    distinct = [max(p) + 1 for p in ordered]
    required = sum(distinct)
    axes = []
    for _ in range(3):
        raw = r.blob()
        if len(raw) % 4:
            raise ValueError("invalid coarse coordinate table")
        dictionary = [raw[k : k + 4] for k in range(0, len(raw), 4)]
        codes = read_numbers(r.blob(), required)
        if len(codes) != required or any(not 0 <= i < len(dictionary) for i in codes):
            raise ValueError("invalid coarse coordinate reference")
        axes.append([dictionary[i] for i in codes])
    points = iter(map(b"".join, zip(*axes, strict=True)))
    result = []
    for n, pattern in zip(distinct, ordered, strict=True):
        local = [next(points) for _ in range(n)]
        result.append(b"".join(local[i] for i in pattern))
    return result


def read_binary(data, length):
    r = Input(data)
    groups = read_numbers(r.blob(), length)
    sizes = read_numbers(r.blob(), length)
    n = r.count(len(groups))
    if len(groups) != len(sizes) or sum(sizes) != length or any(s <= 0 for s in sizes):
        raise ValueError("invalid coarse binary ranges")
    by_group = [[] for _ in range(n)]
    for g, size in zip(groups, sizes, strict=True):
        if not 0 <= g < n:
            raise ValueError("invalid coarse binary group")
        by_group[g].append(size)
    values = []
    for group in by_group:
        content = Input(r.blob())
        mode = content.byte()
        if mode == 0:
            views = [content.take(size) for size in group]
        elif mode == 1:
            keys = read_numbers(content.blob(), len(group))
            table = [content.blob() for _ in range(content.count(len(group)))]
            if len(keys) != len(group) or any(not 0 <= i < len(table) for i in keys):
                raise ValueError("invalid coarse view reference")
            views = [table[i] for i in keys]
        elif mode == 2:
            views = read_points(content, group)
        else:
            raise ValueError("unknown coarse binary mode")
        content.end()
        if [len(v) for v in views] != group:
            raise ValueError("coarse binary view length mismatch")
        values.append(iter(views))
    r.end()
    return b"".join(next(values[g]) for g in groups)


def candidate(raw):
    if not 28 <= len(raw) <= LIMIT or raw[:4] != b"glTF":
        return None
    _, version, size, hlen, kind = GLB_HEAD.unpack_from(raw)
    if (
        version != 2
        or size != len(raw)
        or kind != 0x4E4F534A
        or hlen % 4
        or 28 + hlen > size
    ):
        return None
    blen, binary_tag = struct.unpack_from("<I4s", raw, 20 + hlen)
    if binary_tag != b"BIN\0" or 28 + hlen + blen != size:
        return None
    head, binary = raw[20 : 20 + hlen].rstrip(b" "), raw[28 + hlen :]
    if len(head) + (-len(head) % 4) != hlen:
        return None
    doc = json.loads(head)
    meta = b"\0" + head
    if json_bytes(doc) == head:
        meta = smallest([meta, b"\1" + metadata(doc)])
    geo = b"\0" + binary
    streams = grouped_binary(doc, binary)
    if streams is not None:
        geo = smallest([geo, b"\1" + streams])
    payload = blob(meta) + blob(geo)
    if len(payload) > LIMIT:
        return None
    return HEADER.pack(b"OVC2", len(payload), len(head), len(binary)) + zlib.compress(
        payload, 9
    )


def pack_coarse(raw, mode="auto"):
    original = b"OVC1" + u32(len(raw)) + zlib.compress(raw, 6)
    if mode == "ovc1":
        return original
    if mode != "auto":
        raise ValueError("unknown coarse storage")
    try:
        packed = candidate(raw)
    except SizeLimit:
        return original
    return packed if packed is not None and len(packed) < len(original) else original


def unpack_coarse(wire):
    if wire[:4] == b"OVC1":
        size = struct.unpack_from("<I", wire, 4)[0]
        plain = zlib.decompress(wire[8:])
        if len(plain) != size:
            raise ValueError("historical coarse length mismatch")
        return plain
    if len(wire) < HEADER.size:
        raise ValueError("invalid coarse header")
    magic, size, hlen, blen = HEADER.unpack_from(wire)
    if magic not in (b"OVC2", b"OVCU") or size > LIMIT or 28 + hlen + (-hlen % 4) + blen > LIMIT:
        raise ValueError("invalid coarse size or version")
    if magic == b"OVCU":
        raw = wire[HEADER.size:]
        if len(raw) != size:
            raise ValueError("invalid uncompressed coarse column length")
    else:
        decoder = zlib.decompressobj()
        raw = decoder.decompress(wire[HEADER.size :], size + 1)
        if len(raw) != size or not decoder.eof or decoder.unused_data:
            raise ValueError("invalid compressed coarse body")
    r = Input(raw)
    meta, geo = Input(r.blob()), Input(r.blob())
    r.end()
    m, g = meta.byte(), geo.byte()
    if m > 1 or g > 1:
        raise ValueError("invalid coarse channel encoding")
    h = meta.take(meta.remaining())
    b = geo.take(geo.remaining())
    text = read_metadata(h, hlen) if m else h
    binary = read_binary(b, blen) if g else b
    if len(text) != hlen or len(binary) != blen:
        raise ValueError("reconstructed GLB differs from declared length")
    text += b" " * (-len(text) % 4)
    return (
        GLB_HEAD.pack(b"glTF", 2, 28 + len(text) + blen, len(text), 0x4E4F534A)
        + text
        + struct.pack("<I4s", blen, b"BIN\0")
        + binary
    )
