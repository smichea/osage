#!/usr/bin/env bash
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Builds the bench machine image (family `osage-bench`) in PROJECT.
#
#     tools/cloud-bench/make-image.sh PROJECT
#
# A machine without a GPU installs the system (image.sh) and powers off; its
# disk becomes the image; the machine is deleted. The driver is checked on
# the first bench run, the only place with a GPU.

set -euo pipefail

project=${1:?project}
zone=asia-southeast1-b
name="osage-image-$(date -u +%Y%m%d-%H%M)"
here=$(dirname "$0")
g() { gcloud --project "$project" --quiet "$@"; }

trap 'g compute instances delete "$name" --zone "$zone" >/dev/null 2>&1 || true' EXIT

g compute instances create "$name" --zone "$zone" --machine-type e2-standard-4 \
  --image-family ubuntu-2404-lts-amd64 --image-project ubuntu-os-cloud \
  --boot-disk-size 30GB --boot-disk-type pd-balanced --subnet bench \
  --no-service-account --no-scopes --max-run-duration 60m --instance-termination-action STOP \
  --metadata-from-file startup-script="$here/image.sh" --labels osage=image

# The script's last words are on the serial port, which can only be read
# while the machine runs: read it until it says how it went, then wait for
# the machine to be off.
echo "waiting for $name to install the system and power off"
log= outcome= status=
for _ in $(seq 150); do
  sleep 20
  status=$(g compute instances describe "$name" --zone "$zone" --format 'value(status)' 2>/dev/null || true)
  if [ -z "$outcome" ] && [ "$status" = RUNNING ]; then
    log=$(g compute instances get-serial-port-output "$name" --zone "$zone" 2>/dev/null || true)
    outcome=$(grep -o 'OSAGE-IMAGE-\(OK\|FAILED\).*' <<<"$log" | tail -1 || true)
    [ -n "$outcome" ] && echo "$outcome"
  fi
  [ "$status" = TERMINATED ] && break
done
case $outcome in
OSAGE-IMAGE-OK*) ;;
*)
  grep -i 'startup-script' <<<"$log" | tail -40 >&2
  echo "the system install failed or did not finish (machine $status); see above" >&2
  exit 1
  ;;
esac
[ "$status" = TERMINATED ] || { echo "$name did not power off" >&2; exit 1; }

g compute images create "$name" --family osage-bench --source-disk "$name" --source-disk-zone "$zone" \
  --storage-location asia-southeast1 --labels osage=bench
echo "image $name (family osage-bench) made"
