#!/bin/sh
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
# Hosts a race for this Mac and the others on the same network: starts the
# kart world open to the network, shows this Mac's address for the others
# (they start join.command or join.bat, or open it in a browser on port
# 8080 from a phone or a tablet), then the game. Closing the game stops
# the world.
cd "$(dirname "$0")" || exit 1
# Unzipped from a browser's download, each file here carries macOS's
# quarantine mark, and macOS would stop each program in turn. The player
# who let this launcher run lets the folder run: the mark is lifted.
if xattr -p com.apple.quarantine kart-native >/dev/null 2>&1; then xattr -dr com.apple.quarantine .; fi
# First the newest version, if there is one (kart-update runs this file
# again once updated; exec, so that this shell does not read on in it).
if [ "$OSAGE_UPDATED" != 1 ] && [ -x kart-update ]; then exec ./kart-update --then "$PWD/$(basename "$0")" "$@"; fi
echo
echo "  The others start join.command (join.bat on Windows) and type this Mac's address:"
ifconfig | awk '/inet / && $2 != "127.0.0.1" { print "    " $2 }'
echo
echo "  A phone or a tablet opens  http://THAT-ADDRESS:8080  in its browser,"
echo "  and anyone  http://THAT-ADDRESS:8080/leaderboard.html  for the records."
echo "  If macOS asks whether kart-server and kart-web may accept incoming"
echo "  network connections, allow them."
echo
# The host's music, shared with the others, when there is a "music" folder.
music=
if [ -d music ]; then
    music="--music music"
    echo "  Sharing the music in the \"music\" folder on port 7880."
    echo
fi
# $music unquoted: nothing, or the option and its folder, two words.
# shellcheck disable=SC2086
./kart-server --listen 0.0.0.0:7878 --webtransport 0.0.0.0:7879 --records kart-records.txt --ghosts kart-ghosts $music >kart-server.log 2>&1 &
world=$!
./kart-web web 0.0.0.0:8080 >kart-web.log 2>&1 &
web=$!
# Closing the game, Ctrl+C or closing the window stops the world.
trap 'kill $world $web 2>/dev/null' EXIT
trap 'exit 1' INT TERM HUP
sleep 3
./kart-native "$@"
