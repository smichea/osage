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
