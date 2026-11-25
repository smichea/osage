# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Pack authenticated physics metadata. Geometry and support remain independently addressed."""
import hashlib
import json
import struct
import zlib

from paging_support_storage import pack_catalogue, unpack_support

LIMIT = 64 * 1024 * 1024


def physics_directory(manifest):
    fields = ("source_sha256", "edge_m", "source_parents", "source_instances", "source_triangles",
              "support", "support_regions", "collision_face_order", "object_profile")
    doc = {key: manifest[key] for key in fields if key in manifest}
    doc["format"] = "ville-paris/physics/1"
    doc["tiles"] = [[t["x"], t["z"], t.get("physics")] for t in manifest["tiles"]]
    return doc


def envelope(magic, raw):
    if len(raw) > LIMIT:
        return None
    packed, _ = pack_catalogue(raw, "auto")
    body = magic + struct.pack("<I", len(raw)) + packed
    return body if len(body) <= LIMIT else None


def bound_planes(doc):
    planes = [bytearray() for _ in range(6)]
    for _, _, layer in doc["tiles"]:
        if layer is None:
            continue
        bounds = tuple(v for side in layer["bounds"] for v in side)
        try:
            packed = struct.pack("<6f", *bounds)
        except OverflowError:
            return None
        if struct.unpack("<6f", packed) != bounds:
            return None
        for i, plane in enumerate(planes):
            plane.extend(packed[4 * i:4 * i + 4])
    return b"".join(planes)


def encode_columns(doc, encode, planes, regional):
    hashes = bytearray()

    def descriptor(row, keep_digest=True):
        digest = row["sha256"]
        stem, ext = row["file"].rsplit(".", 1)
        if stem != digest or len(digest) != 64 or digest != digest.lower():
            raise ValueError("invalid immutable district filename")
        if keep_digest:
            hashes.extend(bytes.fromhex(digest))
        return dict(row, file=ext, sha256="")

    header = dict(doc, support=descriptor(doc["support"]), tiles=[])
    if "support_regions" in doc:
        header["support_regions"] = [descriptor(r) for r in doc["support_regions"]]
    for x, z, layer in doc["tiles"]:
        compact = None
        if layer is not None:
            compact = dict(descriptor(layer, not regional), bounds=[[0, 0, 0], [0, 0, 0]])
        header["tiles"].append([x, z, compact])
    metadata = encode(header)
    return struct.pack("<I", len(metadata)) + metadata + planes + hashes


def columns(doc, encode):
    planes = bound_planes(doc)
    return None if planes is None else encode_columns(doc, encode, planes, regional=False)


def pack_physics(manifest, encode):
    doc = physics_directory(manifest)
    plain = envelope(b"ODP1", encode(doc))
    raw = columns(doc, encode)
    compact = envelope(b"ODP2", raw) if raw is not None else None
    choices = [wire for wire in (plain, compact) if wire is not None]
    if not choices:
        raise ValueError("physics directory exceeds the pinned reader limit")
    return min(choices, key=len)


def regional_files(doc, directory, write, storage, bundle):
    addresses = {i: bytes.fromhex(layer["sha256"]) for i, (_, _, layer) in enumerate(doc["tiles"]) if layer}
    table = write(b"".join(addresses.values()), "digests")
    regions = []
    for region in doc["support_regions"]:
        catalogue = unpack_support((directory / region["file"]).read_bytes())
        local = b"".join(addresses[i] for i in region["tiles"])
        raw = b"ODR1" + local + catalogue
        packed, suffix = pack_catalogue(raw, storage)
        layers = [doc["tiles"][i][2] for i in region["tiles"]]
        complete = False
        if (bundle and all(layer["file"].endswith(".cpage") for layer in layers)
                and 36 + sum(layer["bytes"] for layer in layers) <= len(raw)):
            binding = bytes.fromhex(json.loads(catalogue)["directory_sha256"])
            body = b"ODR2" + binding + b"".join((directory / layer["file"]).read_bytes() for layer in layers)
            candidate, ext = pack_catalogue(body, storage)
            if len(candidate) < len(packed):
                raw, packed, suffix = body, candidate, ext
                complete = True
        descriptor = dict(write(packed, suffix), tiles=region["tiles"], decoded_bytes=len(raw))
        if complete:
            descriptor["complete_pages"] = True
        regions.append(descriptor)
    return table, regions


def pack_regional(manifest, directory, encode, write, storage, *, bundle=False):
    doc = physics_directory(manifest)
    planes = bound_planes(doc)
    if not doc.get("support_regions") or planes is None:
        return pack_physics(manifest, encode)
    doc["physical_addresses"], doc["support_regions"] = regional_files(doc, directory, write, storage, bundle)
    wire = envelope(b"ODP3", encode_columns(doc, encode, planes, regional=True))
    if wire is not None:
        return wire
    return pack_physics(manifest, encode)

def visual_header(manifest, directory, write, storage):
    """Keep visual assets independently cacheable and resolve only nearby collision addresses."""
    if manifest.get("support_regions"):
        table, regions = regional_files(physics_directory(manifest), directory, write, storage, bundle=True)
        tiles = []
        visual_addresses = bytearray()
        for row in manifest["tiles"]:
            visual = row["visual"]
            stem, suffix = visual["file"].rsplit(".", 1)
            if stem != visual["sha256"] or len(stem) != 64 or stem != stem.lower():
                raise ValueError("invalid immutable visual filename")
            visual_addresses.extend(bytes.fromhex(stem))
            row = dict(row, visual=dict(visual, file=suffix, sha256=""))
            if layer := row.get("physics"):
                suffix = layer["file"].rsplit(".", 1)[1]
                row = dict(row, physics=dict(layer, file="." + suffix, sha256=""))
            tiles.append(row)
        compact = dict(manifest, tiles=tiles, physical_addresses=table, support_regions=regions)
        return compact, visual_addresses
    return manifest, None


def visual_payload(header, addresses, encode):
    text = encode(header)
    if addresses is None:
        return b"ODI1", text
    return b"ODI2", struct.pack("<I", len(text)) + text + addresses


def pack_visual_district(manifest, directory, encode, write, storage):
    header, addresses = visual_header(manifest, directory, write, storage)
    wire = envelope(*visual_payload(header, addresses, encode))
    if wire is None and addresses is not None:
        wire = envelope(b"ODI1", encode(manifest))
    if wire is None:
        raise ValueError("district directory exceeds the pinned reader limit")
    return wire


def pack_first_frame(manifest, directory, encode, write, storage, cell=None):
    header, addresses = visual_header(manifest, directory, write, storage)
    region_id, support = 0xffffffff, None
    if cell is not None:
        tile = next((i for i, row in enumerate(manifest["tiles"]) if (row["x"], row["z"]) == tuple(cell)), None)
        for i, region in enumerate(header.get("support_regions", [])):
            if tile in region["tiles"]:
                region_id = i
                support = region
                break
        else:
            raise ValueError(f"no collision region contains first-frame tile {cell}")

    elif not header.get("support_regions") and header.get("support"):
        support = header["support"]
    attachments = [desc for desc in (header.get("models"), support) if desc is not None]
    descriptor = header["parent"]
    if descriptor["bytes"] + sum(desc["bytes"] for desc in attachments) > LIMIT:
        raise ValueError("first-frame resources exceed the pinned reader limit")
    original = (directory / descriptor["file"]).read_bytes()
    extras = b"".join((directory / desc["file"]).read_bytes() for desc in attachments)

    def candidate(parent, desc):
        kind, data = visual_payload(dict(header, parent=desc), addresses, encode)
        if 12 + len(data) + len(parent) + len(extras) > LIMIT:
            return None
        return envelope(b"ODI3", struct.pack("<4sII", kind, len(data), region_id) + data + parent + extras)

    best = candidate(original, descriptor)
    # The pin's single outer frame can compress the original GLB columns together with the directory.
    if original[:4] == b"OVC2" and 16 + struct.unpack_from("<I", original, 4)[0] + len(extras) <= LIMIT:
        plain = b"OVCU" + original[4:16] + zlib.decompress(original[16:])
        digest = hashlib.sha256(plain).hexdigest()
        desc = dict(file=digest + ".coarse", sha256=digest, bytes=len(plain))
        alternate = candidate(plain, desc)
        if alternate is not None and (best is None or len(alternate) < len(best)):
            best = alternate
    if best is None:
        raise ValueError("first-frame package exceeds the pinned reader limit")
    return best
