# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""The relay's signature and cleaning, without the network."""

import unittest

import main


class Signature(unittest.TestCase):
    def test_the_same_crash_on_two_machines_has_one_signature(self):
        a = {
            "kind": "panic",
            "message": "called `Option::unwrap()` on a `None` value at C:\\Users\\adrien\\osage\\src\\play.rs:412:9",
            "trace": "   0: std::panicking::begin_panic\n   1: kart_native::play::drive::h0123456789abcdef\n   2: bevy_ecs::system::run",
        }
        b = {
            "kind": "panic",
            "message": "called `Option::unwrap()` on a `None` value at /home/priya/osage/src/play.rs:415:9",
            "trace": "   0: std::panicking::begin_panic\n   1: kart_native::play::drive::hfedcba9876543210\n   2: bevy_ecs::system::run",
        }
        self.assertEqual(main.signature(a), main.signature(b))
        self.assertNotEqual(main.signature(a), main.signature({**a, "kind": "hang"}))
        self.assertEqual(main.frames(a["trace"]), ["kart_native::play::drive", "bevy_ecs::system::run"])

    def test_a_title_reads_as_the_message_was_written(self):
        line = "2026-11-13T02:21:46.143228Z ERROR wgpu_core::device: Validation Error"
        self.assertEqual(main.headline(line + "\nmore"), "wgpu_core::device: Validation Error")

    def test_text_from_a_report_mentions_no_one_and_closes_no_fence(self):
        self.assertEqual(main.safe("@adrien ```"), "@\u200badrien '''")


if __name__ == "__main__":
    unittest.main()
