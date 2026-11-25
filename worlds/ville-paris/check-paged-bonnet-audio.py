#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Trace a moving contact to the listener through a panel above the static district."""

import argparse
import hashlib
import json
from pathlib import Path
import runpy

from paging_checks import require

HERE = Path(__file__).resolve().parent
AUDIO = runpy.run_path(str(HERE / "check-paged-impact.py"))


def prepare(out):
    source, replay = AUDIO["SAMPLE"](out)
    edge = AUDIO["EDGE"]
    tree, chunk = edge["common"]["read_glb"](source)
    scene = out / "screen.glb"
    edge["write_geometry"](
        tree, chunk, scene, [edge["quad"](6.0, 8.8, 9.0, 11.0, 9.45)],
        [0.35, 0.65, 0.8], False, False,
    )
    pages = out / "district"
    AUDIO["COOK"](scene, pages, 32)
    return scene, pages, AUDIO["read"](replay)


def route(out, base, label, velocity, ticks=36, height=11.035):
    replay = dict(base)
    fixture = AUDIO["read"](HERE / "fixtures" / "bonnet.json")["routes"][0]
    camera = {"eye": [10, 10.7, 10], "aim": [7, 9, 10]}
    take = dict(
        fixture, id="drop", ticks=ticks, start_m=[7, height, 10],
        velocity_m_s=[0.5, -4, 0],
        bonnet=dict(fixture["bonnet"], center_m=[7, 9, 10], velocity_m_s=velocity),
        camera=[dict(camera, tick=i) for i in [0, ticks]],
    )
    replay["routes"] = [take]
    path = out / (label + "-route.json")
    path.write_text(json.dumps(replay))
    return path


def run(binary, out):
    out.mkdir(parents=True, exist_ok=True)
    scene, pages, base = prepare(out)
    results = {}
    for label, velocity in [("level", [1, 0, 0]), ("rising", [1, 1, 0])]:
        replay = route(out, base, label, velocity)
        for actor in ["capsule", "human"]:
            name = f"{label}-{actor}"
            ref, reference = AUDIO["resident"](
                binary, out, scene, replay, "rapier", "--direct-only", label=name + "-resident", actor=actor,
            )
            expected = ref["onsets"][0]
            trials = {}
            for plan in ["forecast", "contact"]:
                report, pcm = AUDIO["paged"](
                    binary, out, f"{name}-{plan}", pages, replay,
                    "--actor", actor, "--direct-only", "--acoustic-delay-ms", "50",
                    plan=plan, all_resident=False,
                )
                audio = report["impact_audio"]
                first = audio["contacts"][0]
                direct = first["direct"]
                onset = direct["onset"]
                at = onset["emission_frame"]
                release = direct["publication_frame"]
                require(first["surface"] == "bonnet" and onset["material"] == "steel-plate", name)
                require(
                    (at, onset["basis"], onset["closing_m_s"]) ==
                    (expected["emission_frame"], expected["basis"], expected["closing_m_s"]),
                    f"{name}: moving contact speed or timing",
                )
                crossings = onset["direct_path"]["crossings"]
                require(any(c["id"] == "surface-0" for c in crossings), f"{name}: missing acoustic screen")
                require(any(reference) and any(pcm), f"{name}: silent recording")
                if plan == "forecast":
                    require(release == at and pcm == reference, f"{name}: prefetched recording differs")
                else:
                    require(release > at and not any(pcm[: 4 * release]), f"{name}: invalid late prefix")
                    # A second impact can later request this same page again. Compare through its emission.
                    end = min((s["emission_frame"] for s in ref["onsets"][1:]), default=len(reference) // 4)
                    require(end > release and pcm[4 * release:4 * end] == reference[4 * release:4 * end],
                            f"{name}: late attack restarted")
                trials[plan] = {
                    "emission_frame": at, "publication_frame": release, "basis": onset["basis"],
                    "closing_m_s": onset["closing_m_s"], "first_nonzero_frame": audio["first_nonzero_pcm_frame"],
                    "requests": audio["requests"], "pcm_sha256": hashlib.sha256(pcm).hexdigest(),
                }
            results[name] = trials
    # The split occurs within three microseconds of the end of tick 24.
    # The solver can apply the impulse before any native geometric manifold is available.
    boundary = route(out, base, "boundary", [1, 0, 0], height=11)
    ref, reference = AUDIO["resident"](
        binary, out, scene, boundary, "rapier", "--direct-only", label="boundary-resident",
    )
    report, pcm = AUDIO["paged"](
        binary, out, "boundary-forecast", pages, boundary, "--direct-only",
        "--acoustic-delay-ms", "50", all_resident=False,
    )
    require(len(ref["onsets"]) == len(report["impact_audio"]["contacts"]) == 1, "lost or repeated boundary impulse")
    hit = report["impact_audio"]["contacts"][0]["direct"]["onset"]
    require(hit["basis"] == "applied impulse" and hit["tick"] == 24, "boundary sound lacks the applied reaction")
    require(hit["emission_frame"] == ref["onsets"][0]["emission_frame"] and any(pcm) and pcm == reference,
            "boundary PCM differs")
    results["tick-boundary"] = {"emission_frame": hit["emission_frame"], "basis": hit["basis"],
                                "pcm_sha256": hashlib.sha256(pcm).hexdigest()}
    miss = route(out, base, "escape", [24, 0, 0])
    for actor in ["capsule", "human"]:
        report, pcm = AUDIO["paged"](
            binary, out, "escape-" + actor, pages, miss, "--actor", actor, "--direct-only",
            "--acoustic-delay-ms", "50", all_resident=False,
        )
        require(not report["impact_audio"]["contacts"] and not any(pcm), "a missed bonnet emitted a sound")
        require(not report["impact_audio"]["requests"], "a missed bonnet reserved the isolated screen")
    (out / "summary.json").write_text(json.dumps(results, indent=2) + "\n")
    print("moving bonnet: 5 resident/forecast stereo matches, 4 delayed controls, 2 silent misses")


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--bin", required=True, type=Path)
    ap.add_argument("--out", required=True, type=Path)
    args = ap.parse_args()
    run(args.bin.resolve(), args.out.resolve())
