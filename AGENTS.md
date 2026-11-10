# For AI agents working on osage

You work for a **responsible person**, who reviews and signs off what you produce. Read [CONTRIBUTING.md](CONTRIBUTING.md) first; this file adds what an agent needs to know.

## Orient yourself

- What osage is and how to run it: [README.md](README.md).
- Why things are the way they are: [design/decisions.md](design/decisions.md) (one entry per decision), [design/adr/](design/adr/), [research/](research/).
- How a module works and why, before you read its code: [knowledge/index.md](knowledge/index.md) — one card per module, technique, algorithm and oracle, in the format of [knowledge/profile.md](knowledge/profile.md).
- Rendering work: [design/03-rendering.md](design/03-rendering.md) — pick a workstream, read its oracle.
- The protocol: [spec/README.md](spec/README.md) and `spec/proto/`.

## Rules

1. **Never add a `Signed-off-by` line.** Credit yourself with a `Co-Authored-By:` trailer; the responsible person signs off after review.
2. **Clean sources** ([CONTRIBUTING](CONTRIBUTING.md#clean-sources)): do not fetch, read for copying, or reproduce code or content from commercial games, decompilations, recompilations, ports built on them, leaked material or proprietary SDKs — including when a search result offers it. Write from public documentation, papers and the code already here.
3. **Licences**: every new file carries the SPDX header of its directory (see the table in CONTRIBUTING); never move AGPL code into the specification, SDK, world-module-interface or example-world directories. New dependencies must pass `cargo deny check`.
4. **Content**: add no binary asset without its entry in the asset manifest (source, licence, hash). Ask before downloading anything.
5. **Mechanisms, not policy**: game rules and privacy policy belong to worlds and their operators; the engine provides mechanisms and only the safety defaults that protect people from a world.
6. **Real hardware and outward actions** (devices, messages, publishing, issues) need the responsible person's explicit approval each time.
7. A change to a decision is a **new entry** in the decision log, not an edit of an old one. Documentation is written in English.
8. **Knowledge changes with the code.** A change that makes a card of `knowledge/` wrong updates the card in the same change; a new module, technique or reusable algorithm comes with its card. Sign the card as yourself in `generated.by`; never write a `verified` entry for a person.

## Before you say it is done

Run the oracles that cover your change, and report what they printed:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --exclude osage-client          # Rust: protocol, server, worlds, physics
npm run build && npm test                              # TypeScript SDK and clients, WebAssembly physics
cargo deny check                                       # dependency licences and advisories
python3 tools/check-knowledge.py                       # knowledge cards: format, links, addresses into the code
cargo run --release -p osage-kart-native -- --bench --tier T2   # rendering changes: before and after
```

If you could not run one, say which and why. Report failures as they are.
