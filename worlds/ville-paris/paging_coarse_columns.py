# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Lossless byte columns shared by the OVC2 glTF text and geometry encoders."""

import json
import re
import struct
import zlib
from collections import Counter
from itertools import accumulate

U32 = struct.Struct("<I")
TOKEN = re.compile(
    rb'"(?:[^"\\]|\\.)*"|-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?'
)
INTEGER = re.compile(rb"(?:0|-?[1-9][0-9]*)\Z")
LIMIT = 256 << 20
RESULTS = 8 << 20


class SizeLimit(ValueError):
    """The general GLB envelope can carry a stream beyond the OVC2 per-column limit."""


def u32(n):
    return U32.pack(n)


def blob(data):
    return u32(len(data)) + data


def json_bytes(value):
    return json.dumps(value, separators=(",", ":"), allow_nan=False).encode()


def smallest(items):
    return min(items, key=lambda item: len(zlib.compress(item, 6)))


class Input:
    def __init__(self, data):
        self.data = memoryview(data)
        self.offset = 0

    def remaining(self):
        return len(self.data) - self.offset

    def take(self, n):
        if not 0 <= n <= self.remaining():
            raise ValueError("truncated coarse stream")
        start = self.offset
        self.offset += n
        return bytes(self.data[start : self.offset])

    def byte(self):
        return self.take(1)[0]

    def count(self, limit=LIMIT):
        count = U32.unpack(self.take(4))[0]
        if count > limit:
            raise ValueError("excessive coarse count")
        return count

    def blob(self):
        return self.take(self.count())

    def end(self):
        if self.remaining():
            raise ValueError("trailing coarse bytes")


def varints(values):
    data = bytearray()
    for value in values:
        while value >= 128:
            data.append((value & 127) | 128)
            value >>= 7
        data.append(value)
    return bytes(data)


def unsigned(r):
    value = 0
    for shift in range(0, 63, 7):
        b = r.byte()
        value |= (b & 127) << shift
        if b < 128:
            return value
    raise ValueError("oversize coarse integer")


def zigzag(i):
    return 2 * i if i >= 0 else -2 * i - 1


def signed(r):
    i = unsigned(r)
    return -(i // 2) - 1 if i % 2 else i // 2


def numbers(values):
    """Encode exact integers. A fixed stride costs two signed integers irrespective of count."""
    n = len(values)
    if n > RESULTS:
        raise SizeLimit("coarse integer stream exceeds item limit")
    count = u32(n)
    if not n:
        return b"\x00" + count
    if all(i == values[0] for i in values):
        return b"\x01" + count + varints([zigzag(values[0])])
    stride = values[1] - values[0]
    if all(i == values[0] + j * stride for j, i in enumerate(values)):
        return b"\x02" + count + varints([zigzag(values[0]), zigzag(stride)])
    deltas = (
        zigzag(i - previous)
        for i, previous in zip(values, [0, *values[:-1]], strict=True)
    )
    choices = [b"\x03" + count + varints(deltas)]
    if min(values) >= 0:
        width = max(1, (max(values).bit_length() + 7) // 8)
        raw = b"".join(i.to_bytes(width, "little") for i in values)
        choices.append(
            bytes([4, width]) + count + b"".join(raw[k::width] for k in range(width))
        )
        choices.append(b"\x05" + count + varints(values))
    return smallest(choices)


def read_numbers(data, limit):
    r = Input(data)
    mode = r.byte()
    width = r.byte() if mode == 4 else 0
    n = r.count(min(limit, RESULTS))
    if mode == 0 and not n:
        values = []
    elif mode == 1:
        values = [signed(r)] * n
    elif mode == 2:
        first, stride = signed(r), signed(r)
        values = [first + i * stride for i in range(n)]
    elif mode == 3:
        values = list(accumulate(signed(r) for _ in range(n)))
    elif mode == 4 and 1 <= width <= 8:
        planes = [r.take(n) for _ in range(width)]
        values = [sum(x[j] << (8 * i) for i, x in enumerate(planes)) for j in range(n)]
    elif mode == 5:
        values = [unsigned(r) for _ in range(n)]
    else:
        raise ValueError("unknown coarse integer mode")
    r.end()
    return values


def text_column(values):
    # JSON numbers retain their original decimal spelling. Only canonical small integers use arithmetic.
    if all(value == values[0] for value in values):
        return b"\0" + values[0]
    if all(len(v) <= 18 and INTEGER.fullmatch(v) for v in values):
        return b"\1" + numbers(list(map(int, values)))
    unique = list(dict.fromkeys(values))
    index = {v: i for i, v in enumerate(unique)}
    return smallest(
        [
            b"\2" + b"\0".join(values),
            b"\3" + blob(b"\0".join(unique)) + numbers([index[v] for v in values]),
        ]
    )


def read_column(data, count):
    r = Input(data)
    mode = r.byte()
    if mode == 0:
        out = [r.take(r.remaining())] * count
    elif mode == 1:
        out = [str(v).encode() for v in read_numbers(r.take(r.remaining()), count)]
    elif mode == 2:
        out = r.take(r.remaining()).split(b"\0")
    elif mode == 3:
        table = r.blob().split(b"\0")
        refs = read_numbers(r.take(r.remaining()), count)
        if len(table) > count or any(not 0 <= i < len(table) for i in refs):
            raise ValueError("invalid coarse text reference")
        out = [table[i] for i in refs]
    else:
        raise ValueError("unknown coarse token mode")
    if len(out) != count:
        raise ValueError("coarse column length mismatch")
    return out


def row_template(value):
    columns = []

    def token(match):
        raw = match.group()
        if raw.startswith(b'"'):
            return raw
        columns.append(raw)
        return b"\0"

    return TOKEN.sub(token, json_bytes(value)), columns


def rows(values):
    groups = {}
    order = []
    for row in values:
        template, fields = row_template(row)
        idx, records = groups.setdefault(template, (len(groups), []))
        order.append(idx)
        records.append(fields)
    out = bytearray(blob(numbers(order)) + u32(len(groups)))
    for template, (_, records) in groups.items():
        out.extend(blob(template))
        for fields in zip(*records, strict=True):
            out.extend(blob(text_column(fields)))
    return bytes(out)


def read_rows(data, limit):
    r = Input(data)
    order = read_numbers(r.blob(), limit)
    freq = Counter(order)
    groups = []
    budget = limit
    for idx in range(r.count(len(order))):
        parts = r.blob().split(b"\0")
        count = freq[idx]
        budget -= count * (len(parts) - 1)
        if not count or budget < 0:
            raise ValueError("invalid coarse template expansion")
        cols = [read_column(r.blob(), count) for _ in parts[1:]]
        groups.append((parts, cols))
    r.end()
    current = [0] * len(groups)
    out = bytearray(b"[")
    for row, key in enumerate(order):
        if not 0 <= key < len(groups):
            raise ValueError("invalid coarse template")
        parts, cols = groups[key]
        slot = current[key]
        current[key] += 1
        pieces = [b"," if row else b"", parts[0]]
        for col, literal in zip(cols, parts[1:], strict=True):
            pieces.extend([col[slot], literal])
        if len(out) + sum(map(len, pieces)) >= limit:
            raise ValueError("coarse JSON rows exceed bound")
        for piece in pieces:
            out.extend(piece)
    out.extend(b"]")
    return bytes(out)


def metadata(doc):
    out = bytearray(u32(len(doc)))
    for key, value in doc.items():
        out.extend(blob(json_bytes(key)))
        if key == "scenes" and value == [
            {"nodes": list(range(len(doc.get("nodes", []))))}
        ]:
            content = b"\2" + u32(len(doc.get("nodes", [])))
        elif key in ("nodes", "meshes", "accessors", "bufferViews") and value:
            content = smallest([b"\0" + json_bytes(value), b"\1" + rows(value)])
        else:
            content = b"\0" + json_bytes(value)
        out.extend(blob(content))
    return bytes(out)


def read_metadata(data, limit):
    r = Input(data)
    fields = []
    used = 1
    for _ in range(r.count(limit)):
        key, field = r.blob(), Input(r.blob())
        mode = field.byte()
        if mode == 0:
            value = field.take(field.remaining())
        elif mode == 1:
            value = read_rows(field.take(field.remaining()), limit)
        elif mode == 2:
            value = (
                b'[{"nodes":['
                + b",".join(str(i).encode() for i in range(field.count(limit)))
                + b"]}]"
            )
        else:
            raise ValueError("unknown coarse metadata mode")
        field.end()
        used += len(key) + 1 + len(value) + bool(fields)
        if used + 1 > limit:
            raise ValueError("coarse JSON object exceeds bound")
        fields.append(key + b":" + value)
    r.end()
    return b"{" + b",".join(fields) + b"}"
