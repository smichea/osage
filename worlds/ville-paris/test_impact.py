# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

import math
import runpy
import unittest
from pathlib import Path

from impact_geometry import basis, gap, segment_box_gap, surface_witnesses

BonnetRebound = runpy.run_path(str(Path(__file__).with_name("compare-impact.py")))[
    "BonnetRebound"
]


class CapsuleBoxTest(unittest.TestCase):
    def test_rounded_face_edge_and_corner(self):
        for center, expected in [
            ((0, 3, 0), 1.5),
            ((2, 2, 0), math.sqrt(2) - 0.5),
            ((2, 2, 2), math.sqrt(3) - 0.5),
            ((0, 1.5, 0), 0),
        ]:
            with self.subTest(center=center):
                self.assertAlmostEqual(
                    segment_box_gap(center, [0] * 3, [1] * 3, 0.5), expected
                )

    def test_segment_interior_is_closest(self):
        self.assertAlmostEqual(
            segment_box_gap([0, 2, 0], [2, 0, 0], [1] * 3, 0.25), 0.75
        )
        # The separating face here is perpendicular to the segment and the box Z axis.
        self.assertAlmostEqual(
            segment_box_gap([0.95, -0.95, 0], [2, 2, 0], [1] * 3, 0.25),
            -(math.sqrt(2) * 0.05 + 0.25),
        )

    def test_box_and_actor_rotations(self):
        quarter = [0, 0, math.sqrt(0.5), math.sqrt(0.5)]
        self.assertAlmostEqual(
            gap(
                {"half_segment_m": 1, "radius_m": 0.25},
                {"center": [6.25, 4, 3], "rotation_xyzw": quarter},
                basis(quarter),
                [3, 4, 3],
                [2, 1, 3],
            ),
            1,
        )

    def test_a_flat_contact_spans_the_face_and_separates_each_rotating_body(self):
        # This quaternion takes local Y exactly to world Z. The two metre
        # half-segment projects past each end of the one metre half-box.
        pose = {
            "center": [0.5, 1.2, 0],
            "rotation_xyzw": [0.5] * 4,
            "linear_velocity": [0, -3, 0],
            "angular_velocity": [2, 0, 0],
        }
        box = {
            "center": [0, 0, 0],
            "rotation_xyzw": [0, 0, 0, 1],
            "linear_velocity": [0, -1, 0],
            "angular_velocity": [1, 0, 0],
        }
        witness = surface_witnesses(
            {"half_segment_m": 2, "radius_m": 0.25}, pose, box, [1] * 3
        )
        self.assertEqual(witness["normal"], (0, 1, 0))
        self.assertAlmostEqual(witness["gap_m"], -0.05)
        low, high = witness["points"]
        self.assertEqual(
            [low["box_point_m"], high["box_point_m"]], [(0.5, 1, -1), (0.5, 1, 1)]
        )
        self.assertEqual([low["relative_m_s"], high["relative_m_s"]], [-1, -3])
        self.assertEqual(
            (high["centres_m_s"], high["limb_spin_m_s"], high["box_spin_m_s"]),
            (-2, -2, -1),
        )

    def test_witness_points_reproduce_gap_on_edge_corner_and_inside(self):
        rest = {
            "rotation_xyzw": [0, 0, 0, 1],
            "linear_velocity": [0, 0, 0],
            "angular_velocity": [0, 0, 0],
        }
        box = dict(rest, center=[0, 0, 0])
        for center, half, orientation, kind in [
            ([2, 2, 0], 0, rest["rotation_xyzw"], "closest"),
            ([2, 2, 2], 0, rest["rotation_xyzw"], "closest"),
            ([0.7, 0.1, 0], 0.2, rest["rotation_xyzw"], "minimum_translation"),
            (
                [0.95, -0.95, 0],
                2,
                [0, 0, -math.sin(math.pi / 8), math.cos(math.pi / 8)],
                "minimum_translation",
            ),
        ]:
            with self.subTest(center=center):
                part = {"half_segment_m": half, "radius_m": 0.25}
                pose = dict(rest, center=center, rotation_xyzw=orientation)
                w = surface_witnesses(part, pose, box, [1] * 3)
                self.assertEqual(w["normal_kind"], kind)
                self.assertAlmostEqual(
                    w["gap_m"],
                    gap(
                        part, pose, basis(box["rotation_xyzw"]), box["center"], [1] * 3
                    ),
                )
                self.assertAlmostEqual(math.hypot(*w["normal"]), 1)
                for pair in w["points"]:
                    for a, b, n in zip(
                        pair["limb_point_m"],
                        pair["box_point_m"],
                        w["normal"],
                        strict=True,
                    ):
                        self.assertAlmostEqual(a - b, n * w["gap_m"])
                    self.assertEqual(pair["relative_m_s"], 0)


class NativeWindowTest(unittest.TestCase):
    def test_comparison_uses_a_single_clock_and_rejects_a_truncated_peak(self):
        windows = runpy.run_path(str(Path(__file__).with_name("compare-impact.py")))[
            "common_depth_windows"
        ]

        def trial(reported, end):
            return {
                "reported_step": reported,
                "depth_samples": [
                    {"step": i, "part": "sole", "depth_m": i * 0.001}
                    for i in range(93, end + 1)
                ],
            }

        short, long = windows({"a": trial(95, 107), "b": trial(96, 105)}, 240)
        self.assertEqual((short["first_step"], short["last_step"]), (95, 101))
        self.assertEqual(short["peaks"]["a"], short["peaks"]["b"])
        self.assertEqual(long["peaks"]["a"]["step"], 107)
        self.assertIsNone(long["peaks"]["b"])


class ReboundTest(unittest.TestCase):
    def test_contact_then_airborne_rise(self):
        bounce = BonnetRebound()
        for i, (height, gap_m, hit) in enumerate(
            [
                (3, 2, False),
                (1, 0.0, True),
                (1.1, 0.02, False),
                (1.2, 0.05, False),
                (1.05, 0.01, False),
                (1, 0, True),
            ]
        ):
            bounce.observe(i, height, True, gap_m, hit, False)
        self.assertEqual(bounce.best["start_tick"], 2)
        self.assertEqual(bounce.best["apex_tick"], 3)
        self.assertAlmostEqual(bounce.best["rise_m"], 0.2)

    def test_edge_exit_and_other_surfaces(self):
        for over, other in [(False, False), (True, True)]:
            bounce = BonnetRebound()
            bounce.observe(0, 1, True, 0, True, False)
            bounce.observe(1, 2, over, 1, False, other)
            bounce.observe(2, 3, True, 2, False, False)
            self.assertIsNone(bounce.best)


if __name__ == "__main__":
    unittest.main()
