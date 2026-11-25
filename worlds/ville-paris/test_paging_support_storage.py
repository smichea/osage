# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Check the catalogue's transport selection independently of the semantic JSON reader."""

import hashlib
import json
from pathlib import Path
import random
import shutil
import struct
import tempfile
import unittest
from unittest.mock import patch
import zlib

import paging_support_storage as storage
import paging_physics_directory as directories


class SupportStorage(unittest.TestCase):
    def test_unavailable_optional_encoder_keeps_legacy_bytes(self):
        raw = b'{"pages":[]}'
        with patch.object(storage, "zstd", None), patch.object(shutil, "which", return_value=None):
            self.assertEqual(storage.pack_catalogue(raw, "auto"), (zlib.compress(raw, 9), "zlib"))
            self.assertEqual(storage.unpack_support(zlib.compress(raw)), raw)

    def test_size_selection_and_forced_legacy(self):
        small = b"{}"
        # Distant repeated coordinates lie beyond the DEFLATE window.
        part = b",".join(str(i * 11113).encode() for i in range(12000))
        for raw in (small, b"[" + b",".join([part] * 4) + b"]"):
            with self.subTest(length=len(raw)):
                original = zlib.compress(raw, 9)
                self.assertEqual(storage.pack_catalogue(raw, "zlib"), (original, "zlib"))
                chosen, extension = storage.pack_catalogue(raw, "auto")
                self.assertLessEqual(len(chosen), len(original))
                self.assertEqual(storage.unpack_support(chosen), raw)
                if storage.zstd is not None or shutil.which("zstd"):
                    self.assertEqual(extension, "zlib" if raw == small else "zst")

    def test_physics_bounds_keep_the_publishers_precision(self):
        digest = "ab" * 32
        support = dict(file=digest + ".zlib", sha256=digest, bytes=12, decoded_bytes=50)
        layer = dict(file=digest + ".json", sha256=digest, bytes=50, bounds=[[-0.0, 0., 0.], [1., 2., 3.]])
        manifest = dict(support=support, tiles=[dict(x=0, z=0, physics=layer)])
        encode = lambda obj: json.dumps(obj, separators=(",", ":")).encode()
        compact = directories.columns(directories.physics_directory(manifest), encode)
        size, = struct.unpack_from("<I", compact)
        self.assertEqual(compact[4+size:8+size], struct.pack("<f", -0.0))
        for coordinate in (0.1, 1e100):
            layer["bounds"][1][0] = coordinate
            wire = directories.pack_physics(manifest, encode)
            regional = directories.pack_regional(dict(manifest, support_regions=[support]), None, encode,
                                                 lambda *_: self.fail("unused address object"), "auto")
            self.assertEqual(regional[:4], b"ODP1")
            self.assertEqual(wire[:4], b"ODP1")
            self.assertEqual(json.loads(storage.unpack_support(wire[8:])), directories.physics_directory(manifest))

    def test_bundled_pin_keeps_sparse_region_when_full_pages_cost_more(self):
        # The decoded candidate fits the catalogue budget, but its compressed representation loses.
        with tempfile.TemporaryDirectory() as work:
            root = Path(work)

            def write(data, suffix):
                digest = hashlib.sha256(data).hexdigest()
                name = digest + "." + suffix
                (root / name).write_bytes(data)
                return dict(file=name, sha256=digest, bytes=len(data))

            page = write(b"OCP1" + random.Random(201).randbytes(2048), "cpage")
            binding = hashlib.sha256(bytes(8) + page["sha256"].encode()).hexdigest()
            encode = lambda obj: json.dumps(obj, separators=(",", ":")).encode()
            catalogue = encode(dict(directory_sha256=binding, pages=["x" * 4096]))
            support = dict(write(zlib.compress(catalogue), "zlib"), decoded_bytes=len(catalogue))
            layer = dict(page, bounds=[[0, 0, 0], [1, 1, 1]])
            manifest = dict(support=support, support_regions=[dict(support, tiles=[0])],
                            tiles=[dict(x=0, z=0, physics=layer)])
            for extension in ("cpage", "json"):
                with self.subTest(extension=extension):
                    layer["file"] = page["sha256"] + "." + extension
                    plain = directories.pack_regional(manifest, root, encode, write, "zlib")
                    auto = directories.pack_regional(manifest, root, encode, write, "zlib", bundle=True)
                    self.assertEqual(plain, auto)

    @unittest.skipUnless(storage.zstd is not None and shutil.which("zstd"), "requires both Zstandard readers")
    def test_command_and_stdlib_read_each_others_frames(self):
        raw = bytes(range(251)) * 1000
        frame = storage.zstd.compress(raw, level=19)
        with patch.object(storage, "zstd", None):
            self.assertEqual(storage.unpack_support(frame), raw)
            encoded, extension = storage.pack_catalogue(raw, "auto")
        self.assertEqual(extension, "zst")
        self.assertEqual(storage.zstd.decompress(encoded), raw)


if __name__ == "__main__":
    unittest.main()
