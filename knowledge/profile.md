---
type: Specification
title: osage engineering profile of OKF
description: How osage records the knowledge behind its modules (architecture, algorithms, techniques and their proofs) as an OKF v0.2 bundle that people, agents and other engines can reuse.
status: draft
tags: [okf, profile, documentation]
sources:
  - id: okf
    resource: https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md
    title: Open Knowledge Format v0.2
  - id: okf-federation
    resource: https://github.com/manaty/okf-federation/blob/main/SPEC.md
    title: OKF Federation 0.1-draft
generated: { by: "claude-code/claude-fable-5-2", at: "2026-11-10T06:45:00Z" }
---

# Purpose

Ideas, architectures and algorithms are as valuable as code, and travel further: anyone may reuse an idea, in any engine and any language, where code is bound to its licence and its language. osage therefore keeps the knowledge behind each module in a form that can be read **without reading the code**, and checked by a machine.

This profile is version **0.1**. It adds types, fields and sections to OKF v0.2[^okf]; it removes nothing, so the bundle stays a conformant OKF bundle and a valid member bundle of an OKF Federation[^okf-federation].

# Types

| `type` | One card per | Where |
|---|---|---|
| `Module` | crate, package or world | `modules/` |
| `Technique` | rendering technique: one plugin of `osage-render` | `techniques/` |
| `Algorithm` | algorithm or mechanism worth reusing elsewhere | `algorithms/` |
| `Attested Computation` | oracle: the sanctioned check that proves a claim (OKF §10) | `oracles/` |
| `Specification` | this profile | bundle root |

Decisions, surveys and design notes still live in `design/` and `research/` of the repository; they join the bundle as `Decision` and `Reference` cards in a later version.

# Fields

Besides OKF's `type`, `title`, `description`, `tags`, `status`, `generated`, `verified` and `stale_after`:

| Field | On | Meaning |
|---|---|---|
| `resource` | Module, Technique, Algorithm | Required. The code the card describes: an absolute URL into the repository. The checker verifies that the path exists. |
| `licence` | Module, Technique, Algorithm | Required. SPDX expression of that code. The knowledge itself is CC BY-SA 4.0, like all osage documentation; an idea is free to reuse. |
| `sources` | Technique, Algorithm | Required, as in OKF §5.1. Where the idea comes from: papers, talks, documentation, or osage's own design. This is the record that the work has **clean sources**. |
| `tiers` | Technique | Fidelity tiers the technique serves, for example `[T2, T3, T4]`. |

# Sections

Headings a card of each type has, in this order. A section with nothing to say states so in one line.

- **Module:** `# Purpose`, `# Concepts`, `# Architecture`, `# Interfaces`, `# Invariants`, `# Knowledge` (links to its techniques, algorithms and oracles), `# Reuse`, `# Limits`.
- **Technique:** `# What it does`, `# How`, `# Settings per tier`, `# Cost`, `# Limits`, `# Oracle`, `# Reuse`.
- **Algorithm:** `# Problem`, `# Idea`, `# Method`, `# Invariants`, `# Cost`, `# Failure modes`, `# Alternatives`, `# Oracle`, `# Reuse`.
- **Attested Computation:** the contract fields of OKF §10 (`runtime`, `parameters`, `executor`, `attester`) and `# Computation`.

`# Method` and `# How` are written so that someone could implement the idea in another engine: steps, formulas, pseudocode — not a tour of osage's source.

# Rules

1. **One concept per file.** The file name is the concept, in lower case with hyphens.
2. **A card changes with its code.** A change to a module that makes its card wrong updates the card in the same change; `generated.at` moves, and a `verified` entry that no longer holds is removed.
3. **Measured claims say how.** A cost names the machine, the resolution and the date, and links the oracle that measures it.
4. **Agents propose, people verify.** An agent writes itself in `generated.by` and never writes a `human:` verification. Absence of `verified` means unverified.
5. **Clean sources.** A card cites public material only; nothing learnt from the decompilation of a commercial game ([CONTRIBUTING](https://github.com/smichea/osage/blob/main/CONTRIBUTING.md#clean-sources)).
6. **English**, plain words, one idea per sentence.

# Checking

`python3 tools/check-knowledge.py` checks OKF conformance and this profile: front matter, required fields and sections, links between cards, and that every `resource` in this repository exists. Continuous integration runs it.

# Federation

The bundle is the directory `knowledge/` of the repository; its entry point is [index.md](/index.md). A federation registers it as:

```yaml
members:
  - id: osage
    repository: https://github.com/smichea/osage
    ref: main
    bundle: knowledge
    entrypoint: index.md
```

Registering it is the federation's choice; osage requires none.

[^okf]: Open Knowledge Format v0.2
[^okf-federation]: OKF Federation 0.1-draft
