#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

"""Reads the answers of a blind listening test against its key, as PROTOCOL.md beside this file says.

    python3 tools/paris-soundscape/listening/analyse.py --key KEY.json answers-*.csv

For each question, the share of answers that prefer osage (a tie counts half) over all trials and
listeners, with a 95 % interval from a bootstrap that draws listeners, not answers (each listener's
answers go together), made simultaneous across the questions by Bonferroni's correction. Then the same
by trial, the catch trials (one take twice, where "no difference" is the right answer), and how many
listeners thought they knew which program made a recording. Markdown, to be committed as it is printed.
"""

import argparse
import csv
import json
import pathlib
import random
import sys

QUESTIONS = ["place", "distance", "natural", "space"]
DRAWS = 10_000


def score(answer, osage_is_x):
    """1 when osage is preferred, 0 when the other is, ½ for no difference."""
    if answer == "same":
        return 0.5
    return 1.0 if (answer == "x") == osage_is_x else 0.0


def interval(by_listener, level, rng):
    """The pooled share and its bootstrap interval, listeners drawn with replacement."""
    listeners = [scores for scores in by_listener.values() if scores]
    pooled = sum(map(sum, listeners)) / sum(map(len, listeners))
    shares = []
    for _ in range(DRAWS):
        drawn = [rng.choice(listeners) for _ in listeners]
        shares.append(sum(map(sum, drawn)) / sum(map(len, drawn)))
    shares.sort()
    tail = (1 - level) / 2
    return pooled, shares[int(tail * DRAWS)], shares[min(DRAWS - 1, int((1 - tail) * DRAWS))]


def verdict(low, high):
    if low > 0.5:
        return "osage preferred"
    if high < 0.5:
        return "reference A preferred"
    return "no difference shown"


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--key", required=True)
    parser.add_argument("--seed", type=int, default=19)
    parser.add_argument("answers", nargs="+")
    args = parser.parse_args()
    key = json.loads(pathlib.Path(args.key).read_text(encoding="utf-8"))
    rng = random.Random(args.seed)

    scores = {q: {} for q in QUESTIONS}
    by_trial = {}
    catches = {}
    recognised = {}
    for path in args.answers:
        with open(path, newline="", encoding="utf-8") as file:
            for row in csv.DictReader(file):
                listener = f"{int(row['listener']):02d}"
                if row["question"] == "recognised":
                    recognised[listener] = row["answer"] == "yes"
                    continue
                if row["question"] not in QUESTIONS:
                    continue
                trials = key["listeners"].get(listener)
                if trials is None:
                    sys.exit(f"{path}: listener {listener} is not in the key")
                shown = trials[int(row["position"]) - 1]
                if shown["trial"] != row["trial"]:
                    sys.exit(f"{path}: position {row['position']} is {shown['trial']} in the key, {row['trial']} here")
                if shown["trial"].startswith("catch:"):
                    catches.setdefault(listener, []).append(row["answer"] == "same")
                    continue
                osage_is_x = key["codes"][shown["x"]]["engine"] == "osage"
                value = score(row["answer"], osage_is_x)
                scores[row["question"]].setdefault(listener, []).append(value)
                by_trial.setdefault((shown["trial"], row["question"]), []).append(value)

    listeners = sorted({l for q in scores.values() for l in q})
    level = 1 - 0.05 / len(QUESTIONS)
    print(f"## Blind listening test: {len(listeners)} listeners\n")
    print(f"Share of answers preferring osage, a tie counting half; 95 % simultaneous intervals over the "
          f"{len(QUESTIONS)} questions ({level:.2%} each, {DRAWS} bootstrap draws of listeners, seed {args.seed}).\n")
    print("| Question | Answers | Prefer osage | Interval | Verdict |")
    print("|---|---:|---:|---|---|")
    for q in QUESTIONS:
        if not scores[q]:
            continue
        pooled, low, high = interval(scores[q], level, rng)
        count = sum(map(len, scores[q].values()))
        print(f"| {q} | {count} | {pooled:.0%} | {low:.0%} – {high:.0%} | {verdict(low, high)} |")
    print("\nBy trial (share preferring osage; no interval, a trial's answers are too few):\n")
    trials = sorted({t for t, _ in by_trial})
    print("| Trial | " + " | ".join(QUESTIONS) + " |")
    print("|---|" + "---:|" * len(QUESTIONS))
    for t in trials:
        cells = []
        for q in QUESTIONS:
            values = by_trial.get((t, q), [])
            cells.append(f"{sum(values) / len(values):.0%} of {len(values)}" if values else "—")
        print(f"| {t} | " + " | ".join(cells) + " |")
    right = sum(sum(v) for v in catches.values())
    asked = sum(len(v) for v in catches.values())
    print(f"\nCatch trials: \"no difference\" in {right} of {asked} answers"
          + (f" ({right / asked:.0%})." if asked else "."))
    shaky = [l for l, v in catches.items() if sum(v) < len(v) / 2]
    if shaky:
        print(f"Listeners who heard a difference in most catch answers (kept, as the protocol says): {', '.join(shaky)}.")
    knew = [l for l, v in recognised.items() if v]
    print(f"Listeners who thought they knew which program made a recording: {len(knew)} of {len(recognised)}"
          + (f" ({', '.join(knew)})." if knew else "."))


if __name__ == "__main__":
    main()
