#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

"""How well a field of views compresses (issue #7, holographic textures).

    python3 tools/hologram-measure.py VIEWS [OUT]

VIEWS is a folder of `view-rRR-cCC.png` written by `hologram-bake`: one
picture of an object per viewing direction, row RR of elevation, column CC
of azimuth. This compares, at several qualities, the size of:

  - every view on its own, as JPEG and as WebP: "flat images";
  - the views as one video along the viewing angle (H.265 and AV1): row by
    row, back and forth, each view a step of angle from the one before, as
    each frame of a film is a step of time from the one before;
  - the same videos with the views in a random order: what the order of
    angles is worth.

Quality is measured against the original views with ffmpeg's PSNR and SSIM
over all of them. It writes `results.tsv` and a summary to OUT (default:
VIEWS/../measure). Needs ffmpeg with libx265, libsvtav1, libwebp. Standard
library only.
"""

import pathlib
import random
import re
import shutil
import subprocess
import sys
import tempfile


def run(*args: str) -> str:
    done = subprocess.run(args, capture_output=True, text=True)
    if done.returncode != 0:
        raise RuntimeError(f"{' '.join(args)}\n{done.stderr[-2000:]}")
    return done.stderr


def views(folder: pathlib.Path):
    found = {}
    for path in folder.glob("view-r*-c*.png"):
        m = re.match(r"view-r(\d+)-c(\d+)\.png", path.name)
        if m:
            found[(int(m[1]), int(m[2]))] = path
    rows = max(r for r, _ in found) + 1
    columns = max(c for _, c in found) + 1
    path = [(r, c if r % 2 == 0 else columns - 1 - c) for r in range(rows) for c in range(columns)]
    return [found[v] for v in path if v in found]


def sequence(order, into: pathlib.Path) -> str:
    """The views as numbered frames, by links, in `order`."""
    into.mkdir(parents=True, exist_ok=True)
    for i, src in enumerate(order):
        dst = into / f"{i:04d}.png"
        if dst.exists():
            dst.unlink()
        dst.symlink_to(src.resolve())
    return str(into / "%04d.png")


def quality(reference: str, candidate: str) -> tuple[float, float]:
    """Mean PSNR (dB) and SSIM of a candidate sequence against the reference."""
    log = run(
        "ffmpeg", "-hide_banner", "-i", candidate, "-i", reference,
        "-lavfi", "[0:v]format=yuv444p[a];[1:v]format=yuv444p[b];[a][b]psnr;[0:v]format=yuv444p[c];[1:v]format=yuv444p[d];[c][d]ssim",
        "-f", "null", "-",
    )
    psnr = float(re.search(r"PSNR .*average:([\d.]+|inf)", log)[1].replace("inf", "99"))
    ssim = float(re.search(r"SSIM .*All:([\d.]+)", log)[1])
    return psnr, ssim


def video(frames: str, codec: str, crf: int, out: pathlib.Path) -> pathlib.Path:
    file = out.with_suffix(".mkv")
    if codec == "x265":
        args = ["-c:v", "libx265", "-preset", "slow", "-crf", str(crf), "-x265-params", "log-level=error:keyint=600"]
    else:
        args = ["-c:v", "libsvtav1", "-preset", "6", "-crf", str(crf), "-g", "600"]
    run("ffmpeg", "-hide_banner", "-y", "-framerate", "10", "-i", frames, "-pix_fmt", "yuv420p", *args, str(file))
    return file


def decoded(file: pathlib.Path, into: pathlib.Path) -> str:
    into.mkdir(parents=True, exist_ok=True)
    run("ffmpeg", "-hide_banner", "-y", "-i", str(file), str(into / "%04d.png"))
    return str(into / "%04d.png")


def images(frames: list[pathlib.Path], kind: str, q: int, into: pathlib.Path) -> tuple[int, str]:
    into.mkdir(parents=True, exist_ok=True)
    back = into / "back"
    back.mkdir(exist_ok=True)
    total = 0
    for i, src in enumerate(frames):
        if kind == "jpeg":
            file = into / f"{i:04d}.jpg"
            run("ffmpeg", "-hide_banner", "-y", "-i", str(src), "-q:v", str(q), str(file))
        else:
            file = into / f"{i:04d}.webp"
            run("ffmpeg", "-hide_banner", "-y", "-i", str(src), "-c:v", "libwebp", "-quality", str(q), str(file))
        total += file.stat().st_size
        run("ffmpeg", "-hide_banner", "-y", "-i", str(file), str(back / f"{i:04d}.png"))
    return total, str(back / "%04d.png")


def main() -> int:
    folder = pathlib.Path(sys.argv[1])
    out = pathlib.Path(sys.argv[2]) if len(sys.argv) > 2 else folder.parent / "measure"
    out.mkdir(parents=True, exist_ok=True)
    order = views(folder)
    n = len(order)
    probe = subprocess.run(["ffmpeg", "-hide_banner", "-i", str(order[0])], capture_output=True, text=True).stderr
    width = int(re.search(r", (\d+)x(\d+)", probe)[1])
    raw = n * width * width * 3
    work = pathlib.Path(tempfile.mkdtemp(prefix="hologram-", dir=out))
    reference = sequence(order, work / "path")
    shuffled = order[:]
    random.Random(7).shuffle(shuffled)
    reference_shuffled = sequence(shuffled, work / "shuffled")
    rows = [("method", "setting", "bytes", "bytes_per_view", "ratio_to_raw", "psnr_db", "ssim")]

    def record(method: str, setting: str, size: int, psnr: float, ssim: float):
        rows.append((method, setting, str(size), f"{size / n:.0f}", f"{raw / size:.1f}", f"{psnr:.2f}", f"{ssim:.4f}"))
        print(f"{method:<22} {setting:<8} {size / n / 1024:8.1f} KiB/view  x{raw / size:7.1f}  {psnr:6.2f} dB  SSIM {ssim:.4f}", flush=True)

    for q in (2, 5, 10):
        size, back = images(order, "jpeg", q, work / f"jpeg-{q}")
        record("jpeg each view", f"q{q}", size, *quality(reference, back))
    for q in (90, 75, 50):
        size, back = images(order, "webp", q, work / f"webp-{q}")
        record("webp each view", f"q{q}", size, *quality(reference, back))
    for codec, crfs in (("x265", (18, 24, 30)), ("av1", (25, 35, 45))):
        for crf in crfs:
            file = video(reference, codec, crf, work / f"{codec}-path-{crf}")
            record(f"{codec} along the angle", f"crf{crf}", file.stat().st_size, *quality(reference, decoded(file, work / f"{codec}-path-{crf}-back")))
            file = video(reference_shuffled, codec, crf, work / f"{codec}-shuffled-{crf}")
            back = decoded(file, work / f"{codec}-shuffled-{crf}-back")
            record(f"{codec} shuffled views", f"crf{crf}", file.stat().st_size, *quality(reference_shuffled, back))
    (out / "results.tsv").write_text("\n".join("\t".join(r) for r in rows) + "\n")
    print(f"\n{n} views of {width}x{width}, raw {raw / 1e6:.1f} MB; written to {out / 'results.tsv'}")
    shutil.rmtree(work)
    return 0


if __name__ == "__main__":
    sys.exit(main())
