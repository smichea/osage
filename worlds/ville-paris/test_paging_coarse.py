# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Exercise exact GLB fallback and OVC2 geometry, materials, optional attributes and lineage."""

import random
import runpy
import struct
import tempfile
import unittest
import zlib
from pathlib import Path

from paging_checks import glb
from paging_coarse import HEADER, candidate, pack_coarse, unpack_coarse
from paging_coarse_columns import LIMIT, blob, json_bytes, numbers, read_numbers

HERE = Path(__file__).resolve().parent


def make_glb(head, binary):
    head += b" " * (-len(head) % 4)
    return (
        struct.pack(
            "<4sIIII", b"glTF", 2, 28 + len(head) + len(binary), len(head), 0x4E4F534A
        )
        + head
        + struct.pack("<I4s", len(binary), b"BIN\0")
        + binary
    )


class Coarse(unittest.TestCase):
    def test_exact_generic_attributes_images_and_pose_tree(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "source.glb"
            runpy.run_path(str(HERE / "check-paged-lods.py"))["recipe"](source)
            raw = source.read_bytes()
            doc, chunk = glb(raw)
            attrs = doc["meshes"][0]["primitives"][0]["attributes"]
            self.assertTrue(
                {"COLOR_0", "COLOR_1", "TEXCOORD_0", "_OSAGE_SOURCE_COLOR_0"}
                <= attrs.keys()
            )
            self.assertEqual(len(doc["images"]), 2)
            # A new application attribute, a shear, a signed zero and arbitrary JSON are opaque to the codec.
            attrs["_APP_COLOR"] = attrs["COLOR_1"]
            doc["nodes"][0]["matrix"] = [
                1,
                0,
                0,
                0,
                0.125,
                1,
                0,
                0,
                0,
                0,
                1,
                0,
                -0.0,
                5,
                8,
                1,
            ]
            doc["asset"]["extras"]["unknown"] = {
                "quote": 'surface "47"',
                "bias": -1.25e-5,
                "set": [3, 4, 6],
            }
            raw = make_glb(json_bytes(doc), chunk[8:])
            wire = candidate(raw)
            self.assertEqual(wire[:4], b"OVC2")
            self.assertEqual(unpack_coarse(wire), raw)
            self.assertEqual(unpack_coarse(pack_coarse(raw)), raw)

    def test_fallback_preserves_historical_bytes(self):
        head = b'{"asset":{"version":"2.0"},"scene":0,"scenes":[{}]}'
        head += b" " * (-len(head) % 4)
        raw = (
            struct.pack("<4sIIII", b"glTF", 2, 20 + len(head), len(head), 0x4E4F534A)
            + head
        )
        historical = b"OVC1" + struct.pack("<I", len(raw)) + zlib.compress(raw, 6)
        self.assertIsNone(candidate(raw))
        self.assertEqual(pack_coarse(raw), historical)
        self.assertEqual(pack_coarse(raw, "ovc1"), historical)
        self.assertEqual(unpack_coarse(historical), raw)

    def test_smaller_historical_encoding_wins_on_representable_data(self):
        rng = random.Random(1)
        data = bytes(rng.randrange(2) for _ in range(8000))
        doc = {
            "asset": {"version": "2.0"},
            "buffers": [{"byteLength": len(data)}],
            "bufferViews": [{"buffer": 0, "byteLength": len(data)}],
        }
        raw = make_glb(json_bytes(doc), data)
        packed = candidate(raw)
        historical = pack_coarse(raw, "ovc1")
        self.assertEqual(unpack_coarse(packed), raw)
        self.assertLessEqual(len(historical), len(packed))
        self.assertEqual(pack_coarse(raw), historical)

    def test_lexemes_padding_and_overlapping_views(self):
        # Overlapping views and noncanonical JSON take the direct lossless channel.
        head = b'{ "asset" : {"version":"2.0"},"buffers":[{"byteLength":16}], "bufferViews":[{"buffer":0,"byteOffset":4,"byteLength":8},{"buffer":0,"byteOffset":0,"byteLength":16}],"extras":{"v":1.00e-4} }'
        raw = make_glb(head, bytes(range(16)))
        wire = candidate(raw)
        self.assertEqual(unpack_coarse(wire), raw)

    def test_clipped_corners_keep_order_zero_and_nan_payload_bits(self):
        # The byte codec must not reinterpret floating-point data. The scene reader handles semantic validity.
        word = struct.Struct("<3I")
        p = [
            word.pack(0x8000_0000, 0x3F80_0000, 0x7FC0_0042),
            word.pack(0, 0x3F80_0000, 1),
            word.pack(0x4000_0000, 1, 0),
        ]
        binary, views, accessors, meshes = bytearray(), [], [], []
        for i in range(45):
            face = b"".join(p[k] for k in (0, 1, 2, 0, 2, 1)) + word.pack(0, i, 0)
            views.append(
                {"buffer": 0, "byteOffset": len(binary), "byteLength": len(face)}
            )
            binary.extend(face)
            accessors.append(
                {"bufferView": i, "componentType": 5126, "type": "VEC3", "count": 7}
            )
            meshes.append({"primitives": [{"attributes": {"POSITION": i}}]})
        raw = make_glb(
            json_bytes(
                {
                    "asset": {"version": "2.0"},
                    "buffers": [{"byteLength": len(binary)}],
                    "meshes": meshes,
                    "accessors": accessors,
                    "bufferViews": views,
                }
            ),
            binary,
        )
        self.assertEqual(unpack_coarse(candidate(raw)), raw)

    def test_bad_headers_channels_and_sizes(self):
        raw = make_glb(
            b'{"asset":{"version":"2.0"},"buffers":[{"byteLength":4}]}', b"1234"
        )
        wire = candidate(raw)
        for bad in [
            wire[:7],
            wire[:-1],
            wire + b"x",
            b"OVC3" + wire[4:],
            wire[:4] + struct.pack("<I", LIMIT + 1) + wire[8:],
        ]:
            with self.assertRaises(ValueError):
                unpack_coarse(bad)
        for meta, geo in [
            (b"\xff", b"\0abcd"),
            (b"\0{}", b"\1BAD!"),
            (b"\0{}", b"\0abc"),
        ]:
            data = blob(meta) + blob(geo)
            bad = HEADER.pack(b"OVC2", len(data), 2, 4) + zlib.compress(data)
            with self.assertRaises(ValueError):
                unpack_coarse(bad)
        for seq in (
            [],
            [0] * 100,
            list(range(100, -10, -2)),
            [44, -1, 90, -23, 25],
            [12, 47244, 318, 68783, 223, 0],
        ):
            self.assertEqual(read_numbers(numbers(seq), len(seq)), seq)


if __name__ == "__main__":
    unittest.main()
