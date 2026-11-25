#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Verify regional support against full-catalogue falls, independent HTTP reads and late geometry."""
import argparse
from collections import Counter
import copy
import hashlib
import json
from pathlib import Path
import runpy
import struct
import subprocess
import tempfile

from paging_checks import require
from paging_physics_directory import columns, envelope, physics_directory
from paging_support_storage import unpack_support

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
base = runpy.run_path(str(HERE / "check-headless-collision.py"))
cook, replay, origin = (base[k] for k in ("cook", "replay", "origin"))
FLAGS = ["--no-visuals", "--trace-body", "--delay-ms", "1400", *base["RAYS"]]


def same(control, regional):
    for key in ("source_sha256", "body_before_step", "physics", "physics_events", "physics_forecasts",
                "first_contact", "at_contact", "final_foot_m", "collision_probes", "actor"):
        require(control.get(key) == regional.get(key), f"regional reader changed {key}")
    for key in ("events", "first_full_contact", "duplicate_layer_contact_ticks", "rejected_full_pages"):
        require(control["support"][key] == regional["support"][key], f"regional support changed {key}")
    for key in ("held_ticks", "installations", "full_handoffs", "retained_certified_colliders",
                "replaced_certified_colliders", "peak_active_triangles"):
        require(control["support"]["stats"][key] == regional["support"]["stats"][key], f"regional solver changed {key}")


def pin_bytes(pages, data):
    sha = hashlib.sha256(data).hexdigest()
    (pages / f"{sha}.district").write_bytes(data)
    return f"{sha}:{len(data)}"


def bad_pin(pages, doc):
    return pin_bytes(pages, envelope(b"ODP1", json.dumps(doc, separators=(",", ":")).encode()))


def check_columns(binary, pages, doc, name, work):
    wire = (pages / name).read_bytes()
    require(wire[:4] == b"ODP2", "expected the smaller column directory for Bir-Hakeim")
    raw = unpack_support(wire[8:])
    require(len(raw) == struct.unpack_from("<I", wire, 4)[0], "incorrect column directory size")
    size, = struct.unpack_from("<I", raw)
    header = json.loads(raw[4:4+size])
    fields = raw[4+size:]
    layers = [l for x,z,l in header["tiles"] if l is not None]
    count = len(layers)
    for i, layer in enumerate(layers):
        require(layer["bounds"] == [[0,0,0],[0,0,0]], "unexpected inline bounds")
        coords = [struct.unpack_from("<f", fields, 4*(axis*count+i))[0] for axis in range(6)]
        layer["bounds"] = [coords[:3], coords[3:]]
    files = [header["support"], *header.get("support_regions", []), *layers]
    hashes = fields[24*count:]
    require(len(hashes) == 32*len(files), "incorrect number of addresses")
    for i, entry in enumerate(files):
        require(entry["sha256"] == "", "duplicate digest")
        entry["sha256"] = hashes[32*i:32*(i+1)].hex()
        entry["file"] = entry["sha256"] + "." + entry["file"]
    require(header == doc, "directory differs from the ordinary index")
    for case, message in ((raw[:-1], "digest column length"), (raw+b"x", "digest column length"),
                          (b"\xff"*4+raw[4:], "short column header"),
                          (raw[:4+size] + struct.pack("<f",float("nan")) + fields[4:], "invalid page geometry")):
        reject(binary, pages, pin_bytes(pages,envelope(b"ODP2", case)), work, message)
    wrong_hash = bytearray(raw)
    wrong_hash[-32:] = bytes(32)
    # Force every full page through the directory-to-catalogue binding, including the last row.
    reject(binary, pages, pin_bytes(pages, envelope(b"ODP2", bytes(wrong_hash))), work,
           "different collision directory", ("--no-visuals", "--all-resident"))
    alternate = json.loads(raw[4:4+size])
    alternate["support"]["file"] = "../file"
    encoded = json.dumps(alternate, separators=(",", ":")).encode()
    reject(binary, pages, pin_bytes(pages,envelope(b"ODP2", struct.pack("<I",len(encoded))+encoded+fields)),
           work, "invalid column file descriptor")
    return bad_pin(pages, doc)


def reject(binary, pages, pin, out, message, flags=("--no-visuals",)):
    with tempfile.TemporaryDirectory(dir=out) as work:
        receipt = Path(work) / "result.json"
        result = subprocess.run([binary, "--paging", "--pages", pages, "--index-pin", pin, "--route", "road",
                                 "--tick", "0", "--out", receipt, *flags], capture_output=True, text=True)
        require(result.returncode and message in result.stderr and not receipt.exists(), result.stderr)


def first_step_files(binary, pages, pin, regions, route, reference, out):
    root = pin.split(":")[0] + ".district"
    installed = [regions[group]["file"] for tick, group, state in reference["support"]["regions"]["events"]
                 if tick <= 1 and state == "installed"]
    with tempfile.TemporaryDirectory(dir=out) as tmp:
        sparse = Path(tmp)
        for name in [root, *installed]:
            (sparse / name).symlink_to(pages / name)
        with origin(sparse) as (url, reads):
            actual = replay(binary, url, out / f"{route}-first-step.json", route, [*FLAGS, "--index-pin", pin, "--tick", "1"])
        require(actual["body_before_step"] == reference["body_before_step"][:2] and actual["body_before_step"][1]["step"],
                "reduced bootstrap changed the first solver step")
        require(Counter(r["name"] for r in reads if r["status"] == 200) == Counter([root, *installed]),
                "unexpected successful first-step response")
        if route == "road":
            require(len(installed) == 1, "road control needs one spawn region")
            (sparse / installed[0]).unlink()
            reject(binary, sparse, pin, out, "support region")
    return dict(files=1 + len(installed), body_bytes=sum(r["bytes"] for r in reads))


def delayed_arrival(binary, pages, pin, regions, out):
    ordinary = json.loads((out / "road-cold.json").read_bytes())
    events = ordinary["support"]["regions"]["events"]
    selected = {i:t for t,i,state in events if state == "prefetched"}
    tick, group = next((t,i) for t,i,state in events if state=="installed" and t>0 and i in selected)
    name = regions[group]["file"]
    with origin(pages, delay={name:1.6}) as (url, reads):
        actual = replay(binary, url, out / "road-delayed.json", "road", [*FLAGS, "--index-pin",pin])
        for key in ("source_sha256", "body_before_step", "first_contact", "final_foot_m", "collision_probes"):
            require(ordinary[key] == actual[key], f"late off-path region changed {key}")
        require(actual["support"]["stats"]["held_ticks"] == 0, "off-path region stopped the actor")
        require(actual["physics"]["reads"] == ordinary["physics"]["reads"], "staged physics read twice")
        require(actual["transport"]["tick_work_us"]["max"] < 100_000, "off-path HTTP response blocked a tick")
        require(any(e["state"] == "waiting_for_support" for e in actual["physics_events"]),
                "detail page did not arrive before its support")
        ready = next(t for t,i,state in actual["support"]["regions"]["events"] if i == group and state == "installed")
        require(ready > tick, "configured response did not miss its ordinary deadline")
        deliveries = [r for r in reads if r["name"] == name]
        require(len(deliveries)==1 and deliveries[0]["end_ms"]-deliveries[0]["start_ms"] >= 1590,
                "regional response did not experience the configured delay")
    return dict(start_tick=selected[group], ordinary_install_tick=tick, delayed_install_tick=ready, arrival_delay_ms=1600,
                worst_tick_us=actual["transport"]["tick_work_us"]["max"],
                normal_worst_tick_us=ordinary["transport"]["tick_work_us"]["max"])


def late_landing(binary, pages, pin, doc, out):
    replay_doc = json.loads((HERE / "fixtures/descent.json").read_bytes())
    route = next(r for r in replay_doc["routes"] if r["id"] == "rail")
    route.update(start_m=[0, 55, 0], velocity_m_s=[0, -55, 0], ticks=180,
                 camera=[route["camera"][0], dict(route["camera"][-1], tick=180)])
    replay_doc["routes"] = [route]
    path = out / "fast-landing.json"
    path.write_text(json.dumps(replay_doc))
    flags = [*FLAGS, "--replay", path, "--index-pin", pin]
    reference = replay(binary, pages, out / "fast-control.json", "rail", flags)
    contact = reference["first_contact"]
    # Select the actual landing tile from the contact and its district directory.
    tile = next(i for i,t in enumerate(doc["tiles"]) if t[:2] == contact["tile"])
    file = next(r["file"] for r in doc["support_regions"] if tile in r["tiles"])
    with origin(pages, delay={file:1.6}) as (url, reads):
        actual = replay(binary, url, out / "fast-delayed.json", "rail", flags)
    require(sum(r["name"] == file and r["end_ms"] - r["start_ms"] >= 1590 for r in reads) == 1,
            "missing delayed landing response")
    held = 0
    samples = actual["body_before_step"]
    for sample in samples[1:]:
        control = reference["body_before_step"][sample["tick"] - held]
        for key in ("center_m", "foot_m", "velocity_m_s", "supported"):
            require(sample[key] == control[key], f"late landing changed {key} at tick {sample['tick']}")
        held += not sample["step"]
    require(held > 0 and held == actual["support"]["stats"]["held_ticks"], "unsafe or unnecessary landing steps")
    for key in ("support", "source_node", "source_triangle", "normal", "point", "separation_m",
                "vertical_speed_before_m_s", "vertical_speed_after_m_s"):
        require(contact[key] == actual["first_contact"][key], f"late landing changed contact {key}")
    require(contact["tick"] + held == actual["first_contact"]["tick"], "landing lost or gained a solver step")
    worst = actual["transport"]["tick_work_us"]["max"]
    require(worst < 100_000, "threatened landing blocked the simulation thread")
    with origin(pages, delay={file:0.9}, corrupt_once=file) as (url, _):
        with tempfile.TemporaryDirectory(dir=out) as work:
            receipt = Path(work) / "result.json"
            bad = subprocess.run([binary, "--paging", "--pages", url, "--route", "rail", "--out", receipt, *flags],
                                 capture_output=True, text=True)
            require(bad.returncode and "support region" in bad.stderr and not receipt.exists(),
                    "corrupt landing region allowed the replay to finish")
    return dict(held_ticks=held, contact_tick=actual["first_contact"]["tick"], moving_steps=contact["tick"],
                worst_tick_us=worst, delay_ms=1600)


def fragmented_and_audio(binary, work, out):
    diagonal = runpy.run_path(str(HERE / "check-residual-triangle-paths.py"))
    ramp = work / "ramp"
    ramp.mkdir()
    control, route, distances = diagonal["setup"](ramp)
    pages = ramp / "regional"
    pin = cook(ramp / "interleaved.glb", pages, 32, support_region_tiles=1, physics_pin=True)
    flags = ["--no-visuals", "--trace-body", "--replay", route, "--watch", "-2,0", "--hold", "physics",
             "--hold-ticks", "55", "--delay-ms", "0", "--collision-ray", "-54,5,14", "-54,1,14"]
    for i in range(len(distances)):
        take = f"probe-{i}"
        original = replay(binary, control, ramp / "control.json", take, flags)
        regional = replay(binary, pages, ramp / "regional.json", take, [*flags, "--index-pin", pin])
        same(original, regional)
    sound = runpy.run_path(str(HERE / "check-paged-impact.py"))
    audio = out / "audio"
    audio.mkdir(exist_ok=True)
    source, take, control, _ = sound["fixture"](audio)
    pages = audio / "regional"
    pin = cook(source, pages, 128, support_region_tiles=1, physics_pin=True)
    flags = ["--no-visuals", "--acoustic-ray", "7,1,10", "10,4,15"]
    # This small acoustic scene selects JSON. Exercise its native object tree in
    # the column layout too, including independently addressed legacy JSON meshes.
    json_pages = audio / "json-meshes"
    cook(source, json_pages, 128, physics_storage="json", support_region_tiles=1, physics_pin=True)
    json_doc = physics_directory(json.loads((json_pages / "index.json").read_bytes()))
    require(json_doc.get("object_profile"), "missing acoustic object profile")
    packed = columns(json_doc, lambda v: json.dumps(v, separators=(",", ":")).encode())
    column_pin = pin_bytes(json_pages, envelope(b"ODP2", packed))
    for mode in ("contact", "forecast"):
        original, pcm = sound["paged"](binary, audio, mode + "-control", control, take, *flags, all_resident=False, plan=mode)
        for name, folder, selected_pin in (("regional", pages, pin), ("columns", json_pages, column_pin)):
            regional, actual = sound["paged"](binary, audio, mode + "-" + name, folder, take, *flags,
                                              "--index-pin", selected_pin, all_resident=False, plan=mode)
            require(actual == pcm and original["acoustic_probes"] == regional["acoustic_probes"], "regional audio mismatch")
    return {"fragmented_falls":len(distances), "audio_modes":["contact","forecast"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    parser.add_argument("--out", type=Path, default=ROOT / "bench/out/ville-paris/native-regions")
    args = parser.parse_args()
    binary, out = args.bin.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=out) as tmp:
        work = Path(tmp)
        source = HERE / "fixtures/bir-hakeim.glb"
        ordinary, pages = work / "control", work / "regional"
        control_pin = cook(source, ordinary, 128, pin_index=True)
        pin = cook(source, pages, 128, support_region_tiles=2, physics_pin=True)
        name = pin.split(":")[0] + ".district"
        doc = physics_directory(json.loads((pages / "index.json").read_bytes()))
        old_pin = check_columns(binary, pages, doc, name, work)
        index = json.loads((ordinary / "index.json").read_bytes())
        regions = doc["support_regions"]
        (pages / "index.json").unlink()
        (pages / doc["support"]["file"]).unlink()
        files = {t["physics"]["file"] for t in index["tiles"] if t.get("physics")}
        allowed = files | {g["file"] for g in regions} | {name}
        results = {}
        for route in ("road","rail","roof"):
            control = replay(binary, ordinary, out / f"{route}-control.json", route,
                             [*FLAGS, "--index-pin", control_pin])
            legacy = replay(binary, pages, out / f"{route}-previous-pin.json", route, [*FLAGS, "--index-pin", old_pin])
            same(control, legacy)
            with tempfile.TemporaryDirectory(dir=work) as cache, origin(pages) as (url, records):
                flags = [*FLAGS, "--index-pin", pin, "--content-cache", cache]
                cold = replay(binary, url, out / f"{route}-cold.json", route, flags)
                same(control,cold)
                n = len(records)
                bodies = sum(r["bytes"] for r in records)
                io = cold["transport"]["io"]
                require(all(r["status"] == 200 and r["name"] in allowed and
                            Path(r["name"]).stem == r["sha256"] for r in records), "unexpected HTTP response")
                require(len(Counter(r["name"] for r in records)) == n and
                        io["content_requests"] == n and io["content_body_bytes"] == bodies, "HTTP receipts disagree")
                require(cold["support"]["regions"]["loaded"] < len(regions), "local fall read all regions")
                warm = replay(binary, url, out / f"{route}-warm.json", route, flags)
                same(control, warm)
                require(len(records) == n and warm["transport"]["io"]["content_requests"] == 0, "unexpected warm read")
                (out / f"{route}-origin.json").write_text(json.dumps(records, indent=2)+"\n")
                regional_bytes = sum(r["bytes"] for r in records if r["name"] in {g["file"] for g in regions})
                first_step = first_step_files(binary, pages, pin, regions, route, cold, out)
                results[route] = dict(first_step=first_step, total_bytes=bodies, requests=n, warm_requests=0, region_bytes=regional_bytes,
                                      page_bytes=sum(r["bytes"] for r in records if r["name"] in files),
                                      installed=cold["support"]["regions"], samples=len(cold["body_before_step"]),
                                      contact=cold["first_contact"])
        reject(binary, pages, pin, work, "requires --no-visuals", ())
        for case in ("missing","duplicate","unordered"):
            bad = copy.deepcopy(doc)
            rows = bad["support_regions"]
            if case == "missing":
                rows.pop()
                message = "do not cover"
            elif case == "duplicate":
                rows.append(rows[0])
                message = "duplicate support region"
            else:
                group = next(r for r in rows if len(r["tiles"]) > 1)
                group["tiles"].reverse()
                message = "unordered support region"
            reject(binary, pages, bad_pin(pages,bad), work, message)
        installed = next(i for t,i,state in results["road"]["installed"]["events"] if t == 0 and state == "installed")
        region = regions[installed]
        with origin(pages, corrupt_once=region["file"]) as (url, _):
            reject(binary, url, pin, work, "support region")
        bad = copy.deepcopy(doc)
        alternate = next(g for g in regions if g["file"] != region["file"])
        bad["support_regions"][installed] = dict(alternate, tiles=region["tiles"])
        reject(binary, pages, bad_pin(pages,bad), work, "different collision directory")
        # The two directory profiles must also describe the same fully resident geometry.
        # This operation deliberately reads the complete catalogue through its independent address.
        (pages / doc["support"]["file"]).symlink_to(ordinary / index["support"]["file"])
        global_flags = [*FLAGS, "--all-resident", "--tick", "0"]
        original = replay(binary,ordinary,out / "resident-control.json","road",global_flags)
        resident = replay(binary,pages,out / "resident-physics.json","road",[*global_flags, "--index-pin", pin])
        same(original,resident)
        ordinary_rig = replay(binary,ordinary,out / "rig-control.json","road",[*FLAGS,"--actor","human"])
        regional_rig = replay(binary,pages,out / "rig-regional.json","road",[*FLAGS,"--actor","human","--index-pin",pin])
        same(ordinary_rig,regional_rig)
        delay = delayed_arrival(binary,pages,pin,regions,out)
        fast = late_landing(binary, pages, pin, doc, out)
        coverage = fragmented_and_audio(binary,work,out)
        summary = dict(delayed_region=delay,late_landing=fast,coverage=coverage,scene_sha256=index["source_sha256"], pin=pin, region_count=len(regions),
                       index_bytes=int(pin.split(":")[1]), previous_pin=old_pin, routes=results)
        (out / "summary.json").write_text(json.dumps(summary,indent=2)+"\n")
        print(json.dumps(summary))


if __name__ == "__main__":
    main()
