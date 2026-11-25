# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

"""Run the native consumers on two instances sharing geometry below a rotated parent."""

import copy
import json
import math
import os
import runpy
import struct
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

FIX = Path(__file__).resolve().parent
CITY = os.environ.get("OSAGE_CITY_BIN")


def sample(out, subtree_proxy=False, certified=False):
    path = out / "object.glb"
    subprocess.run(
        [
            sys.executable,
            FIX / "convert.py",
            "--id",
            "slab",
            "--drawable",
            FIX / "fixtures/slab.ydr.xml",
            "--bounds",
            FIX / "fixtures/slab.ybn.xml",
            "--acoustic-proxy",
            "-o",
            path,
        ],
        check=True,
        capture_output=True,
    )
    source = path.read_bytes()
    split = 20 + struct.unpack_from("<I", source, 12)[0]
    doc = json.loads(source[20:split])
    (piece,) = doc["nodes"]
    doc["asset"]["extras"]["osageObject"]["id"] = "crossing"
    doc["nodes"] = [
        {
            "translation": [10, 0, 12],
            "rotation": [0, math.sqrt(0.5), 0, math.sqrt(0.5)],
            "children": [1],
            "extras": {"osage": {"id": "bridge"}},
        },
        {
            "translation": [2, 0, 0],
            "children": [2, 3],
            "extras": {"osage": {"id": "upper-deck"}},
        },
    ]
    for side, z in [("west", -3), ("east", 3)]:
        node = copy.deepcopy(piece)
        node["translation"] = [0, 0, z]
        binding = node["extras"]["osage"]
        if certified:
            # Every point on the square lies within 1 m of the inscribed triangle.
            binding["visualLods"][0]["maxDeviationM"] = 1.0
        binding["id"] = f"span/{side}"
        binding["collision"][0]["id"] = f"{side}-floor"
        binding["acoustic"][0]["id"] = f"{side}-sound"
        doc["nodes"].append(node)
    if subtree_proxy:
        group = doc["nodes"][1]
        group["mesh"] = piece["mesh"]
        group_geometry = copy.deepcopy(piece["extras"]["osage"])
        group_geometry.update(
            id="upper-deck",
            subtreeProxy={"visual": "low", "collision": "low", "acoustic": "low"},
        )
        group_geometry["collision"][0]["id"] = "deck-hull"
        group_geometry["acoustic"][0]["id"] = "deck-sound"
        group["extras"]["osage"] = group_geometry
    encoded = json.dumps(doc).encode()
    encoded += b" " * (-len(encoded) % 4)
    payload = struct.pack("<I4s", len(encoded), b"JSON") + encoded + source[split:]
    path.write_bytes(struct.pack("<4sII", b"glTF", 2, 12 + len(payload)) + payload)
    replay = out / "drop.json"
    cam = {"eye": [10, 4, 15], "aim": [7, 2, 10]}
    replay.write_text(
        json.dumps(
            {
                "version": 1,
                "tick_hz": 60,
                "gravity_m_s2": 9.81,
                "terminal_m_s": 55,
                "viewport": [640, 480],
                "vertical_fov_deg": 45,
                "routes": [
                    {
                        "id": "drop",
                        "ticks": 120,
                        "start_m": [7, 3, 10],
                        "velocity_m_s": [0, 0, 0],
                        "expected_support": "west-floor",
                        "camera": [dict(cam, tick=0), dict(cam, tick=120)],
                    }
                ],
            }
        )
    )
    return path, replay


@unittest.skipUnless(CITY, "set OSAGE_CITY_BIN to run native geometry consumers")
class ObjectHierarchyTest(unittest.TestCase):
    def test_coarse_parent_replaces_its_descendants_independently_per_channel(self):
        with tempfile.TemporaryDirectory() as temp:
            path, _ = sample(Path(temp), subtree_proxy=True)
            channels = ("visual", "collision", "acoustic")
            for chosen in ((), ("visual",), ("collision",), ("acoustic",), channels):
                options = [
                    arg for channel in chosen for arg in (f"--{channel}-lod", "low")
                ]
                for x, coarse in ((10, True), (7, False), (13, False)):
                    result = json.loads(
                        subprocess.check_output(
                            [
                                CITY,
                                "--scene",
                                str(path),
                                "--inspect-object",
                                *options,
                                "--ray",
                                f"{x},4,10",
                                f"{x},0,10",
                            ]
                        )
                    )
                    for channel in channels:
                        hits = result["ray"][channel]
                        if (channel in chosen) != coarse:
                            self.assertEqual(hits, [], (x, chosen, channel))
                            continue
                        expected = ["bridge", "upper-deck"]
                        if not coarse:
                            expected.append("span/west" if x == 7 else "span/east")
                        self.assertEqual([hit["nodePath"] for hit in hits], [expected])

    def test_view_dependent_mesh_keeps_physical_and_acoustic_owners(self):
        with tempfile.TemporaryDirectory() as temp:
            out = Path(temp)
            views = []
            for distance, height in [(50, 1000), (1000, 1000), (350, 1000), (350, 2000)]:
                views += ["--detail-view", f"10,2,{10+distance}", "10,2,10", "90", str(height)]
            for certified in (False, True):
                path, _ = sample(out, certified=certified)
                result = json.loads(subprocess.check_output([
                    CITY, "--scene", path, "--inspect-object", "--ray", "7,4,10", "7,0,10", *views
                ]))
                samples = result["detailViews"]
                if certified:
                    self.assertEqual([s["triangles"] for s in samples], [4, 2, 2, 4])
                    self.assertEqual([s["nodes"][0]["level"] for s in samples], ["high", "low", "low", "high"])
                    for sample_data in samples:
                        for node in sample_data["nodes"]:
                            self.assertLessEqual(node["errorBoundPx"], 2)
                    self.assertEqual(samples[1]["nodes"][0]["owner"], ["bridge", "upper-deck", "span/west"])
                else:
                    self.assertEqual([s["triangles"] for s in samples], [4]*4)
                    self.assertTrue(all(not s["nodes"] for s in samples))
                for channel in ("collision", "acoustic"):
                    self.assertEqual(result["ray"][channel][0]["nodePath"], ["bridge", "upper-deck", "span/west"])

    @unittest.skipUnless(os.environ.get("OSAGE_TEST_GPU") == "1", "opt in to offscreen GPU captures")
    def test_adaptive_draw_groups_in_rendered_frames(self):
        pixels = runpy.run_path(str(FIX.parents[1] / "worlds/ville-paris/check-pictures.py"))["rgb"]
        with tempfile.TemporaryDirectory() as temp:
            out = Path(temp)
            model, replay = sample(out, certified=True)
            data = json.loads(replay.read_text())
            data.update(viewport=[640, 360], vertical_fov_deg=60)
            for distance, triangles in [(8, 4), (800, 2)]:
                for view in data["routes"][0]["camera"]:
                    view["aim"] = [10, 1, 10]
                    view["eye"] = [10, 1 + 0.6*distance, 10 + 0.8*distance]
                replay.write_text(json.dumps(data))
                images = {}
                for mode in ("auto", "high", "low"):
                    picture = out / f"{distance}-{mode}.png"
                    args = [CITY, "--scene", model, "--replay", replay, "--actor", "capsule",
                            "--physics", "rapier", "--tick", "0", "--shot", picture]
                    if mode != "auto":
                        args += ["--visual-lod", mode]
                    render = subprocess.run(args, check=True, capture_output=True, text=True)
                    if mode == "auto":
                        self.assertIn(f"adaptive object meshes at capture: 2, triangles: {triangles}",
                                      render.stdout + render.stderr)
                    images[mode] = pixels(picture)
                self.assertEqual(images["auto"], images["high" if distance == 8 else "low"])
                if distance == 8:
                    self.assertTrue(images["high"] != images["low"], "near camera must distinguish the two meshes")

    def test_same_owner_for_draw_sound_prediction_and_solved_impact(self):
        with tempfile.TemporaryDirectory() as temp:
            out = Path(temp)
            path, replay = sample(out)
            cmd = [CITY, "--scene", str(path)]
            for side, x in [("west", 7), ("east", 13)]:
                owners = ["bridge", "upper-deck", f"span/{side}"]
                for lod in ("high", "low"):
                    data = json.loads(
                        subprocess.check_output(
                            [
                                *cmd,
                                "--visual-lod",
                                lod,
                                "--inspect-object",
                                "--ray",
                                f"{x},4,10",
                                f"{x},0,10",
                            ]
                        )
                    )
                    self.assertEqual(data["roots"], ["bridge"])
                    self.assertEqual(
                        data["nodes"][1]["children"], ["span/west", "span/east"]
                    )
                    self.assertEqual(
                        data["nodes"][1]["parentFromLocal"][3][:3], [2, 0, 0]
                    )
                    for channel in ("visual", "collision", "acoustic"):
                        (hit,) = data["ray"][channel]
                        self.assertEqual(hit["nodePath"], owners)
            cmd.extend(["--replay", str(replay)])
            pred = json.loads(subprocess.check_output([*cmd, "--predict-impact"]))[
                "cases"
            ][0]["hit"]
            expected = ["bridge", "upper-deck", "span/west"]
            self.assertEqual(pred["nodePath"], expected)
            self.assertAlmostEqual(pred["seconds"], math.sqrt(4 / 9.81), places=6)
            subprocess.run(
                [*cmd, "--actor", "capsule", "--physics", "rapier", "--bench", out],
                check=True,
                capture_output=True,
            )
            contact = json.loads((out / "dynamics.json").read_text())["run"][
                "outcomes"
            ][0]["first_contacts"][0]
            self.assertEqual(
                (contact["surface"], contact["nodePath"]), (pred["surface"], expected)
            )
