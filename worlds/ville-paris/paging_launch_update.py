# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Prepare an optional Zstandard dictionary update for an immutable first-frame package."""
import hashlib
import re
import shutil
import struct
import subprocess
import tempfile
import zlib

from paging_physics_directory import LIMIT
from paging_support_storage import zstd


def address(data):
    return f"{hashlib.sha256(data).hexdigest()}:{len(data)}"


def payload(wire):
    if wire[:4] != b"ODI3" or len(wire) < 8:
        raise ValueError("update requires an ODI3 first-frame package")
    size, = struct.unpack_from("<I", wire, 4)
    if not 0 < size <= LIMIT:
        raise ValueError("invalid decoded package length")
    body = wire[8:]
    if body.startswith(b"\x28\xb5\x2f\xfd") and zstd is None:
        # Limit output even if a corrupt publisher wrote a false decoded length.
        with tempfile.TemporaryFile() as source:
            source.write(body)
            source.seek(0)
            with subprocess.Popen(["zstd", "-qdc", "--memory=64MB"], stdin=source, stdout=subprocess.PIPE) as decoder:
                data = decoder.stdout.read(size + 1)
                if len(data) > size:
                    decoder.kill()
                if decoder.wait() != 0:
                    raise ValueError("invalid Zstandard first-frame package")
    else:
        decoder = zstd.ZstdDecompressor() if body.startswith(b"\x28\xb5\x2f\xfd") else zlib.decompressobj()
        data = decoder.decompress(body, max_length=size + 1)
        if not decoder.eof or decoder.unused_data:
            raise ValueError("invalid or trailing first-frame stream")
    if len(data) != size:
        raise ValueError("incomplete first-frame package")
    return data


def dictionary_patch(base, target):
    if zstd is not None:
        return zstd.compress(target, level=19, zstd_dict=zstd.ZstdDict(base, is_raw=True))
    elif command := shutil.which("zstd"):
        with tempfile.NamedTemporaryFile() as dictionary:
            dictionary.write(base)
            dictionary.flush()
            return subprocess.run([command, "-q", "-c", "-19", "-D", dictionary.name,
                                    f"--stream-size={len(target)}"], input=target,
                                   stdout=subprocess.PIPE, check=True).stdout
    return None


def update_reference(directory, previous, wire, write):
    # A previous release can itself have an update reference. Its first field always names the full package.
    previous = previous.split("~", 1)[0]
    if not re.fullmatch(r"[0-9a-f]{64}:[1-9][0-9]{0,8}", previous):
        raise ValueError("expected previous first-frame SHA256:BYTES")
    digest, length = previous.split(":")
    path = directory / (digest + ".district")
    if int(length) > LIMIT or path.stat().st_size != int(length):
        raise ValueError("previous first-frame package size mismatch")
    old = path.read_bytes()
    if address(old) != previous:
        raise ValueError("previous first-frame package digest mismatch")
    full = address(wire)
    if previous == full or (zstd is None and not shutil.which("zstd")):
        return full
    base, target = payload(old), payload(wire)
    patch = dictionary_patch(base, target)
    if patch is None:
        return full
    reference = "~".join((full, address(target), previous, address(base), address(patch)))
    if len(patch) + len(reference) - len(full) >= len(wire):
        return full
    write(patch, "delta")
    return reference
