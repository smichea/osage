#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
"""Write the original striped panel PNG and its portable material choices."""

import argparse
import json
import struct
import zlib
from pathlib import Path


def rgba_png(width, height, pixels):
    def chunk(kind, data):
        return (
            struct.pack(">I", len(data))
            + kind
            + data
            + struct.pack(">I", zlib.crc32(kind + data))
        )

    scanlines = b"".join(
        b"\0" + pixels[y * width * 4 : (y + 1) * width * 4] for y in range(height)
    )
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(scanlines))
        + chunk(b"IEND", b"")
    )


def fixture(out, *, unit_vertex_alpha=False, basisu=False):
    pixels = bytes(
        value
        for y in range(4)
        for x in range(4)
        for value in (210, 170 - 30 * y, 20 * x, 255 if x == y or x == 0 else 0)
    )
    png = rgba_png(4, 4, pixels)
    out.mkdir(parents=True, exist_ok=True)
    (out / "stripes.png").write_bytes(png)
    manifest = {
        "version": 1,
        "textures": {"panel_stripes": "stripes.png"},
        "materials": {
            "0": {
                "shader": "panel_near",
                "alphaMode": "MASK",
                "alphaCutoff": 0.4,
                "doubleSided": True,
            },
            "1": {
                "shader": "panel_far",
                "alphaMode": "MASK",
                "alphaCutoff": 0.6,
                "doubleSided": False,
            },
        },
    }
    if basisu:
        (out / "stripes-2.png").write_bytes(
            rgba_png(2, 2, bytes([8, 232, 224, 192]) * 4)
        )
        (out / "stripes-1.png").write_bytes(rgba_png(1, 1, bytes([240, 40, 8, 64])))
        manifest["basisuMips"] = {"panel_stripes": ["stripes-2.png", "stripes-1.png"]}
    if unit_vertex_alpha:
        for material in manifest["materials"].values():
            material["vertexColorAlpha"] = "one"
    path = out / "appearance.json"
    path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return path


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("out", type=Path)
    parser.add_argument("--unit-vertex-alpha", action="store_true")
    parser.add_argument(
        "--basisu",
        action="store_true",
        help="supply explicit original 2x2 and 1x1 PNG mip levels",
    )
    args = parser.parse_args()
    print(
        fixture(args.out, unit_vertex_alpha=args.unit_vertex_alpha, basisu=args.basisu)
    )
