#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Attests the receipts of a cloud bench run and summarises them in Markdown.

    python3 tools/cloud-bench/summary.py OUT TIER...

Runs the attester (tools/attest-bench.py) on OUT/<tier>/report.json for each
tier, writes its verdict next to the receipt, prints a table, and exits 1 if
a tier failed. Standard library only.
"""

import json
import pathlib
import subprocess
import sys

ATTESTER = pathlib.Path(__file__).resolve().parent.parent / "attest-bench.py"


def row(out: pathlib.Path, tier: str) -> tuple[bool, str]:
    receipt = out / tier / "report.json"
    if not receipt.exists():
        return False, f"| {tier} | — | — | — | — | — | fail: no receipt (see `{tier}.log`) |"
    attested = subprocess.run([sys.executable, str(ATTESTER), str(receipt), "--tier", tier], capture_output=True, text=True)
    (out / tier / "verdict.json").write_text(attested.stdout)
    verdict = json.loads(attested.stdout)
    report = json.loads(receipt.read_text())
    frames = report["frame_ms"]
    drawn = "×".join(map(str, report.get("rendered", report["resolution"])))
    outcome = "pass" if verdict["verdict"] == "pass" else "fail: " + "; ".join(verdict["reasons"])
    cells = [
        tier,
        report["machine"]["gpu"],
        drawn,
        f"{frames['mean']} ms ({report['fps_mean']} fps)",
        f"{frames['p95']} ms",
        f"{report['over_budget_percent']} % of frames over {report['budget_ms']} ms",
        outcome,
    ]
    return verdict["verdict"] == "pass", "| " + " | ".join(cells) + " |"


def main() -> int:
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    out, tiers = pathlib.Path(sys.argv[1]), sys.argv[2:]
    print("| Tier | GPU | Drawn at | Mean | p95 | Over budget | Verdict |")
    print("|---|---|---|---|---|---|---|")
    passed = True
    for tier in tiers:
        ok, line = row(out, tier)
        passed &= ok
        print(line)
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
