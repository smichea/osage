#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Qualify the bundled visual launch against the same fully rendered district and its regional pin."""
import argparse
import fcntl
import hashlib
import json
from pathlib import Path
import runpy
import statistics
import struct
import subprocess
import time
import tempfile

from paging_checks import require
from paging_coarse import unpack_coarse
from paging_physics_directory import envelope
from paging_support_storage import unpack_support

HERE = Path(__file__).resolve().parent
controls = runpy.run_path(str(HERE/"check-visual-district.py"))
ROOT, base, frames, cook, replay, origin, read = (controls[k] for k in ("ROOT","base","frames","cook","replay","origin","read"))


def pack_parts(pages, pin):
    name, size = pin.split(":")
    wire = (pages/(name+".district")).read_bytes()
    require(wire[:4] == b"ODI3" and len(wire)==int(size) and hashlib.sha256(wire).hexdigest()==name,"package identity")
    raw = unpack_support(wire[8:])
    magic, length, region = struct.unpack_from("<4sII",raw)
    require(len(raw)==struct.unpack_from("<I",wire,4)[0] and magic in (b"ODI1",b"ODI2"),"package layout")
    info = raw[12:12+length]
    if magic == b"ODI2":
        n, = struct.unpack_from("<I",info)
        header = json.loads(info[4:4+n])
        require(len(info[4+n:]) == 32*len(header["tiles"]), "visual address count")
    else:
        header = json.loads(info)
    cursor = 12+length
    def consume(desc, inline_columns=False):
        nonlocal cursor
        data = raw[cursor:cursor+desc["bytes"]]
        cursor += len(data)
        require(len(data)==desc["bytes"] and hashlib.sha256(data).hexdigest()==desc["sha256"], "bad embedded member")
        if not (inline_columns and data.startswith(b"OVCU")):
            require((pages/desc["file"]).read_bytes()==data, "embedded/external content differs")
        return data
    parent = consume(header["parent"], inline_columns=True)
    if header.get("models"):
        consume(header["models"])
    if region != 0xffffffff:
        consume(header["support_regions"][region])
    elif not header.get("support_regions") and header.get("support"):
        consume(header["support"])
    require(cursor==len(raw),"trailing first-frame content")
    return header, raw, parent, region


def verify_inputs(pages, pins):
    control, hashes = controls["metadata"](pages,pins["split"])
    reference = unpack_coarse((pages/control["parent"]["file"]).read_bytes())
    for key in ("sky","road"):
        h, raw, parent, region = pack_parts(pages,pins[key])
        require(dict(h,parent=control["parent"])==control and unpack_coarse(parent)==reference, "changed launch scene")
        size=struct.unpack_from("<I",raw,4)[0]
        require(raw[12+size-len(hashes):12+size]==hashes, "changed fine visual addresses")
        require((region==0xffffffff)==(key=="sky"), "unexpected spawn selection")
        if key=="road":
            require(any((h["tiles"][i]["x"],h["tiles"][i]["z"])==(0,0)
                        for i in h["support_regions"][region]["tiles"]), "wrong spawn")
    return hashlib.sha256(reference).hexdigest()


def malformed(binary, pages, pin, out):
    header, raw, _, _ = pack_parts(pages,pin)
    size, = struct.unpack_from("<I",raw,4)
    off = 12+size
    for bad, error in ((raw[:9],"first-frame header"), (raw[:off-1],"first-frame directory"),
                       (raw[:off+2],"first-frame parent"), (raw[:off]+bytes([raw[off]^1])+raw[off+1:],"bad tile content"),
                       (raw[:-1],"bad tile content"), (raw+b"x","bad tile content"),
                       (raw[:8]+struct.pack("<I",len(header["support_regions"]))+raw[12:],"unknown first-frame region")):
        key = base["pin_bytes"](pages,envelope(b"ODI3",bad))
        base["reject"](binary,pages,key,out,error,flags=())


def capture(binary, pages, out, case, pin, label):
    with origin(pages,mbps=5) as (url, rows), tempfile.TemporaryDirectory(dir=out) as cache:
        opts = ["--index-pin",pin,"--content-cache",cache]
        stem = out/f"{case}-{label}"
        frame = frames["capture"](binary,url,case,stem,opts)
        n = len(rows)
        warm = frames["capture"](binary,url,case,out/f"{case}-{label}-warm",opts)
        require(n==len(rows) and warm[0]==frame[0] and warm[2]==frame[2], "warm first frame changed or fetched data")
        data = read(stem.with_suffix(".json"))
        io = data["transport"]["io"]
        require(io["content_body_bytes"] == sum(r["bytes"] for r in rows) and
                io["content_requests"]==len(rows) and all(r["status"]==200 for r in rows), "origin and reader disagree")
        report=dict(bytes=io["content_body_bytes"], requests=n, warm_requests=0, names=[r["name"] for r in rows],
                    warm_pixel_changes=frames["differences"](frame[1],warm[1])["different_pixels"])
        (out/f"{case}-{label}-origin.json").write_text(json.dumps(rows,indent=2)+"\n")
    return frame,report


def presentation(binary, pages, out, case, pins):
    selected=pins["road" if case=="road" else "sky"]
    original, split=capture(binary,pages,out,case,pins["split"],"split")
    new, packed=capture(binary,pages,out,case,selected,"packed")
    require(original[0]==new[0] and original[2]==new[2] and packed["bytes"]<split["bytes"]
            and packed["requests"]<split["requests"], "GPU input or launch traffic regression")
    root=selected.split(":")[0]+".district"
    with tempfile.TemporaryDirectory(dir=out) as folder:
        sparse=Path(folder)
        (sparse/root).symlink_to(pages/root)
        missing=[]
        with origin(sparse,missing=missing) as (url, records):
            only=frames["capture"](binary,url,case,out/f"{case}-only",["--index-pin",selected])
        require([r["name"] for r in records if r["status"]==200]==[root] and only[0]==original[0],
                "other files needed to render")
        attempts=len(missing)
    changes=frames["differences"](new[1],original[1])["different_pixels"]
    require(max(changes,frames["differences"](new[1],only[1])["different_pixels"])
            <= max(512,3*max(packed["warm_pixel_changes"],split["warm_pixel_changes"])), "changed first image")
    require(read(out/f"{case}-packed.json")["bootstrap_visual_bytes"]==0,"parent was charged twice")
    return dict(split=split,packed=packed,minimum_bytes=int(selected.split(":")[1]),minimum_files=[root],
                unavailable_optional_requests=attempts,different_pixels=changes,pixels=len(new[1])//4)


def legacy_and_sound(binary, out):
    sound = runpy.run_path(str(HERE/"check-paged-impact.py"))
    folder=out/"object-tree"
    folder.mkdir(exist_ok=True)
    source, take, original, _ = sound["fixture"](folder)
    opts=["--acoustic-ray","7,1,10","10,4,15"]
    pages=folder/"demand"
    pin=cook(source,pages,128,physics_storage="json",first_frame_pin=True,first_frame_cell=(0,0))
    pack_parts(pages,pin)
    for plan in ("forecast","contact"):
        before, pcm=sound["paged"](binary,folder,plan+"-original",original,take,*opts,plan=plan,all_resident=False)
        actual, samples=sound["paged"](binary,folder,plan+"-packed",pages,take,*opts,"--index-pin",pin,
                                      plan=plan,all_resident=False)
        require(samples==pcm and actual["acoustic_probes"]==before["acoustic_probes"] and
                actual["first_contact"]==before["first_contact"],"source tree or impact changed")
    for storage in ("shared","embedded"):
        pages=folder/storage
        # Legacy scene encodings use named collision meshes. This source instead has an object profile.
        # Repeated visual placement in a different cell also exercises the old shared model library.
        doc, chunk = sound["EDGE"]["common"]["read_glb"](source)
        placed = next(n for n in doc["nodes"] if "mesh" in n)
        for x in (300,600):
            doc["scenes"][doc.get("scene",0)]["nodes"].append(len(doc["nodes"]))
            doc["nodes"].append(dict(mesh=placed["mesh"],translation=[x,0,0]))
        generated=folder/"old-placements.glb"
        sound["EDGE"]["common"]["write_glb"](generated,doc,chunk)
        pin=cook(generated,pages,128,storage=storage,first_frame_pin=True)
        header, _, _, _=pack_parts(pages,pin)
        require(bool(header.get("models"))==(storage=="shared") and not header.get("support_regions"),
                "expected legacy visual storage and global-only support")
        with tempfile.TemporaryDirectory(dir=out) as temp:
            sparse=Path(temp)
            name=pin.split(":")[0]+".district"
            (sparse/name).symlink_to(pages/name)
            launch=["--tick","0","--replay",HERE/"fixtures/lift.json"]
            ordinary=replay(binary,pages,folder/(storage+"-plain.json"),"standing-lift",launch)
            with origin(sparse) as (url,rows):
                launched=replay(binary,url,folder/(storage+"-only.json"),"standing-lift",[*launch,"--index-pin",pin])
            require(ordinary["published"]==launched["published"] and [r["name"] for r in rows if r["status"]==200]==[name],
                    "legacy first frame requires an external resource")


def revisit(binary, pages, out, pins):
    # Each launch is a new process. The URL and private disk cache stay the same for the sequence.
    run = {}
    for policy in ("split","one","per-spawn"):
        with origin(pages) as (url,rows), tempfile.TemporaryDirectory(dir=out) as cache:
            visits = []
            for seq,case in enumerate(("rail","road","roof","road")):
                name = "split" if policy=="split" else "road" if policy=="one" or case=="road" else "sky"
                mark=len(rows)
                label=out/f"revisit-{policy}-{seq}"
                frame,_,_=frames["capture"](binary,url,case,label,["--index-pin",pins[name],"--content-cache",cache])
                reference=read(out/f"{case}-packed.gpu.json")["frames"][0]
                require(all(reference[k]==v for k,v in frame.items()), "shared pin changed the destination camera")
                io=read(label.with_suffix(".json"))["transport"]["io"]
                entries=rows[mark:]
                require(all(r["status"]==200 for r in entries) and
                        io["content_body_bytes"]==sum(r["bytes"] for r in entries), "cross-route cache accounting")
                visits.append(dict(route=case,bytes=io["content_body_bytes"],requests=len(entries),
                                  launch_fetches=sum(r["name"].endswith(".district") for r in entries)))
            require(sum(v["launch_fetches"] for v in visits)==(2 if policy=="per-spawn" else 1)
                    and visits[-1]["bytes"]==0, "snapshot reuse across launches")
            run[policy]=visits
    return run


def revision(binary, pages, out, pins):
    # A changed authoring tag changes the source fingerprint while all renderable geometry stays identical.
    io=runpy.run_path(str(HERE/"check-impact-transmission.py"))
    scene=HERE/"fixtures/bir-hakeim.glb"
    doc, chunk=io["read_glb"](scene)
    doc["asset"]["generator"]="Ville-Paris snapshot revision qualification"
    updated=out/"revised.glb"
    io["write_glb"](updated,doc,chunk)
    old=read(pages/"index.json")
    with tempfile.TemporaryDirectory(dir=out) as temp:
        published=Path(temp)
        for f in pages.iterdir():
            if f.name!="index.json":
                (published/f.name).symlink_to(f)
        new={name:cook(updated,published,128,**opts) for name,opts in (
            ("split",dict(visual_regional_pin=True)),("sky",dict(first_frame_pin=True)))}
        index=read(published/"index.json")
        require(index["source_sha256"]!=old["source_sha256"] and dict(index,source_sha256=old["source_sha256"])==old,
                "revision changed something besides source identity")
        results={}
        for mode in new:
            with origin(published) as (url,rows), tempfile.TemporaryDirectory(dir=out) as cache:
                for label,pin in (("before",pins[mode]),("after",new[mode])):
                    mark=len(rows)
                    target=out/f"revision-{mode}-{label}.json"
                    replay(binary,url,target,"rail",["--tick","0","--index-pin",pin,"--content-cache",cache])
                    if label=="after":
                        entries=rows[mark:]
                        require([r["name"] for r in entries]==[pin.split(":")[0]+".district"],
                                "unexpected revision refetch")
                        results[mode]=entries[0]["bytes"]
    return results


def timed_launches(binary, pages, out, pins, pairs, mbps=5, delay_ms=40):
    if pairs < 1:
        raise ValueError("a timing series needs at least one pair")
    directory=out/f"latency-{mbps:g}M-{delay_ms:g}ms"
    directory.mkdir(exist_ok=True)
    header, _=controls["metadata"](pages,pins["split"])
    # All bootstrap resources share one shaped last mile.
    delay={r["file"]:delay_ms/1000 for r in [header["parent"], *header["support_regions"]]}
    delay.update({p.split(":")[0]+".district":delay_ms/1000 for p in pins.values()})
    result={}
    with open("/tmp/osage-ville-paris-gpu.lock","a") as lock, origin(pages,mbps=mbps,delay=delay) as (url,rows):
        fcntl.flock(lock, fcntl.LOCK_EX)
        for case in ("rail","road"):
            samples={"split":[],"packed":[]}
            for pair in range(pairs+1):
                order=("split","packed") if pair%2==0 else ("packed","split")
                for mode in order:
                    pin=pins["split" if mode=="split" else "sky" if case=="rail" else "road"]
                    stem=directory/f"{case}-{mode}-{pair}"
                    png=stem.with_suffix(".png")
                    png.unlink(missing_ok=True)
                    tick=stem.with_suffix(".json")
                    mark=len(rows)
                    with stem.with_suffix(".log").open("wb") as log:
                        start=time.monotonic()
                        with subprocess.Popen([binary,"--paging","--pages",url,"--route",case,"--index-pin",pin,
                                               "--tick","0","--gpu-trace",stem.with_suffix(".gpu.json"),
                                               "--shot",png,"--out",tick],stdout=log,stderr=subprocess.STDOUT) as proc:
                            while True:
                                if png.exists() and png.stat().st_size>=12:
                                    with png.open("rb") as image:
                                        image.seek(-12,2)
                                        if image.read()==bytes.fromhex("0000000049454e44ae426082"):
                                            end=time.monotonic()
                                            break
                                if proc.poll() is not None and not png.exists():
                                    raise RuntimeError(f"missing timed image: {stem}")
                                if time.monotonic()-start>60:
                                    proc.kill()
                                    raise RuntimeError("timed image deadline")
                                time.sleep(0.002)
                            require(proc.wait(timeout=20)==0,"timed native process failed")
                    required={pin.split(":")[0]+".district"}
                    if mode=="split":
                        required.add(header["parent"]["file"])
                        if case=="road":
                            required.add(next(r["file"] for r in header["support_regions"]
                                              if any((header["tiles"][i]["x"],header["tiles"][i]["z"])==(0,0)
                                                     for i in r["tiles"])))
                    critical=[r for r in rows[mark:] if r["name"] in required]
                    require({r["name"] for r in critical}==required,"timed bootstrap dependencies missing")
                    network_ms=max(r["end_ms"] for r in critical)-min(r["start_ms"] for r in critical)
                    size=sum(r["bytes"] for r in rows[mark:])
                    require(size==read(tick)["transport"]["io"]["content_body_bytes"], "timed origin disagreement")
                    if pair:
                        samples[mode].append(dict(ms=1000*(end-start),bytes=size,critical_response_interval_ms=network_ms))
            differences=[s["ms"]-p["ms"] for s,p in zip(samples["split"],samples["packed"],strict=True)]
            result[case]=dict(samples=samples,median_ms={k:statistics.median(s["ms"] for s in v)
                                                       for k,v in samples.items()},
                              median_paired_saving_ms=statistics.median(differences))
    record=dict(mbps=mbps,bootstrap_service_ms=delay_ms,pairs=pairs,endpoint="complete PNG IEND on disk",routes=result)
    (directory/"summary.json").write_text(json.dumps(record,indent=2)+"\n")
    return record


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--bin",type=Path,default=ROOT/"target/release/ville-paris")
    p.add_argument("--out",type=Path,default=ROOT/"bench/out/ville-paris/first-frame")
    p.add_argument("--timing-pairs",type=int,default=0,help="alternate cold launches through complete PNG publication")
    p.add_argument("--bootstrap-mbps",type=float,default=5)
    p.add_argument("--bootstrap-delay-ms",type=float,default=40)
    args=p.parse_args()
    out,binary=args.out.resolve(),args.bin.resolve()
    out.mkdir(parents=True,exist_ok=True)
    pages=out/"pages"
    scene=HERE/"fixtures/bir-hakeim.glb"
    pins={name:cook(scene,pages,128,**opts) for name,opts in (
        ("split",dict(visual_regional_pin=True)),("sky",dict(first_frame_pin=True)),
        ("road",dict(first_frame_pin=True,first_frame_cell=(0,0))))}
    (out/"pins.json").write_text(json.dumps(pins,indent=2)+"\n")
    glb=verify_inputs(pages,pins)
    malformed(binary,pages,pins["road"],out)
    routes={}
    for case in ("road","rail","roof"):
        pair=dict(full=pins["split"],regional=pins["road" if case=="road" else "sky"])
        trace=controls["traces"](binary,pages,out,case,pair)
        detail=controls["first_detail"](binary,out,pages,case,trace["first_fine_tick"],pair)
        routes[case]=dict(trajectory=trace,detail_frame=detail,first_frame=presentation(binary,pages,out,case,pins))
    legacy_and_sound(binary,out)
    visits=revisit(binary,pages,out,pins)
    revisions=revision(binary,pages,out,pins)
    report=dict(scene=read(pages/"index.json")["source_sha256"],pins=pins,parent_glb_sha256=glb,routes=routes,revisits=visits,revision_bytes=revisions)
    (out/"summary.json").write_text(json.dumps(report,indent=2)+"\n")
    if args.timing_pairs:
        timed_launches(binary,pages,out,pins,args.timing_pairs,args.bootstrap_mbps,args.bootstrap_delay_ms)
    for case,data in routes.items():
        f=data["first_frame"]
        print(case,f["split"]["bytes"],f["packed"]["bytes"],f["minimum_bytes"],data["trajectory"]["first_fine_tick"])


if __name__=="__main__":
    main()
