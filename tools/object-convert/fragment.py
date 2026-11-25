# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

"""Read the independent pristine, single-bound-per-link articulated interchange."""

import math
import struct
from collections import defaultdict

from glb import cross
from rig_math import (
    Y_UP,
    ZERO,
    I,
    add,
    assemble,
    compose,
    dot,
    quaternion,
    relative_pose,
)
from xml_geometry import integer, number, numbers, scalar, vector, visual_geometry, y_up

EPS = 2e-5


def gltf_float(value):
    return struct.unpack("<f", struct.pack("<f", value))[0]


def gltf_rotation(q):
    """Expand a glTF quaternion at the node channel's float32 precision."""
    x, y, z, w = map(gltf_float, q)
    xx, yy, zz = (gltf_float(2 * v * v) for v in (x, y, z))
    xy, xz, yz, xw, yw, zw = (
        gltf_float(2 * a * b) for a, b in ((x, y), (x, z), (y, z), (x, w), (y, w), (z, w))
    )
    return (
        (gltf_float(gltf_float(1 - yy) - zz), gltf_float(xy + zw), gltf_float(xz - yw)),
        (gltf_float(xy - zw), gltf_float(gltf_float(1 - xx) - zz), gltf_float(yz + xw)),
        (gltf_float(xz + yw), gltf_float(yz - xw), gltf_float(gltf_float(1 - xx) - yy)),
    )


def idx(node, name):
    return integer(scalar(node, name))


def four(node, name):
    field = node.find(name)
    if field is None:
        raise ValueError(f"missing {name}")
    return tuple(number(field.attrib[c]) for c in "xyzw")


def rigid(r):
    if (
        any(abs(dot(a, b) - (i == j)) > EPS for i, a in enumerate(r) for j, b in enumerate(r))
        or dot(cross(*r[:2]), r[2]) < 1 - EPS
    ):
        raise ValueError("expected an orthonormal right-handed rigid frame")
    return r


def frame(element):
    if element is None:
        raise ValueError("missing link or drawable transform")
    v = numbers(element.text)
    if len(v) not in (12, 16):
        raise ValueError("expected a 4×3 or 4×4 matrix")
    n = len(v) // 4
    if n == 4 and any(abs(x - y) > EPS for x, y in zip(v[3::4], (0, 0, 0, 1))):
        raise ValueError("expected an affine bind transform")
    rows = tuple(tuple(v[i : i + 3]) for i in range(0, len(v), n))
    return rigid(rows[:3]), rows[3]


def close_frame(f, g):
    return max(math.dist(a, b) for a, b in zip((*f[0], f[1]), (*g[0], g[1]))) <= EPS


def local_joint_frame(joint, start, convention):
    fields = [four(joint, f"Unknown{i}0") for i in range(start, start + 4)]
    axes = rigid(tuple(v[:3] for v in fields[:3]))
    if convention == "body-to-joint-z":
        # Columns of the body-to-joint matrix are joint axes in body space.
        # The portable frame carries the twist or hinge axis on X.
        x, y, z = zip(*axes)
        axes = z, x, y
    return axes, fields[3][:3]


def bind_nodes(draw, glb, report):
    transform = draw.find("Matrix")
    if transform is not None and not close_frame(frame(transform), (I, ZERO)):
        raise ValueError("a nonidentity fragment drawable transform requires an explicit model-space adapter")
    nodes = glb.doc["nodes"]
    bones = draw.findall("Skeleton/Bones/Item")
    by_tag = {}
    bone_world = [(I, ZERO)]
    names = set()
    for i, bone in enumerate(bones):
        parent, tag = idx(bone, "ParentIndex"), idx(bone, "Tag")
        if idx(bone, "Index") != i or not -1 <= parent < i or tag in by_tag:
            raise ValueError("expected parent-first bones, matching ordinal indices and unique bone tags")
        name = bone.findtext("Name")
        if not name or name != name.strip() or name in names or "," in name or any(ord(c) < 32 for c in name):
            raise ValueError("a skeleton bone needs a unique trace-safe name")
        names.add(name)
        bone_id = f"{glb.profile['id']}/bone/{tag}"
        q = four(bone, "Rotation")
        if abs(math.hypot(*q) - 1) > EPS:
            raise ValueError("non-unit bone rotation")
        scale = vector(bone, "Scale")
        if min(scale) <= 0:
            raise ValueError("invalid bone scale")
        nodes[parent + 1].setdefault("children", []).append(i + 1)
        nodes.append(
            {
                "name": name,
                "translation": y_up(vector(bone, "Translation")),
                "rotation": (*y_up(q[:3]), q[3]),
                "scale": (scale[0], scale[2], scale[1]),
                "extras": {"osage": {"id": bone_id, "collision": [], "acoustic": []}},
            }
        )
        node = nodes[-1]
        # The glTF node channel has float32 precision. Express local physics against that exported bind.
        axes = gltf_rotation(node["rotation"])
        scaled = tuple(
            tuple(gltf_float(x * gltf_float(s)) for x in axis) for axis, s in zip(axes, node["scale"])
        )
        world = assemble(bone_world[parent + 1], (scaled, tuple(map(gltf_float, node["translation"]))))
        bone_world.append(world)
        by_tag[tag] = (name, bone_id, world)
    if not by_tag:
        raise ValueError("fragment needs a skeleton")
    shaders = draw.findall("ShaderGroup/Shaders/Item")
    report["shaders"] = []
    for i, shader in enumerate(shaders):
        name = shader.findtext("Name") or f"shader-{i}"
        report["shaders"].append(
            {
                "slot": i,
                "name": name,
                "file": shader.findtext("FileName"),
                "parameters": [p.get("name") for p in shader.findall("Parameters/Item")],
            }
        )
        glb.doc.setdefault("materials", []).append(
            {"name": name, "pbrMetallicRoughness": {"metallicFactor": 0.0, "roughnessFactor": 1.0}}
        )
    for level, out_level in (("High", "high"), ("Medium", "medium"), ("Low", "low"), ("VeryLow", "veryLow")):
        geometry = defaultdict(list)
        for model in draw.findall(f"DrawableModels{level}/Item"):
            i = idx(model, "BoneIndex")
            if (
                not 0 <= i < len(bones)
                or integer(scalar(model, "HasSkin", "0"))
                or model.find("Matrix") is not None
            ):
                raise ValueError("fragment visual needs a rigid model with a valid bone attachment")
            items = model.findall("Geometries/Item")
            if not items:
                raise ValueError("empty rigid bone visual")
            geometry[i].extend(visual_geometry(part, glb) for part in items)
        for i, primitives in geometry.items():
            node = nodes[i + 1]
            mesh = glb.mesh(node["name"] + "/" + out_level, primitives)
            if level == "High":
                node["mesh"] = mesh
            elif "mesh" in node:
                node["extras"]["osage"].setdefault("visualLods", []).append(
                    {"level": out_level, "mesh": mesh}
                )
            else:
                raise ValueError("a lower visual level requires an attachment at high detail")
    note = "Rigid bone visuals keep their geometry."
    if "appearance" not in report:
        note += " Original shader slots use neutral PBR materials."
    report["notes"].append(note)
    return by_tag


def read_fragment(root, glb, report, table, space, sphere_radius=None, joint_frames=None, swing_envelope=None):
    if root.tag != "Fragment":
        raise ValueError(f"expected Fragment, found {root.tag}")
    draw, lod = root.find("Drawable"), root.find("Physics/LOD1")
    if draw is None or lod is None:
        raise ValueError("fragment needs its drawable and physics LOD1")
    by_tag = bind_nodes(draw, glb, report)
    report["fragmentName"] = root.findtext("Name")
    children = lod.findall("Children/Item")
    raw_shapes = lod.findall("Archetype/Bounds/Children/Item")
    attachments = lod.findall("Transforms/Item")
    group_count = len(lod.findall("Groups/Item"))
    if not children or len(children) != len(raw_shapes) or len(children) != len(attachments):
        raise ValueError("expected one pristine analytic bound and link bind transform per physical child")
    offset = vector(lod, "PositionOffset", ZERO)
    links = []
    bodies = []
    groups_by_body = []
    seen_bones = set()
    for child, bound, transform in zip(children, raw_shapes, attachments, strict=True):
        tag = idx(child, "BoneTag")
        if tag not in by_tag or tag in seen_bones:
            raise ValueError(f"missing or repeated physical bone tag {tag}")
        seen_bones.add(tag)
        name, bone_id, bone_world = by_tag[tag]
        group = idx(child, "GroupIndex")
        if not 0 <= group < group_count:
            raise ValueError(f"{name}: invalid fragment group index")
        groups_by_body.append(group)
        # Attachment translations are relative to PositionOffset. Bounds are in archetype space.
        r, p = frame(transform)
        bind = (r, add(p, offset))
        shape_bind = frame(bound.find("CompositeTransform"))
        if not close_frame(bind, shape_bind):
            raise ValueError(
                f"{name}: offset or rotated shape relative to the mass frame is outside this profile"
            )
        links.append((compose(Y_UP, r), y_up(bind[1])))
        for centre_tag in ("BoxCenter", "SphereCenter"):
            if math.dist(vector(bound, centre_tag), ZERO) > EPS:
                raise ValueError(f"{name}: off-centre analytic bound")
        kind = bound.get("type")
        lo, hi = vector(bound, "BoxMin"), vector(bound, "BoxMax")
        if math.dist(tuple((a + b) / 2 for a, b in zip(lo, hi)), ZERO) > EPS:
            raise ValueError(f"{name}: expected centred analytic shape")
        if kind == "Sphere":
            declared_radius = number(scalar(bound, "SphereRadius"))
            if declared_radius <= 0:
                raise ValueError(f"{name}: positive declared sphere radius required")
            radius = declared_radius
            if sphere_radius is not None:
                raw_margin = bound.find("Margin")
                margin = None if raw_margin is None else number(scalar(bound, "Margin"))
                if sphere_radius == "margin":
                    if margin is None or margin <= 0:
                        raise ValueError(f"{name}: a positive sphere Margin is required for a contact sphere")
                    radius = margin
                report.setdefault("sphereRadii", []).append({
                    "bone": bone_id, "sourceField": "Margin" if sphere_radius == "margin" else "SphereRadius",
                    "declaredRadiusM": declared_radius, "marginM": margin, "usedRadiusM": radius,
                })
            shape = {"type": "sphere", "radiusM": radius}
        elif kind == "Capsule":
            radius = number(scalar(bound, "Margin"))
            extent = number(scalar(bound, "SphereRadius"))
            half = extent - radius
            if (
                half < 0
                or math.dist(tuple((b - a) / 2 for a, b in zip(lo, hi)), (radius, extent, radius)) > EPS
            ):
                raise ValueError(f"{name}: invalid Y-axis capsule bounds")
            shape = {"type": "capsule", "radiusM": radius, "halfSegmentM": half}
        else:
            raise ValueError(f"{name}: unsupported articulated bound {kind}")
        mass = number(scalar(child, "PristineMass"))
        inertia = vector(child, "InertiaTensor")
        if min(radius, mass, *inertia) <= 0:
            raise ValueError(f"{name}: positive radius, pristine mass and principal moments required")
        mat = f"surface-{integer(scalar(bound, 'MaterialIndex', '0'))}"
        mapped = table.get(mat, {})
        glb.profile["collisionMaterials"][mat] = {
            k: mapped.get(k, d) for k, d in (("friction", 0.5), ("restitution", 0.0))
        }
        report["materialOrigins"].setdefault(mat, {})["collision"] = (
            "mapping" if "friction" in mapped else "Osage defaults"
        )
        axes, center = links[-1]
        rotation = quaternion(axes)
        if space == "bone":
            rigid(bone_world[0])
            rotation, center = relative_pose(bone_world, links[-1], node=True)
        bodies.append(
            {
                "name": name,
                "bone": bone_id,
                "parent": None,
                "massKg": mass,
                "inertiaKgM2": inertia,
                "centerM": center,
                "rotationXyzw": rotation,
                "shape": shape,
                "material": mat,
                "joint": None,
            }
        )
    raw_joints = []
    for joint in lod.findall("ArticulatedBody/Joints/Item"):
        a, b = idx(joint, "FragIndex1"), idx(joint, "FragIndex2")
        if not 0 <= a < b < len(bodies) or bodies[b]["parent"] is not None:
            raise ValueError("expected a connected parent-first physical joint tree")
        parent_local, child_local = local_joint_frame(joint, 2, joint_frames), local_joint_frame(joint, 6, joint_frames)
        parent_frame, child_frame = assemble(links[a], parent_local), assemble(links[b], child_local)
        if not close_frame(parent_frame, child_frame):
            raise ValueError(f"{bodies[b]['name']}: parent/child joint frames disagree at rest")
        angles = four(joint, "UnknownA0")
        t = joint.get("type")
        if t == "DOF1":
            kind, limits = "hinge", ((angles[0], angles[1]), (0.0, 0.0), (0.0, 0.0))
        elif t == "DOF3":
            # A lean towards an axis is a rotation about its perpendicular.
            lean_y, lean_z = (angles[1], angles[0]) if joint_frames == "body-to-joint-z" else angles[:2]
            kind, limits = "three-axis", [(-x, x) for x in (angles[2], lean_y, lean_z)]
        else:
            raise ValueError(f"unsupported joint {t}")
        if any(not -math.pi - 1e-6 <= lo <= 0 <= hi <= math.pi + 1e-6 for lo, hi in limits):
            raise ValueError(f"{bodies[b]['name']}: invalid angular stops")
        bodies[b]["parent"] = a
        if space == "actor":
            bodies[b]["joint"] = {
                "kind": kind,
                "anchorM": parent_frame[1],
                "frameXyzw": quaternion(parent_frame[0]),
                "limitsRad": limits,
            }
        else:

            def endpoint(link, assembled):
                q, p = relative_pose(link, assembled)
                return {"anchorM": p, "rotationXyzw": q}

            bodies[b]["joint"] = {
                "kind": kind,
                "parentFrame": endpoint(links[a], parent_frame),
                "childFrame": endpoint(links[b], child_frame),
                "limitsRad": limits,
            }
        if swing_envelope and t == "DOF3" and limits[1][1] > 0 and limits[2][1] > 0:
            bodies[b]["joint"]["swingEnvelope"] = swing_envelope
        raw_joints.append(
            {
                "parent": a,
                "child": b,
                "type": t,
                "parameters": angles,
                "stiffness": number(scalar(joint, "Unknown10", "0")),
                "parentFrame": [four(joint, f"Unknown{i}0") for i in range(2, 6)],
                "childFrame": [four(joint, f"Unknown{i}0") for i in range(6, 10)],
            }
        )
    # The public writer stores one physical-inertia vector per link and two fixed
    # 22-entry joint tables. Check every redundant value before accepting a rig.
    art = lod.find("ArticulatedBody")
    if art is None or len(raw_joints) > 22:
        raise ValueError("missing articulated-body table or more than 22 source joints")
    for tag, expected in (
        ("ItemIndices", [j["parent"] for j in raw_joints]),
        ("ItemFlags", [int(j["type"] == "DOF3") for j in raw_joints]),
    ):
        array = [integer(n) for n in (art.findtext(tag) or "").split()]
        if array != expected + [0] * (22 - len(expected)):
            raise ValueError(f"{tag}: inconsistent source joint lookup table")
    link_mass = numbers(art.findtext("UnknownVectors"))
    if len(link_mass) != 4 * len(bodies):
        raise ValueError("expected a source inertia vector for every articulated link")
    for i, b in enumerate(bodies):
        expected = (*b["inertiaKgM2"], b["massKg"])
        if any(
            not math.isclose(a, v, rel_tol=EPS, abs_tol=1e-8)
            for a, v in zip(expected, link_mass[4 * i : 4 * i + 4])
        ):
            raise ValueError(f"{b['name']}: inconsistent source link mass/inertia")
    if any(b["parent"] is None for b in bodies[1:]):
        raise ValueError("each non-root body needs one source joint")
    report["fragmentPhysics"] = {"level": 1, "groupByBody": groups_by_body, "jointSourceFrames": raw_joints}
    if joint_frames is not None:
        report["fragmentPhysics"]["jointFrameConvention"] = joint_frames
    report["notes"].append(
        "The passive pristine rig uses centred sphere and capsule bounds, mass, principal moments and hard angular stops. Soft drives, muscle torque, damage, group breakage, source damping and contact filters are outside the adapter. DOF1 uses the first joint axis. DOF3 maps twist to X and the two lean half-ranges to Y and Z."
    )
    if not swing_envelope and joint_frames == "body-to-joint-z" and any(j["type"] == "DOF3" for j in raw_joints):
        report["notes"].append(
            "The converted intervals preserve the twist and two principal swing amplitudes. "
            "The source envelope for simultaneous swing on both axes requires separate solver support."
        )
    if any(root.find("Physics/" + level) is not None for level in ("LOD2", "LOD3")):
        report["notes"].append("Additional source physics LODs are present. Only LOD1 drives this profile.")
    if any(child.find("Drawable") is not None or child.find("Drawable2") is not None for child in children):
        raise ValueError("child-local rigid drawables need an explicit visual bind adapter")
    glb.profile["articulation"] = (
        {"bodies": bodies} if space == "actor" else {"space": "bone", "bodies": bodies}
    )
