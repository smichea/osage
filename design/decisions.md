# Decision log

Decisions taken by the maintainer, in order. Each one closes an open decision from the [synthesis](../research/00-synthesis.md#open-decisions) or a design note. A decision is changed only by a new entry that supersedes it.

## D-001 — Bevy is the base of the world server and the native client

Date: 2026-11-09

- **Decision:** the world server (headless) and the reference native client are built on **Bevy + wgpu**. osage contributes upstream what belongs in Bevy (rendering, GPU-driven features) and builds its own value on top (protocol, streaming, server meshing, tiers, agents, identity, devices, tools).
- **Why:** Bevy's ECS plugin model fits many parallel human + agent teams (G3); it has the most modern open rendering trajectory — GPU-driven rendering, meshlet virtual geometry, Solari ray-traced lighting (G1); the same code runs as a headless server, and the Bevy Remote Protocol already exposes the world to agents (G4).
- **Accepted costs:** no editor (osage builds world tools as protocol/BRP clients), breaking Bevy releases every 3–4 months (one pinned Bevy version per osage release, scheduled upgrades), a weaker low-end path than Godot (low tiers are designed explicitly).
- **Alternatives:** Godot (better editor, community and low-end/web today, but no GPU-driven rendering or virtual geometry, and reaching the RAGE bar would mean rewriting its renderer core); an in-house engine on wgpu (full control, but years of rebuilding before touching what is actually missing).

## D-002 — Browser client: TypeScript SDK + three.js first, Bevy-wasm later

Date: 2026-11-09

- **Decision:** the first browser client is a **TypeScript SDK of the osage protocol with a three.js renderer** — the stack 2030workspace already uses. A **Bevy-wasm** build comes later as a "rich" web client.
- **Why:** light bundle, WebGPU with a WebGL2 fallback in one build (about 1 user in 5 still needs WebGL2), large ecosystem, and the first world (2030workspace) already runs on it.
- **Accepted cost:** two client rendering implementations (TypeScript and Rust); the protocol and the content format are the contract that keeps them consistent.
- **Alternatives:** Bevy-wasm only (single codebase, but heavy wasm, single-threaded on the web, separate WebGL2/WebGPU builds, experimental web WebGPU); three.js only (permanent split from the native client).

## D-003 — Licences: open protocol and permissive SDKs, AGPL engine and server

Date: 2026-11-09

- **Decision:**
  - **protocol specification:** open, under **Apache-2.0** (its patent grant protects every implementer);
  - **client SDKs** (Rust, TypeScript, later others): **MIT OR Apache-2.0**;
  - **server, engine, tools:** **AGPL-3.0**;
  - **documentation:** CC BY-SA 4.0 (unchanged).
- **Why:** any client — including proprietary apps and apps distributed through the iOS App Store, whose terms conflict with the GPL family — can join an osage world, which directly serves G4; iPhone AR needs a native app. The worlds themselves (servers) and the engine stay copyleft. This is the Matrix model: open spec and Apache SDKs, AGPL reference server.
- **Alternatives:** SDKs under MPL-2.0 (file-level copyleft); everything AGPL (no proprietary clients, no iOS app without an exception from every contributor).

## D-004 — In-house persistence

Date: 2026-11-09

- **Decision:** region servers (in-memory Bevy ECS) are authoritative; durable state is an **event log plus periodic snapshots in PostgreSQL**; content lives in object storage. No SpacetimeDB dependency.
- **Why:** everything stays AGPL-compatible and self-hostable; the event log also gives replay and audit (useful for sanctions, agents' gated actions and the time travel 2030workspace already offers).
- **Alternatives:** SpacetimeDB (BSL 1.1 — not open source until each version converts to AGPL four years later); a storage interface with an optional SpacetimeDB adapter (more abstraction work up front).
