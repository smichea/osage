#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

"""Convert a static drawable and bounds, or an articulated fragment, to one Osage GLB."""

import argparse
import hashlib
import json
import math
import os
import shutil
import subprocess
import tempfile
import tomllib
import xml.etree.ElementTree as ET
from pathlib import Path

from appearance import apply_appearance
from fragment import read_fragment
from glb import Glb
from lod_error import measure_lods
from xml_geometry import add_bounds, read_drawable


def parse_source(path, extension, tool, report, role):
    data = path.read_bytes()
    metadata = {"name": path.name, "sha256": hashlib.sha256(data).hexdigest()}
    report["inputs"][role] = metadata
    if path.suffix.lower() == ".xml":
        return ET.fromstring(data)
    if path.suffix.lower() != "." + extension:
        raise ValueError(f"{path}: expected .{extension} or .xml")
    if data[:4] != b"RSC7":
        raise ValueError(f"{path}: expected a loose legacy RSC7 resource")
    if not tool:
        raise ValueError(f"{path}: binary input needs --resource-tool pointing to a compatible local executable")
    executable = shutil.which(os.fspath(tool))
    if not executable:
        raise ValueError(f"cannot find resource tool {tool}")
    executable = str(Path(executable).resolve())
    if "resourceTool" not in report:
        version = subprocess.run([executable, "--version"], capture_output=True, check=True, text=True)
        with open(executable, "rb") as f:
            digest = hashlib.file_digest(f, "sha256").hexdigest()
        report["resourceTool"] = {"version": version.stdout.strip(), "sha256": digest}
    with tempfile.TemporaryDirectory(prefix="osage-object-") as tmp:
        directory = Path(tmp)
        source = directory / ("input." + extension)
        source.write_bytes(data)
        output = directory / "output.xml"
        result = subprocess.run([executable, "resource", "dump", "--no-update-check", "--no-dds",
                                 source.name, "-o", output.name], cwd=directory, capture_output=True, check=False, text=True)
        if result.returncode:
            raise ValueError(f"{path}: resource dump failed ({result.returncode}): {result.stderr.strip()}")
        diagnostics = [line.replace(str(directory) + os.sep, "") for line in result.stderr.splitlines()]
        diagnostics = [line for line in diagnostics if line and line != "Wrote output.xml"]
        if diagnostics:
            metadata["decoderMessages"] = diagnostics
        return ET.parse(output).getroot()


def material_tables(data):
    tables = tomllib.loads(data.decode("utf-8"))
    for key, profile in tables.items():
        if not isinstance(profile, dict) or set(profile) - {
            "friction", "restitution", "absorption", "transmission", "scattering",
        }:
            raise ValueError(f"{key}: unknown material keys")
        physical = [k for k in ("friction", "restitution") if k in profile]
        if physical and len(physical) != 2:
            raise ValueError(f"{key}: supply friction and restitution together")
        if physical:
            for prop in physical:
                x = profile[prop]
                if isinstance(x, bool) or not isinstance(x, (int, float)) or not math.isfinite(x) or x < 0:
                    raise ValueError(f"{key}: {prop} must be a finite nonnegative number")
            if profile["restitution"] > 1:
                raise ValueError(f"{key}: restitution must be within [0,1]")
        acoustic = [k for k in ("absorption", "transmission", "scattering") if k in profile]
        if acoustic:
            if len(acoustic) != 3 or any(not isinstance(profile[k], list) or len(profile[k]) != 3 for k in acoustic[:2]):
                raise ValueError(f"{key}: acoustic data needs three bands and a scattering coefficient")
            values = profile["absorption"] + profile["transmission"] + [profile["scattering"]]
            if any(isinstance(x, bool) or not isinstance(x, (int, float)) or not 0 <= x <= 1 for x in values):
                raise ValueError(f"{key}: expected acoustic energy fractions within [0,1]")
            if any(a + t > 1 for a, t in zip(profile["absorption"], profile["transmission"])):
                raise ValueError(f"{key}: absorption plus transmission exceeds one")
    return tables


def convert(*, object_id, drawable=None, bounds=None, acoustic_bounds=None,
            acoustic_proxy=False, materials=None, resource_tool=None, fragment=None, fragment_core=None, rig_space=None,
            lod_error=None, appearance=None, output_path=None, ktx_tool=None, sphere_radius=None, joint_frames=None, swing_envelope=None):
    if not object_id or object_id.strip() != object_id or any(ord(c) < 32 or c == "," for c in object_id):
        raise ValueError("object ID must be nonempty, trimmed and usable in a contact trace")
    if fragment and (drawable or bounds or acoustic_bounds or acoustic_proxy):
        raise ValueError("supply the articulated fragment alone, optionally with a material map")
    if rig_space is not None and (not fragment or rig_space not in ("bone","actor")):
        raise ValueError("--rig-space bone|actor requires --fragment")
    if swing_envelope is not None and (not fragment or swing_envelope != "angular-ellipse"):
        raise ValueError("--swing-envelope angular-ellipse requires an articulated fragment")
    if joint_frames is not None and (not fragment or joint_frames not in ("joint-to-body-x", "body-to-joint-z")):
        raise ValueError("--joint-frames requires a fragment and a supported convention")
    if sphere_radius is not None and (not fragment or sphere_radius not in ("bound", "margin")):
        raise ValueError("--sphere-radius bound|margin requires --fragment")
    if fragment_core and not fragment:
        raise ValueError("--fragment-core requires --fragment")
    if not (drawable or bounds or acoustic_bounds or fragment):
        raise ValueError("supply a drawable, bounds or acoustic bounds file")
    if acoustic_proxy and acoustic_bounds:
        raise ValueError("choose independent acoustic bounds or collision proxies")
    glb = Glb(object_id)
    report = {"inputs": {}, "bounds": [], "notes": [], "materialOrigins": {}}
    if appearance and not (drawable or fragment):
        raise ValueError("--appearance needs a drawable or an articulated fragment")
    if appearance:
        report["appearance"] = {}
    table = {}
    if materials:
        data = materials.read_bytes()
        table = material_tables(data)
        report["inputs"]["materials"] = {"name": materials.name, "sha256": hashlib.sha256(data).hexdigest()}

    def add_surface(id_, mat_type, vertices, triangles, *, acoustic=False):
        key = f"surface-{mat_type}"
        local = table.get(key, {})
        mesh = glb.mesh(id_, [glb.primitive(vertices, [i for tri in triangles for i in tri])])
        origins = report["materialOrigins"].setdefault(key, {})
        if not acoustic:
            if "friction" in local:
                properties = {k: local[k] for k in ("friction", "restitution")}
                origins["collision"] = "mapping"
            else:
                properties = {"friction": 0.5, "restitution": 0.0}
                origins["collision"] = "Osage defaults"
            glb.profile["collisionMaterials"][key] = properties
            glb.node_meta["collision"].append({"id": id_, "mesh": mesh, "material": key})
        if acoustic or acoustic_proxy:
            if "absorption" in local:
                response = {k: local[k] for k in ("absorption", "transmission", "scattering")}
                origins["acoustic"] = "mapping"
            else:
                response = {}
                origins["acoustic"] = "unknown"
            glb.profile["acousticMaterials"][key] = response
            glb.node_meta["acoustic"].append({
                "id": id_, "mesh": mesh, "material": key,
                "source": "authored" if acoustic else "collision-proxy",
            })

    if fragment:
        data = fragment.read_bytes()
        report["inputs"]["fragment"] = {"name": fragment.name, "sha256": hashlib.sha256(data).hexdigest()}
        if fragment.suffix.lower() != ".xml":
            if fragment.suffix.lower() != ".yft" or data[:4] != b"RSC7" or not fragment_core:
                raise ValueError("loose .yft input requires an RSC7 header and --fragment-core")
            from fragment_bridge import transcode
            report["fragmentCoreSha256"] = hashlib.sha256(fragment_core.read_bytes()).hexdigest()
            data = transcode(data, fragment_core, encode=False)
        fragment_root = ET.fromstring(data)
        read_fragment(fragment_root, glb, report, table, rig_space or "bone", sphere_radius, joint_frames, swing_envelope)
        visual_root = fragment_root.find("Drawable")
    embedded = None
    if drawable:
        root = parse_source(drawable, "ydr", resource_tool, report, "drawable")
        read_drawable(root, glb, report)
        visual_root = root
        embedded = root.find("Bounds")
    if bounds:
        phys = parse_source(bounds, "ybn", resource_tool, report, "collision")
        if embedded is not None:
            report["notes"].append("Explicit bounds take precedence over the embedded drawable bounds.")
    else:
        phys = embedded
    if phys is not None:
        add_bounds(phys, object_id + "/bounds", add_surface, report)
    if acoustic_proxy and not glb.node_meta["collision"]:
        raise ValueError("acoustic proxies require collision surfaces")
    if acoustic_bounds:
        acoustic = parse_source(acoustic_bounds, "ybn", resource_tool, report, "acoustic")
        def emit_acoustic(id_, mat, positions, triangles):
            add_surface(id_, mat, positions, triangles, acoustic=True)
        add_bounds(acoustic, object_id + "/acoustic", emit_acoustic, report)
    if report["bounds"]:
        report["notes"].append("The mesh carries the original triangle faces and exact box faces. Source margins, edge flags, filter masks, shrunk vertices and acceleration trees are not applied by this converter.")
    unused = sorted(set(table) - set(report["materialOrigins"]))
    if unused:
        report["unusedMaterialKeys"] = unused
    if appearance:
        apply_appearance(appearance, visual_root, glb, report, output_path, ktx_tool)
    if lod_error is not None:
        measure_lods(glb, lod_error, report)
    glb.doc["asset"]["extras"]["osageImport"] = report
    return glb.encode(), report


def publish(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(dir=path.parent)
    pending = Path(name)
    try:
        with os.fdopen(fd, "wb") as tmp:
            tmp.write(data)
        os.replace(pending, path)
    finally:
        pending.unlink(missing_ok=True)


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--id", required=True, dest="object_id")
    ap.add_argument("--drawable", type=Path, help="static .ydr.xml or .ydr")
    ap.add_argument("--bounds", type=Path, help="external .ybn.xml or .ybn (replaces embedded bounds)")
    ap.add_argument("--fragment", type=Path, help="articulated .yft.xml or loose .yft")
    ap.add_argument("--fragment-core", type=Path, help="local CodeWalker.Core assembly for .yft binary input")
    ap.add_argument("--sphere-radius", choices=("bound", "margin"),
                    help="fragment sphere radius from SphereRadius (default) or the contact Margin field")
    ap.add_argument("--swing-envelope", choices=("angular-ellipse",),
                    help="couple the two positive transverse swing ranges in an angular ellipse")
    ap.add_argument("--joint-frames", choices=("joint-to-body-x", "body-to-joint-z"),
                    help="joint-to-body axes with twist on X (default) or body-to-joint matrix with twist on Z")
    ap.add_argument("--rig-space", choices=("bone","actor"), help="bone-local rigid bind data (default) or legacy actor space")
    acoustic = ap.add_mutually_exclusive_group()
    acoustic.add_argument("--acoustic-proxy", action="store_true", help="explicitly reuse the collision faces for sound")
    acoustic.add_argument("--acoustic-bounds", type=Path, help="an independently authored surface in bounds format")
    ap.add_argument("--materials", type=Path, help="TOML map keyed by surface-N, N is the source material type")
    ap.add_argument("--ktx-tool", type=Path, help="local Khronos ktx executable for explicitly supplied Basis Universal mip levels")
    ap.add_argument("--appearance", type=Path, help="JSON manifest of diffuse image names and portable raster settings")
    ap.add_argument("--resource-tool", type=Path, help="local executable implementing resource dump for loose binary inputs")
    ap.add_argument("--lod-error", nargs="?", type=float, const=0.01, metavar="TOLERANCE_M",
                    help="bound each visual LOD against the primary mesh (default gap 0.01 m)")
    ap.add_argument("--report", action="store_true", help="also print the import notes embedded in the GLB as JSON")
    ap.add_argument("-o", "--output", type=Path, required=True)
    args = ap.parse_args()
    output = args.output
    inputs = (args.drawable, args.bounds, args.acoustic_bounds, args.materials, args.appearance, args.ktx_tool, args.resource_tool, args.fragment, args.fragment_core)
    if any(src and src.resolve() == output.resolve() for src in inputs):
        ap.error("choose a separate output path")
    options = vars(args).copy()
    del options["output"], options["report"]
    try:
        data, report = convert(**options, output_path=output.resolve())
        publish(output, data)
    except (ValueError, KeyError, OverflowError, OSError, ET.ParseError, subprocess.SubprocessError) as exc:
        ap.exit(1, f"conversion failed: {exc}\n")
    if args.report:
        print(json.dumps(report, ensure_ascii=False, indent=2))
    else:
        print(f"{output} ({len(data)} bytes)")


if __name__ == "__main__":
    main()
