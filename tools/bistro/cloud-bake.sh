#!/usr/bin/env bash
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Bakes tiles of light (D-136) on a cloud GPU, renders there the picture a
# client is measured against, and draws the client there at T4 and T3.
#
#     tools/bistro/cloud-bake.sh PROJECT TILES.json OUT [SIZE [SAMPLES]]
#
# PROJECT is the render bench's (tools/cloud-bench/setup.sh): its bucket,
# its network, its `osage-bench` image (NVIDIA driver, Vulkan) and its
# `bench-vm` account. TILES lists the tiles, as `paris-tiles` does. A
# g2-standard-8 machine (one NVIDIA L4) runs on-vm-bake.sh and is deleted
# when this script exits, or by Google after three hours if this script
# dies first. Blender, the scene, the street and the source go up from
# this machine (OSAGE_BAKE_REUSE=gs://…/RUN: Blender, the scene and the
# street copied from an earlier run instead; OSAGE_BAKE_PACKED=DIR: those
# tiles, packed already, and no bake, only the shots; OSAGE_BAKE_CUBE=X,Y,Z:
# that cube's probes baked instead, `cube-probes.py`, left raw in the
# bucket as RUN/raw.tar, OSAGE_BAKE_PROBES=N of them, 1 its centre's alone,
# and with OSAGE_BAKE_PACK=1 packed there too, brought back as OUT/tiles; OSAGE_BAKE_FILM=SECONDS: a walk down the street
# filmed at T4 besides, `film-T4.mp4`; OSAGE_BAKE_VARIANTS=1: T4 drawn plain
# with Solari's settings one by one); the packed tiles, the shots and the
# logs come back to OUT.

set -euo pipefail

project=${1:?project}
tiles=${2:?tiles}
out=${3:?output directory}
size=${4:-2048}
samples=${5:-256}
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
id="$(date -u +%Y%m%d-%H%M%S)-bake"
run="gs://$project-runs/$id"
name="osage-bake-$id"
g() { gcloud --project "$project" --quiet "$@"; }

zone=
delete() {
  [ -n "$zone" ] && g compute instances delete "$name" --zone "$zone" >/dev/null 2>&1 || true
}
trap delete EXIT

# What goes up. Blender, the scene and the street are made once and kept.
stage=$root/target/cloud-bake
mkdir -p "$stage"
blender_dir=${OSAGE_BLENDER_DIR:-$HOME/.local/opt/blender-5.2.4-linux-x64}
[ -f "$stage/blender.tar.zst" ] ||
  tar -C "$(dirname "$blender_dir")" -I 'zstd -T0 -3' -cf "$stage/blender.tar.zst" "$(basename "$blender_dir")"
[ -f "$stage/scene.tar" ] ||
  tar -C "$root/assets/orca" -cf "$stage/scene.tar" bistro/exterior.blend bistro/src/Bistro_v5_2/Textures \
    bistro/src/Bistro_v5_2/san_giuseppe_bridge_4k.hdr
[ -f "$stage/street.tar" ] ||
  tar -C "$root/assets" -cf "$stage/street.tar" orca/bistro/out/bistro.gltf orca/bistro/out/bistro.bin \
    orca/bistro/out/textures orca/bistro/out/walk.json orca/bistro/out/walk.bin \
    orca/bistro/src/Bistro_v5_2/san_giuseppe_bridge_4k.hdr
# The source as it stands, new files included.
(cd "$root" && git ls-files -z --cached --others --exclude-standard | tar --null -T - -cf "$stage/source.tar")
cp "$tiles" "$stage/tiles.json"
if [ -n "${OSAGE_BAKE_REUSE:-}" ]; then
  gcloud storage cp "$OSAGE_BAKE_REUSE/in/blender.tar.zst" "$OSAGE_BAKE_REUSE/in/scene.tar" \
    "$OSAGE_BAKE_REUSE/in/street.tar" "$run/in/"
else
  gcloud storage cp "$stage/blender.tar.zst" "$stage/scene.tar" "$stage/street.tar" "$run/in/"
fi
gcloud storage cp "$stage/source.tar" "$stage/tiles.json" "$run/in/"
packed=
if [ -n "${OSAGE_BAKE_PACKED:-}" ]; then
  tar -C "$(dirname "$OSAGE_BAKE_PACKED")" -cf "$stage/tiles.tar" "$(basename "$OSAGE_BAKE_PACKED")"
  gcloud storage cp "$stage/tiles.tar" "$run/in/"
  packed=1
fi

for candidate in asia-southeast1-b asia-southeast1-a asia-southeast1-c; do
  if g compute instances create "$name" --zone "$candidate" --machine-type "${OSAGE_BAKE_MACHINE:-g2-standard-8}" \
    --maintenance-policy TERMINATE --provisioning-model STANDARD \
    --max-run-duration 180m --instance-termination-action DELETE \
    --image-family osage-bench --image-project "$project" --boot-disk-size 100GB --boot-disk-type pd-balanced \
    --subnet bench --service-account "bench-vm@$project.iam.gserviceaccount.com" \
    --scopes https://www.googleapis.com/auth/devstorage.read_write \
    --metadata "^@^osage-run=$run@osage-size=$size@osage-samples=$samples@osage-packed=$packed@osage-cube=${OSAGE_BAKE_CUBE:-}@osage-probes=${OSAGE_BAKE_PROBES:-}@osage-pack=${OSAGE_BAKE_PACK:-}@osage-film=${OSAGE_BAKE_FILM:-}@osage-variants=${OSAGE_BAKE_VARIANTS:-}" \
    --metadata-from-file startup-script="$here/on-vm-bake.sh" --labels osage=bake; then
    zone=$candidate
    break
  fi
  echo "no machine in $candidate, trying the next zone" >&2
done
[ -n "$zone" ] || { echo "no L4 machine free in Singapore" >&2; exit 1; }
echo "baking on $name in $zone (run $run)"

status=
seen=
silent=0
for _ in $(seq 700); do
  sleep 15
  progress=$(gcloud storage cat "$run/progress.txt" 2>/dev/null || true)
  if [ "$progress" != "$seen" ]; then
    # diff says 1 when they differ, which they do.
    diff <(echo "$seen") <(echo "$progress") | sed -n 's/^> //p' || true
    seen=$progress
  fi
  status=$(gcloud storage cat "$run/done" 2>/dev/null) && break
  # Expired credentials are not a machine gone: it is left to log in again.
  gcloud auth print-access-token >/dev/null 2>&1 ||
    { echo "gcloud's credentials expired: $name ($zone) left as it is; log in, fetch $run/out, delete it" >&2; exit 1; }
  # One question without an answer is the network's doing (a download
  # beside, a call that times out), not a machine gone: eight in a row are.
  if state=$(g compute instances describe "$name" --zone "$zone" --format 'value(status)' 2>/dev/null); then
    silent=0
  else
    silent=$((silent + 1))
    state=UNKNOWN
    [ "$silent" -ge 8 ] && state=GONE
  fi
  case $state in
  TERMINATED | STOPPED | GONE)
    status=$(gcloud storage cat "$run/done" 2>/dev/null) && break
    echo "the machine stopped ($state) before finishing" >&2
    exit 1
    ;;
  esac
done
[ -n "$status" ] || { echo "no result after three hours" >&2; exit 1; }
mkdir -p "$out"
gcloud storage cp -r "$run/out/*" "$out/"
echo "osage bake: $status"
[ "$status" = ok ]
