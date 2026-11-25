# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

"""Independent GLB reader and authored source-to-object regression fixtures."""

import copy
import json
import math
import os
import struct
import subprocess
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

from convert import convert

ROOT = Path(__file__).resolve().parent
FIX = ROOT / "fixtures"
DRAWABLE = FIX / "slab.ydr.xml"
BOUNDS = FIX / "slab.ybn.xml"
MATERIALS = FIX / "surfaces.toml"
TOOL = os.environ.get("OSAGE_RESOURCE_TOOL")


class Packed:
    def __init__(self, content):
        magic, version, size = struct.unpack_from("<4sII", content)
        if (magic, version, size) != (b"glTF", 2, len(content)):
            raise ValueError("wrong container")
        size, tag = struct.unpack_from("<I4s", content, 12)
        if tag != b"JSON" or size % 4:
            raise ValueError("wrong JSON chunk")
        self.json = json.loads(content[20:20 + size])
        tail = 28 + size
        binary_size, tag = struct.unpack_from("<I4s", content, 20 + size)
        if tag != b"BIN\0" or binary_size != len(content) - tail or binary_size % 4:
            raise ValueError("wrong BIN chunk")
        self.binary = content[tail:]
        self.node = self.json["nodes"][0]
        self.roles = self.node["extras"]["osage"]
        self.profile = self.json["asset"]["extras"]["osageObject"]

    def read(self, accessor):
        acc = self.json["accessors"][accessor]
        view = self.json["bufferViews"][acc["bufferView"]]
        data = self.binary[view["byteOffset"]:view["byteOffset"] + view["byteLength"]]
        code = {5126: "f", 5125: "I", 5121: "B"}[acc["componentType"]]
        width = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}[acc["type"]]
        rows = list(struct.iter_unpack("<" + code * width, data))
        if len(rows) != acc["count"] or view["byteOffset"] % 4 != 0:
            raise ValueError("inconsistent accessor")
        return rows

    def mesh(self, index):
        part, = self.json["meshes"][index]["primitives"]
        attrs = {name: self.read(ref) for name, ref in part["attributes"].items()}
        return attrs, [i for (i,) in self.read(part["indices"])]


class ConvertTest(unittest.TestCase):
    def fixture(self, **options):
        base = {"drawable": DRAWABLE, "bounds": BOUNDS}
        base.update(options)
        return Packed(convert(object_id="slab", **base)[0])

    def test_visible_and_hidden_positions_and_material_origin(self):
        obj = self.fixture(acoustic_proxy=True)
        self.assertEqual(obj.json["scenes"][0]["nodes"], [0])
        self.assertEqual(len(obj.json["nodes"]), 1)
        source, indices = obj.mesh(obj.node["mesh"])
        self.assertEqual(indices, [0, 1, 2, 0, 2, 3])
        self.assertEqual(source["POSITION"], [(-1, 1.25, 1), (1, 1.25, 1), (1, 1.25, -1), (-1, 1.25, -1)])
        self.assertEqual(source["TEXCOORD_0"][2], (1, 1))
        self.assertEqual(source["COLOR_0"][0], (192, 128, 64, 255))
        self.assertEqual(source["NORMAL"], [(0, 1, 0)] * 4)
        low, = obj.roles["visualLods"]
        self.assertEqual(low["level"], "low")
        self.assertEqual(obj.mesh(low["mesh"])[1], [0, 1, 2])
        collision, = obj.roles["collision"]
        acoustic, = obj.roles["acoustic"]
        self.assertEqual(collision["id"], "slab/bounds/0/m0")
        self.assertEqual(collision["material"], "surface-7")
        self.assertEqual(acoustic, {**collision, "source": "collision-proxy"})
        self.assertNotIn(collision["mesh"], (obj.node["mesh"], low["mesh"]))
        self.assertEqual(obj.mesh(collision["mesh"])[0]["POSITION"][0], (-1, 1, 1))
        self.assertEqual(obj.profile["collisionMaterials"], {"surface-7": {"friction": 0.5, "restitution": 0}})
        self.assertEqual(obj.profile["acousticMaterials"], {"surface-7": {}})
        self.assertEqual(obj.json["asset"]["extras"]["osageImport"]["materialOrigins"]["surface-7"]["collision"], "Osage defaults")

    def test_all_four_drawable_levels_use_their_separate_distance_names(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "levels.xml"
            xml = ET.parse(DRAWABLE)
            low = xml.find("DrawableModelsLow")
            for name in ("Medium", "VeryLow"):
                level = copy.deepcopy(low)
                level.tag = "DrawableModels" + name
                xml.getroot().append(level)
            xml.write(path)
            obj = self.fixture(drawable=path)
            self.assertEqual([r["level"] for r in obj.roles["visualLods"]], ["medium", "low", "veryLow"])
            self.assertEqual(obj.json["asset"]["extras"]["osageImport"]["levelDistances"]["medium"], 70)

    def test_sound_is_optional_and_can_have_its_own_geometry(self):
        self.assertEqual(self.fixture().roles["acoustic"], [])
        obj = self.fixture(materials=MATERIALS, acoustic_bounds=FIX / "sound.ybn.xml")
        acoustic, = obj.roles["acoustic"]
        self.assertEqual(acoustic["source"], "authored")
        self.assertEqual(acoustic["id"], "slab/acoustic/m0")
        self.assertNotEqual(acoustic["mesh"], obj.roles["collision"][0]["mesh"])
        self.assertEqual(obj.mesh(acoustic["mesh"])[0]["POSITION"][0], (-.75, .75, .75))
        self.assertEqual(obj.profile["acousticMaterials"]["surface-7"]["transmission"], [.16, .09, .04])
        self.assertEqual(obj.profile["collisionMaterials"]["surface-7"]["restitution"], .08)

    def test_embedded_and_external_bounds_precedence(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "embedded.xml"
            drawable = ET.parse(DRAWABLE)
            drawable.getroot().append(ET.parse(FIX / "sound.ybn.xml").getroot().find("Bounds"))
            drawable.write(path)
            embedded = self.fixture(drawable=path, bounds=None)
            self.assertEqual(embedded.roles["collision"][0]["id"], "slab/bounds/m0")
            explicit = self.fixture(drawable=path)
            self.assertEqual(explicit.roles["collision"][0]["id"], "slab/bounds/0/m0")
            self.assertTrue(any("precedence" in note for note in explicit.json["asset"]["extras"]["osageImport"]["notes"]))

    def test_parent_rotation_and_reflected_local_transform(self):
        with tempfile.TemporaryDirectory() as tmp:
            xml = ET.parse(BOUNDS)
            outer = xml.find("Bounds")
            ET.SubElement(outer, "CompositeTransform").text = "0 1 0 0  -1 0 0 0  0 0 1 0   10 0 0 1"
            child = outer.find("Children/Item")
            child.find("CompositeTransform").text = "-1 0 0 0   0 1 0 0   0 0 1 0   1 0 0.25 1"
            file = Path(tmp) / "oblique.xml"
            xml.write(file)
            obj = self.fixture(drawable=None, bounds=file)
            positions, _ = obj.mesh(obj.roles["collision"][0]["mesh"])
            self.assertEqual(set(positions["POSITION"]), {(11, 1, -2), (11, 1, 0), (9, 1, 0), (9, 1, -2)})
            self.assertEqual(set(positions["NORMAL"]), {(0, 1, 0)})

    def test_materials_follow_polygon_slots_and_original_order(self):
        with tempfile.TemporaryDirectory() as tmp:
            xml = ET.parse(BOUNDS)
            node = xml.find("Bounds/Children/Item")
            second = ET.SubElement(node.find("Materials"), "Item")
            ET.SubElement(second, "Type", value="83")
            second_face = node.find("Polygons")[1]
            second_face.set("m", "1")
            second_face.set("f2", "1")
            path = Path(tmp) / "split.xml"
            xml.write(path)
            obj = self.fixture(bounds=path)
            self.assertEqual([s["material"] for s in obj.roles["collision"]], ["surface-7", "surface-83"])
            source, = obj.json["asset"]["extras"]["osageImport"]["bounds"]
            self.assertEqual(source["sourcePolygons"], {"0": [0], "1": [1]})
            self.assertEqual(source["triangleEdgeFlags"], {"1": [0, 1, 0]})

    def test_exact_oriented_box_faces(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "box.xml"
            path.write_text('<Bounds type="Box"><BoxMin x="-1" y="-2" z="-3"/><BoxMax x="1" y="2" z="3"/></Bounds>')
            obj = self.fixture(drawable=None, bounds=path)
            a, faces = obj.mesh(obj.roles["collision"][0]["mesh"])
            self.assertEqual(len(faces), 36)
            expect = {(x, y, z) for x in (-1, 1) for y in (-3, 3) for z in (-2, 2)}
            self.assertEqual(set(a["POSITION"]), expect)
            poly = ET.parse(BOUNDS)
            node = poly.find("Bounds/Children/Item")
            node.remove(node.find("CompositeTransform"))
            node.remove(node.find("GeometryCenter"))
            node.find("Vertices").text = "-1,-2,-3\n1,2,-3\n1,-2,3\n-1,2,3"
            polygons = node.find("Polygons")
            polygons.clear()
            ET.SubElement(polygons, "Box", m="0", v1="0", v2="1", v3="2", v4="3")
            poly.write(path)
            obj = self.fixture(drawable=None, bounds=path)
            a, faces = obj.mesh(obj.roles["collision"][0]["mesh"])
            p = a["POSITION"]
            self.assertEqual(set(p), expect)
            self.assertEqual(len(faces), 36)
            for i in range(0, len(faces), 3):
                a, b, c = [p[j] for j in faces[i:i+3]]
                u = tuple(y-x for x,y in zip(a,b))
                v = tuple(y-x for x,y in zip(a,c))
                normal = (u[1]*v[2]-u[2]*v[1], u[2]*v[0]-u[0]*v[2], u[0]*v[1]-u[1]*v[0])
                self.assertGreater(sum(n*x for n,x in zip(normal,a)), 0)

    def test_reject_corrupt_unsupported_or_ambiguous_data(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "bad.xml"
            for original, old, new, why in [
                (BOUNDS, 'v1="0"', 'v1="-1"', "vertex index"),
                (BOUNDS, 'm="0"', 'm="44"', "material index"),
                (BOUNDS, 'type="GeometryBVH"', 'type="Capsule"', "analytic bound"),
                (BOUNDS, '<Triangle ', '<Sphere ', "analytic polygon"),
                (BOUNDS, '0 0 0.25 1', '0 0 0.25 0', "affine"),
                (DRAWABLE, '<HasSkin value="0"', '<HasSkin value="1"', "skinned"),
                (DRAWABLE, '0 1 2 0 2 3', '0 1 7 0 2 3', "vertex index"),
                (DRAWABLE, '-1 -1 1.25', 'nan -1 1.25', "non-finite"),
            ]:
                with self.subTest(why=why):
                    path.write_text(original.read_text().replace(old, new, 1))
                    options = {"drawable": path} if original == DRAWABLE else {"bounds": path}
                    with self.assertRaisesRegex(ValueError, why):
                        self.fixture(**options)
            path = Path(tmp) / "bad.toml"
            path.write_text(MATERIALS.read_text().replace("0.16", "0.95"))
            with self.assertRaisesRegex(ValueError, "exceeds one"):
                self.fixture(materials=path)

    def test_atomic_output_and_reproducibility(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "slab.glb"
            cli = [sys.executable, str(ROOT / "convert.py"), "--id", "slab", "--bounds", str(BOUNDS),
                   "--acoustic-proxy", "-o", str(path)]
            subprocess.run(cli, check=True, capture_output=True)
            original = path.read_bytes()
            subprocess.run(cli, check=True, capture_output=True)
            self.assertEqual(original, path.read_bytes())
            failed = subprocess.run(cli + ["--drawable", str(BOUNDS)], check=False, capture_output=True)
            self.assertNotEqual(failed.returncode, 0)
            self.assertEqual(original, path.read_bytes())

    @unittest.skipUnless(TOOL, "set OSAGE_RESOURCE_TOOL to measure the binary point quantisation")
    def test_fractional_bound_quantisation(self):
        with tempfile.TemporaryDirectory() as tmp:
            xml = ET.parse(BOUNDS)
            root = xml.find("Bounds")
            root.find("BoxMax").set("z", "1.083")
            child = root.find("Children/Item")
            child.find("BoxMax").set("z", "0.833")
            child.find("GeometryCenter").set("z", "0.7915")
            child.find("Vertices").text = "-1,-1,-0.0415\n1,-1,-0.0415\n1,1,-0.0415\n-1,1,-0.0415\n0.137,0.249,0.0415"
            triangles = child.find("Polygons")
            triangles.clear()
            for i in range(4):
                ET.SubElement(triangles, "Triangle", m="0", v1=str(i), v2=str((i + 1) % 4), v3="4")
            source = Path(tmp) / "fraction.ybn.xml"
            binary = Path(tmp) / "fraction.ybn"
            xml.write(source)
            subprocess.run([TOOL, "resource", "build", "--strict", "--no-update-check", str(source), "-o", str(binary)],
                           check=True, capture_output=True)
            decoded = [self.fixture(drawable=None, bounds=p, resource_tool=TOOL) for p in (source, binary)]
            meshes = [obj.mesh(obj.roles["collision"][0]["mesh"]) for obj in decoded]
            self.assertEqual(meshes[0][1], meshes[1][1])
            error = max(math.dist(a, b) for a, b in zip(meshes[0][0]["POSITION"], meshes[1][0]["POSITION"], strict=True))
            self.assertGreater(error, 0)
            self.assertLess(error, 0.0001)

    @unittest.skipUnless(TOOL, "set OSAGE_RESOURCE_TOOL to the compatible binary for the loose-resource round trip")
    def test_binary_resource_compiler_and_decoder(self):
        with tempfile.TemporaryDirectory() as tmp:
            source_files = {}
            for role, xml in (("drawable", DRAWABLE), ("bounds", BOUNDS), ("acoustic_bounds", FIX / "sound.ybn.xml")):
                target = Path(tmp) / xml.stem
                subprocess.run([TOOL, "resource", "build", "--strict", "--no-update-check", str(xml), "-o", str(target)],
                               check=True, capture_output=True)
                self.assertEqual(target.read_bytes()[:4], b"RSC7")
                source_files[role] = target
            raw, _ = convert(object_id="slab", **source_files, materials=MATERIALS, resource_tool=TOOL)
            repeated, _ = convert(object_id="slab", **source_files, materials=MATERIALS, resource_tool=TOOL)
            self.assertEqual(raw, repeated)
            binary = Packed(raw)
            text = self.fixture(acoustic_bounds=FIX / "sound.ybn.xml", materials=MATERIALS)
            self.assertEqual(binary.roles, text.roles)
            self.assertEqual(binary.profile, text.profile)
            for mesh in range(len(text.json["meshes"])):
                self.assertEqual(binary.mesh(mesh), text.mesh(mesh))


if __name__ == "__main__":
    unittest.main()
