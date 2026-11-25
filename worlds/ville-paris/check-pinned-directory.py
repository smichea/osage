#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Verify immutable district snapshots across disk, HTTP, origin changes and cache reuse."""
import argparse
from collections import Counter
import hashlib
import json
import math
from pathlib import Path
import runpy
import statistics
import struct
import subprocess
import tempfile
import time
import zlib

from paging_checks import require
from paging_support_storage import unpack_support

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
checks = runpy.run_path(str(HERE / "check-headless-collision.py"))
replay, cook, origin = (checks[k] for k in ("replay", "cook", "origin"))
RAYS = checks["RAYS"]


def pinned_file(directory, payload):
    sha = hashlib.sha256(payload).hexdigest()
    (directory / (sha + ".district")).write_bytes(payload)
    return f"{sha}:{len(payload)}"


def reject(binary, directory, pin, receipt):
    result = subprocess.run([binary, "--paging", "--pages", directory, "--index-pin", pin, "--tick", "0",
                             "--route", "road", "--out", receipt], capture_output=True)
    require(result.returncode and not receipt.exists(), f"accepted invalid directory {pin}")


def same(a, b, keys):
    for key in keys:
        require(a[key] == b[key], f"directory selection changed {key}")


def timed_start(binary, pages, out, pin, mbps, flags, expected):
    pairs = []
    with origin(pages, mbps=mbps) as (url, rows):
        for turn in range(8):
            pair = {}
            for pinned in ((False, True) if turn % 2 == 0 else (True, False)):
                pos = len(rows)
                started = time.monotonic()
                replay(binary, url, out / "timed.json", "road", [*flags, *(["--index-pin", pin] if pinned else [])])
                elapsed = 1000 * (time.monotonic() - started)
                requests = rows[pos:]
                filename = pin.split(":")[0] + ".district" if pinned else "index.json"
                require(Counter(r["name"] for r in requests) == Counter(expected | {filename}), "timed reads differ")
                pair["pinned" if pinned else "plain"] = elapsed
            if turn:
                pairs.append(pair)
    return {"body_rate_mbps": mbps, "elapsed_ms": pairs,
            "median_ms": {k: statistics.median(pair[k] for pair in pairs) for k in pairs[0]}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    parser.add_argument("--out", type=Path, default=ROOT / "bench/out/ville-paris/pinned-directory")
    parser.add_argument("--bootstrap-mbps", type=float, help="time cold startup on a shared loopback link in Mbit/s")
    args = parser.parse_args()
    if args.bootstrap_mbps is not None and (not math.isfinite(args.bootstrap_mbps) or args.bootstrap_mbps <= 0):
        parser.error("--bootstrap-mbps must be positive")
    out, binary = args.out.resolve(), args.bin.resolve()
    out.mkdir(exist_ok=True, parents=True)
    # A new temporary origin also makes the missing-index control safe to rerun.
    with tempfile.TemporaryDirectory(dir=out) as temp:
        pages = Path(temp)
        pin = cook(HERE / "fixtures/bir-hakeim.glb", pages, 128, pin_index=True)
        sha, length = pin.split(":")
        name = sha + ".district"
        packed = (pages / name).read_bytes()
        raw = (pages / "index.json").read_bytes()
        index = json.loads(raw)
        require(hashlib.sha256(packed).hexdigest() == sha and len(packed) == int(length), "incorrect published pin")
        require(packed[:4] == b"ODI1" and struct.unpack("<I", packed[4:8])[0] == len(raw)
                and unpack_support(packed[8:]) == raw, "packed directory did not reproduce the JSON")
        keys = ("source_sha256", "body_before_step", "physics", "physics_events", "support",
                "visual_events", "published", "first_contact", "collision_probes", "final_foot_m")
        contacts = {}
        for route in ("road", "rail", "roof"):
            flags = ["--trace-body", "--delay-ms", "1400", *RAYS]
            original = replay(binary, pages, out / (route + "-plain.json"), route, flags)
            frozen = replay(binary, pages, out / (route + "-pinned.json"), route, [*flags, "--index-pin", pin])
            same(original, frozen, keys)
            contacts[route] = {"tick": frozen["first_contact"]["tick"], "samples": len(frozen["body_before_step"])}
        http_flags = ["--all-resident", "--no-visuals", "--tick", "0", *RAYS]
        expected = {t["physics"]["file"] for t in index["tiles"] if t.get("physics")} | {index["support"]["file"]}
        with origin(pages) as (url, rows), tempfile.TemporaryDirectory(dir=out) as cache:
            flags = [*http_flags, "--content-cache", cache]
            plain = replay(binary, url, out / "http-plain.json", "road", flags)
            require(Counter(r["name"] for r in rows) == Counter(expected | {"index.json"}), "plain requests differ")
            plain_wire = sum(r["bytes"] for r in rows)
        timings = None
        if args.bootstrap_mbps is not None:
            timings = timed_start(binary, pages, out, pin, args.bootstrap_mbps, http_flags, expected)
        (pages / "index.json").unlink()
        with tempfile.TemporaryDirectory(dir=out) as cache:
            with origin(pages) as (url, rows):
                flags = [*http_flags, "--content-cache", cache, "--index-pin", pin]
                cold = replay(binary, url, out / "http-cold.json", "road", flags)
                same(plain, cold, ("source_sha256", "collision_probes", "final_foot_m", "physics", "published"))
                require(Counter(r["name"] for r in rows) == Counter(expected | {name}), "wrong immutable requests")
                require(all(r["content_encoding"] is None and r["sha256"] == Path(r["name"]).stem for r in rows),
                        "origin changed the stored representation")
                wire = sum(r["bytes"] for r in rows)
                io = cold["transport"]["io"]
                require(io["index_requests"] == 0 and io["content_body_bytes"] == wire, "wrong immutable accounting")
                n = len(rows)
                warm = replay(binary, url, out / "http-warm.json", "road", flags)
                require(len(rows) == n and warm["transport"]["io"]["cache_hits"] == n, "warm run contacted origin")
                with (Path(cache) / name).open("wb") as damaged:
                    damaged.write(packed[:-1] + bytes([packed[-1] ^ 1]))
                repaired = replay(binary, url, out / "http-repaired.json", "road", flags)
                require([r["name"] for r in rows[n:]] == [name] and repaired["transport"]["io"]["invalid_cache_entries"] == 1,
                        "damaged directory was not repaired with one read")
            # The same origin is now offline.
            offline = replay(binary, url, out / "offline.json", "road",
                             [*http_flags, "--index-pin", pin, "--content-cache", cache])
            same(cold, offline, ("collision_probes", "source_sha256", "physics", "published"))
            require(offline["transport"]["io"]["content_requests"] == 0, "offline simulation tried to fetch")
        zlib_frame = packed[:8] + zlib.compress(raw, 9)
        other = pinned_file(pages, zlib_frame)
        fallback = replay(binary, pages, out / "zlib.json", "road", [*http_flags, "--index-pin", other])
        same(cold, fallback, ("collision_probes", "source_sha256", "published"))
        for i, bad in enumerate((packed[:-1], packed + b"x", zlib_frame[:-1], zlib_frame + b"x",
                                 b"BAD!" + packed[4:], b"ODI1" + struct.pack("<I", 64 * 1024 * 1024 + 1) + packed[8:])):
            reject(binary, pages, pinned_file(pages, bad), out / f"bad-{i}.json")
        reject(binary, pages, sha + ":" + str(len(packed) + 1), out / "wrong-size.json")
        next_pin = cook(HERE / "fixtures/bir-hakeim.glb", pages, 256, pin_index=True)
        require(next_pin != pin, "changed district kept the same pin")
        previous = replay(binary, pages, out / "old-snapshot.json", "road", [*http_flags, "--index-pin", pin])
        latest = replay(binary, pages, out / "new-snapshot.json", "road", [*http_flags, "--index-pin", next_pin])
        require(previous["tile_edge_m"] == 128 and latest["tile_edge_m"] == 256, "snapshot identities crossed")
        same(cold, previous, ("source_sha256", "collision_probes", "physics", "published"))
        result = {"source_sha256": index["source_sha256"], "pin": pin, "pin_ascii_bytes": len(pin),
                  "packed_index_bytes": len(packed), "plain_index_bytes": len(raw), "plain_http_bytes": plain_wire,
                  "pinned_http_bytes": wire, "cold_requests": n, "warm_requests": 0, "cache_repair_bytes": len(packed),
                  "contacts": contacts, "physical_files": len(expected) - 1}
        result["plain_index_sha256"] = hashlib.sha256(raw).hexdigest()
        result["physical_page_bytes"] = sum(t["physics"]["bytes"] for t in index["tiles"] if t.get("physics"))
        result["support_bytes"] = index["support"]["bytes"]
        if timings:
            result["timings"] = timings
        (out / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
        print(json.dumps(result))


if __name__ == "__main__":
    main()
