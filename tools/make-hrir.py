#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Makes the engine's measured head responses from the SADIE II set (D-242).

    python3 tools/make-hrir.py

Reads assets/sadie/D2_HRIR_SOFA.zip (subject D2, a KEMAR, of the SADIE II
database, University of York, Apache-2.0; fetched by tools/fetch-assets.py),
takes the 48 kHz responses and their directions out of the SOFA file with
HDF5's own `h5dump`, and has `paris-soundscape hrir` derive
assets/sadie/d2-48k.hrir: 54 virtual loudspeakers in rings, each ear's response cut to
64 taps round its peak, and the time between the ears on a 5° grid. Needs
h5dump (HDF5) and cargo; Python standard library only. The engine hears with
the sphere model when the file is not there.

    python3 tools/make-hrir.py --check

hears a noise round the head through the file made and through the sphere
model, against the measured set itself (`paris-soundscape ears`).
"""

import pathlib
import subprocess
import sys
import tempfile
import zipfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
ARCHIVE = ROOT / "assets" / "sadie" / "D2_HRIR_SOFA.zip"
MEMBER = "D2_HRIR_SOFA/D2_48K_24bit_256tap_FIR_SOFA.sofa"
OUT = ROOT / "assets" / "sadie" / "d2-48k.hrir"


def main() -> int:
    if not ARCHIVE.exists():
        print(f"{ARCHIVE.relative_to(ROOT)} is missing: run tools/fetch-assets.py", file=sys.stderr)
        return 1
    with tempfile.TemporaryDirectory() as work:
        work = pathlib.Path(work)
        sofa = work / "set.sofa"
        with zipfile.ZipFile(ARCHIVE) as archive:
            sofa.write_bytes(archive.read(MEMBER))
        arrays = {}
        for name, dataset in (("ir", "/Data.IR"), ("positions", "/SourcePosition")):
            arrays[name] = work / f"{name}.bin"
            subprocess.run(
                ["h5dump", "-d", dataset, "-b", "LE", "-o", str(arrays[name]), str(sofa)],
                check=True,
                stdout=subprocess.DEVNULL,
            )
        given = ["--ir", str(arrays["ir"]), "--positions", str(arrays["positions"])]
        if "--check" in sys.argv[1:]:
            command = ["ears", "--hrir", str(OUT), "--quality", "reference", *given]
        else:
            command = ["hrir", *given, "--taps", "256", "--rate", "48000", "--keep", "64",
                       "--out", str(OUT)]
        subprocess.run(
            ["cargo", "run", "--quiet", "--release", "-p", "paris-soundscape", "--", *command],
            check=True,
            cwd=ROOT,
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
