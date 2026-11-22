#!/usr/bin/env bash
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
# Packs what the Paris walk reads into the two zips a paris-v release
# carries beside its programs (paris-update unpacks them over the folder):
#
# - osage-paris-street.zip: Amazon Lumberyard's Bistro exported for the
#   client, its walk grid, its sky, Blender's AgX table and the manifest.
#   It does not change: it is made only if OUT_DIR has none (or with
#   --street), so that every release carries the same file and no folder
#   downloads it twice.
# - osage-paris-light.zip: the light baked into the street's blocks (the
#   probes and tiles of out/tiles). Made anew each time.
#
#   tools/bistro/content-zip.sh OUT_DIR [--street]
#
# Run from the repository's root, on a machine that has the content.
set -euo pipefail
out=${1:?usage: content-zip.sh OUT_DIR [--street]}
again=${2:-}
mkdir -p "$out"
out=$(cd "$out" && pwd)
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
street=orca/bistro/out
sky=orca/bistro/src/Bistro_v5_2/san_giuseppe_bridge_4k.hdr

# pack NAME: zips $stage/NAME/osage-paris. Textures and AV1 pictures are
# compressed already: the fastest level.
pack() {
  rm -f "$out/osage-paris-$1.zip"
  (cd "$stage/$1" && zip -q -r -1 "$out/osage-paris-$1.zip" osage-paris)
  ls -l "$out/osage-paris-$1.zip"
}

if [ "$again" = --street ] || [ ! -f "$out/osage-paris-street.zip" ]; then
  root=$stage/street/osage-paris/assets
  mkdir -p "$root/$street" "$root/$(dirname $sky)" "$root/blender"
  for f in bistro.gltf bistro.bin walk.json walk.bin textures; do
    [ -e "assets/$street/$f" ] || { echo "assets/$street/$f is not here" >&2; exit 1; }
    ln -s "$PWD/assets/$street/$f" "$root/$street/$f"
  done
  ln -s "$PWD/assets/$sky" "$root/$sky"
  ln -s "$PWD/assets/blender/agx-64.bin" "$root/blender/agx-64.bin"
  cp assets/manifest.toml "$root/"
  cat > "$root/orca/bistro/ATTRIBUTION.txt" <<'TEXT'
The street in this folder is "Amazon Lumberyard Bistro, Open Research
Content Archive (ORCA), 2017", by Amazon Lumberyard, published by NVIDIA:
https://developer.nvidia.com/orca/amazon-lumberyard-bistro

It is under the Creative Commons Attribution 4.0 International licence
(CC BY 4.0): https://creativecommons.org/licenses/by/4.0/

Changes, by the osage contributors (https://github.com/smichea/osage):
the exterior exported to glTF for the client (out/bistro.gltf, bistro.bin,
textures), a grid of where one may walk (out/walk.json, walk.bin), and the
light coming into each block of the street, rendered from it by Blender's
Cycles and packed as AV1 pictures (out/tiles). These are under CC BY 4.0
as well.
TEXT
  pack street
else
  echo "osage-paris-street.zip kept as it is"
fi

[ -f "assets/$street/tiles/index.json" ] || { echo "assets/$street/tiles/index.json is not here" >&2; exit 1; }
mkdir -p "$stage/light/osage-paris/assets/$street"
ln -s "$PWD/assets/$street/tiles" "$stage/light/osage-paris/assets/$street/tiles"
pack light
sha256sum "$out"/osage-paris-street.zip "$out"/osage-paris-light.zip
