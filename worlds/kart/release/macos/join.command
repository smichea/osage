#!/bin/sh
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
# Joins a race hosted on another computer of the same network
# (host.command on a Mac, host.bat on Windows).
cd "$(dirname "$0")" || exit 1
# Unzipped from a browser's download, each file here carries macOS's
# quarantine mark, and macOS would stop each program in turn. The player
# who let this launcher run lets the folder run: the mark is lifted.
if xattr -p com.apple.quarantine kart-native >/dev/null 2>&1; then xattr -dr com.apple.quarantine .; fi
# First the newest version, if there is one (kart-update runs this file
# again once updated; exec, so that this shell does not read on in it).
if [ "$OSAGE_UPDATED" != 1 ] && [ -x kart-update ]; then exec ./kart-update --then "$PWD/$(basename "$0")" "$@"; fi
printf 'Address of the computer that hosts (host.command shows it, e.g. 192.168.1.20): '
read -r host
./kart-native "ws://$host:7878" "$@"
