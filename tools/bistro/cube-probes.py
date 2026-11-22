# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Photographs the light coming into a walker's block from inside the
walker's cube (research 10): a trial against the probes on the block's
surface, which see objects near the block from angles too unlike the eye's.

    blender -b assets/orca/bistro/exterior.blend --python tools/bistro/cube-probes.py -- X,Y,Z OUT [WIDTH] [SAMPLES]

X,Y,Z is the cube (3 m, the glTF's frame: x east, y up, z south). Its block
is the 9 cubes around it, one layer; Cycles renders the whole street, but
the block's surfaces are transparent to the camera's rays (they still cast
their shadows and their light): each probe sees what comes into the block.
Five probes at the eyes' height (1.65 m over the ground under them): the
cube's centre, and 0.75 m from it towards each corner. Each is a full
sphere, equirectangular, WIDTH x WIDTH/2 pixels (4096 unless given), SAMPLES
samples a pixel (256 unless given): column 0 looks south and the middle one
north (-z), east to the right of it; row 0 straight up. In OUT/c_X_Y_Z_K/:
`radiance.f32`, `distance.f32` (as `tiles.py` writes them) and `meta.json`.
A cube given as X,Y,Z:x,y,z has its centre probe's eyes at x,y,z (the walk's
grid knows where one stands; tools/bistro/probe-spots.py lists them).
OSAGE_DEVICE=OPTIX (or CUDA) renders on the GPU; OSAGE_PROBES=N bakes the
first N probes only (1: the cube's centre alone); OSAGE_ALBEDO=1 renders
instead what colour each surface is, apart from its light (`albedo.f32`, one
sample: a client that draws the surfaces' own textures takes the probes'
light alone from the two). Checks its own projection
against where each pixel's point lies, and says by how much it is off.
"""

import json
import math
import os
import pathlib
import sys
import time

import bpy
import numpy as np
from mathutils import Matrix, Vector

CUBE = 3.0
EYES = 1.65
OFFSETS = [(0.0, 0.0), (-0.75, -0.75), (0.75, -0.75), (-0.75, 0.75), (0.75, 0.75)]


def blender(v):
    """A point or direction of the glTF's frame (y up) in Blender's (z up)."""
    return Vector((v[0], -v[2], v[1]))


def device(scene):
    kind = os.environ.get("OSAGE_DEVICE")
    if not kind:
        scene.cycles.device = "CPU"
        return
    prefs = bpy.context.preferences.addons["cycles"].preferences
    prefs.compute_device_type = kind
    prefs.refresh_devices()
    used = 0
    for d in prefs.devices:
        d.use = d.type == kind
        used += d.use
    if not used:
        sys.exit(f"cube-probes: no {kind} device")
    scene.cycles.device = "GPU"


def inside_hidden(lo, hi):
    """Every material: transparent to the camera's rays inside the block
    (Blender's frame, lo..hi; from lo's height up: the ground below stays,
    or the probes would see under the street). Tells the nodes holding the
    block's bounds, for another block to be hidden instead (`hide`)."""
    bounds = []
    for material in bpy.data.materials:
        tree = material.node_tree
        if tree is None:
            continue
        output = next((n for n in tree.nodes if n.bl_idname == "ShaderNodeOutputMaterial"), None)
        if output is None or not output.inputs["Surface"].links:
            continue
        surface = output.inputs["Surface"].links[0].from_socket
        nodes, links = tree.nodes, tree.links
        geometry = nodes.new("ShaderNodeNewGeometry")
        split = nodes.new("ShaderNodeSeparateXYZ")
        links.new(geometry.outputs["Position"], split.inputs["Vector"])
        inside = None
        for axis, name in enumerate("XYZ"):
            for bound, op in ((lo[axis], "GREATER_THAN"), (hi[axis], "LESS_THAN")):
                test = nodes.new("ShaderNodeMath")
                test.operation = op
                links.new(split.outputs[name], test.inputs[0])
                test.inputs[1].default_value = bound
                bounds.append((test, axis, op == "LESS_THAN"))
                if inside is None:
                    inside = test
                else:
                    both = nodes.new("ShaderNodeMath")
                    both.operation = "MULTIPLY"
                    links.new(inside.outputs[0], both.inputs[0])
                    links.new(test.outputs[0], both.inputs[1])
                    inside = both
        path = nodes.new("ShaderNodeLightPath")
        hidden = nodes.new("ShaderNodeMath")
        hidden.operation = "MULTIPLY"
        links.new(inside.outputs[0], hidden.inputs[0])
        links.new(path.outputs["Is Camera Ray"], hidden.inputs[1])
        clear = nodes.new("ShaderNodeBsdfTransparent")
        mix = nodes.new("ShaderNodeMixShader")
        links.new(hidden.outputs[0], mix.inputs["Fac"])
        links.new(surface, mix.inputs[1])
        links.new(clear.outputs["BSDF"], mix.inputs[2])
        links.new(mix.outputs["Shader"], output.inputs["Surface"])
    return bounds


def hide(bounds, lo, hi):
    """Another block transparent to the camera's rays."""
    for test, axis, high in bounds:
        test.inputs[1].default_value = (hi if high else lo)[axis]


def position_material():
    """Paints each point with where it is in the world (for distances)."""
    material = bpy.data.materials.new("osage-position")
    tree = material.node_tree
    tree.nodes.clear()
    geometry = tree.nodes.new("ShaderNodeNewGeometry")
    emission = tree.nodes.new("ShaderNodeEmission")
    out = tree.nodes.new("ShaderNodeOutputMaterial")
    tree.links.new(geometry.outputs["Position"], emission.inputs["Color"])
    tree.links.new(emission.outputs["Emission"], out.inputs["Surface"])
    return material


def albedo_materials():
    """Every material shining its own base colour, lit by nothing: what a
    surface is, apart from the light on it."""
    for material in bpy.data.materials:
        tree = material.node_tree
        if tree is None:
            continue
        output = next((n for n in tree.nodes if n.bl_idname == "ShaderNodeOutputMaterial"), None)
        principled = next((n for n in tree.nodes if n.bl_idname == "ShaderNodeBsdfPrincipled"), None)
        if output is None or principled is None:
            continue
        nodes, links = tree.nodes, tree.links
        glow = nodes.new("ShaderNodeEmission")
        base = principled.inputs["Base Color"]
        if base.links:
            links.new(base.links[0].from_socket, glow.inputs["Color"])
        else:
            glow.inputs["Color"].default_value = base.default_value
        shader = glow.outputs[0]
        alpha = principled.inputs["Alpha"]
        if alpha.links:
            # Leaves and railings: seen through where they are cut out.
            clear = nodes.new("ShaderNodeBsdfTransparent")
            mix = nodes.new("ShaderNodeMixShader")
            links.new(alpha.links[0].from_socket, mix.inputs["Fac"])
            links.new(clear.outputs["BSDF"], mix.inputs[1])
            links.new(shader, mix.inputs[2])
            shader = mix.outputs["Shader"]
        links.new(shader, output.inputs["Surface"])


def panorama(name, origin, width):
    """A full sphere from `origin`, looking north (-z), up the sky."""
    data = bpy.data.cameras.new(name)
    data.type = "PANO"
    data.panorama_type = "EQUIRECTANGULAR"
    data.latitude_min, data.latitude_max = -math.pi / 2, math.pi / 2
    data.longitude_min, data.longitude_max = -math.pi, math.pi
    data.clip_start, data.clip_end = 0.005, 3000.0
    obj = bpy.data.objects.new(name, data)
    bpy.context.scene.collection.objects.link(obj)
    forward, up = blender((0.0, 0.0, -1.0)), blender((0.0, 1.0, 0.0))
    right = forward.cross(up).normalized()
    obj.matrix_world = Matrix.Translation(origin) @ Matrix((right, up, -forward)).transposed().to_4x4()
    return obj


def setup(scene, width, samples, transparent):
    scene.render.engine = "CYCLES"
    device(scene)
    scene.cycles.samples = samples
    scene.cycles.use_denoising = samples > 1
    scene.cycles.filter_width = 1.5 if samples > 1 else 0.01
    scene.render.resolution_x, scene.render.resolution_y = width, width // 2
    scene.render.resolution_percentage = 100
    scene.render.use_persistent_data = True
    scene.render.film_transparent = transparent
    settings = scene.render.image_settings
    settings.file_format, settings.color_mode, settings.color_depth = "OPEN_EXR", "RGBA", "32"


def render(scene, camera, path):
    scene.camera = camera
    scene.render.filepath = str(path)
    bpy.ops.render.render(write_still=True)


def load(path, width, height):
    image = bpy.data.images.load(str(path))
    pixels = np.empty(width * height * 4, np.float32)
    image.pixels.foreach_get(pixels)
    bpy.data.images.remove(image)
    # Blender's rows go up from the bottom; the probe's go down from the top.
    return pixels.reshape(height, width, 4)[::-1]


def directions(width, height):
    """Each pixel's direction in the glTF's frame, as the client reads them."""
    u = (np.arange(width) + 0.5) / width
    v = (np.arange(height) + 0.5) / height
    lon = (u - 0.5) * 2.0 * math.pi
    lat = (0.5 - v) * math.pi
    lon, lat = np.meshgrid(lon, lat)
    return np.stack([np.cos(lat) * np.sin(lon), np.sin(lat), -np.cos(lat) * np.cos(lon)], axis=-1)


def eyes(scene, depsgraph, cube, spots, dx, dz):
    """Where a probe's eyes stand (glTF's frame), `dx`, `dz` from the cube's
    centre probe: EYES over the ground under them. The centre probe stands
    at the cube's given spot, or over the cube's middle; the ground is looked
    for from the spot's eyes down, under what hangs over a walker."""
    spot = spots.get(tuple(cube))
    if spot and dx == 0.0 and dz == 0.0:
        return spot
    x = (spot[0] if spot else (cube[0] + 0.5) * CUBE) + dx
    z = (spot[2] if spot else (cube[2] + 0.5) * CUBE) + dz
    top = blender((x, spot[1] if spot else (cube[1] + 1) * CUBE - 0.01, z))
    hit, ground, *_ = scene.ray_cast(depsgraph, top, Vector((0, 0, -1)))
    return (x, (ground.z if hit else cube[1] * CUBE) + EYES, z)


def main():
    rest = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    if len(rest) < 2:
        sys.exit(__doc__)
    # X,Y,Z:x,y,z: the eyes of the cube's centre probe stand at x,y,z, a
    # place the walk's grid holds; a cube's middle may be inside a wall.
    cubes, spots = [], {}
    for one in rest[0].split(";"):
        where, _, at = one.partition(":")
        cubes.append([int(c) for c in where.split(",")])
        if at:
            spots[tuple(cubes[-1])] = tuple(float(v) for v in at.split(","))
    cube = cubes[0]
    out = pathlib.Path(rest[1]).resolve()
    width = int(rest[2]) if len(rest) > 2 else 4096
    height = width // 2
    samples = int(rest[3]) if len(rest) > 3 else 256
    scene = bpy.context.scene
    layer = scene.view_layers[0]
    lo_g = [(cube[0] - 1) * CUBE, cube[1] * CUBE, (cube[2] - 1) * CUBE]
    hi_g = [(cube[0] + 2) * CUBE, (cube[1] + 1) * CUBE, (cube[2] + 2) * CUBE]
    lo = Vector((lo_g[0], -hi_g[2], lo_g[1]))
    hi = Vector((hi_g[0], -lo_g[2], hi_g[1]))
    # The street, its pavements and kerbs stay: hidden from 30 cm over the
    # ground under the cube's centre.
    depsgraph = bpy.context.evaluated_depsgraph_get()
    lo.z = eyes(scene, depsgraph, cube, spots, 0.0, 0.0)[1] - EYES + 0.3
    # Made first, so that it is transparent inside the block too.
    position = position_material()
    albedo = os.environ.get("OSAGE_ALBEDO")
    if albedo:
        albedo_materials()
    bounds = inside_hidden(lo, hi)
    if albedo:
        # The surfaces' colours take a sample: several cubes (X,Y,Z;X,Y,Z…)
        # in one go, the scene loaded once.
        setup(scene, width, 1, False)
        scene.view_settings.view_transform = "Standard"
        work = out / ".work"
        work.mkdir(parents=True, exist_ok=True)
        started = time.time()
        for cube in cubes:
            lo_g = [(cube[0] - 1) * CUBE, cube[1] * CUBE, (cube[2] - 1) * CUBE]
            hi_g = [(cube[0] + 2) * CUBE, (cube[1] + 1) * CUBE, (cube[2] + 2) * CUBE]
            lo, hi = Vector((lo_g[0], -hi_g[2], lo_g[1])), Vector((hi_g[0], -lo_g[2], hi_g[1]))
            depsgraph = bpy.context.evaluated_depsgraph_get()
            lo.z = eyes(scene, depsgraph, cube, spots, 0.0, 0.0)[1] - EYES + 0.3
            hide(bounds, lo, hi)
            for k, (dx, dz) in enumerate(OFFSETS[: int(os.environ.get("OSAGE_PROBES", len(OFFSETS)))]):
                key = f"c_{cube[0]}_{cube[1]}_{cube[2]}_{k}"
                folder = out / key
                if (folder / "albedo.f32").exists():
                    continue
                centre = eyes(scene, depsgraph, cube, spots, dx, dz)
                cam = panorama(key, blender(centre), width)
                render(scene, cam, work / f"{key}-albedo.exr")
                folder.mkdir(parents=True, exist_ok=True)
                load(work / f"{key}-albedo.exr", width, height)[..., :3].astype("<f4").tofile(folder / "albedo.f32")
                meta = {"width": width, "height": height, "key": key, "centre": list(centre), "cube": cube}
                (folder / "meta.json").write_text(json.dumps(meta, indent=1) + "\n")
                (work / f"{key}-albedo.exr").unlink()
                print(f"cube-probes: {key}: its surfaces' colours, {time.time() - started:.0f} s on", flush=True)
        return
    depsgraph = bpy.context.evaluated_depsgraph_get()
    probes = []
    for k, (dx, dz) in enumerate(OFFSETS[: int(os.environ.get("OSAGE_PROBES", len(OFFSETS)))]):
        centre = eyes(scene, depsgraph, cube, spots, dx, dz)
        key = f"c_{cube[0]}_{cube[1]}_{cube[2]}_{k}"
        probes.append((key, centre, panorama(key, blender(centre), width)))
    work = out / ".work"
    work.mkdir(parents=True, exist_ok=True)
    started = time.time()
    setup(scene, width, samples, False)
    layer.material_override = None
    for key, _, cam in probes:
        if not (out / key / "distance.f32").exists():
            render(scene, cam, work / f"{key}.exr")
    # Where each pixel lies.
    setup(scene, width, 1, True)
    layer.material_override = position
    for key, _, cam in probes:
        if not (out / key / "distance.f32").exists():
            render(scene, cam, work / f"{key}-position.exr")
    layer.material_override = None
    rays = directions(width, height)
    for key, centre, _ in probes:
        folder = out / key
        if (folder / "distance.f32").exists():
            continue
        colour = load(work / f"{key}.exr", width, height)
        where = load(work / f"{key}-position.exr", width, height)
        origin = np.array(blender(centre))
        offset = where[..., :3] - origin
        distance = np.linalg.norm(offset, axis=-1)
        solid = where[..., 3] >= 0.5
        distance[~solid] = np.inf
        # The projection checked: the direction to each point, back in the
        # glTF's frame, against the pixel's.
        seen = np.stack([offset[..., 0], offset[..., 2], -offset[..., 1]], axis=-1)
        ok = solid & (distance > 0.5)
        unit = seen[ok] / distance[ok][:, None]
        off = np.degrees(np.arccos(np.clip((unit * rays[ok]).sum(-1), -1.0, 1.0)))
        folder.mkdir(parents=True, exist_ok=True)
        colour[..., :3].astype("<f4").tofile(folder / "radiance.f32")
        meta = {"width": width, "height": height, "samples": samples, "projection": "equirect", "key": key,
                "centre": list(centre), "cube": cube}
        (folder / "meta.json").write_text(json.dumps(meta, indent=1) + "\n")
        distance.astype("<f4").tofile(folder / "distance.f32")
        (work / f"{key}.exr").unlink()
        (work / f"{key}-position.exr").unlink()
        if off.size == 0:
            # Nothing further than half a metre: the probe stands inside a mesh.
            print(f"cube-probes: {key} at {tuple(round(c, 2) for c in centre)} sees nothing", flush=True)
            continue
        print(f"cube-probes: {key} at {tuple(round(c, 2) for c in centre)}; projection off by "
              f"{np.median(off):.2f} deg (median), {np.percentile(off, 99):.2f} (99th)", flush=True)
    print(f"cube-probes: {len(probes)} probes in {time.time() - started:.0f} s", flush=True)


if __name__ == "__main__":
    main()
