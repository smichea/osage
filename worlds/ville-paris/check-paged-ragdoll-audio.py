#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Compare physical attack measurements, stereo samples and publication times for a falling rig."""

import argparse
import json
from pathlib import Path
import runpy
import struct
import subprocess
import tempfile

from paging_checks import require

HERE = Path(__file__).resolve().parent
COMMON = runpy.run_path(str(HERE / "check-paged-impact.py"))
RATE = COMMON["RATE"]


def first_strike(scene, route, pages, out, binary, hz, near):
    data = COMMON["read"](route)
    data["tick_hz"] = hz
    take = data["routes"][0]
    take["ticks"] = int(0.81 * hz)
    take["camera"][-1]["tick"] = take["ticks"]
    if near:
        for camera in take["camera"]:
            camera.update(eye=[7, 1.3, 10.2], aim=[7, 1.3, 9.2])
    label = f"{hz}-{'near' if near else 'window'}"
    route = out / f"{label}.json"
    route.write_text(json.dumps(data))
    flags = ["--direct-only"] if near else []
    reference, resident_pcm = COMMON["resident"](
        binary,
        out,
        scene,
        route,
        "rapier",
        *flags,
        label=f"{label}-resident",
        actor="human",
    )
    require(
        len(reference["onsets"]) == 1, f"{label}: expected one complete material attack"
    )
    original = reference["onsets"][0]
    ready_pcm = None
    rows = {}
    for mode, plan, extra in [
        ("ready", "forecast", []),
        ("prefetch", "forecast", ["--acoustic-delay-ms", "50"]),
        ("late", "contact", ["--acoustic-delay-ms", "0"]),
    ]:
        report, pcm = COMMON["paged"](
            binary,
            out,
            f"{label}-{mode}",
            pages,
            route,
            "--actor",
            "human",
            *flags,
            *extra,
            plan=plan,
        )
        audio = report["impact_audio"]
        require(len(audio["contacts"]) == 1, f"{label}: missing or duplicate reaction")
        contact = audio["contacts"][0]
        path = contact["direct" if near else "edge"]
        onset = path["onset"]
        evidence = contact["physical"]
        require(
            onset["physical"] == evidence == contact["direct"]["onset"]["physical"],
            "incomplete excitation receipt",
        )
        members = [entry["name"] for entry in evidence["members"]]
        require(
            members == ["foot_l", "foot_r"] and contact["surface"] == "west-floor",
            "incorrect reacting body",
        )
        require(
            onset["emission_frame"] == original["emission_frame"]
            and onset["owner_path"] == original["owner_path"],
            "wrong source clock or authored owner",
        )
        impulses = evidence["excitation"]["samples"]
        expected = original["physical"]["excitation"]["samples"]
        require(
            [s["offset_frames"] for s in impulses]
            == [s["offset_frames"] for s in expected],
            "attack phase changed",
        )
        require(
            all(
                abs(a["cumulative_impulse_ns"] - b["cumulative_impulse_ns"])
                < max(0.001, 0.0002 * b["cumulative_impulse_ns"])
                for a, b in zip(impulses, expected)
            ),
            "first impact load changed",
        )
        cut = path["publication_frame"]
        heard = audio["first_nonzero_pcm_frame"]
        if mode == "ready":
            ready_pcm = pcm
        errors = [
            abs(a - b)
            for a, b in zip(
                memoryview(pcm).cast("h"),
                memoryview(resident_pcm).cast("h"),
                strict=True,
            )
        ]
        peak_error = max(errors)
        if mode != "late":
            require(
                pcm == ready_pcm and peak_error == 0,
                f"{label}/{mode}: first strike PCM changed",
            )
        else:
            require(
                not any(pcm[: 4 * cut]) and pcm[4 * cut :] == ready_pcm[4 * cut :],
                "late path restarted or rewrote the prefix",
            )
            require(any(pcm[4 * cut :]), "late control missed the complete tail")
        if near and hz == 60 and mode != "late":
            require(
                onset["emission_frame"] <= heard < contact["tick"] * RATE // hz,
                "nearby ears lost the impact within its originating physics tick",
            )
        if hz == 90:
            require(
                len(impulses) == 2
                and onset["emission_frame"] + impulses[1]["offset_frames"]
                > contact["tick"] * RATE // hz,
                "missing cross-tick force measurement",
            )
        rows[mode] = {
            "emission_frame": onset["emission_frame"],
            "members": members,
            "attack_offsets": [s["offset_frames"] for s in impulses],
            "positive_impulse_ns": impulses[-1]["cumulative_impulse_ns"],
            "publication_frame": cut,
            "first_nonzero_pcm_frame": heard,
            "file_bytes": audio["read_bytes"],
            "matches_resident": pcm == resident_pcm,
            "peak_abs_pcm_error": peak_error,
            "different_channels_samples": sum(bool(n) for n in errors),
            "remaining_nonzero_frames": sum(
                any(pair) for pair in struct.iter_unpack("<hh", pcm[4 * cut :])
            ),
        }
    return rows


def resolved_reactions(events, surfaces):
    rows = []
    for event in events:
        evidence = event["physical"]
        for sample in evidence["measurements"]:
            attribution = sample["attribution"]
            bodies = (
                [attribution["pair"]]
                if "pair" in attribution
                else attribution["unresolved"]
            )
            for body in bodies:
                name = surfaces[body["surface"]]
                require(
                    name == event["surface"],
                    "impulse was attributed to a different authored surface",
                )
                body["surface"] = name
        rows.append((event["tick"], event["surface"], evidence))
    return rows


def run(binary, out):
    out.mkdir(parents=True, exist_ok=True)
    scene, route, pages, _ = COMMON["fixture"](out)
    rows = {
        f"{hz}-{'near' if near else 'window'}": first_strike(
            scene, route, pages, out, binary, hz, near
        )
        for hz in [60, 90]
        for near in [False, True]
    }
    complete = {}
    resident, reference_wave = COMMON["resident"](
        binary, out, scene, route, "rapier", actor="human", label="all-resident-rig"
    )
    bench = out / "resident-dynamics"
    subprocess.run(
        [
            binary,
            "--scene",
            scene,
            "--replay",
            route,
            "--physics",
            "rapier",
            "--actor",
            "human",
            "--bench",
            bench,
        ],
        check=True,
        capture_output=True,
        timeout=45,
    )
    surfaces = COMMON["read"](bench / "dynamics.json")["run"]["surfaces"]
    reactions = resolved_reactions(resident["onsets"], surfaces)
    for name, options in [("ready", []), ("forecast", ["--acoustic-delay-ms", "50"])]:
        report, samples = COMMON["paged"](
            binary,
            out,
            f"all-{name}",
            pages,
            route,
            "--actor",
            "human",
            "--trace-body",
            *options,
        )
        audio = report["impact_audio"]
        surfaces = [s["id"] for s in report["contact_surfaces"]]
        current = resolved_reactions(audio["contacts"], surfaces)
        require(
            reactions == current and len(current) == 12 and any(samples),
            "acoustic reads changed a solved reaction",
        )
        require(
            reference_wave == samples,
            "prefetched audio differs after subsequent limb strikes",
        )
        require(
            audio["read_bytes"] == 560 and len(audio["requests"]) == 5,
            "reread an active acoustic mesh",
        )
        complete[name] = {
            "events": len(current),
            "ready_bytes": audio["read_bytes"],
            "first_nonzero_pcm_frame": audio["first_nonzero_pcm_frame"],
        }
    report, samples = COMMON["paged"](
        binary, out, "airborne", pages, route, "--actor", "human", "--tick", "20"
    )
    require(
        not any(samples) and not report["impact_audio"]["contacts"],
        "forecast emitted airborne sound",
    )
    receipt = {
        "binary_sha256": COMMON["DIGEST"](binary),
        "single_impact": rows,
        "full_fall": complete,
    }
    (out / "summary.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps(receipt["full_fall"]))


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
        with tempfile.TemporaryDirectory() as temp:
            run(args.bin.resolve(), Path(temp))
