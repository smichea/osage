#!/usr/bin/env bash
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Start-up script of a bench machine (bench.sh passes it). Reads from the
# machine's metadata where its run is (`osage-run`, a bucket folder) and which
# tiers to bench (`osage-tiers`); fetches the build from there, benches each
# tier with no screen, and puts back what it measured, under `out/`, then
# `done` with a one-line status. Then it powers off; bench.sh deletes it.

set -uo pipefail
# Start-up scripts run with a bare environment: gcloud lives in /snap/bin.
export PATH="$PATH:/usr/local/bin:/usr/bin:/bin:/snap/bin" HOME=/root

metadata() {
  curl -sf -H 'Metadata-Flavor: Google' "http://metadata.google.internal/computeMetadata/v1/instance/attributes/$1"
}
run=$(metadata osage-run)
tiers=$(metadata osage-tiers)
frames=$(metadata osage-frames || echo 2400)
work=/var/tmp/osage
rm -rf "$work" && mkdir -p "$work/out" && cd "$work"

finish() {
  echo "osage bench: $1"
  cd "$work"
  gcloud storage cp -r out "$run/"
  echo "$1" | gcloud storage cp - "$run/done"
  poweroff
  exit 0
}

gcloud storage cp "$run/kart.tar.gz" . || finish "failed: the build is not in $run"
tar xzf kart.tar.gz && cd kart || finish "failed: the build does not unpack"
nvidia-smi >../out/gpu.txt 2>&1 || finish "failed: no GPU driver (see out/gpu.txt)"
vulkaninfo --summary >../out/vulkan.txt 2>&1 || finish "failed: no Vulkan (see out/vulkan.txt)"

status=ok
for tier in $tiers; do
  echo "osage bench: $tier"
  ./kart-native --bench --headless --tier "$tier" --frames "$frames" --out "../out/$tier" >"../out/$tier.log" 2>&1 ||
    status="failed: the $tier bench stopped (see out/$tier.log)"
done
finish "$status"
