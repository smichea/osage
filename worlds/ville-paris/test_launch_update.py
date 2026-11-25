# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
import shutil
import struct
import tempfile
import unittest
import zlib
from pathlib import Path
from unittest.mock import patch

import paging_launch_update as update
from paging_physics_directory import LIMIT


class LaunchUpdate(unittest.TestCase):
    def test_declared_length_and_frame_boundary(self):
        packed = zlib.compress(b"some district bytes")
        for size, body in [(2, packed), (LIMIT + 1, packed), (19, packed + b"tail"), (19, packed[:-1])]:
            with self.subTest(size=size, body=body), self.assertRaises(ValueError):
                update.payload(b"ODI3" + struct.pack("<I", size) + body)
        self.assertEqual(update.payload(b"ODI3" + struct.pack("<I", 19) + packed), b"some district bytes")

    def test_verify_previous_immutable_object_before_using_it(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            key = update.address(b"abc")
            (root / (key.split(":")[0] + ".district")).write_bytes(b"xyz")
            with self.assertRaisesRegex(ValueError, "digest"):
                update.update_reference(root, key, b"abc", None)
            with self.assertRaisesRegex(ValueError, "expected previous"):
                update.update_reference(root, "../x:3", b"abc", None)

    @unittest.skipUnless(shutil.which("zstd") and update.zstd, "both Zstandard encoders required")
    def test_cli_encoder_for_older_python_uses_the_same_raw_dictionary(self):
        base = b"ODI2" + bytes(range(256)) * 400
        target = base[:-500] + b"a changed suffix"
        body = update.zstd.compress(base)
        with patch.object(update, "zstd", None):
            self.assertEqual(update.payload(b"ODI3" + struct.pack("<I", len(base)) + body), base)
            delta = update.dictionary_patch(base, target)
        self.assertEqual(update.zstd.decompress(delta, zstd_dict=update.zstd.ZstdDict(base, is_raw=True)), target)
