# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""OCP1 preserves binary32 positions and the exact authored face order in every encoding."""

import copy
import hashlib
import json
import struct
import tempfile
import unittest
from pathlib import Path

from paging_collision_codec import (
    CORNERS,
    FACES,
    MAGIC,
    decode_collision,
    encode_collision,
    read_collision,
)


def indexed(vertices, triangles, sid="roof", owner=3):
    return {
        "id": sid,
        "source_node": owner,
        "vertices": vertices,
        "triangles": triangles,
    }


def fixture(kind, backwards=False):
    bottom = [
        (4.0, -0.0, -3.0),
        (4.0, -0.0, 8.0),
        (-2.0, -0.0, 8.0),
        (-2.0, -0.0, -3.0),
    ]
    top = [(-2.0, 3.0, -3.0), (-2.0, 3.0, 8.0), (4.0, 3.0, 8.0), (4.0, 3.0, -3.0)]
    if kind != 3:
        top = [(-1.0, 3.0, -2.0), (-1.0, 3.0, 7.0), (3.0, 3.0, 7.0), (3.0, 3.0, -2.0)]
    if kind == 1:
        top[1] = (0.125, 4.0, 6.0)
    vertices = [(bottom + top)[i] for i in CORNERS]
    if kind == 0:
        vertices.append((2**-149, 1.0, -0.0))
    faces = [(a, c, b) if backwards else (a, b, c) for a, b, c in FACES]
    return indexed(vertices, faces)


def directory(shapes, data, suffix):
    digest = hashlib.sha256(data).hexdigest()
    return {
        "file": digest + "." + suffix,
        "sha256": digest,
        "bytes": len(data),
        "ids": [s["id"] for s in shapes],
        "source_nodes": [s["source_node"] for s in shapes],
        "triangles": sum(len(s["triangles"]) for s in shapes),
    }


def point_bits(shape):
    return b"".join(struct.pack("<3f", *p) for p in shape["vertices"])


class CollisionCodec(unittest.TestCase):
    def roundtrip(self, shapes):
        wire = encode_collision(shapes)
        header = directory(shapes, wire, "cpage")
        decoded = decode_collision(wire, header)
        self.assertEqual(encode_collision(decoded), wire)
        for a, b in zip(shapes, decoded, strict=True):
            self.assertEqual((a["id"], a["source_node"]), (b["id"], b["source_node"]))
            self.assertEqual(
                list(map(tuple, a["triangles"])), list(map(tuple, b["triangles"]))
            )
            self.assertEqual(point_bits(a), point_bits(b))
        return wire

    def test_forward_and_reflected_compact_shapes(self):
        for kind, length in ((1, 97), (2, 41), (3, 25)):
            for backwards in (False, True):
                with self.subTest(kind=kind, backwards=backwards):
                    data = self.roundtrip([fixture(kind, backwards)])
                    self.assertEqual(
                        data[:9],
                        MAGIC + struct.pack("<IB", 1, kind | (128 if backwards else 0)),
                    )
                    self.assertEqual(len(data), 8 + length)

    def test_generic_index_width_at_boundaries_and_subnormal(self):
        for count, width in ((3, 1), (256, 1), (257, 2), (65536, 2), (65537, 4)):
            vertices = [(1.0, -0.0, 2**-149)] * count
            shape = indexed(vertices, [(count - 1, 0, 1)])
            data = self.roundtrip([shape])
            self.assertEqual(data[8], 0)
            self.assertEqual(len(data), 17 + 12 * count + 3 * width)
            self.assertEqual(
                data[-3 * width : -2 * width], (count - 1).to_bytes(width, "little")
            )

    def test_mismatching_vertex_or_face_order_is_kept(self):
        s = fixture(3)
        s["vertices"][8] = tuple(0.0 if v == 0.0 else v for v in s["vertices"][8])
        self.assertEqual(self.roundtrip([s])[8], 0)
        s = fixture(2)
        s["triangles"][1], s["triangles"][2] = s["triangles"][2], s["triangles"][1]
        self.assertEqual(self.roundtrip([s])[8], 0)

    def test_homonymous_owners_and_materials_remain_directory_properties(self):
        a = fixture(3)
        b = {**fixture(1), "source_node": 9}
        raw = self.roundtrip([a, b])
        layer = directory([a, b], raw, "cpage")
        layer["materials"] = [
            {"friction": 0.25, "restitution": 0.2},
            {"friction": 0.75, "restitution": 0},
        ]
        owners = layer["source_nodes"][:]
        entries = decode_collision(raw, layer)
        self.assertEqual(
            [
                (x["id"], x["source_node"], m["friction"])
                for x, m in zip(entries, layer["materials"], strict=True)
            ],
            [("roof", 3, 0.25), ("roof", 9, 0.75)],
        )
        layer["source_nodes"] = [9, 3]
        self.assertEqual(
            [s["source_node"] for s in decode_collision(raw, layer)], owners[::-1]
        )
        layer["materials"].pop()
        with self.assertRaises(ValueError):
            decode_collision(raw, layer)

    def test_legacy_file_and_binary_verification(self):
        a = fixture(1)
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder)
            for raw, suffix in (
                (encode_collision([a]), "cpage"),
                (json.dumps([a]).encode(), "json"),
            ):
                descriptor = directory([a], raw, suffix)
                target = path / descriptor["file"]
                target.write_bytes(raw)
                self.assertEqual(
                    point_bits(read_collision(path, descriptor)[0]), point_bits(a)
                )
                target.write_bytes(raw[:-1])
                self.assertRaises(ValueError, read_collision, path, descriptor)
                target.write_bytes(bytes([raw[0] ^ 1]) + raw[1:])
                self.assertRaises(ValueError, read_collision, path, descriptor)
        minimal = indexed([(0.0, 0.0, 0.0)], [(0, 0, 0)])
        del minimal["source_node"]
        layer = {"file": "old.json", "ids": ["roof"], "triangles": 1}
        self.assertEqual(
            decode_collision(json.dumps([minimal]).encode(), layer),
            json.loads(json.dumps([minimal])),
        )

    def test_first_directory_can_lack_a_tree_and_packed_bounds_are_enforced(self):
        shape = fixture(1)
        raw = encode_collision([shape])
        desc = directory([shape], raw, "cpage")
        desc.pop("source_nodes")
        restored = decode_collision(raw, desc)[0]
        self.assertNotIn("source_node", restored)
        self.assertEqual(point_bits(restored), point_bits(shape))
        desc["bounds"] = [[-1, -1, -1], [1, 1, 1]]
        with self.assertRaises(ValueError):
            decode_collision(raw, desc)

    def test_bad_tokens_bounds_and_nonfinite_floats(self):
        s = fixture(0)
        data = encode_collision([s])
        desc = directory([s], data, "cpage")
        variants = [data[:k] for k in (0, 4, 7, 8, 9, 14, len(data) - 1)]
        variants.extend(
            [
                b"OCP2" + data[4:],
                data + b"\0",
                data[:8] + b"\x80" + data[9:],
                data[:8] + b"\x04" + data[9:],
                data[:8] + b"\xff" + data[9:],
                data[:9] + struct.pack("<I", 0xFFFFFFFF) + data[13:],
                data[:-1] + b"\xfe",
                data[:17] + struct.pack("<I", 0x7F800000) + data[21:],
            ]
        )
        for bad in variants:
            with self.assertRaises(ValueError):
                decode_collision(bad, desc)
        foreign = {**desc, "triangles": desc["triangles"] - 1}
        with self.assertRaises(ValueError):
            decode_collision(data, foreign)
        for bad in (float("nan"), float("inf"), 1e150):
            copy_of_s = copy.deepcopy(s)
            copy_of_s["vertices"][0] = (bad, 0, 0)
            with self.assertRaises(ValueError):
                encode_collision([copy_of_s])
        s = fixture(2)
        s["triangles"][0] = (0.0, 1, 2)
        with self.assertRaises(ValueError):
            encode_collision([s])


if __name__ == "__main__":
    unittest.main()
