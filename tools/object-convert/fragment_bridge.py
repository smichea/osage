#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

"""Optional independent interchange bridge. Supply a local CodeWalker.Core assembly and its dependencies."""

import argparse
import sys
import tempfile
from pathlib import Path


def transcode(data, assembly, *, encode):
    try:
        from pythonnet import load

        load("coreclr")
        import clr
        import System

        assembly = assembly.resolve(strict=True)
        directory = str(assembly.parent)
        if directory not in sys.path:
            sys.path.append(directory)
        loaded = clr.AddReference(str(assembly))
        if not assembly.samefile(str(loaded.Location)):
            raise ValueError("a different assembly has already been loaded in this process")
        from CodeWalker.GameFiles import XmlYft, YftFile, YftXml

        with tempfile.TemporaryDirectory(prefix="osage-fragment-") as directory:
            if encode:
                return bytes(XmlYft.GetYft(data.decode("utf-8-sig"), directory).Save())
            f = YftFile()
            f.Load(System.Array[System.Byte](data))
            return YftXml.GetXml(f, directory).encode("utf-8")
    except Exception as exc:
        raise ValueError(f"fragment bridge failed: {exc}") from exc


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--assembly", type=Path, required=True)
    ap.add_argument("mode", choices=("encode", "decode"))
    ap.add_argument("input", type=Path)
    ap.add_argument("output", type=Path)
    a = ap.parse_args()
    if a.output.resolve() in (a.input.resolve(), a.assembly.resolve()):
        ap.error("choose a separate output path")
    try:
        payload = transcode(a.input.read_bytes(), a.assembly, encode=a.mode == "encode")
        from convert import publish

        publish(a.output, payload)
    except (OSError, ValueError) as e:
        ap.exit(1, f"{e}\n")
