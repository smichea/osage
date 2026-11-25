#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Compare the complete OVC1 and OVC2 district parents and optionally time the actual native decoders."""

import argparse
import hashlib
import json
import os
import platform
import statistics
import subprocess
import tempfile
import zlib
from collections import Counter
from pathlib import Path

from paging_checks import glb
from paging_coarse import unpack_coarse

TEST = "paging::coarse::verification::measure_native_decoders"


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def audit(before, after):
    directories, parents, decoded, paths = [], [], [], []
    for folder, signature in ((before, b"OVC1"), (after, b"OVC2")):
        directory = json.loads((folder / "index.json").read_bytes())
        asset = directory.pop("parent")
        path = folder / asset["file"]
        content = path.read_bytes()
        if (len(content), sha(content), content[:4]) != (
            asset["bytes"],
            asset["sha256"],
            signature,
        ):
            raise ValueError(
                "parent resource does not match its directory entry or expected format"
            )
        directories.append(directory)
        parents.append(asset)
        paths.append(path)
        decoded.append(unpack_coarse(content))
    if directories[0] != directories[1] or decoded[0] != decoded[1]:
        raise ValueError("district inputs or reconstructed GLB bytes differ")
    raw = decoded[0]
    doc, chunk = glb(raw)
    attributes = Counter()
    for mesh in doc.get("meshes", []):
        for primitive in mesh["primitives"]:
            attributes.update(primitive["attributes"].keys())
    return {
        "source_sha256": directories[0]["source_sha256"],
        "parents": dict(zip(("ovc1", "ovc2"), parents, strict=True)),
        "decoded_glb_sha256": sha(raw),
        "decoded_glb_bytes": len(raw),
        "metadata_bytes_with_padding": len(raw) - 20 - len(chunk),
        "binary_bytes": len(chunk) - 8,
        "nodes": len(doc.get("nodes", [])),
        "meshes": len(doc.get("meshes", [])),
        "materials": len(doc.get("materials", [])),
        "primitive_attributes": dict(attributes),
        "images": len(doc.get("images", [])),
        "identical_nonparent_directory": True,
        "zlib_glb_with_old_header_bytes": {
            level: 8 + len(zlib.compress(raw, level)) for level in (1, 6, 7, 8, 9)
        },
        "zlib9_of_existing_envelope_bytes": len(
            zlib.compress(paths[0].read_bytes(), 9)
        ),
    }, paths


def summary(samples):
    order = sorted(samples)
    return {
        "count": len(order),
        "min_ms": order[0] / 1e6,
        "median_ms": statistics.median(order) / 1e6,
        "p95_ms": order[(95 * len(order) + 99) // 100 - 1] / 1e6,
        "max_ms": order[-1] / 1e6,
    }


def measure(exe, paths, expected, rounds, cpu):
    env = {
        **os.environ,
        "OSAGE_COARSE_OLD": str(paths[0].resolve()),
        "OSAGE_COARSE_NEW": str(paths[1].resolve()),
    }

    def run(mode, repeats):
        env.update(OSAGE_COARSE_MODE=mode, OSAGE_COARSE_ROUNDS=str(repeats))
        raw = subprocess.check_output(
            [
                "taskset",
                "-c",
                str(cpu),
                exe.resolve(),
                "--exact",
                TEST,
                "--ignored",
                "--nocapture",
            ],
            env=env,
            text=True,
        )
        line = next(
            row for row in raw.splitlines() if "OSAGE_COARSE_MEASUREMENT " in row
        )
        receipt = json.loads(line.partition("OSAGE_COARSE_MEASUREMENT ")[2])
        if receipt["decoded_sha256"] != expected:
            raise ValueError("native decoder fingerprint differs")
        return receipt

    paired = run("paired", rounds)
    fresh = {method: [] for method in ("ovc1", "ovc2")}
    for turn in range(11):
        order = ("ovc1", "ovc2") if turn % 2 == 0 else ("ovc2", "ovc1")
        for method in order:
            fresh[method].append(run(method, 1))
    cols = {}
    for method, trials in fresh.items():
        values = [v for v in paired["samples"] if v["method"] == method]
        cols[method] = {
            "thread_cpu_warm": summary([v["cpu_ns"] for v in values[1:]]),
            "wall_warm": summary([v["wall_ns"] for v in values[1:]]),
            "first_paired_thread_cpu_ms": values[0]["cpu_ns"] / 1e6,
            "fresh_thread_cpu": summary([t["samples"][0]["cpu_ns"] for t in trials]),
            "fresh_wall": summary([t["samples"][0]["wall_ns"] for t in trials]),
            "median_fresh_peak_rss_kib": statistics.median(
                t["peak_rss_kib"] for t in trials
            ),
            "median_fresh_peak_increment_kib": statistics.median(
                t["peak_rss_kib"] - t["baseline_peak_rss_kib"] for t in trials
            ),
        }
    return {
        "binary_sha256": sha(exe.read_bytes()),
        "platform": platform.platform(),
        "cpu": cpu,
        "rounds_in_paired_process": rounds,
        "independent_fresh_runs": 11,
        "summary": cols,
        "raw": {"paired": paired, "fresh": fresh},
    }


def reject_invalid(directory, binary):
    """Exercise the ordinary reader after the corrupt body has received a valid new content digest."""
    directory = directory.resolve()
    index = json.loads((directory / "index.json").read_bytes())
    content = (directory / index["parent"]["file"]).read_bytes()
    corrupted = {
        "incomplete_stream": (content[:-1], "coarse zlib"),
        "forged_json_length": (
            content[:8] + (2**32 - 1).to_bytes(4, "little") + content[12:],
            "coarse count",
        ),
    }
    results = {}
    with tempfile.TemporaryDirectory() as tmp:
        for name, (raw, expected) in corrupted.items():
            target = Path(tmp) / name
            target.mkdir()
            for source in directory.iterdir():
                if source.name != "index.json":
                    (target / source.name).symlink_to(source)
            digest = sha(raw)
            resource = {"file": digest + ".coarse", "sha256": digest, "bytes": len(raw)}
            (target / resource["file"]).write_bytes(raw)
            local = dict(index, parent=resource)
            (target / "index.json").write_text(json.dumps(local))
            result = subprocess.run(
                [
                    binary.resolve(),
                    "--paging",
                    "--pages",
                    target,
                    "--route",
                    "road",
                    "--tick",
                    "0",
                    "--out",
                    target / "unexpected.json",
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            if (
                result.returncode == 0
                or expected not in result.stderr
                or (target / "unexpected.json").exists()
            ):
                raise ValueError(
                    f"{name}: unexpected admission or diagnostic {result.stderr}"
                )
            results[name] = {
                "bytes": len(raw),
                "sha256": digest,
                "exit": result.returncode,
                "diagnostic": result.stderr.strip(),
            }
    return results


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--ovc1", required=True, type=Path)
    ap.add_argument("--ovc2", required=True, type=Path)
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument(
        "--test-bin",
        type=Path,
        help="release Ville-Paris test binary, for the optional CPU/RSS measurement",
    )
    ap.add_argument(
        "--reader",
        type=Path,
        help="normal release executable for digest-consistent corruption controls",
    )
    ap.add_argument("--rounds", type=int, default=51)
    ap.add_argument("--cpu", type=int, default=min(os.sched_getaffinity(0)))
    args = ap.parse_args()
    report, paths = audit(args.ovc1, args.ovc2)
    if args.reader:
        report["rejected"] = reject_invalid(args.ovc2, args.reader)
    if args.test_bin:
        if args.rounds < 3 or args.cpu not in os.sched_getaffinity(0):
            ap.error("expected an available CPU and at least three rounds")
        report["native"] = measure(
            args.test_bin, paths, report["decoded_glb_sha256"], args.rounds, args.cpu
        )
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2) + "\n")
    print(
        f"{report['parents']['ovc1']['bytes']} → {report['parents']['ovc2']['bytes']} stored bytes, {report['decoded_glb_bytes']} identical decoded bytes"
    )


if __name__ == "__main__":
    main()
