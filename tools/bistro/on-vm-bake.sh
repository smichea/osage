#!/usr/bin/env bash
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Start-up script of a bake machine (cloud-bake.sh passes it). Reads from the
# machine's metadata where its run is (`osage-run`, a bucket folder), the
# tiles' size (`osage-size`) and samples (`osage-samples`), or that the
# tiles come packed (`osage-packed`: no bake, only the shots); fetches Blender,
# the scene, the street and the source from `in/` there; bakes the tiles
# and the picture to measure against on the GPU while the client builds on
# the processors; packs the tiles; draws the client at T4 and T3 from where
# the scene's camera stands; and puts back under `out/` what it made, then
# `done` with a one-line status. Then it powers off; cloud-bake.sh deletes it.

set -uo pipefail
# Start-up scripts run with a bare environment: gcloud lives in /snap/bin.
export PATH="$PATH:/usr/local/bin:/usr/bin:/bin:/snap/bin" HOME=/root DEBIAN_FRONTEND=noninteractive

metadata() {
  curl -sf -H 'Metadata-Flavor: Google' "http://metadata.google.internal/computeMetadata/v1/instance/attributes/$1"
}
run=$(metadata osage-run)
size=$(metadata osage-size || echo 2048)
samples=$(metadata osage-samples || echo 256)
packed=$(metadata osage-packed || echo "")
# A cube (X,Y,Z): its probes (cube-probes.py) baked instead of the tiles,
# brought back raw, nothing else.
cube=$(metadata osage-cube || echo "")
work=/var/tmp/osage
rm -rf "$work" && mkdir -p "$work/out" && cd "$work"

say() {
  echo "osage bake: $*"
  echo "$(date -u +%T) $*" >>"$work/out/progress.txt"
  gcloud storage cp "$work/out/progress.txt" "$run/progress.txt" >/dev/null 2>&1 || true
}
finish() {
  say "$1"
  cd "$work"
  gcloud storage cp -r out "$run/"
  echo "$1" | gcloud storage cp - "$run/done"
  poweroff
  exit 0
}

nvidia-smi >out/gpu.txt 2>&1 || finish "failed: no GPU driver (see out/gpu.txt)"
gcloud storage cp "$run/in/*" . || finish "failed: the inputs are not in $run/in"
# zstd for Blender's archive; the X libraries Blender links to, though it
# draws nothing here.
apt-get update -qq && apt-get install -y -qq --no-install-recommends zstd libx11-6 libxrender1 libxfixes3 libxi6 \
  libxkbcommon0 libxxf86vm1 libsm6 libice6 >/dev/null || finish "failed: Blender's libraries"
say "inputs here"

# The client and light-pack, built on the processors while the GPU bakes.
(
  set -e
  apt-get install -y -qq --no-install-recommends build-essential pkg-config libudev-dev libwayland-dev \
    libxkbcommon-dev libasound2-dev nasm ffmpeg ca-certificates curl >/dev/null
  curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable >/dev/null
  # shellcheck disable=SC1091
  . "$HOME/.cargo/env"
  mkdir -p src && tar -xf source.tar -C src
  mkdir -p src/assets && tar -xf street.tar -C src/assets
  cd src
  cargo build --release -p osage-paris-native --features raytracing --bin paris -p osage-hologram-stream --bin light-pack
) >out/build.log 2>&1 &
builder=$!

if [ -n "$packed" ]; then
  mkdir -p raw
else
  mkdir -p blender scene tools
  tar -I zstd -xf blender.tar.zst -C blender --strip-components=1 || finish "failed: Blender does not unpack"
  tar -xf scene.tar -C scene || finish "failed: the scene does not unpack"
  tar -xf source.tar -C tools --strip-components=2 tools/bistro/tiles.py tools/bistro/truth.py \
    tools/bistro/cube-probes.py || finish "failed: the bake's scripts are not in the source"
  blender=$work/blender/blender
  device=OPTIX
  OSAGE_DEVICE=$device "$blender" -b scene/bistro/exterior.blend --python-exit-code 1 \
    --python tools/truth.py -- out/truth 1280 720 16 >out/device.log 2>&1 || device=CUDA
  if [ -n "$cube" ]; then
    say "rendering on $device; baking the probes of cube $cube, $size px wide, $samples samples"
    OSAGE_DEVICE=$device "$blender" -b scene/bistro/exterior.blend --python-exit-code 1 \
      --python tools/cube-probes.py -- "$cube" raw "$size" "$samples" >out/bake.log 2>&1 ||
      finish "failed: the probes (see out/bake.log)"
    tar -cf raw.tar raw && gcloud storage cp raw.tar "$run/raw.tar" >/dev/null 2>&1 || finish "failed: the raw probes did not go up"
    finish ok
  fi
  say "rendering on $device; baking $(grep -c '"key"' tiles.json) tiles at $size px, $samples samples"
  OSAGE_DEVICE=$device "$blender" -b scene/bistro/exterior.blend --python-exit-code 1 \
    --python tools/tiles.py -- tiles.json raw "$size" "$samples" >out/bake.log 2>&1 ||
    finish "failed: the bake (see out/bake.log)"
  say "tiles baked: $(grep '^tiles:' out/bake.log | tail -1)"
  OSAGE_DEVICE=$device "$blender" -b scene/bistro/exterior.blend --python-exit-code 1 \
    --python tools/truth.py -- out/truth 1280 720 1024 >out/truth.log 2>&1 || say "the truth failed (see out/truth.log)"
  # The raw tiles stay in the bucket, to pack again otherwise.
  tar -cf raw.tar raw && gcloud storage cp raw.tar "$run/raw.tar" >/dev/null 2>&1 || say "the raw tiles did not go up"
fi

wait "$builder" || finish "failed: the build (see out/build.log)"
say "built"
cd src
if [ -n "$packed" ]; then
  mkdir -p assets/orca/bistro/out && tar -xf ../tiles.tar -C assets/orca/bistro/out || finish "failed: the packed tiles"
else
  ./target/release/light-pack ../raw assets/orca/bistro/out/tiles >../out/pack.log 2>&1 ||
    finish "failed: packing (see out/pack.log)"
  cp -r assets/orca/bistro/out/tiles ../out/tiles
fi
for tier in T4 T3; do
  for plain in "" --plain; do
    shot="$tier${plain:+-plain}"
    # shellcheck disable=SC2086 # the option is a word or none
    timeout 900 ./target/release/paris --offline --tier "$tier" --fov 46.7 $plain --shot "../out/$shot.png" \
      --at 24.82,1.96,-61.65 --heading 161.4 >"../out/$shot.log" 2>&1 || say "the $shot shot failed (see out/$shot.log)"
  done
done
cd "$work"
finish ok
