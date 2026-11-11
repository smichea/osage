---
type: Attested Computation
title: Physics golden file
description: Proves that the kart physics gives the same bits natively and in WebAssembly, which exact prediction relies on.
status: draft
tags: [oracle, physics, determinism]
runtime: shell
parameters: []
executor:
  resource: https://github.com/smichea/osage/blob/main/.github/workflows/ci.yml
  receipt: [exit_status, output]
attester:
  resource: https://github.com/smichea/osage/blob/main/worlds/kart/physics/tests/golden.rs
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-12T00:30:00Z" }
---

# Computation

    cargo test -p osage-kart-physics --test golden
    npm test -w @osage/kart-client

Both commands run the same script: 1200 physics steps on a small rectangular track, with controls that change every step and are exactly representable in both languages. Every 60 steps, the nine numbers of the kart's state are written as the hexadecimal of their bits.

The first command also runs a second script, natively only (contacts between karts are the server's): four karts on the same track for 900 steps, driven into one another and into the barriers, their states written every 30 steps to `golden-contacts.txt`.

- The first command runs it natively and compares with `worlds/kart/physics/tests/golden.txt`.
- The second runs it through the WebAssembly build, from JavaScript, and compares with the same file.

The attester is the comparison itself: any difference, down to the last bit, fails the test.

# What it proves

That the server (native) and a browser (WebAssembly) compute identical kart states from identical controls. [Exact client prediction](/algorithms/exact-client-prediction.md) is only exact if this holds.

# When the physics changes on purpose

`UPDATE_GOLDEN=1 cargo test -p osage-kart-physics --test golden` rewrites the files; the change to `golden.txt` is then part of the review, and both builds must still agree on it.

# Used by

[osage-kart-physics](/modules/kart-physics.md).
