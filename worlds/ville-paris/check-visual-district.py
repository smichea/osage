#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Measure the first rendered district frame, fine tile publication and contact under the same camera."""
import argparse
import hashlib
import json
from pathlib import Path
import runpy
import struct
import subprocess
import tempfile

from paging_checks import require
from paging_physics_directory import envelope
from paging_support_storage import unpack_support

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
base = runpy.run_path(str(HERE/"check-support-regions.py"))
frames = runpy.run_path(str(HERE/"check-bootstrap-frames.py"))
cook, origin, replay = (base[k] for k in ("cook", "origin", "replay"))


def read(path):
    return json.loads(path.read_bytes())


def metadata(pages, pin):
    digest, length = pin.split(":")
    wire = (pages/(digest+".district")).read_bytes()
    require(wire[:4] == b"ODI2" and hashlib.sha256(wire).hexdigest() == digest and len(wire) == int(length),
            "unverified visual directory")
    data = unpack_support(wire[8:])
    size, = struct.unpack_from("<I", data)
    header = json.loads(data[4:4+size])
    column = data[4+size:]
    index = read(pages/"index.json")
    require(len(column) == 32*len(index["tiles"]), "invalid visual digest count")
    for i, (tile, saved) in enumerate(zip(header["tiles"], index["tiles"], strict=True)):
        v = tile["visual"]
        digest = column[32*i:32*i+32].hex()
        require(v["sha256"] == "" and dict(v, sha256=digest, file=digest+"."+v["file"]) == saved["visual"],
                "visual addresses, dimensions or owners changed")
        rest = dict(tile, visual=saved["visual"])
        if layer := saved.get("physics"):
            require(rest["physics"] == dict(layer, sha256="", file="."+layer["file"].rsplit(".",1)[1]),
                    "physical metadata changed")
            rest["physics"] = layer
        require(rest == saved, "tile placement changed")
    require({k:v for k,v in header.items() if k not in ("physical_addresses","tiles","support_regions")} ==
            {k:v for k,v in index.items() if k not in ("tiles","support_regions")}, "district contents changed")
    return header, column


def corruptions(binary, out, pages, header, column):
    def expect(root, hashes, message):
        text = json.dumps(root, separators=(",",":")).encode()
        wire = envelope(b"ODI2", struct.pack("<I",len(text))+text+hashes)
        pin = base["pin_bytes"](pages, wire)
        base["reject"](binary, pages, pin, out, message, flags=())
    expect(header, column[:-1], "visual address count mismatch")
    expect(header, column+bytes(32), "visual address count mismatch")
    visual = header["tiles"][0]["visual"]
    for key in ("file", "sha256"):
        old, visual[key] = visual[key], "unexpected"
        expect(header, column, "invalid visual address column")
        visual[key] = old


def traces(binary, pages, out, case, pins):
    flags = ["--trace-body", "--delay-ms", "0", "--visual-radius", "180"]
    runs = [replay(binary, pages, out/f"{case}-route-{name}.json", case, [*flags, "--index-pin", pin])
            for name, pin in pins.items()]
    original = runs[0]
    for current in runs[1:]:
        for key in ("source_sha256","body_before_step","final_foot_m","visual","visual_events","published"):
            require(current[key] == original[key], f"{case} changed {key}")
        for key in ("visual_lods","model_events"):
            require(current["storage"][key] == original["storage"][key], f"{case} changed {key}")
        require({k:v for k,v in original["first_contact"].items() if k != "geometry"} ==
                {k:v for k,v in current["first_contact"].items() if k != "geometry"}, "contact changed")
    ready = [e for e in original["visual_events"] if e["state"] == "ready"]
    require(ready and original["published"]["fine_triangles"] > 0, "route never drew detail")
    return dict(first_fine_tick=ready[0]["tick"], first_fine_cell=ready[0]["tile"],
                clock_hz=original["clock_hz"], samples=len(original["body_before_step"]),
                contact_tick=original["first_contact"]["tick"], final_fine_triangles=original["published"]["fine_triangles"])


def first_detail(binary, out, pages, case, tick, pins):
    before, after = [], []
    for name in ("full", "regional"):
        target = out/f"{case}-first-detail-{name}"
        gpu = target.with_suffix(".gpu.json")
        with target.with_suffix(".log").open("wb") as log:
            subprocess.run(["flock", "/tmp/osage-ville-paris-gpu.lock", binary, "--paging", "--pages", pages,
                            "--index-pin", pins[name], "--route", case, "--tick", str(tick), "--delay-ms", "0",
                            "--visual-radius", "180", "--gpu-trace", gpu, "--gpu-stride", str(max(1,tick-1)),
                            "--out", target.with_suffix(".json"), "--shot", target.with_suffix(".png")],
                           check=True, timeout=180, stdout=log, stderr=subprocess.STDOUT)
        trace = read(gpu)["frames"]
        states = {f["tick"]: {k:f[k] for k in frames["FRAME_FIELDS"]} for f in trace}
        before.append(states[tick-1])
        after.append(states[tick])
    require(before[0] == before[1] and after[0] == after[1], "GPU detail publication changed")
    require(before[0]["fine_cells"] == 0 and after[0]["fine_cells"] > 0, "unconfirmed detail handoff")
    return dict(fine_cells=after[0]["fine_cells"], previous_draw_owners=before[0]["draw_owners"],
                draw_owners=after[0]["draw_owners"], prepared_camera_meshes=after[0]["render_world_camera_meshes"])


def first_frame(binary, out, pages, case, pins, header):
    result = {}
    images = []
    groups = {r["file"] for r in header["support_regions"]}
    for label, pin in pins.items():
        with origin(pages, mbps=5) as (url, records), tempfile.TemporaryDirectory(dir=out) as cache:
            opts = ["--index-pin", pin, "--content-cache", cache]
            frame, image, adapter = frames["capture"](binary, url, case, out/f"{case}-frame-{label}", opts)
            names = {r["name"] for r in records}
            io = read(out/f"{case}-frame-{label}.json")["transport"]["io"]
            total = sum(r["bytes"] for r in records)
            require(len(records) == len(names) == io["content_requests"] and total == io["content_body_bytes"]
                    and all(r["status"] == 200 and r["sha256"] == Path(r["name"]).stem for r in records),
                    "origin and client disagree on bootstrap")
            if label == "regional":
                root_parent = {pin.split(":")[0]+".district", header["parent"]["file"]}
                optional = {t["visual"]["file"] for t in read(pages/"index.json")["tiles"]}
                optional.update(r["file"] for r in header["model_resources"])
                require(root_parent <= names and names <= groups | root_parent | optional,
                        "unexpected bootstrap resource")
                events = read(out/f"{case}-frame-{label}.json")["support"]["regions"]["events"]
                speculative = {i for t,i,event in events if t == 0 and event == "prefetched"}
                spawn_groups = {header["support_regions"][i]["file"] for t,i,event in events
                                if t == 0 and event == "installed" and i not in speculative}
                required = root_parent | spawn_groups
                with tempfile.TemporaryDirectory(dir=out) as tmp:
                    sparse = Path(tmp)
                    for file in required:
                        (sparse/file).symlink_to(pages/file)
                    with origin(sparse) as (sparse_url, sparse_rows):
                        probe = frames["capture"](binary,sparse_url,case,out/f"{case}-frame-sparse",["--index-pin",pin])
                    require(probe[0] == frame and {r["name"] for r in sparse_rows if r["status"]==200} == required,
                            "sparse first frame changed GPU inputs")
                    for file in required:
                        (sparse/file).unlink()
                        missing = sparse/"missing.json"
                        run = subprocess.run([binary,"--paging","--pages",sparse,"--route",case,"--index-pin",pin,
                                              "--tick","0","--out",missing],capture_output=True)
                        require(run.returncode and not missing.exists(), "surplus claimed bootstrap dependency")
                        (sparse/file).symlink_to(pages/file)
                result["required_files"] = sorted(required)
                result["required_bytes"] = sum((pages/file).stat().st_size for file in required)
                (out/f"{case}-regional-origin.json").write_text(json.dumps(records,indent=2)+"\n")
                n = len(records)
                again = frames["capture"](binary, url, case, out/f"{case}-frame-warm", opts)
                require(len(records) == n, "warm frame fetched data")
                images.append(again)
        images.append((frame, image, adapter))
        result[label] = dict(bytes=total, requests=len(names), files=sorted(names))
    ref, image, adapter = images[0]
    require(all(f == ref and dev == adapter for f,_,dev in images), "first GPU inputs changed")
    same = frames["differences"](images[-1][1], images[-2][1])["different_pixels"]
    changes = [frames["differences"](image, pixels)["different_pixels"] for _,pixels,_ in images[1:]]
    require(max(changes) <= max(512, 3*same), f"first image changed on {case}: {changes}")
    require(result["regional"]["bytes"] < min(result["full"]["bytes"], result["partitioned"]["bytes"]),
            "bootstrap did not improve on both controls")
    result.update(different_pixels=changes, repeat_different_pixels=same, pixels=len(image)//4,
                  draw_owners=ref["draw_owners"], adapter=adapter)
    return result


def object_tree(binary, out):
    sound = runpy.run_path(str(HERE/"check-paged-impact.py"))
    folder = out/"object-tree"
    folder.mkdir(exist_ok=True)
    source, take, original, _ = sound["fixture"](folder)
    pages = folder/"regional"
    pin = cook(source,pages,128,visual_regional_pin=True,physics_storage="json")
    header, _ = metadata(pages,pin)
    require(header.get("object_profile") and all(not r.get("complete_pages") for r in header["support_regions"]),
            "expected a source object tree and sparse JSON certificate")
    for plan in ("contact","forecast"):
        flags = ["--acoustic-ray","7,1,10","10,4,15"]
        reference, pcm = sound["paged"](binary,folder,plan+"-original",original,take,*flags,plan=plan,all_resident=False)
        actual, samples = sound["paged"](binary,folder,plan+"-pinned",pages,take,*flags,"--index-pin",pin,
                                        plan=plan,all_resident=False)
        for key in ("visual_events","published","acoustic_probes","first_contact"):
            require(reference[key] == actual[key], "object-tree control changed "+key)
        require(pcm == samples, "sound rendering changed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=ROOT/"bench/out/ville-paris/visual-regions")
    parser.add_argument("--bin", type=Path, default=ROOT/"target/release/ville-paris")
    args = parser.parse_args()
    out, binary = args.out.resolve(), args.bin.resolve()
    out.mkdir(exist_ok=True, parents=True)
    pages = out/"pages"
    source = HERE/"fixtures/bir-hakeim.glb"
    pins = {name:cook(source, pages,128,**opts) for name,opts in (
        ("full",dict(pin_index=True)), ("partitioned",dict(pin_index=True,support_region_tiles=4)),
        ("regional",dict(visual_regional_pin=True)))}
    (out/"pins.json").write_text(json.dumps(pins, indent=2)+"\n")
    header, column = metadata(pages, pins["regional"])
    corruptions(binary, out, pages, header, column)
    routes = {}
    for case in ("road","rail","roof"):
        trace = traces(binary,pages,out,case,pins)
        routes[case] = dict(trajectory=trace, first_frame=first_frame(binary,out,pages,case,pins,header),
                            detail_frame=first_detail(binary,out,pages,case,trace["first_fine_tick"],pins))
    object_tree(binary, out)
    report = dict(scene=header["source_sha256"], pins=pins, routes=routes)
    (out/"summary.json").write_text(json.dumps(report, indent=2)+"\n")
    for case, row in routes.items():
        print(case, {k:row["first_frame"][k]["bytes"] for k in pins}, row["trajectory"])


if __name__ == "__main__":
    main()
