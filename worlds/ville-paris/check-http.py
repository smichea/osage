#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Qualify ordinary district HTTP reads against an independent origin and local replay."""

import argparse
import collections
import contextlib
import gzip
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


@contextlib.contextmanager
def origin(
    directory, *, delay=None, corrupt_once=None, fail_once=None, conditional=False, gzip_index=False, mbps=None, missing=None
):
    records = []
    lock = threading.Lock()
    seen = collections.Counter()
    start = time.monotonic()
    next_send = start

    class Serve(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, *args):
            pass

        def do_GET(self):
            nonlocal next_send
            name = self.path.removeprefix("/district/")
            if Path(name).name != name or not (directory / name).is_file():
                if missing is not None:
                    with lock:
                        missing.append(name)
                self.send_error(404)
                return
            with lock:
                first = not seen[name]
                seen[name] += 1
            begin = time.monotonic()
            time.sleep((delay or {}).get(name, 0))
            status = 503 if first and name == fail_once else 200
            raw = (directory / name).read_bytes() if status == 200 else b""
            accepts = self.headers.get("Accept-Encoding", "")
            compressed = gzip_index and name == "index.json" and "gzip" in [
                entry.strip() for entry in accepts.lower().split(",")
            ]
            if compressed:
                raw = gzip.compress(raw, compresslevel=6, mtime=0)
            if first and name == corrupt_once:
                raw = bytes([raw[0] ^ 1]) + raw[1:]
            tag = (
                '"' + hashlib.sha256(raw).hexdigest() + '"'
                if conditional and name == "index.json"
                else None
            )
            sent_tag = self.headers.get("If-None-Match")
            if tag and sent_tag == tag:
                status = 304
                raw = b""
            self.send_response(status)
            if tag:
                self.send_header("ETag", tag)
            if gzip_index and name == "index.json":
                self.send_header("Vary", "Accept-Encoding")
            if compressed and status == 200:
                self.send_header("Content-Encoding", "gzip")
            self.send_header("Content-Type", "application/octet-stream")
            if status != 304:
                self.send_header("Content-Length", str(len(raw)))
            self.end_headers()
            if mbps:
                # All request handlers draw bandwidth from the same simulated last mile.
                for offset in range(0, len(raw), 8192):
                    block = raw[offset:offset + 8192]
                    with lock:
                        next_send = max(time.monotonic(), next_send) + 8 * len(block) / (mbps * 1_000_000)
                        deadline = next_send
                    time.sleep(max(0., deadline - time.monotonic()))
                    self.wfile.write(block)
                    self.wfile.flush()
            else:
                self.wfile.write(raw)
                self.wfile.flush()
            with lock:
                records.append(
                    {
                        "name": name,
                        "status": status,
                        "if_none_match": sent_tag,
                        "accept_encoding": accepts,
                        "content_encoding": "gzip" if compressed and status == 200 else None,
                        "bytes": len(raw),
                        "sha256": hashlib.sha256(raw).hexdigest(),
                        "start_ms": round(1000 * (begin - start), 2),
                        "end_ms": round(1000 * (time.monotonic() - start), 2),
                    }
                )

    with ThreadingHTTPServer(("127.0.0.1", 0), Serve) as srv:
        thread = threading.Thread(target=srv.serve_forever, daemon=True)
        thread.start()
        try:
            yield f"http://127.0.0.1:{srv.server_port}/district", records
        finally:
            srv.shutdown()
            thread.join()


def check_index(exe, src, replay, run_root, out):
    with tempfile.TemporaryDirectory(dir=run_root) as temp:
        cache = Path(temp) / "cache"
        district = Path(temp) / "district"
        district.mkdir()
        for path in src.iterdir():
            if path.is_file() and path.name != "index.json":
                (district / path.name).symlink_to(path)
        manifest = district / "index.json"
        shutil.copyfile(src / "index.json", manifest)
        results = {}
        with origin(district, conditional=True) as (url, records):
            for label, expected in [
                ("new", 200),
                ("unchanged", 304),
                ("changed", 200),
                ("healed", 200),
            ]:
                if label == "changed":
                    with manifest.open("ab") as f:
                        f.write(b" ")
                if label == "healed":
                    (entry,) = cache.glob("index-*.http")
                    data = bytearray(entry.read_bytes())
                    data[-1] ^= 1
                    entry.write_bytes(data)
                before = len(records)
                report = out / ("index-" + label + ".json")
                subprocess.run(
                    [
                        exe,
                        "--paging",
                        "--pages",
                        url,
                        "--content-cache",
                        cache,
                        "--route",
                        "floor",
                        "--replay",
                        replay,
                        "--all-resident",
                        "--tick",
                        "0",
                        "--out",
                        report,
                    ],
                    check=True,
                )
                io = json.loads(report.read_bytes())["transport"]["io"]
                (row,) = [v for v in records[before:] if v["name"] == "index.json"]
                assert (
                    row["status"] == expected and row["bytes"] == io["index_body_bytes"]
                )
                assert (row["if_none_match"] is not None) == (
                    label in ("changed", "unchanged")
                )
                assert io["index_revalidations"] == int(expected == 304)
                if label != "new":
                    assert io["content_requests"] == 0
                results[label] = {
                    "status": expected,
                    "index_bytes": row["bytes"],
                    "http_content_bytes": io["content_body_bytes"],
                }
        return results


def check(run_root, exe, source, city_pages):
    from paging_support_storage import unpack_support

    out = run_root / "results"
    out.mkdir(parents=True, exist_ok=True)
    src = source / "demand"
    index = json.loads((src / "index.json").read_bytes())
    image = max(index["texture_resources"], key=lambda v: v["bytes"])["file"]
    cases = {}

    def invoke(
        label,
        pages,
        route,
        last,
        *,
        origin_options=None,
        wall_clock=False,
        trace_body=False,
        cache=None,
        flight=None,
        radius=40,
    ):
        args = [
            exe,
            "--paging",
            "--route",
            route,
            "--tick",
            str(last),
            "--out",
            out / (label + ".json"),
            "--visual-radius",
            str(radius),
        ]
        if trace_body:
            args += ["--trace-body"]
        if flight:
            args += ["--replay", flight]
        if cache:
            args += ["--content-cache", cache]
        with (out / (label + ".log")).open("w") as log:
            if wall_clock:
                subprocess.run(
                    [*args, "--wall-clock", "--pages", pages],
                    stdout=log,
                    stderr=subprocess.STDOUT,
                    check=True,
                    timeout=30,
                )
                records = []
            elif origin_options is None:
                subprocess.run(
                    [
                        *args,
                        "--pages",
                        pages,
                        "--delay-ms",
                        "0",
                        "--mbps-per-kind",
                        "1000",
                    ],
                    stdout=log,
                    stderr=subprocess.STDOUT,
                    check=True,
                )
                records = []
            else:
                with origin(pages, **origin_options) as (url, records):
                    subprocess.run(
                        [*args, "--pages", url],
                        stdout=log,
                        stderr=subprocess.STDOUT,
                        check=True,
                    )
        report = json.loads((out / (label + ".json")).read_bytes())
        if origin_options is not None:
            io = report["transport"]["io"]
            for is_index, key in ((True, "index"), (False, "content")):
                requests = [
                    r for r in records if (r["name"] == "index.json") == is_index
                ]
                assert len(requests) == io[key + "_requests"], (label, requests, io)
                assert sum(r["bytes"] for r in requests) == io[key + "_body_bytes"], (
                    label,
                    requests,
                    io,
                )
            (out / (label + "-origin.json")).write_text(
                json.dumps(records, indent=2) + "\n"
            )
        cases[label] = {
            "published": report["published"],
            "first_contact": report["first_contact"],
            ("disk" if wall_clock else "http"): report.get("transport", {}).get("io"),
        }
        return report, records

    def floor(label, **kwargs):
        return invoke(label, src, "floor", 110, flight=source / "replay.json", **kwargs)

    local, _ = floor("directory")
    disk, _ = floor("paced-disk", wall_clock=True)
    assert disk["transport"]["kind"] == "disk"
    assert (
        disk["published"] == local["published"]
        and disk["final_foot_m"] == local["final_foot_m"]
    )
    disk_io = disk["transport"]["io"]
    assert disk_io["content_requests"] == 0 and disk_io["index_requests"] == 0
    assert disk_io["worker_image_decodes"] == 1
    assert disk_io["disk_index_bytes"] == (src / "index.json").stat().st_size

    with tempfile.TemporaryDirectory(dir=run_root) as directory:
        cache = Path(directory)
        cold, rec = floor("cold", origin_options={}, cache=cache)
        assert sum(r["name"] == image for r in rec) == 1
        assert len({r["name"] for r in rec}) == len(rec)
        assert (
            disk_io["disk_content_bytes"]
            == cold["transport"]["io"]["content_body_bytes"]
        )
        assert (
            disk_io["disk_content_reads"] == cold["transport"]["io"]["content_requests"]
        )
        accessed = {r["name"] for r in rec if r["name"] != "index.json"}
        assert not any(
            r["file"] in accessed
            for r in index["texture_resources"]
            if r["file"] != image
        )
        warm, rec = floor("warm", origin_options={}, cache=cache)
        assert [r["name"] for r in rec] == ["index.json"]
        assert warm["transport"]["io"]["cache_hits"] > 0
        path = cache / image
        valid = path.read_bytes()
        path.write_bytes(bytes([valid[0] ^ 1]) + valid[1:])
        healed, rec = floor("disk-repair", origin_options={}, cache=cache)
        assert sorted(r["name"] for r in rec) == sorted(["index.json", image])
        assert (
            healed["transport"]["io"]["invalid_cache_entries"] == 1
            and path.read_bytes() == valid
        )
        path.unlink()
        bad, rec = floor(
            "bad-body", origin_options={"corrupt_once": image}, cache=cache
        )
        assert sum(r["name"] == image for r in rec) == 2
        assert (
            bad["transport"]["io"]["rejected_bodies"] == 1
            and path.read_bytes() == valid
        )
        first = next(
            e["tick"]
            for e in bad["storage"]["model_events"]
            if e["state"] == "rejected"
        )
        ready = next(
            e["tick"] for e in bad["storage"]["model_events"] if e["state"] == "ready"
        )
        assert ready >= first + bad["clock_hz"]
        cases["bad-body"]["rejected_tick"] = first
        cases["bad-body"]["ready_tick"] = ready
        for report in (cold, warm, healed, bad):
            assert (
                report["published"] == local["published"]
                and report["final_foot_m"] == local["final_foot_m"]
            )

        delayed, rec = floor("slow-image", origin_options={"delay": {image: 0.35}})
        first = next(
            e["tick"]
            for e in delayed["storage"]["model_events"]
            if e["state"] == "started"
        )
        ready = next(
            e["tick"]
            for e in delayed["storage"]["model_events"]
            if e["state"] == "ready"
        )
        assert ready - first >= 15 and delayed["published"] == local["published"]
        cases["slow-image"]["start_tick"] = first
        cases["slow-image"]["ready_tick"] = ready
        page = next(r["name"] for r in rec if r["name"].endswith(".vpage"))
        fixed, rec = floor("page-503", origin_options={"fail_once": page})
        assert [r["status"] for r in rec if r["name"] == page] == [503, 200]
        assert (
            len(fixed["transport"]["rejected_visual_pages"]) == 1
            and fixed["published"] == local["published"]
        )

    if hasattr(os, "mkfifo"):
        # A gated local read exposes a blocking simulation without adding delay code to the client.
        with tempfile.TemporaryDirectory(dir=run_root) as temp:
            pages = Path(temp)
            for page in src.iterdir():
                if page.name != image:
                    (pages / page.name).symlink_to(page)
            pipe = pages / image
            os.mkfifo(pipe)
            raw = (src / image).read_bytes()

            def deliver():
                with pipe.open("wb") as writer:
                    time.sleep(0.35)
                    writer.write(raw)

            writer = threading.Thread(target=deliver, daemon=True)
            writer.start()
            late_disk, _ = invoke(
                "slow-image-disk",
                pages,
                "floor",
                110,
                wall_clock=True,
                flight=source / "replay.json",
            )
            writer.join(timeout=5)
            assert not writer.is_alive()
            events = late_disk["storage"]["model_events"]
            started, ready = (
                next(
                    e["tick"]
                    for e in events
                    if e["sha256"] == Path(image).stem and e["state"] == state
                )
                for state in ("started", "ready")
            )
            assert (
                ready - started >= 15 and late_disk["published"] == local["published"]
            )
            assert late_disk["final_foot_m"] == local["final_foot_m"]
            cases["slow-image-disk"].update(start_tick=started, ready_tick=ready)

    road, _ = invoke("road-directory", city_pages, "road", 130)
    idx = json.loads((city_pages / "index.json").read_bytes())
    tile = road["first_contact"]["tile"]
    physical = next(
        t["physics"]["file"] for t in idx["tiles"] if [t["x"], t["z"]] == tile
    )
    support_file = idx.get("support")
    detailed_bounds = (
        bool(support_file)
        and json.loads(
            unpack_support((city_pages / support_file["file"]).read_bytes())
        )["format"]
        == "ville-paris/support-faces/2"
    )
    flight = None
    end_tick = 130
    if not detailed_bounds:
        # A legacy catalogue can hold the pose for the whole unverified page.
        end_tick = 250
        routes = json.loads((HERE / "fixtures/descent.json").read_bytes())
        trip = next(t for t in routes["routes"] if t["id"] == "road")
        trip["camera"].append({**trip["camera"][-1], "tick": end_tick})
        trip["ticks"] = end_tick
        flight = out / "long-road.json"
        flight.write_text(json.dumps(routes))
    late, _ = invoke(
        "road-late-http",
        city_pages,
        "road",
        end_tick,
        origin_options={"delay": {physical: 1.55}},
        flight=flight,
        trace_body=True,
    )
    a, b = road["first_contact"], late["first_contact"]
    held = sum(
        not v["step"] for v in late["body_before_step"] if 0 < v["tick"] <= b["tick"]
    )
    assert a["tick"] == b["tick"] - held and a["support"] == b["support"], (a, b, held)
    if detailed_bounds:
        assert held == 0 and b["geometry"] == "certified_support"
    else:
        assert held > 0 and b["geometry"] == "full"
    assert (
        late["support"]["duplicate_layer_contact_ticks"] == 0
        and late["support"]["first_full_contact"]
    )
    cases["road-late-http"]["full_contact"] = late["support"]["first_full_contact"]
    cases["road-late-http"]["duplicate_contacts"] = late["support"][
        "duplicate_layer_contact_ticks"
    ]
    cases["index_revalidation"] = check_index(
        exe, src, source / "replay.json", run_root, out
    )
    return cases


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    parser.add_argument(
        "--source", type=Path, default=ROOT / "bench/out/ville-paris/texture-storage"
    )
    parser.add_argument("--city-pages", type=Path)
    parser.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/content-http"
    )
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    src = args.source.resolve()
    if not (src / "demand/index.json").exists():
        subprocess.run(
            [sys.executable, HERE / "check-texture-storage.py", "--out", src],
            check=True,
        )
    city = args.city_pages or out / "city"
    if not (city / "index.json").exists():
        subprocess.run(
            [
                sys.executable,
                HERE / "cook-pages.py",
                HERE / "fixtures/bir-hakeim.glb",
                city,
                "--visual-storage",
                "demand",
            ],
            check=True,
        )
    result = check(out, args.bin.resolve(), src, city)
    (out / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
    print("HTTP cases:", ", ".join(result))


if __name__ == "__main__":
    main()
