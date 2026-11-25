# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Exercise face identities across mirrored, divided, coplanar and duplicate triangles."""

import copy
import json
import runpy
import struct
import tempfile
import unittest
from pathlib import Path

from paging_support_storage import unpack_support
from paging_collision_codec import read_collision
from paging_face_order import face_runs, support_order, validate_face_order

HERE = Path(__file__).resolve().parent
TOPOLOGY = runpy.run_path(str(HERE / "check-collision-topology.py"))
COOK = TOPOLOGY["AUDIO"]["COOK"]
GLB = TOPOLOGY["AUDIO"]["EDGE"]["common"]
FACE = struct.Struct("<3H")


def mixed_ramp(out):
    """Give the authored slab two primitives, two identical faces, omitted sides and a reflected placement."""
    out.mkdir(parents=True, exist_ok=True)
    path, _, expected, _, _ = TOPOLOGY["prepare"](out)
    doc, raw = GLB["read_glb"](path)
    chunk = bytearray(raw)
    mesh = doc["nodes"][-1]["extras"]["osage"]["collision"][0]["mesh"]
    primitive = doc["meshes"][mesh]["primitives"][0]
    index = primitive["indices"]
    accessor = doc["accessors"][index]
    view = doc["bufferViews"][accessor["bufferView"]]
    offset = 8 + view["byteOffset"] + accessor.get("byteOffset", 0)
    expected[1] = expected[0]
    FACE.pack_into(chunk, offset + FACE.size, *FACE.unpack_from(chunk, offset))
    for i, (a, b, c) in enumerate(expected):
        if 64 <= i < 80 or i % 5 in (2, 4):
            u, v, w = FACE.unpack_from(chunk, offset + i * FACE.size)
            FACE.pack_into(chunk, offset + i * FACE.size, u, w, v)
            expected[i] = (a, c, b)
    # The second primitive begins in the middle of a cell and uses the same position accessor.
    cut = 37
    other = dict(
        accessor, byteOffset=cut * FACE.size, count=accessor["count"] - 3 * cut
    )
    accessor["count"] = 3 * cut
    doc["meshes"][mesh]["primitives"].append(
        dict(primitive, indices=len(doc["accessors"]))
    )
    doc["accessors"].append(other)
    path = out / "mixed-ramp.glb"
    GLB["write_glb"](path, doc, chunk)
    return path, expected


def expand(runs):
    return [i for first, count in runs for i in range(first, first + count)]


class FaceOrder(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.temporary.cleanup)
        root = Path(cls.temporary.name)
        source, cls.expected = mixed_ramp(root)
        cls.pages = root / "district"
        COOK(source, cls.pages, 32)
        cls.index = json.loads((cls.pages / "index.json").read_bytes())
        cls.catalogue = json.loads(
            unpack_support((cls.pages / cls.index["support"]["file"]).read_bytes())
        )

    def test_all_source_triangles_and_certified_subsets(self):
        self.assertTrue(validate_face_order(self.index, self.catalogue))
        self.assertEqual(self.expected[0], self.expected[1])
        layout = self.catalogue["face_order"]
        exceptions = {(page, slot): runs for page, slot, runs in layout["exceptions"]}
        self.assertTrue(any(len(runs) > 1 for runs in exceptions.values()))
        shapes_by_page = [t["physics"] for t in self.index["tiles"] if "physics" in t]
        seen = []
        supports = []
        for pi, layer in enumerate(shapes_by_page):
            shapes = read_collision(self.pages, layer)
            for slot, total, runs in layer.get("source_face_runs", []):
                self.assertEqual(total, len(self.expected))
                self.assertEqual(
                    (layer["ids"][slot], shapes[slot]["id"]), ("ramp", "ramp")
                )
                ids = expand(runs)
                seen.extend(ids)
                body = shapes[slot]
                faces = [
                    tuple(tuple(body["vertices"][i]) for i in face)
                    for face in body["triangles"]
                ]
                self.assertEqual(faces, [self.expected[i] for i in ids])
                node, positions, certified = self.catalogue["pages"][pi][slot]
                self.assertEqual(node, layer["source_nodes"][slot])
                # For this source all faces lie in the horizontal plane.
                expected_subset = [
                    i
                    for i, (a, b, c) in enumerate(faces)
                    if (b[2] - a[2]) * (c[0] - a[0]) - (b[0] - a[0]) * (c[2] - a[2]) > 0
                ]
                if not certified or len(certified) == len(faces):
                    self.assertNotIn((pi, slot), exceptions)
                    local = list(range(len(certified)))
                elif (pi, slot) in exceptions:
                    local = expand(exceptions[pi, slot])
                else:
                    local = list(
                        range(layout["start"], layout["start"] + len(certified))
                    )
                self.assertEqual(local, expected_subset)
                native = [
                    tuple(tuple(positions[i]) for i in face) for face in certified
                ]
                self.assertEqual(native, [self.expected[ids[j]] for j in local])
                supports.append(len(certified))
        self.assertEqual(sorted(seen), list(range(81)))
        self.assertIn(0, supports)
        self.assertTrue(any(supports))

    def test_global_coverage_and_slot_counts(self):
        for defect in (
            "gap",
            "duplicate",
            "order",
            "count",
            "nonmaximal",
            "outside",
            "wrong_version",
        ):
            with self.subTest(defect=defect):
                idx = copy.deepcopy(self.index)
                layer = next(
                    t["physics"]
                    for t in idx["tiles"]
                    if t.get("physics", {}).get("source_face_runs")
                )
                row = layer["source_face_runs"][0]
                first, n = row[2][0]
                if defect == "gap":
                    row[2][0][0] -= 1
                elif defect == "duplicate":
                    layer["source_face_runs"].append(copy.deepcopy(row))
                elif defect == "order":
                    row[2].append([first, 1])
                elif defect == "count":
                    row[1] += 1
                elif defect == "nonmaximal":
                    row[2][:1] = [[first, 2], [first + 2, n - 2]]
                elif defect == "outside":
                    row[2][0][0] = row[1]
                else:
                    idx["collision_face_order"] = True
                with self.assertRaises(ValueError):
                    validate_face_order(idx, self.catalogue)

    def test_support_subsets_checked_before_publication(self):
        for defect in (
            "length",
            "invalid_default",
            "duplicate_key",
            "missing_page",
            "whole",
            "empty",
        ):
            with self.subTest(defect=defect):
                cat = copy.deepcopy(self.catalogue)
                layout = cat["face_order"]
                overrides = layout["exceptions"]
                if defect == "length":
                    overrides[0][2][0][1] += 2
                elif defect == "invalid_default":
                    layout["start"] = -1
                elif defect == "duplicate_key":
                    overrides.insert(0, copy.deepcopy(overrides[0]))
                elif defect == "missing_page":
                    overrides.append([999999, 0, [[0, 1]]])
                else:
                    for page, proxies in enumerate(cat["pages"]):
                        for slot, (_, _, tris) in enumerate(proxies):
                            rest = sum(
                                row[2]
                                for row, owner in zip(
                                    cat["residual"][page],
                                    cat["residual_shapes"][page],
                                    strict=True,
                                )
                                if owner == slot
                            )
                            if (defect == "empty" and not tris) or (
                                defect == "whole" and tris and not rest
                            ):
                                overrides.append([page, slot, [[0, max(1, len(tris))]]])
                                overrides.sort()
                                break
                        else:
                            continue
                        break
                    else:
                        self.fail("missing control shape")
                with self.assertRaises(ValueError):
                    validate_face_order(self.index, cat)

    def test_canonical_intervals_and_legacy_index(self):
        self.assertEqual(face_runs([2, 3, 7, 11, 12]), [[2, 2], [7, 1], [11, 2]])
        for bad in ([3, 3], [7, 6], [False], [-1], [2**32]):
            with self.assertRaises(ValueError):
                face_runs(bad)
        self.assertEqual(
            support_order([[0, 0, [[5, 2]]], [1, 0, [[3, 1]]]]),
            {"start": 3, "exceptions": [[0, 0, [[5, 2]]]]},
        )
        self.assertFalse(validate_face_order({"tiles": []}, {}))


if __name__ == "__main__":
    unittest.main()
