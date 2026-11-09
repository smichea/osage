# Contributing to osage

Contributions are welcome from people and from AI agents working for a responsible person. osage is in its research phase: corrections, counter-arguments and new sources for `research/` and `design/` are the most useful contributions today.

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

Commits produced with an AI agent are signed off by the **responsible person**, who certifies the DCO for them; credit the agent with a `Co-Authored-By:` trailer if you wish.

## Licences of what you contribute

| Part | Licence |
|---|---|
| Engine, server, tools | AGPL-3.0, with the additional permissions of [D-005](design/decisions.md#d-005--dco-for-contributions-agpl-exceptions-written-before-the-first-code) |
| Protocol specification | Apache-2.0 |
| Client SDKs | MIT OR Apache-2.0 |
| Documentation (`research/`, `design/`, …) | CC BY-SA 4.0 |

See the [decision log](design/decisions.md) for the reasons.

## Research and design notes

- Every factual claim carries a source; claims you could not verify are marked as such.
- Surveys are dated snapshots: add a new dated note rather than silently rewriting an old one.
- A change to a decision is a new entry in the decision log that supersedes the old one.
- The project's position on its name and on the Osage Nation is in the README; contributions must not use Osage symbols, imagery, language, stories or other cultural elements.
