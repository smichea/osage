# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Partition collision support by physical extent, preserving local and source face order."""
from collections import defaultdict
import hashlib
import struct

from paging_support_storage import pack_catalogue


def partition(tiles, catalogue, edge, span, encode, write, storage):
    if span < 1:
        raise ValueError("support region width must be positive")
    groups = defaultdict(list)
    ordinals = {}
    for i, tile in enumerate(tiles):
        if layer := tile.get("physics"):
            lo, hi = layer["bounds"]
            x, z = tile["x"], tile["z"]
            wide = max(hi[0] - lo[0], hi[2] - lo[2]) > edge * span
            key = ("wide", x, z) if wide else ("grid", x // span, z // span)
            ordinals[i] = len(ordinals)
            groups[key].append(i)
    placements = defaultdict(list)
    corners = catalogue.get("corner_meshes", {})
    for placement in corners.get("placements", []):
        placements[tuple(placement[0])].append(placement)
    selection = catalogue.get("face_order")
    exceptions = defaultdict(list)
    for page, *row in (selection or {}).get("exceptions", []):
        exceptions[page].append(row)
    certificates = defaultdict(list)
    for page, *row in catalogue.get("residual_triangle_corners", []):
        certificates[page].append(row)

    regions = []
    for ids in groups.values():
        binding = hashlib.sha256()
        part = {"format": catalogue["format"]}
        for field in ("pages", "residual", "residual_shapes"):
            part[field] = [catalogue[field][ordinals[i]] for i in ids]
        local_placements, local_exceptions, local_certificates = [], [], []
        for local, i in enumerate(ids):
            tile = tiles[i]
            cell = tile["x"], tile["z"]
            binding.update(struct.pack("<ii", *cell))
            binding.update(tile["physics"]["sha256"].encode("ascii"))
            local_placements.extend(placements[cell])
            local_exceptions.extend([local, *row] for row in exceptions[ordinals[i]])
            local_certificates.extend([local, *row] for row in certificates[ordinals[i]])
        if local_placements:
            part["corner_meshes"] = {"patterns": corners["patterns"], "placements": local_placements}
        if selection:
            part["face_order"] = {"start": selection["start"], "exceptions": local_exceptions}
        if local_certificates:
            part["residual_triangle_corners"] = local_certificates
        part["directory_sha256"] = binding.hexdigest()
        raw = encode(part)
        packed, extension = pack_catalogue(raw, storage)
        regions.append({"tiles": ids, **write(packed, extension), "decoded_bytes": len(raw)})
    return regions
