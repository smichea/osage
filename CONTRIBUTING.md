# Contributing to osage

Contributions are welcome from people and from AI agents working for a responsible person. osage is in its research phase: corrections, counter-arguments and new sources for `research/` and `design/` are the most useful contributions today.

> **Code contributions are accepted from the maintainer only** until the additional permissions in [`LICENSE-EXCEPTIONS.md`](LICENSE-EXCEPTIONS.md) are final ([D-024](design/decisions.md#d-024--code-starts-before-the-lawyers-review-maintainer-only-contributions-until-then)). Documentation contributions are welcome now.

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
| Client SDKs, GPU Plugin / Platform / World Module interface bindings | MIT OR Apache-2.0 |
| Documentation (`research/`, `design/`, …) | CC BY-SA 4.0 |

See the [decision log](design/decisions.md) and the [licensing review](research/06-licensing-review.md) for the reasons.

**Rules that keep these licences working:**

- <!-- REUSE-IgnoreStart -->Your contribution is licensed under the licence stated in each file's header, **including the additional permissions**. Every code file carries an SPDX header, `SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions` for engine code (the AGPL with the osage additional permissions — `AGPL-3.0-or-later WITH AdditionRef-osage-exceptions-1.0` in SPDX 3.0 terms, written as a REUSE 3.3 custom licence); files without one are not accepted.<!-- REUSE-IgnoreEnd -->
- Do not add third-party GPL or AGPL code unless its authors grant the same additional permissions.
- AGPL code never moves into the specification or SDK directories.
- Never commit proprietary vendor headers or binaries (GPU, platform or console SDKs); they are reached only through the documented interfaces.

## Research and design notes

- Every factual claim carries a source; claims you could not verify are marked as such.
- Surveys are dated snapshots: add a new dated note rather than silently rewriting an old one.
- A change to a decision is a new entry in the decision log that supersedes the old one.
- The project's position on its name and on the Osage Nation is in the README; contributions must not use Osage symbols, imagery, language, stories or other cultural elements.
