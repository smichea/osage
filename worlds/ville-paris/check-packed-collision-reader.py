#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Replay equivalent JSON and OCP1 districts, including failed reads and historic directory versions."""

import argparse
import hashlib
import json
import runpy
import struct
import subprocess
import tempfile
import zlib
from pathlib import Path

from paging_support_storage import unpack_support
from paging_checks import require

HERE = Path(__file__).resolve().parent
CORNER = runpy.run_path(str(HERE / "check-corner-support.py"))
BENCH = CORNER["BENCH"]
RIG = BENCH["RIG"]
POSE_HASH = BENCH["ORDER"]["pose_hash"]
FIXTURE = HERE / "fixtures" / "bonnet.json"
SOURCE = HERE / "fixtures" / "bir-hakeim.glb"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def read(path):
    return json.loads(path.read_bytes())


def state(trace):
    rows = trace["body_before_step"]
    table = trace["contact_surfaces"]
    witness = hashlib.sha256()
    contact_count = 0
    for row in rows:
        # Numeric surface handles reflect insertion order. Bind each witness back to its owner.
        contacts = []
        for entry in row["contacts_after_step"]:
            owner = table[entry["surface"]]
            event = dict(entry, surface=[owner["source_node"], owner["id"]])
            contacts.append(json.dumps(event, sort_keys=True, separators=(",", ":")))
        witness.update(
            json.dumps(
                [row["tick"], row.get("swept_impact_offset_s"), sorted(contacts)]
            ).encode()
        )
        contact_count += len(contacts)
    return {
        "states": len(rows),
        "actor_sha256": POSE_HASH([row["parts_after_step"] for row in rows]),
        "box_sha256": POSE_HASH([row["boxes_after_step"] for row in rows]),
        "ordered_ticks_contact_multisets_sha256": witness.hexdigest(),
        "contact_points": contact_count,
    }


def geometry(index, extension):
    slots = {}
    for tile in index["tiles"]:
        if (layer := tile.get("physics")) is not None:
            require(
                layer["file"].endswith("." + extension), "unexpected collision encoding"
            )
            slots[tile["x"], tile["z"]] = {
                k: v for k, v in layer.items() if k not in ("file", "bytes", "sha256")
            }
    require(slots, "empty physics directory")
    return slots


def paired(binary, old, packed, out):
    index_old, index_new = read(old / "index.json"), read(packed / "index.json")
    require(
        index_old["source_sha256"] == index_new["source_sha256"] == RIG["sha"](SOURCE),
        "different city sources",
    )
    require(
        geometry(index_old, "json") == geometry(index_new, "cpage"),
        "physics directory metadata changed",
    )
    for label, pages in [("json", old), ("packed", packed)]:
        CORNER["run"](binary, out / label, pages)
    reports = {}
    for name in ("capsule", "human", "imported"):
        for route in ("parked", "moving"):
            for mode in ("all", "streamed", "held"):
                label = f"{name}-{route}-{mode}"
                traces = [
                    read(out / variant / (label + ".json"))
                    for variant in ("json", "packed")
                ]
                reference, result = (state(trace) for trace in traces)
                require(reference == result, f"{label}: contact or state mismatch")
                require(
                    traces[0]["first_contact"] == traces[1]["first_contact"],
                    f"{label}: first contact mismatch",
                )
                traffic = {
                    v: t["physics"]["read_bytes"]
                    for v, t in zip(("json", "packed"), traces, strict=True)
                }
                require(
                    traffic["packed"] < traffic["json"], f"{label}: no read reduction"
                )
                reports[label] = dict(
                    result,
                    physics_read_bytes=traffic,
                    first_contact=traces[1]["first_contact"],
                )
    a, b = (read(out / enc / "summary.json")["sound"] for enc in ("json", "packed"))
    require(a == b and len(b) == 4, "stereo PCM mismatch")
    return reports, b


def duplicate_resources(source, destination):
    for file in source.iterdir():
        if file.name != "index.json" and file.is_file():
            (destination / file.name).symlink_to(file.resolve())


def corrupt(source, destination, authenticated):
    duplicate_resources(source, destination)
    index = read(source / "index.json")
    layer = next(t["physics"] for t in index["tiles"] if (t["x"], t["z"]) == (0, 0))
    data = bytearray((source / layer["file"]).read_bytes())
    require(data[:4] == b"OCP1" and len(data) > 8, "missing OCP1 source")
    if authenticated:
        # The city page holds the road followed by the rail. Leave the first record
        # intact to exercise transactionality after one complete collider was decoded.
        sizes = {1: 96, 2: 40, 3: 24}
        require(
            struct.unpack_from("<I", data, 4)[0] >= 2 and data[8] & 0x7F in sizes,
            "expected two city objects",
        )
        data[9 + sizes[data[8] & 0x7F]] = 0x80

        sha = digest(data)
        layer.update(file=sha + ".cpage", sha256=sha, bytes=len(data))
        (destination / layer["file"]).write_bytes(data)
        binding = hashlib.sha256()
        for tile in index["tiles"]:
            if (physics := tile.get("physics")) is not None:
                binding.update(struct.pack("<ii", tile["x"], tile["z"]))
                binding.update(physics["sha256"].encode("ascii"))
        support = index["support"]
        catalogue = json.loads(unpack_support((source / support["file"]).read_bytes()))
        catalogue["directory_sha256"] = binding.hexdigest()
        raw = json.dumps(catalogue, separators=(",", ":")).encode()
        packed = zlib.compress(raw, 9)
        sha = digest(packed)
        support.update(
            file=sha + ".zlib", sha256=sha, bytes=len(packed), decoded_bytes=len(raw)
        )
        (destination / support["file"]).write_bytes(packed)
    else:
        data[8] = 0x80
        # Replace the local link so the original content-addressed directory stays intact.
        path = destination / layer["file"]
        path.unlink()
        path.write_bytes(data)
    (destination / "index.json").write_text(json.dumps(index))


def recovery(binary, pages, out, reference):
    results = {}
    for name, authenticated, error in [
        ("wrong-digest", False, "bad tile content"),
        ("invalid-record", True, "invalid collision shape tag"),
    ]:
        with tempfile.TemporaryDirectory(dir=out) as temp:
            broken = Path(temp)
            corrupt(pages, broken, authenticated)
            path = out / (name + ".json")
            trace = RIG["paged"](
                binary, broken, FIXTURE, "parked", ["--actor", "human"], [], path
            )
            rejected = trace["support"]["rejected_full_pages"]
            ready = [
                event
                for event in trace["physics_events"]
                if event["tile"] == [0, 0] and event["state"] == "ready"
            ]
            require(
                rejected
                and all(r["tile"] == [0, 0] and error in r["error"] for r in rejected),
                name,
            )
            require(
                not ready and trace["support"]["stats"]["held_ticks"] == 0,
                "invalid collider reached publication",
            )
            solution = state(trace)
            require(
                solution == reference,
                "certified support did not preserve body/box poses and all contacts",
            )
            strict = subprocess.run(
                [
                    binary,
                    "--paging",
                    "--all-resident",
                    "--pages",
                    broken,
                    "--replay",
                    FIXTURE,
                    "--route",
                    "parked",
                    "--out",
                    out / (name + "-all.json"),
                ],
                capture_output=True,
                check=False,
            )
            require(
                strict.returncode != 0 and error in strict.stderr.decode(),
                f"{name}: all-resident admitted invalid bytes",
            )
            results[name] = {
                "error": error,
                "failed_read_ticks": [r["tick"] for r in rejected],
                "ready_events": len(ready),
                "strict_exit": strict.returncode,
                **solution,
            }
    return results


def historic(binary, out):
    glb = runpy.run_path(str(HERE / "check-impact-transmission.py"))
    doc, buffer = glb["read_glb"](SOURCE)
    (road,) = [n for n in doc["nodes"] if n.get("name") == "support/road"]
    doc["nodes"] = [road]
    doc["scenes"][doc["scene"]]["nodes"] = [0]
    out.mkdir(exist_ok=True)
    source = out / "road.glb"
    glb["write_glb"](source, doc, buffer)
    cook = runpy.run_path(str(HERE / "cook-pages.py"))["cook"]
    results = {}
    for encoding in ("json", "packed"):
        for version, storage in [(2, "embedded"), (3, "shared"), (4, "demand")]:
            directory = out / f"v{version}-{encoding}"
            cook(source, directory, 512, storage=storage, physics_storage=encoding)
            idx = read(directory / "index.json")
            require(
                idx["format"] == f"ville-paris/pages/{version}",
                "unexpected pages version",
            )
            geometry(idx, "json" if encoding == "json" else "cpage")
            variants = [(version, directory)]
            if version == 2:
                first = out / f"v1-{encoding}"
                first.mkdir(exist_ok=True)
                for file in directory.iterdir():
                    if file.name != "index.json" and not (first / file.name).exists():
                        (first / file.name).symlink_to(file.resolve())
                # The original embedded format had neither a source tree nor a resident support catalogue.
                idx["format"] = "ville-paris/pages/1"
                idx.pop("support")
                idx.pop("source_parents")
                for tile in idx["tiles"]:
                    if (collider := tile.get("physics")) is not None:
                        collider.pop("source_nodes", None)
                        collider.pop("materials", None)
                        if encoding == "json":
                            former = read(directory / collider["file"])
                            for s in former:
                                s.pop("source_node", None)
                            raw = json.dumps(former, separators=(",", ":")).encode()
                            sha = digest(raw)
                            collider.update(
                                file=sha + ".json", sha256=sha, bytes=len(raw)
                            )
                            (first / collider["file"]).write_bytes(raw)
                (first / "index.json").write_text(json.dumps(idx))
                variants.append((1, first))
            for v, pages in variants:
                trace = RIG["paged"](
                    binary,
                    pages,
                    FIXTURE,
                    "parked",
                    ["--actor", "capsule"],
                    ["--all-resident"],
                    out / f"trace-v{v}-{encoding}.json",
                )
                snapshot = state(trace)
                results[f"{v}/{encoding}"] = {
                    "actor": snapshot["actor_sha256"],
                    "box": snapshot["box_sha256"],
                    "contacts": snapshot["contact_points"],
                    "format": trace["storage"]["format"],
                }
    require(
        len({(r["actor"], r["box"], r["contacts"]) for r in results.values()}) == 1,
        "directory version changed the result",
    )
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, required=True)
    parser.add_argument("--json-pages", type=Path, required=True)
    parser.add_argument("--pages", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    binary, old, packed, out = (
        p.resolve() for p in (args.bin, args.json_pages, args.pages, args.out)
    )
    out.mkdir(parents=True, exist_ok=True)
    trajectories, recordings = paired(binary, old, packed, out)
    ref = state(read(out / "packed/human-parked-streamed.json"))
    report = {
        "binary_sha256": RIG["sha"](binary),
        "source_sha256": RIG["sha"](SOURCE),
        "inputs": {
            enc: RIG["sha"](p / "index.json")
            for enc, p in [("json", old), ("packed", packed)]
        },
        "trajectories": trajectories,
        "stereo_recordings": recordings,
        "rejections": recovery(binary, packed, out, ref),
        "historic_versions": historic(binary, out / "versions"),
    }
    (out / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    print(
        "18 exact trajectory pairs and their contact witnesses, 4 stereo pairs, 2 rejection paths, 8 directory variants"
    )


if __name__ == "__main__":
    main()
