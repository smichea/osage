# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Measure RGB error between two city screenshots taken at the same replay tick."""
import argparse
import hashlib
import json
import struct
import subprocess
from pathlib import Path


def rgb(path):
    with path.open("rb") as image:
        header = image.read(24)
    if not header.startswith(bytes.fromhex("89504e470d0a1a0a")) or header[12:16] != b"IHDR":
        raise ValueError(f"Expected a PNG at {path}")
    size = struct.unpack(">II", header[16:24])
    pixels = subprocess.check_output([
        "ffmpeg", "-loglevel", "error", "-threads", "1", "-i", str(path), "-frames:v", "1",
        "-threads", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1",
    ])
    if len(pixels) != size[0] * size[1] * 3:
        raise ValueError(f"Incomplete RGB frame at {path}")
    return size, pixels


def compare(full, distant):
    size, a = rgb(full)
    other_size, b = rgb(distant)
    if size != other_size:
        raise ValueError("Frame dimensions differ")
    delta = bytes(abs(x - y) for x, y in zip(a, b))
    changed = sum(max(delta[i:i + 3]) > 8 for i in range(0, len(delta), 3))
    return {
        "size": size,
        "mean_abs_rgb_in_8bit": sum(delta) / len(delta),
        "pixels_max_channel_delta_gt_8_fraction": changed / (size[0] * size[1]),
        "full_rgb_sha256": hashlib.sha256(a).hexdigest(),
        "distant_rgb_sha256": hashlib.sha256(b).hexdigest(),
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("full", type=Path)
    parser.add_argument("distant", type=Path)
    args = parser.parse_args()
    result = compare(args.full, args.distant)
    print(json.dumps(result, indent=2))
    raise SystemExit(int(result["mean_abs_rgb_in_8bit"] > 1
                         or result["pixels_max_channel_delta_gt_8_fraction"] > 0.01))
