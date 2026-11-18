# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""What the walker sees where the scene's camera stands, rendered by Cycles:
the picture a client is measured against (research 10).

    blender -b exterior.blend --python tools/bistro/truth.py -- OUT [WIDTH HEIGHT [SAMPLES]]

The walker stands on the ground under the scene's camera, its eyes 1.65 m
up, looking where the camera looks, level; 75 degrees across. OUT.exr holds
the radiance, linear; OUT.png the picture through the scene's own view
transform (AgX), as a client's tonemapping shows it. OSAGE_DEVICE=OPTIX (or
CUDA) renders on the GPU.
"""

import math
import os
import pathlib
import sys

import bpy
from mathutils import Matrix, Vector

EYE = 1.65
FOV = math.radians(75.0)


def gpu(scene):
    kind = os.environ.get("OSAGE_DEVICE")
    if not kind:
        scene.cycles.device = "CPU"
        return
    prefs = bpy.context.preferences.addons["cycles"].preferences
    prefs.compute_device_type = kind
    prefs.refresh_devices()
    used = 0
    for device in prefs.devices:
        device.use = device.type == kind
        used += device.use
    if not used:
        sys.exit(f"truth: no {kind} device")
    scene.cycles.device = "GPU"


def main():
    rest = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    if not rest:
        sys.exit(__doc__)
    out = pathlib.Path(rest[0]).resolve()
    out.parent.mkdir(parents=True, exist_ok=True)
    width = int(rest[1]) if len(rest) > 1 else 960
    height = int(rest[2]) if len(rest) > 2 else 540
    samples = int(rest[3]) if len(rest) > 3 else 64
    scene = bpy.context.scene
    first = scene.camera
    depsgraph = bpy.context.evaluated_depsgraph_get()
    hit, ground, *_ = scene.ray_cast(depsgraph, first.matrix_world.translation, Vector((0, 0, -1)))
    standing = Vector(ground) if hit else first.matrix_world.translation - Vector((0, 0, EYE))
    eye = standing + Vector((0, 0, EYE))
    forward = -(first.matrix_world.to_3x3() @ Vector((0, 0, 1)))
    forward.z = 0.0
    forward.normalize()
    data = bpy.data.cameras.new("walker")
    data.sensor_fit = "HORIZONTAL"
    data.angle = FOV
    data.clip_start, data.clip_end = 0.005, 2000.0
    walker = bpy.data.objects.new("walker", data)
    scene.collection.objects.link(walker)
    up = Vector((0, 0, 1))
    right = forward.cross(up).normalized()
    up = right.cross(forward).normalized()
    walker.matrix_world = Matrix.Translation(eye) @ Matrix((right, up, -forward)).transposed().to_4x4()
    print(f"truth: eye {tuple(round(v, 3) for v in eye)}, forward {tuple(round(v, 4) for v in forward)}", flush=True)

    scene.render.engine = "CYCLES"
    gpu(scene)
    scene.cycles.samples = samples
    scene.cycles.use_denoising = True
    scene.render.resolution_x, scene.render.resolution_y = width, height
    scene.render.resolution_percentage = 100
    scene.render.film_transparent = False
    scene.camera = walker
    bpy.ops.render.render()
    image = bpy.data.images["Render Result"]
    settings = scene.render.image_settings
    settings.file_format, settings.color_mode, settings.color_depth = "OPEN_EXR", "RGB", "32"
    image.save_render(str(out.with_suffix(".exr")), scene=scene)
    settings.file_format, settings.color_mode, settings.color_depth = "PNG", "RGB", "8"
    image.save_render(str(out.with_suffix(".png")), scene=scene)
    print(f"truth: {out.with_suffix('.png')} ({scene.view_settings.view_transform})", flush=True)


main()
