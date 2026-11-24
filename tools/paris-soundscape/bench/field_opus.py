#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

"""What a far field costs to send: its bit rate in Opus, against what is lost.

    cargo build --release -p paris-soundscape
    python3 tools/paris-soundscape/bench/field_opus.py [--sphere 64] [--seconds 10]

For each order (1: 4 channels, 2: 9), the far field of a sphere of noises
round the listener is written as 16-bit WAV (`paris-soundscape field`),
encoded with ffmpeg's libopus at a ladder of bit rates in its ambisonic
channel mapping (family 2), decoded back, and both — the field as made and
as sent — are heard by a listener at the cube's centre (`decode`). The two
renders are compared (`compare --json`): loudness, each octave, and what
the ears compare. It prints, for each rate, the bit rate the file really
took and the largest differences. Needs ffmpeg with libopus; standard
library only. Nothing is downloaded.
"""

import argparse
import json
import pathlib
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
PROGRAM = ROOT / "target/release/paris-soundscape"
RATES = (16, 24, 32, 48, 64, 96, 128, 192, 256)


def run(*command):
    done = subprocess.run([str(part) for part in command], capture_output=True, text=True, check=False)
    if done.returncode != 0:
        sys.exit(f"{' '.join(map(str, command))}: {done.stderr.strip() or done.stdout.strip()}")
    return done.stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--sphere", type=int, default=64)
    parser.add_argument("--radius", type=float, default=150.0)
    parser.add_argument("--seconds", type=float, default=10.0)
    parser.add_argument("--gain-db", type=float, default=36.0, help="brings the field near full scale in 16 bits")
    parser.add_argument("--out", default=str(ROOT / "tools/paris-soundscape/out/bench/field_opus.json"))
    args = parser.parse_args()
    if not PROGRAM.exists():
        sys.exit("build it first: cargo build --release -p paris-soundscape")
    if not shutil.which("ffmpeg"):
        sys.exit("ffmpeg is missing")
    receipt = {"sphere": args.sphere, "radius_m": args.radius, "seconds": args.seconds, "orders": {}}
    with tempfile.TemporaryDirectory(prefix="osage-field-") as tmp:
        tmp = pathlib.Path(tmp)
        for order in (1, 2):
            field = tmp / f"field-{order}.wav"
            run(PROGRAM, "field", "--sphere", args.sphere, "--radius", args.radius, "--far-order", order,
                "--seconds", args.seconds, "--gain-db", args.gain_db, "--out", field)
            reference = tmp / f"heard-{order}.wav"
            run(PROGRAM, "decode", field, "--out", reference)
            print(f"order {order} ({(order + 1) ** 2} channels), {args.sphere} noises at {args.radius:.0f} m, {args.seconds:.0f} s")
            print("  asked   took    loudness  worst octave  worst ILD  worst ITD")
            rows = []
            for kbps in RATES:
                sent, back, heard = tmp / f"f{order}-{kbps}.opus", tmp / f"b{order}-{kbps}.wav", tmp / f"h{order}-{kbps}.wav"
                run("ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-i", field, "-c:a", "libopus",
                    "-b:a", f"{kbps}k", "-mapping_family", "2", sent)
                run("ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-i", sent, "-ar", "48000",
                    "-c:a", "pcm_s16le", back)
                run(PROGRAM, "decode", back, "--out", heard)
                d = json.loads(run(PROGRAM, "compare", reference, heard, "--json"))
                took = sent.stat().st_size * 8 / args.seconds / 1000
                loud = d["loudness"][1] - d["loudness"][0]
                # The octaves where there is sound to speak of: 63 Hz to 8 kHz.
                octave = max(abs(db) for hz, db in d["octaves_db"] if 60 <= hz <= 8000)
                ild = max(row[1] for row in d["cues_ild_db_itd_us"])
                itd = max(row[2] for row in d["cues_ild_db_itd_us"])
                rows.append({"asked_kbps": kbps, "took_kbps": round(took, 1), "loudness_lu": round(loud, 3),
                             "worst_octave_db": round(octave, 2), "worst_ild_db": round(ild, 2), "worst_itd_us": round(itd, 1)})
                print(f"  {kbps:>5}  {took:6.1f}  {loud:+8.2f}  {octave:12.2f}  {ild:9.2f}  {itd:9.0f}")
            receipt["orders"][str(order)] = rows
    out = pathlib.Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(receipt, indent=2) + "\n")
    print(f"receipt: {out}")


if __name__ == "__main__":
    main()
