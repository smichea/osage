#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Measure the district directory and the complete cold/warm transfer at a real HTTP origin."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import runpy
import statistics
import subprocess
import tempfile
import time

from paging_checks import require

HERE = Path(__file__).resolve().parent
origin = runpy.run_path(str(HERE / "check-http.py"))["origin"]


def visit(binary, url, requests, cache, out, route, *flags):
    start = len(requests)
    result = subprocess.run([binary, "--paging", "--pages", url, "--content-cache", cache, "--route", route,
                             "--out", out, *flags], capture_output=True)
    require(result.returncode == 0, result.stderr.decode()[-1000:])
    receipt = json.loads(out.read_bytes())
    rows = requests[start:]
    (index,) = [row for row in rows if row["name"] == "index.json"]
    require(all(row["accept_encoding"] == "identity" for row in rows if row["name"] != "index.json"),
            "a content-addressed page negotiated a different representation")
    return {"status": index["status"], "index_body_bytes": index["bytes"],
            "decoded_index_bytes_read": receipt["transport"]["io"]["index_body_bytes"],
            "total_body_bytes": sum(row["bytes"] for row in rows), "request_count": len(rows),
            "index_encoding": index["content_encoding"], "contact": receipt["first_contact"]}


def timed_bootstrap(binary, out, previous, current, mbps):
    require(mbps > 0, "the shared HTTP rate must be positive")
    samples = {"previous": [], "current": []}
    for rep in range(5):
        for name in (("previous", "current") if rep % 2 == 0 else ("current", "previous")):
            pages = current if name == "current" else previous
            with origin(pages, gzip_index=name == "current", mbps=mbps) as (url, records), tempfile.TemporaryDirectory(dir=out) as cache:
                started = time.perf_counter()
                take = visit(binary, url, records, cache, out/f"bootstrap-{name}-{rep}.json", "road", "--tick", "0")
                first_tick_ms = 1000 * (time.perf_counter() - started)
                first_request = min(v["start_ms"] for v in records)
                complete = max(v["end_ms"] for v in records)
                samples[name].append({"body_bytes": take["total_body_bytes"], "requests": take["request_count"],
                                      "origin_delivery_ms": complete - first_request, "process_to_tick_zero_ms": first_tick_ms})
    medians = {mode: {k: statistics.median(row[k] for row in rows) for k in rows[0]} for mode, rows in samples.items()}
    return {"shared_body_rate_mbps": mbps, "samples": samples, "medians": medians}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, required=True)
    parser.add_argument("--pages", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--previous-pages", type=Path, help="compare cold transfer to the same district with the earlier coarse parent")
    parser.add_argument("--bootstrap-mbps", type=float, help="time paired cold first simulation ticks at a shared HTTP rate")
    args = parser.parse_args()
    require(args.bootstrap_mbps is None or args.previous_pages, "timed paired bootstrap needs --previous-pages")
    pages, out = args.pages.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    index = (pages / "index.json").read_bytes()
    if args.previous_pages:
        previous = json.loads((args.previous_pages / "index.json").read_bytes())
        current = json.loads(index)
        require(previous["source_sha256"] == current["source_sha256"] and previous["tiles"] == current["tiles"]
                and previous["support"] == current["support"], "previous district differs in more than coarse storage")
    packed = len(gzip.compress(index, compresslevel=6, mtime=0))
    results = {}
    for route in ("rail", "road", "roof"):
        results[route] = {}
        for label, zipped in (("identity", False), ("gzip", True)):
            with origin(pages, conditional=True, gzip_index=zipped) as (url, requests), tempfile.TemporaryDirectory(dir=out) as cache:
                cold = visit(args.bin, url, requests, cache, out/f"{route}-{label}.json", route)
                require(cold["status"] == 200 and cold["index_body_bytes"] == (packed if zipped else len(index))
                        and cold["decoded_index_bytes_read"] == len(index), "directory encoding or size mismatch")
                warm = visit(args.bin, url, requests, cache, out/f"{route}-{label}-warm.json", route)
                require(warm["status"] == 304 and warm["request_count"] == 1 and warm["total_body_bytes"] == 0
                        and warm["contact"] == cold["contact"], "cache validation or landing mismatch")
                results[route][label] = {"cold": cold, "warm": warm}
        original, compressed = (v["cold"] for v in results[route].values())
        require(compressed["contact"] == original["contact"], "directory transfer changed the contact")
        require(original["total_body_bytes"] - compressed["total_body_bytes"] == len(index) - packed,
                "another resource changed in the cold comparison")
        if args.previous_pages:
            with origin(args.previous_pages, conditional=True) as (url, requests), tempfile.TemporaryDirectory(dir=out) as cache:
                control = visit(args.bin, url, requests, cache, out/f"{route}-previous.json", route)
            old_parent, new_parent = previous["parent"]["bytes"], current["parent"]["bytes"]
            saving = control["index_body_bytes"] - packed + old_parent - new_parent
            require(control["contact"] == compressed["contact"] and control["total_body_bytes"] - compressed["total_body_bytes"] == saving,
                    "coarse parent changed the expected resources or landing")
            results[route]["previous_identity"] = control
    with origin(pages, conditional=True, gzip_index=True, corrupt_once="index.json") as (url, requests), tempfile.TemporaryDirectory(dir=out) as cache:
        path = out/"bad-header.json"
        command = [args.bin, "--paging", "--pages", url, "--content-cache", cache, "--route", "road",
                   "--tick", "0", "--out", path]
        failed = subprocess.run(command, capture_output=True)
        require(failed.returncode != 0 and len(requests) == 1 and not list(Path(cache).glob("index-*.http")),
                "invalid compressed bytes were accepted or cached")
        retry = visit(args.bin, url, requests, cache, out/"retry.json", "road", "--tick", "0")
        require(retry["status"] == 200 and retry["index_encoding"] == "gzip", "retry did not fetch the valid directory")
    with args.bin.open("rb") as f:
        binary = hashlib.file_digest(f, "sha256").hexdigest()
    report = {"binary_sha256": binary, "index_sha256": hashlib.sha256(index).hexdigest(),
              "index_identity_bytes": len(index), "index_gzip_bytes": packed, "routes": results, "after_corruption": retry}
    if args.bootstrap_mbps is not None:
        report["bootstrap"] = timed_bootstrap(args.bin, out, args.previous_pages, pages, args.bootstrap_mbps)
    (out/"summary.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"Directory {len(index):,} → {packed:,} bytes; 3 equal landings, 6 warm revalidations, rejected gzip and successful retry")


if __name__ == "__main__":
    main()
