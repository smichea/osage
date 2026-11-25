#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Compare complete regional collision payloads with separate detail reads and resident contacts."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import runpy
import struct
import tempfile

from paging_checks import require
from paging_physics_directory import envelope
from paging_support_storage import pack_catalogue, unpack_support

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
previous = runpy.run_path(str(HERE / "check-regional-addresses.py"))
cook, replay, origin, flags, parts, read, checks = (
    previous[key] for key in ("cook", "replay", "origin", "flags", "pinned_parts", "read", "checks")
)


def verify_regions(pages, index, header, names):
    for region, name in zip(header["support_regions"], names, strict=True):
        require(region.get("complete_pages"), "Bir-Hakeim fixture expected a compact full region")
        blob = (pages / name).read_bytes()
        require(len(blob) == region["bytes"] and hashlib.sha256(blob).hexdigest() == Path(name).stem,
                "invalid regional object")
        data = unpack_support(blob)
        binding = hashlib.sha256()
        cursor = 36
        for i in region["tiles"]:
            row = index["tiles"][i]
            layer = row["physics"]
            body = (pages / layer["file"]).read_bytes()
            require(len(body) == layer["bytes"] and hashlib.sha256(body).hexdigest() == layer["sha256"]
                    and data[cursor:cursor + len(body)] == body, "collision tile changed in transit")
            cursor += len(body)
            binding.update(struct.pack("<ii", row["x"], row["z"]) + layer["sha256"].encode())
        require(data[:36] == b"ODR2" + binding.digest() and cursor == len(data) == region["decoded_bytes"],
                "region binding or trailing bytes")


def comparable(value):
    return {k: v for k, v in value.items() if k != "geometry"}


def witnesses(actual, control):
    for probe, reference in zip(actual["collision_probes"], control["collision_probes"], strict=True):
        require((probe["from"], probe["to"]) == (reference["from"], reference["to"]), "ray changed")
        hits = [comparable(hit) for hit in probe["intersections"]]
        full = [comparable(hit) for hit in reference["intersections"]]
        require(all(hit in full for hit in hits), "ray reported a nonresident witness")
        if probe["coverage"]["complete"]:
            require(hits == full, "complete ray missed source geometry")


def route(binary, out, pages, sparse, case, extra, pin, old):
    opts = [*flags, *extra]
    resident = replay(binary, pages, out / (case+"-resident.json"), case, [*opts, "--all-resident", "--index-pin", old])
    with origin(pages) as (url, baseline):
        control = replay(binary, url, out / (case+"-separate.json"), case, [*opts, "--index-pin", old])
    with origin(sparse) as (url, records), tempfile.TemporaryDirectory(dir=out) as cache:
        cold, warm = [replay(binary, url, out / f"{case}-{label}.json", case,
                             [*opts, "--index-pin", pin, "--content-cache", cache]) for label in ("cold", "warm")]
    for run in (cold, warm, control):
        for key in ("source_sha256", "body_before_step", "final_foot_m"):
            require(run[key] == resident[key], f"{case} differs from resident in {key}")
        require(comparable(run["first_contact"]) == comparable(resident["first_contact"]), "first contact changed")
        witnesses(run, resident)
    require(cold["collision_probes"] == warm["collision_probes"], "warm ray results changed")
    require(cold["physics"]["bundled_installs"] and cold["physics"]["reads"] == 0, "duplicate detail read")
    size = sum(r["bytes"] for r in records)
    old_size = sum(r["bytes"] for r in baseline)
    io = cold["transport"]["io"]
    require(len({r["name"] for r in records}) == len(records) == io["content_requests"] and
            io["content_body_bytes"] == size and all(r["status"] == 200 for r in records), "unexpected cold traffic")
    require(not warm["transport"]["io"]["content_requests"] and size < old_size and len(records) < len(baseline),
            "expected fewer bytes and round trips")
    (out / (case+"-origin.json")).write_text(json.dumps(records, indent=2)+"\n")
    return dict(bytes=size, requests=len(records), separate_bytes=old_size, separate_requests=len(baseline),
                samples=len(cold["body_before_step"]), contact_tick=cold["first_contact"]["tick"],
                bundled_installs=cold["physics"]["bundled_installs"], warm_requests=0)


def replace_region(pages, header, tail, pos, descriptor, name):
    header = copy.deepcopy(header)
    header["support_regions"][pos] = descriptor
    offset = len(tail) - 32 * (len(header["support_regions"]) - pos)
    tail = tail[:offset] + bytes.fromhex(Path(name).stem) + tail[offset+32:]
    body = json.dumps(header, separators=(",", ":")).encode()
    return checks["pin_bytes"](pages, envelope(b"ODP3", struct.pack("<I", len(body)) + body + tail))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, default=ROOT/"target/release/ville-paris")
    parser.add_argument("--out", type=Path, default=ROOT/"bench/out/ville-paris/bundled-regions")
    args = parser.parse_args()
    out, binary = args.out.resolve(), args.bin.resolve()
    out.mkdir(exist_ok=True, parents=True)
    pages = out/"pages"
    scene = HERE/"fixtures/bir-hakeim.glb"
    old = cook(scene, pages, 128, regional_pin=True)
    pin = cook(scene, pages, 128, bundled_pin=True)
    header, tail, names = parts(pages, pin)
    index = read(pages/"index.json")
    verify_regions(pages, index, header, names)
    doc = read(HERE/"fixtures/descent.json")
    track = next(r for r in doc["routes"] if r["id"] == "roof")
    track.update(id="traverse", ticks=480, start_m=[-700,95,-70], velocity_m_s=[140,0,0],
                 expected_support="roof--1-0--1", camera=[track["camera"][0], dict(track["camera"][-1], tick=480)])
    doc["routes"] = [track]
    path = out/"traverse.json"
    path.write_text(json.dumps(doc))
    with tempfile.TemporaryDirectory(dir=out) as temp:
        sparse = Path(temp)
        for name in [pin.split(":")[0]+".district", *names]:
            (sparse/name).symlink_to(pages/name)
        runs = {case: route(binary, out, pages, sparse, case, ["--replay", path] if case == "traverse" else [],
                            pin, old) for case in ("road", "rail", "roof", "traverse")}
        regions = [dict(r, file=n) for r, n in zip(header["support_regions"], names, strict=True)]
        delayed = checks["late_landing"](binary, sparse, pin, dict(tiles=header["tiles"], support_regions=regions), out)
        pos = next(i for t, i, state in read(out/"road-cold.json")["support"]["regions"]["events"]
                   if t == 0 and state == "installed")
        original = unpack_support((sparse/names[pos]).read_bytes())
        changed = bytearray(original)
        changed[4] ^= 1
        for raw in (changed, original + b"x", original[:-1]):
            packed, ext = pack_catalogue(raw, "auto")
            name = hashlib.sha256(packed).hexdigest()+"."+ext
            (sparse/name).write_bytes(packed)
            desc = dict(header["support_regions"][pos], file=ext, bytes=len(packed), decoded_bytes=len(raw))
            bad = replace_region(sparse, header, tail, pos, desc, name)
            checks["reject"](binary, sparse, bad, out, "support region")
    old_header, _, old_names = parts(pages, old)
    # A pin may serve a sparse certificate for one region and full geometry for a neighbour.
    mixed = replace_region(pages, header, tail, pos, old_header["support_regions"][pos], old_names[pos])
    hybrid = replay(binary, pages, out/"mixed.json", "road", [*flags, "--index-pin", mixed])
    require(hybrid["body_before_step"] == read(out/"road-cold.json")["body_before_step"] and
            hybrid["physics"]["bundled_installs"] and hybrid["physics"]["reads"], "mixed region handling")
    report = dict(scene=index["source_sha256"], pin=pin, groups=len(names), routes=runs, delayed=delayed)
    (out/"summary.json").write_text(json.dumps(report, indent=2)+"\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
