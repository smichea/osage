# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Blender's own AgX view transform, as a table a client tonemaps with, so
that the Paris client shows radiance as Blender shows Cycles' (research 10).

    blender -b --factory-startup --python tools/bistro/agx-lut.py -- OUT

OUT receives 64 x 64 x 64 entries, red fastest, then green, then blue, each
four half floats (little endian): the light an sRGB display gives (the sRGB
curve undone) for the scene-linear Rec.709 stimulus 0.18 * 2^(n * 23 - 11)
in each band, n = index / 63: the domain over which Bevy reads its Blender
Filmic table, from 11 stops under mid-grey to 12 over. Bevy's own AgX is Sobotka's
first; Blender's (4.0 and on) is another, brighter-contrasted one: under
the same light the two differ by a fifth in the mid-tones.
"""

import pathlib
import sys
import tempfile

import bpy
import numpy as np

N = 64


def main():
    rest = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    if not rest:
        sys.exit(__doc__)
    out = pathlib.Path(rest[0]).resolve()
    out.parent.mkdir(parents=True, exist_ok=True)
    scene = bpy.context.scene
    view = scene.view_settings
    view.view_transform, view.look, view.exposure, view.gamma = "AgX", "None", 0.0, 1.0
    scene.display_settings.display_device = "sRGB"
    stimulus = 0.18 * 2.0 ** (np.arange(N) / (N - 1) * 23.0 - 11.0)
    # Rows (blue, green), columns red: red fastest.
    blue, green, red = np.meshgrid(stimulus, stimulus, stimulus, indexing="ij")
    pixels = np.ones((N * N, N, 4), np.float32)
    pixels[..., 0] = red.reshape(N * N, N)
    pixels[..., 1] = green.reshape(N * N, N)
    pixels[..., 2] = blue.reshape(N * N, N)
    image = bpy.data.images.new("stimulus", width=N, height=N * N, alpha=False, float_buffer=True)
    image.colorspace_settings.name = "Linear Rec.709"
    image.pixels.foreach_set(pixels.ravel())
    settings = scene.render.image_settings
    settings.file_format, settings.color_mode, settings.color_depth = "PNG", "RGB", "16"
    with tempfile.TemporaryDirectory() as folder:
        shown = pathlib.Path(folder) / "shown.png"
        image.save_render(str(shown), scene=scene)
        back = bpy.data.images.load(str(shown))
        back.colorspace_settings.name = "Non-Color"
        values = np.empty(N * N * N * 4, np.float32)
        back.pixels.foreach_get(values)
    encoded = values.reshape(N * N, N, 4)[..., :3]
    light = np.where(encoded <= 0.04045, encoded / 12.92, ((encoded + 0.055) / 1.055) ** 2.4)
    table = np.concatenate([light, np.ones(light.shape[:-1] + (1,), np.float32)], axis=-1)
    out.write_bytes(table.astype("<f2").tobytes())
    grey = int(round(11.0 / 23.0 * (N - 1)))
    entry = table.reshape(N, N, N, 4)[grey, grey, grey]
    print(f"agx-lut: {out} ({N}^3); stimulus {stimulus[grey]:.3f} shows as {entry[0]:.3f} of white", flush=True)


main()
