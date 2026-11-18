# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Bistro for the Paris world's client and server (D-136).

    blender -b assets/orca/bistro/exterior.blend --python tools/bistro/export.py -- OUT

`exterior.blend` is written by `tools/bistro/prepare.py`. In OUT:

- `bistro.gltf`, its `.bin` and `textures/`: the street for Bevy, whose
  client traces the cubes around the walker. Normal maps are turned to
  OpenGL's convention (green up) as glTF wants them; cut-out materials
  (leaves, wrought iron) are masked at half opacity rather than blended.
- `walk.json` and `walk.bin`: where a walker can stand, a grid of 0.25 m
  cells over the street, in the glTF's frame (x east, y up, z south): the
  height of the ground in each cell, NaN where nobody stands — under a
  table, a bollard, inside a building. Found from where the scene's camera
  stands, cell by cell: a step up or down of at most 0.35 m, 1.8 m of room
  above, and no wall between two cells at the knee or the chest.

Runs inside Blender (tested with 5.2), numpy as Blender bundles it.
"""

import json
import pathlib
import sys
import time
from collections import deque

import bpy
import numpy as np
from mathutils import Vector
from mathutils.bvhtree import BVHTree

CELL = 0.25
STEP = 0.35
ROOM = 1.8
WALL_HEIGHTS = (0.45, 1.2)


def out_folder():
    rest = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    if not rest:
        sys.exit(__doc__)
    out = pathlib.Path(rest[0]).resolve()
    (out / "textures").mkdir(parents=True, exist_ok=True)
    return out


def opengl_normals(out):
    """Every material's normal map turned to OpenGL's convention, its node
    chain made the plain one glTF exporters read: image, normal map."""
    done = {}
    for material in bpy.data.materials:
        tree = material.node_tree
        if tree is None:
            continue
        mapping = next((n for n in tree.nodes if n.bl_idname == "ShaderNodeNormalMap"), None)
        if mapping is None:
            continue
        source = next(
            (
                n
                for n in tree.nodes
                if n.bl_idname == "ShaderNodeTexImage" and n.image and "_normal" in n.image.name.lower()
            ),
            None,
        )
        if source is None:
            continue
        image = source.image
        if image.name not in done:
            width, height = image.size
            pixels = np.empty(width * height * 4, np.float32)
            image.pixels.foreach_get(pixels)
            pixels = pixels.reshape(-1, 4)
            pixels[:, 1] = 1.0 - pixels[:, 1]
            pixels[:, 3] = 1.0
            name = pathlib.Path(image.name).stem + "_GL"
            turned = bpy.data.images.new(name, width, height, alpha=False)
            turned.pixels.foreach_set(pixels.ravel())
            turned.filepath_raw = str(out / "textures" / f"{name}.png")
            turned.file_format = "PNG"
            turned.save()
            turned.colorspace_settings.name = "Non-Color"
            done[image.name] = turned
        node = tree.nodes.new("ShaderNodeTexImage")
        node.image = done[image.name]
        for link in list(mapping.inputs["Color"].links):
            tree.links.remove(link)
        tree.links.new(node.outputs["Color"], mapping.inputs["Color"])
    print(f"export: {len(done)} normal maps turned", flush=True)


def masked_alpha():
    """Cut-outs: alpha through a rounding, which glTF exporters read as a mask."""
    count = 0
    for material in bpy.data.materials:
        tree = material.node_tree
        if tree is None:
            continue
        bsdf = next((n for n in tree.nodes if n.bl_idname == "ShaderNodeBsdfPrincipled"), None)
        if bsdf is None or not bsdf.inputs["Alpha"].links:
            continue
        link = bsdf.inputs["Alpha"].links[0]
        source = link.from_socket
        tree.links.remove(link)
        cut = tree.nodes.new("ShaderNodeMath")
        cut.operation = "ROUND"
        tree.links.new(source, cut.inputs[0])
        tree.links.new(cut.outputs[0], bsdf.inputs["Alpha"])
        count += 1
    print(f"export: {count} cut-out materials masked", flush=True)


def gltf(out):
    started = time.time()
    bpy.ops.export_scene.gltf(
        filepath=str(out / "bistro.gltf"),
        export_format="GLTF_SEPARATE",
        export_texture_dir="textures",
        export_image_format="AUTO",
        export_yup=True,
        export_apply=True,
        export_cameras=False,
        export_lights=False,
        export_animations=False,
        export_extras=False,
    )
    print(f"export: glTF in {time.time() - started:.0f} s", flush=True)


def tree_of_scene():
    """One BVH of every mesh as it is in the world."""
    depsgraph = bpy.context.evaluated_depsgraph_get()
    vertices, polygons = [], []
    offset = 0
    for obj in bpy.context.scene.objects:
        if obj.type != "MESH":
            continue
        evaluated = obj.evaluated_get(depsgraph)
        mesh = evaluated.to_mesh()
        mesh.calc_loop_triangles()
        n = len(mesh.vertices)
        co = np.empty(n * 3, np.float32)
        mesh.vertices.foreach_get("co", co)
        co = co.reshape(-1, 3)
        m = np.array(obj.matrix_world)
        co = co @ m[:3, :3].T + m[:3, 3]
        tris = np.empty(len(mesh.loop_triangles) * 3, np.int64)
        mesh.loop_triangles.foreach_get("vertices", tris)
        vertices.append(co)
        polygons.append(tris.reshape(-1, 3) + offset)
        offset += n
        evaluated.to_mesh_clear()
    vertices = np.concatenate(vertices)
    polygons = np.concatenate(polygons)
    print(f"export: {len(polygons)} triangles for the walking grid", flush=True)
    return BVHTree.FromPolygons(vertices.tolist(), polygons.tolist()), vertices


def floors(tree, x, y, top):
    """The surfaces facing up at (x, y), from the top down, and those facing
    down (what is above a floor)."""
    ups, downs = [], []
    z = top
    for _ in range(8):
        hit, normal, _, distance = tree.ray_cast(Vector((x, y, z)), Vector((0, 0, -1)))
        if hit is None:
            break
        (ups if normal.z > 0.6 else downs).append(hit.z)
        z = hit.z - 0.01
    return ups, downs + ups


def walls(tree, a, b, ground):
    """Whether a wall stands between two points at a walker's knee or chest."""
    d = b - a
    length = d.length
    for h in WALL_HEIGHTS:
        start = Vector((a.x, a.y, ground + h))
        hit, *_ = tree.ray_cast(start, d.normalized(), length)
        if hit is not None:
            return True
    return False


def walk_grid(out):
    started = time.time()
    tree, vertices = tree_of_scene()
    lo, hi = vertices.min(axis=0), vertices.max(axis=0)
    nx, ny = int((hi[0] - lo[0]) / CELL) + 1, int((hi[1] - lo[1]) / CELL) + 1
    top = float(hi[2]) + 1.0
    camera = bpy.context.scene.camera.matrix_world.translation
    start = (int((camera.x - lo[0]) / CELL), int((camera.y - lo[1]) / CELL))
    centre = lambda i, j: (lo[0] + (i + 0.5) * CELL, lo[1] + (j + 0.5) * CELL)
    ups, _ = floors(tree, *centre(*start), float(camera.z))
    if not ups:
        sys.exit("export: no ground under the camera")
    height = np.full((ny, nx), np.nan, np.float32)
    height[start[1], start[0]] = ups[0]
    queue = deque([start])
    cache = {}
    while queue:
        i, j = queue.popleft()
        here = float(height[j, i])
        for di, dj in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            ni, nj = i + di, j + dj
            if not (0 <= ni < nx and 0 <= nj < ny) or not np.isnan(height[nj, ni]):
                continue
            if (ni, nj) not in cache:
                cache[(ni, nj)] = floors(tree, *centre(ni, nj), top)
            candidates, everything = cache[(ni, nj)]
            near = [z for z in candidates if abs(z - here) <= STEP]
            if not near:
                continue
            ground = min(near, key=lambda z: abs(z - here))
            above = [z for z in everything if z > ground + 0.02]
            if above and min(above) < ground + ROOM:
                continue
            a, b = Vector((*centre(i, j), 0.0)), Vector((*centre(ni, nj), 0.0))
            if walls(tree, a, b, max(here, ground)):
                continue
            height[nj, ni] = ground
            queue.append((ni, nj))
    walkable = int((~np.isnan(height)).sum())
    print(f"export: {walkable} cells to walk ({walkable * CELL * CELL:.0f} m2) in {time.time() - started:.0f} s", flush=True)
    # In the glTF's frame: x east, y up, z south (Blender's -y). Rows go
    # with z, from the smallest z (Blender's largest y).
    rows = height[::-1, :]
    # Row r covers z from origin + r * CELL: Blender's cells j = ny - 1 - r.
    header = {
        "cell": CELL,
        "columns": nx,
        "rows": ny,
        "origin": [float(lo[0]), float(-(lo[1] + ny * CELL))],
        "start": [float(camera.x), float(ups[0]), float(-camera.y)],
        "note": "heights of the ground, metres, f32 little endian, row by row from the smallest z; NaN: nobody stands",
    }
    (out / "walk.json").write_text(json.dumps(header, indent=1) + "\n")
    rows.astype("<f4").tofile(out / "walk.bin")


def main():
    out = out_folder()
    opengl_normals(out)
    masked_alpha()
    gltf(out)
    walk_grid(out)


main()
