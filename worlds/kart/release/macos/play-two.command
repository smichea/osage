#!/bin/sh
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
# Two players on this Mac, the screen split (D-086), against the world's
# bots: player 1 W A S D, Space to drift, F for the item, the first
# gamepad; player 2 the arrows, right Shift, the / key, the second gamepad.
cd "$(dirname "$0")" || exit 1
# Unzipped from a browser's download, each file here carries macOS's
# quarantine mark, and macOS would stop each program in turn. The player
# who let this launcher run lets the folder run: the mark is lifted.
if xattr -p com.apple.quarantine kart-native >/dev/null 2>&1; then xattr -dr com.apple.quarantine .; fi
# First the newest version, if there is one (kart-update runs this file
# again once updated; exec, so that this shell does not read on in it).
if [ "$OSAGE_UPDATED" != 1 ] && [ -x kart-update ]; then exec ./kart-update --then "$PWD/$(basename "$0")" "$@"; fi
./kart-server --records kart-records.txt --ghosts kart-ghosts >kart-server.log 2>&1 &
world=$!
# The world's leaderboard in a browser: http://localhost:8080/leaderboard.html
./kart-web web 127.0.0.1:8080 >kart-web.log 2>&1 &
web=$!
# Closing the game, Ctrl+C or closing the window stops the world.
trap 'kill $world $web 2>/dev/null' EXIT
trap 'exit 1' INT TERM HUP
sleep 2
./kart-native --players 2 "$@"
