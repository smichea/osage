#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy", "pillow"]
# ///

"""Bakes a scene too heavy for the GPU to draw at once (issue #8): its
places in groups, each group photographed from every view of the scene's
grid with the same framing, then the groups merged view by view, each pixel
from the group whose surface stands nearest the camera.

    uv run tools/hologram-groups.py SCENE OUT GROUPS [SIZE]

SCENE is a scene file (`osage_hologram::Scene`, as `woods_holograms` writes
them), OUT the folder of the hologram's views, GROUPS how many groups,
SIZE the views' side in pixels (default 1024). The views are baked on black
(`HOLOGRAM_KEY=black`): what a pixel shows is told by its depth picture.

A forest of real trees, needles modelled, is about a billion faces a chunk:
a laptop's integrated GPU loses itself drawing that in one frame, and draws
it in groups of a hundred million or so. A hologram's geometry can be as
detailed as wanted (D-133); this is how it is baked where the GPU is small.
"""

import json
import os
import pathlib
import shutil
import subprocess
import sys

import numpy as np
from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parent.parent
BAKE = ROOT / "target" / "release" / "hologram-bake"
KINDS = ("view", "albedo", "normal", "depth")


def bake(scene: pathlib.Path, out: pathlib.Path, size: int, env: dict) -> None:
    command = [str(BAKE), str(out), "", str(size)]
    if "frame" in env:
        command.append(env.pop("frame"))
    full = {**os.environ, "HOLOGRAM_KEY": "black", "HOLOGRAM_SCENE": str(scene), **env}
    logs = ROOT / "target" / "hologram-groups"
    logs.mkdir(parents=True, exist_ok=True)
    with open(logs / f"{out.parent.name}-{out.name}.log", "w") as log:
        subprocess.run(command, env=full, stdout=log, stderr=subprocess.STDOUT, check=True)


def merge(groups: list[pathlib.Path], out: pathlib.Path) -> int:
    """Each view of the groups merged into `out`, by depth. How many views."""
    names = sorted(p.name[len("depth-") :] for p in groups[0].glob("depth-*.png"))
    for name in names:
        depths = [np.asarray(Image.open(g / f"depth-{name}")) for g in groups]
        # Towards the camera, in 16 bits; -1 where the group saw nothing.
        near = np.stack(
            [np.where(d[..., 2] > 127, d[..., 0].astype(np.int32) * 256 + d[..., 1], -1) for d in depths]
        )
        winner = near.argmax(axis=0)
        for kind in KINDS:
            pictures = [Image.open(g / f"{kind}-{name}") for g in groups]
            mode = pictures[0].mode
            stack = np.stack([np.asarray(p.convert(mode)) for p in pictures])
            merged = np.take_along_axis(stack, winner[None, ..., None], axis=0)[0]
            Image.fromarray(merged, mode).save(out / f"{kind}-{name}")
    return len(names)


def main() -> int:
    if len(sys.argv) < 4:
        print(__doc__)
        return 2
    scene_file, out, groups = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), int(sys.argv[3])
    size = int(sys.argv[4]) if len(sys.argv) > 4 else 1024
    scene = json.loads(scene_file.read_text())
    out.mkdir(parents=True, exist_ok=True)
    # The framing of the whole scene, written and kept.
    bake(scene_file, out, size, {"HOLOGRAM_FRAME_ONLY": "1"})
    framing = json.loads((out / "hologram.json").read_text())
    print(f"framed: radius {framing['radius']:.1f} m, aspect {framing['aspect']:.2f}", flush=True)
    work = out / "groups"
    folders = []
    for g in range(groups):
        # Every GROUPS-th place of each model: the groups spread over the
        # whole scene alike.
        part = dict(scene, models=[dict(m, places=m["places"][g::groups]) for m in scene["models"]])
        part["models"] = [m for m in part["models"] if m["places"]]
        folder = work / f"g{g}"
        folder.mkdir(parents=True, exist_ok=True)
        (folder / "scene.json").write_text(json.dumps(part))
        frame = ",".join(f"{v}" for v in [*framing["centre"], framing["radius"]])
        # The GPU may still lose itself near its limit: a second try.
        # (A lost device quits without an error: the framing is written last.)
        for attempt in range(2):
            if attempt:
                print(f"group {g}: lost, tried again", flush=True)
            bake(folder / "scene.json", folder, size, {"frame": frame, "HOLOGRAM_ASPECT": str(framing["aspect"])})
            if (folder / "hologram.json").exists():
                break
        if not (folder / "hologram.json").exists():
            print(f"group {g}: the bake failed, see target/hologram-groups/", file=sys.stderr)
            return 1
        print(f"group {g} of {groups} baked", flush=True)
        folders.append(folder)
    views = merge(folders, out)
    shutil.rmtree(work)
    print(f"{views} views merged from {groups} groups into {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
