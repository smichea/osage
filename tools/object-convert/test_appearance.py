# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

"""Portable images, material bindings and fallback reports on authored geometry."""

import json
import os
import runpy
import shutil
import subprocess
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

from convert import convert
from fixtures.make_appearance import fixture
from test_convert import FIX, ROOT, Packed

SOURCE = FIX / "textured.ydr.xml"


class AppearanceTests(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.directory = Path(tmp.name)
        self.appearance = fixture(self.directory)

    def set_manifest(self, **fields):
        spec = json.loads(self.appearance.read_bytes())
        spec.update(fields)
        self.appearance.write_text(json.dumps(spec))

    def result(self, path=SOURCE):
        data, report = convert(
            object_id="signal", drawable=path, appearance=self.appearance
        )
        return Packed(data), report["appearance"]["materials"]

    def test_distinct_lod_materials_share_original_encoded_image_and_uv0(self):
        obj, rows = self.result()
        materials = obj.json["materials"]
        self.assertEqual(
            [m["pbrMetallicRoughness"]["baseColorTexture"] for m in materials],
            [{"index": 0, "texCoord": 0}] * 2,
        )
        self.assertEqual(
            [(m["alphaMode"], m["alphaCutoff"], m["doubleSided"]) for m in materials],
            [("MASK", 0.4, True), ("MASK", 0.6, False)],
        )
        (img,) = obj.json["images"]
        self.assertEqual(obj.json["textures"], [{"source": 0}])
        self.assertEqual(img["mimeType"], "image/png")
        view = obj.json["bufferViews"][img["bufferView"]]
        self.assertNotIn("target", view)
        self.assertEqual(view["byteOffset"] % 4, 0)
        self.assertEqual(
            obj.binary[view["byteOffset"] : view["byteOffset"] + view["byteLength"]],
            (self.directory / "stripes.png").read_bytes(),
        )
        self.assertEqual(
            [mesh["primitives"][0]["material"] for mesh in obj.json["meshes"]], [0, 1]
        )
        self.assertEqual([r["status"] for r in rows], ["base colour texture bound"] * 2)
        self.assertEqual(rows[0]["untranslatedParameters"], ["SignalPulse"])
        neutral, _ = convert(object_id="signal", drawable=SOURCE)
        geometry = Packed(neutral)
        self.assertEqual(obj.json["accessors"], geometry.json["accessors"])
        size = geometry.json["buffers"][0]["byteLength"]
        self.assertEqual(obj.binary[:size], geometry.binary[:size])
        self.assertNotIn("images", geometry.json)
        self.assertNotIn("alphaMode", geometry.json["materials"][0])

    def test_source_slots_unmatched_names_and_unused_supplier_entries(self):
        self.set_manifest(textures={"other": "not-read.png"}, materials={})
        obj, rows = self.result()
        self.assertNotIn("images", obj.json)
        self.assertEqual(rows[1]["status"], "no image supplied for diffuse name")
        self.assertEqual(rows[1]["sourceProgram"], "untranslated")
        self.assertEqual(
            rows[1]["portableRaster"], {"alphaMode": "OPAQUE", "doubleSided": False}
        )
        data, report = convert(
            object_id="signal",
            drawable=FIX / "slab.ydr.xml",
            appearance=self.appearance,
        )
        self.assertTrue(data)
        self.assertEqual(
            report["appearance"]["materials"][0]["status"], "no named diffuse texture"
        )
        self.assertEqual(report["appearance"]["unusedTextureNames"], ["other"])

    def test_uv_required_on_every_visual_level_that_references_the_slot(self):
        root = ET.parse(SOURCE)
        buf = root.find(".//DrawableModelsMedium//VertexBuffer")
        layout = buf.find("Layout")
        layout.remove(layout.find("TexCoord0"))
        values = buf.find("Data")
        values.text = "\n".join(
            " ".join(line.split()[:-2]) for line in values.text.strip().splitlines()
        )
        path = self.directory / "bad.ydr.xml"
        root.write(path)
        with self.assertRaisesRegex(
            ValueError, "missing TEXCOORD_0 in signal/visual/medium"
        ):
            self.result(path)

    def test_named_aliases_and_two_filenames_with_identical_content_are_deduplicated(
        self,
    ):
        root = ET.parse(SOURCE)
        root.findall("ShaderGroup/Shaders/Item")[1].find(
            "Parameters/Item/Name"
        ).text = "panel_far_alias"
        path = self.directory / "alias.ydr.xml"
        root.write(path)
        shutil.copyfile(self.directory / "stripes.png", self.directory / "same.png")
        self.set_manifest(
            textures={"Panel_Stripes": "stripes.png", "panel_far_alias": "same.png"}
        )
        obj, rows = self.result(path)
        self.assertEqual(len(obj.json["images"]), 1)
        self.assertEqual([r["texture"] for r in rows], [0, 0])

    def test_invalid_manifests_and_files_fail_clearly(self):
        cases = [
            ({"textures": {"panel_stripes": "missing.png"}}, "missing.png"),
            ({"textures": {"panel_stripes": "appearance.json"}}, "PNG or JPEG"),
            ({"materials": {"2": {}}}, "unknown material slots"),
            ({"materials": {"1": {"shader": "panel_near"}}}, "expected shader"),
            ({"materials": {"0": {"alphaMode": "transparent"}}}, "alpha|MASK"),
            (
                {"materials": {"0": {"alphaMode": "MASK", "alphaCutoff": 1.1}}},
                "alphaCutoff",
            ),
            ({"materials": {"0": {"doubleSided": "yes"}}}, "must be boolean"),
            ({"materials": {"0": {"vertexColorAlpha": True}}}, "vertexColorAlpha"),
            ({"textures": {"PANEL_STRIPES": "a", "panel_stripes": "b"}}, "ambiguous"),
        ]
        for mapping, error in cases:
            with self.subTest(mapping=mapping):
                fixture(self.directory)
                self.set_manifest(**mapping)
                with self.assertRaisesRegex(ValueError, error):
                    self.result()

    def test_render_alpha_preserves_both_authored_layers_and_unselected_slots(self):
        original, _ = self.result()
        recipe = json.loads(self.appearance.read_bytes())["materials"]
        recipe["0"]["vertexColorAlpha"] = "one"
        self.set_manifest(materials=recipe)
        obj, report = self.result()
        rendered, ids = obj.mesh(0)
        source, source_ids = original.mesh(0)
        self.assertEqual(source_ids, ids)
        self.assertEqual(rendered["COLOR_1"], source["COLOR_1"])
        self.assertEqual(rendered["_OSAGE_SOURCE_COLOR_0"], source["COLOR_0"])
        self.assertEqual(
            rendered["COLOR_0"], [(*v[:3], 255) for v in source["COLOR_0"]]
        )
        # Every source alpha is below the cutoff, including between vertices. A passing
        # texel on the rendered panel therefore distinguishes the two appearance choices.
        cutoff = obj.json["materials"][0]["alphaCutoff"]
        self.assertLess(max(v[3] for v in source["COLOR_0"]) / 255, cutoff)
        self.assertTrue(all((v[3] / 255) >= cutoff for v in rendered["COLOR_0"]))
        self.assertEqual(obj.mesh(1), original.mesh(1))
        self.assertEqual(report[0]["originalVertexColour"], "_OSAGE_SOURCE_COLOR_0")
        self.assertNotIn("vertexColorAlpha", report[1])
        fixture(self.directory, unit_vertex_alpha=True)
        all_levels, _ = self.result()
        far = all_levels.json["meshes"][1]["primitives"][0]["attributes"]
        self.assertEqual(far["_OSAGE_SOURCE_COLOR_0"], far["COLOR_0"])

    def test_implicit_unit_alpha_does_not_invent_authored_colours(self):
        fixture(self.directory, unit_vertex_alpha=True)
        root = ET.parse(SOURCE)
        for vb in root.findall(".//VertexBuffer"):
            layout = vb.find("Layout")
            layout.remove(layout.find("Colour0"))
            layout.remove(layout.find("Colour1"))
            data = vb.find("Data")
            data.text = "\n".join(
                " ".join(line.split()[:6] + line.split()[-2:])
                for line in data.text.strip().splitlines()
            )
        plain = self.directory / "plain.ydr.xml"
        root.write(plain)
        obj, rows = self.result(plain)
        for mesh_index, row in enumerate(rows):
            attrs, _ = obj.mesh(mesh_index)
            self.assertNotIn("COLOR_0", attrs)
            self.assertNotIn("_OSAGE_SOURCE_COLOR_0", attrs)
            self.assertNotIn("originalVertexColour", row)

    @unittest.skipUnless(
        os.environ.get("OSAGE_TEST_GPU") and os.environ.get("OSAGE_CITY_BIN"),
        "set OSAGE_TEST_GPU and OSAGE_CITY_BIN to render the original mask control",
    )
    def test_native_render_uses_only_the_authored_render_colour_attribute(self):
        read_rgb = runpy.run_path(
            str(ROOT.parents[1] / "worlds/ville-paris/check-pictures.py")
        )["rgb"]
        camera = {"eye": [0, 1, 3], "aim": [0, 1, 0]}
        route = {
            "id": "panel",
            "ticks": 60,
            "start_m": [25, 5, 25],
            "velocity_m_s": [0, 0, 0],
            "camera": [dict(camera, tick=0), dict(camera, tick=60)],
        }
        scene = {
            "version": 1,
            "tick_hz": 60,
            "gravity_m_s2": 9.81,
            "terminal_m_s": 55,
            "viewport": [320, 180],
            "vertical_fov_deg": 50,
            "routes": [route],
        }
        shots = {}
        for unit in (False, True):
            manifest = fixture(self.directory, unit_vertex_alpha=unit)
            buf, _ = convert(
                object_id="panel",
                drawable=SOURCE,
                bounds=FIX / "slab.ybn.xml",
                appearance=manifest,
            )
            model = self.directory / "panel.glb"
            model.write_bytes(buf)
            route["expected_support"] = Packed(buf).roles["collision"][0]["id"]
            replay = self.directory / "replay.json"
            replay.write_text(json.dumps(scene))
            for level in ("high", "medium"):
                picture = self.directory / f"{unit}-{level}.png"
                subprocess.run(
                    [
                        os.environ["OSAGE_CITY_BIN"],
                        "--scene",
                        model,
                        "--replay",
                        replay,
                        "--actor",
                        "capsule",
                        "--physics",
                        "rapier",
                        "--tick",
                        "0",
                        "--visual-lod",
                        level,
                        "--shot",
                        picture,
                    ],
                    check=True,
                    capture_output=True,
                )
                size, shots[unit, level] = read_rgb(picture)
                self.assertEqual(size, (320, 180))
        self.assertEqual(shots[False, "medium"], shots[True, "medium"])
        source, unit = shots[False, "high"], shots[True, "high"]
        changed = sum(
            max(abs(a - b) for a, b in zip(source[i : i + 3], unit[i : i + 3])) >= 32
            for i in range(0, len(source), 3)
        )
        self.assertGreater(changed, 320 * 180 // 10)

    def test_cli_protects_declared_image_even_when_its_name_is_unused(self):
        image = self.directory / "stripes.png"
        original = image.read_bytes()
        self.set_manifest(textures={"unused": image.name})
        result = subprocess.run(
            [
                sys.executable,
                ROOT / "convert.py",
                "--id",
                "panel",
                "--drawable",
                SOURCE,
                "--appearance",
                self.appearance,
                "-o",
                image,
            ],
            capture_output=True,
            check=False,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b"separate output path", result.stderr)
        self.assertEqual(image.read_bytes(), original)

    def test_fragment_without_a_matching_diffuse_keeps_rigid_visuals(self):
        self.set_manifest(textures={}, materials={})
        data, report = convert(
            object_id="actor",
            fragment=FIX / "pedestrian.yft.xml",
            appearance=self.appearance,
        )
        baseline, _ = convert(object_id="actor", fragment=FIX / "pedestrian.yft.xml")
        self.assertEqual(Packed(data).json["meshes"], Packed(baseline).json["meshes"])
        self.assertEqual(report["appearance"]["images"], 0)

    @unittest.skipUnless(shutil.which("ffmpeg"), "independent JPEG encoder missing")
    def test_jpeg_binding_through_cli_and_separate_decoder(self):
        jpeg = self.directory / "panel.jpg"
        subprocess.run(
            [
                "ffmpeg",
                "-hide_banner",
                "-loglevel",
                "error",
                "-i",
                self.directory / "stripes.png",
                "-frames:v",
                "1",
                jpeg,
            ],
            check=True,
        )
        self.set_manifest(textures={"panel_stripes": jpeg.name}, materials={})
        dest = self.directory / "output.glb"
        subprocess.run(
            [
                sys.executable,
                ROOT / "convert.py",
                "--id",
                "signal",
                "--drawable",
                SOURCE,
                "--appearance",
                self.appearance,
                "-o",
                dest,
            ],
            check=True,
            capture_output=True,
        )
        obj = Packed(dest.read_bytes())
        img = obj.json["images"][0]
        self.assertEqual(img["mimeType"], "image/jpeg")
        view = obj.json["bufferViews"][img["bufferView"]]
        data = obj.binary[view["byteOffset"] : view["byteOffset"] + view["byteLength"]]
        self.assertEqual(data, jpeg.read_bytes())

    @unittest.skipUnless(
        os.environ.get("OSAGE_RESOURCE_TOOL"), "loose binary bridge unavailable"
    )
    def test_loose_resource_uses_the_same_shader_binding(self):
        tool = Path(os.environ["OSAGE_RESOURCE_TOOL"])
        loose = self.directory / "signal.ydr"
        subprocess.run(
            [tool, "resource", "build", "--no-update-check", SOURCE, "-o", loose],
            check=True,
            capture_output=True,
        )
        byte_data, report = convert(
            object_id="signal",
            drawable=loose,
            resource_tool=tool,
            appearance=self.appearance,
        )
        obj = Packed(byte_data)
        self.assertEqual(len(obj.json["images"]), 1)
        self.assertEqual(
            [r["texture"] for r in report["appearance"]["materials"]], [0, 0]
        )


if __name__ == "__main__":
    unittest.main()
