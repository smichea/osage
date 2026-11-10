# Contributing to osage

Contributions are welcome from people and from AI agents working for a responsible person: code, and corrections, counter-arguments and new sources for `research/` and `design/`. Code contributions are open since the additional permissions in [`LICENSE-EXCEPTIONS.md`](LICENSE-EXCEPTIONS.md) became final ([D-027](design/decisions.md#d-027--the-additional-permissions-are-final-code-contributions-are-open)).

## Developer Certificate of Origin

osage uses the [Developer Certificate of Origin 1.1](https://developercertificate.org/) (DCO), not a CLA: you keep your copyright, and you certify that you have the right to submit your contribution under the project's licences.

Sign off every commit:

```sh
git commit -s
```

which adds a line such as:

```
Signed-off-by: Your Name <you@example.org>
```

**AI agents never add a `Signed-off-by` line.** Commits produced with an AI agent are signed off by the **responsible person**, after reviewing the change and checking that it copies no third-party code; credit the agent with a `Co-Authored-By:` trailer for provenance. Never name an AI in a copyright notice.

## Licences of what you contribute

| Part | Licence |
|---|---|
| Engine, server, tools, device bridge | AGPL-3.0-or-later WITH the additional permissions of [`LICENSE-EXCEPTIONS.md`](LICENSE-EXCEPTIONS.md) |
| Protocol specification text | Community Specification License 1.0 |
| `.proto` schemas, spec examples | Apache-2.0 |
| Client SDKs, GPU Plugin / Platform / World Module interface bindings (`osage-world-module-api`) | MIT OR Apache-2.0 |
| Example worlds (`worlds/`, e.g. the kart world) | MIT OR Apache-2.0 |
| Documentation (`research/`, `design/`, …) | CC BY-SA 4.0 |

See the [decision log](design/decisions.md) and the [licensing review](research/06-licensing-review.md) for the reasons.

**Rules that keep these licences working:**

- <!-- REUSE-IgnoreStart -->Your contribution is licensed under the licence stated in each file's header, **including the additional permissions**. Every code file carries an SPDX header, `SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions` for engine code (the AGPL with the osage additional permissions — `AGPL-3.0-or-later WITH AdditionRef-osage-exceptions-1.0` in SPDX 3.0 terms, written as a REUSE 3.3 custom licence); files without one are not accepted.<!-- REUSE-IgnoreEnd -->
- Do not add third-party GPL or AGPL code unless its authors grant the same additional permissions.
- AGPL code never moves into the specification or SDK directories.
- Never commit proprietary vendor headers or binaries (GPU, platform or console SDKs); they are reached only through the documented interfaces.

## Clean sources

osage is written from public knowledge and its contributors' own work ([research/07](research/07-zelda-reverse-engineering.md)).

- **Never bring in code or content from a commercial game**: nothing decompiled, recompiled, extracted, leaked or copied from a port built on those — code, assets, level data, SDK headers or names. A decompilation repository's own licence covers its contributors' rights only, never the game's.
- **Learn techniques from talks, papers, documentation and your own experiments.** If you worked on the decompilation or a port of a game, do not write the equivalent part of osage from memory; say so and let someone else take the task.
- **Third-party code** enters only with a licence compatible with the file's licence (`cargo deny` checks dependencies) and with its attribution kept.
- **Content** (models, textures, sounds) enters only with a known free licence, listed in the asset manifest ([design/03 §5](design/03-rendering.md#5-content)).
- AI agents follow the same rules and do not fetch such sources while writing code ([AGENTS.md](AGENTS.md)). Some projects refuse AI-written contributions: respect their policy when contributing upstream.
- In doubt, ask a maintainer before writing.

## Tasks and their proof

Work is cut into tasks small enough for one person or agent, each with an **oracle**: the check a machine can run to say it is done — a test, a budget held on the bench, a reference image, a determinism hash ([design/03 §3](design/03-rendering.md#3-oracles-how-done-is-decided)). Say in the pull request which oracle you ran and what it printed.

## Knowledge cards

An idea is worth as much as its code, and travels further. Each module, rendering technique and reusable algorithm has a card in [`knowledge/`](knowledge/index.md) that explains it well enough to rebuild it elsewhere: the problem, the method, the invariants, the cost, the limits, and the oracle that proves it. The format is the [osage profile](knowledge/profile.md) of the Open Knowledge Format.

- A change that makes a card wrong updates the card in the same pull request; a new module, technique or algorithm comes with its card.
- A card cites where its idea comes from: papers, talks, documentation, or osage's own design. This is also the record of its [clean sources](#clean-sources).
- Whoever writes a card signs it in `generated`; a person who has checked it against the code adds themselves under `verified`. Agents never write a person's verification.
- `python3 tools/check-knowledge.py` checks the bundle; continuous integration runs it.

## Research and design notes

- Every factual claim carries a source; claims you could not verify are marked as such.
- Surveys are dated snapshots: add a new dated note rather than silently rewriting an old one.
- A change to a decision is a new entry in the decision log that supersedes the old one.
- The project's position on its name and on the Osage Nation is in the README; contributions must not use Osage symbols, imagery, language, stories or other cultural elements.
