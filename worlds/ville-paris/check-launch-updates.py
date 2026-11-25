#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Exercise a chain of first-frame releases, including a changed bridge and a broken update."""
import argparse
import hashlib
import json
from pathlib import Path
import runpy
import tempfile

from paging_checks import require
from paging_launch_update import address, dictionary_patch

HERE = Path(__file__).resolve().parent
first = runpy.run_path(str(HERE / "check-first-frame.py"))
io = runpy.run_path(str(HERE / "check-impact-transmission.py"))


def filename(pin, extension):
    return pin.split(":")[0] + "." + extension


def prepare(pages, out):
    original = HERE / "fixtures/bir-hakeim.glb"
    cook = first["cook"]
    one = cook(original, pages, 128, first_frame_pin=True)
    doc, chunk = io["read_glb"](original)
    doc["asset"]["generator"] = "Ville-Paris snapshot revision qualification"
    metadata = out / "metadata.glb"
    io["write_glb"](metadata, doc, chunk)
    two = cook(metadata, pages, 128, first_frame_pin=True, previous_first_frame=one)
    water = next(m for m in doc["materials"] if m["name"] == "water")
    water["pbrMetallicRoughness"]["baseColorFactor"] = [0.8, 0.1, 0.4, 1]
    quay = next(n for n in doc["nodes"] if n.get("name") == "support/quay--1")
    quay["translation"][1] += 1.0
    changed = out / "changed.glb"
    io["write_glb"](changed, doc, chunk)
    three = cook(changed, pages, 128, first_frame_pin=True, previous_first_frame=two)
    require(len(two.split("~")) == len(three.split("~")) == 5, "expected economical updates")
    return one, two, three


def qualify(binary, out):
    pages = out / "pages"
    one, two, three = prepare(pages, out)
    (out / "pins.json").write_text(json.dumps([one, two, three], indent=2) + "\n")
    root2, _, _, _, patch2 = two.split("~")
    root3, raw3, _, _, patch3 = three.split("~")
    runs = {}
    def launch(label, pin, cache, expected, **faults):
        missing = []
        with first["origin"](pages, missing=missing, **faults) as (url, rows):
            receipt = first["replay"](binary, url, out/(label+".json"), "rail",
                                      ["--tick", "0", "--content-cache", cache, "--index-pin", pin])
        require(not missing and [r["name"] for r in rows] == expected, f"{label}: {rows}, missing {missing}")
        runs[label] = sum(r["bytes"] for r in rows)
        require(receipt["transport"]["io"]["content_body_bytes"] == runs[label], "reader and origin disagree")
        return receipt

    with tempfile.TemporaryDirectory(dir=out) as tmp:
        cache = Path(tmp)
        launch("cold", two, cache, [filename(root2, "district")])
        launch("warm", two, cache, [])
    with tempfile.TemporaryDirectory(dir=out) as tmp:
        cache = Path(tmp)
        launch("install", one, cache, [filename(one, "district")])
        launch("metadata-update", two, cache, [filename(patch2, "delta")])
        launch("geometry-update", three, cache, [filename(patch3, "delta")])
        # Both wire snapshots are absent. This launch uses only the verified decoded target.
        require(not (cache/filename(root3, "district")).exists(), "unexpected compressed target")
        launch("reopen", three, cache, [])
        raw = (cache/filename(raw3, "frame")).read_bytes()
        require(address(raw) == raw3, "decoded cache identity")
        control = first["pack_parts"](pages, root3)
        require(control[1] == raw, "patched package differs byte for byte from full release")
        captures = []
        for label, pin, local in [("original", one, None), ("full", root3, None), ("patched", three, cache),
                                  ("full-repeat", root3, None)]:
            with first["origin"](pages) as (url, rows):
                flags = ["--index-pin", pin] + (["--content-cache", local] if local else [])
                captures.append(first["frames"]["capture"](binary, url, "rail", out/("view-"+label), flags))
        before, full, patched, repeat = captures
        require(full[0] == patched[0] and full[2] == patched[2], "changed frame inputs")
        compare = first["frames"]["differences"]
        visible = compare(before[1], full[1])["different_pixels"]
        difference = compare(full[1], patched[1])["different_pixels"]
        variability = compare(full[1], repeat[1])["different_pixels"]
        require(visible > 512 and difference <= max(512, 3 * variability), "unexpected visual difference")
        probes = []
        for label, pin, local in [("original", one, None), ("full", root3, None), ("patched", three, cache)]:
            with first["origin"](pages) as (url, _):
                flags = ["--tick", "0", "--all-resident", "--no-visuals", "--index-pin", pin,
                         "--collision-ray", "-120,12,100", "-120,-10,100"]
                if local:
                    flags.extend(["--content-cache", local])
                result = first["replay"](binary, url, out/("quay-"+label+".json"), "road", flags)
                probe, = result["collision_probes"]
                require(probe["coverage"]["complete"] and probe["intersections"][0]["id"] == "quay--1",
                        "quay collision is missing")
                probes.append(probe)
        require(probes[1] == probes[2], "patched quay collision differs from full version")
        heights = [p["intersections"][0]["point_m"][1] for p in probes[:2]]
        require(abs(heights[1] - heights[0] - 1) < 1e-5, "quay did not move by one metre")
    with tempfile.TemporaryDirectory(dir=out) as tmp:
        cache = Path(tmp)
        old_name, new_name, delta = filename(one,"district"), filename(root2,"district"), filename(patch2,"delta")
        (cache/old_name).write_bytes(b"truncated")
        launch("damaged-base", two, cache, [new_name])
        (cache/new_name).unlink()
        launch("restore-base", one, cache, [old_name])
        launch("damaged-patch", two, cache, [delta, new_name], corrupt_once=delta)
        (cache/new_name).unlink()
        launch("unavailable-patch", two, cache, [delta, new_name], fail_once=delta)
        (cache/new_name).unlink()
        # A valid patch can still reconstruct another target. The decoded identity must be checked.
        wrong = bytearray(first["pack_parts"](pages, root2)[1])
        wrong[-1] ^= 1
        bad = dictionary_patch(first["pack_parts"](pages, one)[1], wrong)
        bad_name = filename(address(bad), "delta")
        (pages/bad_name).write_bytes(bad)
        launch("wrong-target", "~".join([*two.split("~")[:-1], address(bad)]), cache, [bad_name, new_name])
        (cache/new_name).unlink()
        bad = b"unusable update"
        bad_name = filename(address(bad), "delta")
        (pages/bad_name).write_bytes(bad)
        launch("invalid-frame", "~".join([*two.split("~")[:-1], address(bad)]), cache, [bad_name, new_name])
    return {"http_body_bytes": runs, "quay_height_m": heights, "reference_bytes": len(three), "full_geometry_bytes": int(root3.split(":")[1]),
            "geometry_edit_changed_pixels": visible, "patched_vs_full_pixels": difference, "full_repeat_pixels": variability,
            "decoded_geometry_sha256": hashlib.sha256(raw).hexdigest()}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    summary = json.dumps(qualify(args.bin.resolve(), out), indent=2)
    (out / "summary.json").write_text(summary + "\n")
    print(summary)
