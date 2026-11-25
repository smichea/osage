# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Lossless compression of a district directory or its bootstrap collision catalogue."""

import shutil
import subprocess
import zlib

try:
    from compression import zstd
except ImportError:  # Python before 3.14 can use the system encoder.
    zstd = None


def pack_catalogue(raw, storage):
    compressed = zlib.compress(raw, 9)
    if storage == "zlib":
        return compressed, "zlib"
    if storage != "auto":
        raise ValueError(f"unknown support storage: {storage}")
    if zstd is not None:
        candidate = zstd.compress(raw, level=19)
    elif command := shutil.which("zstd"):
        candidate = subprocess.run(
            [command, "-q", "-c", "-19", "--no-check", f"--stream-size={len(raw)}"],
            input=raw, stdout=subprocess.PIPE, check=True,
        ).stdout
    else:
        return compressed, "zlib"
    return (candidate, "zst") if len(candidate) < len(compressed) else (compressed, "zlib")


# Earlier district preparation scripts import this name.
pack_support = pack_catalogue


def unpack_support(payload):
    if payload[:4] != b"\x28\xb5\x2f\xfd":
        return zlib.decompress(payload)
    if zstd is not None:
        return zstd.decompress(payload)
    if command := shutil.which("zstd"):
        return subprocess.run([command, "-dq", "-c"], input=payload, stdout=subprocess.PIPE, check=True).stdout
    raise RuntimeError("reading a .zst support catalogue requires Python 3.14 or the zstd command")
