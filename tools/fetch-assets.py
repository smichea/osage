#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Fetches and checks the content listed in assets/manifest.toml.

    python3 tools/fetch-assets.py            # download what is missing, check everything
    python3 tools/fetch-assets.py --check    # check only, download nothing

Files with a `url` are downloaded; files marked `manual` are downloaded by a
person, as their publisher asks, and only checked here; files with `made_by`
are derived from others by that tool. Every other file present is checked
against its `sha256`. Standard library only (Python 3.11 or later).
"""

import hashlib
import pathlib
import sys
import tomllib
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
# Publishers ask automated clients to say who they are.
USER_AGENT = "osage-asset-fetch/0.1 (+https://github.com/smichea/osage)"


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for block in iter(lambda: file.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def main() -> int:
    check_only = "--check" in sys.argv[1:]
    manifest = tomllib.loads((ROOT / "assets" / "manifest.toml").read_text())
    missing, wrong = [], []
    for asset in manifest.get("asset", []):
        path = ROOT / asset["path"]
        if "made_by" in asset:
            # Derived here from other assets: present or not, nothing to check.
            if not path.exists():
                missing.append(f"{asset['path']}: run {asset['made_by']}")
            continue
        if not path.exists():
            if "url" not in asset or check_only:
                how = asset.get("manual", "run tools/fetch-assets.py")
                missing.append(f"{asset['path']}: {how}")
                continue
            print(f"fetching {asset['path']}")
            path.parent.mkdir(parents=True, exist_ok=True)
            request = urllib.request.Request(asset["url"], headers={"User-Agent": USER_AGENT})
            with urllib.request.urlopen(request, timeout=60) as response:
                path.write_bytes(response.read())
        if sha256(path) != asset["sha256"]:
            wrong.append(asset["path"])
    for line in missing:
        print(f"missing  {line}")
    for path in wrong:
        print(f"WRONG HASH  {path}: not the file the manifest lists; delete it and fetch again")
    total = len(manifest.get("asset", []))
    print(f"{total - len(missing) - len(wrong)} of {total} assets present and checked")
    return 1 if wrong else 0


if __name__ == "__main__":
    sys.exit(main())
