#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Observe actual Rapier/Jolt native shape identities and bytes on an x86_64 Linux release with symbols."""

import argparse
import json
import os
import platform
import subprocess
from pathlib import Path

HERE = Path(__file__).resolve().parent


def builder_symbol(binary):
    # Match the actual Rust crate disambiguator, including its dependency feature set.
    with subprocess.Popen(
        ["nm", "-g", "--defined-only", binary],
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
    ) as nm:
        for line in nm.stdout:
            cols = line.split()
            if len(cols) == 3 and cols[-1].endswith("15ColliderBuilder5build"):
                nm.stdout.close()
                return cols[-1]
    return None


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    for key in ["bin", "scene", "replay", "out"]:
        ap.add_argument("--" + key, type=Path, required=True)
    ap.add_argument(
        "--deps", type=Path, default=HERE.parents[1] / "target/release/deps"
    )
    ap.add_argument("--placements", type=int, required=True)
    args = ap.parse_args()
    out = args.out.resolve()
    out.mkdir(exist_ok=True, parents=True)
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        ap.error("native observation uses the x86_64 System V ABI")
    symbol = builder_symbol(args.bin)
    rlib = next(
        (
            p
            for p in args.deps.glob("librapier3d-*.rlib")
            if symbol and builder_symbol(p) == symbol
        ),
        None,
    )
    if rlib is None:
        ap.error(
            "no exact matching Rapier rlib; supply the release dependencies with --deps"
        )
    helper = out / "libosage_shape_observer.so"
    subprocess.run(
        [
            "rustc",
            "--edition",
            "2024",
            "--crate-type",
            "cdylib",
            "--crate-name",
            "osage_shape_observer",
            "-C",
            "opt-level=3",
            "-L",
            "dependency=" + str(args.deps),
            "--extern",
            "rapier3d=" + str(rlib),
            HERE / "probes/rapier-shape-observer.rs",
            "-o",
            helper,
        ],
        check=True,
    )
    rows = {}
    for eng in ["rapier", "jolt"]:
        env = {
            **os.environ,
            "OSAGE_NATIVE_ENGINE": eng,
            "OSAGE_SHAPE_OBSERVER": str(helper),
        }
        command = [
            "gdb",
            "-iex",
            "set debuginfod enabled off",
            "-nx",
            "-q",
            "-batch",
            "-x",
            HERE / "probes/native-shape-memory.gdb",
            "--args",
            args.bin.resolve(),
            "--scene",
            args.scene.resolve(),
            "--physics",
            eng,
            "--replay",
            args.replay.resolve(),
            "--bench",
            out / eng,
        ]
        raw = subprocess.check_output(
            command, env=env, stderr=subprocess.STDOUT, text=True
        )
        (out / (eng + ".log")).write_text(raw)
        row = json.loads(
            next(
                l.removeprefix("OSAGE_SHAPE_MEMORY ")
                for l in raw.splitlines()
                if l.startswith("OSAGE_SHAPE_MEMORY ")
            )
        )
        if (
            "exited normally" not in raw
            or row["fixed_colliders"] != args.placements
            or not row["initial_shapes"]
            or row["unique_shape_bytes_seen"] <= 0
        ):
            raise ValueError(f"{eng}: incomplete observation")
        rows[eng] = row
    (out / "summary.json").write_text(json.dumps(rows, indent=2) + "\n")
    print(rows)


if __name__ == "__main__":
    main()
