# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

"""Analytic triangle-surface controls and optional LOD certificates on the normal converter path."""

import json
import math
import struct
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from convert import convert
from lod_error import deviation
from test_convert import BOUNDS, DRAWABLE, ROOT, Packed


class LodDeviation(unittest.TestCase):
    def test_identical_reindexed_reversed_and_duplicate_faces(self):
        a, b, c = (0, 0, 0), (2, 0, 0), (0, 2, 0)
        result = deviation([(a, b, c)], [(c, b, a), (b, a, c)], 0.001)
        self.assertEqual(
            (result["lowerM"], result["upperM"], result["subdivisions"]), (0, 0, 0)
        )

    def test_two_parallel_triangles_and_degenerate_target(self):
        face = ((0, 0, 0), (1, 0, 0), (0, 1, 0))
        offset = tuple((x, y, z + 0.375) for x, y, z in face)
        result = deviation([face], [offset], 1e-6)
        self.assertLessEqual(result["lowerM"], 0.375)
        self.assertGreaterEqual(result["upperM"], 0.375)
        self.assertLessEqual(result["upperM"] - result["lowerM"], 1e-6)
        self.assertEqual(
            result["upperM"],
            struct.unpack("<f", struct.pack("<f", result["upperM"]))[0],
        )
        # A collapsed target edge is still a closed convex surface with a well-defined distance.
        line = ((0, 0, 0), (1, 0, 0), (0, 0, 0))
        result = deviation([line], [face], 1e-6)
        self.assertLessEqual(result["lowerM"], 1.0)
        self.assertGreaterEqual(result["upperM"], 1.0)
        self.assertLess(result["upperM"] - result["lowerM"], 1e-6)

    def test_missing_surface_in_both_directions(self):
        face = ((0, 0, 0), (1, 0, 0), (0, 1, 0))
        island = tuple((x, y, z + 11) for x, y, z in face)
        for a, b in (([face], [face, island]), ([face, island], [face])):
            bound = deviation(a, b, 0.00001)
            self.assertLessEqual(bound["lowerM"], 11)
            self.assertGreaterEqual(bound["upperM"], 11)
            self.assertLessEqual(bound["upperM"] - bound["lowerM"], 0.00001)

    def test_same_vertices_different_diagonal_changes_the_interior(self):
        # Two complementary pairs of faces of a regular tetrahedron. Every vertex is shared.
        # The greatest distance is at either shared edge's midpoint, half a tetrahedron altitude.
        a, b, c, d = (1, 1, 1), (-1, -1, 1), (-1, 1, -1), (1, -1, -1)
        high, low = [(a, b, c), (c, d, a)], [(a, b, d), (c, d, b)]
        target = 2 / math.sqrt(3)
        result = deviation(high, low, 1e-5)
        self.assertLessEqual(result["lowerM"], target)
        self.assertGreaterEqual(result["upperM"], target)
        self.assertLessEqual(result["upperM"] - result["lowerM"], 1e-5)
        self.assertGreater(result["subdivisions"], 0)
        limited = deviation(high, low, 1e-5, max_splits=0)
        self.assertFalse(limited["gapReached"])
        self.assertGreaterEqual(limited["upperM"], target)

    def test_retriangulated_square_has_zero_surface_distance(self):
        a, b, c, d = (-1, -1, 0), (1, -1, 0), (1, 1, 0), (-1, 1, 0)
        result = deviation([(a, b, c), (c, d, a)], [(a, b, d), (c, d, b)], 1e-5)
        self.assertEqual(result["lowerM"], 0.0)
        self.assertLessEqual(result["upperM"], 1e-5)
        self.assertTrue(result["gapReached"])

    def test_cli_and_actual_accessor_values(self):
        legacy, _ = convert(object_id="slab", drawable=DRAWABLE)
        old = Packed(legacy)
        self.assertNotIn("maxDeviationM", old.roles["visualLods"][0])
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "slab.glb"
            run = subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "convert.py"),
                    "--id",
                    "slab",
                    "--drawable",
                    str(DRAWABLE),
                    "--lod-error",
                    "--report",
                    "-o",
                    str(output),
                ],
                check=True,
                capture_output=True,
                text=True,
            )
            doc = Packed(output.read_bytes())
        (record,) = json.loads(run.stdout)["visualDeviation"]
        (lod,) = doc.roles["visualLods"]
        self.assertEqual(lod["maxDeviationM"], record["upperM"])
        # The full square's two rear corners are 2/sqrt(5) from the sloping sides of the lower triangle.
        self.assertLessEqual(record["lowerM"], 2 / math.sqrt(5))
        self.assertGreaterEqual(record["upperM"], 2 / math.sqrt(5))
        self.assertTrue(record["gapReached"])
        self.assertEqual(doc.binary, old.binary)
        self.assertEqual(record["requestedGapM"], 0.01)

    def test_explicit_invalid_precision_is_rejected_even_without_lods(self):
        for t in (0.0, -1.0, math.inf, math.nan):
            with self.assertRaisesRegex(ValueError, "finite and positive"):
                convert(object_id="slab", bounds=BOUNDS, lod_error=t)
