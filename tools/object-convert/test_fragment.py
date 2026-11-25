# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

"""Articulated interchange, including the independent legacy binary bridge when installed."""

import copy
import hashlib
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
from fragment_bridge import transcode
from test_convert import FIX, MATERIALS, Packed

SOURCE = FIX / "pedestrian.yft.xml"
CORE = os.environ.get("OSAGE_FRAGMENT_CORE")
CITY = os.environ.get("OSAGE_CITY_BIN")


def object_from(path=SOURCE, **options):
    return Packed(convert(object_id="pedestrian", fragment=path, **options)[0])


class FragmentTests(unittest.TestCase):
    def test_body_to_joint_rows_recover_both_endpoints_and_the_three_rotation_limits(self):
        tree = ET.parse(SOURCE)
        for j in tree.findall("Physics/LOD1/ArticulatedBody/Joints/Item"):
            # Write the same fixture in a Z-twist coordinate system. Each new row
            # maps a body direction into the joint; translations stay body-local.
            for start in (2, 6):
                entries = [j.find(f"Unknown{i}0") for i in range(start, start + 3)]
                x, y, z = [[v.attrib[k] for k in "xyz"] for v in entries]
                for v, row in zip(entries, zip(y, z, x), strict=True):
                    v.attrib.update(zip("xyz", row, strict=True))
            if j.get("type") == "DOF3":
                limits = j.find("UnknownA0").attrib
                limits["x"], limits["y"] = limits["y"], limits["x"]
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "rows.yft.xml"
            tree.write(path, encoding="utf-8", xml_declaration=True)
            with self.assertRaisesRegex(ValueError, "frames disagree"):
                object_from(path)
            for space in ("bone", "actor"):
                data, report = convert(object_id="pedestrian", fragment=path, rig_space=space,
                                       joint_frames="body-to-joint-z")
                expected = object_from(rig_space=space).profile["articulation"]
                self.assertEqual(Packed(data).profile["articulation"], expected)
                source = report["fragmentPhysics"]
                self.assertEqual(source["jointFrameConvention"], "body-to-joint-z")
                self.assertEqual(len(source["jointSourceFrames"]), 14)
            if CORE:
                packed = Path(tmp) / "rows.yft"
                packed.write_bytes(transcode(path.read_bytes(), Path(CORE), encode=True))
                rig = object_from(packed, fragment_core=Path(CORE), joint_frames="body-to-joint-z")
                reference = Path(tmp) / "legacy.yft"
                reference.write_bytes(transcode(SOURCE.read_bytes(), Path(CORE), encode=True))
                self.assertEqual(rig.profile["articulation"],
                                 object_from(reference, fragment_core=Path(CORE)).profile["articulation"])

    def test_swing_envelope_is_explicit_and_only_applies_to_two_free_swing_axes(self):
        with tempfile.TemporaryDirectory() as tmp:
            tree = ET.parse(SOURCE)
            # Lock one lean on the first three-axis joint.
            first = tree.find("Physics/LOD1/ArticulatedBody/Joints/Item[@type='DOF3']")
            first.find("UnknownA0").set("x", "0")
            path = Path(tmp) / "arm.xml"
            tree.write(path)
            default = object_from(path).profile["articulation"]["bodies"]
            marked = object_from(path, swing_envelope="angular-ellipse").profile["articulation"]["bodies"]
            count = 0
            for a, b in zip(default, marked, strict=True):
                if b["joint"]:
                    j = b["joint"]
                    if all(hi > 0 for _, hi in j["limitsRad"][1:]):
                        self.assertEqual(j.pop("swingEnvelope"), "angular-ellipse")
                        count += 1
                self.assertEqual(a, b)
            self.assertEqual(count, 9)

    def test_contact_sphere_radius_preserves_declared_extent_and_reports_its_choice(self):
        tree = ET.parse(SOURCE)
        spheres = tree.findall("Physics/LOD1/Archetype/Bounds/Children/Item[@type='Sphere']")
        for child in spheres:
            child.find("Margin").set("value", child.find("SphereRadius").get("value"))
        sphere = spheres[0]
        extent = float(sphere.find("SphereRadius").get("value"))
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "sphere.xml"
            for radius in (0, extent / 2, extent):
                sphere.find("Margin").set("value", str(radius))
                tree.write(path)
                old = object_from(path).profile["articulation"]["bodies"]
                self.assertIn({"type": "sphere", "radiusM": extent}, [b["shape"] for b in old])
                if radius == 0:
                    with self.assertRaisesRegex(ValueError, "positive sphere Margin"):
                        object_from(path, sphere_radius="margin")
                    continue
                packed, report = convert(object_id="pedestrian", fragment=path, sphere_radius="margin")
                self.assertTrue(any(e["marginM"] == radius and e["declaredRadiusM"] == extent and
                                    e["usedRadiusM"] == radius for e in report["sphereRadii"]))
                bodies = Packed(packed).profile["articulation"]["bodies"]
                self.assertIn({"type": "sphere", "radiusM": radius}, [b["shape"] for b in bodies])

    def test_source_rebuild_matches_and_keeps_separate_bind_trees(self):
        with tempfile.TemporaryDirectory() as tmp:
            result = Path(tmp) / "original.xml"
            subprocess.run([sys.executable, FIX / "make_pedestrian.py", result], check=True)
            self.assertEqual(SOURCE.read_bytes(), result.read_bytes())
        obj = object_from()
        legacy = object_from(rig_space="actor").profile["articulation"]
        self.assertNotIn("space", legacy)
        self.assertEqual(obj.profile["articulation"]["space"], "bone")
        nodes = obj.json["nodes"]
        bodies = obj.profile["articulation"]["bodies"]
        self.assertEqual((len(nodes), len(bodies), len(obj.json["meshes"])), (16, 15, 15))
        self.assertEqual(sum(b["massKg"] for b in bodies), 75)
        self.assertEqual({b["bone"] for b in bodies}, {n["extras"]["osage"]["id"] for n in nodes[1:]})
        self.assertEqual(sum(1 for b in bodies if b["shape"]["type"] == "sphere"), 2)
        self.assertEqual([b["parent"] for b in bodies], [None, 0, 1, 1, 3, 4, 0, 6, 7, 1, 9, 10, 0, 12, 13])
        self.assertEqual(sum(1 for b in bodies if b["joint"] and b["joint"]["kind"] == "hinge"), 4)
        self.assertEqual([b["name"] for b in bodies if b["name"].startswith("foot_")], ["foot_l", "foot_r"])
        for b, n in zip(bodies, nodes[1:]):
            self.assertEqual(b["name"], n["name"])
            self.assertEqual(len(obj.mesh(n["mesh"])[1]), 24)
        spine = bodies[1]
        self.assertAlmostEqual(spine["shape"]["radiusM"], 0.145)
        self.assertAlmostEqual(spine["shape"]["halfSegmentM"], math.hypot(0.44, 0.03) / 2 - 0.145)
        for got, expected in zip(spine["centerM"], (0, 0.22, -0.015)):
            self.assertAlmostEqual(got, expected)
        elbow = bodies[4]["joint"]
        self.assertEqual(elbow["kind"], "hinge")
        self.assertAlmostEqual(elbow["limitsRad"][0][0], math.radians(-130))
        self.assertAlmostEqual(elbow["limitsRad"][0][1], math.radians(2))
        self.assertNotIn("anchorM", elbow)
        self.assertEqual(set(elbow["parentFrame"]), {"anchorM", "rotationXyzw"})
        self.assertEqual(set(elbow["childFrame"]), {"anchorM", "rotationXyzw"})
        self.assertAlmostEqual(abs(legacy["bodies"][4]["joint"]["frameXyzw"][0]), 0.5)
        self.assertAlmostEqual(legacy["bodies"][1]["centerM"][1], 1.30)
        self.assertEqual(
            obj.profile["collisionMaterials"], {"surface-7": {"friction": 0.5, "restitution": 0}}
        )
        self.assertEqual(
            object_from(materials=MATERIALS).profile["collisionMaterials"]["surface-7"]["friction"], 0.72
        )

    def test_rotated_source_bone_keeps_the_physical_frame(self):
        tree = ET.parse(SOURCE)
        pelvis = tree.find("Drawable/Skeleton/Bones/Item/Rotation")
        q = math.sqrt(0.5)
        pelvis.set("z", str(q))
        pelvis.set("w", str(q))
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "rotated.xml"
            tree.write(path)
            local = object_from(path)
            old = object_from(path, rig_space="actor")
        body = local.profile["articulation"]["bodies"][0]
        actor = old.profile["articulation"]["bodies"][0]
        # The source pelvis now points 90° around the destination's vertical axis.
        for actual, expected in zip(body["centerM"], (-0.015, 0.12, 0), strict=True):
            self.assertAlmostEqual(actual, expected)
        x, y, z, w = body["rotationXyzw"]
        in_actor = [q * (x + z), q * (y + w), q * (z - x), q * (w - y)]
        ref = actor["rotationXyzw"]
        self.assertLess(min(math.dist(in_actor, ref), math.dist(in_actor, [-v for v in ref])), 1e-6)
        self.assertEqual(actor, object_from(rig_space="actor").profile["articulation"]["bodies"][0])

    def test_rejects_disagreement_in_shape_joints_or_redundant_articulation_tables(self):
        cases = [
            (
                "Physics/LOD1/Archetype/Bounds/Children/Item[@type='Capsule']/Margin",
                "value",
                "0",
                "invalid Y-axis capsule",
            ),
            ("Physics/LOD1/ArticulatedBody/ItemFlags", None, "0 " * 22, "inconsistent source joint"),
            ("Physics/LOD1/ArticulatedBody/UnknownVectors", None, "1, 2, 3, 4", "every articulated link"),
            ("Physics/LOD1/Children/Item/PristineMass", "value", "12", "inconsistent source link mass"),
            ("Physics/LOD1/ArticulatedBody/Joints/Item/Unknown50", "x", "0.25", "frames disagree"),
            ("Drawable/DrawableModelsHigh/Item/HasSkin", "value", "1", "rigid model"),
            ("Drawable/Skeleton/Bones/Item/ParentIndex", "value", "0", "parent-first bones"),
            ("Drawable/Skeleton/Bones/Item/Scale", "x", "2", "orthonormal right-handed"),
        ]
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "invalid.xml"
            for query, key, value, msg in cases:
                with self.subTest(query=query):
                    tree = ET.parse(SOURCE)
                    item = tree.find(query)
                    if key:
                        item.set(key, value)
                    else:
                        item.text = value
                    tree.write(path)
                    with self.assertRaisesRegex(ValueError, msg):
                        object_from(path)

    @unittest.skipUnless(
        CORE, "set OSAGE_FRAGMENT_CORE and run with Python.NET for the independent binary round trip"
    )
    def test_native_rsc7_preserves_every_body_joint_visual_and_link(self):
        assembly = Path(CORE)
        original = object_from()
        with tempfile.TemporaryDirectory() as tmp:
            file = Path(tmp) / "pedestrian.yft"
            data = transcode(SOURCE.read_bytes(), assembly, encode=True)
            self.assertEqual(data[:8], b"RSC7\xa2\0\0\0")
            file.write_bytes(data)
            decoded = object_from(file, fragment_core=assembly)
            tag = ET.fromstring(transcode(data, assembly, encode=False)).find("Physics/LOD1/ArticulatedBody")
            self.assertEqual(len(tag.findtext("UnknownVectors").strip().splitlines()), 15)
            for lookup in ("ItemIndices", "ItemFlags"):
                self.assertEqual(len(tag.findtext(lookup).split()), 22)
                self.assertEqual(
                    tag.findtext(lookup).split(),
                    ET.parse(SOURCE).findtext("Physics/LOD1/ArticulatedBody/" + lookup).split(),
                )

        def same(a, b):
            if isinstance(a, float):
                self.assertAlmostEqual(a, b, places=5)
            elif isinstance(a, list):
                self.assertEqual(len(a), len(b))
                for x, y in zip(a, b):
                    same(x, y)
            elif isinstance(a, dict):
                self.assertEqual(a.keys(), b.keys())
                for k in a:
                    same(a[k], b[k])
            else:
                self.assertEqual(a, b)

        same(original.profile, decoded.profile)
        same(original.json["nodes"], decoded.json["nodes"])
        for i in range(15):
            va, ia = original.mesh(i)
            vb, ib = decoded.mesh(i)
            self.assertEqual(ia, ib)
            for attr in ("POSITION", "NORMAL"):
                for a, b in zip(va[attr], vb[attr], strict=True):
                    self.assertLess(math.dist(a, b), 2e-6)

    @unittest.skipUnless(CITY, "set OSAGE_CITY_BIN to check the two native solvers")
    def test_real_city_routes_consume_and_replay_the_imported_actor(self):
        payload = convert(object_id="pedestrian", fragment=SOURCE)[0]
        replay = FIX.parents[2] / "worlds/ville-paris/fixtures/bonnet.json"
        with tempfile.TemporaryDirectory() as tmp:
            glb = Path(tmp) / "actor.glb"
            glb.write_bytes(payload)
            legacy = Path(tmp) / "legacy.glb"
            legacy.write_bytes(convert(object_id="pedestrian", fragment=SOURCE, rig_space="actor")[0])
            for engine in ("rapier", "jolt"):
                out = Path(tmp) / engine
                args = [CITY, "--actor-rig", glb, "--replay", replay, "--physics", engine]
                subprocess.run([*args, "--bench", out], check=True, capture_output=True)
                subprocess.run([*args, "--verify", out], check=True, capture_output=True)
                old_out = Path(tmp) / ("legacy-" + engine)
                subprocess.run(
                    [
                        CITY,
                        "--actor-rig",
                        legacy,
                        "--replay",
                        replay,
                        "--physics",
                        engine,
                        "--bench",
                        old_out,
                    ],
                    check=True,
                    capture_output=True,
                )
                self.assertEqual((out / "dynamics.csv").read_bytes(), (old_out / "dynamics.csv").read_bytes())
                run = json.loads((out / "dynamics.json").read_bytes())["run"]
                self.assertEqual((run["actor"], len(run["parts"]), run["trace_rows"]), ("imported", 15, 482))
                self.assertEqual(run["inputs"]["actor_sha256"], hashlib.sha256(payload).hexdigest())
                self.assertAlmostEqual(sum(b["mass_kg"] for b in run["parts"]), 75.0, places=5)
                self.assertAlmostEqual(run["parts"][1]["radius_m"], 0.145)
                self.assertEqual({r["id"] for r in run["outcomes"]}, {"parked", "moving"})
                for r in run["outcomes"]:
                    first = r["first_contacts"][0]
                    self.assertTrue(r["target_contacted"])
                    self.assertEqual(
                        (first["surface"], first["tick"], set(first["parts"])),
                        ("bonnet", 24, {"foot_l", "foot_r"}),
                    )

    @unittest.skipUnless(CITY, "set OSAGE_CITY_BIN to compare mounted rigid actors")
    def test_two_nested_placements_carry_visuals_bodies_and_both_joint_frames(self):
        def write_doc(path, doc, packed):
            data = json.dumps(doc, separators=(",", ":")).encode()
            data += b" " * (-len(data) % 4)
            path.write_bytes(
                struct.pack("<4sIII4s", b"glTF", 2, 28 + len(data) + len(packed.binary), len(data), b"JSON")
                + data
                + struct.pack("<I4s", len(packed.binary), b"BIN\0")
                + packed.binary
            )

        def placement(id, child, t, q):
            return {
                "name": id,
                "children": [child],
                "translation": t,
                "rotation": q,
                "extras": {"osage": {"id": id, "collision": [], "acoustic": []}},
            }

        def inspect(path, *options):
            args = [CITY, "--scene", path, "--inspect-object", *options]
            return json.loads(subprocess.run(args, check=True, capture_output=True).stdout)

        def close_vec(a, b):
            self.assertLess(math.dist(a, b), 5e-6)

        def same_rotation(a, b):
            self.assertLess(min(math.dist(a, b), math.dist(a, [-x for x in b])), 5e-6)

        def move(pos):
            x, y, z = pos
            return [2 - y, 4 + z, 6 - x]

        def rotate(a, b):
            x, y, z, w = a
            i, j, k, s = b
            return [
                w * i + x * s + y * k - z * j,
                w * j - x * k + y * s + z * i,
                w * k + x * j - y * i + z * s,
                w * s - x * i - y * j - z * k,
            ]

        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            packed = object_from()
            legacy = object_from(rig_space="actor")
            old, current, mounted = (directory / n for n in ("actor.glb", "bone.glb", "mounted.glb"))
            write_doc(old, legacy.json, legacy)
            write_doc(current, packed.json, packed)
            old_rig = inspect(old)["resolvedRigBodies"]
            baseline = inspect(current)["resolvedRigBodies"]
            doc = copy.deepcopy(packed.json)
            first = len(doc["nodes"])
            q = 2**-0.5
            rot_y, rot_z = [0, q, 0, q], [0, 0, q, q]
            doc["nodes"].append(placement("inner", 0, [1, 0, 2], rot_y))
            doc["nodes"].append(placement("mount", first, [2, 3, 4], rot_z))
            doc["scenes"][0]["nodes"] = [first + 1]
            write_doc(mounted, doc, packed)
            assembled = inspect(mounted)["resolvedRigBodies"]
            as_matrix = copy.deepcopy(doc)
            # Equivalent glTF matrix and TRS authoring of the same two rigid placements.
            for node, axes in zip(
                as_matrix["nodes"][-2:],
                ((0, 0, -1, 0, 1, 0, 1, 0, 0), (0, 1, 0, -1, 0, 0, 0, 0, 1)),
                strict=True,
            ):
                tr = node.pop("translation")
                node.pop("rotation")
                node["matrix"] = [*axes[:3], 0, *axes[3:6], 0, *axes[6:], 0, *tr, 1]
            matrix_path = directory / "matrix.glb"
            write_doc(matrix_path, as_matrix, packed)
            matrix_rig = inspect(matrix_path)["resolvedRigBodies"]
            for a, b in zip(assembled, matrix_rig, strict=True):
                close_vec(a["centerM"], b["centerM"])
                same_rotation(a["rotationXyzw"], b["rotationXyzw"])
                if a["joint"]:
                    close_vec(a["joint"]["anchorM"], b["joint"]["anchorM"])
                    same_rotation(a["joint"]["rotationXyzw"], b["joint"]["rotationXyzw"])
            orientation = rotate(rot_z, rot_y)
            for before, after, ancient in zip(baseline, assembled, old_rig, strict=True):
                self.assertEqual(after["name"], before["name"])
                close_vec(before["centerM"], ancient["centerM"])
                same_rotation(before["rotationXyzw"], ancient["rotationXyzw"])
                close_vec(move(before["centerM"]), after["centerM"])
                same_rotation(rotate(orientation, before["rotationXyzw"]), after["rotationXyzw"])
                if before["joint"]:
                    close_vec(ancient["joint"]["anchorM"], before["joint"]["anchorM"])
                    same_rotation(ancient["joint"]["rotationXyzw"], before["joint"]["rotationXyzw"])
                    close_vec(move(before["joint"]["anchorM"]), after["joint"]["anchorM"])
                    same_rotation(
                        rotate(orientation, before["joint"]["rotationXyzw"]), after["joint"]["rotationXyzw"]
                    )
            # Cast the same segment through the head before and after the mount.
            a, b = [0, 1.655, 1], [0, 1.655, -1]
            line = lambda points: [",".join(map(str, v)) for v in points]
            original_hits = inspect(current, "--ray", *line([a, b]))["ray"]["visual"]
            mounted_hits = inspect(mounted, "--ray", *line([move(a), move(b)]))["ray"]["visual"]
            self.assertEqual([x["id"] for x in original_hits], ["pedestrian/bone/1002"])
            self.assertEqual([x["id"] for x in original_hits], [x["id"] for x in mounted_hits])
            self.assertAlmostEqual(original_hits[0]["distanceM"], mounted_hits[0]["distanceM"], places=5)
            for engine in ("rapier", "jolt"):
                out = directory / engine
                subprocess.run(
                    [
                        CITY,
                        "--actor-rig",
                        mounted,
                        "--replay",
                        FIX.parents[2] / "worlds/ville-paris/fixtures/bonnet.json",
                        "--physics",
                        engine,
                        "--bench",
                        out,
                    ],
                    check=True,
                    capture_output=True,
                )
                parts = json.loads((out / "dynamics.json").read_bytes())["run"]["parts"]
                for part, resolved in zip(parts, assembled, strict=True):
                    close_vec(part["center_m"], resolved["centerM"])
                    same_rotation(part["rotation_xyzw"], resolved["rotationXyzw"])
                    if resolved["joint"]:
                        close_vec(part["anchor_m"], resolved["joint"]["anchorM"])
            # Reject a detached endpoint and an assembly that would shear the analytic capsule.
            bad_doc = copy.deepcopy(doc)
            joint = bad_doc["asset"]["extras"]["osageObject"]["articulation"]["bodies"][1]["joint"]
            joint["childFrame"]["anchorM"][0] += 0.02
            for variant, warning in ((bad_doc, "endpoints disagree"), (doc, "rigid right-handed")):
                if variant is doc:
                    variant["nodes"][-1]["scale"] = [1, 2, 1]
                write_doc(mounted, variant, packed)
                result = subprocess.run(
                    [CITY, "--scene", mounted, "--inspect-object"],
                    check=False,
                    capture_output=True,
                    text=True,
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(warning, result.stderr)
