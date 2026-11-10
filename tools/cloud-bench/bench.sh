#!/usr/bin/env bash
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Runs the render bench on a cloud GPU (D-042) and brings back its receipts.
#
#     tools/cloud-bench/bench.sh PROJECT BUILD.tar.gz OUT TIER...
#
# BUILD.tar.gz holds `kart/kart-native` and `kart/assets/`. A machine with one
# NVIDIA L4 (g2-standard-4) is made from the `osage-bench` image in the first
# Singapore zone that has one free, runs on-vm.sh, and is deleted, whatever
# happens: by this script when it exits, and by Google after 40 minutes if
# this script dies first. The receipts land in OUT/<tier>/.

set -euo pipefail

project=${1:?project}
build=${2:?build archive}
out=${3:?output directory}
shift 3
tiers="$*"
[ -n "$tiers" ] || { echo "no tier given" >&2; exit 2; }
id="$(date -u +%Y%m%d-%H%M%S)-${GITHUB_RUN_ID:-local}"
run="gs://$project-runs/$id"
name="osage-bench-$id"
here=$(dirname "$0")
g() { gcloud --project "$project" --quiet "$@"; }

zone=
delete() {
  [ -n "$zone" ] && g compute instances delete "$name" --zone "$zone" >/dev/null 2>&1 || true
}
trap delete EXIT

gcloud storage cp "$build" "$run/kart.tar.gz"

for candidate in asia-southeast1-b asia-southeast1-a asia-southeast1-c; do
  if g compute instances create "$name" --zone "$candidate" --machine-type g2-standard-4 \
    --maintenance-policy TERMINATE --provisioning-model STANDARD \
    --max-run-duration 40m --instance-termination-action DELETE \
    --image-family osage-bench --image-project "$project" --boot-disk-type pd-balanced \
    --subnet bench --service-account "bench-vm@$project.iam.gserviceaccount.com" \
    --scopes https://www.googleapis.com/auth/devstorage.read_write \
    --metadata "osage-run=$run,osage-tiers=$tiers" \
    --metadata-from-file startup-script="$here/on-vm.sh" --labels osage=bench; then
    zone=$candidate
    break
  fi
  echo "no machine in $candidate, trying the next zone" >&2
done
[ -n "$zone" ] || { echo "no L4 machine free in Singapore" >&2; exit 1; }

echo "benching $tiers on $name in $zone"
status=
for _ in $(seq 150); do
  sleep 15
  status=$(gcloud storage cat "$run/done" 2>/dev/null) && break
  state=$(g compute instances describe "$name" --zone "$zone" --format 'value(status)' 2>/dev/null || echo GONE)
  case $state in
  TERMINATED | STOPPED | GONE)
    status=$(gcloud storage cat "$run/done" 2>/dev/null) && break
    echo "the machine stopped ($state) before finishing" >&2
    g compute instances get-serial-port-output "$name" --zone "$zone" 2>/dev/null | grep -i 'startup-script' | tail -40 >&2 || true
    exit 1
    ;;
  esac
done
[ -n "$status" ] || { echo "no result after 37 minutes" >&2; exit 1; }

mkdir -p "$out"
gcloud storage cp -r "$run/out/*" "$out/"
echo "osage bench: $status"
[ "$status" = ok ]
