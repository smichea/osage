#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Exercise shared physical reservations, reusable slots and late real bytes at Bir-Hakeim."""

import argparse
import hashlib
import json
import os
import resource
import runpy
import statistics
import subprocess
import sys
import tempfile
import threading
import time
from contextlib import contextmanager
from pathlib import Path

from paging_support_storage import unpack_support
from paging_collision_codec import read_collision

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
origin = runpy.run_path(str(HERE / "check-http.py"))["origin"]


def save(p, data):
    p.write_text(json.dumps(data, indent=2) + "\n")


@contextmanager
def disk_origin(pages, pauses):
    # Regular assets are untouched. Each traced physical body is served through its own named pipe.
    with tempfile.TemporaryDirectory(prefix="osage-observer-disk-") as tmp:
        d = Path(tmp)
        records = []
        closed = threading.Event()
        threads = []
        for path in pages.iterdir():
            if path.name not in pauses:
                (d / path.name).symlink_to(path)
                continue
            fifo = d / path.name
            os.mkfifo(fifo)

            def send(path=path, fifo=fifo):
                content = path.read_bytes()
                with fifo.open("wb", buffering=0) as out:
                    if closed.is_set():
                        return
                    time.sleep(pauses[path.name])
                    assert out.write(content) == len(content)
                    records.append(
                        {
                            "name": path.name,
                            "sha256": hashlib.sha256(content).hexdigest(),
                            "bytes": len(content),
                        }
                    )

            t = threading.Thread(target=send, daemon=True)
            t.start()
            threads.append((t, fifo))
        try:
            yield d, records
        finally:
            closed.set()
            for thread, fifo in threads:
                if thread.is_alive():
                    try:
                        fd = os.open(fifo, os.O_RDONLY | os.O_NONBLOCK)
                        thread.join(timeout=5)
                        os.close(fd)
                    except OSError:
                        pass


def cpu_cost(cpu, native, pages, duplicate, out, expected):
    """Time the complete local native process; rotate execution order and discard warm-up."""
    empty = out / "primary-reservation.json"
    save(empty, {"version": 1, "tracks": []})
    streams = {"single": None, "reserved_primary": empty, "duplicated": duplicate}
    times = {name: [] for name in streams}
    orders = list(streams)
    for sample in range(16):
        shift = sample % len(orders)
        for name in orders[shift:] + orders[:shift]:
            dest = out / ("cpu-" + name + ".json")
            cmd = [
                "taskset",
                "-c",
                str(cpu),
                native,
                "--paging",
                "--pages",
                pages,
                "--route",
                "road",
                "--delay-ms",
                "200",
                "--tick",
                "134",
                "--out",
                dest,
            ]
            if streams[name] is not None:
                cmd.extend(["--physics-observers", streams[name]])
            before = resource.getrusage(resource.RUSAGE_CHILDREN)
            subprocess.run(cmd, check=True, capture_output=True, timeout=45)
            after = resource.getrusage(resource.RUSAGE_CHILDREN)
            duration = (
                after.ru_utime - before.ru_utime + after.ru_stime - before.ru_stime
            ) * 1000
            r = json.loads(dest.read_bytes())
            assert (
                r["physics_events"] == expected["physics_events"]
                and r["first_contact"] == expected["first_contact"]
            )
            if sample:
                times[name].append(duration)
    return {
        "cpu": cpu,
        "steps": 135,
        "process_cpu_samples_ms": times,
        "paired_delta_ms": {
            k: statistics.median(
                b - a for a, b in zip(times["single"], times[k], strict=True)
            )
            for k in ("reserved_primary", "duplicated")
        },
        "process_cpu_medians_ms": {
            name: statistics.median(values) for name, values in times.items()
        },
    }


def main():
    a = argparse.ArgumentParser(description=__doc__)
    a.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    a.add_argument("--pages", type=Path)
    a.add_argument(
        "--out", type=Path, default=ROOT / "bench/out/ville-paris/physics-observers"
    )
    a.add_argument(
        "--cpu",
        type=int,
        help="measure 15 alternating full-process samples per mode pinned to this Linux CPU",
    )
    args = a.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    pages = (args.pages or out / "pages").resolve()
    if not (pages / "index.json").is_file():
        subprocess.run(
            [
                sys.executable,
                HERE / "cook-pages.py",
                HERE / "fixtures/bir-hakeim.glb",
                pages,
            ],
            check=True,
        )
    index = json.loads((pages / "index.json").read_bytes())
    coords = {"shared": (0, 0), "abandoned": (1, -1), "replacement": (-1, 0)}
    tile = {
        k: next(v["physics"] for v in index["tiles"] if (v["x"], v["z"]) == cell)
        for k, cell in coords.items()
    }
    physical_cells = [t for t in index["tiles"] if t.get("physics")]
    proxy = json.loads(unpack_support((pages / index["support"]["file"]).read_bytes()))
    proxies = {
        (t["x"], t["z"]): v for t, v in zip(physical_cells, proxy["pages"], strict=True)
    }
    surfaces = {}
    for key, layer in tile.items():
        full = read_collision(pages, layer)
        assert [s["source_node"] for s in full] == layer["source_nodes"]
        assert [s["id"] for s in full] == layer["ids"]
        assert [owner for owner, _, _ in proxies[coords[key]]] == layer["source_nodes"]
        surfaces[key] = dict(zip(layer["ids"], layer["source_nodes"], strict=True))
    assert surfaces["shared"]["road"] == 146
    if "road" in surfaces["replacement"]:
        # Older directories split the road over both cells.
        assert surfaces["replacement"]["road"] == surfaces["shared"]["road"]
        replacement_foot = [-102, 17, 0]
        radius_m = 8
    else:
        # The current cooker keeps the road together. Visit the western quay
        # away from the bridge so this actor needs only the replacement page.
        assert "quay--1" in surfaces["replacement"]
        replacement_foot = [-122, 8.5, 64]
        # A separate island page is 5.8 m below the road in this layout.
        radius_m = 4

    cuts = {
        tile[k]["file"]: s
        for k, s in [("shared", 0.45), ("abandoned", 1.25), ("replacement", 0.0)]
    }

    def run(name, location, tracks=None, flags=(), radius=True):
        dest = out / (name + ".json")
        cmd = [
            args.bin,
            "--paging",
            "--pages",
            location,
            "--route",
            "road",
            "--trace-body",
            "--tick",
            "134",
            "--out",
            dest,
        ]
        if radius:
            cmd += ["--physics-radius", str(radius_m)]
        if tracks:
            cmd += ["--physics-observers", tracks]
        subprocess.run([*cmd, *flags], check=True, capture_output=True, timeout=45)
        return json.loads(dest.read_bytes())

    def trajectory_record(frame):
        return {
            k: frame[k]
            for k in ("tick", "foot_m", "center_m", "velocity_m_s", "supported")
        }

    def single_track(slot, keys):
        return {
            "slot": slot,
            "start": keys[0]["tick"],
            "end": keys[-1]["tick"] + 1,
            "keys": keys,
        }

    solo = run("solo", pages, flags=("--wall-clock",))
    baseline = solo["first_contact"]
    assert baseline["support"] == "road" and baseline["geometry"] == "full"
    k = [trajectory_record(frame) for frame in solo["body_before_step"]]
    duplicate = out / "same-actor.json"
    save(duplicate, {"version": 1, "tracks": [single_track(1, k)]})

    def moving(start, end, pos):
        return [
            {
                "tick": t,
                "foot_m": [pos[0] + (t - start) * 0.045, pos[1], pos[2]],
                "center_m": [pos[0] + (t - start) * 0.045, pos[1] + 0.88, pos[2]],
                "velocity_m_s": [2.7, 0, 0],
                "supported": True,
            }
            for t in (start, end)
        ]

    tracks = out / "three-occupants.json"
    save(
        tracks,
        {
            "version": 1,
            "tracks": [
                single_track(1, k[:8]),
                single_track(1, moving(8, 19, [148, 31, -110])),
                single_track(1, moving(20, 134, replacement_foot)),
            ],
        },
    )

    def same_contact(r):
        assert (
            r["first_contact"] == baseline
            and r["at_contact"]["source_path"] == solo["at_contact"]["source_path"]
        )
        assert (
            r["body_before_step"] == solo["body_before_step"]
            and r["final_foot_m"] == solo["final_foot_m"]
        )
        assert (
            r["support"]["duplicate_layer_contact_ticks"]
            == r["support"]["stats"]["held_ticks"]
            == 0
        )

    def equivalent(tag, location, extra=()):
        one = run(tag + "-one", location, flags=extra)
        two = run(tag + "-duplicate", location, duplicate, extra)
        same_contact(one)
        same_contact(two)
        assert (
            one["physics"]["reads"] == two["physics"]["reads"] == 1
            and one["physics"]["read_bytes"]
            == two["physics"]["read_bytes"]
            == tile["shared"]["bytes"]
        )
        stats = two["physics_observers"]["transfers"][tile["shared"]["sha256"]]
        assert (
            stats["verified_reads"] == stats["starts"] == stats["installs"] == 1
            and stats["verified_bytes"] == tile["shared"]["bytes"]
        )
        event = next(
            e
            for e in two["physics_observers"]["events"]
            if e["state"] == "read_started"
        )
        assert event["observers"] == [
            {"slot": 0, "generation": 1},
            {"slot": 1, "generation": 1},
        ]
        return {
            "physical_reads_per_replay": 1,
            "bytes_per_replay": stats["verified_bytes"],
            "source_path": two["at_contact"]["source_path"],
        }

    equality = {}
    equality["disk"] = equivalent("disk", pages, ("--wall-clock",))
    with origin(pages) as (url, records):
        equality["http"] = equivalent("http", url)
        assert sum(v["name"] == tile["shared"]["file"] for v in records) == 2
    # The default trajectory scheduler also coalesces identical observations in the known link control.
    control = run("trajectory-one", pages, flags=("--delay-ms", "200"), radius=False)
    both = run("trajectory-two", pages, duplicate, ("--delay-ms", "200"), radius=False)
    assert (
        control["physics_events"] == both["physics_events"]
        and control["physics"] == both["physics"]
        and control["first_contact"] == both["first_contact"]
    )

    deterministic_reads = both["physics_observers"]["transfers"]
    assert (
        sum(x["verified_bytes"] for x in deterministic_reads.values())
        == both["physics"]["read_bytes"]
    )
    assert (
        sum(x["installs"] for x in deterministic_reads.values())
        == both["physics"]["reads"]
    )

    preload = run(
        "all-resident",
        pages,
        duplicate,
        ("--all-resident", "--tick", "1"),
        radius=False,
    )
    preload_transfers = preload["physics_observers"]["transfers"]
    assert (
        sum(x["verified_bytes"] for x in preload_transfers.values())
        == preload["physics"]["read_bytes"]
    )
    assert (
        sum(x["starts"] for x in preload_transfers.values())
        == sum(x["installs"] for x in preload_transfers.values())
        == preload["physics"]["reads"]
    )

    def validate(r, records, cache=False):
        same_contact(r)
        pool = r["physics_observers"]
        events = pool["events"]
        cost = pool["transfers"]
        assert r["physics"]["peak_in_flight"] <= 4 and pool["slots_allocated"] == 2

        def event(which, state):
            return next(
                e
                for e in events
                if e["tile"] == list(coords[which]) and e["state"] == state
            )

        shared = event("shared", "read_started")
        drop = event("shared", "observers_changed")
        landing = event("shared", "current_arrival")
        assert shared["observers"] == [
            {"slot": 0, "generation": 1},
            {"slot": 1, "generation": 1},
        ]
        assert (
            drop["observers"] == [{"slot": 0, "generation": 1}]
            and shared["lease"] == drop["lease"] == landing["lease"]
        )
        old = event("abandoned", "read_started")
        new = event("replacement", "read_started")
        assert (
            old["lease"]["slot"] == new["lease"]["slot"]
            and old["lease"]["generation"] + 1 == new["lease"]["generation"]
        )
        assert new["observers"] == [{"slot": 1, "generation": 3}] and old[
            "observers"
        ] == [{"slot": 1, "generation": 2}]
        if not cache:
            stale = event("abandoned", "stale_arrival")
            ready = event("replacement", "current_arrival")
            assert (
                shared["tick"] < drop["tick"] < landing["tick"]
                and ready["tick"] < stale["tick"]
                and stale["lease"] == old["lease"]
            )
            assert (
                stale["bytes"] == tile["abandoned"]["bytes"]
                and stale["observers"] == []
            )
            assert not any(
                e["tile"] == list(coords["abandoned"]) and e["state"] == "ready"
                for e in r["physics_events"]
            )
            assert r["physics"]["reads"] == 2
        names = {v["file"] for v in tile.values()}
        observed = [e for e in records if e["name"] in names]
        assert sorted(v["name"] for v in observed) == ([] if cache else sorted(names))
        origin_by_digest = {}
        for key, layer in tile.items():
            reads = [v for v in observed if v["name"] == layer["file"]]
            assert all(
                v["sha256"] == layer["sha256"] and v["bytes"] == layer["bytes"]
                for v in reads
            )
            origin_by_digest[layer["sha256"]] = {
                "accesses": len(reads),
                "bytes": sum(v["bytes"] for v in reads),
            }
            counter = cost[layer["sha256"]]
            assert (
                counter["starts"] == counter["verified_reads"] == 1
                and counter["verified_bytes"] == layer["bytes"]
            )
            assert counter["installs"] == int(key != "abandoned" or cache)
            if not cache:
                assert counter["stale_arrivals"] == int(key == "abandoned")
        return {
            "generation_events": events,
            "reads_per_digest": cost,
            "origin_reads_per_digest": origin_by_digest,
            "wire_content_bytes": sum(e.get("bytes", 0) for e in observed),
            "read_physical_bytes": sum(x["verified_bytes"] for x in cost.values()),
            "installed_bytes": r["physics"]["read_bytes"],
            "peak_in_flight": r["physics"]["peak_in_flight"],
            "peak_resident_bytes": r["physics"]["peak_page_bytes"],
            "active_slots": pool["active_reservations"],
            "allocated_slots": pool["slots_allocated"],
            "first_contact": r["first_contact"],
            "owner_path": r["at_contact"]["source_path"],
            "transport": r["transport"]["io"],
        }

    with disk_origin(pages, cuts) as (loc, records):
        disk = validate(run("late-disk", loc, tracks, ("--wall-clock",)), records)
    with (
        tempfile.TemporaryDirectory(prefix="osage-observer-cache-") as cache,
        origin(pages, delay=cuts, conditional=True) as (url, records),
    ):
        cold = validate(
            run("late-http", url, tracks, ("--content-cache", cache)), records
        )
        n = len(records)
        warmrun = run("warm-http", url, tracks, ("--content-cache", cache))
        warm = validate(warmrun, records[n:], True)
        assert (
            warmrun["transport"]["io"]["content_body_bytes"] == 0
            and warmrun["transport"]["io"]["index_revalidations"] == 1
        )
    # Visit the abandoned page again while its old transfer is still running.
    again = out / "reacquire.json"
    occupants = json.loads(tracks.read_bytes())["tracks"][:2]
    occupants.append(single_track(1, moving(40, 134, [148, 31, -110])))
    save(again, {"version": 1, "tracks": occupants})
    with (
        tempfile.TemporaryDirectory(prefix="osage-observer-reacquire-") as cache,
        origin(pages, delay=cuts, conditional=True) as (url, records),
    ):
        reacquired = run("reacquire-http", url, again, ("--content-cache", cache))
        same_contact(reacquired)
        q = tile["abandoned"]
        transfers = reacquired["physics_observers"]["transfers"][q["sha256"]]
        assert (
            transfers["starts"] == transfers["verified_reads"] == 2
            and transfers["verified_bytes"] == 2 * q["bytes"]
        )
        assert (
            transfers["installs"] == transfers["stale_arrivals"] == 1
            and transfers["stale_bytes"] == q["bytes"]
        )
        records_q = [r for r in records if r["name"] == q["file"]]
        assert (
            len(records_q) == 1
            and records_q[0]["sha256"] == q["sha256"]
            and records_q[0]["bytes"] == q["bytes"]
        )
        events_q = [
            e
            for e in reacquired["physics_observers"]["events"]
            if e["sha256"] == q["sha256"]
        ]
        reservations_q = [e for e in events_q if e["state"] == "reserved"]
        reads_q = [e for e in events_q if e["state"] == "read_started"]
        stale_q = next(e for e in events_q if e["state"] == "stale_arrival")
        publish_q = next(e for e in events_q if e["state"] == "installed")
        assert len(reservations_q) == len(reads_q) == 2
        assert (
            reservations_q[1]["tick"]
            < stale_q["tick"]
            < reads_q[1]["tick"]
            <= publish_q["tick"]
        )
        assert (
            reservations_q[0]["lease"] == stale_q["lease"]
            and reservations_q[1]["lease"] == publish_q["lease"]
        )
        assert (
            stale_q["lease"]["slot"] == publish_q["lease"]["slot"]
            and stale_q["lease"]["generation"] < publish_q["lease"]["generation"]
        )
        assert publish_q["observers"] == [{"slot": 1, "generation": 3}]
        reentry = {
            "sha256": q["sha256"],
            "transfers": transfers,
            "events": events_q,
            "origin_accesses": len(records_q),
            "origin_bytes": records_q[0]["bytes"],
        }

    with args.bin.open("rb") as native:
        binary_sha256 = hashlib.file_digest(native, "sha256").hexdigest()
    report = {
        "binary_sha256": binary_sha256,
        "source_sha256": solo["source_sha256"],
        "equal_motion": equality,
        "trajectory_duplicate": {
            "identical_events": True,
            "physical_reads": both["physics"]["reads"],
            "physical_bytes": both["physics"]["read_bytes"],
        },
        "immediate_preload": {
            "reads": preload["physics"]["reads"],
            "bytes": preload["physics"]["read_bytes"],
        },
        "cpu": cpu_cost(args.cpu, args.bin, pages, duplicate, out, control)
        if args.cpu is not None
        else None,
        "body_samples_equal": len(k),
        "physical_surface_owners_in_full_and_proxy": surfaces,
        "disk": disk,
        "http_cold": cold,
        "http_warm": warm,
        "http_reacquire": reentry,
        "assets": {
            k: {"tile": coords[k], "sha256": v["sha256"], "bytes": v["bytes"]}
            for k, v in tile.items()
        },
    }
    save(out / "summary.json", report)
    print(
        json.dumps(
            {
                "body_samples": len(k),
                "bytes": disk["read_physical_bytes"],
                "installed": disk["installed_bytes"],
                "contact": baseline["tick"],
            }
        )
    )


if __name__ == "__main__":
    main()
