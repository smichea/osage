#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Exercise lazy collision addresses on independent HTTP origins and a whole-district control."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import runpy
import struct
import tempfile

from paging_checks import require
from paging_support_storage import unpack_support, pack_catalogue
from paging_physics_directory import envelope

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
checks = runpy.run_path(str(HERE / "check-support-regions.py"))
cook, replay, origin, flags = (checks[k] for k in ("cook", "replay", "origin", "FLAGS"))


def read(path):
    return json.loads(path.read_bytes())


def pinned_parts(pages, pin):
    wire = (pages / (pin.split(":")[0] + ".district")).read_bytes()
    require(wire[:4] == b"ODP3", "expected regional addresses")
    raw = unpack_support(wire[8:])
    n, = struct.unpack_from("<I", raw)
    header, tail = json.loads(raw[4:4+n]), raw[4+n:]
    count = sum(row[2] is not None for row in header["tiles"])
    hashes = tail[24*count:]
    groups = header["support_regions"]
    require(len(hashes) == 32*(1+len(groups)), "individual page digest in the root")
    return header, tail, [hashes[32*(i+1):32*(i+2)].hex()+"."+r["file"] for i,r in enumerate(groups)]


def verify_files(pages, index, header, groups):
    full = b"".join(bytes.fromhex(t["physics"]["sha256"]) for t in index["tiles"] if t.get("physics"))
    table = header["physical_addresses"]
    require(table["bytes"] == len(full) and (pages/table["file"]).read_bytes() == full and
            hashlib.sha256(full).hexdigest() == table["sha256"], "whole-district addresses differ")
    for row, original, name in zip(header["support_regions"], index["support_regions"], groups, strict=True):
        blob = (pages/name).read_bytes()
        require(hashlib.sha256(blob).hexdigest() == Path(name).stem and len(blob) == row["bytes"],
                "regional address object differs from root")
        raw = unpack_support(blob)
        addresses = b"".join(bytes.fromhex(index["tiles"][i]["physics"]["sha256"]) for i in row["tiles"])
        require(row["tiles"] == original["tiles"] and len(raw)==row["decoded_bytes"] and
                raw == b"ODR1"+addresses+unpack_support((pages/original["file"]).read_bytes()),
                "regional addresses or certificate mismatch")


def trajectory(binary, sparse, pin, pages, previous, directory, case, args, out, allowed):
    controls = [replay(binary, pages, out/f"{case}-{label}.json", case, [*flags,*args,"--index-pin",previous,*more])
                for label,more in (("control",[]),("resident",["--all-resident"]))]
    with origin(pages) as (url, old_reads):
        baseline = replay(binary, url, out/f"{case}-previous-http.json", case, [*flags,*args,"--index-pin",previous])
    old_bytes = sum(r["bytes"] for r in old_reads)
    require(baseline["transport"]["io"]["content_body_bytes"] == old_bytes, "previous reader and origin disagree")
    with origin(sparse) as (url, records), tempfile.TemporaryDirectory(dir=out) as cache:
        options = [*flags,*args,"--index-pin",pin,"--content-cache",cache]
        cold = replay(binary, url, out/f"{case}-cold.json", case, options)
        n = len(records)
        warm = replay(binary, url, out/f"{case}-warm.json", case, options)
    require(len(records)==n and not warm["transport"]["io"]["content_requests"], "warm requests")
    for control in [*controls,baseline,warm]:
        for key in ("source_sha256","body_before_step","final_foot_m"):
            require(control[key] == cold[key], f"{case} changed {key}")
        for key in ("tick","source_node","source_triangle","point","normal","separation_m","support"):
            require(control["first_contact"][key]==cold["first_contact"][key], f"{case} changed first contact {key}")
    require(controls[0]["collision_probes"] == cold["collision_probes"], "different resident ray results")
    names = Counter(r["name"] for r in records)
    require(all(v==1 for v in names.values()), "duplicate cold requests")
    require(all(r["status"]==200 and r["name"] in allowed and r["sha256"]==Path(r["name"]).stem for r in records),
            "unexpected cold response")
    byte_count=sum(r["bytes"] for r in records)
    require(cold["transport"]["io"]["content_requests"]==n and
            cold["transport"]["io"]["content_body_bytes"]==byte_count, "origin and client disagree")
    cells={t["physics"]["file"]:(t["x"],t["z"]) for t in directory["tiles"] if t.get("physics")}
    reference={tuple(e["tile"]) for e in controls[0]["physics_events"] if e["state"]=="started"}
    matched=sorted(cells[name] for name in names if name in cells)
    require(reference==set(matched), "different physical pages requested")
    (out/f"{case}-origin.json").write_text(json.dumps(records,indent=2)+"\n")
    return dict(bytes=byte_count, previous_bytes=old_bytes, previous_requests=len(old_reads), requests=n, warm_requests=0, samples=len(cold["body_before_step"]),
                detail_bytes=sum(r["bytes"] for r in records if r["name"] in cells), cells=matched,
                contact_tick=cold["first_contact"]["tick"], held_ticks=cold["support"]["stats"]["held_ticks"])


def object_audio(binary, out):
    sound = runpy.run_path(str(HERE / "check-paged-impact.py"))
    folder = out / "object-audio"
    folder.mkdir(exist_ok=True)
    source, take, original, _ = sound["fixture"](folder)
    pages = folder / "regional"
    pin = cook(source, pages, 128, regional_pin=True, physics_storage="json")
    header, _, _ = pinned_parts(pages, pin)
    require(header.get("object_profile"), "no acoustic object tree in regional directory")
    args = ["--no-visuals", "--acoustic-ray", "7,1,10", "10,4,15"]
    for mode in ("contact", "forecast"):
        before, reference = sound["paged"](binary, folder, mode+"-original", original, take, *args,
                                           all_resident=False, plan=mode)
        after, pcm = sound["paged"](binary, folder, mode+"-addresses", pages, take, *args, "--index-pin", pin,
                                    all_resident=False, plan=mode)
        require(reference == pcm and before["acoustic_probes"] == after["acoustic_probes"],
                "regional addressing changed object acoustics")


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin",type=Path,default=ROOT/"target/release/ville-paris")
    parser.add_argument("--out",type=Path,default=ROOT/"bench/out/ville-paris/regional-addresses")
    args=parser.parse_args()
    out,binary=args.out.resolve(),args.bin.resolve()
    out.mkdir(parents=True,exist_ok=True)
    pages=out/"pages"
    scene=HERE/"fixtures/bir-hakeim.glb"
    previous=cook(scene,pages,128,physics_pin=True,support_region_tiles=4)
    pin=cook(scene,pages,128,regional_pin=True)
    index=read(pages/"index.json")
    header,tail,groups=pinned_parts(pages,pin)
    verify_files(pages,index,header,groups)
    allowed={pin.split(":")[0]+".district", *groups, *(t["physics"]["file"] for t in index["tiles"] if t.get("physics"))}
    authored=read(HERE/"fixtures/descent.json")
    route=next(r for r in authored["routes"] if r["id"]=="roof")
    route.update(id="traverse", ticks=480, start_m=[-700,95,-70], velocity_m_s=[140,0,0],
                 expected_support="roof--1-0--1",camera=[route["camera"][0],dict(route["camera"][-1],tick=480)])
    authored["routes"]=[route]
    track=out/"traverse.json"
    track.write_text(json.dumps(authored))
    with tempfile.TemporaryDirectory(dir=out) as tmp:
        sparse=Path(tmp)
        for name in allowed:
            (sparse/name).symlink_to(pages/name)
        runs={}
        for case in ("road","rail","roof","traverse"):
            extra=["--replay",track] if case=="traverse" else []
            runs[case]=trajectory(binary,sparse,pin,pages,previous,index,case,extra,out,allowed)
        # Drop every detailed mesh before the first solver step.
        group_rows=[dict(r,file=name) for r,name in zip(header["support_regions"],groups)]
        for case in ("road","rail","roof"):
            runs[case]["first_step"]=checks["first_step_files"](binary,sparse,pin,group_rows,case,read(out/f"{case}-control.json"),out)
        # Delay the landing region itself; this also withholds the detailed page's address.
        landing = checks["late_landing"](binary,sparse,pin,dict(tiles=header["tiles"],support_regions=group_rows),out)
        spawn=next(i for tick,i,state in read(out/"road-cold.json")["support"]["regions"]["events"]
                   if tick==0 and state=="installed")
        # An unavailable address for the island can be retried while the capsule lands on the road.
        island = next(i for tick,i,state in read(out/"road-control.json")["support"]["regions"]["events"]
                      if tick>0 and state=="installed")
        with origin(sparse, fail_once=groups[island]) as (url, reads):
            retry = replay(binary,url,out/"road-retry.json","road",[*flags,"--index-pin",pin])
        reference = read(out/"road-cold.json")
        require(retry["body_before_step"] == reference["body_before_step"] and retry["first_contact"]==reference["first_contact"],
                "temporary address failure changed road physics")
        require([r["status"] for r in reads if r["name"]==groups[island]] == [503,200],
                "address was not recovered with one retry")
        data=bytearray(unpack_support((sparse/groups[spawn]).read_bytes()))
        data[4]^=1
        packed,suffix=pack_catalogue(data,"auto")
        digest=hashlib.sha256(packed).hexdigest()
        (sparse/(digest+"."+suffix)).write_bytes(packed)
        altered=header["support_regions"][spawn]
        altered.update(file=suffix,bytes=len(packed))
        payload=json.dumps(header,separators=(",",":")).encode()
        start=len(tail)-32*(len(groups)-spawn)
        changed=tail[:start]+bytes.fromhex(digest)+tail[start+32:]
        bad=checks["pin_bytes"](sparse,envelope(b"ODP3",struct.pack("<I",len(payload))+payload+changed))
        checks["reject"](binary,sparse,bad,out,"different collision directory")
    for mode,extra in (("resident",["--all-resident"]),("human",["--actor","human"]),("unprotected",["--late-collision","unprotected"])):
        results=[replay(binary,pages,out/f"{mode}-{i}.json","road",[*flags,*extra,"--index-pin",p])
                 for i,p in enumerate((previous,pin))]
        checks["same"](*results)
    full_reads = {}
    for label, selected in (("previous",previous),("regional",pin)):
        with origin(pages) as (url, records):
            data = replay(binary,url,out/f"{label}-resident-http.json","road",[*flags,"--index-pin",selected,"--all-resident","--tick","0"])
        full_reads[label] = dict(bytes=sum(r["bytes"] for r in records),requests=len(records))
        require(full_reads[label]["bytes"] == data["transport"]["io"]["content_body_bytes"],"whole-district traffic differs")
        require(sum(r["name"]==header["physical_addresses"]["file"] for r in records)==int(label=="regional"),
                "unexpected global address read")
    object_audio(binary, out)
    report=dict(scene=index["source_sha256"],pin=pin,previous=previous,routes=runs,delayed_landing=landing,all_resident=full_reads)
    (out/"summary.json").write_text(json.dumps(report,indent=2)+"\n")
    print(json.dumps(report))


if __name__=="__main__":
    main()
