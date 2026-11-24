#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

"""The bridge played by osage-sound in a browser's AudioWorklet.

    cargo build --release --target wasm32-unknown-unknown -p osage-sound-web
    python3 tools/paris-soundscape/web/live.py check [--quality low]
    python3 tools/paris-soundscape/web/live.py play  [--quality low] [--seconds 45]

Serves the repository's root on 127.0.0.1 and opens web/live.html in a
headless Chromium with a fresh profile. `check` renders two seconds through
the worklet in an OfflineAudioContext and compares them with the same
module's blocks made directly: they must be the same samples. `play` plays
the bridge in an AudioContext, the listener walked by a pose sixty times a
second, and prints what Chromium says of its audio thread
(`AudioContext.renderCapacity`: load and underruns; `playoutStats`). A
headless browser plays to a fake output on its own clock, muted: it says
whether the engine keeps up, not what a sound card adds. Standard library
only.
"""

import argparse
import functools
import http.server
import json
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import threading

ROOT = pathlib.Path(__file__).resolve().parents[3]
MARK = re.compile(r"PARIS-LIVE (\{.*\})")
FLAGS = (
    "--headless",
    "--disable-gpu",
    "--mute-audio",
    "--autoplay-policy=no-user-gesture-required",
    # AudioContext.renderCapacity and playoutStats are behind it in Chromium 155.
    "--enable-experimental-web-platform-features",
    "--no-first-run",
    "--no-default-browser-check",
    "--disable-extensions",
    "--disable-component-update",
    "--disable-background-networking",
    "--disable-sync",
    "--disable-default-apps",
    "--enable-logging=stderr",
    "--log-level=0",
)


class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def end_headers(self):
        # A module worker fetches .wasm and .js: say what they are.
        self.send_header("Cache-Control", "no-store")
        super().end_headers()


Quiet.extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map, ".wasm": "application/wasm", ".js": "text/javascript", ".mjs": "text/javascript"}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("mode", choices=["check", "play"])
    parser.add_argument("--quality", default="low", choices=["low", "medium", "high", "reference"])
    parser.add_argument("--seconds", type=float, default=45.0)
    parser.add_argument("--browser", default=None)
    args = parser.parse_args()

    wasm = ROOT / "target/wasm32-unknown-unknown/release/osage_sound_web.wasm"
    if not wasm.exists():
        sys.exit(f"{wasm.relative_to(ROOT)} is missing: cargo build --release --target wasm32-unknown-unknown -p osage-sound-web")
    browser = args.browser or next((b for b in ("chromium", "google-chrome-stable", "google-chrome", "chromium-browser") if shutil.which(b)), None)
    if not browser:
        sys.exit("no browser found: install Chromium or pass --browser")

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(Quiet, directory=str(ROOT)))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    url = f"http://127.0.0.1:{server.server_address[1]}/tools/paris-soundscape/web/live.html?mode={args.mode}&quality={args.quality}&seconds={args.seconds}"
    with tempfile.TemporaryDirectory(prefix="osage-live-") as profile:
        process = subprocess.Popen([browser, *FLAGS, f"--user-data-dir={profile}", url], stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True, start_new_session=True)
        result = None
        try:
            for line in process.stderr:
                found = MARK.search(line)
                if found:
                    result = json.loads(found.group(1))
                    break
        finally:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
    server.shutdown()
    if result is None:
        sys.exit("the page said nothing")
    print(json.dumps(result, indent=2))
    if "error" in result:
        sys.exit(1)
    if args.mode == "check" and (result["differing"] or result["loudest"] <= 0):
        sys.exit("the worklet's samples are not the module's")
    capacity = result.get("renderCapacity")
    if args.mode == "play" and isinstance(capacity, dict) and capacity["worstUnderrun"] > 0:
        sys.exit("the audio thread ran short")
    if args.mode == "play" and result["cost"]["over"] > 0:
        sys.exit(f"{result['cost']['over']} blocks took longer than their budget")


if __name__ == "__main__":
    main()
