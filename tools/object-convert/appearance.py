# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

"""Bind independently supplied base-colour images to authored diffuse sampler names."""

import hashlib
import json
import math
import re
import struct

from portable_images import bind_basisu, image_info


def unique_keys(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate appearance manifest key {key!r}")
        result[key] = value
    return result


def shader_key(name):
    """The public interchange can spell an ASCII shader by name or its uint32 one-at-a-time hash."""
    if not isinstance(name, str) or not name:
        raise ValueError("appearance shader guard needs a name or hash")
    if re.fullmatch(r"hash_[0-9a-f]{8}", name, flags=re.IGNORECASE):
        return int(name[5:], 16)
    if not name.isascii():
        return name
    value = 0
    for byte in name.lower().encode("ascii"):
        value = (value + byte) & 0xFFFFFFFF
        value = (value + (value << 10)) & 0xFFFFFFFF
        value ^= value >> 6
    value = (value + (value << 3)) & 0xFFFFFFFF
    value ^= value >> 11
    return (value + (value << 15)) & 0xFFFFFFFF


def raster_policy(recipe, slot, name):
    if not isinstance(recipe, dict) or set(recipe) - {
        "shader",
        "alphaMode",
        "alphaCutoff",
        "doubleSided",
        "vertexColorAlpha",
    }:
        raise ValueError(
            f"material slot {slot}: unknown appearance property or invalid object"
        )
    if "shader" in recipe and shader_key(recipe["shader"]) != shader_key(name):
        raise ValueError(
            f"material slot {slot}: expected shader {recipe['shader']!r}, found {name!r}"
        )
    mode = recipe.get("alphaMode", "OPAQUE")
    if mode not in ("OPAQUE", "MASK", "BLEND"):
        raise ValueError(f"material slot {slot}: expected OPAQUE, MASK or BLEND")
    sided = recipe.get("doubleSided", False)
    if not isinstance(sided, bool):
        raise ValueError(f"material slot {slot}: doubleSided must be boolean")  # noqa: TRY004 -- invalid manifest data
    settings = {"alphaMode": mode, "doubleSided": sided}
    if mode == "MASK":
        threshold = recipe.get("alphaCutoff", 0.5)
        if (
            isinstance(threshold, bool)
            or not isinstance(threshold, (float, int))
            or not math.isfinite(threshold)
            or not 0 <= threshold <= 1
        ):
            raise ValueError(
                f"material slot {slot}: alphaCutoff must be a finite fraction between zero and one"
            )
        settings["alphaCutoff"] = threshold
    elif "alphaCutoff" in recipe:
        raise ValueError(f"material slot {slot}: alphaCutoff requires MASK")
    if recipe.get("vertexColorAlpha", "source") not in ("source", "one"):
        raise ValueError(
            f"material slot {slot}: vertexColorAlpha must be 'source' or 'one'"
        )
    return settings


def unit_vertex_alpha(glb, attributes):
    source = attributes["COLOR_0"]
    # Each source colour is emitted by the XML reader as a tightly packed, normalized RGBA8.
    accessor = glb.doc["accessors"][source]
    view = glb.doc["bufferViews"][accessor["bufferView"]]
    start = view["byteOffset"]
    rgba = glb.binary[start : start + view["byteLength"]]
    attributes["_OSAGE_SOURCE_COLOR_0"] = source
    if all(alpha == 255 for alpha in rgba[3::4]):
        return
    rendered = [(r, g, b, 255) for r, g, b, _ in struct.iter_unpack("<4B", rgba)]
    attributes["COLOR_0"] = glb.accessor(rendered, "B", 4)


def apply_appearance(path, drawable, glb, report, output_path=None, ktx_tool=None):
    encoded = path.read_bytes()
    manifest = json.loads(encoded, object_pairs_hook=unique_keys)
    if (
        not isinstance(manifest, dict)
        or set(manifest) - {"version", "textures", "materials", "basisuMips"}
        or type(manifest.get("version")) is not int
        or manifest["version"] != 1
    ):
        raise ValueError(
            "expected an appearance manifest version 1 with textures and optional materials"
        )
    textures, recipes = manifest.get("textures"), manifest.get("materials", {})
    if not isinstance(textures, dict) or not isinstance(recipes, dict):
        raise ValueError("appearance textures and materials must be objects")  # noqa: TRY004 -- invalid manifest data
    paths = {}
    for name, entry in textures.items():
        if not name.strip() or not isinstance(entry, str) or not entry.strip():
            raise ValueError(
                "appearance texture names and image paths must be nonempty strings"
            )
        key = name.casefold()
        if key in paths:
            raise ValueError(f"ambiguous case-insensitive diffuse name {name!r}")
        image_path = (path.parent / entry).resolve()
        if image_path == output_path:
            raise ValueError(f"choose a separate output path for image {entry!r}")
        paths[key] = image_path
    shaders = drawable.findall("ShaderGroup/Shaders/Item")
    unknown_slots = set(recipes) - {str(i) for i in range(len(shaders))}
    if unknown_slots:
        raise ValueError(
            f"appearance references unknown material slots {sorted(unknown_slots)}"
        )
    report["inputs"]["appearance"] = {
        "name": path.name,
        "sha256": hashlib.sha256(encoded).hexdigest(),
    }
    analysis = report["appearance"]
    analysis["materials"] = []
    bound, alias_paths, images = set(), {}, {}
    uses = {}
    for mesh in glb.doc["meshes"]:
        for prim in mesh["primitives"]:
            if "material" in prim:
                uses.setdefault(prim["material"], []).append(
                    (mesh["name"], prim["attributes"])
                )
    for slot, shader in enumerate(shaders):
        material = glb.doc["materials"][slot]
        recipe = recipes.get(str(slot), {})
        settings = raster_policy(recipe, slot, material["name"])
        material.update(settings)
        params = shader.findall("Parameters/Item")
        diffuse = [
            p for p in params if (p.get("name") or "").casefold() == "diffusesampler"
        ]
        if len(diffuse) > 1:
            raise ValueError(f"material slot {slot}: ambiguous DiffuseSampler")
        row = {
            "slot": slot,
            "shader": material["name"],
            "portableRaster": settings,
            "explicitRasterProperties": sorted(
                set(recipe) - {"shader", "vertexColorAlpha"}
            ),
            "sourceProgram": "untranslated",
            "untranslatedParameters": [
                p.get("name") for p in params if p not in diffuse
            ],
        }
        analysis["materials"].append(row)
        if "vertexColorAlpha" in recipe:
            row["vertexColorAlpha"] = recipe["vertexColorAlpha"]
        if recipe.get("vertexColorAlpha") == "one":
            for _, attributes in uses.get(slot, []):
                if "COLOR_0" in attributes:
                    unit_vertex_alpha(glb, attributes)
                    row["originalVertexColour"] = "_OSAGE_SOURCE_COLOR_0"
        if slot not in uses:
            row["status"] = "unused material slot"
            continue
        if (
            not diffuse
            or (diffuse[0].get("type") or "").casefold() != "texture"
            or not (name := (diffuse[0].findtext("Name") or "").strip())
        ):
            row["status"] = "no named diffuse texture"
            continue
        row["diffuseName"] = name
        key = name.casefold()
        if key not in paths:
            row["status"] = "no image supplied for diffuse name"
            continue
        bound.add(key)
        locations = [
            mesh_name
            for mesh_name, attributes in uses[slot]
            if "TEXCOORD_0" not in attributes
        ]
        if locations:
            raise ValueError(
                f"material slot {slot} ({name}): missing TEXCOORD_0 in {', '.join(locations)}"
            )
        image = paths[key]
        if image not in alias_paths:
            try:
                data = image.read_bytes()
                mime, _ = image_info(data)
            except (OSError, ValueError) as exc:
                raise ValueError(
                    f"material slot {slot}, diffuse {name!r}, image {image.name!r}: {exc}"
                ) from exc
            digest = hashlib.sha256(data).hexdigest()
            if digest not in images:
                images[digest] = glb.texture(name, mime, data)
            alias_paths[image] = (images[digest], digest)
        index, digest = alias_paths[image]
        material["pbrMetallicRoughness"]["baseColorTexture"] = {
            "index": index,
            "texCoord": 0,
        }
        row.update(
            status="base colour texture bound",
            image=image.name,
            imageSha256=digest,
            texture=index,
        )
    analysis["unusedTextureNames"] = sorted(
        name for name in textures if name.casefold() not in bound
    )
    analysis["images"] = len(images)
    if "basisuMips" in manifest:
        bind_basisu(
            manifest["basisuMips"],
            paths,
            path.parent,
            output_path,
            ktx_tool,
            glb,
            analysis,
        )
    report["notes"].append(
        "DiffuseSampler supplies the base colour on stored UV0. glTF combines it with COLOR_0 where present. Source shader programs, original mip levels, filter states and other sampler or shader parameters are not reproduced."
    )
