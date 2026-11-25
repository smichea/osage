#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Render the same district's first GPU frame from an older and a newer parent representation."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import subprocess

from paging_checks import require

FRAME_FIELDS = ("draw_owners", "fine_cells", "camera_eye", "camera_aim", "geometry_assets", "drawable_payload_bytes",
                "vertex_slice_bytes", "index_slice_bytes", "render_world_camera_meshes", "actor_bodies")


def capture(binary, pages, route, base, flags=()):
    png = base.with_suffix(".png")
    gpu = base.with_suffix(".gpu.json")
    with base.with_suffix(".log").open("wb") as log:
        subprocess.run(["flock", "/tmp/osage-ville-paris-gpu.lock", binary, "--paging", "--pages", pages,
                        "--route", route, "--tick", "0", "--delay-ms", "400", "--out", base.with_suffix(".json"),
                        "--gpu-trace", gpu, "--gpu-stride", "1", "--shot", png, *flags],
                       stdout=log, stderr=subprocess.STDOUT, check=True, timeout=180)
    receipt = json.loads(gpu.read_bytes())
    frame, = receipt["frames"]
    pixels = subprocess.check_output(["magick", png, "-depth", "8", "rgba:-"])
    require(frame["fine_cells"] == 0 and frame["draw_owners"] > 0 and len(set(pixels)) > 16, "empty or fine first view")
    return {k: frame[k] for k in FRAME_FIELDS}, pixels, receipt["adapter"]


def differences(a, b):
    require(len(a) == len(b) and len(a) % 4 == 0, "incompatible captures")
    da = zip(*[iter(a)] * 4)
    db = zip(*[iter(b)] * 4)
    levels = Counter(max(abs(x-y) for x,y in zip(p,q)) for p,q in zip(da,db,strict=True) if p != q)
    return {"pixels": len(a)//4, "different_pixels": sum(levels.values()), "deltas": levels}


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--bin", type=Path, required=True)
    ap.add_argument("--previous-pages", type=Path, required=True)
    ap.add_argument("--pages", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    a = ap.parse_args()
    out = a.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    indices = [json.loads((p/"index.json").read_bytes()) for p in [a.previous_pages, a.pages]]
    require(indices[0]["source_sha256"] == indices[1]["source_sha256"] and indices[0]["tiles"] == indices[1]["tiles"],
            "district or fine geometry changed")
    result = {}
    for route in ("rail", "roof", "road"):
        runs = [capture(a.bin, pages, route, out/f"{route}-{name}") for name,pages in
                (("previous", a.previous_pages), ("current", a.pages), ("repeat", a.previous_pages))]
        fields, base, adapter = runs[0]
        require(all(f == fields and dev == adapter for f,_,dev in runs), f"{route}: GPU input changed")
        versus_new = differences(base, runs[1][1])
        versus_repeat = differences(base, runs[2][1])
        # Headless GPU exports of two draws of the same frame can differ at shared edges.
        # Report the full distribution and reject a change affecting a visible part of the frame.
        require(versus_new["different_pixels"] <= max(512, 3 * versus_repeat["different_pixels"]),
                f"{route}: {versus_new['different_pixels']} changed pixels, control {versus_repeat['different_pixels']}")
        result[route] = {"adapter": adapter, "gpu_input": fields, "different_format": versus_new,
                         "same_format_repeat": versus_repeat, "frame_hashes": [hashlib.sha256(v[1]).hexdigest() for v in runs]}
    with a.bin.open("rb") as f:
        version = hashlib.file_digest(f, "sha256").hexdigest()
    (out/"summary.json").write_text(json.dumps({"binary_sha256": version, "source_sha256": indices[0]["source_sha256"],
               "parent_sha256": [i["parent"]["sha256"] for i in indices], "routes": result}, indent=2) + "\n")
    print({k: (v["different_format"]["different_pixels"], v["same_format_repeat"]["different_pixels"]) for k,v in result.items()})


if __name__ == "__main__":
    main()
