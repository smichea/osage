# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

"""Read portable images and bind optional GPU-compressed alternatives to glTF textures."""

import hashlib
import shutil
import struct
import subprocess
import tempfile
import zlib
from pathlib import Path


def image_info(data):
    # Check container signatures and required framing. The native image loader decodes the pixels.
    if data[:8] == b"\x89PNG\r\n\x1a\n":
        pos, has_pixels = 8, False
        while pos + 12 <= len(data):
            length, kind = struct.unpack_from(">I4s", data, pos)
            end = pos + length + 8
            if (
                end + 4 > len(data)
                or zlib.crc32(memoryview(data)[pos + 4 : end])
                != struct.unpack_from(">I", data, end)[0]
            ):
                break
            if pos == 8 and (
                kind != b"IHDR"
                or length != 13
                or not all(struct.unpack_from(">II", data, pos + 8))
            ):
                break
            if kind == b"IDAT" and length:
                has_pixels = True
            if kind == b"IEND":
                if has_pixels and length == 0 and end + 4 == len(data):
                    return "image/png", struct.unpack_from(">II", data, 16)
                break
            pos = end + 4
    if data.startswith(b"\xff\xd8\xff") and data.endswith(b"\xff\xd9"):
        pos = 2
        dimensions = False
        while pos < len(data) and data[pos] == 0xFF:
            while pos < len(data) and data[pos] == 0xFF:
                pos += 1
            if pos + 3 > len(data):
                break
            marker = data[pos]
            length = int.from_bytes(data[pos + 1 : pos + 3], "big")
            if length < 2 or pos + length >= len(data):
                break
            if marker in (0xC0, 0xC1, 0xC2) and length >= 8:
                height, width = struct.unpack_from(">HH", data, pos + 4)
                dimensions = (width, height) if width and height else None
            if marker == 0xDA and dimensions:
                return "image/jpeg", dimensions
            pos += 1 + length
    raise ValueError("expected a self-contained PNG or JPEG image")


EXTENSION = "KHR_texture_basisu"
ENCODE_FLAGS = (
    "--encode",
    "uastc",
    "--uastc-quality",
    "2",
    "--zstd",
    "10",
    "--threads",
    "1",
)


def mip_paths(spec, names, directory, output):
    if not isinstance(spec, dict):
        raise ValueError(  # noqa: TRY004 -- invalid manifest data
            "basisuMips must map diffuse names to arrays of lower mip PNG paths"
        )
    result = {}
    for name, entries in spec.items():
        key = name.casefold()
        if key not in names or key in result:
            raise ValueError(f"unknown or ambiguous basisuMips diffuse name {name!r}")
        if not isinstance(entries, list) or any(
            not isinstance(v, str) or not v.strip() for v in entries
        ):
            raise ValueError(
                f"{name}: basisuMips needs an ordered array of lower mip PNG paths"
            )
        paths = tuple((directory / entry).resolve() for entry in entries)
        if output in paths:
            raise ValueError(f"{name}: choose a separate output path for supplied mips")
        result[key] = paths
    return result


def prepared_levels(base, paths, name):
    mime, (w, h) = image_info(base)
    if w % 4 or h % 4:
        raise ValueError(
            f"{name}: KHR_texture_basisu base dimensions {w}x{h} must be multiples of four"
        )
    n = max(w, h).bit_length()
    if len(paths) != n - 1:
        raise ValueError(
            f"{name}: expected {n - 1} lower mip PNGs down to 1x1; found {len(paths)}"
        )
    levels = [(base, ".png" if mime == "image/png" else ".jpg")]
    for i, path in enumerate(paths, 1):
        w, h = max(1, w // 2), max(1, h // 2)
        try:
            data = path.read_bytes()
            kind, size = image_info(data)
        except (OSError, ValueError) as exc:
            raise ValueError(f"{name}: mip {i} ({path.name}): {exc}") from exc
        if kind != "image/png" or size != (w, h):
            raise ValueError(f"{name}: mip {i} ({path.name}) needs a {w}x{h} PNG")
        levels.append((data, ".png"))
    return levels


def run_ktx(tool, args):
    proc = subprocess.run([tool, *args], capture_output=True, text=True, check=False)
    if proc.returncode:
        raise ValueError(
            f"ktx {args[0]} failed ({proc.returncode}): {(proc.stderr or proc.stdout).strip()}"
        )
    return proc.stdout.strip()


def encode_basisu(tool, levels):
    with tempfile.TemporaryDirectory(prefix="osage-ktx-") as stage:
        directory = Path(stage)
        inputs = []
        for index, (data, suffix) in enumerate(levels):
            path = directory / f"level-{index}{suffix}"
            path.write_bytes(data)
            inputs.append(path)
        output = directory / "texture.ktx2"
        run_ktx(
            tool,
            [
                "create",
                "--format",
                "R8G8B8A8_SRGB",
                "--assign-tf",
                "srgb",
                "--assign-primaries",
                "bt709",
                "--levels",
                str(len(levels)),
                *ENCODE_FLAGS,
                *inputs,
                output,
            ],
        )
        run_ktx(tool, ["validate", "--gltf-basisu", "--warnings-as-errors", output])
        return output.read_bytes()


def bind_basisu(spec, paths, directory, output, requested_tool, glb, report):
    recipes = mip_paths(spec, paths, directory, output)
    rows = [row for row in report["materials"] if "texture" in row]
    if not any(row["diffuseName"].casefold() in recipes for row in rows):
        return
    tool = shutil.which(str(requested_tool) if requested_tool is not None else "ktx")
    if tool is None:
        raise ValueError(
            "basisuMips needs --ktx-tool pointing to the Khronos ktx executable"
        )
    tool = Path(tool).resolve()
    meta = {
        "tool": run_ktx(tool, ["--version"]),
        "toolSha256": hashlib.sha256(tool.read_bytes()).hexdigest(),
        "codec": "UASTC",
        "quality": 2,
        "zstdLevel": 10,
        "images": [],
    }
    report["basisu"] = meta
    textures = glb.doc["textures"]
    # Distinct material slots may use the same fallback with different filtered mip chains.
    groups = {}
    for row in rows:
        key = row["diffuseName"].casefold()
        groups.setdefault(row["texture"], {}).setdefault(recipes.get(key), []).append(
            row
        )
    alternatives = {}
    encoded_images = {}
    for original, chains in groups.items():
        if len(chains) == 1 and None in chains:
            continue
        core = textures[original].copy()
        img = glb.doc["images"][core["source"]]
        view = glb.doc["bufferViews"][img["bufferView"]]
        start = view["byteOffset"]
        base = bytes(glb.binary[start : start + view["byteLength"]])
        emitted = {}
        for lower, bindings in chains.items():
            if lower is None:
                continue
            name = bindings[0]["diffuseName"]
            levels = prepared_levels(base, lower, name)
            hashes = tuple(hashlib.sha256(data).hexdigest() for data, _ in levels)
            if hashes not in alternatives:
                encoded = encode_basisu(tool, levels)
                digest = hashlib.sha256(encoded).hexdigest()
                if digest in encoded_images:
                    record = encoded_images[digest]
                    record.setdefault("equivalentLevelSha256", []).append(list(hashes))
                else:
                    record = {
                        "image": glb.image(name, "image/ktx2", encoded),
                        "bytes": len(encoded),
                        "sha256": digest,
                        "levelSha256": list(hashes),
                        "dimensions": image_info(base)[1],
                    }
                    encoded_images[digest] = record
                    meta["images"].append(record)
                alternatives[hashes] = record["image"]
            image = alternatives[hashes]
            if image not in emitted:
                if not emitted and None not in chains:
                    index = original
                else:
                    index = len(textures)
                    textures.append(core.copy())
                textures[index]["extensions"] = {EXTENSION: {"source": image}}
                emitted[image] = index
            index = emitted[image]
            for row in bindings:
                row.update(texture=index, basisuImage=image)
                glb.doc["materials"][row["slot"]]["pbrMetallicRoughness"][
                    "baseColorTexture"
                ]["index"] = index
    glb.doc.setdefault("extensionsUsed", []).append(EXTENSION)
    report["fallbackImages"] = report["images"]
    report["images"] = len(glb.doc["images"])
