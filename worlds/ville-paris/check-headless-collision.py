#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Replay with only the physical files present and account for every HTTP response."""
import argparse
from collections import Counter
import json
from pathlib import Path
import runpy
import statistics
import subprocess
import tempfile

from paging_checks import require

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
cook = runpy.run_path(str(HERE / "cook-pages.py"))["cook"]
origin = runpy.run_path(str(HERE / "check-http.py"))["origin"]
RAYS = ["--collision-ray", "0,32,0", "0,-10,0", "--collision-ray", "185,80,-70", "185,-10,-70"]


def replay(binary, pages, out, route, flags):
    subprocess.run(
        [binary, "--paging", "--pages", pages, "--route", route, "--out", out, *flags],
        stdout=subprocess.DEVNULL, check=True,
    )
    return json.loads(out.read_bytes())


def physical_files(pages, sparse):
    index = json.loads((pages / "index.json").read_bytes())
    files = {"index.json", index["support"]["file"]}
    files.update(t["physics"]["file"] for t in index["tiles"] if t.get("physics"))
    for name in files:
        (sparse / name).symlink_to(pages / name)
    return files


def check_empty_visuals(result):
    require(not result["settings"]["visual_streaming"], "visual streaming enabled")
    require(result["bootstrap_visual_bytes"] == result["coarse_triangles"] == 0, "coarse geometry loaded")
    require(not any(result["published"].values()), "a drawing was published")
    require(not result["visual_events"] and not any(result["visual"].values()), "a visual read was scheduled")
    storage = result["storage"]
    require(not any(storage["visual_cpu_buffers"].values()), "render buffers were allocated")
    require(storage["model_library_reads"] == storage["model_library_bytes"] == 0, "model bank loaded")
    require(storage["image_io"]["reads"] == storage["model_io"]["reads"] == 0, "model or image loaded")
    if result.get("at_contact"):
        require(result["at_contact"]["visual_has_same_source"] is None, "visual owner reported without a drawing")


def compare_motion(binary, pages, sparse, out, route, flags):
    common = ["--trace-body", *RAYS, *flags]
    full = replay(binary, pages, out / f"{route}-full.json", route, common)
    physics = replay(binary, sparse, out / f"{route}-physics.json", route, [*common, "--no-visuals"])
    check_empty_visuals(physics)
    require(full["bootstrap_visual_bytes"] > 0, "control did not load its coarse geometry")
    for key in ("source_sha256", "body_before_step", "physics", "physics_events", "physics_forecasts", "support",
                "first_contact", "final_foot_m", "collision_probes", "actor"):
        require(physics.get(key) == full.get(key), f"{route}: different {key}")
    require(physics["first_contact"], f"{route}: missing landing")
    return {"contact": physics["first_contact"], "samples": len(physics["body_before_step"]),
            "full_file_bytes": full["storage"]["cold_file_bytes"], "physics_file_bytes": physics["storage"]["cold_file_bytes"]}


def transfer(binary, directory, out, visuals):
    with tempfile.TemporaryDirectory(dir=out) as cache, origin(directory, conditional=True, gzip_index=True) as (url, requests):
        flags = ["--content-cache", cache, "--all-resident", "--tick", "0", *RAYS]
        if not visuals:
            flags.append("--no-visuals")
        label = "full" if visuals else "physics"
        cold = replay(binary, url, out / f"http-{label}-cold.json", "road", flags)
        n = len(requests)
        warm = replay(binary, url, out / f"http-{label}-warm.json", "road", flags)
        require([(r["name"], r["status"], r["bytes"]) for r in requests[n:]] == [("index.json", 304, 0)],
                "unexpected warm request")
        require(cold["collision_probes"] == warm["collision_probes"], "cached query changed")
        (index,) = [r for r in requests[:n] if r["name"] == "index.json"]
        require(index["content_encoding"] == "gzip", "index was sent without compression")
        require(all(r["status"] == 200 for r in requests[:n]), "cold fetch failed")
        require(all(Path(r["name"]).stem == r["sha256"] for r in requests[:n] if r["name"] != "index.json"),
                "content hash differed at the origin")
        counts = Counter(r["name"] for r in requests[:n])
        wire = sum(r["bytes"] for r in requests[:n])
        require(cold["transport"]["io"]["content_body_bytes"] + index["bytes"] == wire, "HTTP counters disagree")
        return cold, counts, {"requests": n, "index_body_bytes": index["bytes"], "total_body_bytes": wire,
                              "warm_requests": 1, "warm_body_bytes": 0}


def check_sound(binary, out):
    impact = runpy.run_path(str(HERE / "check-paged-impact.py"))
    out.mkdir(exist_ok=True)
    _, route, pages, _ = impact["fixture"](out)
    probe = ["--acoustic-ray", "7,1,10", "10,4,15"]
    full, reference = impact["paged"](binary, out, "visual", pages, route, *probe)
    headless, samples = impact["paged"](binary, out, "headless", pages, route, "--no-visuals", *probe)
    check_empty_visuals(headless)
    require(samples == reference and full["acoustic_probes"] == headless["acoustic_probes"],
            "disabling drawings changed acoustic queries or impact samples")
    audio = headless["impact_audio"]
    require(audio["read_bytes"] > 0, "the authored acoustic meshes were never read")
    return {"frames": audio["frames"], "acoustic_bytes": audio["read_bytes"]}


def time_resident(binary, pages, out):
    # Child CPU covers startup, verified file reads, decode, world construction and receipt output.
    import resource

    def children_ms():
        usage = resource.getrusage(resource.RUSAGE_CHILDREN)
        return (usage.ru_utime + usage.ru_stime) * 1000

    pairs = []
    for turn in range(8):
        observation = {}
        for visuals in ((True, False) if turn % 2 == 0 else (False, True)):
            flags = ["--all-resident", "--tick", "0", *RAYS]
            if not visuals:
                flags.append("--no-visuals")
            start = children_ms()
            replay(binary, pages, out / "timed-snapshot.json", "road", flags)
            observation["visual_and_physical" if visuals else "physical"] = children_ms() - start
        if turn:
            pairs.append(observation)
    return {"cpu_ms": pairs, "median_cpu_ms": {k: statistics.median(v[k] for v in pairs) for k in pairs[0]}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    parser.add_argument("--out", type=Path, default=ROOT / "bench/out/ville-paris/headless-collision")
    parser.add_argument("--timings", action="store_true", help="measure paired cold-process CPU with the disk cache warm")
    args = parser.parse_args()
    out, binary = args.out.resolve(), args.bin.resolve()
    out.mkdir(exist_ok=True, parents=True)
    reports = {}
    for mode in ("demand", "shared", "embedded"):
        case = out / mode
        pages = case / "pages"
        cook(HERE / "fixtures/bir-hakeim.glb", pages, 128, storage=mode)
        with tempfile.TemporaryDirectory(dir=case) as temp:
            sparse = Path(temp)
            files = physical_files(pages, sparse)
            routes = ("road", "rail", "roof") if mode == "demand" else ("road",)
            reports[mode] = {route: compare_motion(binary, pages, sparse, case, route, ["--delay-ms", "1400"])
                             for route in routes}
            if mode == "demand":
                rig = case / "articulated"
                rig.mkdir(exist_ok=True)
                reports[mode]["articulated"] = compare_motion(binary, pages, sparse, rig, "road", ["--actor", "human", "--all-resident"])
                full, _, full_io = transfer(binary, pages, case, True)
                physics, counts, physics_io = transfer(binary, sparse, case, False)
                check_empty_visuals(physics)
                require(counts == Counter(files), "HTTP did not read each distinct physical file exactly once")
                require(full["collision_probes"] == physics["collision_probes"], "all-resident queries changed")
                require(all(p["coverage"]["complete"] and p["intersections"] for p in physics["collision_probes"]),
                        "expected a complete resident hit on both segments")
                reports["http"] = {"visual_and_physical": full_io, "physical": physics_io}
    reports["authored_audio"] = check_sound(binary, out / "audio")
    if args.timings:
        reports["resident_timing"] = time_resident(binary, out / "demand/pages", out)
    (out / "summary.json").write_text(json.dumps(reports, indent=2) + "\n")
    print(json.dumps(reports["http"]))


if __name__ == "__main__":
    main()
