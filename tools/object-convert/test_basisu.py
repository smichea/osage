# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

"""Check optional Basis Universal objects against the independent Khronos transcoder."""

import hashlib
import json
import os
import shutil
import struct
import subprocess
import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

from convert import convert
from fixtures.make_appearance import fixture, rgba_png
from portable_images import EXTENSION, prepared_levels
from test_appearance import SOURCE
from test_convert import Packed

TOOL = os.environ.get("OSAGE_KTX_TOOL") or shutil.which("ktx")
CITY = os.environ.get("OSAGE_CITY_BIN")


def image_content(obj, index):
    image = obj.json["images"][index]
    v = obj.json["bufferViews"][image["bufferView"]]
    return obj.binary[v["byteOffset"] : v["byteOffset"] + v["byteLength"]]


def levels(path):
    data = path.read_bytes()
    assert data[:12] == b"\xabKTX 20\xbb\r\n\x1a\n"
    count = struct.unpack_from("<I", data, 40)[0]
    result = []
    for i in range(count):
        at, length, plain = struct.unpack_from("<3Q", data, 80 + 24 * i)
        assert length == plain
        result.append(data[at : at + length])
    return result


class BasisUniversalTest(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        self.manifest = fixture(self.root, basisu=True)

    def convert(self, manifest=None, source=SOURCE, **kwargs):
        if manifest is not None:
            self.manifest.write_text(json.dumps(manifest))
        data, report = convert(
            object_id="panel", drawable=source, appearance=self.manifest, **kwargs
        )
        return Packed(data), report["appearance"]

    @unittest.skipUnless(
        TOOL, "set OSAGE_KTX_TOOL to qualify KTX2 encoding and transcoding"
    )
    def test_explicit_levels_extension_fallback_and_extracted_blocks(self):
        obj, report = self.convert(ktx_tool=TOOL)
        self.assertEqual(obj.json["extensionsUsed"], [EXTENSION])
        self.assertNotIn("extensionsRequired", obj.json)
        (tex,) = obj.json["textures"]
        self.assertEqual(tex, {"source": 0, "extensions": {EXTENSION: {"source": 1}}})
        self.assertEqual(
            [
                m["pbrMetallicRoughness"]["baseColorTexture"]["index"]
                for m in obj.json["materials"]
            ],
            [0, 0],
        )
        self.assertEqual(
            image_content(obj, 0), (self.root / "stripes.png").read_bytes()
        )
        self.assertEqual(obj.json["images"][1]["mimeType"], "image/ktx2")
        ktx = self.root / "texture.ktx2"
        ktx.write_bytes(image_content(obj, 1))
        (row,) = report["basisu"]["images"]
        self.assertEqual(row["sha256"], hashlib.sha256(ktx.read_bytes()).hexdigest())
        self.assertEqual(
            row["levelSha256"],
            [
                hashlib.sha256((self.root / f).read_bytes()).hexdigest()
                for f in ("stripes.png", "stripes-2.png", "stripes-1.png")
            ],
        )
        self.assertEqual((report["fallbackImages"], report["images"]), (1, 2))
        subprocess.run(
            [TOOL, "validate", "--gltf-basisu", "--warnings-as-errors", ktx], check=True
        )
        for target in ("bc7", "rgba8"):
            path = self.root / f"{target}.ktx2"
            subprocess.run(
                [TOOL, "transcode", "--target", target, ktx, path], check=True
            )
            data = levels(path)
            if target == "bc7":
                self.assertEqual([len(level) for level in data], [16, 16, 16])
            else:
                self.assertEqual([len(level) for level in data], [64, 16, 4])
                for level, wanted in [
                    (1, bytes((8, 232, 224, 192)) * 4),
                    (2, bytes((240, 40, 8, 64))),
                ]:
                    self.assertLessEqual(
                        max(abs(x - y) for x, y in zip(data[level], wanted)), 3
                    )

    @unittest.skipUnless(
        TOOL and CITY, "set OSAGE_KTX_TOOL and OSAGE_CITY_BIN for a native GPU capture"
    )
    def test_native_viewer_uses_one_compressed_image_and_matches_the_core_fallback(
        self,
    ):
        manifest = fixture(self.root, unit_vertex_alpha=True, basisu=True)
        glb_options = {"object_id": "panel", "drawable": SOURCE, "lod_error": 0.01}
        source = self.root / "panel.glb"
        source.write_bytes(
            convert(**glb_options, appearance=manifest, ktx_tool=TOOL)[0]
        )
        core = self.root / "core.glb"
        manifest = fixture(self.root, unit_vertex_alpha=True)
        core.write_bytes(convert(**glb_options, appearance=manifest)[0])
        root = Path(__file__).resolve().parents[2]
        replay = json.loads(
            (root / "worlds/ville-paris/fixtures/bonnet.json").read_bytes()
        )
        route = replay["routes"][0]
        route.update(ticks=8, start_m=[-100, 4, -100], velocity_m_s=[0, 0, 0])
        route["bonnet"].update(center_m=[-100, 0, -100])
        route["camera"] = [
            {"tick": t, "eye": [0, 1, 2.8], "aim": [0, 1, 0]} for t in (0, 8)
        ]
        replay.update(viewport=[320, 180], vertical_fov_deg=42, routes=[route])
        path = self.root / "replay.json"
        path.write_text(json.dumps(replay))

        def render(tag, obj, disable):
            picture = self.root / (tag + ".png")
            process = subprocess.run(
                [
                    CITY,
                    "--scene",
                    obj,
                    "--replay",
                    path,
                    "--route",
                    route["id"],
                    "--physics",
                    "rapier",
                    "--actor",
                    "capsule",
                    "--shot",
                    picture,
                ],
                env={
                    **os.environ,
                    "OSAGE_TEXTURE_TRACE": "1",
                    "OSAGE_DISABLE_BC": str(int(disable)),
                },
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                check=True,
            )
            rows = {}
            for kind in ("worker", "resident"):
                marker = "embedded texture " + kind + " "
                rows[kind] = [
                    json.loads(line.split(marker, 1)[1])
                    for line in process.stdout.splitlines()
                    if marker in line
                ]
                self.assertTrue(rows[kind], process.stdout)
            self.assertTrue(all("Async" in row["worker"] for row in rows["worker"]))
            return picture.read_bytes(), rows

        compressed, hw = render("hw", source, False)
        fallback, sw = render("fallback", source, True)
        control, baseline = render("core", core, False)
        self.assertEqual(fallback, control)
        self.assertFalse(sw["worker"][0]["adapterBc7"])
        self.assertEqual([x["payloadBytes"] for x in sw["resident"]], [84, 84])
        self.assertEqual(
            [x["dataSha256"] for x in sw["worker"]],
            [x["dataSha256"] for x in baseline["worker"]],
        )
        if hw["worker"][0]["adapterBc7"]:
            self.assertEqual(
                [x["format"] for x in hw["resident"]], ["Bc7RgbaUnormSrgb"]
            )
            self.assertEqual([x["payloadBytes"] for x in hw["resident"]], [48])
            self.assertEqual(hw["worker"][0]["materialSlots"], 2)
            self.assertNotEqual(compressed, fallback)
        else:
            self.assertEqual(compressed, control)

    @unittest.skipUnless(
        TOOL, "set OSAGE_KTX_TOOL for shared and distinct compression variants"
    )
    def test_a_shared_fallback_can_have_slots_with_distinct_or_missing_chains(self):
        root = ET.parse(SOURCE)
        root.findall("ShaderGroup/Shaders/Item")[1].find(
            "Parameters/Item/Name"
        ).text = "far"
        resource = self.root / "alias.ydr.xml"
        root.write(resource)
        spec = json.loads(self.manifest.read_bytes())
        spec["textures"]["far"] = "stripes.png"
        near = spec["basisuMips"]["panel_stripes"]
        for common in (None, True, False):
            if common is not None:
                name = "near-copy.png" if common else "far-2.png"
                data = (
                    (self.root / near[0]).read_bytes()
                    if common
                    else rgba_png(2, 2, bytes([240, 10, 80, 255]) * 4)
                )
                (self.root / name).write_bytes(data)
                spec["basisuMips"]["far"] = [name, near[1]]
            obj, _ = self.convert(spec, resource, ktx_tool=TOOL)
            tex = obj.json["textures"]
            slots = [
                tex[m["pbrMetallicRoughness"]["baseColorTexture"]["index"]]
                for m in obj.json["materials"]
            ]
            self.assertEqual([t["source"] for t in slots], [0, 0])
            self.assertIn(EXTENSION, slots[0]["extensions"])
            if common is None:
                self.assertNotIn("extensions", slots[1])
            else:
                self.assertEqual(slots[0] == slots[1], common)
            self.assertEqual(len(obj.json["images"]), 3 if common is False else 2)
            self.assertEqual(len(tex), 1 if common else 2)

    def test_invalid_or_incomplete_mips_and_source_protection(self):
        png = (self.root / "stripes.png").read_bytes()
        paths = [self.root / "stripes-2.png", self.root / "stripes-1.png"]
        for base, lower, pattern in (
            (png, paths[:1], "expected 2 lower"),
            (png, list(reversed(paths)), "2x2 PNG"),
            (png, [self.root / "missing.png", paths[1]], "mip 1"),
            (rgba_png(3, 4, bytes(48)), paths, "multiples of four"),
        ):
            with (
                self.subTest(pattern=pattern),
                self.assertRaisesRegex(ValueError, pattern),
            ):
                prepared_levels(base, lower, "panel")
        with self.assertRaisesRegex(ValueError, "separate output"):
            self.convert(output_path=paths[0])
        with self.assertRaisesRegex(ValueError, "--ktx-tool"):
            self.convert(ktx_tool=self.root / "nonexistent")
        spec = json.loads(self.manifest.read_bytes())
        for bad in (
            {"unknown": []},
            {"PANEL_STRIPES": [], "panel_stripes": []},
            {"panel_stripes": "bad"},
            [],
        ):
            with (
                self.subTest(bad=bad),
                self.assertRaisesRegex(ValueError, "basisuMips"),
            ):
                self.convert({**spec, "basisuMips": bad})

    def test_no_requested_compression_needs_no_tool_and_keeps_original_glb(self):
        fixture(self.root)
        a, _ = self.convert()
        b, _ = self.convert(ktx_tool=self.root / "absent")
        self.assertEqual(a.json, b.json)
        self.assertEqual(a.binary, b.binary)
        self.assertNotIn("extensionsUsed", a.json)
        self.assertEqual(len(a.json["images"]), 1)


if __name__ == "__main__":
    unittest.main()
