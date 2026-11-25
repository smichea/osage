#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

"""Makes the kit of a blind listening test (PROTOCOL.md beside this file).

    python3 tools/paris-soundscape/listening/make_kit.py --plan PLAN.json --out KIT --key KEY.json \\
        --listeners 12 --seed 2026 [--measure target/release/paris-soundscape]

PLAN.json lists the trials, each a pair of takes of the same thing by two engines:

    {"trials": [{"id": "bridge", "osage": "a.wav", "reference": "b.wav", "from": 5.0, "seconds": 20.0}]}

The kit holds what listeners get and, apart, what only the experimenter keeps:

  KIT/audio/XXXXXX.wav     every excerpt as the same kind of file (stereo, 48 kHz, 16 bits, dithered),
                           as long as its pair, faded alike, its loudness matched to the others, under a
                           code that says nothing of where it came from
  KIT/listener-NN.html     one page per listener: the trials in an order of its own, which take is X
                           balanced across listeners, a catch trial (one take twice), the questions, and
                           a button that saves the answers as a CSV file. It plays from the folder, offline.
  KEY.json                 which code is which engine, and each listener's order: kept out of the kit
                           folder, and given to no listener

Every take is read and every excerpt's loudness measured by `paris-soundscape measure` (ITU-R BS.1770).
What a take holds of digital silence (runs of 1 ms or more where both channels are exactly zero, as
an output that was not fed leaves) is printed: such gaps can tell a listener which engine made a take.
"""

import argparse
import array
import html
import json
import math
import pathlib
import random
import struct
import subprocess
import sys
import tempfile

RATE = 48_000
FADE_S = 0.02
CEILING_DB = -1.0
TARGET_LUFS = -23.0
QUESTIONS = [
    ("place", "Where the sounds are: which places each sound more clearly and steadily where it seems to be?"),
    ("distance", "Distance: which makes how near or far each sound is more believable?"),
    ("natural", "The sources: which sounds more like the real things it stands for?"),
    ("space", "The space: which makes the place the sounds are in more believable?"),
]
CHOICES = [("x", "X is better"), ("same", "No difference"), ("y", "Y is better")]


def read_wav(path):
    """Two channels of floats, from a WAV file of 16-bit, 24-bit or 32-bit float samples."""
    data = pathlib.Path(path).read_bytes()
    if data[:4] != b"RIFF" or data[8:12] != b"WAVE":
        sys.exit(f"{path}: not a WAV file")
    at, fmt, samples = 12, None, None
    while at + 8 <= len(data):
        tag, size = data[at:at + 4], struct.unpack("<I", data[at + 4:at + 8])[0]
        body = data[at + 8:at + 8 + size]
        if tag == b"fmt ":
            code, channels, rate = struct.unpack("<HHI", body[:8])
            bits = struct.unpack("<H", body[14:16])[0]
            if code == 0xFFFE:
                code = struct.unpack("<H", body[24:26])[0]
            fmt = (code, channels, rate, bits)
        elif tag == b"data":
            samples = body
        at += 8 + size + (size & 1)
    if not fmt or samples is None:
        sys.exit(f"{path}: no format or no samples")
    code, channels, rate, bits = fmt
    if rate != RATE:
        sys.exit(f"{path}: {rate} Hz; the kit is made at {RATE}")
    if code == 3 and bits == 32:
        values = array.array("f", samples)
    elif code == 1 and bits == 16:
        values = [v / 32768.0 for v in array.array("h", samples)]
    elif code == 1 and bits == 24:
        values = [int.from_bytes(samples[i:i + 3], "little", signed=True) / 8388608.0
                  for i in range(0, len(samples) - 2, 3)]
    else:
        sys.exit(f"{path}: format {code} with {bits} bits is not read")
    left = list(values[0::channels])
    right = list(values[1::channels]) if channels > 1 else left
    return left, right


def write_float(path, left, right):
    frames = array.array("f", (v for pair in zip(left, right) for v in pair))
    body = frames.tobytes()
    head = struct.pack("<4sI4s4sIHHIIHH4sI", b"RIFF", 36 + len(body), b"WAVE", b"fmt ", 16, 3, 2, RATE,
                       RATE * 8, 8, 32, b"data", len(body))
    pathlib.Path(path).write_bytes(head + body)


def write_pcm16(path, left, right, rng):
    """16 bits, with triangular dither of one step, as the engine's own files are written."""
    out = array.array("h")
    for pair in zip(left, right):
        for v in pair:
            dithered = v * 32767.0 + rng.random() - rng.random()
            out.append(max(-32768, min(32767, round(dithered))))
    body = out.tobytes()
    head = struct.pack("<4sI4s4sIHHIIHH4sI", b"RIFF", 36 + len(body), b"WAVE", b"fmt ", 16, 1, 2, RATE,
                       RATE * 4, 4, 16, b"data", len(body))
    pathlib.Path(path).write_bytes(head + body)


def gaps(left, right):
    """Runs of at least 1 ms where both channels are exactly zero: how many, and how long in all, s."""
    shortest = RATE // 1000
    count, total, run = 0, 0, 0
    for l, r in zip(left, right):
        if l == 0.0 and r == 0.0:
            run += 1
            continue
        if run >= shortest:
            count, total = count + 1, total + run
        run = 0
    if run >= shortest:
        count, total = count + 1, total + run
    return count, total / RATE


def excerpt(take, start, seconds):
    left, right = take
    a, b = int(start * RATE), int((start + seconds) * RATE)
    if b > len(left):
        sys.exit(f"an excerpt ends at {start + seconds} s, after its take ({len(left) / RATE:.2f} s)")
    left, right = left[a:b], right[a:b]
    fade = int(FADE_S * RATE)
    for i in range(fade):
        g = 0.5 - 0.5 * math.cos(math.pi * i / fade)
        left[i] *= g
        right[i] *= g
        left[-1 - i] *= g
        right[-1 - i] *= g
    return left, right


def measure(binary, left, right):
    with tempfile.TemporaryDirectory() as folder:
        path = pathlib.Path(folder) / "take.wav"
        write_float(path, left, right)
        done = subprocess.run([binary, "measure", "--json", str(path)], capture_output=True, text=True, check=True)
    report = json.loads(done.stdout)
    return report["loudness_lkfs"], report["true_peak_dbtp"]


def code(rng, used):
    while True:
        name = "".join(rng.choice("bcdfghjkmnpqrstvwxz") for _ in range(6))
        if name not in used:
            used.add(name)
            return name


def page(listener, trials):
    """One listener's page: the trials in its order, and a button that saves its answers."""
    rows = []
    for n, trial in enumerate(trials, 1):
        questions = "".join(
            f"<fieldset><legend>{html.escape(text)}</legend>"
            + "".join(f'<label><input type="radio" name="t{n}-{key}" value="{value}" required> {html.escape(label)}</label>'
                      for value, label in CHOICES)
            + "</fieldset>"
            for key, text in QUESTIONS)
        rows.append(
            f'<section data-trial="{html.escape(trial["trial"])}"><h2>Trial {n} of {len(trials)}</h2>'
            f'<p><b>X</b> <audio controls preload="auto" src="audio/{trial["x"]}.wav"></audio></p>'
            f'<p><b>Y</b> <audio controls preload="auto" src="audio/{trial["y"]}.wav"></audio></p>{questions}</section>')
    fields = json.dumps([key for key, _ in QUESTIONS])
    return f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Listening test</title>
<style>
body {{ font: 16px/1.5 system-ui, sans-serif; max-width: 46rem; margin: 2rem auto; padding: 0 1rem; background: #fff; color: #111; }}
section {{ border-top: 1px solid #ccc; padding: 1rem 0; }}
fieldset {{ border: 0; padding: 0.3rem 0; }} legend {{ font-weight: 600; }}
label {{ margin-right: 1.2rem; white-space: nowrap; }} audio {{ vertical-align: middle; width: 100%; max-width: 30rem; }}
</style></head><body>
<h1>Listening test</h1>
<p>Listener {listener:02d}. Use the headphones you were given, at the level set at the start, and do not change it.
Each trial has two recordings, X and Y. Play each as often as you like, in any order, then answer every question.
"No difference" is a good answer when you hear none.</p>
<form id="test">{"".join(rows)}
<section><h2>Last question</h2><fieldset><legend>Did you think you knew which program made a recording, in any trial?</legend>
<label><input type="radio" name="recognised" value="no" required> No</label>
<label><input type="radio" name="recognised" value="yes"> Yes</label></fieldset>
<p><label>If yes, which trials and why: <input name="why" size="40"></label></p>
<p><button>Save my answers</button></p></section></form>
<script>
const fields = {fields};
document.getElementById("test").addEventListener("submit", (event) => {{
  event.preventDefault();
  const form = new FormData(event.target);
  const lines = ["listener,position,trial,question,answer"];
  document.querySelectorAll("section[data-trial]").forEach((section, i) => {{
    for (const field of fields) lines.push([{listener}, i + 1, section.dataset.trial, field, form.get(`t${{i + 1}}-${{field}}`)].join(","));
  }});
  lines.push([{listener}, "", "", "recognised", form.get("recognised")].join(","));
  lines.push([{listener}, "", "", "why", JSON.stringify(form.get("why") || "")].join(","));
  const link = document.createElement("a");
  link.href = URL.createObjectURL(new Blob([lines.join("\\n") + "\\n"], {{type: "text/csv"}}));
  link.download = "answers-{listener:02d}.csv";
  link.click();
}});
</script></body></html>
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--plan", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--key", required=True)
    parser.add_argument("--listeners", type=int, default=12)
    parser.add_argument("--seed", type=int, required=True)
    parser.add_argument("--measure", default="target/release/paris-soundscape")
    args = parser.parse_args()
    plan = json.loads(pathlib.Path(args.plan).read_text(encoding="utf-8"))
    out = pathlib.Path(args.out)
    if pathlib.Path(args.key).resolve().is_relative_to(out.resolve()):
        sys.exit("--key inside --out: the key would go to the listeners with the kit")
    (out / "audio").mkdir(parents=True, exist_ok=True)
    rng = random.Random(args.seed)
    used = set()

    # Cut every excerpt and measure it; then one gain for each brings all to the same loudness, as loud as the
    # take with least headroom allows.
    cut = []
    for trial in plan["trials"]:
        for engine in ("osage", "reference"):
            take = read_wav(trial[engine])
            count, silent = gaps(*take)
            clip = excerpt(take, trial.get("from", 0.0), trial["seconds"])
            in_clip, silent_clip = gaps(*clip)
            loudness, peak = measure(args.measure, *clip)
            print(f"{trial['id']:>10} {engine:>9}: {loudness:6.1f} LUFS, true peak {peak:6.1f} dBTP; "
                  f"digital silence: {count} gaps, {silent:.3f} s in the take, {in_clip} gaps, {silent_clip:.3f} s in the excerpt")
            cut.append({"trial": trial["id"], "engine": engine, "clip": clip, "loudness": loudness, "peak": peak})
    target = min([TARGET_LUFS] + [c["loudness"] + CEILING_DB - c["peak"] for c in cut])
    print(f"every excerpt brought to {target:.1f} LUFS")
    key = {"seed": args.seed, "target_lufs": target, "codes": {}, "listeners": {}}
    for c in cut:
        name = code(rng, used)
        gain = 10 ** ((target - c["loudness"]) / 20)
        left, right = ([v * gain for v in channel] for channel in c["clip"])
        write_pcm16(out / "audio" / f"{name}.wav", left, right, rng)
        c["code"] = name
        key["codes"][name] = {"trial": c["trial"], "engine": c["engine"], "gain_db": round(20 * math.log10(gain), 2)}

    ids = [t["id"] for t in plan["trials"]]
    of = {(c["trial"], c["engine"]): c["code"] for c in cut}
    for listener in range(1, args.listeners + 1):
        order = ids[:]
        rng.shuffle(order)
        trials = []
        for k, trial in enumerate(order):
            # Which take is X: alternate across listeners, shifted by the trial, so each trial has osage first
            # for half of them.
            osage_first = (listener + ids.index(trial)) % 2 == 0
            a, b = of[(trial, "osage")], of[(trial, "reference")]
            trials.append({"trial": trial, "x": a if osage_first else b, "y": b if osage_first else a})
        catch = rng.choice(ids)
        same = of[(catch, rng.choice(["osage", "reference"]))]
        trials.insert(rng.randrange(1, len(trials) + 1), {"trial": f"catch:{catch}", "x": same, "y": same})
        key["listeners"][f"{listener:02d}"] = trials
        (out / f"listener-{listener:02d}.html").write_text(page(listener, trials), encoding="utf-8")
    pathlib.Path(args.key).write_text(json.dumps(key, indent=1) + "\n", encoding="utf-8")
    print(f"{args.listeners} listeners' pages and {len(cut)} excerpts in {out}; the key in {args.key}")


if __name__ == "__main__":
    main()
