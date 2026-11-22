#!/bin/sh
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
# A walk in the street, on this Mac alone: the street's model round the
# walker, lit by the light baked into the street's folder.
cd "$(dirname "$0")" || exit 1
# Unzipped from a browser's download, each file here carries macOS's
# quarantine mark, and macOS would stop each program in turn. The player
# who let this launcher run lets the folder run: the mark is lifted.
if xattr -p com.apple.quarantine paris >/dev/null 2>&1; then xattr -dr com.apple.quarantine .; fi
# First the newest version and the street, if they are not here yet
# (paris-update runs this file again once updated; exec, so that this
# shell does not read on in it).
if [ "$OSAGE_UPDATED" != 1 ] && [ -x paris-update ]; then exec ./paris-update --then "$PWD/$(basename "$0")" "$@"; fi
# It sets out looking down the street.
./paris --offline --ring --block-probes --at 24.82,1.96,-61.65 --heading 161.4 "$@"
