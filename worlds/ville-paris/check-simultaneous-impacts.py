#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Drop opposite feet onto two materials behind separate acoustic panels and compare the resulting stereo signal."""

import argparse
import hashlib
import json
from pathlib import Path
import runpy
import shutil
import tempfile

from paging_checks import require

HERE = Path(__file__).resolve().parent
AUDIO = runpy.run_path(str(HERE / "check-paged-impact.py"))
SPLIT = runpy.run_path(str(HERE / "check-ragdoll-contact-order.py"))
RATE = AUDIO["RATE"]


def fixture(out):
    scene, _, _, replay = SPLIT["RIG"]["prepare"](out / "source")
    split, _ = SPLIT["district"](out, scene)
    edge = AUDIO["EDGE"]
    tree, chunk = edge["common"]["read_glb"](split)
    materials = tree["asset"]["extras"]["osageObject"]["collisionMaterials"]
    for node, material in [(2, "steel-plate"), (3, "asphalt")]:
        tree["nodes"][node]["extras"]["osage"]["collision"][0]["material"] = material
        materials[material] = materials["surface-7"]
    faces = [
        edge["quad"](15.65, 15.98, 9.5, 10.8, 1.12),
        edge["quad"](16.02, 16.4, 9.3, 10.6, 1.12),
    ]
    source = out / "materials.glb"
    edge["write_geometry"](tree, chunk, source, faces, [0.4, 0.6, 0.8], False, False)
    pages = out / "district"
    AUDIO["COOK"](source, pages, 32)
    take = {
        "id": "drop",
        "ticks": 49,
        "start_m": [16, 3, 10],
        "velocity_m_s": [0, 0, 0],
        "expected_support": "plus-floor",
        "camera": [
            {"tick": t, "eye": [16, 1.78, 10.06], "aim": [16, 1.78, 9.06]}
            for t in [0, 49]
        ],
    }
    route = out / "two-feet.json"
    route.write_text(json.dumps(dict(replay, tick_hz=60, routes=[take])))
    return source, pages, route


def acoustic_path(path):
    """A district can visit fewer triangles while returning the same crossings and transfer."""
    if isinstance(path, dict):
        return {
            k: acoustic_path(v) for k, v in path.items() if k != "candidates_tested"
        }
    if isinstance(path, list):
        return [acoustic_path(v) for v in path]
    return path


def delivered_order(report, order):
    if order == "all":
        return
    pages = [
        (r["tile"], r["tick"])
        for r in report["physics_events"]
        if r["state"] == "ready"
    ]
    tile = [1, 0] if order.endswith("plus-first") else [0, 0]
    require(
        len(pages) == 2 and pages[0][0] == tile and pages[0][1] < pages[1][1],
        f"{order}: physical delivery order",
    )
    require(
        (pages[1][1] > 39) == order.startswith("handoff"),
        f"{order}: wrong physical promotion time",
    )


def independent_panels(binary, out, scene, pages, replay, panels):
    """A damaged panel can silence its own source while the opposite foot remains audible."""
    results = {}
    for panel, floor in [("surface-0", "minus-floor"), ("surface-1", "plus-floor")]:
        # Keep both incoming impulses, material seeds and the valid ear signal in the reference.
        tree, chunk = AUDIO["EDGE"]["common"]["read_glb"](scene)
        materials = tree["asset"]["extras"]["osageObject"]["acousticMaterials"]
        materials["silent"] = dict(materials["wall"], transmission=[0, 0, 0])
        for node in tree["nodes"]:
            for binding in node.get("extras", {}).get("osage", {}).get("acoustic", []):
                if binding["id"] == panel:
                    binding["material"] = "silent"
        muted = out / f"mute-{panel}.glb"
        AUDIO["EDGE"]["common"]["write_glb"](muted, tree, chunk)
        resident, reference = AUDIO["resident"](
            binary,
            out,
            muted,
            replay,
            "rapier",
            "--direct-only",
            actor="human",
            label=f"mute-{panel}",
        )
        require(
            len(resident["onsets"]) == 2 and any(reference),
            "missing reference for the surviving foot",
        )
        with tempfile.TemporaryDirectory(dir=out) as temp:
            damaged = Path(temp) / "damaged"
            shutil.copytree(pages, damaged)
            AUDIO["corrupt"](damaged, panels[panel])
            report, pcm = AUDIO["paged"](
                binary,
                out,
                f"damaged-{panel}",
                damaged,
                replay,
                "--actor",
                "human",
                "--direct-only",
                "--acoustic-delay-ms",
                "50",
            )
        failed = [e for e in report["impact_audio"]["contacts"] if e.get("rejected")]
        require(
            len(failed) == 1
            and failed[0]["surface"] == floor
            and failed[0]["direct"] is None,
            "a local failure reached the wrong foot",
        )
        require(
            reference[: len(pcm)] == pcm and not any(reference[len(pcm) :]),
            "rejection changed the surviving material voice or omitted an audible tail",
        )
        results[panel] = {
            "rejected_surface": floor,
            "silent_reference_tail_frames": (len(reference) - len(pcm)) // 4,
            "surviving_pcm_sha256": hashlib.sha256(pcm).hexdigest(),
        }
    return results


def http_delivery(binary, out, pages, replay, panels, reference_digest):
    records = {}
    with (
        tempfile.TemporaryDirectory(dir=out) as temp,
        AUDIO["ORIGIN"](pages, conditional=True) as (url, events),
    ):
        cache = Path(temp) / "cache"
        for case in ["cold", "warm"]:
            start = len(events)
            report, pcm = AUDIO["paged"](
                binary,
                out,
                f"http-{case}",
                url,
                replay,
                "--actor",
                "human",
                "--direct-only",
                "--acoustic-delay-ms",
                "50",
                cache=cache,
            )
            require(
                hashlib.sha256(pcm).hexdigest() == reference_digest,
                "HTTP changed a simultaneous impact",
            )
            fetched = [
                r["name"] for r in events[start:] if r["name"] in panels.values()
            ]
            require(
                sorted(fetched) == (sorted(panels.values()) if case == "cold" else []),
                "duplicate, missing or uncached panel",
            )
            require(
                all(
                    h["direct"]["publication_frame"] == 30800
                    for h in report["impact_audio"]["contacts"]
                ),
                "HTTP panel arrived after the source",
            )
            records[case] = {"panel_requests": fetched, "responses": events[start:]}
    return records


def run(binary, out):
    out.mkdir(parents=True, exist_ok=True)
    scene, pages, replay = fixture(out)
    index = AUDIO["read"](pages / "index.json")["object_profile"]
    panels = {
        surface["id"]: index["acoustic_meshes"][surface["resource"]]["file"]
        for node in index["nodes"]
        for surface in node.get("acoustic", [])
        if surface["id"].startswith("surface-")
    }
    require(
        len(set(panels.values())) == 2,
        "a single decoded mesh would hide the missing second reservation",
    )
    rows = {}
    for direct_only in (True, False):
        flags = ["--direct-only"] if direct_only else []
        playback = "direct" if direct_only else "both"
        resident, reference = AUDIO["resident"](
            binary,
            out,
            scene,
            replay,
            "rapier",
            *flags,
            actor="human",
            label=f"{playback}-resident",
        )
        reference_events = resident["onsets"]
        expected = [
            ("minus-floor", "asphalt", "foot_r", "surface-0"),
            ("plus-floor", "steel-plate", "foot_l", "surface-1"),
        ]
        observed = [
            (
                h["surface"],
                h["material"],
                h["physical"]["members"][0]["name"],
                h["direct_path"]["crossings"][0]["id"],
            )
            for h in reference_events
        ]
        require(observed == expected, f"contact or material identity: {observed}")
        require(
            {e["emission_frame"] for e in reference_events} == {30800},
            "feet did not strike at the same substep",
        )
        for case, order, delay, plan in [
            ("all-ready", "all", None, "forecast"),
            ("all-50", "all", 50, "forecast"),
            ("plus-first", "plus-first", 50, "forecast"),
            ("minus-first", "minus-first", 50, "forecast"),
            ("handoff-plus-first", "handoff-plus-first", 50, "forecast"),
            ("handoff-minus-first", "handoff-minus-first", 50, "forecast"),
            ("cold", "all", 50, "contact"),
        ]:
            label = f"{playback}-{case}"
            config = list(flags)
            if order != "all":
                config.extend(
                    [
                        "--delay-ms",
                        "0",
                        "--mbps-per-kind",
                        "1000",
                        "--hold",
                        "physics",
                        "--watch",
                        "0,0" if order.endswith("plus-first") else "1,0",
                        "--hold-ticks",
                        "46" if order.startswith("handoff") else "12",
                        "--late-collision",
                        "proxy" if order.startswith("handoff") else "unprotected",
                    ]
                )
            if delay is not None:
                config.extend(["--acoustic-delay-ms", str(delay)])
            report, pcm = AUDIO["paged"](
                binary,
                out,
                label,
                pages,
                replay,
                "--actor",
                "human",
                *config,
                plan=plan,
                all_resident=order == "all",
            )
            delivered_order(report, order)
            audio = report["impact_audio"]
            events = audio["contacts"]
            require(len(events) == 2, f"{label}: duplicate or missing material voice")
            for event, source in zip(events, reference_events, strict=True):
                direct = event["direct"]["onset"]
                for field in (
                    "point_m",
                    "surface",
                    "owner_path",
                    "material",
                    "emission_frame",
                    "arrival_frame",
                    "part",
                    "direct_path",
                ):
                    require(
                        acoustic_path(direct[field]) == acoustic_path(source[field]),
                        f"{label}: {field} differs for {event['surface']}",
                    )
                require(
                    event["physical"]["excitation"] == source["physical"]["excitation"],
                    f"{label}: excitation differs",
                )
                if not direct_only:
                    edge = (event["edge"] or {}).get("onset", {}).get("diffracted_path")
                    require(
                        acoustic_path(edge) == acoustic_path(source["diffracted_path"]),
                        f"{label}: differing indirect path",
                    )
            panel_reads = {
                r["file"]: r for r in audio["requests"] if r["file"] in panels.values()
            }
            require(len(panel_reads) == 2, f"{label}: missing acoustic panel")
            if plan == "forecast":
                require(
                    all(r["ready_tick"] < 39 for r in panel_reads.values()),
                    f"{label}: panel published after contact",
                )
                require(
                    pcm == reference, f"{label}: stereo output changed with file order"
                )
            else:
                require(
                    all(r["requested_tick"] >= 39 for r in panel_reads.values()),
                    "late control read early",
                )
                releases = [
                    pub["publication_frame"]
                    for e in events
                    for k in ("direct", "edge")
                    if (pub := e[k])
                ]
                first, last = min(releases), max(releases)
                require(
                    not any(pcm[: 4 * first])
                    and pcm[4 * last :] == reference[4 * last :]
                    and any(pcm[4 * last :]),
                    f"{label}: late wave prefix or tail",
                )
            rows[label] = {
                "events": [
                    {
                        "surface": h["surface"],
                        "part": h["physical"]["members"][0]["name"],
                        "published": h["direct"]["publication_frame"],
                    }
                    for h in events
                ],
                "panel_ready_ticks": {
                    name: panel_reads[file]["ready_tick"]
                    for name, file in panels.items()
                },
                "first_nonzero_pcm_frame": audio["first_nonzero_pcm_frame"],
                "frames": audio["frames"],
                "identical_stereo": pcm == reference,
                "pcm_sha256": hashlib.sha256(pcm).hexdigest(),
            }
    early, pcm = AUDIO["paged"](
        binary,
        out,
        "airborne",
        pages,
        replay,
        "--actor",
        "human",
        "--tick",
        "18",
        "--direct-only",
        "--acoustic-delay-ms",
        "50",
    )
    require(
        not any(pcm) and not early["impact_audio"]["contacts"],
        "an acoustic forecast started a voice",
    )
    panel_failures = independent_panels(binary, out, scene, pages, replay, panels)
    http = http_delivery(
        binary, out, pages, replay, panels, rows["direct-all-ready"]["pcm_sha256"]
    )
    receipt = {
        "individual_rejections": panel_failures,
        "http": http,
        "binary_sha256": AUDIO["DIGEST"](binary),
        "source_sha256": AUDIO["DIGEST"](scene),
        "cases": rows,
    }
    (out / "summary.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(
        f"{len(rows)} two-surface recordings, {sum(r['identical_stereo'] for r in rows.values())} match the full resident recording"
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--bin", type=Path, default=HERE.parents[1] / "target/release/ville-paris"
    )
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    if args.out:
        run(args.bin.resolve(), args.out.resolve())
    else:
        with tempfile.TemporaryDirectory() as folder:
            run(args.bin.resolve(), Path(folder))
