#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Attester of the render bench (knowledge/oracles/render-bench.md).

    python3 tools/attest-bench.py bench/out/<run>/report.json [--tier T2] [--allow-dirty]

Reads the receipt a bench run wrote and answers pass or fail: the receipt
must come from the sanctioned bench, for the claimed tier, with nothing left
out, from a committed state of the code, and its mean frame time must be
within the tier's budget. Deterministic; standard library only.
"""

import json
import sys

SANCTIONED = "kart-lap/1"
FEWEST_FRAMES = 2000


def attest(report: dict, tier: str | None, allow_dirty: bool) -> tuple[bool, list[str]]:
    reasons = []
    if report.get("bench") != SANCTIONED:
        reasons.append(f"not the sanctioned bench: {report.get('bench')!r}, expected {SANCTIONED!r}")
    if tier is not None and report.get("tier") != tier:
        reasons.append(f"run for tier {report.get('tier')}, not {tier}")
    if report.get("without"):
        reasons.append(f"techniques left out: {', '.join(report['without'])}")
    timed = report.get("frames", {}).get("timed", 0)
    if timed < FEWEST_FRAMES:
        reasons.append(f"only {timed} frames timed, at least {FEWEST_FRAMES} needed")
    commit = str(report.get("commit", "unknown"))
    if not allow_dirty and (commit == "unknown" or commit.endswith("-dirty")):
        reasons.append(f"not run from a committed state of the code: {commit}")
    mean, budget = report.get("frame_ms", {}).get("mean"), report.get("budget_ms")
    if mean is None or budget is None:
        reasons.append("the receipt has no mean frame time or no budget")
    elif mean > budget:
        reasons.append(f"mean frame time {mean} ms is over the budget of {budget} ms")
    return not reasons, reasons


def main() -> int:
    args = sys.argv[1:]
    allow_dirty = "--allow-dirty" in args
    tier = args[args.index("--tier") + 1] if "--tier" in args else None
    paths = [a for i, a in enumerate(args) if not a.startswith("--") and (i == 0 or args[i - 1] != "--tier")]
    if len(paths) != 1:
        print(__doc__)
        return 2
    with open(paths[0]) as file:
        report = json.load(file)
    passed, reasons = attest(report, tier, allow_dirty)
    verdict = {
        "verdict": "pass" if passed else "fail",
        "reasons": reasons,
        "tier": report.get("tier"),
        "machine": report.get("machine", {}).get("gpu"),
        "commit": report.get("commit"),
        "mean_ms": report.get("frame_ms", {}).get("mean"),
        "budget_ms": report.get("budget_ms"),
        "over_budget_percent": report.get("over_budget_percent"),
    }
    print(json.dumps(verdict, indent=2))
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
