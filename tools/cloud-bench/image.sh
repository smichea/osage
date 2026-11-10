#!/usr/bin/env bash
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Start-up script of the machine that becomes the bench image
# (make-image.sh): Ubuntu 24.04 with NVIDIA's driver, open kernel modules,
# and Vulkan; nothing that updates itself, so that every bench machine boots
# the same system. It powers the machine off when done, and prints
# OSAGE-IMAGE-OK last if all went well.

set -euxo pipefail
export DEBIAN_FRONTEND=noninteractive

# However it ends, it says how and powers off: make-image.sh waits for that.
ended() {
  status=$?
  set +x
  if [ "$status" -eq 0 ]; then echo "OSAGE-IMAGE-OK $summary"; else echo "OSAGE-IMAGE-FAILED with status $status"; fi
  # make-image.sh reads the serial port every 20 seconds, only while the machine runs.
  sleep 60
  poweroff
}
summary=
trap ended EXIT

# A machine of the image only lives for one run: no updates behind its back.
systemctl disable --now unattended-upgrades.service apt-daily.timer apt-daily-upgrade.timer || true
apt-get -y purge unattended-upgrades || true

apt-get update
apt-get -y full-upgrade

# The newest driver branch the archive has, with modules prebuilt for the
# Google Cloud kernel: the kernel the upgrade above installed boots next.
branch=
for candidate in 580 575 570; do
  if apt-cache show "nvidia-driver-$candidate-open" "linux-modules-nvidia-$candidate-open-gcp" >/dev/null 2>&1; then
    branch=$candidate
    break
  fi
done
test -n "$branch"
apt-get -y install --no-install-recommends "nvidia-driver-$branch-open" "linux-modules-nvidia-$branch-open-gcp" \
  nvidia-utils-"$branch" libvulkan1 vulkan-tools libwayland-client0 libudev1 libffi8

# The bench machine reads its build and writes its receipts with gcloud.
command -v gcloud || snap install google-cloud-cli --classic

apt-get clean
summary="nvidia $branch, kernels: $(ls /lib/modules | tr '\n' ' ')"
