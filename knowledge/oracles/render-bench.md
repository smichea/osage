---
type: Attested Computation
title: The render bench
description: Proves that a fidelity tier holds its frame-time budget on a machine, and records what was drawn.
status: draft
tags: [oracle, rendering, benchmark]
runtime: shell
parameters:
  - { name: tier, type: string, required: true }
executor:
  resource: https://github.com/smichea/osage/blob/main/bench/README.md
  receipt: [bench, tier, camera, without, commit, machine, resolution, rendered, assets, frames, frame_ms, budget_ms, over_budget_percent, screenshots]
attester:
  resource: https://github.com/smichea/osage/blob/main/tools/attest-bench.py
generated: { by: "claude-code/claude-fable-5-2", at: "2026-11-10T06:45:00Z" }
---

# Computation

    cargo run --release -p osage-kart-native -- --bench --tier {tier}

The run writes `report.json` and four screenshots; `report.json` is the receipt. How the bench makes every machine draw the same frames is described in [timedemo bench](/algorithms/timedemo-bench.md).

# What it proves

A claim such as "T2 holds 60 frames a second on an integrated GPU" is accepted with a receipt, not on a statement. The attester reads the receipt and answers **pass** only if:

- it comes from the sanctioned bench (`bench` is `kart-lap/2`: version 2 lets karts overtake instead of running into each other, so its frames differ from version 1) run for the claimed tier;
- the camera followed the race (`camera` is `race`), not held at a place;
- no technique was left out and at least 2000 frames were timed;
- it was run from a committed state of the code (`commit` does not end in `-dirty`);
- the mean frame time is within the tier's budget.

It also reports the share of frames over budget, which the mean hides.

    python3 tools/attest-bench.py bench/out/<run>/report.json

# What it does not prove

That the picture is right: compare the screenshots, which are taken at the same frames in every run. That another machine does as well: a receipt names its GPU and driver. Times taken while other programs run are pessimistic.

# Used by

Every technique of [osage-render](/modules/osage-render.md), for its cost and for its tier's budget.
