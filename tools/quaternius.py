#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Takes the people of the asset manifest out of Quaternius's two zips.

    python3 tools/quaternius.py

Quaternius publishes its packs on itch.io, downloaded by a person (the
manifest's `manual` entries): put `Universal Base Characters[Standard].zip`
and `Universal Animation Library[Standard].zip` in assets/quaternius/. This
copies out of them what osage-humans loads, at the paths the manifest lists:

    assets/quaternius/humans/   two bodies (glTF), their textures, the light skins
    assets/quaternius/hair/     hairstyles and eyebrows rigged to the same skeleton
    assets/quaternius/moves/    the animation library (glTF binary), same skeleton

Both packs are CC0 1.0. Standard library only.
"""

import pathlib
import sys
import zipfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
HERE = ROOT / "assets" / "quaternius"
CHARACTERS = "Universal Base Characters[Standard].zip"
ANIMATIONS = "Universal Animation Library[Standard].zip"

# (folder inside the zip, file names wanted or None for all, where they go)
TAKEN = [
    (CHARACTERS, "Universal Base Characters[Standard]/Base Characters/Godot - UE/", None, "humans"),
    (
        CHARACTERS,
        "Universal Base Characters[Standard]/Base Characters/Textures/",
        {"T_Superhero_Male_Ligh.png", "T_Superhero_Female_Light_BaseColor.png"},
        "humans",
    ),
    (CHARACTERS, "Universal Base Characters[Standard]/Hairstyles/Rigged to Head Bone/glTF (Godot -Unreal)/", None, "hair"),
    (CHARACTERS, "Universal Base Characters[Standard]/License_Standard.txt", None, "humans"),
    (ANIMATIONS, "Universal Animation Library[Standard]/Unreal-Godot/", {"UAL1_Standard.glb"}, "moves"),
    (ANIMATIONS, "Universal Animation Library[Standard]/License.txt", None, "moves"),
]

# (folder, file present, name the glTF asks for)
ALIASES = [
    ("humans", "T_Eye_Normal.png", "T_Eye_Normal_png.png"),
    ("humans", "T_Hair_1_Normal.png", "T_Hair_1_Normal_png.png"),
]


def main() -> int:
    for name in (CHARACTERS, ANIMATIONS):
        if not (HERE / name).exists():
            print(f"missing {HERE / name}: download it from Quaternius's itch.io page (assets/manifest.toml)")
            return 1
    written = 0
    for archive, prefix, wanted, folder in TAKEN:
        with zipfile.ZipFile(HERE / archive) as zipped:
            for info in zipped.infolist():
                if info.is_dir() or not info.filename.startswith(prefix):
                    continue
                name = pathlib.PurePosixPath(info.filename).name
                # Only plain names: nothing from a zip lands outside its folder.
                if wanted is not None and name not in wanted or "/" in name or name.startswith("."):
                    continue
                target = HERE / folder / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(zipped.read(info))
                written += 1
    # The bodies' glTF name two textures the zip holds under other names.
    for folder, name, alias in ALIASES:
        (HERE / folder / alias).write_bytes((HERE / folder / name).read_bytes())
        written += 1
    print(f"{written} files written under {HERE.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
