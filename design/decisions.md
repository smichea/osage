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

Date: 2026-11-09 · **Partly superseded** by D-020 (AGPL-3.0-or-later) and D-023 (spec under the Community Specification License 1.0).

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

## D-005 — DCO for contributions, AGPL exceptions written before the first code

Date: 2026-11-09 · **Wording superseded:** the permissions now live in [`LICENSE-EXCEPTIONS.md`](../LICENSE-EXCEPTIONS.md) (osage-exceptions-1.0, §A–§D), revised after the [licensing review](../research/06-licensing-review.md) (D-020 to D-022) and final since D-027.

- **Decision:** contributions are accepted under the **Developer Certificate of Origin** (every commit signed off, no copyright assignment — see [CONTRIBUTING.md](../CONTRIBUTING.md)). Two **additional permissions under AGPL-3.0 section 7** are added to the engine's licence **before the first line of code is committed**:
  1. **optional GPU vendor plugins** — binary-only upscaling, frame-generation or denoising libraries (DLSS, FSR 4, XeSS…) may be loaded at run time through a defined plugin interface, provided osage stays fully functional with open implementations when they are absent;
  2. **app-store distribution** — osage client applications may be distributed through app stores whose terms would otherwise conflict with the AGPL, provided the corresponding source stays freely available to every recipient.
- **Why:** without a copyright assignment, an exception can only be added later with the agreement of every contributor, so it has to exist from the start. After that, the licence is locked, which protects the community against a later relicensing.
- **Caveats:** a vendor's own licence may still forbid the combination (NVIDIA's RTX SDK licence forbids use that would subject the SDK to an open-source licence) — checked vendor by vendor. The final wording below is a **draft that needs a legal review** before the first code commit.
- **Alternatives:** a CLA (allows later relicensing, but deters contributors and concentrates power — the mechanism behind the HashiCorp and Elastic relicensings); pure AGPL with no exception (no proprietary GPU plugin, no AGPL app on the iOS App Store).

<details>
<summary>Draft wording of the two additional permissions (to be reviewed)</summary>

> **Additional permission 1 — Optional GPU vendor plugins.** As an additional permission under section 7 of the GNU Affero General Public License version 3, you may convey osage combined with a GPU Vendor Plugin, provided that (a) the plugin is loaded at run time through the osage GPU plugin interface and is not required for osage to run; (b) osage remains fully functional, using open-source implementations of the same function, when the plugin is absent; and (c) the plugin's own licence permits such use. "GPU Vendor Plugin" means an upscaling, frame-generation or denoising library published by a GPU hardware vendor and distributed only in binary form. The Corresponding Source of such a combination does not include the GPU Vendor Plugin.
>
> **Additional permission 2 — Application stores.** As an additional permission under section 7 of the GNU Affero General Public License version 3, you may convey osage client applications through an application store whose terms of service impose restrictions that would otherwise conflict with this License, provided that the Corresponding Source of the conveyed version is made available, free of charge and under this License, to every recipient, from a publicly accessible location referenced in the application's description or "about" screen.

</details>

## D-006 — Default identity levels: A2 for scarce privileges, A3 to take value out

Date: 2026-11-09

- **Decision:** the default world policy of [design/01 §2.1](01-accounts-and-agents.md#21-assurance-levels-world-facing) is adopted: visiting, chatting and sandbox building at **A0**; persistent avatar and inventory at **A1**; trading, owning land or rare assets, ranked play and running agents at **A2** (unique human, community-attested); withdrawing value out of the world, moderation powers and vouching for others at **A3** (documented) or a world-specific level. Every world can change these thresholds.
- **Why:** strong protection where fraud pays, without excluding people who have no documents or refuse to show them.
- **Alternatives:** A3 for every scarce privilege (strongest, excludes people without documents); A1 by default with uniqueness opt-in per world (multi-accounts by default).

## D-007 — Cross-world sanctions only within voluntary federations

Date: 2026-11-09

- **Decision:** worlds may form **federations** that share a scope for per-world identifiers and sanctions. A person sees, when joining a world, which federation it belongs to. Outside a federation, nothing correlates a person across worlds.
- **Why:** deters bad actors who hop between related worlds without creating a global blacklist.
- **Alternatives:** never share (bad actors move freely); global sharing by default (global blacklist, abuse risk, correlation of people across all worlds).

## D-008 — Game rules for agents belong to each world, not to osage

Date: 2026-11-09

- **Decision:** osage does **not** decide where agents may play (ranked modes or not) nor how many each person may run. It provides the **mechanisms** — delegation credentials with scopes, a controller kind (`human` / `agent` / `system`) always carried by the protocol, quota enforcement in the agent gateway, per-world policy hooks — and **each world sets its own rules**.
- **Why:** these are game-design choices, not engine choices.
- **Closes:** [design/01 §5](01-accounts-and-agents.md#5-open-questions) questions 3 and 4.

## D-009 — Physical actuation: risk classes, read-only by default, the owner decides

Date: 2026-11-09

- **Decision:** osage defines the risk classes for acting on a real device — **read-only, pre-approved, gated (human approval receipt), on-site operator only, forbidden** — and the device bridge ships **read-only**. **Only the device's owner raises a class; a world never can.** The local watchdog and emergency stop are always active, whatever the class.
- **Why:** physical safety is enforced next to the device (EU Machinery Regulation 2023/1230 from 2027, drone line-of-sight rules); a world, compromised or not, must not be able to widen what a device does.
- **Alternatives:** classes with no default (the owner configures everything at install); defaults per OpenDeviceDB device category (more convenient, but osage would take a position device by device).
- **Closes:** [design/02 §9](02-mixed-worlds-and-devices.md#9-open-questions) question 1.

## D-010 — Privacy of real cameras and captured places: osage imposes nothing

Date: 2026-11-09

- **Decision:** osage imposes **no privacy policy** on real cameras, microphones or captured places. It provides the **mechanisms** — per-stream permissions set by the owner, server-side interest management, a recording/streaming state carried by the protocol — and **world operators and device owners decide** how to use them.
- **Why:** consistent with D-008: policy belongs to the people who run worlds and own devices, who are also the ones legally responsible (a shared world likely falls outside the GDPR household exemption, making them data controllers).
- **Alternatives:** private by default with visible signalling and no facial recognition imposed by osage; private by default only.
- **Closes:** [design/02 §9](02-mixed-worlds-and-devices.md#9-open-questions) question 5.

## D-011 — Device contracts live in OpenDeviceDB

Date: 2026-11-09

- **Decision:** the device contracts (video frames, calibration, measurements, 3D scenes, actions, and the future continuous-control contract) are **specified and versioned in OpenDeviceDB**. osage consumes them and contributes to them; it keeps no device vocabulary of its own. The continuous-control contract for drones and robots is written jointly there.
- **Why:** one vocabulary for every project that talks to devices; OpenDeviceDB already defines the contracts its components use.
- **Alternatives:** contracts in osage with converters to OpenDeviceDB (two vocabularies); a neutral third repository (one more governance).
- **Closes:** [design/02 §9](02-mixed-worlds-and-devices.md#9-open-questions) question 2 (where the contracts live).

## D-012 — OpenDeviceDB publishes WoT Thing Descriptions

Date: 2026-11-09

- **Decision:** OpenDeviceDB publishes a **W3C WoT Thing Description 1.1** for its device records and components; osage, Eclipse Ditto, node-wot and other tools read them directly.
- **Why:** the mapping lives where the contracts are defined (D-011), and every WoT tool benefits, not only osage.
- **Alternatives:** conversion inside the osage bridge (only osage benefits); no WoT for now.
- **Closes:** [design/02 §9](02-mixed-worlds-and-devices.md#9-open-questions) question 6.

## D-013 — Identity: a generic verifier, tested with two different providers before phase 2

Date: 2026-11-09

- **Decision:** osage implements a **generic verifier of W3C Verifiable Credentials presented with OpenID for Verifiable Presentations** (the mechanism); **each world chooses the issuers it trusts** (the policy). Before phase 2 it is proven with **two providers of different natures** plus a local test provider: **nexlink** (social validation; levels mapped by D-025) and a **European Digital Identity Wallet or a national eID** for A3.
- **Why:** an interface tested against a single provider ends up shaped by it, which would contradict "reference, not imposed" ([design/01](01-accounts-and-agents.md)).
- **To study:** how to derive a stable per-world identifier from an EUDI wallet presentation without the world ever seeing civil identity.
- **Alternatives:** generic verifier + nexlink only (A3 later, risk of a nexlink-shaped interface); a direct nexlink API adapter first (fastest for phase 1, throw-away work later).
- **Closes:** [design/01 §5](01-accounts-and-agents.md#5-open-questions) question 5.

## D-014 — AGPL device bridge; incompatible components run out of process

Date: 2026-11-09

- **Decision:** the device bridge stays **AGPL-3.0**. Components whose licence is compatible with an AGPL host — MIT, BSD, Apache-2.0, LGPL, GPL-3.0, GPL-2.0-or-later, MPL-2.0 without an "incompatible with secondary licenses" notice — run **inside** the bridge's WASM runtime. Others — notably **GPL-2.0-only**, EPL-1.0, licences with non-commercial clauses — run **in a separate process** that speaks OpenDeviceDB contracts with the bridge. The bridge chooses the mode from the licence (SPDX expression) declared in each OpenDeviceDB component manifest.
- **Why:** a WASM module executed in the bridge and calling its host functions is, conservatively, one combined work with it (untested legally for WASM); an arm's-length exchange of contract messages between separate processes keeps the programs separate, so no device is excluded for licence reasons.
- **Caveat:** to be confirmed by the same legal review as D-005.
- **Corrected after the [licensing review](../research/06-licensing-review.md#4-d-014--in-process-vs-out-of-process-components) (2026-11-09):**
  - **in process:** 0BSD, BSD-2/3-Clause, ISC, Zlib, MIT, Apache-2.0; LGPL-2.1-only and LGPL-3.0; GPL-3.0 and AGPL-3.0; MPL-2.0 unless "Incompatible With Secondary Licenses"; EUPL-1.2; EPL-2.0 only with the Exhibit A secondary-licence notice; CC BY-SA 4.0;
  - **out of process:** BSD-4-Clause, EPL-1.0, EPL-2.0 without the notice, CDDL, GPL-2.0-only (including `WITH Classpath-exception-2.0`), non-free licences; **unknown, `NOASSERTION` or `LicenseRef-` licences by default**;
  - whole SPDX expressions are evaluated (`AND`: every part must fit; `OR`: at least one);
  - the **out-of-process runner is a separate MIT-licensed project** containing no AGPL code (not even OpenDeviceDB's AGPL host code), with GPL-2.0-compatible host functions (wasmtime is `Apache-2.0 WITH LLVM-exception`; take the MIT side of osage SDKs);
  - running a non-commercial component out of process does not lift its terms: osage does not redistribute such components.
- **Alternatives:** a permissive bridge (MIT OR Apache-2.0: loads any free component, but a vendor could embed it in a closed hub without publishing improvements, including to the safety envelope); AGPL bridge loading compatible components only (devices with GPL-2.0-only code stay out).
- **Closes:** [design/02 §9](02-mixed-worlds-and-devices.md#9-open-questions) question 3.

## D-015 — OpenDeviceDB compositions in a world: outputs as entities, optional panel

Date: 2026-11-09

- **Decision:** a composition **runs where its data is** — on the owner's bridge, or in a content-pipeline worker if the world operator allows it. Its **typed outputs** (scene revisions, measurements, video…) become **components of world entities**, usable by every client and agent. Clients able to show web UI (browser, desktop) may also display the composition's **interface components in an isolated panel**. The link is a small **placement manifest**: composition id, version and digest, anchor in the world, port → entity-component mapping, permissions.
- **Why:** every client tier and every agent gets the results; third-party UI code only runs where it can be isolated; OpenDeviceDB's existing interfaces are not lost.
- **Alternatives:** outputs as entities only (no third-party UI code at all, but OpenDeviceDB's interfaces are lost); the full composition as a world object (richest, but third-party code in every client and nothing for low tiers or agents).
- **Closes:** [design/02 §9](02-mixed-worlds-and-devices.md#9-open-questions) question 4.

## D-016 to D-019 — Topology and protocol v0

Date: 2026-11-09 · Details and reasons: [ADR-0002](adr/0002-protocol-v0.md#1-decisions), architecture: [ADR-0001](adr/0001-architecture.md)

- **D-016 — Replication gateway:** clients connect to a gateway that merges the streams of the regions their area touches; they never connect to region servers. *Alternative:* direct connection to regions with ghosts and signed handoff tickets.
- **D-017 — osage specifies its own protocol**, taking ideas but not the wire format from bevy_replicon and lightyear. *Alternative:* adopt lightyear's format.
- **D-018 — Protocol Buffers with the canonical JSON mapping** for agents and tools. *Alternatives:* CBOR + CDDL; FlatBuffers.
- **D-019 — Self-describing worlds:** the world sends its component schemas at join. *Alternative:* compiled-in schemas only.

## D-020 — AGPL-3.0-or-later

Date: 2026-11-09

- <!-- REUSE-IgnoreStart -->**Decision:** the engine, server, tools and bridge are licensed **AGPL-3.0-or-later** with the additional permissions of `LICENSE-EXCEPTIONS.md`: `SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions` in every file header (SPDX 3.0 equivalent: `AGPL-3.0-or-later WITH AdditionRef-osage-exceptions-1.0`; REUSE 3.3 rejects `AdditionRef-` licence files, so the AGPL and the permissions are combined in one custom licence file).<!-- REUSE-IgnoreEnd -->
- **Why:** consistent with OpenDeviceDB (also -or-later); a future AGPL version can be adopted without contacting every contributor.
- **Alternative:** AGPL-3.0-only.

## D-021 — World modules permission (§C)

Date: 2026-11-09

- **Decision:** a third additional permission lets **the logic specific to a world** — loaded only through the documented World Module Interface (e.g. WebAssembly modules in region servers), with permissively licensed bindings — be **under any licence**. The engine and every modification of it remain AGPL, including the section 13 source offer to every player and agent.
- **Why:** consistent with D-008 and D-010 — each world decides, including whether its game logic is open; it lets studios build closed worlds on an open engine whose improvements stay shared. Permissions can be removed later but never added, so it had to be decided now.
- **Consequence:** the World Module Interface bindings are a permissive crate (like the SDKs).
- **Alternative:** world logic under the AGPL (open worlds by construction; studios wanting closed logic would not use the osage server).

## D-022 — Platform components permission (§D)

Date: 2026-11-09

- **Decision:** a fourth additional permission allows combining osage with **proprietary platform components** (Steamworks, console SDKs under NDA, store and payment SDKs) through a documented Platform Interface, provided the work stays usable on at least one platform without them; the platform-specific adapter's source may be withheld **only** as far as the platform's confidentiality terms require. §B's definition of application stores also covers Steam and console stores.
- **Why:** opens Steam and, one day, consoles (out of scope for v0 in ADR-0001, not excluded forever); the asymmetry — removable later, never addable — argues for including it now.
- **Alternative:** GPU plugins and app stores only (no Steamworks in the official AGPL client, consoles closed for good).

## D-023 — Protocol specification under the Community Specification License 1.0

Date: 2026-11-09

- **Decision:** the **text of the protocol specification** is licensed under the **Community Specification License 1.0**; the `.proto` schemas and example code stay **Apache-2.0**; the client SDKs stay MIT OR Apache-2.0.
- **Why:** Apache-2.0's patent grant covers the spec text, not independent implementations; the Community Specification License grants patent rights "for Licensee's Implementation", so every independent client and server is covered.
- **Alternatives:** Apache-2.0 plus a public patent non-assertion pledge (simpler, weaker); Apache-2.0 alone.
- **Supersedes:** the spec part of D-003.

## Follow-ups from the licensing review

1. Lawyer review of ~~`LICENSE-EXCEPTIONS.md` §A–§D~~ (done, no remarks — D-027), of the DCO-plus-headers mechanism, of the process and WASM boundaries (EU and US), of the spec's patent licence and of AI-generated code — **before the first code commit**.
2. Write to NVIDIA (licence §19) about loading DLSS through the driver (Blender 5.3 model); ask AMD to confirm the MIT listing of its FSR binaries.
3. Ask OpenDeviceDB to grant the app-store permission (§B) on its AGPL components — [opendevicedb#36](https://github.com/opendevicedb/opendevicedb/issues/36).
4. ~~CI before the first code~~ — done: `.github/workflows/licensing.yml` (REUSE lint, DCO sign-off on pull requests, cargo-deny once a `Cargo.toml` exists), `REUSE.toml`, `LICENSES/`, `deny.toml`.
5. Permissive crates for the GPU Plugin Interface, the Platform Interface and the World Module Interface bindings; never commit vendor headers.

## D-024 — Code starts before the lawyer's review; maintainer-only contributions until then

Date: 2026-11-09

- **Decision:** phase 1 code starts now, with the draft `LICENSE-EXCEPTIONS.md` headers. Until its final text is reviewed and published, **only the maintainer (and agents working for the maintainer) contribute code**; no external code contribution is accepted.
- **Why:** as sole copyright holder, the maintainer can still adjust or finalise the additional permissions alone; external contributions before that would require each contributor's agreement for any change.
- **Also decided:** phase 1 is built in this working session, starting with an end-to-end skeleton (minimal spec v0, gateway + Bevy region in one binary, Rust and TypeScript SDKs, a server-driven object seen by a three.js page and a Bevy client), then identity, agents and the device bridge.

## D-025 — How nexlink levels map to osage assurance levels

Date: 2026-11-09

- **Decision:** a nexlink account is **A1**; a social validation by other people is a **reputation signal**, not a proof of uniqueness; an identity whose documents have been checked is **A3**. nexlink becomes an **A2** issuer only if and when it attests uniqueness without documents. The thresholds of D-006 are unchanged; A2 has no identified issuer for now.
- **Why:** osage's A2 means *unique* human, which is what scarce privileges rely on; a level must say what its issuer actually guarantees.
- **Alternatives:** redefine A2 as "community-attested, uniqueness not proven" and raise D-006's scarce privileges to A3; or make nexlink attest uniqueness first (a nexlink-side project).
- **Corrects:** design/01, which first described nexlink as an A2 issuer.

## D-026 — Agents reach worlds through a local MCP adapter speaking the osage protocol

Date: 2026-11-09

- **Decision:** the MCP server for AI agents is **`osage-agent-mcp`, a client-side adapter** launched by the agent's harness over stdio. It joins the world like any client, with the `agent` role and the delegation of its responsible human, and turns the world into MCP tools (`world_info`, `observe`, `move`); components are decoded generically from the world's schema bundle (D-019). It is licensed **MIT OR Apache-2.0**, like the SDKs (D-003), so any agent ecosystem can embed it.
- **Why:** agents stay "clients like any other" (design/01): the osage protocol remains the only public interface of a world, the gateway keeps one authentication path, and the adapter works with any harness (Claude Code, Codex…) without the world exposing a second, MCP-specific endpoint.
- **Changes:** ADR-0001 placed an MCP "agent gateway" inside the replication gateway; agent-specific checks (delegation, quotas, controller kind) stay in the gateway, the MCP translation moves to the client side. A world-hosted MCP endpoint remains possible later as another client of the protocol.

## D-027 — The additional permissions are final; code contributions are open

Date: 2026-11-09

- **Decision:** a lawyer reviewed [`LICENSE-EXCEPTIONS.md`](../LICENSE-EXCEPTIONS.md) (osage-exceptions-1.0, §A–§D) and made **no remarks**. The text is final and in force for every file under `LicenseRef-osage-AGPL-3.0-or-later-with-exceptions`, since the first code commit. As D-024 planned, **external code contributions are now accepted**, under the DCO ([CONTRIBUTING.md](../CONTRIBUTING.md)).
- **Scope:** this review covers the additional permissions. The other points of follow-up 1 — the DCO-plus-headers mechanism, the process and WASM boundaries of D-014, the spec's patent licence, AI-generated code — stay listed there until reviewed.
- **Closes:** D-024's restriction; the "draft" status of D-005 and D-020 to D-022.

## D-028 — WebTransport in v0: one reliable stream, datagrams, discovery on the WebSocket host

Date: 2026-11-10

- **Decision:** the WebTransport transport of [ADR-0002 §3](adr/0002-protocol-v0.md#3-transport) is implemented in phase 1 as follows:
  - **one bidirectional stream**, opened by the client, carries every reliable message, length-prefixed as protobuf's length-delimited format; separate streams for entity events (ADR-0002 §3) wait until head-of-line blocking shows up in measurements;
  - **datagrams** carry `StateDatagram`s, cut at the datagram size into several datagrams of the same tick, and the clients' `Intent`s; nothing else is accepted in a datagram;
  - **discovery:** the WebSocket host answers `GET /.well-known/osage/webtransport` with the endpoint's URL and, for a self-signed development certificate, its SHA-256 hash; clients try WebTransport and fall back to WebSocket, except after a refusal (`Goodbye`), which is final;
  - **losses:** clients apply a volatile value only if its tick is not older than the last one applied to that entity and type; a one-second keyframe of all volatile values repairs losses;
  - **liveness:** 10 s idle timeout, 3 s keep-alives (QUIC's default of 30 s left a vanished browser's avatar in the world for half a minute).
- **Why:** the smallest design that gives browsers and native clients unreliable state with a WebSocket fallback, without a second session or a separate HTTP service; development needs no certificate authority.
- **Library:** [`wtransport`](https://github.com/BiagioFesta/wtransport) 0.7 (MIT OR Apache-2.0, on quinn) in the server and the Rust SDK; browsers use their native WebTransport API.
- **Alternatives:** announce the endpoint in the WebSocket `Welcome` (needs two sessions per client, which the duplicate-session rule refuses); per-entity reliable "settle" messages instead of keyframes (needs ticks on reliable messages too).


## D-029 — Credentials signed with RustCrypto directly; vulnerable dependencies fail CI

Date: 2026-11-10

- **Decision:** `osage-credentials` drops `jsonwebtoken` for a compact-JWS module limited to the two algorithms osage uses — ES256 (identity credentials, `p256`) and HS256 (delegations, `hmac` + `sha2`). The caller fixes the algorithm; a token never chooses it (`none` and algorithm confusion are impossible) and critical header extensions are refused. Tests verify the RFC 7515 examples, and tokens made by the former implementation and by WebCrypto. A `security` workflow runs `cargo deny check advisories` on every change and every Monday; vulnerabilities fail it wherever they are, "unmaintained" notices only for direct dependencies.
- **Why:** with its pure-Rust backend, `jsonwebtoken` 11.1 always pulls `rsa` (RUSTSEC-2023-0071, no fixed release) and an old `p256` on `crypto-bigint` 0.5 (RUSTSEC-2026-0387, fixed only in 0.7); its other backend, aws-lc, carries the OpenSSL licence, which does not fit the AGPL. osage needs two algorithms, not a JWT library.
- **Alternatives:** keep `jsonwebtoken` and ignore both advisories (osage never uses RSA, and the timing leak concerns signing, done only by the insecure development issuer); `jwt-compact` (0.9 still in beta).

## D-030 — World modules: the engine provides the mechanisms, the world its rules

Date: 2026-11-10

- **Decision:** a region runs a **world module**: a Bevy plugin plus the component types it adds to the core ones (with their schemas and a description each). The region provides the mechanisms:
  - sessions, and `Joined` and `Left` messages to the world;
  - replication: what the world spawns with `Replicated` is announced, its `Discrete` values are sent when they change, its `Transform` and `Volatile` values go in state datagrams, and despawned entities are retired;
  - intents, routed to the world only for an entity the sending session `Owner`s;
  - devices and actions, as before.
  
  The world provides the rules. The phase-1 bindings are the crate `osage-world-module-api` (**MIT OR Apache-2.0**), the World Module Interface of [`LICENSE-EXCEPTIONS.md`](../LICENSE-EXCEPTIONS.md) §C. `osage-server` adds a `Harness` that runs a world tick by tick in simulated time for its tests, and a shared command line (`osage_server::cli`), so that every world has its own binary. The cube-and-avatars demo became `DemoWorld`, the first module.
- **Why:** game rules and policy belong to worlds (see [engine provides mechanisms](../README.md#goals)); a permissive interface lets a world carry its authors' licence, which §C requires. Component types gain a `description` so that people and AI agents discover a world from its Welcome alone.
- **Next:** WebAssembly world modules loaded at run time, sandboxed, as §C anticipates.

## D-031 — The kart world: shared physics, exact prediction, a world that never makes up a step

Date: 2026-11-10

- **Decision:** phase 1 ends with a kart racing game in place of the 2030workspace integration, which waits for a later phase. The game is original: no name, character or element from existing kart games.
  - **Licence:** `worlds/kart`, the circuit, the race rules, the bots, the physics and the browser client, is under MIT OR Apache-2.0, so that world authors can start from it. The `kart-server` binary combines it with the AGPL server.
  - **Physics:** the kart physics is one Rust crate, run natively by the server and compiled to WebAssembly (38 KB, no imports) for clients. It uses fixed 1/60 s steps and `libm` for every non-trivial function, so it gives the same bits everywhere. A recorded script checks this in CI on both builds.
  - **Prediction:** a human's client applies each step at once and sends it to the world. When the world's state comes back with the last step it applied, the client replays its later steps on top. The server **never makes up a human's step**: a late step makes the kart wait (up to 8 ticks, then it coasts), and a backlog is caught up at two steps a tick under a credit that keeps any client from going faster than 60 steps a second. Prediction is then exact, and corrections come only from what the client cannot know (other karts, walls hit after a collision).
  - **Sending:** one message per frame carries the frame's steps plus the 3 before them. A browser may send only about one datagram per frame; this was measured with headless Chromium, where one datagram per step left the world 90 m behind.
  - **AI agents:** agents drive through the world's autopilot (`kart.autopilot/1`: pace, line, drift), discovered and sent through the generic MCP tool `send_intent`. An LLM cannot steer 60 times a second.
  - **Bots** fill the grid.
- **Alternatives:** a TypeScript copy of the physics (two implementations that would drift apart); repeating a late human's last controls (it made every late step a misprediction); one datagram per step.

## D-032 — Rendering programme: one plugin per technique, tiers, and the bench as oracle

Date: 2026-11-10

- **Decision:** rendering advances as a programme many people and agents can work on in parallel ([design/03](03-rendering.md)):
  - `osage-render` holds **one Bevy plugin per technique**, each configured per fidelity tier (T1–T4); the first four are sky and sun, shadows, ambient occlusion, post-processing;
  - the **native kart client** (`worlds/kart/native`) is the testbed: it plays online like any client and runs **the bench**, a timedemo that draws the same frames on every machine and reports frame times and screenshots;
  - every rendering change shows the bench before and after on a reference machine; each tier has a frame-time budget;
  - content comes from CC0 libraries or procedural generation, listed in an asset manifest;
  - work is cut into fourteen workstreams (R1–R14), each with milestones and an oracle.
- **Why:** reaching the GTA 6 bar is a multi-year, many-hands effort ([research/04](../research/04-rendering-vs-rage.md)); what one session can give it is the structure that makes each contribution small, independent and measurable.
- **Owner's choices:** the high end is carried by the native Bevy client, the three.js client stays the light tier; the high-end reference machine is the maintainer's desktop with ray tracing, the modest one an Intel Iris Xe laptop.
- **Alternatives:** one monolithic "quality" setting (no room for parallel work or old hardware); the browser as the only client (no ray tracing in WebGPU).

## D-033 — Tasks carry their oracle; clean sources

Date: 2026-11-10

- **Decision:** two rules taken from how decompilation communities work and from what they may not be used for ([research/07](../research/07-zelda-reverse-engineering.md)):
  1. **Every task names its oracle** — the check a machine can run to say it is done (a test, a budget held on the bench, a reference image, a determinism hash). The issue and pull-request templates ask for it; [AGENTS.md](../AGENTS.md) tells AI agents which checks to run before reporting.
  2. **Clean sources:** nothing from commercial games enters osage — no decompiled, recompiled, extracted or leaked code or content, no port built on them, no proprietary SDK headers. Techniques are learnt from public talks, papers and experiments; someone who worked on a game's decompilation does not write the equivalent part of osage from memory ([CONTRIBUTING](../CONTRIBUTING.md#clean-sources)).
- **Why:** the matching decompilation of a game is a translation of its publisher's code, whatever licence its repository carries; and what makes those communities finish whole games with thousands of contributors and agents is small tasks with an objective check and public progress — which osage can have with budgets, reference images and hashes instead of byte-for-byte matches.
- **What is reused:** ideas from the MIT-licensed RT64 renderer (a universal shader while specialised ones compile, frames drawn between simulation steps, background texture streaming), listed as milestones of workstream R13.

## D-034 — Knowledge is kept as an OKF bundle with an osage profile

Date: 2026-11-10

- **Decision:** the knowledge behind each module — its architecture, the algorithms and techniques it uses, their invariants, costs and limits, and the oracle that proves each claim — is recorded in [`knowledge/`](../knowledge/index.md) as a bundle in the [Open Knowledge Format](https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md) v0.2, following the [osage engineering profile](../knowledge/profile.md):
  1. **Four kinds of card:** `Module`, `Technique`, `Algorithm`, and OKF's `Attested Computation` for oracles. The profile adds the fields `resource` (the code described), `licence`, `sources` and `tiers`, and fixed sections per kind; it removes nothing from OKF.
  2. **A card is written to be read without the code**, well enough to rebuild the idea in another engine or language, and cites where the idea comes from.
  3. **A card changes with its code**, in the same change. Whoever writes it signs it; agents propose, people verify.
  4. **A machine checks the bundle** (`tools/check-knowledge.py`, in continuous integration): format, required fields and sections, citations, links, and that every address into the repository exists.
- **Why:** an idea can be reused where code cannot — in another language, under another licence, by an agent that has no room to read a whole crate. A community of people and agents needs a way into a module that is shorter than its source and that cannot silently rot. An existing open format gives tools and readers for free; the profile is what makes cards comparable and checkable.
- **Federation:** the bundle is a valid member bundle of an [OKF Federation](https://github.com/manaty/okf-federation) and passes its bundle validator; nothing in osage requires a federation, and a federation chooses to register it.
- **Scope of the first version:** the rendering crate and the kart world's physics and native client: 3 modules, 5 techniques, 5 algorithms, 2 oracles. The other modules get their cards as they are next changed; decisions, design notes and research surveys stay in `design/` and `research/` and join the bundle in a later version of the profile.
- **Alternatives:** documentation comments only (bound to the code and its language, no place for sources, costs or alternatives); a wiki (outside review and version control); a format of our own (no shared tools, one more thing to learn).

## D-035 — Games are assembled from reusable components, one crate each

Date: 2026-11-10 · From the architecture review of 2026-11-10, with the maintainer.

- **Decision:** a game made with osage is assembled from **reusable components**, even those that only that game uses. A component is:
  1. **one crate** — a Bevy plugin with a declared contract: the data it reads and writes, the events it sends, its settings; it never reaches into another component's internals;
  2. **documented** by a knowledge card of a new `Component` type in the [osage profile](../knowledge/profile.md) (contract, settings, dependencies, licence), and **checked** by an oracle (D-033).
- **Where a component lives does not change what it is:** a game's own components may stay beside the game (`worlds/<game>/components/`) and move to the shared catalogue (`components/`) when another game needs them, at no cost. A game is then mostly a list of components and their settings.
- **Licence:** each crate carries its own. What other clients or games embed (models, generators, content loading, interfaces) is MIT OR Apache-2.0; engine services stay AGPL-3.0-or-later with the osage exceptions (D-020).
- **Consequences:**
  - Rendering techniques become component crates that register themselves; `osage-render` keeps what they share — tiers, the world's look, which techniques are active — so adding a technique edits no central file (supersedes the closed list of techniques of D-032).
  - The kart's native client is split into components: procedural models (`sculpt`), content loading, the connection plugin (shared with `osage-client`, which becomes a library), the benchmark harness, and the kart's own pieces (karts, track, landmarks, interface).
  - A template (`tools/new-component`) creates a component with its manifest, licence headers, card and test; continuous integration checks that every component has its card and its oracle.
- **Why:** many teams and AI agents working in parallel need boundaries that the compiler enforces, not conventions; "reusable" is only real for something that can be depended on, versioned and licensed on its own. Bevy itself is built this way, and many small crates rebuild faster than one large one.
- **Alternatives:** one module per component in a few crates (lighter, but the boundary is a convention); moving code out of a game only when a second game needs it (each game would grow its own engine meanwhile).

## D-036 — A content service: content by hash, files or recipes

Date: 2026-11-10 · From the architecture review of 2026-11-10, with the maintainer.

- **Decision:** the next server work is a **content service**, so that any client can show any world:
  1. **Every visible entity declares a generic look** — `osage.transform/1` and `osage.content_ref/1` — even when a world's own clients draw it their own way. A generic client always shows something right; a specialised one may show more.
  2. **Content is addressed by its hash** and served by the world (in the single-binary profile, by the server itself over HTTPS, beside the WebTransport discovery); files are immutable and cached for ever. A per-world content manifest records each file's licence, author and source, as `assets/manifest.toml` does today.
  3. **Content is a file or a recipe.** A file is glTF, 3D Tiles or splats. A recipe is "generator, version, parameters": clients that have the generator build the model locally — tiny downloads, good for old hardware, deterministic. **A baked file is always provided as well**, for clients without the generator.
- **Recipes are run safely:** generators are WebAssembly modules, content-addressed like any file, run as pure functions (parameters in, mesh out; no network, no files, no clock, bounded work). A recipe carries the hash of the mesh it must produce; on a mismatch, a refusal or a missing generator, the client takes the file.
- **Why:** today only the kart's own clients can show the kart, and each client re-implements a world's look. The protocol already carries content by reference (`osage.content_ref/1`); nothing served it.
- **Alternatives:** files only (simple, standard, but heavier and the generators are not reused at run time); geometry in the protocol (rejected since ADR-0002: no geometry travels in the protocol).

## D-037 — The gateway and the region talk by messages, from now on

Date: 2026-11-10 · From the architecture review of 2026-11-10, with the maintainer.

- **Decision:** the replication gateway and the region server, two modules of one process today, become **two components with a message contract** between them (D-035). In one process the messages go through channels; when regions move to other machines, the same messages go over the network. This is done **before** the content service, so that every server feature added from now on respects the boundary.
- **Why:** ADR-0001 makes the gateway/region interface internal and free to change, but a boundary kept by function calls erodes with every feature, and splitting a process late is costly.
- **Order of work that follows from the review:** (1) the gateway/region message boundary; (2) components and their template, starting with the pieces the content service needs (procedural models, content loading); (3) the content service. Persistence (D-004) comes after.

## D-038 — Computation runs where it can; trust by default, verification by choice

Date: 2026-11-10 · From the architecture review of 2026-11-10, with the maintainer.

- **Decision:** where a computation runs is **not fixed**. A client runs what it can; what it cannot is supplied by the server, or by other machines — other clients, or a peer compute network that a world chooses to use. The pattern of D-036 (a client generates a recipe if it can, otherwise it receives the baked file) applies to all world logic.
  1. **Logic components do not depend on where they run:** WebAssembly, deterministic, explicit inputs and outputs, the same result on the server, on a client or on a peer (as the kart's physics already does, D-031).
  2. **Clients declare their capabilities** (power, budget, battery, what they can run); the world places computations accordingly, and may move them during a session.
  3. **Computing is not being the authority:** what a client computes for itself (its prediction) engages only itself; the world's authority decides the shared state, and decides whose computations it accepts.
- **Trust is total by default.** Security is added in layers that each world chooses, per kind of computation, knowing that each costs performance. The engine provides the layers, from cheapest to dearest:
  1. full trust (the default);
  2. verification by sampling: since computations are deterministic, the authority or other peers redo a share of them, and a peer caught wrong is set aside;
  3. computation reserved to trusted machines (identity, reputation, hardware attestation);
  4. the server alone is the authority.
- **Two exceptions, whatever a world chooses** (the engine's safety defaults protect people from a world):
  - **actions on real devices:** the device bridge keeps its safety envelope, watchdog and emergency stop locally (D-009); they are never computed by a third party;
  - **identity and permissions:** who is who and who may do what are never delegated to a peer.
- **Why:** old hardware and phones should join any world, and idle machines can help those that cannot keep up; a world among friends should not pay for checks it does not need, while a public competitive world can buy the guarantees it wants.
- **Alternatives:** fixed placement (server computes, clients display — simple, but wastes clients and excludes weak ones); untrusted by default (every world pays for verification it may not need).

## D-039 — Versions are bridged by translation modules

Date: 2026-11-10 · From the architecture review of 2026-11-10, with the maintainer.

- **Decision:** when a world type or the protocol itself changes major version (`kart.state/1` → `kart.state/2`, protocol v0 → v1), old clients keep working through **translation modules**: small components (D-035) that convert one version into another, supplied by the world for its types and by osage for the protocol. Like any computation (D-038), they run where they can — on the gateway, or on the client itself — and each is checked by an oracle: recorded messages of one version and their expected translation.
- **Rules:**
  1. A world announces, for each type, the versions it can serve directly or by translation; a client says which it understands; the gateway chooses.
  2. A translation may lose information. When a version cannot express something (a concept that did not exist), **only that type is degraded**, with a message the client can show; the rest of the connection keeps working.
  3. Adding fields within a major version stays compatible without translation (protobuf rules); a new major version is a new type name.
- **Why:** "any client joins any world" must hold for clients that update slowly — a browser tab kept open, an app store release, an AI agent written against an older world. Worlds should not have to carry their past in their logic.
- **Alternatives:** worlds serving several versions themselves (each world carries its past); refusing old clients (breaks the promise above). The difficulty of translating between modular, distributed versions is accepted: it is work, not a reason to give up the promise.

## D-040 — A machine computing for others answers through a responsible human

Date: 2026-11-10 · From the architecture review of 2026-11-10, with the maintainer.

- **Decision:** a machine that computes for others (D-038) joins a world like an agent (design/01, P4): with a **delegation from a responsible human**. If a world chooses to set a peer aside, it is that person's delegation that is set aside. Nothing else is required of it: trust stays total by default.
- **Priority:** performance comes first. Cheating is not the first concern; the verification layers of D-038 come when a world needs them, and are not built before.
- **Why:** the identity model knows people, agents and device bridges; compute peers fit the existing agent pattern without a new mechanism.
- **Alternatives:** anonymous compute paid as a service (no one to set aside, and payment is a policy for worlds, not a mechanism to build now).

## D-041 — Performance is guarded by the reference machines, on every change to main

Date: 2026-11-10 · From the architecture review of 2026-11-10, with the maintainer.

- **Decision:** the reference machines — the Iris Xe laptop for T1 and T2, the maintainer's ray-tracing desktop for T3 and T4 — are connected to continuous integration as **self-hosted runners** and run the render bench (`kart-lap/2`) on **every change merged into `main`**. Each run is attested (`tools/attest-bench.py`) and compared with the previous one; a slowdown beyond the noise is flagged on the change that caused it.
- **Security of the maintainer's machines:** the runners execute **only code already merged into `main`** (push events on the main branch), **never** pull requests, from forks or otherwise; they run under an unprivileged account dedicated to them, with no access to the maintainer's other files or credentials. Outside contributions are benched after review and merge, not before.
- **Noise:** a run on a busy machine is not a measurement. A runner records the machine's load with each run; a flagged slowdown is confirmed by running the previous and the new build back to back before anyone is asked to act on it.
- **Why:** performance is the first priority (D-040); with many contributors, a project slows down one unnoticed change at a time unless every change is measured on the hardware the tiers are defined on.
- **To do by the maintainer:** install and register the two runners (a change of system settings, done by the maintainer).
- **Alternatives:** receipts attached by contributors (relies on each of them running the bench); a nightly pass only (finds a slowdown a day late, among several changes).

## D-042 — The high tiers are benched on cloud GPUs

Date: 2026-11-10 · Amends [D-041](#d-041--performance-is-guarded-by-the-reference-machines-on-every-change-to-main), with the maintainer.

- **Decision:** T3 and T4 are benched on a **cloud GPU machine created for each run and deleted after it** (Google Cloud, Singapore region), instead of the maintainer's desktop. T1 and T2 stay, for now, on the Iris Xe laptop with the restrictions of D-041: no cloud provider rents integrated graphics, and a throttled data-center GPU would not measure what an old laptop sees.
- **How:** the bench binary is built on an ordinary CI machine; only the bench itself runs on the GPU machine, which renders without a window and is deleted when the report is back. The cloud project used for it is separate from any other, with a spending limit, and CI reaches it with short-lived credentials (no stored keys).
- **Why:** nothing of the maintainer's is at risk; the machine is the same at every run, so measurements are steadier than on a desktop in use; anyone can reproduce a result on the same instance type. The cost is a few minutes of GPU per change to `main`.
- **Later:** a dedicated mini PC with integrated graphics may replace the laptop for T1 and T2.

## D-043 — Few CI runs: the developer's machine first, the cloud GPU at most once an hour

Date: 2026-11-10 · Amends [D-041](#d-041--performance-is-guarded-by-the-reference-machines-on-every-change-to-main) and [D-042](#d-042--the-high-tiers-are-benched-on-cloud-gpus), with the maintainer.

- **Decision:** continuous integration runs as few times a day as the work allows, especially while development is intense. The oracles run first on the developer's machine; commits are pushed in batches rather than one push each, and changes can be grouped into one pull request. The cloud GPU bench runs **only when needed and never twice within an hour**: on a push to `main` that touches what can be drawn, by hand, and once a night to catch up. It benches only if something drawn changed since the last benched commit, and a commit already benched is not benched again, even if it failed its budget.
- **How:** the workflow `cloud-bench` first asks whether anything is to be benched: the newest artefact of receipts says which commit was benched last, and when. Only then is the bench built and a GPU machine made.
- **Why:** every run costs money (the GPU machine) or shared resources (CI minutes and quota), and the developer's machine is at hand. A slowdown is still caught, at most an hour or a night after the change that caused it, among the changes pushed together.
- **Consequence for D-041:** "every change to `main`" becomes every batch of changes, for the cloud bench as for the laptop's runner once it exists.

## D-044 — The kart world looks photographed when its content is there

Date: 2026-11-11 · With the maintainer, who set the goal of a photorealistic kart, in the class of GTA V.

- **Decision:** the kart world's look changes from a toy circuit to a photographed one. With the asset manifest's content (a CC0 batch from Poly Haven, approved and fetched on 2026-11-11: a sky panorama, 2k surfaces, roadside models), the sky is a photograph that also places and calibrates the sun, the road, verges, gravel traps, kerbs and walls have real surfaces, and the roadside is dressed with models from T2. Without the content, the world stays procedural, as the manifest promises. The toys (round clouds, balloons, the fair, inflatable arches, the windmill) give way; the karts become racing karts to scale at every tier.
- **How, by the rules of D-035:** the work is done as components, each a crate with its card and its tests: `osage-content` (textures and sky panoramas), `osage-procedural` (sculpted geometry and the seeded generator), `osage-props` (models placed many times), and for the kart game `osage-kart-land`, `osage-kart-roadside` and `osage-kart-model`; the technique `panorama` joins `osage-render`.
- **Why:** the maintainer's goal for the testbed is photorealism; a photographed world measures the rendering programme against the bar it aims at (design/03), where a toy world hides what is missing.
- **Measured:** high tiers on the cloud L4 (D-042, at most once an hour, D-043); low tiers on the developer's laptop.

## D-045 — Kart contacts with momentum, and wheels that feel the ground

Date: 2026-11-12 · Amends [D-031](#d-031--the-kart-world-shared-physics-exact-prediction-a-world-that-never-makes-up-a-step), with the maintainer, who asked for realistic collisions between karts and for wheels that run off the road.

- **Decision:** a kart still drives as an arcade kart (a speed along it, a slide across it, a drift), but its contacts become physical. A kart has an outline (a capsule 1.9 m by 1.3 m), a mass and a moment of inertia; when two karts touch, or a kart touches a barrier, they exchange an impulse at the point of contact, with a little restitution and the friction of bodywork and tyres rubbing. A knock therefore pushes, slows and spins each kart by where it lands: a rear-end knock passes the speed on, a hit on a corner spins the kart, which keeps going the way it went while its tyres scrub the slide off and stop the spin; a glancing blow on a barrier keeps most of the speed and turns the kart along it. Each wheel reads the ground under it — road, kerb or grass: grass slows the kart in proportion to the wheels on it and drags it towards the side they are on; kerbs and grass grip less.
- **How:** the state gains one number, `spin`, the rate of turn a knock gave. It travels in `kart.state/1` as field 10 — an added field, which older readers ignore, predicting without it until they update — and through the WebAssembly interface. The barriers' face is part of the physics (`BARRIER`, 0.9 m beyond the verge, where the native client already drew them): the kart's outline stops there. Contacts between karts run on the server only; a second golden file records them, natively.
- **Why:** contact was a separation without momentum — karts slid off one another and walls stopped them on a line through their centre, turned by a fixed share. In close racing, contact is what players feel most.
- **Kept:** determinism (fixed steps, `libm`, sanitised controls); the golden files change once, in the same review.

## D-046 — Paddock objects in the kart world's races

Date: 2026-11-12 · With the maintainer, who asked for objects to collect and throw during a race and chose paddock objects among the proposals.

- **Decision:** the kart world's races have items. Racks of crates stand across the road at three places of the lap; breaking one gives a kart one item: a nitro canister (a boost), a tyre thrown ahead that rolls and bounces off the barriers and spins the first kart it meets, a cone dropped behind (it slows and knocks), an oil patch left behind (a kart that drives in spins). The draw favours the back of the field. The world's bots use them, and agents can let the world use theirs.
- **How:** a new component, `osage-kart-items` (D-035), deterministic like the physics, run by the world server, which alone decides what items do. The protocol grows by added fields — `Step.use_item`, `Autopilot.use_items`, `Racer.held` — and one new volatile type, `kart.paddock/1`, on the race board; older clients ignore them and race without items. A human's predicted kart is corrected for what items do to it as for a contact (D-031, D-045).
- **Why:** items mix the field and give AI drivers one more way to race; paddock objects suit a circuit and are the world's own design, not a copy of another game's.
- **Kept:** the bench's race (`kart-lap/3`) has no items: its pictures and its golden race do not change.

## D-047 — The kart's Windows folder updates itself from GitHub releases

Date: 2026-11-12 · With the maintainer, who asked for an automatic update on Windows after friends installed the kart from a zip.

- **Decision:** before each game, the Windows launchers run `kart-update.exe`, which brings the folder up to the newest `kart-v…` release of the repository: the programs when the release was built from another commit than the folder's, the content when it changed or is missing. Every download is checked against the SHA-256 GitHub publishes for the asset. Offline or failing, the game starts as it is; `OSAGE_NO_UPDATE=1` or a `no-update` file turns it off.
- **How:** a new crate, `osage-update` (mechanisms: newest release, checked download, unpack, copy over), and the kart's policy in `kart-update`. The system's `curl` and `tar` do the network and the archives: no HTTP, TLS or zip code in the engine. The release workflow writes the commit into `release.txt`.
- **Why:** players who unzip a folder do not come back to the release page; a fix reaches them only if it comes to them.
- **Kept:** releases are still the maintainer's act: the updater installs only what was published.

## D-048 — A rocket in the kart's physics

Date: 2026-11-12 · Amends [D-045](#d-045--kart-contacts-with-momentum-and-wheels-that-feel-the-ground), with the maintainer, who asked for a rocket faster and longer than the nitro, and a longer nitro.

- **Decision:** a paddock object, the rocket, gives 3.5 s during which the kart's top speed is 34 m/s (a boost's is 28); a nitro gives 2.5 s of boost instead of 1.2. The rocket goes mostly to the back of the field.
- **How:** the kart's state gains `rocket`, seconds left, stepped by the physics like `boost`, so that a client predicts it exactly. It travels in `kart.state/1` as field 11 — an added field: an older reader predicts a plain kart and is corrected — and through the WebAssembly interface (`kart_step` takes it after `spin`; `kart_get(9)` reads it). `ITEM_ROCKET = 5` is added to the item enum.
- **Kept:** the golden files do not change: no rocket in them.

## D-049 — The host starts each race of the kart world

Date: 2026-11-12 · With the maintainer, who asked that the host decide when the next race starts (leaving time for others to join) and that a player alone can end a race.

- **Decision:** lined up, the karts wait on the grid (`PHASE_READY`) until the host starts the race; the host is the first human driver still in the world, named in `Race.host` (its kart). A driver racing alone — no other human or agent — can end the race; the results follow. A world without a human (agents only) starts its races by itself, and so does one whose host stays silent five minutes.
- **How:** `Step.call` (`RaceCall`: start, stop) rides on the controls a human already sends, a few messages in a row against loss; no new type. `KartWorld::host_starts` (on by default) keeps the old flow for tests and unattended worlds (`KartWorld::unattended`).
- **Why:** friends joining on the same network arrived mid-race and waited a whole race; the one who hosts knows when everyone is there.

## D-050 — Skill rewarded in the kart's driving: three-level drifts, ramps and tricks, starts, slipstreams

Date: 2026-11-12 · Milestone M1 of the [kart programme](04-kart-programme.md), which the maintainer set as the goal.

- **Decision:** the kart's physics rewards skill where the genre does. A drift charges a mini-turbo of three levels (0.6, 1.2, 2.0 s of drift for 0.5, 1.0, 1.6 s of boost), shown by the colour of the sparks. Ramps across the road throw karts into the air (gravity 18 m/s², a quarter of the steering, no drive); a press of the drift button in the air is a trick, a 0.7 s boost on landing. The world judges each start as the lights go: throttle pressed at most 0.4 s before is a 1 s boost, more than 1.5 s before stalls the engine for 0.8 s. A kart in another's wake charges a slipstream: 1.2 s of it is a 0.6 s boost. The paddock circuit gets two ramps.
- **How:** `Kart` gains `height`, `climb`, `trick`, `stall`, `draft` and `held` (the button at the last step, so that a trick needs a press); `Track` gains ramps; `kart.state/1` and `kart.track/1` carry them (fields added, readers that ignore them are corrected). Slipstream and start are separate functions the world calls (`slipstream`, `launch`): they involve other karts or the race. The WebAssembly interface sets and reads a kart's fields by number (`kart_set`, `kart_get`), so that the next field changes no signature. The golden files are recorded again; their script now flies off two ramps.
- **Why:** the drifting was the only skill the driving rewarded, and only in two steps; friends playing asked for the depth of the best kart racers. A trick and a start in time are cheap to learn and fun to land; a slipstream keeps a pack together.

## D-051 — Crests instead of ramps: the road goes over the brow of a hill

Date: 2026-11-12 · Replaces the ramps of [D-050](#d-050--skill-rewarded-in-the-karts-driving-three-level-drifts-ramps-and-tricks-starts-slipstreams), at the maintainer's request: "assure-toi du réalisme et de la pertinence".

- **Decision:** the track has no ramps. Where karts leave the ground, it is over a **crest**: the road, its kerbs, verges and barriers and the land beside them rise and fall together, half a cosine up over `rise` metres to `height`, half a cosine down over `fall` metres — no edge, no step, nothing to crash into from either side. A kart leaves the road only where the road curves away faster than gravity pulls it down, so speed decides: on the paddock circuit's crest (12 m up to 1.4 m, 6 m down, on the start straight) a kart flat out (22 m/s) flies about 0.55 s, a metre over the road, and lands 12 m on, on the downslope; with a boost, longer; below about 18 m/s it stays on the road. The second ramp, in the long sweep, is gone. Tricks in the air and their boost stay as D-050 has them.
- **How:** `Ramp` becomes `Crest { along, rise, fall, height }` (`Crest::profile`), the same field numbers in `kart.track/1` plus `fall`; the WebAssembly interface takes `track_crest(along, rise, fall, height)` and gives `track_lift(along)`, which the browser client draws the road with. The flight is ballistic: each step the kart falls freely from where it was; if the road under it is higher, the road carries it and how fast the road rose becomes its climb, whichever way it drives. The land's frames carry the crest, and every ribbon along the track (road, kerbs, verges, barriers, fences) is drawn in half-metre pieces over it; the land beside the track follows the road and fades into the hills over 30 m; things on the road are drawn on it; the chase camera rises and falls with the kart. Golden files recorded again.
- **Why:** a ramp across a race track — a wedge of asphalt from barrier to barrier ending in a metre-high wall — is nothing a real circuit has, and from behind it was a wall. Crests are real (a road over a hill), they keep the trick, and they ask for a choice: lift off the throttle to stay down, or take it flat out and fly.

## D-052 — Items that keep a race open: drawn by place and gap, held behind, ten kinds

Date: 2026-11-12 · Milestone M2 of the [kart programme](04-kart-programme.md). Extends [D-046](#d-046--paddock-objects-in-the-kart-worlds-races).

- **Decision:** the paddock hands out ten items: nitro, rocket, tyre, a stack of three tyres, cone, oil, smoke, a drone, an air bag (bumper) and a push from the pit crew. The draw weighs each by how far back a kart is — the larger of its place as a share of the field and its gap to the leader as a share of 120 m — so that a kart far behind gets strong help even in third place; the leader never draws a rocket, a drone or a push. A tyre, a stack, a cone or oil can be **held behind** the kart while the button is down: it shields the kart from a tyre thrown from behind and is used as the button comes up. The drone flies to the leader over the field and lands on it; the bumper knocks aside the karts within 4.5 m; the push is 2 s of boost and 2.5 s in which nothing on the track touches the kart. Crates come back after 1.5 s and there are five to a rack.
- **How:** `Paddock::step` takes each kart's `Standing` (place, metres behind the leader), which the world works out from its race; the use button is a level, and `step` acts on its edges. `Paddock::button` gives AI drivers a button with the same rule for holding behind. `kart.state/1`'s racer gains `held_count`, `trailing` and `guard`; items `STACK`, `DRONE`, `BUMPER` and `PUSH`; a thing its `height` (the drone in the air). The native client draws the new items carried, held behind, and on the track, with a drone's rotors heard and cues for a bumper and a push. Bots steer round cones, oil and tyres on their line (`drive_among`, with the hazards the world gives).
- **Why:** with four crates per rack, a 4 s return and a draw by place alone, a race of nine shared 13 items, the pack in single file finding crates broken, and a kart 150 m back in fourth drew as if it were close. A race stays open when the back of the field can come back and the front has something to defend with; holding an item behind is that defence, and it asks for a choice: keep it as a shield, or use it.

## D-053 — The kart world keeps its records

Date: 2026-11-12 · G4.5 of the [kart programme](04-kart-programme.md), brought forward: the maintainer asked for "tous les extras comme leaderboard".

- **Decision:** each kart world keeps records on each track — the best laps and the best races of its number of laps, the fastest ten, one per driver name (that driver's best) — with when they were set. Humans and agents set them; the world's bots do not. They survive a restart of the world. Clients and agents read them from the race board; those set in the race just run are marked fresh. A board shared between worlds is not the engine's: an operator who wants one runs it from the worlds' records (mechanisms, not policy).
- **How:** a component crate, `osage-kart-records` ([card](../knowledge/modules/kart-records.md)): boards, a plain text file (`kart-records/1`, a record a line) written beside and renamed over, lines that do not read skipped. The world offers each lap and each race finished, saves what enters and publishes `kart.records/1`. `kart-server --records FILE` names the file (the engine's `cli::run_with` passes the rest of the command line on); the Windows launchers use `kart-records.txt`, which updates leave alone. The native client shows the records between races.
- **Why:** a record to beat is what brings friends back to a track, and the world is the authority on its races already. A text file needs no database and no new dependency, reads by eye, and survives a crash while saving.

## D-054 — Nothing repeats: grounds untiled, a road's history drawn, species by place

Date: 2026-11-12 · V1.0 of the [kart programme](04-kart-programme.md), brought forward beside M2 at the maintainer's request (the road, the grass and the trees varied deterministically, never repeating).

- **Decision:** a ground's grain is sampled without a tile — hexagonal cells, each shifted and turned by a hash of where it is, blended across their edges — so that no period shows in the road, the verges, the gravel or the meadows. A road also shows its history, a deterministic function of the distance along it and across it: paving seams, repaired patches, cracks in older stretches, stains, a tone that wanders. In the woods, the species follows the place: stands of pines, of firs, of broadleaf trees, mixed at their edges. The same picture on every client: nothing is random but hashes of places.
- **How:** `osage-render`'s ground material (the [card](../knowledge/techniques/ground-variation.md)) takes the surface's colour map as its own grain (`Ground::with_grain`, `untextured`) and a road's history (`Ground::paved`); the shader's tiling is mirrored on the CPU (`hex_tiles`) for the oracle: a strip of untiled grain has no period (autocorrelation). The kart client untiles its road from T2 and its grass and gravel from T3, where the cost fits (on an Iris Xe: the road alone 1.2 ms a frame at T2, every ground 1.7 ms at T2 and 3.6 % at T3); the history is drawn at every tier. `osage-kart-roadside` draws a tree's species from a noise of its place with a roll of the dice.
- **Why:** a photographed texture is convincing close up and a carpet from ten metres; the eye finds a period at once, and a road that is the same every five metres is the first thing that says "game". Hex-tiling is the cheapest published way to break it without new textures (three samples at worst, one most of the time); a road's history costs only arithmetic. Normal maps still repeat: turning them with their cells is the next step.


## D-055 — Drivers and kart bodies: a build of five shares, a choice on the grid

Date: 2026-11-12 · G3.1 of the [kart programme](04-kart-programme.md) (M3).

- **Decision:** a kart has a **build** — top speed, acceleration, handling, grip and weight, each a share of the standard kart's — and a player chooses who drives (eight original drivers in three classes: light, medium, heavy) and in what (four bodies: Classic, Sprint, Cruiser, Drifter). The build is the product of the driver's class and the body, clamped to a narrow range, so that no pair is the best one: every one of the 32 pairs laps within ±1.3 % of the others when the world's autopilot drives it (the oracle allows 2 %). Weight matters in contact: in a bump the impulse is shared by mass, so a heavy kart is pushed less and keeps its line. The choice is an intent, `kart.choice/1`; on the grid (waiting, ready, countdown) it changes the kart at once, during a race from the next one, and the world keeps it for the races that follow. Bots get drivers and bodies in turn. A kart's colour is its driver's.
- **How:** `osage-kart-physics` gains `Build` (the standard build is the old kart bit for bit: the golden trace is unchanged) and the WebAssembly physics carries it in fields 16 to 20; `osage-kart-roster` (the [card](../knowledge/modules/kart-roster.md)) lists the drivers and bodies and makes builds; the world publishes the roster (`kart.roster/1`) on the race board and each racer's driver, kart and build in `kart.racer/1`, so that clients predict their own kart with its build (D-031). The native client picks with the arrow keys or a pad's d-pad between races, the browser with arrows or buttons; records note the driver and kart a time was set with (D-053).
- **Why:** choosing who to be is half of a kart game's charm and its second reason to play again; differences that a player feels (a Sprint leaps out of bends, a Cruiser holds its line, a heavy driver shrugs off a bump) but that do not decide a race keep every choice honest. A narrow band, tested on every pair, is what keeps a roster from collapsing onto one pair. Five shares, not a table of curves: what a body does is readable, and an agent can reason about it from the roster.

## D-056 — A track is data: a text file, checked, built by the existing land and scenery

Date: 2026-11-12 · G3.2 and the shapes of G3.3 of the [kart programme](04-kart-programme.md) (M3).

- **Decision:** a track is a short text file (`kart-track/1`): an id, a name, a line about it, its sky and its scenery, the road's half width and verge, the control points of its closed centre line (one marked `start`) and its crests, placed from the start line. Nothing about a track is code: the land, the roadside and the landmarks already follow the road. A file is checked before it is raced — the loop never comes within its own road and verges of itself, no bend is tighter than 12 m of radius, crests and the grid stand on straight road, the length is 400 to 3000 m — and the world refuses one that fails. Four tracks come with the game: the Paddock as it was (the same control points, crest and start; its golden values are unchanged), Gull Point (a fast cliff road, 883 m), Pinewood (a narrow forest lane at dusk, 692 m) and Harbour Town (a street circuit, 699 m). Records are kept per track (D-053). `kart-server --track ID|FILE` races one; the native bench takes `--track` too and is raced under the track's sky.
- **How:** `osage-kart-tracks` (the [card](../knowledge/modules/kart-tracks.md)) reads, checks and builds tracks; `kart.track/1` carries the track's id, name, about, sky and scenery beside its road; clients build the scene again when the track changes. The shapes were drawn with a throwaway plot of the spline and its checks, and the world's bots race every one (`tests/bots.rs`).
- **Why:** a cup needs tracks, and a track that is data can be made, shared and checked without touching the engine — by a player, a world operator or an AI agent. Elevation beyond crests, banking and surfaces beyond road, kerb and grass are not in the format because the physics does not have them yet: a format that promises what the game ignores would mislead whoever writes a track.

## D-057 — Cups: four tracks, points by place, the host chooses, the others vote

Date: 2026-11-12 · G3.4 of the [kart programme](04-kart-programme.md) (M3).

- **Decision:** a world races an **event**: a cup (`cup:ID`) — its tracks one after the other, points for each place (10, 8, 6, 5, 4, 3, 2, 1; ninth and after none), standings between the races, a podium after the last that stays up twice as long, then the cup again — or one track alone (`track:ID`), race after race. Ties go to more wins, then more second places and so on, then to the better place in the latest race; a driver who joins mid-cup starts from nothing, bots score like everyone. The host (the first human driver) chooses the event with `Choice.event`: on the grid it is raced at once, between races from the next one. The others' choices are votes, shown to all; with no host, the event most drivers want is raced next. One cup comes with the game, the Grand Tour (the Paddock, Gull Point, Pinewood, Harbour Town); `kart-server` races it unless `--track` names one track.
- **How:** `osage-kart-cup` (the [card](../knowledge/modules/kart-cup.md)) scores and ranks, knowing nothing of karts; the world's `cup.rs` keeps the event, the votes and the series, changes the circuit when the next race is on another track (D-056) and publishes the cup, its standings, the event, the votes and the events there are in `kart.race/1`. The native client shows the cup and its standings between races, with the event (T to choose or vote); the browser does the same with a button. The oracle is the world test of a whole cup (`tests/race.rs`).
- **Why:** a cup is what turns a few races into an evening — something to win over four tracks, a reason to stay for the next race. Points that fall away gently keep the middle of the field in it; the host choosing keeps a LAN party simple, and votes let the others say what they want without a menu fight. Scoring is a component because any racing world can use it (D-035).

## D-058 — A kart engine that does not tire the ear: cruising revs, a ridden mix

Date: 2026-11-12 · after the maintainer's playtest ("le son du kart est toujours pénible à pleine vitesse"; "baisser le volume du moteur à pleine vitesse"), within M2's sound (D-052).

- **Decision:** at top speed a kart's engine turns at 11 800 turns a minute, not at the limiter, which only a boost reaches; a recorded loop is never played more than 1.25 times as fast as it was recorded, and a low-pass that moves down as it is played faster keeps its timbre; under way the revs sway by up to 1 %. The mix rides the engine as a sound engineer rides a fader: 30 % lower high in the revs, a further quarter lower once flat out and steady for a few seconds, back at once when the revs change. Sound and music can each be switched off in the settings, or with N and M at any time, and the choice is kept.
- **Why:** flat out, the engine was the highest loop sped up by half again — a held, nasal note over everything, for most of a lap. A note the ear has learnt is a note it can hear lower; the others' engines, the tyres and the music then come through.

## D-059 — Music from anywhere, the host's playlist shared on the network, voice later

Date: 2026-11-12 · after the maintainer's requests ("un module générique de musique liant radio et streaming musique libre"; "un module permettant au host de streamer sa playlist perso"; whether to do it "dans nexlink", with the players "parler entre eux comme dans une conf call audio").

- **Decision:** music is an engine crate, `osage-music`: a player on a thread that plays a playlist of files, an internet radio station, or a host's shared playlist, into a feed the audio output never waits on. The radio-browser.info directory is asked only when the player turns it on, for a kind of music they choose; a player's own stations are a text file of theirs. A host shares a folder with `kart-server --music DIR`: a small HTTP server beside the world serves the files and where the playlist is on the host's clock, the world announces its port (`Race.music_port`), and each player fetches the piece being played and starts at the point the host is at. The kart game ships five CC0 pieces (OpenGameArt), levelled; the settings choose where the music comes from — auto (the host's when shared, else the game's), the game's, the player's folder, the host's, radio — and B skips. Voice between players is next, in osage too: a conference of the players of a world, relayed by the world, microphone off until the player turns it on. nexlink can carry both later as an adapter, never as a requirement.
- **Why:** the four friends play on one network with no account anywhere; what they need — the same music for all, and later each other's voices — is a mechanism of a world, not of a social network, and it must work without one. Fetching the piece and seeking to the host's point keeps the share a file server (simple, no transcoding, no pacing) with everyone in step to a fraction of a second. Asking a directory tells it who asks: a choice for the player, so off by default.
- **Rejected:** streaming the host's sound card (it would carry the host's game too, and needs an encoder); a live re-encoded stream (pacing, an encoder, more CPU on the host for no gain on a LAN); doing it in nexlink first (it would make the game need an account and a server outside the room); bundling third-party station addresses (they change, and choosing them is the player's).
- **Consequences:** the host is responsible for having the right to share their music with their friends (README); the share is plain HTTP on the local network, heard by anyone on it, as the game is joined by anyone on it. The browser client has no music yet.

## D-060 — Players talk to one another: a relay beside the world, ADPCM over UDP, the microphone off by default

Date: 2026-11-12 · after the maintainer's request (players "parler entre eux comme dans une conf call audio"; nexlink "plus tard"), following D-059.

- **Decision:** voice is an engine crate, `osage-voice`. Each player's speech — 16 kHz mono in 20 ms frames, coded in IMA ADPCM (4 bits a sample, each frame on its own) — goes over UDP to a relay beside the world (`kart-server` starts one on port 7881, announced as `Race.voice_port`), which hands it to the others; each player queues every voice two to eight frames deep and mixes them. A player joins under their driver's name and hears the others at once; their microphone opens only when they choose push to talk (V held) or an open microphone (gated by loudness, 400 ms hangover) in the settings, and each other player can be muted there. nexlink can carry the conference later, as an adapter.
- **Why:** a conference of four on one network needs little: low latency (UDP, no retransmission, a lost frame is 20 ms of silence), no account, nothing outside the room. ADPCM is pure Rust, a few lines, and 66 kbit/s a speaker is nothing on a local network; Opus would need a C library on every platform for a gain that only matters over the internet. The microphone is the most private device a game touches: closed until the player opens it.
- **Rejected:** carrying voice in the world's own messages (WebSocket/WebTransport, ordered and reliable: late frames would hold the next ones back); Opus now (a C dependency; to revisit with play over the internet); peer-to-peer (each player sending to each, and finding them through firewalls, for four players on one network).
- **Consequences:** the relay hears everything it relays (it keeps nothing); anyone on the network can join it, as anyone there can join the game. The browser client has no voice yet. With an open microphone and loudspeakers the others hear the game too: a headset is better.

## D-061 — Each track its place: a forest, a coast above the sea, a harbour town

Date: 2026-11-12 · toward the maintainer's goal of a photographed-looking game (D-056 gave four tracks; until now they differed only by their shape and sky).

- **Decision:** a track's `scenery` says what kind of place it runs through, and the components build it. `osage_kart_roadside::trees(scenery)` sets how wooded it is (forest 2.0, coast 0.35, town 0.5, else 1.0): more trees by the track and denser woods from 60 m in a forest. On a coast the land (`Land::with_sea`) falls away on the open side of the circuit to a sea 8 m below the road — cliffs, a shoreline that wanders, a sea bed — and the client draws the sea to the horizon, a lighthouse with its keeper's house on the headland, and no trees where the land meets the water. In a town, houses of two to five floors, gable roofs, windows and doors stand in rows behind the barriers.
- **Why:** a place is what makes a track remembered; the same meadow round four shapes is one track. Everything is still derived from the track's text and the land, the same on every machine, with no new asset.
- **Rejected:** a model per track (an artist's work for each, and content to download); a sea under the whole land (it would show through the hills' hollows inland).
- **Consequences:** a forest costs frames — at T2 on the reference laptop, under load, 49.7 fps against 54.0 for the paddock; the coast and the town cost about what the paddock does. A track file with an unknown scenery gets the meadow.

## D-062 — Time trials: alone against the clock and the ghosts of the best runs, kept as what the kart did

Date: 2026-11-12 · design/04 G4.1, toward the maintainer's goal ("tous les extras").

- **Decision:** a time trial is an event of the kart world, `trial:TRACK`, chosen like a cup (D-057). Every driver races alone from the same place on the grid: no bots, no crates, karts pass through one another and give no slipstream. A run that beats its driver's best is kept as a **ghost** — the kart's state as the lights went and the steps the world applied to it in each tick, not its path (`osage-kart-ghost`) — and driven again by the world in the trials that follow: each driver's own best and the track's fastest, four at most, shown on the board and to agents as racers marked `ghost`. Clients draw them seen through, silent, and nothing touches them. Trials are timed in ticks of the world, not by its clock; their best times are a board of their own in `kart.records/1` (`trials`). Ghosts are kept one a text file in a folder (`kart-server --ghosts DIR`), each checked by driving it once before it is shown: one that no longer starts on the grid and finishes its laps in its last tick, in the time it says, is left out. The others have 90 s after the first driver finishes (30 s in a race).
- **Why:** the physics is deterministic (D-031): the steps are enough, a few kilobytes a run, and the ghost is exact — it crosses the line in the same tick it did. The world drives the ghost, so every client sees it with nothing new to predict, the browser included. Timing in ticks makes a run's time a property of the run: the same on a busy host, and the same when driven again, which is also what lets a ghost be checked. Recording each tick's steps (none while a late step is waited for, two when catching up) keeps the ghost where the kart was at every instant.
- **Rejected:** recording positions (larger, and a ghost that slides rather than drives); the client keeping and driving its own ghost (each client its own code, and no ghost of a friend's run); timing by the clock (a ghost driven again would finish a few milliseconds off its own record); ghosts in races (they would hide the karts that matter).
- **Consequences:** a change to the physics or to a track's shape retires the ghosts recorded before it (their times stay on the records' board). A trial's time is quantised to a tick (16.7 ms). Anyone who can write the folder can write a ghost; it must still be a run the physics drives to the line in the time it says — it need not have been driven by who it names (trust by default, D-038). G4.7's replays can reuse the format for whole races once items and knocks are replayed too.
