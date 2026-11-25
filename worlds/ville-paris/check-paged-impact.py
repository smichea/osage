#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Compare a solved Rapier landing, its paged acoustic geometry and the actual stereo recording."""

import argparse
import hashlib
import json
import runpy
import shutil
import struct
import subprocess
import tempfile
import wave
from pathlib import Path

from paging_checks import require

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
EDGE = runpy.run_path(str(HERE / "check-impact-diffraction.py"))
COOK = runpy.run_path(str(HERE / "cook-pages.py"))["cook"]
ORIGIN = runpy.run_path(str(HERE / "check-http.py"))["origin"]
SAMPLE = runpy.run_path(str(ROOT / "tools/object-convert/test_hierarchy.py"))["sample"]
DIGEST = EDGE["common"]["digest"]
SCENE_HASH = "ab5086430d01e05ab5743acf5bc375ac078eac63f36c6e91acd07611577cfcdb"
REPLAY_HASH = "b23c93d4f139f79916cb05988c6af60b811f2b2d0b5c0112b7d15e51f139d6e9"
SOURCE = [7.0, 1.0, 10.0]
OWNER = ["bridge", "upper-deck", "span/west"]
EAR = [10.0, 4.0, 15.0]
RATE = 48000
HZ = 60


def read(path):
    return json.loads(path.read_bytes())


def fixture(out):
    slab, route = SAMPLE(out)
    doc, chunk = EDGE["common"]["read_glb"](slab)
    path = out / "displaced.glb"
    walls = EDGE["aperture"](12.5)
    EDGE["write_geometry"](doc, chunk, path, walls, (0.0,) * 3, False, False)
    route_data = read(route)
    for key in route_data["routes"][0]["camera"]:
        key["eye"] = EAR
        key["aim"] = [10.0, 4.0, 14.0]
    replay = out / "displaced-route.json"
    replay.write_text(json.dumps(route_data))
    require(
        (DIGEST(path), DIGEST(replay)) == (SCENE_HASH, REPLAY_HASH),
        "the source mesh or route differs from the original displaced-window control",
    )
    pages = out / "district"
    COOK(path, pages, 128)
    # Leave the four coplanar strips of the aperture intact. Only an isolated fifth surface
    # can demonstrate that a corrupt distant page is irrelevant to the diffraction search.
    far_scene = out / "detached.glb"
    EDGE["write_geometry"](
        doc,
        chunk,
        far_scene,
        [*walls, EDGE["quad"](800, 802, 800, 802, 40)],
        (0.0,) * 3,
        False,
        False,
    )
    extra = out / "district-with-detached-surface"
    COOK(far_scene, extra, 128)
    return path, replay, pages, extra


def pcm(path, receipt):
    with wave.open(str(path), "rb") as wav:
        require(
            (wav.getframerate(), wav.getnchannels(), wav.getsampwidth())
            == (RATE, 2, 2),
            f"{path.name}: expected 48 kHz stereo 16-bit PCM",
        )
        require(
            wav.getnframes() == receipt["frames"],
            f"{path.name}: frame count in RIFF disagrees",
        )
        raw = wav.readframes(wav.getnframes())
    first = next(
        (i for i, pair in enumerate(struct.iter_unpack("<hh", raw)) if any(pair)), None
    )
    require(
        first == receipt["first_nonzero_pcm_frame"],
        f"{path.name}: measured first sample {first}",
    )
    return raw


def resident(binary, out, scene, replay, backend, *flags, label=None, actor="capsule"):
    label = label or "resident-" + backend
    wav = out / (label + ".wav")
    result = subprocess.run(
        [
            binary,
            "--scene",
            scene,
            "--replay",
            replay,
            "--route",
            "drop",
            "--physics",
            backend,
            "--actor",
            actor,
            "--listening",
            "headphones",
            "--impact-wav",
            wav,
            *flags,
        ],
        capture_output=True,
        check=True,
        timeout=45,
    )
    (out / (label + ".json")).write_bytes(result.stdout)
    (out / (label + ".log")).write_bytes(result.stderr)
    report = json.loads(result.stdout)
    return report, pcm(wav, report)


def paged(
    binary,
    out,
    label,
    pages,
    replay,
    *flags,
    plan="forecast",
    all_resident=True,
    cache=None,
):
    wav = out / (label + ".wav")
    receipt = out / (label + ".json")
    cmd = [
        binary,
        "--paging",
        "--pages",
        pages,
        "--replay",
        replay,
        "--route",
        "drop",
        "--impact-wav",
        wav,
        "--out",
        receipt,
        "--listening",
        "headphones",
        "--acoustic-plan",
        plan,
    ]
    if all_resident:
        cmd.append("--all-resident")
    if cache:
        cmd.extend(["--content-cache", cache])
    cmd.extend(map(str, flags))
    result = subprocess.run(cmd, check=False, capture_output=True, timeout=60)
    (out / (label + ".log")).write_bytes(result.stdout + result.stderr)
    require(result.returncode == 0, f"{label}: {result.stderr.decode()[-1000:]}")
    report = read(receipt)
    audio = report["impact_audio"]
    require(
        audio["plan"] == plan and audio["sample_rate_hz"] == RATE,
        "playback configuration changed",
    )
    require(
        audio["read_bytes"]
        == sum(r["bytes"] for r in audio["requests"] if r["ready_tick"] is not None)
        and audio["final_decoded_pages"] == 0,
        "published byte accounting or final residency mismatch",
    )
    return report, pcm(wav, audio)


def acoustic_files(pages):
    profile = read(pages / "index.json")["object_profile"]
    return {
        a["id"]: profile["acoustic_meshes"][a["resource"]]["file"]
        for node in profile["nodes"]
        for a in node.get("acoustic", [])
    }


def physical(report):
    first = report["first_contact"]
    require(
        (first["tick"], first["support"], first["source_node"], first["point"])
        == (39, "west-floor", 2, SOURCE),
        f"district struck a different surface: {first}",
    )
    require(
        report["at_contact"]["source_path"] == [0, 1, 2],
        "the contact lost its model owner",
    )
    audio = report["impact_audio"]
    require(len(audio["contacts"]) == 1, "expected exactly one solved landing")
    event = audio["contacts"][0]
    require(
        (event["tick"], event["point_m"], event["surface"], event["owner"])
        == (first["tick"], SOURCE, first["support"], first["source_node"]),
        "sound diverges from solved contact",
    )
    require(
        event["closing_m_s"] == -first["vertical_speed_before_m_s"],
        "sound gain uses a different speed",
    )
    return event


def paths(report, hit):
    event = physical(report)
    for component in ("direct", "edge"):
        entry = event[component]
        require(
            entry and entry["published_tick"] >= event["tick"],
            f"unpublished {component} of real landing",
        )
        onset = entry["onset"]
        for field in (
            "emission_frame",
            "point_m",
            "closing_m_s",
            "owner_path",
            "material",
            "surface",
            "part",
            "distance_m",
            "arrival_frame",
            "listener_at_arrival_m",
            "ear_delay_frames",
        ):
            require(
                onset[field] == hit[field],
                f"{component}: {field} differs from resident Rapier",
            )
        require(
            onset["direct_path"]["crossings"] == hit["direct_path"]["crossings"]
            and onset["direct_path"]["transmission_energy"]
            == hit["direct_path"]["transmission_energy"],
            f"{component}: direct geometry, material or ancestor was lost",
        )
        require(
            entry["publication_frame"] == entry["published_tick"] * RATE // HZ,
            "different audio clock",
        )
    require(
        event["direct"]["onset"]["diffracted_path"] is None,
        "direct publication already contains an edge",
    )
    require(
        event["edge"]["onset"]["diffracted_path"] == hit["diffracted_path"],
        "paged acoustic legs, three-band energy, receiver, or bend owner differs from the full model",
    )
    return event


def summarize(report):
    audio = report["impact_audio"]
    event = audio["contacts"][0] if audio["contacts"] else {}
    return {
        "first_nonzero_pcm_frame": audio["first_nonzero_pcm_frame"],
        "frames": audio["frames"],
        "requested": [r["requested_tick"] for r in audio["requests"]],
        "ready": [r["ready_tick"] for r in audio["requests"]],
        "files": [r["file"] for r in audio["requests"]],
        "bytes": audio["read_bytes"],
        "peak_pages": audio["peak_decoded_pages"],
        "final_pages": audio["final_decoded_pages"],
        "direct_published_tick": (event.get("direct") or {}).get("published_tick"),
        "edge_published_tick": (event.get("edge") or {}).get("published_tick"),
        "rejected": event.get("rejected"),
    }


def corrupt(pages, name):
    p = pages / name
    data = bytearray(p.read_bytes())
    data[-1] ^= 1
    p.write_bytes(data)
    return hashlib.sha256(data).hexdigest()


def transmitter(binary, out, scene, replay, capture):
    doc, chunk = EDGE["common"]["read_glb"](scene)
    material = doc["asset"]["extras"]["osageObject"]["acousticMaterials"]["wall"]
    material["transmission"] = [0.04, 0.0025, 0.0001]
    scene = out / "transmitting-window.glb"
    EDGE["common"]["write_glb"](scene, doc, chunk)
    district = out / "transmitting-window"
    COOK(scene, district, 128)
    mono, whole = resident(
        binary, out, scene, replay, "rapier", label="transmitter-resident"
    )
    _, direct = resident(
        binary,
        out,
        scene,
        replay,
        "rapier",
        "--direct-only",
        label="transmitter-resident-direct",
    )
    hit = mono["onsets"][0]
    require(
        all(0.0 < x < 1.0 for x in hit["direct_path"]["transmission_energy"])
        and hit["diffracted_path"],
        "transmitting control needs two audible paths",
    )
    forecast, emitted = capture(
        "transmitter-forecast", "--acoustic-delay-ms", 50, where=district
    )
    paths(forecast, hit)
    require(whole == emitted, "the complete, two-component district recording changed")
    only, visible = capture(
        "transmitter-direct-only",
        "--direct-only",
        "--acoustic-delay-ms",
        50,
        where=district,
    )
    e = physical(only)
    require(
        e["direct"]
        and e["edge"] is None
        and direct == visible
        and len(only["impact_audio"]["requests"]) == 2,
        "direct-only needs only the two files on the straight segment",
    )
    late, heard = capture(
        "transmitter-at-contact",
        "--acoustic-delay-ms",
        0,
        where=district,
        plan="contact",
    )
    e = paths(late, hit)
    start = e["direct"]["publication_frame"]
    end = e["edge"]["publication_frame"]
    a, b = 4 * start, 4 * end
    require(
        start < end and any(whole[:a]) and not any(heard[:a]),
        "empty or retroactively filled lost prefix",
    )
    require(
        any(direct[a:b]) and direct[a:b] != whole[a:b] and heard[a:b] == direct[a:b],
        "during the refinement reads only the verified direct voice may be heard",
    )
    require(
        any(whole[b:]) and whole[b:] != direct[b:] and heard[b:] == whole[b:],
        "adding the verified edge changed either path's remaining PCM history",
    )
    with tempfile.TemporaryDirectory(dir=out) as tmp:
        damaged = Path(tmp) / "district"
        shutil.copytree(district, damaged)
        name = acoustic_files(damaged)["surface-1"]
        corrupt(damaged, name)
        bad, preserved = capture(
            "transmitter-bad-edge", "--acoustic-delay-ms", 50, where=damaged
        )
        failed = physical(bad)
        require(
            name in failed["rejected"]
            and failed["edge"] is None
            and preserved == direct,
            "rejecting the damaged edge must preserve the entire verified audible direct recording",
        )
    return {
        "direct_published_frame": start,
        "edge_published_frame": end,
        "nonzero_direct_only_frames_between_publications": sum(
            any(pair) for pair in struct.iter_unpack("<hh", direct[a:b])
        ),
        "nonzero_combined_frames_after_edge": sum(
            any(pair) for pair in struct.iter_unpack("<hh", whole[b:])
        ),
        "damaged_edge_retained_exact_direct_pcm": True,
    }


def cold_budget(binary, out, pages, replay, capture):
    baseline = out / "no-audio.json"
    subprocess.run(
        [
            binary,
            "--paging",
            "--pages",
            pages,
            "--replay",
            replay,
            "--route",
            "drop",
            "--all-resident",
            "--out",
            baseline,
        ],
        check=True,
        capture_output=True,
        timeout=45,
    )
    base = read(baseline)["storage"]["cold_file_bytes"]
    audio = read(out / "zero-service.json")["storage"]["cold_file_bytes"]
    combined, _ = capture(
        "record-and-probe", "--acoustic-ray", "7,1,10", "10,4,15", plan="contact"
    )
    require(
        combined["acoustic_probe"]["mesh_reads"] == 2
        and combined["acoustic_io"]["read_bytes"] == 222
        and [h["id"] for h in combined["acoustic_probe"]["hits"]]
        == ["west-sound", "surface-0"],
        "the independent straight segment should read the slab and the first screen",
    )
    total = combined["storage"]["cold_file_bytes"]
    require(
        audio - base == 560 and total - audio == 222,
        "physical storage total omitted one acoustic reader",
    )
    return {
        "without_acoustic": base,
        "with_recording": audio,
        "with_recording_and_diagnostic_ray": total,
        "recording_bytes": audio - base,
        "additional_ray_bytes": total - audio,
    }


def run(binary, out):
    scene, replay, pages, detached = fixture(out)
    files = acoustic_files(pages)
    require(
        len(set(files.values())) == 5,
        "expected one shared slab and four screen-strip resources",
    )
    mono, reference = resident(binary, out, scene, replay, "rapier")
    (hit,) = mono["onsets"]
    edge = hit["diffracted_path"]
    require(
        (
            hit["tick"],
            hit["point_m"],
            hit["owner_path"],
            hit["emission_frame"],
            hit["direct_path"]["transmission_energy"],
            edge["arrival_frame"],
            edge["owner_path"],
        )
        == (39, SOURCE, OWNER, 31200, [0.0] * 3, 32388, ["room-screens", "surface-0"]),
        "the resident, solver-driven displaced-window reference changed",
    )
    jolt, other = resident(binary, out, scene, replay, "jolt")
    cross = [
        abs(a[0] - b[0])
        for a, b in zip(
            struct.iter_unpack("<h", reference),
            struct.iter_unpack("<h", other),
            strict=True,
        )
    ]
    require(
        jolt["onsets"][0]["point_m"] == SOURCE
        and jolt["onsets"][0]["diffracted_path"] == edge,
        "the independent Jolt landing yields different geometry",
    )
    cases = {}

    def capture(label, *flags, where=pages, **options):
        receipt, raw = paged(binary, out, label, where, replay, *flags, **options)
        cases[label] = summarize(receipt)
        return receipt, raw

    immediate, raw = capture("zero-service", plan="contact")
    paths(immediate, hit)
    require(
        raw == reference, "zero-service district differs from monolithic stereo Rapier"
    )
    require(
        set(cases["zero-service"]["files"]) == set(files.values())
        and len(cases["zero-service"]["files"]) == 5,
        "an acoustic digest was duplicated or missed",
    )

    for label, delay, force in (
        ("forecast-fast", 0, True),
        ("forecast-50ms", 50, True),
        ("forecast-normal-physics", 50, False),
    ):
        report, raw = capture(
            label, "--delay-ms", 0, "--acoustic-delay-ms", delay, all_resident=force
        )
        event = paths(report, hit)
        require(
            raw == reference and event["edge"]["published_tick"] == hit["tick"],
            f"{label}: early geometry did not preserve the complete reference recording",
        )
        require(
            min(cases[label]["requested"]) < hit["tick"],
            f"{label}: no read before the real contact",
        )
    # With no propagation latency each small transfer still occupies one service tick.
    # The strip hit by the straight ray is available before the three neighbouring strips.
    late, truncated = capture("at-contact", "--acoustic-delay-ms", 0, plan="contact")
    event = paths(late, hit)
    require(
        set(cases["at-contact"]["files"]) == set(files.values())
        and len(cases["at-contact"]["files"]) == 5,
        "deferred geometry changed the file set",
    )
    require(
        min(cases["at-contact"]["requested"]) == hit["tick"]
        and event["direct"]["published_tick"] < event["edge"]["published_tick"],
        "late control did not reach both stages",
    )
    cutoff = event["edge"]["publication_frame"]
    require(
        any(reference[: 4 * cutoff]) and not any(truncated[: 4 * cutoff]),
        f"audio before {cutoff} must already have left the device",
    )
    require(
        any(reference[4 * cutoff :])
        and truncated[4 * cutoff :] == reference[4 * cutoff :],
        f"the late edge must resume the still-audible resident PCM, with both ears aligned at {cutoff}",
    )
    cases["at-contact"]["identical_suffix_from_frame"] = cutoff
    cases["at-contact"]["matching_audible_suffix_frames"] = sum(
        any(pair) for pair in struct.iter_unpack("<hh", reference[4 * cutoff :])
    )
    expired, silent = capture(
        "at-contact-50ms", "--acoustic-delay-ms", 50, plan="contact"
    )
    expired_event = paths(expired, hit)
    require(
        expired_event["edge"]["publication_frame"] > cutoff
        and not any(silent)
        and not any(reference[4 * expired_event["edge"]["publication_frame"] :]),
        "a fully elapsed transient was restarted, or the negative control did not expire",
    )
    early, before = capture(
        "forecast-before-contact", "--tick", 34, "--acoustic-delay-ms", 50
    )
    require(
        not early["impact_audio"]["contacts"]
        and early["impact_audio"]["requests"]
        and not any(before),
        "a reservation started a voice before the solver supplied a real contact",
    )
    mixed = transmitter(binary, out, scene, replay, capture)

    with tempfile.TemporaryDirectory(dir=out) as tmp:
        temp = Path(tmp)
        extra_files = acoustic_files(detached)
        far = extra_files["surface-4"]
        for label, source, name in (
            ("bad-far", detached, far),
            ("bad-direct", pages, files["surface-0"]),
            ("bad-edge", pages, files["surface-1"]),
        ):
            branch = temp / label
            shutil.copytree(source, branch)
            changed = corrupt(branch, name)
            report, raw = capture(label, "--acoustic-delay-ms", 50, where=branch)
            event = physical(report)
            rejected = [r for r in report["impact_audio"]["requests"] if r["rejected"]]
            if label == "bad-far":
                paths(report, hit)
                require(
                    raw == reference
                    and not rejected
                    and name not in cases[label]["files"],
                    "corrupt distant surface became a dependency",
                )
            else:
                require(
                    any(
                        r["file"] == name and "bad tile content" in r["rejected"]
                        for r in rejected
                    )
                    and name in event.get("rejected", "")
                    and event["edge"] is None
                    and not any(raw),
                    f"{label}: a failed resource must be identified and cannot create an audible route",
                )
                require(
                    (event["direct"] is None) == (label == "bad-direct"),
                    f"{label}: direct publication must depend only on its own verified geometry",
                )
            cases[label]["corrupt_file"] = name
            cases[label]["corrupt_sha256"] = changed
        logs = {}
        cache = temp / "http-cache"
        with ORIGIN(pages, conditional=True) as (url, origin):
            for label in ("http-cold", "http-warm"):
                offset = len(origin)
                receipt, raw = capture(
                    label, "--acoustic-delay-ms", 50, where=url, cache=cache
                )
                paths(receipt, hit)
                require(
                    raw == reference, f"{label}: verified source changed the waveform"
                )
                records = list(origin[offset:])
                selected = [row for row in records if row["name"].endswith(".acoustic")]
                logs[label] = records
                cases[label]["origin_acoustic_GETs"] = len(selected)
                cases[label]["origin_acoustic_bytes"] = sum(
                    row["bytes"] for row in selected
                )
                if label == "http-cold":
                    require(
                        len(selected) == len(set(files.values()))
                        and {r["name"] for r in selected} == set(files.values())
                        and all(
                            row["status"] == 200
                            and row["sha256"] == Path(row["name"]).stem
                            for row in selected
                        ),
                        "cold HTTP source did not serve each verified acoustic digest once",
                    )
                else:
                    require(
                        not selected
                        and any(
                            r["name"] == "index.json" and r["status"] == 304
                            for r in records
                        ),
                        "fresh process failed to reuse verified content or revalidate its index",
                    )
        (out / "origin.json").write_text(json.dumps(logs, indent=2) + "\n")
    budget = cold_budget(binary, out, pages, replay, capture)
    return {
        "binary_sha256": DIGEST(binary),
        "transmitter_control": mixed,
        "cold_budget": budget,
        "source_sha256": DIGEST(scene),
        "route_sha256": DIGEST(replay),
        "resident": {
            "backend": "rapier",
            "contact_tick": hit["tick"],
            "position_m": SOURCE,
            "closing_m_s": hit["closing_m_s"],
            "first_pcm_frame": mono["first_nonzero_pcm_frame"],
            "edge_arrival_frame": edge["arrival_frame"],
            "edge_length_m": edge["distance_m"],
            "edge_owner": edge["owner_path"],
            "wav_sha256": DIGEST(out / "resident-rapier.wav"),
        },
        "other_backend": {
            "backend": "jolt",
            "closing_m_s": jolt["onsets"][0]["closing_m_s"],
            "differing_interleaved_samples": sum(v != 0 for v in cross),
            "peak_abs_pcm_difference": max(cross),
        },
        "cases": cases,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, default=ROOT / "target/release/ville-paris")
    parser.add_argument(
        "--out",
        type=Path,
        default=ROOT / "bench/out/ville-paris/acoustic-impact-pages/qualified",
    )
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    report = run(args.bin.resolve(), out)
    (out / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    print(
        json.dumps(
            {
                "first_frame": report["resident"]["first_pcm_frame"],
                "forecast_and_contact_edge_ticks": [
                    report["cases"][case]["edge_published_tick"]
                    for case in ("forecast-fast", "at-contact")
                ],
                "late_suffix_nonzero_frames": report["cases"]["at-contact"][
                    "matching_audible_suffix_frames"
                ],
                "http_cold_bytes": report["cases"]["http-cold"][
                    "origin_acoustic_bytes"
                ],
                "http_warm_bytes": report["cases"]["http-warm"][
                    "origin_acoustic_bytes"
                ],
            }
        )
    )


if __name__ == "__main__":
    main()
