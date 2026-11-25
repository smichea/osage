# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

"""Read the static triangle and box subset of the public drawable/bounds XML."""

import math
import re
from collections import defaultdict
from itertools import product

from glb import cross, difference, unit


def number(text):
    v = float(text)
    if not math.isfinite(v):
        raise ValueError(f"non-finite number {text}")
    return v


def numbers(text):
    return [number(v) for v in (text or "").replace(",", " ").split()]


def vector(node, tag, default=None):
    found = node.find(tag)
    if found is None:
        if default is None:
            raise ValueError(f"missing {tag}")
        return default
    return tuple(number(found.attrib[axis]) for axis in "xyz")


def scalar(node, tag, default=None):
    found = node.find(tag)
    if found is None:
        if default is None:
            raise ValueError(f"missing {tag}")
        return default
    return found.attrib["value"]


def integer(value):
    return int(value, 16) if value.lower().startswith("0x") else int(value)


def y_up(v):
    return (v[0], v[2], -v[1])


def field_layout(name):
    if name == "Position":
        return ("POSITION", 3)
    if name == "Normal":
        return ("NORMAL", 3)
    if name == "Tangent":
        return ("TANGENT", 4)
    m = re.fullmatch(r"(Colour|Color|TexCoord)([0-9]+)", name)
    if m:
        return (("TEXCOORD_" + m[2], 2) if m[1] == "TexCoord" else ("COLOR_" + m[2], 4))
    raise ValueError(f"unsupported static vertex field {name}")


def visual_geometry(geometry, glb):
    vb = geometry.find("VertexBuffer")
    if vb is None:
        raise ValueError("missing VertexBuffer")
    layout = vb.find("Layout")
    if layout is None:
        raise ValueError("missing vertex Layout")
    fields = [field_layout(element.tag) for element in layout]
    columns = {semantic: [] for semantic, _ in fields}
    if len(columns) != len(fields) or "POSITION" not in columns:
        raise ValueError("vertex Layout needs one Position and unique semantics")
    for prefix in ("COLOR_", "TEXCOORD_"):
        layers = sorted(int(key.removeprefix(prefix)) for key in columns if key.startswith(prefix))
        if layers != list(range(len(layers))):
            raise ValueError(f"{prefix} layers must be contiguous from zero")
    width = sum(n for _, n in fields)
    first = (vb.findtext("Data") or "").strip()
    second = (vb.findtext("Data2") or "").strip()
    if first and second and first.split() != second.split():
        raise ValueError("distinct primary and secondary vertex buffers need an explicit selection")
    for row in (first or second).splitlines():
        if not row.strip():
            continue
        values = numbers(row)
        if len(values) != width:
            raise ValueError(f"vertex row has {len(values)} values, layout needs {width}")
        offset = 0
        for semantic, n in fields:
            value = tuple(values[offset:offset + n])
            offset += n
            if semantic == "POSITION":
                value = y_up(value)
            elif semantic == "NORMAL":
                value = unit(y_up(value))
            elif semantic == "TANGENT":
                if abs(value[3]) != 1:
                    raise ValueError("tangent handedness must be -1 or +1")
                value = (*unit(y_up(value[:3])), value[3])
            elif semantic.startswith("COLOR_"):
                if any(not x.is_integer() or not 0 <= x <= 255 for x in value):
                    raise ValueError("expected unsigned byte colour channels")
                value = tuple(int(x) for x in value)
            columns[semantic].append(value)
    ids = [integer(n) for n in (geometry.findtext("IndexBuffer/Data") or "").split()]
    material = integer(scalar(geometry, "ShaderIndex", "0"))
    if material < 0:
        raise ValueError("negative ShaderIndex")
    materials = glb.doc.get("materials", [])
    if materials and material >= len(materials):
        raise ValueError(f"ShaderIndex {material} exceeds shader count")
    return glb.primitive(columns.pop("POSITION"), ids, columns, material if materials else None)


def read_drawable(root, glb, report):
    if root.tag != "Drawable":
        raise ValueError(f"expected Drawable, found {root.tag}")
    if root.find("Matrix") is not None:
        raise ValueError("drawable-level Matrix is outside the static profile")
    if root.find("Skeleton/Bones/Item") is not None:
        raise ValueError("skinned or bone-attached drawable is outside the static profile")
    report["drawableName"] = root.findtext("Name")
    shaders = root.findall("ShaderGroup/Shaders/Item")
    report["shaders"] = []
    for i, shader in enumerate(shaders):
        name = shader.findtext("Name") or f"shader-{i}"
        report["shaders"].append({"slot": i, "name": name, "file": shader.findtext("FileName"),
                                  "parameters": [p.get("name") for p in shader.findall("Parameters/Item")]})
        glb.doc.setdefault("materials", []).append({
            "name": name, "pbrMetallicRoughness": {"metallicFactor": 0.0, "roughnessFactor": 1.0},
        })
    if shaders and "appearance" not in report:
        report["notes"].append("Named shader slots use a neutral PBR material. Source shader programs, textures and parameters are not translated.")
    if root.find("Lights/Item") is not None:
        report["notes"].append("Drawable lights are outside the object geometry profile.")
    levels = (("High", "High", "high"), ("Medium", "Med", "medium"),
              ("Low", "Low", "low"), ("VeryLow", "Vlow", "veryLow"))
    known = {"DrawableModels" + model_name for model_name, _, _ in levels}
    unknown = [c.tag for c in root if c.tag.startswith("DrawableModels") and len(c) and c.tag not in known]
    if unknown:
        raise ValueError(f"unknown drawable model list {unknown}")
    report["levelDistances"] = {}
    for model_name, distance_name, level in levels:
        name = "DrawableModels" + model_name
        models = root.findall(name + "/Item")
        dist_tag = "LodDist" + distance_name
        if root.find(dist_tag) is not None:
            report["levelDistances"][level] = number(scalar(root, dist_tag))
        if not models:
            continue
        primitives = []
        for model in models:
            if integer(scalar(model, "HasSkin", "0")) or integer(scalar(model, "BoneIndex", "0")):
                raise ValueError("skinned or bone-attached model is outside the static profile")
            if model.find("Matrix") is not None:
                raise ValueError("a static drawable model with a local Matrix needs a transform adapter")
            geometries = model.findall("Geometries/Item")
            if not geometries:
                raise ValueError(f"empty {name} model")
            primitives.extend(visual_geometry(part, glb) for part in geometries)
        mesh = glb.mesh(glb.node["name"] + "/visual/" + level, primitives)
        if level == "high":
            glb.node["mesh"] = mesh
        else:
            glb.node_meta.setdefault("visualLods", []).append({"level": level, "mesh": mesh})
    if "mesh" not in glb.node:
        raise ValueError("expected a static high-detail drawable model")


def affine(node):
    text = node.findtext("CompositeTransform")
    if text is None:
        return None
    m = numbers(text)
    if len(m) != 16 or [m[i] for i in (3, 7, 11, 15)] != [0, 0, 0, 1]:
        raise ValueError("expected a 4 by 4 affine CompositeTransform in row-vector order")
    a, b, c = (m[start:start + 3] for start in (0, 4, 8))
    determinant = sum(i * j for i, j in zip(a, cross(b, c)))
    if determinant == 0:
        raise ValueError("singular CompositeTransform")
    return m, determinant < 0


def transformed(point, matrices):
    for matrix in matrices:
        point = tuple(matrix[12 + i] + sum(point[j] * matrix[j * 4 + i] for j in range(3)) for i in range(3))
    return y_up(point)


def box_faces(origin, axes):
    """Generate an oriented parallelepiped from one corner and its three adjacent edges."""
    det = sum(x * y for x, y in zip(axes[0], cross(axes[1], axes[2])))
    if det == 0:
        raise ValueError("zero-volume box")
    corners = [tuple(origin[i] + sum(bit * axis[i] for bit, axis in zip(bits, axes)) for i in range(3))
               for bits in product(range(2), repeat=3)]
    quads = ((0, 1, 3, 2), (4, 6, 7, 5), (0, 4, 5, 1), (2, 3, 7, 6), (0, 2, 6, 4), (1, 5, 7, 3))
    tris = []
    for a, b, c, d in quads:
        for face in ((a, b, c), (a, c, d)):
            tris.append(face if det > 0 else face[::-1])
    return corners, tris


def box_from_extents(low, high):
    if any(lo >= hi for lo, hi in zip(low, high)):
        raise ValueError("box extents must enclose positive volume")
    return box_faces(low, [(high[0] - low[0], 0, 0), (0, high[1] - low[1], 0), (0, 0, high[2] - low[2])])


def box_from_alternate(corners):
    # Four alternating corners define a box. Reflection in its centre gives the
    # other four, including the three corners adjacent to the first input.
    twice_center = tuple(sum(axis) / 2 for axis in zip(*corners))
    axes = [difference(difference(twice_center, corner), corners[0]) for corner in corners[1:]]
    return box_faces(corners[0], axes)


def add_bounds(root, base, emit, report):
    if root.tag == "BoundsFile":
        root = root.find("Bounds")
    if root is None or root.tag != "Bounds":
        raise ValueError("expected BoundsFile/Bounds or Bounds")

    def visit(node, path, matrices, reverse):
        transform = affine(node)
        if transform is not None:
            matrix, inverted = transform
            matrices = [matrix, *matrices]
            reverse ^= inverted
        kind = node.get("type")
        if kind == "Composite":
            children = node.findall("Children/Item")
            if not any(child.get("type") not in ("None", "Null") for child in children):
                raise ValueError(f"{path}: empty composite")
            for i, child in enumerate(children):
                if child.get("type") not in ("None", "Null"):
                    visit(child, f"{path}/{i}", matrices, reverse)
            return
        meta = {"path": path, "type": kind}
        for name in ("Margin", "PolyFlags", "UnkFlags", "CompositeFlags1", "CompositeFlags2"):
            item = node.find(name)
            if item is not None:
                meta[name] = item.get("value") or (item.text or "").strip()
        report["bounds"].append(meta)
        if kind == "Box":
            positions, triangles = box_from_extents(vector(node, "BoxMin"), vector(node, "BoxMax"))
            triangles = [t[::-1] if reverse else t for t in triangles]
            mat_type = integer(scalar(node, "MaterialIndex", "0"))
            if not 0 <= mat_type <= 255:
                raise ValueError("material type outside byte range")
            emit(f"{path}/m0", mat_type, [transformed(p, matrices) for p in positions], triangles)
            return
        if kind not in ("Geometry", "GeometryBVH"):
            raise ValueError(f"{path}: unsupported analytic bound {kind}")
        center = vector(node, "GeometryCenter", (0, 0, 0))
        rows = [numbers(row) for row in (node.findtext("Vertices") or "").splitlines() if row.strip()]
        if not rows or any(len(row) != 3 for row in rows):
            raise ValueError(f"{path}: expected one vertex triple per line")
        vertices = [tuple(c + v for c, v in zip(center, row)) for row in rows]
        source_vertex_count = len(vertices)
        materials = []
        for entry in node.findall("Materials/Item"):
            material = integer(scalar(entry, "Type"))
            if not 0 <= material <= 255:
                raise ValueError(f"{path}: material type outside byte range")
            materials.append(material)
        meta["materials"] = [
            {entry.tag: entry.get("value") or (entry.text or "").strip() for entry in m}
            for m in node.findall("Materials/Item")
        ]
        meta["usesShrunkVertices"] = bool((node.findtext("VerticesShrunk") or "").strip())
        polygons = node.find("Polygons")
        if polygons is None or len(polygons) == 0:
            raise ValueError(f"{path}: missing polygons")
        groups = defaultdict(list)
        for ordinal, polygon in enumerate(polygons):
            slot = integer(polygon.attrib["m"])
            if not 0 <= slot < len(materials):
                raise ValueError(f"{path}: polygon material index {slot} outside table")
            if polygon.tag not in ("Triangle", "Box"):
                raise ValueError(f"{path}: unsupported analytic polygon {polygon.tag}")
            count = 3 if polygon.tag == "Triangle" else 4
            ids = [integer(polygon.attrib[f"v{i + 1}"]) for i in range(count)]
            if any(i < 0 or i >= source_vertex_count for i in ids):
                raise ValueError(f"{path}: bad polygon vertex index")
            meta.setdefault("sourcePolygons", {}).setdefault(str(slot), []).append(ordinal)
            if polygon.tag == "Triangle":
                flags = [integer(polygon.get(f"f{i}", "0")) for i in (1, 2, 3)]
                if any(flags):
                    meta.setdefault("triangleEdgeFlags", {})[str(ordinal)] = flags
                groups[slot].append(ids)
            else:
                meta.setdefault("boxPolygons", []).append(ordinal)
                corners, tris = box_from_alternate([vertices[i] for i in ids])
                offset = len(vertices)
                vertices.extend(corners)
                groups[slot].extend(tuple(offset + i for i in tri) for tri in tris)
        for slot, triangles in sorted(groups.items()):
            lookup = {}
            positions = []
            faces = []
            for face in triangles:
                tri = []
                for vid in (face[::-1] if reverse else face):
                    if vid not in lookup:
                        lookup[vid] = len(positions)
                        positions.append(transformed(vertices[vid], matrices))
                    tri.append(lookup[vid])
                faces.append(tri)
            emit(f"{path}/m{slot}", materials[slot], positions, faces)

    visit(root, base, [], False)
