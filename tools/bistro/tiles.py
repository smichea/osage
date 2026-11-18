# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Photographs the light coming into the Paris world's blocks (D-136).

    blender -b assets/orca/bistro/exterior.blend --python tools/bistro/tiles.py -- TILES OUT [SIZE] [SAMPLES]

TILES is what `paris-tiles` lists (each tile's key, where its probe stands,
how it looks, in the glTF's frame); `exterior.blend` is written by
`tools/bistro/prepare.py`. For each tile, from its probe, Cycles renders an
equidistant fisheye over 200 degrees (the half sphere and a little more, so
that what lies along the tile falls inside its circle, not on its rim),
SIZE pixels square (384 unless
given), SAMPLES samples a pixel (16 unless given): the radiance coming in,
sky and all, and where each pixel lies. In OUT/KEY/: `radiance.f32` (red,
green, blue, float32, rows from the top), `distance.f32` (metres, float32,
infinite where the sky is) and `meta.json`. A tile already there is left
as it is, so that a bake can stop and go on. `light-pack` then makes them
AV1. Runs inside Blender (tested with 5.2), numpy as Blender bundles it.
"""

import json
import math
import pathlib
import sys
import time

import bpy
import numpy as np
from mathutils import Matrix, Vector

# Tiles rendered in a row before the scene changes material and back.
BATCH = 16


def blender(v):
    """A point or direction of the glTF's frame (y up) in Blender's (z up)."""
    return Vector((v[0], -v[2], v[1]))


def fisheye(name, origin, forward, up):
    data = bpy.data.cameras.new(name)
    data.type = "PANO"
    data.panorama_type = "FISHEYE_EQUIDISTANT"
    data.fisheye_fov = math.radians(200.0)
    data.clip_start = 0.005
    data.clip_end = 3000.0
    obj = bpy.data.objects.new(name, data)
    bpy.context.scene.collection.objects.link(obj)
    forward, up = forward.normalized(), up.normalized()
    right = forward.cross(up).normalized()
    up = right.cross(forward).normalized()
    obj.matrix_world = Matrix.Translation(origin) @ Matrix((right, up, -forward)).transposed().to_4x4()
    return obj


def position_material():
    material = bpy.data.materials.new("osage-position")
    tree = material.node_tree
    tree.nodes.clear()
    geometry = tree.nodes.new("ShaderNodeNewGeometry")
    emission = tree.nodes.new("ShaderNodeEmission")
    out = tree.nodes.new("ShaderNodeOutputMaterial")
    tree.links.new(geometry.outputs["Position"], emission.inputs["Color"])
    tree.links.new(emission.outputs["Emission"], out.inputs["Surface"])
    return material


def setup(scene, size, samples, transparent):
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = samples
    scene.cycles.use_denoising = samples > 1
    scene.cycles.filter_width = 1.5 if samples > 1 else 0.01
    scene.render.resolution_x = scene.render.resolution_y = size
    scene.render.resolution_percentage = 100
    scene.render.use_persistent_data = True
    scene.render.film_transparent = transparent
    scene.render.image_settings.file_format = "OPEN_EXR"
    scene.render.image_settings.color_mode = "RGBA"
    scene.render.image_settings.color_depth = "32"


def render(scene, camera, path):
    scene.camera = camera
    scene.render.filepath = str(path)
    bpy.ops.render.render(write_still=True)


def load(path, size):
    image = bpy.data.images.load(str(path))
    pixels = np.empty(size * size * 4, np.float32)
    image.pixels.foreach_get(pixels)
    bpy.data.images.remove(image)
    # Blender's rows go up from the bottom; the tile's go down from the top.
    return pixels.reshape(size, size, 4)[::-1]


def main():
    rest = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    if len(rest) < 2:
        sys.exit(__doc__)
    tiles = json.loads(pathlib.Path(rest[0]).read_text())
    out = pathlib.Path(rest[1]).resolve()
    size = int(rest[2]) if len(rest) > 2 else 384
    samples = int(rest[3]) if len(rest) > 3 else 16
    scene = bpy.context.scene
    layer = scene.view_layers[0]
    position = position_material()
    work = out / ".work"
    work.mkdir(parents=True, exist_ok=True)
    todo = [t for t in tiles if not (out / t["key"] / "distance.f32").exists()]
    print(f"tiles: {len(tiles)} listed, {len(todo)} to photograph at {size} px, {samples} samples", flush=True)
    started = time.time()
    for first in range(0, len(todo), BATCH):
        batch = todo[first : first + BATCH]
        cameras = [
            fisheye(t["key"], blender(t["centre"]) + blender(t["normal"]) * 0.002, blender(t["normal"]), blender(t["up"]))
            for t in batch
        ]
        setup(scene, size, samples, False)
        layer.material_override = None
        for t, cam in zip(batch, cameras):
            render(scene, cam, work / f"{t['key']}.exr")
        setup(scene, size, 1, True)
        layer.material_override = position
        for t, cam in zip(batch, cameras):
            render(scene, cam, work / f"{t['key']}-position.exr")
        layer.material_override = None
        for t, cam in zip(batch, cameras):
            colour = load(work / f"{t['key']}.exr", size)
            where = load(work / f"{t['key']}-position.exr", size)
            centre = np.array(blender(t["centre"]))
            distance = np.linalg.norm(where[..., :3] - centre, axis=-1)
            distance[where[..., 3] < 0.5] = np.inf
            folder = out / t["key"]
            folder.mkdir(parents=True, exist_ok=True)
            colour[..., :3].astype("<f4").tofile(folder / "radiance.f32")
            meta = {"width": size, "height": size, "samples": samples, "projection": "fisheye-equidistant-200"}
            meta.update({k: t[k] for k in ("key", "centre", "normal", "up")})
            (folder / "meta.json").write_text(json.dumps(meta, indent=1) + "\n")
            # Last: its presence says the tile is whole.
            distance.astype("<f4").tofile(folder / "distance.f32")
            (work / f"{t['key']}.exr").unlink()
            (work / f"{t['key']}-position.exr").unlink()
            bpy.data.objects.remove(cam, do_unlink=True)
        done = first + len(batch)
        rate = (time.time() - started) / done
        print(f"tiles: {done}/{len(todo)}, {rate:.0f} s a tile, {rate * (len(todo) - done) / 60:.0f} min to go", flush=True)


main()
