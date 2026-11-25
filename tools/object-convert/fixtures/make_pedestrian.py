#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Author the bench mannequin as an independent fragment interchange file (metres and kilograms)."""

import math
import sys
import xml.etree.ElementTree as E
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from glb import difference
from rig_math import Z_UP, ZERO, I, add, body_basis, compose, rotate


def num(x):
    return f"{x:.9g}" if x else "0"


def vec(root, tag, values):
    return E.SubElement(root, tag, dict(zip("xyzw", map(num, values))))


def scalar(root, tag, value):
    return E.SubElement(root, tag, value=num(value))


def text(root, tag, value):
    e = E.SubElement(root, tag)
    e.text = str(value)
    return e


def matrix(root, tag, r, p):
    return text(
        root, tag, "\n" + "\n".join(" ".join(map(num, (*v, i == 3))) for i, v in enumerate((*r, p))) + "\n"
    )


def source(v):
    return rotate(Z_UP, v)


def segments():
    pelvis, chest, neck = (0, 0.84, -0.04), (0, 1.08, -0.01), (0, 1.52, -0.04)
    # The three angle parameters are lean 1, lean 2 and twist. Hinges carry low and high.
    rows = [
        ("pelvis", None, pelvis, chest, 0.128, 11, None),
        ("spine_01", 0, chest, neck, 0.145, 25, (26, 26, 30)),
        ("Head", 1, neck, (0, 1.79, -0.01), 0.1, 5, (35, 50, 40)),
    ]
    for s, side in ((1, "l"), (-1, "r")):
        a = len(rows)
        shoulder, elbow, wrist = (s * 0.21, 1.46, -0.06), (s * 0.46, 1.46, -0.075), (s * 0.70, 1.46, -0.065)
        hip, knee, ankle = (s * 0.114, 0.97, -0.036), (s * 0.114, 0.542, -0.036), (s * 0.114, 0.086, -0.088)
        for n, par, start, end, r, mass, lim in [
            ("upperarm", 1, shoulder, elbow, 0.061, 3, (85, 85, 80)),
            ("lowerarm", a, elbow, wrist, 0.052, 1.55, (-130, 2) if s > 0 else (-2, 130)),
            ("hand", a + 1, wrist, (s * 0.83, 1.46, -0.06), 0.042, 0.45, (35, 35, 35)),
            ("thigh", 0, hip, knee, 0.086, 7, (45, 50, 70)),
            ("calf", a + 3, knee, ankle, 0.063, 3.7, (-3, 125)),
            ("foot", a + 4, ankle, (s * 0.114, 0.046, 0.136), 0.048, 1.3, (12, 18, 35)),
        ]:
            rows.append((f"{n}_{side}", par, start, end, r, mass, lim))
    return rows


def write(path):
    write_fragment(path, segments(), "osage_pedestrian", ((-0.86, -0.16, 0), (0.86, 0.16, 1.8)),
                   (0, 0, 0.9), 1.2, 1.25, legacy_zero_sphere_margin=True)


def write_fragment(path, parts, object_name, bounds_z_up, sphere_center, radius, compound_radius,
                   *, legacy_zero_sphere_margin=False):
    root = E.Element("Fragment")
    text(root, "Name", object_name)
    vec(root, "BoundingSphereCenter", sphere_center)
    scalar(root, "BoundingSphereRadius", radius)
    drawable = E.SubElement(root, "Drawable")
    text(drawable, "Name", object_name)
    matrix(drawable, "Matrix", I, ZERO)
    vec(drawable, "BoundingSphereCenter", sphere_center)
    scalar(drawable, "BoundingSphereRadius", radius)
    vec(drawable, "BoundingBoxMin", bounds_z_up[0])
    vec(drawable, "BoundingBoxMax", bounds_z_up[1])
    for tag, d in (("High", 80), ("Med", 120), ("Low", 150), ("Vlow", 200)):
        scalar(drawable, "LodDist" + tag, d)
    shader = E.SubElement(E.SubElement(E.SubElement(drawable, "ShaderGroup"), "Shaders"), "Item")
    text(shader, "Name", "default")
    text(shader, "FileName", "default.sps")
    scalar(shader, "RenderBucket", 0)
    E.SubElement(shader, "Parameters")
    bones = E.SubElement(E.SubElement(drawable, "Skeleton"), "Bones")
    models = E.SubElement(drawable, "DrawableModelsHigh")
    physique = E.SubElement(E.SubElement(root, "Physics"), "LOD1")
    vec(physique, "PositionOffset", ZERO)
    body = E.SubElement(physique, "ArticulatedBody")
    pad = [0] * (22 - len(parts) + 1)
    text(body, "ItemIndices", " ".join(map(str, [part[1] for part in parts[1:]] + pad)))
    text(body, "ItemFlags", " ".join(map(str, [int(len(part[-1]) == 3) for part in parts[1:]] + pad)))
    inertias = E.SubElement(body, "UnknownVectors")
    joints = E.SubElement(body, "Joints")
    archetype = E.SubElement(physique, "Archetype")
    text(archetype, "Name", object_name)
    scalar(archetype, "Mass", sum(p[5] for p in parts))
    compound = E.SubElement(archetype, "Bounds", type="Composite")
    vec(compound, "BoxMin", bounds_z_up[0])
    vec(compound, "BoxMax", bounds_z_up[1])
    vec(compound, "SphereCenter", sphere_center)
    scalar(compound, "SphereRadius", compound_radius)
    bounds = E.SubElement(compound, "Children")
    transforms = E.SubElement(physique, "Transforms")
    groups = E.SubElement(physique, "Groups")
    children = E.SubElement(physique, "Children")
    frames = []
    for i, (name, parent, a, b, radius, mass, limit) in enumerate(parts):
        center = tuple((x + y) / 2 for x, y in zip(a, b))
        half = max(0.0, math.dist(a, b) / 2 - radius)
        # The head is an authored sphere, deliberately distinct from the limb capsules.
        if name == "Head":
            half = 0.0
        bone = E.SubElement(bones, "Item")
        text(bone, "Name", name)
        scalar(bone, "Tag", 1000 + i)
        scalar(bone, "Index", i)
        scalar(bone, "ParentIndex", parent if parent is not None else -1)
        sibling = next((j for j in range(i + 1, len(parts)) if parts[j][1] == parent), -1)
        scalar(bone, "SiblingIndex", sibling)
        vec(bone, "Translation", source(difference(a, parts[parent][2] if parent is not None else ZERO)))
        vec(bone, "Rotation", (0, 0, 0, 1))
        vec(bone, "Scale", (1, 1, 1))
        text(bone, "Flags", "RotX, RotY, RotZ")
        local_to_world = compose(Z_UP, body_basis(difference(b, a)))
        frame = (local_to_world, source(center))
        frames.append(frame)
        geom = E.SubElement(bounds, "Item", type="Capsule" if half else "Sphere")
        dims = (radius, half + radius, radius)
        vec(geom, "BoxMin", tuple(-x for x in dims))
        vec(geom, "BoxMax", dims)
        vec(geom, "BoxCenter", ZERO)
        vec(geom, "SphereCenter", ZERO)
        scalar(geom, "SphereRadius", half + radius)
        scalar(geom, "Margin", radius if half or not legacy_zero_sphere_margin else 0)
        scalar(geom, "MaterialIndex", 7)
        matrix(geom, "CompositeTransform", *frame)
        # Analytic moments about the local capsule axes. Integrate the cylinder and two hemispheres.
        h = 2 * half
        v1 = math.pi * radius**2 * h
        v2 = 4 * math.pi * radius**3 / 3
        mc = mass * v1 / (v1 + v2)
        ms = mass - mc
        axial = mc * radius**2 / 2 + 2 * ms * radius**2 / 5
        radial = mc * (3 * radius**2 + h * h) / 12 + ms * (
            2 * radius**2 / 5 + half * half + 3 * half * radius / 4
        )
        inertia = (radial, axial, radial)
        inertias.text = (inertias.text or "\n") + ", ".join(map(num, (*inertia, mass))) + "\n"
        vec(geom, "Inertia", tuple(x / mass for x in inertia))
        scalar(geom, "Volume", v1 + v2)
        text(geom, "CompositeFlags1", "MAP_DYNAMIC")
        text(geom, "CompositeFlags2", "MAP_DYNAMIC")
        matrix(transforms, "Item", *frame)
        g = E.SubElement(groups, "Item")
        text(g, "Name", name)
        scalar(g, "ParentIndex", parent if parent is not None else 255)
        c = E.SubElement(children, "Item")
        scalar(c, "GroupIndex", i)
        scalar(c, "BoneTag", 1000 + i)
        scalar(c, "PristineMass", mass)
        scalar(c, "DamagedMass", mass)
        vec(c, "InertiaTensor", (*inertia, v1 + v2))
        if parent is not None:
            jt = E.SubElement(joints, "Item", type="DOF1" if len(limit) == 2 else "DOF3")
            scalar(jt, "FragIndex1", parent)
            scalar(jt, "FragIndex2", i)
            scalar(jt, "Unknown10", 0)
            axis = (I[1], I[2], I[0]) if name.startswith("lowerarm") else I
            for link, start in ((parent, 2), (i, 6)):
                rot, pos = frames[link]
                inverse = tuple(zip(*rot))
                axes = compose(inverse, compose(Z_UP, axis))
                offset = rotate(inverse, difference(source(a), pos))
                for j, v in enumerate((*axes, offset)):
                    vec(jt, f"Unknown{start + j}0", (*v, 1 if j == 3 else 0))
            vals = tuple(map(math.radians, limit))
            vec(jt, "UnknownA0", (*(vals + (0,) * (4 - len(vals))),))
        # Independently authored rigid octahedral visual in this bone's local frame.
        model = E.SubElement(models, "Item")
        scalar(model, "HasSkin", 0)
        scalar(model, "BoneIndex", i)
        visual = E.SubElement(E.SubElement(model, "Geometries"), "Item")
        scalar(visual, "ShaderIndex", 0)
        vertices = [
            add(rotate(frame[0], v), difference(frame[1], source(a)))
            for v in (
                (radius, 0, 0),
                (-radius, 0, 0),
                (0, half + radius, 0),
                (0, -half - radius, 0),
                (0, 0, radius),
                (0, 0, -radius),
            )
        ]
        vec(visual, "BoundingBoxMin", tuple(map(min, zip(*vertices))))
        vec(visual, "BoundingBoxMax", tuple(map(max, zip(*vertices))))
        vb = E.SubElement(visual, "VertexBuffer")
        layout = E.SubElement(vb, "Layout", type="GTAV1")
        E.SubElement(layout, "Position")
        text(vb, "Data", "\n" + "\n".join(" ".join(map(num, v)) for v in vertices) + "\n")
        ib = E.SubElement(visual, "IndexBuffer")
        text(ib, "Data", "2 4 0 2 1 4 2 5 1 2 0 5 3 0 4 3 4 1 3 1 5 3 5 0")
    E.indent(root)
    payload = E.tostring(root, encoding="utf-8")
    path.write_bytes(
        b'<?xml version="1.0" encoding="UTF-8"?>\n<!-- SPDX-FileCopyrightText: The osage contributors -->\n<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->\n'
        + payload
        + b"\n"
    )


if __name__ == "__main__":
    write(Path(sys.argv[1]))
