# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The first experiment of D-136: a block of Bistro seen from inside, all
that lies outside it seen only as the light coming in through its surface.

    blender -b assets/orca/bistro/exterior.blend --python tools/bistro/incoming.py -- OUT

`exterior.blend` is written by `tools/bistro/prepare.py`. A walker stands
where the scene's own camera is, 1.65 m above the ground, looking as it
looks; its block is the 9 cubes of 3 m around its cube, one layer (D-136;
LAYERS=3 for three). Rendered with Cycles, in OUT:

- `truth`: the whole scene, as the walker sees it;
- `inside`: the same, but what lies outside the block left out of the
  camera's rays (it still casts its shadows and its light): the block as
  the client would trace it;
- `probe-F-A-B`: the light coming in through face F of the 3 m cube
  (A, B) along that face (the world's tile numbers) — a picture from the
  tile's centre of everything out there (a fisheye of the half sphere,
  equidistant), and where each of its pixels lies (its depth); the same
  tile's for any block it bounds;
- `composed`: `inside`, with each of its rays that leaves the block taking
  the light the tiles' probes saw arrive along it: marched along the ray,
  each of the four nearest probes is asked where the ray first passes
  behind what it saw, and those that agree give the colour;
- `naive`: the same with the nearest probe's colour in the ray's direction,
  depth unused;

and `sheet.png`, the four pictures side by side, with the errors printed.
Runs inside Blender (tested with 5.2), numpy as Blender bundles it.
"""

import math
import os
import pathlib
import sys
import time

import bpy
import numpy as np
from mathutils import Matrix, Vector

# OSAGE_QUICK=1: small and noisy, to try the script.
QUICK = os.environ.get("OSAGE_QUICK") == "1"
CELL = 3.0
LAYERS = int(os.environ.get("LAYERS", "1"))
EYE = 1.65
WIDTH, HEIGHT = (240, 135) if QUICK else (960, 540)
FOV = math.radians(75.0)
PROBE = 64 if QUICK else 768
SAMPLES = 4 if QUICK else 64
PROBE_SAMPLES = 2 if QUICK else 32
# Distances marched beyond where a ray leaves the block, m.
STEPS = np.concatenate([[0.0], np.geomspace(0.01, 500.0, 64)])
FACES = ["-x", "+x", "-y", "+y", "-z", "+z"]


def out_folder():
    rest = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    if not rest:
        sys.exit(__doc__)
    out = pathlib.Path(rest[0]).resolve()
    out.mkdir(parents=True, exist_ok=True)
    return out


def setup(scene, samples, width, height, transparent):
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = samples
    scene.cycles.use_denoising = samples > 1
    scene.cycles.filter_width = 1.5 if samples > 1 else 0.01
    scene.render.resolution_x, scene.render.resolution_y = width, height
    scene.render.resolution_percentage = 100
    scene.render.use_persistent_data = True
    scene.render.film_transparent = transparent
    scene.render.image_settings.file_format = "OPEN_EXR"
    scene.render.image_settings.color_mode = "RGBA"
    scene.render.image_settings.color_depth = "32"


def render(scene, camera, path):
    """Renders to `path`, unless OSAGE_REUSE=1 and it is already there."""
    if os.environ.get("OSAGE_REUSE") == "1" and path.exists():
        return
    scene.camera = camera
    scene.render.filepath = str(path)
    started = time.time()
    bpy.ops.render.render(write_still=True)
    print(f"incoming: {path.name} in {time.time() - started:.0f} s", flush=True)


def load(path, width, height):
    image = bpy.data.images.load(str(path))
    pixels = np.empty(width * height * 4, np.float32)
    image.pixels.foreach_get(pixels)
    bpy.data.images.remove(image)
    return pixels.reshape(height, width, 4)


def save(scene, pixels, path):
    """Saves linear pixels through the scene's view transform, as a render is."""
    height, width = pixels.shape[:2]
    image = bpy.data.images.new(path.stem, width, height, alpha=True, float_buffer=True)
    rgba = np.concatenate([pixels[..., :3], np.ones((height, width, 1), np.float32)], -1)
    image.pixels.foreach_set(rgba.astype(np.float32).ravel())
    settings = scene.render.image_settings
    kept = (settings.file_format, settings.color_mode, settings.color_depth)
    settings.file_format, settings.color_mode, settings.color_depth = "PNG", "RGB", "8"
    image.save_render(str(path), scene=scene)
    settings.file_format, settings.color_mode, settings.color_depth = kept
    bpy.data.images.remove(image)


def camera(name, origin, forward, up, fov, width, height):
    """A camera at `origin` looking along `forward`: perspective over `fov`
    across, or (fov None) a fisheye of the half sphere, equidistant."""
    data = bpy.data.cameras.new(name)
    if fov is None:
        data.type = "PANO"
        data.panorama_type = "FISHEYE_EQUIDISTANT"
        data.fisheye_fov = math.radians(200.0)
    else:
        data.sensor_fit = "HORIZONTAL"
        data.angle = fov
    data.clip_start = 0.005
    data.clip_end = 2000.0
    obj = bpy.data.objects.new(name, data)
    bpy.context.scene.collection.objects.link(obj)
    forward, up = Vector(forward).normalized(), Vector(up).normalized()
    right = forward.cross(up).normalized()
    up = right.cross(forward).normalized()
    basis = Matrix((right, up, -forward)).transposed()
    obj.matrix_world = Matrix.Translation(Vector(origin)) @ basis.to_4x4()
    return obj


def outside_hidden(lo, hi):
    """Every material: transparent to the camera's rays outside the block."""
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
                if inside is None:
                    inside = test
                else:
                    both = nodes.new("ShaderNodeMath")
                    both.operation = "MULTIPLY"
                    links.new(inside.outputs[0], both.inputs[0])
                    links.new(test.outputs[0], both.inputs[1])
                    inside = both
        outside = nodes.new("ShaderNodeMath")
        outside.operation = "SUBTRACT"
        outside.inputs[0].default_value = 1.0
        links.new(inside.outputs[0], outside.inputs[1])
        path = nodes.new("ShaderNodeLightPath")
        hidden = nodes.new("ShaderNodeMath")
        hidden.operation = "MULTIPLY"
        links.new(outside.outputs[0], hidden.inputs[0])
        links.new(path.outputs["Is Camera Ray"], hidden.inputs[1])
        clear = nodes.new("ShaderNodeBsdfTransparent")
        mix = nodes.new("ShaderNodeMixShader")
        links.new(hidden.outputs[0], mix.inputs["Fac"])
        links.new(surface, mix.inputs[1])
        links.new(clear.outputs["BSDF"], mix.inputs[2])
        links.new(mix.outputs["Shader"], output.inputs["Surface"])


def position_material():
    """Paints each point with where it is in the world (for depths)."""
    material = bpy.data.materials.new("osage-position")
    tree = material.node_tree
    tree.nodes.clear()
    geometry = tree.nodes.new("ShaderNodeNewGeometry")
    emission = tree.nodes.new("ShaderNodeEmission")
    out = tree.nodes.new("ShaderNodeOutputMaterial")
    tree.links.new(geometry.outputs["Position"], emission.inputs["Color"])
    tree.links.new(emission.outputs["Emission"], out.inputs["Surface"])
    return material


def rays(obj, width, height):
    """Each pixel's ray (rows from the bottom, as Blender stores them)."""
    t = math.tan(obj.data.angle / 2)
    xs = ((np.arange(width) + 0.5) / width * 2 - 1) * t
    ys = ((np.arange(height) + 0.5) / height * 2 - 1) * t * height / width
    x, y = np.meshgrid(xs, ys)
    d = np.stack([x, y, -np.ones_like(x)], -1).reshape(-1, 3)
    d = d @ np.array(obj.matrix_world.to_3x3()).T
    return np.array(obj.matrix_world.translation), d / np.linalg.norm(d, axis=1, keepdims=True)


def exits(o, d, lo, hi):
    """Where each ray from inside the block leaves it: face, point."""
    with np.errstate(divide="ignore", invalid="ignore"):
        t = np.where(d > 0, (hi - o) / d, (lo - o) / d)
    t = np.where(np.abs(d) < 1e-9, np.inf, t)
    axis = np.argmin(t, axis=1)
    rows = np.arange(len(d))
    sign = d[rows, axis] > 0
    return axis * 2 + sign, o + d * t[rows, axis][:, None]


def face_frame(face, lo, hi):
    """A face's outward normal, the two axes along it and its corner."""
    axis, positive = face // 2, face % 2 == 1
    normal = np.zeros(3)
    normal[axis] = 1.0 if positive else -1.0
    a, b = [i for i in range(3) if i != axis]
    corner = lo.copy()
    corner[axis] = hi[axis] if positive else lo[axis]
    return normal, a, b, corner


def tiles(lo, hi, axis):
    """How many tiles a face of the block has along `axis`."""
    return int(round((hi[axis] - lo[axis]) / CELL))


def up_of(normal):
    return np.array([0.0, 0.0, 1.0]) if abs(normal[2]) < 0.5 else np.array([0.0, 1.0, 0.0])


class Probe:
    def __init__(self, origin, normal, colour, position):
        self.origin, self.normal = origin, normal
        up = up_of(normal)
        self.right = np.cross(normal, up)
        self.right /= np.linalg.norm(self.right)
        self.up = np.cross(self.right, normal)
        self.colour, self.position = colour, position

    def project(self, v):
        """Pixel (column, row) of directions `v` from the probe, on its
        equidistant fisheye of the half sphere; in view or not."""
        z = v @ self.normal
        x, y = v @ self.right, v @ self.up
        theta = np.arctan2(np.hypot(x, y), z)
        r = theta / math.radians(100.0)
        phi = np.arctan2(y, x)
        u, w = r * np.cos(phi), r * np.sin(phi)
        seen = (theta <= math.pi / 2) & (np.hypot(u, w) <= 1)
        col = np.clip(((u + 1) / 2 * PROBE).astype(np.int64), 0, PROBE - 1)
        row = np.clip(((w + 1) / 2 * PROBE).astype(np.int64), 0, PROBE - 1)
        return col, row, seen

    def march(self, start, d):
        """For rays leaving the block at `start` along `d`: the colour this
        probe saw where each first passes behind its picture, and whether
        that is the ray's own hit (not something in front of it)."""
        points = start[:, None, :] + d[:, None, :] * STEPS[None, :, None]
        col, row, seen = self.project(points - self.origin)
        where = self.position[row, col]
        sky = where[..., 3] < 0.5
        depth = np.where(sky, np.inf, np.linalg.norm(where[..., :3] - self.origin, axis=-1))
        reach = np.linalg.norm(points - self.origin, axis=-1)
        behind = seen & ~sky & (reach >= depth - 0.01)
        hit = behind.any(axis=1)
        first = np.argmax(behind, axis=1)
        n = np.arange(len(d))
        gap = np.linalg.norm(points[n, first] - where[n, first, :3], axis=-1)
        spacing = STEPS[first] - STEPS[np.maximum(first - 1, 0)]
        tolerance = spacing + 0.02 * STEPS[first] + 0.03
        agrees = hit & (gap <= 2 * tolerance)
        self.closeness = np.where(agrees, np.exp(-((gap / tolerance) ** 2)), 0.0)
        colour = self.colour[row[n, first], col[n, first], :3]
        # Rays that pass behind nothing: the sky, in the ray's own direction.
        c, r, s = self.project(d)
        open_sky = ~hit & s & (self.position[r, c, 3] < 0.5)
        colour = np.where(open_sky[:, None], self.colour[r, c, :3], colour)
        self.closeness = np.where(open_sky, 1.0, self.closeness)
        return colour, agrees | open_sky

    def direction(self, d):
        c, r, s = self.project(d)
        return self.colour[r, c, :3], s


def main():
    out = out_folder()
    scene = bpy.context.scene
    started = time.time()
    view_layer = scene.view_layers[0]
    # The walker: where the scene's camera stands, on the ground.
    first = scene.camera
    depsgraph = bpy.context.evaluated_depsgraph_get()
    hit, ground, *_ = scene.ray_cast(depsgraph, first.matrix_world.translation, Vector((0, 0, -1)))
    standing = Vector(ground) if hit else first.matrix_world.translation - Vector((0, 0, EYE))
    eye = standing + Vector((0, 0, EYE))
    forward = -(first.matrix_world.to_3x3() @ Vector((0, 0, 1)))
    forward.z = 0.0
    walker = camera("walker", eye, forward, (0, 0, 1), FOV, WIDTH, HEIGHT)
    cell = np.floor(np.array(eye) / CELL)
    lo, hi = (cell - 1) * CELL, (cell + 2) * CELL
    if LAYERS == 1:
        lo[2], hi[2] = cell[2] * CELL, (cell[2] + 1) * CELL
    print(f"incoming: eye {tuple(round(v, 2) for v in eye)}, block {lo} to {hi}", flush=True)

    # The truth.
    setup(scene, SAMPLES, WIDTH, HEIGHT, False)
    render(scene, walker, out / "truth.exr")

    # The faces the walker looks out through (its rays that meet nothing in
    # the block, one in seven), and their tiles' probes.
    o, d = rays(walker, WIDTH, HEIGHT)
    face_of, at = exits(o, d, lo, hi)
    counts = np.zeros(6, int)
    for k in range(0, len(d), 7):
        hit, where, *_ = scene.ray_cast(depsgraph, Vector(o), Vector(d[k]))
        if hit and np.all(np.array(where) >= lo - 1e-3) and np.all(np.array(where) <= hi + 1e-3):
            continue
        counts[face_of[k]] += 1
    faces = [f for f in range(6) if counts[f] > 20]
    print(f"incoming: looks out through {[FACES[f] for f in faces]}", flush=True)
    position = position_material()
    cameras = {}
    for face in faces:
        normal, a, b, corner = face_frame(face, lo, hi)
        for i in range(tiles(lo, hi, a)):
            for j in range(tiles(lo, hi, b)):
                centre = corner.copy()
                centre[a] += CELL * (i + 0.5)
                centre[b] += CELL * (j + 0.5)
                at_a, at_b = int(math.floor(centre[a] / CELL)), int(math.floor(centre[b] / CELL))
                plane = int(round(corner[face // 2] / CELL))
                name = f"probe-{FACES[face]}{plane}-{at_a}-{at_b}"
                cam = camera(name, centre + normal * 0.002, normal, up_of(normal), None, PROBE, PROBE)
                cameras[(face, i, j)] = (name, centre, normal, cam)
    # All the colours (the sky drawn), then all the depths (the sky
    # transparent): the scene changes twice only.
    setup(scene, PROBE_SAMPLES, PROBE, PROBE, False)
    for name, _, _, cam in cameras.values():
        render(scene, cam, out / f"{name}.exr")
    setup(scene, 1, PROBE, PROBE, True)
    view_layer.material_override = position
    for name, _, _, cam in cameras.values():
        render(scene, cam, out / f"{name}-position.exr")
    view_layer.material_override = None
    probes = {
        key: Probe(centre, normal, load(out / f"{name}.exr", PROBE, PROBE), load(out / f"{name}-position.exr", PROBE, PROBE))
        for key, (name, centre, normal, _) in cameras.items()
    }

    # The block as the client traces it.
    outside_hidden(lo, hi)
    setup(scene, SAMPLES, WIDTH, HEIGHT, True)
    render(scene, walker, out / "inside.exr")

    truth = load(out / "truth.exr", WIDTH, HEIGHT).reshape(-1, 4)
    inside = load(out / "inside.exr", WIDTH, HEIGHT).reshape(-1, 4)
    leaving = inside[:, 3] < 0.999
    composed = np.zeros((len(d), 3), np.float32)
    naive = np.zeros((len(d), 3), np.float32)
    agreed = np.zeros(len(d), bool)
    for face in faces:
        normal, a, b, corner = face_frame(face, lo, hi)
        mine = np.where(leaving & (face_of == face))[0]
        if len(mine) == 0:
            continue
        # Where on the face, in tiles: the four nearest probes and their weights.
        na, nb = tiles(lo, hi, a), tiles(lo, hi, b)
        fa = np.clip((at[mine, a] - corner[a]) / CELL - 0.5, 0, na - 1)
        fb = np.clip((at[mine, b] - corner[b]) / CELL - 0.5, 0, nb - 1)
        i0, j0 = np.minimum(fa.astype(int), max(na - 2, 0)), np.minimum(fb.astype(int), max(nb - 2, 0))
        ta, tb = (fa - i0) if na > 1 else np.zeros_like(fa), (fb - j0) if nb > 1 else np.zeros_like(fb)
        total = np.zeros((len(mine), 3))
        weight = np.zeros(len(mine))
        for di, dj, w in ((0, 0, (1 - ta) * (1 - tb)), (1, 0, ta * (1 - tb)), (0, 1, (1 - ta) * tb), (1, 1, ta * tb)):
            for i in range(na):
                for j in range(nb):
                    pick = (i0 + di == i) & (j0 + dj == j) & (w > 0)
                    if not pick.any():
                        continue
                    rows = mine[pick]
                    probe = probes[(face, i, j)]
                    colour, agrees = probe.march(at[rows], d[rows])
                    trust = w[pick] * probe.closeness
                    weight[pick] += trust
                    total[pick] += trust[:, None] * colour
        nearest = (np.rint(fa).astype(int), np.rint(fb).astype(int))
        for i in range(na):
            for j in range(nb):
                pick = (nearest[0] == i) & (nearest[1] == j)
                if pick.any():
                    colour, _ = probes[(face, i, j)].direction(d[mine[pick]])
                    naive[mine[pick]] = colour
        good = weight > 1e-6
        composed[mine[good]] = total[good] / weight[good][:, None]
        composed[mine[~good]] = naive[mine[~good]]
        agreed[mine[good]] = True

    keep = 1.0 - inside[:, 3:4]
    final = inside[:, :3] + keep * composed
    plain = inside[:, :3] + keep * naive

    def shown(x):
        x = np.clip(x, 0, 1)
        return np.where(x <= 0.0031308, 12.92 * x, 1.055 * np.power(x, 1 / 2.4) - 0.055)

    def psnr(x, region=None):
        e = (shown(x) - shown(truth[:, :3])) ** 2
        if region is not None:
            e = e[region]
        return 10 * math.log10(1.0 / max(float(e.mean()), 1e-12))

    out_region = inside[:, 3] < 0.5
    report = (
        f"rays leaving the block: {100 * leaving.mean():.0f} % of the picture; "
        f"of them, the probes agreed on {100 * agreed[leaving].mean():.0f} %\n"
        f"composed against the truth: {psnr(final):.1f} dB (beyond the block: {psnr(final, out_region):.1f} dB)\n"
        f"naive against the truth: {psnr(plain):.1f} dB (beyond the block: {psnr(plain, out_region):.1f} dB)\n"
        f"probes: {len(probes)} fisheyes of the half sphere, {PROBE} px; "
        f"all done in {time.time() - started:.0f} s"
    )
    print("incoming: " + report.replace("\n", "\nincoming: "), flush=True)
    (out / "report.txt").write_text(report + "\n")
    image = lambda x: x.reshape(HEIGHT, WIDTH, 3)
    for name, x in (("truth", truth[:, :3]), ("inside", inside[:, :3]), ("composed", final), ("naive", plain)):
        save(scene, np.concatenate([image(x), np.ones((HEIGHT, WIDTH, 1))], -1), out / f"{name}.png")
    # Rows from the bottom: the inside and the naive one below, the truth
    # and the composed one above.
    sheet = np.concatenate(
        [
            np.concatenate([image(inside[:, :3]), image(plain)], axis=1),
            np.concatenate([image(truth[:, :3]), image(final)], axis=1),
        ],
        axis=0,
    )
    save(scene, np.concatenate([sheet, np.ones(sheet.shape[:2] + (1,))], -1), out / "sheet.png")


main()
