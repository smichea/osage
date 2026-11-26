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

## D-063 — The race as television shows it: a flyover on the grid, the finish again in slow motion

Date: 2026-11-13 · design/04 G4.4.

- **Decision:** the native client presents the race (`show.rs`). While the karts wait on the grid — the host choosing, the others voting — a camera flies the circuit 11 m over the road at 17 m/s, swaying a little, looking 32 m ahead; when the countdown starts, the chase camera eases down from where the flyover was to behind the kart. The client keeps the last six seconds of where every kart was drawn; when the player crosses the line for the last time, the 3.2 s before the line and 0.8 s after are shown again at half speed — first from beside the kart, low, then from the trackside by the line — with REPLAY where the lights are; then the game goes on. A viewpoint given on the command line turns the show off, and the bench has none.
- **Why:** what makes a finish feel like one is seeing it; a circuit seen from above while waiting is learned before it is driven. The client already draws every kart from what it was told: replaying what it drew needs no message, no world change, and works for every event, cups, trials and races alike.
- **Rejected:** the world sending a replay (G4.7 will replay whole races from the inputs; this one is the client's own memory of seconds); a flyover on a recorded path per track (four paths to keep, and none for a track read from a file — the centre line is the path).
- **Consequences:** the browser client has no show yet. A kart's wheels and particles run on the game's clock, not the replay's, so in slow motion the wheels turn too fast. The replay hides the race for seven seconds; a player who finished is no longer racing.

## D-064 — A player's profile on their own computer; paints won in cups, cosmetic only

Date: 2026-11-13 · design/04 G4.6.

- **Decision:** the native client keeps a profile in `osage/kart-profile.txt` (`osage-kart-profile`): the driver, kart and paint raced with last, sent to the world once a connection, and what the player has won — points in cups, cups raced to the end, won, on the podium — counted when a cup the player is in ends. Points unlock paints: the eight team colours are everyone's from the start; silver (20 points), pearl (50), midnight (90) and gold (140) are won. P (a pad's West button) goes round the paints the player has, between races. The world carries the choice as `Choice.paint` (0 the driver's colour, N the palette's N-1; unset, no change), keeps it per session, and paints the racer with it (`Racer.color`); the browser client draws the four new paints.
- **Why:** something to come back for that is the player's own, without an account (no account is ever needed to play, D-038) and without touching how a kart drives: a paint is a look. The profile lives where the player plays, as their settings do, so it follows them from world to world on the same computer.
- **Rejected:** unlocks that change driving (a ladder, not a flavour, against D-055); the world keeping profiles (a world is one host's evening; the player's progress is theirs); the world checking that a paint was won (trust by default, D-038: anyone may wear gold by editing a file, and nobody is harmed by it).
- **Consequences:** a profile does not follow a player to another computer; the browser client keeps none yet. The player's name is still the identity's (`per_world_id`). Parts (wheels, wings) as unlocks are left for when the kart models have them (M5 V1.4).

## D-065 — Battles: three lives each in an arena, the last kart in wins

Date: 2026-11-13 · design/04 G4.2.

- **Decision:** a battle is an event of the kart world, `battle:ARENA`. An arena is a track file too — a wide loop with walls (the Yard: a 280 m stadium bowl, 24 m of road round a grassy island) — kept apart from the tracks (`osage_kart_tracks::arenas`) and checked as an arena: shorter than a circuit may be, no grid. The karts, drivers and bots, start spread round it with three lives each; every thing that hits a kart and every bumper that knocks it takes one, and a kart is spared for 1.5 s after a hit, so that one stack of tyres takes one life. A kart with no life left is out: it coasts, nothing touches it, it touches nothing, and clients draw it seen through. The battle ends when one kart or none is left in, when every driver is out, or after 150 s; the places are the lives, then how long each lasted. A battle's paddock has six racks instead of three and its crates give what hits — tyres, stacks, cones, oil, drones, bumpers — more than what helps catch up, whatever the place; a drone still goes for the kart in first place, the one with most lives. `Race.battle`, `Racer.lives` and `Racer.out` carry it; while racing, `phase_ends_us` is when time is up.
- **Why:** the genre's second mode, made from what is already there: the items, their hits, the bots who use them, the arena as data. A loop of wide road keeps the physics' one rule of walls (distance from a centre line) and lets the bots drive it as they drive a circuit.
- **Rejected:** an open arena with no centre line (a second wall model in the physics, and bots that cannot drive it); losing a life to a kart's knock (a battle of rams, not of items); eliminated karts removed from the world (the player would lose their kart and their view of the end).
- **Consequences:** bots alone take about one life each in 150 s: a battle between bots ends by the clock; players make it shorter. A loop sends everyone the same way round; an arena with crossings waits for a physics of open spaces. No records and no cup points come from battles.

## D-066 — A photo mode: the picture frozen here, a free camera with a full-frame lens

Date: 2026-11-13 · design/04 G4.7 (its photo half; whole-race replays are left for later).

- **Decision:** in the native client, O (a pad's right stick pressed) freezes the picture — every kart where it was drawn — and frees the camera: W A S D move it, E and Q raise and lower it, the arrows turn it, Shift goes faster, R and F zoom (8° to 75°, 17 to 160 mm), Z and X move the focus, 1 and 2 open and close the lens (f/1.4 to f/16); the depth of field is Bevy's, computed for a full-frame sensor (24 mm). The help shows for four seconds, then the interface goes, for F12 to save a clean picture. O again goes back. The world goes on: the player's kart coasts meanwhile. `--focus METRES` with `--camera` puts the same lens, wide open, on the bench's held camera, for pictures of it.
- **Why:** a game that looks like this is photographed by its players, and a long lens wide open is what makes a picture look taken rather than rendered. Freezing the picture locally needs nothing from the world, so it works in any event, alone or with friends.
- **Rejected:** pausing the world for a photo (four friends in one race: nobody else would want it); a camera of the game's own size of sensor (the default, Super 35, gives little blur at the wide angles players start from).
- **Consequences:** a photo taken in a race costs the race. The particles go on while the karts are frozen. A race replay — the world's ticks recorded and stepped again — is still to do (G4.7's other half).

## D-067 — Marks on the track: rubber where tyres scrub, ruts in the grass, fading

Date: 2026-11-13 · design/04 V1.1.

- **Decision:** the native client lays marks where the karts' rear tyres work the surface (`marks.rs`): black rubber on the asphalt through a drift, a slide, a spin or a stop harder than braking alone gives, darker the harder; brown ruts where a kart runs on the grass. They are pieces of strip 0.35 m long and a tyre wide, all karts' in one mesh, built again as they are laid and as they fade over 25 s (4000 pieces at most, the oldest first); a new circuit starts clean. They lie over the road and its baked marks (D-054), which stay as the season's rubber through the bends.
- **Why:** a track shows what was driven on it; marks that appear under the player's own drift are the cheapest proof that the picture follows the game. Laid from the poses the client draws, they need nothing from the world, and the bench draws them as the game does.
- **Rejected:** decals projected on the ground (a second pass for every mark, for a road that is a mesh one can lay strips on); marks kept for the whole race (the asphalt would turn black in a long one).
- **Consequences:** marks are each client's: two players see the same marks only as far as their poses agree. Debris after hits (the rest of V1.1) waits for the world to tell clients what hit what.

## D-068 — Rain: a wet road grips and brakes less; each race's weather rolled from its track

Date: 2026-11-13 · design/04 V2.5 (the "less grip in the physics" its row asks a decision for).

- **Decision:** the physics' `Track` has a wetness, `wet`, 0 dry to 1 in the rain. On a wet road every tyre's footing is 0.6 of a dry road's — a slide takes longer to scrub off, a spin longer to stop — and the brakes 0.7 of their dry force; nothing else changes, and a dry road (×1) gives the same bits as before. A track file says its chance of rain (`rain 0.35`: Gull Point 0.35, Harbour Town 0.3, Pinewood 0.25, the Paddock 0.15); the world rolls each race's weather from it and the race's number (the same roll everywhere), wets its road, and says so in `Race.rain`; clients predict on the wet road (`Track::wet`, `track_wet` in WebAssembly). Time trials are dry, so that ghosts compare. `kart-server --weather track|dry|rain`. The native client shows it: rain falling in streaks round the camera, the road and the painted concrete darker and a third as rough (the sky shows in them), wetting and drying over six seconds; the sun at 0.22 of its light, the sky at 0.45 of its brightness, a grey haze at 380 m; spray from the rear wheels; smoke darkened with the sky. `--rain` gives the bench a race in the rain from its first frame.
- **Why:** weather is what the genre's tracks rarely have and the GTA bar always has; a wet road that drives differently makes it more than paint. Two numbers keep it a change of feel, not a different game: the bots, which brake for bends as on a dry road, still lap every track and keep to it (tested). Rolling the weather from the race's number keeps every client, the bench and a replay in agreement without a message beyond one flag.
- **Rejected:** puddles that change grip in places (a map of the road the physics would have to share with every client: later, if rain plays well); a rain that starts and stops during a race (a change of grip in the middle of a prediction); an overcast photographed sky (a new asset to fetch and ship: the next step for the picture, as the clear sky under rain is the most visible flaw).
- **Consequences:** a ghost recorded dry is not offered in rain (trials are dry). The browser client predicts on the wet road but draws no rain yet. At T2 on the reference laptop, the paddock race in the rain benches about 52 fps against 54 dry (machine loaded).

## D-069 — Rain under an overcast photographed sky, calibrated by its brightness overhead

Date: 2026-11-13 · design/04 V2.5, following D-068.

- **Decision:** a race in the rain is raced under a photographed overcast sky (Poly Haven's Kloofendal Overcast, CC0, the same place as the noon sky), loaded as the race's weather changes, as a track's sky is. A photographed sky is calibrated by its sun (a photographed sky: the sun gives 100,000 lux); an overcast one has none, so a world may say how bright a sky is straight up (`Panorama::overhead`, cd/m²): the panorama is scaled to that — 2,200 cd/m² for a heavy overcast — and the sun light gives what the brightest of its clouds gives (`Prepared::sun_lux`, a few hundred lux: no hard shadows). The camera opens 3.4 stops over a sunny day's exposure. Unlit things — smoke, spray, the rain's streaks — are exposed like the rest, so their colours are divided by what the exposure adds (`scene::unlit`), and the rain's own veil of sun and sky (D-068) applies only under a sky that is not grey already.
- **Why:** under rain the clear sky was the picture's most visible flaw; with the overcast sky the scene is lit as a grey day is — from everywhere, softly, the wet road shining with the bright sky — which is what makes a rainy picture read as photographed.
- **Rejected:** greying the clear sky (its sun and its blue stay in every reflection); a computed overcast (the atmosphere of T2+ has no clouds).
- **Consequences:** the content gains a 16 MB sky; the next release's content zip is downloaded once more by the players. A race's first seconds in the rain may show the clear sky while the overcast one is decoded on its thread.

## D-070 — The karts heard from the player's kart: grains of the engine, distance, panning and Doppler

Date: 2026-11-13 · after the maintainer's play test on the reference RTX PC ("lancinant"; quiet in the chase view, silent from above; "on devrait entendre les autres karts quand ils arrivent et dépassent").

- **Decision:** the recorded engine loops are played as grains (`osage-kart-sound`): pieces of 120 ms taken from anywhere in the loop, two at a time, overlapping by half under half-sine windows (equal power). The high-revs loop is 1.1 s long: played as it was, its pattern came back every 0.9 s at top speed — the drone the ear would not let go of; as grains it keeps its timbre and loses its period. The kart voices are no longer placed by the audio engine's spatial sinks, whose ear was the camera and whose loudness fell with the square of the distance (the chase camera, 3.4 m back, heard a tenth; the high view almost nothing): the game places them itself (`sound::heard`). The ear is in the followed kart, turned as the camera looks; that kart's own voice is heard centred at full level in every view; each other kart at `9 / (9 + d)` of its level (fading to nothing at 220 m), panned at equal power by where it is across the view, and shifted in pitch by the Doppler effect of its speed and the player's along the line between them (`(c + v_ear·u) / (c + v_kart·u)`, c = 343 m/s, within 0.8–1.25), the voice resampled in its stereo stream, its gains and pitch eased over 20 ms. The crowd and the drones stay with the audio engine's spatial sinks, heard from the camera.
- **Why:** what was wrong was what a player hears most: their own engine. Grains are the usual cure for a short loop; a listener in the player's kart is what racing games do, so that the view chosen does not change the mix.
- **Rejected:** longer loops (the original recording has no longer steady passage at high revs); the audio engine's spatial sinks with a scale (no Doppler, an inverse-square fall that cannot be shaped).
- **Consequences:** a kart's sound now costs a resampling in its stream (two multiplications a sample). The player's engine does not move with the camera's look-back (Q, E, X): it is the ear that is in the kart.

## D-071 — A player's own items drawn at once, where the world will put them

Date: 2026-11-13 · from the play test on the reference RTX PC ("le lancer de pneu se fait à quelques mètres de la voiture"; a cone or oil dropped seemed to land far behind).

- **Decision:** where and how fast a thing leaves a kart is one function of `osage-kart-items`, `released(item, kart)`, which the world uses to release it and the native client to draw it: when the player's kart lets go of a tyre, a stack's tyre, a cone or oil — the use button let go while it was held behind — the client draws it at once from the predicted kart (`PaddockView::mine`), a tyre rolling on, until the world's thing of that kind arrives within 3.5 m of it, or for 1.5 s.
- **Why:** the world places the thing right against the kart, but the client saw it only a round trip later, by when the predicted kart had gone on a few metres: a cone or oil appeared metres behind, a tyre metres ahead. The prediction is exact, so the client's thing and the world's lie in the same place.
- **Consequences:** a release the world refuses (an item the client did not yet know was gone) shows for 1.5 s at most.

## D-072 — Oil takes the tyres' grip, not the kart's speed

Date: 2026-11-13 · from the same play test ("le passage sur la flaque d'huile est pas réaliste").

- **Decision:** the physics' kart has `slick`, seconds its tyres stay oily: while it lasts the footing is 0.12 of what it would be and the steering 0.3, so the kart slides on the way it went and the wheel hardly turns it. Oil keeps a kart's tyres oily while it is in the patch and 0.6 s after; driving in keeps 97 % of the speed (it was 90 %) and starts a slow turn of 1.1 rad/s (it was a spin of 3.5). Clean tyres give the same bits as before.
- **Why:** on oil a real kart does not stop or spin on the spot: it stops gripping, and goes where it was going.
- **Consequences:** `kart.state/1` carries `slick`; the WebAssembly field numbers of the build move up by one (16 is now `slick`, the build 17 to 21). A ghost's run starts on clean tyres.

## D-073 — Crash reports filed as GitHub issues through a relay, with the player's consent

Date: 2026-11-13 · at the maintainer's request (no more asking players for their logs), with the choices they made: a relay on Cloud Run, the players asked once, DeepSeek reading the logs.

- **Decision:** an engine crate, `osage-report`, watches a program's runs in a folder of its own: a marker (`starting`, then `started`, removed by a clean exit), the run's log (a tracing layer), a panic's message and trace (its hook), and a watchdog that notes a start not finished after a while. At the next start it gives the report of the run before, if there is one to make — a panic, a start that never finished (the marker still says `starting`: hung, or closed before anything was shown, as the grey screen at T3/T4 on the reference RTX PC), or errors of the renderer in the log — cleaned on the player's computer (home folder, user and machine names, network addresses, e-mail-like names), with the start of the log (the machine, the graphics) and its end. A run that started and was then killed is not reported. The kart game keeps these in `osage/kart-reports/` and, when there is a report, asks the player once: send, always, not now, never (kept in `consent`). A report goes to a relay (`tools/crash-relay`, Python, standard library only), deployed on Cloud Run in the project `osage-bench` with the only key to the repository (an issues-only token in Secret Manager). The relay works out the report's signature without AI — the error's message and the first frames of its trace with numbers, addresses, paths and quoted values taken out — ends the issue's title with it, and comments on the issue that has it or opens one; with a DeepSeek key it heads a new issue with DeepSeek's short reading of the log, marked automatic and unchecked. It takes 512 KB at most a report and twelve reports an hour from one address; text from a report is made inert on GitHub (no mention, no closed fence).
- **Why:** a player who meets a crash should not have to find a log; a developer should get each problem once, with its machine. A relay keeps the key out of the game (a key in a zip is a key for anyone); a signature without AI keeps duplicates out even when the AI is down or wrong.
- **Rejected:** a token in the game; pre-filled GitHub pages (a GitHub account for every player); deduplication by AI.
- **Consequences:** reports are public (the repository is), and DeepSeek reads them: the question to the player says both. The relay is a service to keep running and its token to renew within a year. A world chooses its own relay or none (mechanisms, not policy).

## D-074 — 30 frames a second by default

Date: 2026-11-13 · the maintainer's choice ("par défaut le nombre de fps goal devrait être 30").

- **Decision:** the kart game aims at 30 frames a second unless the player chooses another rate in the settings; within the tier, the frame pacing's ladder (render scale, effects, shadows; `osage-render`'s `pace`) keeps the whole picture more often at 30 than at 60.
- **Why:** a kart race at 30 steady frames with the whole picture reads better on the friends' PCs than 60 with the picture cut down.
- **Consequences:** a player whose settings file already says 60 keeps it until they change it.

## D-075 — The player chooses a frame rate; the game chooses the tier, and steps down where one fails

Date: 2026-11-13 · the maintainer's request ("il ne faut plus avoir à choisir de tiers, juste les fps cible").

- **Decision:** the settings have no quality any more: the game asks the graphics adapter, before it starts, as Bevy will pick it (high performance; Direct3D 12 on Windows unless `WGPU_BACKEND`), and gives a discrete one T3, an integrated one T2, none T1 (T4, ray tracing, only with `--tier T4` until it is ready). Each start notes its tier; if the run before, at a tier chosen so, did not finish starting, panicked or reported renderer errors (D-073), that tier is marked failed on the machine (`osage/kart-reports/failed`) and the next lower one is used from then on. Ten seconds into the race the game looks at one frame: a picture of one colour — the grey screen T3 gave on the reference RTX PC — marks the tier failed, leaves the run to be reported, and starts the game again a tier lower. `--tier` still forces one (the bench, tests); the meter shows the tier in use.
- **Why:** the tier is what a machine can draw, not a taste; the frame rate is the taste. A machine where a tier fails should never show it twice, and its player should not have to know why.
- **Consequences:** the first start on a machine whose T3 does not work shows ten seconds of it before the game starts again at T2; its report tells why.

## D-076 — Karts knock in the prediction: the others stepped on with their own controls

Date: 2026-11-13 · from the play test on the reference RTX PC ("les karts se traversent et rebondissent pas de façon réaliste"), and the maintainer's idea of streaming every player's controls so that each client works out the collisions near it.

- **Decision:** a kart's state (`kart.state/1`) carries the controls of the last step the world applied to it (`throttle`, `steer`; the drift button is `held`), for every kart — players, agents, bots. The native client steps the other karts on with those controls, held, by as many steps as its prediction is ahead of the world's state, with the same physics: it draws them there, and its prediction knocks the player's kart into them there (`bump`, as the world does) at every step, replayed ones included. What a knock does to the other kart stays the world's to say.
- **Why:** the player's kart was predicted alone: on the player's screen it drove into another kart, and the world's knock arrived a round trip later as a correction — karts passing through one another, then a jump. With deterministic physics, the controls are enough to put the others where they are at the player's moment (a tenth of a second on, among bots that knock one another: a few centimetres off, measured, and nearer than a straight line), so the knock happens on screen when it happens.
- **Rejected:** each client authoritative for its own kart, the others shown as clones (what peer-to-peer racing and open-world games do: two screens can disagree on who hit whom, and a host cannot referee); the world waiting for every input before each tick (latency for everyone).
- **Next:** the others' controls are held over the lead; a driver who turns in during it is found a state late. Predicting what they do next — with the bots' driving model, or better a model of each player's own way of driving, learnt from their races (the maintainer's "avatar") — is the step after. The browser client still predicts its kart alone. Things thrown (tyres, cones) still answer a kart with a fixed slow-down and spin, not an impulse.

## D-077 — Hills, bridges and tunnels: a road with a height everywhere

Date: 2026-11-13 · the maintainer's request after the play test ("les terrains ont au mieux une petite bosse; il faudrait des terrains avec plus de dénivelé, un en montagne, un dans un canyon avec des ponts, des tunnels").

- **Decision:** the physics' track may have a height at every point of its centre line (`Track::with_heights`, joined straight between points; the crests of D-051 add to it). A kart is carried at the road's height; uphill its top speed falls and downhill it rises with the slope it faces (×(1 − slope), within 0.8–1.15), and the slope pulls a moving kart back or on (9 m/s² per unit of slope: half of gravity, an arcade's). A level track gives the same bits as before. In a track file a point may have a third number, the road's height there, joined by the spline that joins the points; a road steeper than 16 % or a grid on a slope is refused. `bridge AFTER LENGTH` and `tunnel AFTER LENGTH` are scenery the driving does not feel: the land (`osage-kart-land`) falls into a gorge 16 m deep under a bridge and stands as a ridge 12 m high beside a tunnel, the country around a hilly track lies at about the road's height, a mountain's hills are 2.6 times taller and a canyon has rock walls 26 m high; the native client draws a bridge's deck, parapets and piers, a tunnel's vault (lit, a lamp every eight metres and a light every sixteen), its rock and portals, and bare rock where the ground is steep (grey granite, red sandstone). Two tracks come with it: Eagle Ridge, in the mountains (1.1 km, 40 m of climb, a tunnel under the summit, a bridge over a ravine), and Red Gorge, in a canyon (1.26 km, two bridges, a tunnel); and a cup, High Roads (Pinewood, Eagle Ridge, Gull Point, Red Gorge). `kart.track/1` carries `heights` and `spans`; the browser client gives the heights to its physics.
- **Why:** relief is what makes a track a place and a lap a story — the climb, the summit, the view down — and what flat circuits cannot give.
- **Rejected:** a road that crosses itself (a bridge over the track: the physics finds where a kart is by the nearest point of the centre line, which a crossing makes ambiguous); banking (later, with the models of M5).
- **Consequences:** a ghost recorded on a track whose heights change no longer holds (D-062). Bridges and tunnels are seen, not felt: no barrier differs, nothing falls off a bridge.

## D-078 — A professional interface: Saira under the OFL, panels, keycaps, and a quieter own engine

Date: 2026-11-13 · the maintainer's request after the play test ("l'UI (textes, input) est très médiocre et pas facile à utiliser, retravaille-la pour qu'elle soit de qualité professionnelle"), their choice of fonts ("OFL pour les polices: Saira, Bold pour les chiffres et titres, Regular pour les textes"), and "le son du kart est un peu fort en mode par défaut".

- **Decision:** the native client's interface is redrawn on one look (`theme`): Saira, from the content, Bold for figures and titles (in place of Bevy's default face, so that every text has it) and Regular for text; rounded see-through panels with a faint edge, amber as the accent, gold, silver and bronze for places, opaque dialogs; keys shown as keycaps, the arrows drawn; the layout drawn for 720 lines and scaled with the window. The HUD is laid out as racing games lay theirs (place, lap and clock top left; the item top centre; the standings top right, a row a kart, the player's lit; map and speed bottom right; between races, three cards over a bar of keys); the settings are a panel of rows with what the one chosen does written under them; the crash-report question (D-073) is a dialog with its four answers as keys. Texts no longer name the keys the keycaps show; lives are written in words. The meter shows the frame rate alone (D-075 had it show the tier: the player has no say in it any more). `--bench --ui-preview` draws all of it over the bench's race, to be looked at before a release. The player's own engine is a little softer from behind (×0.75, about 2.5 dB) and from above (×0.63, about 4 dB) than from the seat (D-070 had it as loud in every view).
- **Fonts under the SIL Open Font Licence 1.1:** the content stays CC0 but for the fonts, which come from their authors' repository at a fixed commit (Omnibus-Type/Saira), with the OFL's text beside them in the content, as the licence asks. Kenney's KenVector Future (CC0) was tried first and dropped: its K and X read as H in a racing game's words.
- **Why:** an interface read at a glance at speed — large figures, colours that mean something, keys shown as keys — is half of what makes a game feel finished; a monospace text block over the picture did not. From behind, the player's engine at the seat's level covered the others' coming and going (D-070's point).
- **Rejected:** the variable Saira (one file for every weight; two static faces are simpler to check and to draw); Kenney Future; images of text.
- **Consequences:** the content zip carries `saira/` (three files, 0.23 MB). Without the content the interface falls back to Bevy's face, with the same layout.

## D-079 — Thrown tyres and cones are bodies: an impulse where they meet a kart

Date: 2026-11-13 · the remainder of the play test's third point ("les collisions ne sont pas réalistes, ni entre les karts ni entre les objets et les glissières ou kart").

- **Decision:** a thrown tyre and a cone meet a kart as two karts do (D-045): the physics' `struck` exchanges an impulse where the body touches the kart's outline — momentum kept, the body's give, the rub along — so that the kart is pushed and turned by where the body lands, and the body is sent off and stays on the track. A tyre weighs a quarter of a standard kart (a real one a fiftieth: the game's one liberty with the weights, so that a tyre is felt) and gives back 60 %; then it goes under the kart's wheels: 70 % of the speed kept, drift and boost lost, the grip gone for half a second, and a spin of at least 3 rad/s the way the blow turned it. A cone weighs a fortieth of a kart and gives back a fifth: the kart keeps 92 % of its speed and the cone flies off, slides and tumbles (9 m/s²), bounces off the barriers and lies where it stops, for the next kart. A thing knocked by a kart spares it 0.4 s. The native client draws a knocked cone sliding and tumbling.
- **Why:** a tyre that stopped a kart dead and vanished, whichever way it came, and a cone that vanished and pushed the kart sideways, read as scripted; a blow at the nose and one at the tail must turn a kart opposite ways, and a cone hit at speed must fly.
- **Replaces:** D-052's fixed responses (a tyre: 45 % of the speed kept and 5 rad/s of spin; a cone: 85 % and a 1.5 m/s knock; both gone).
- **Consequences:** a tyre that bounces off one kart can hit another (a life each in a battle, D-065); a cone knocked onto the racing line stays a hazard. The browser client still draws no things.

## D-080 — Bridges and tunnels built as a game studio builds them

Date: 2026-11-13 · the maintainer's reminders ("qualité GTA V, oublie pas stp") after D-077's bridges and tunnels: the tunnel black inside, the hillside through its vault, the bridge's edge reading as two decks.

- **Decision:** `spans` sweeps every piece along the track's frames from a cross-section, with texture coordinates in metres and tangents, under photographed CC0 sets at their real size — Poly Haven's `concrete_wall_008` (2.7 m) for concrete, `rock_face_03` (4 m) for rock, red-tinted in a canyon — and a colour per vertex for the weather's marks: a fascia streaked and darker low, a deck's underside darker, columns splashed at their foot, a tunnel's lining grimed by the road's spray and sooted at its crown. A bridge: deck; fascia and parapet one face under a cornice; a crosshead under the deck every 18 m on two round columns sunk 6 m into the gorge. A tunnel: the lining faced with pale panels (painted concrete's grain under cream, matt: the scene's reflections are the sky's) up to 3.4 m; lamps in housings high on the walls, one side then the other every 8 m, each a light of 3·10⁷ lm (about 30 000 lux on the road, a tunnel's entrance zone under daylight's exposure; 4·10⁶ gave a black tunnel); headwall portals with a coping. The land keeps under the road up to 3 m past the barrier's face in a tunnel (`TUNNEL_CLEAR`), so that the ridge, joined by the land's grid of about 4 m, no longer crosses the vault.
- **Why:** shapes scaled from cubes carry their texture stretched over metres or none, and read as toys; a game of this ambition builds its structures from profiles with materials at their real size and weathering where water and traffic leave it.
- **Rejected:** a reflection probe inside each tunnel (the right way to stop the sky's reflections indoors; later, with M6's reflections); emissive panels.
- **Consequences:** the content gains the two sets (18 MB). Each tunnel lamp is a point light: a 70 m tunnel has nine.

## D-081 — Rock on steep ground, and cliffs in a canyon

Date: 2026-11-13 · the maintainer's reminder ("qualité GTA V, oublie pas stp"); Red Gorge's walls read as smooth orange clay.

- **Decision:** the ground material takes a rock layer (`Ground::with_rock`): on steep ground a photographed rock grain (Poly Haven's `rock_face_03`, CC0, already in the content for the tunnels) projected along the three axes, its contrast raised, with strata across it and varnish streaks down it; the land's own colours stay (red sandstone, grey schist). A canyon's walls (`osage-kart-land`) become cliffs: faces and ledges in steps of 8 m whose heights wander, spurs and gullies every few metres, some faces slumped. The land's grid around the circuit is 40 % finer (280 cells a side: 1.3 to 3.4 m) so that it carries them.
- **Why:** relief and material are what read as rock at a glance; a ramp painted orange reads as a toy whatever the light.
- **Rejected:** separate rock meshes stood against the walls (more draw calls and seams; later, for boulders and overhangs); triplanar normal maps (the next step).
- **Consequences:** the T2 kart bench on Eagle Ridge: 19 ms a frame on the Iris Xe laptop (53 fps).

## D-082 — The whole race shown again, as television would

Date: 2026-11-13 · design/04 M4 (whole-race replays), toward the maintainer's goal of Mario Kart's extras.

- **Decision:** the native client keeps every kart's pose thirty times a second from the countdown to the results. Between races (results, then the grid while the host has not started), R shows the race again: the camera cuts between cameras by the track (one every 42 m on alternate sides, 1.3 to 5.8 m up, the next ahead of the kart followed, cut when it is 12 m past), a helicopter's behind and above (every fifth shot) and one on the kart behind its driver (every fifth, and always in a tunnel, where no other sees). Left and right follow another kart, up and down run the replay at ¼, ½, 1, 2 or 4 times, Space pauses, R goes back; it loops; the next countdown ends it. The cards between races give way to a line naming the kart followed, with the race clock, the speed and the keys; the keycap bar gains R REPLAY.
- **Why:** a race seen again from the trackside is half of what makes it a story to tell, and a game's showcase of its own pictures.
- **Rejected:** keeping it on disk and an editor (later: the tape is a few megabytes, and the photo mode already frames shots); replaying the things on the track (their tape would be the world's).
- **Consequences:** a few megabytes of memory for a long race (eight karts, six minutes). Only in a game, not on the bench.

## D-083 — Content updates download what changed

Date: 2026-11-13 · two content changes in a day (the fonts of D-078, the concrete and rock of D-080) would each have had every player download 234 MB again.

- **Decision:** a release may carry, beside its whole content zip, patches named `osage-kart-content-from-<the first 16 hex digits of an earlier content zip's digest>.zip`, each holding the files added or changed since that content (the manifest among them). `kart-update` takes the patch for the content it installed when there is one, else the whole zip; either way it records the whole zip's digest. Patches are made at release time from the earlier content zips kept.
- **Why:** content grows by a few sets at a time; the friends playing should download megabytes, not the whole library.
- **Rejected:** fetching each file from its source (the manifest's URLs): some sources ask a person to download, others are derived files, and a player's game would depend on a dozen sites; deleting files a patch no longer has (content only grows; a stale file is harmless).
- **Consequences:** an updater from before this decision still downloads the whole zip once; patches serve from the next update on.

## D-084 — The browser draws the paddock's things and the rain

Date: 2026-11-13 · the browser client drew neither the crates and things the world sends (D-052) nor the rain (D-068): a player in a browser drove through tyres and oil they could not see.

- **Decision:** the browser client draws `kart.paddock/1` — crates bobbing and turning, hidden while broken; tyres rolling; cones standing, or sliding and tumbling once knocked; oil spreading; drones flying — each on the road's height under it, which the WebAssembly physics now gives for any point (`track_along`, then `track_lift`); and, when the race is wet, rain streaks round the camera and a grey sky and haze, easing over four seconds. Meshes are pooled and reused frame to frame.
- **Why:** the world is the same for every client; what changes the race must be seen in all of them.
- **And the items:** the browser sends the item button with each step (F or Ctrl, a gamepad's North button, an ITEM button on a touch screen) and shows the item held; a player in a browser races with items as the native client's do.
- **Consequences:** the browser draws things at the world's time, not as far ahead as its prediction.

## D-085 — Races at night, under floodlights

Date: 2026-11-13 · design/04 M6 (time of day), toward the maintainer's "qualité GTA V".

- **Decision:** a track may say how often it is raced at night (`night 0.25` in its file); the world rolls it for each race as it rolls the rain, with a roll of its own, and says so in `Race.night` (`--time track|day|night` on the server forces it). Only the look changes. The native client then shows Poly Haven's `kloppenheim_02_puresky` (CC0), its sky brightened to 0.6 cd/m² overhead so that it reads deep blue with its stars and its moon a faint moonlight, exposed 7.5 stops over a sunny day for floodlights; a mast every 40 m round the circuit on alternate sides, 13 m tall beyond the barrier, its spot light (2.4·10⁶ lm, without shadows) aimed at the road 14 m ahead — about 500 lux where it lands, a club circuit's night lighting — none in a tunnel (its own lamps) nor on a bridge; a lamp under each kart's nose and a red glow at its back. The browser darkens its sky and lights. The built-in tracks are raced at night 10 to 30 % of the time.
- **Why:** night under lights is half of what a racing game shows of its light — pools of white on the asphalt, karts' lamps in the dark, the sky's stars — and every circuit gets a second face.
- **Rejected:** shadows from the floodlights (twenty-odd spot shadow maps: later, the nearest few at T3); street lamps' sodium orange (a race circuit is lit white).
- **Consequences:** the T2 bench on Eagle Ridge at night: 23 ms a frame on the Iris Xe laptop, against 19 by day (the pacing lowers the picture to hold the rate chosen). The content gains the night sky (22 MB).

## D-086 — Two players on one computer: a split screen

Date: 2026-11-13 · design/04 M4 (split screen), toward the maintainer's "tous les extras" of Mario Kart.

- **Decision:** `kart-native --players 2` (and `play-two.bat`) splits the screen, one player above the other. The second player opens a connection of their own to the world: to the world they are another player, with their own kart and their own controls; the first's connection keeps the world's state, which serves both. Each kart is predicted at this computer (D-031, D-076) and followed by its own chase camera; the two cameras draw into the same off-screen picture, the second after the first without clearing it. Keys: player 1 W A S D, Space or left Shift to drift, F or left Ctrl for the item, the first gamepad; player 2 the arrows, right Shift, right Ctrl or /, the second gamepad. Whichever of them is the world's host starts the race. Player 2's place, lap, speed and item are a line at the top of their half. The replay, the flyover and the photo mode keep to player 1's camera; sound is heard from player 1's kart. `--headless` with `--screenshot-after` plays a game without a window, for checks like this one.
- **Why:** racing a friend on the same sofa is half of what a kart game is for.
- **Rejected:** a second kart driven through the first's connection (the world gives one kart per player: a second identity is the world's own rule, and it keeps the two players' records, places and points apart).
- **Consequences:** between races player 2 picks a driver and a kart with the arrows and the second gamepad's d-pad (player 1 then with D A and S W), named on their line; player 2 paints their kart with `.` or the second gamepad's West, among the paints this computer's profile has won (the household's); each particle faces the nearer camera; two predicted karts and two cameras cost about a second view's rendering.

## D-087 — Drivers who react: cheering on the podium, dizzy in a spin

Date: 2026-11-13 · design/04 M5 (animated drivers), toward Mario Kart's characters who react.

- **Decision:** a driver who has finished on the podium raises the left fist above the helmet and waves it (`osage-kart-model`'s `driver_cheering`: the wrist's target eased from the rim to above the shoulder, the arm solved as before, its lengths kept); a driver whose kart spins shakes their head, as fast as the spin. The world's `Racer` says who finished where; the client draws it (`Pose.cheer`).
- **Why:** a character who answers what happens to them is what turns a kart with a figure on it into a driver.
- **Consequences:** the bench's drivers never cheer (no one finishes there); hands off the wheel are drawn, not felt: the kart coasts as before.

## D-088 — How hard the bots race: easy, normal, hard

Date: 2026-11-13 · toward Mario Kart's playability: a race should be winnable by a child and hard for a regular.

- **Decision:** the world has a difficulty for its bots (`osage-kart-racecraft`'s `Difficulty`): their temperaments are drawn as ever, then tempered — easy: slower on the straights, timid in bends, gentler on others, more mistakes, no drifts; hard: faster, bolder, steadier, every one drifting. The host chooses it between races (L in the native client; `Choice.bots`), from the next line-up; `--bots easy|normal|hard` sets it on the server; `Race.bots` says it, and the event card shows it. Normal is what the bots were.
- **Why:** with one strength of bots, a race is either too hard for a newcomer or too easy for the friends who play every evening.
- **Rejected:** rubber-banding (bots that speed up behind and slow down ahead): the items already help the back of the field (D-052), and a bot that cheats is felt; engine classes (50 to 200 cc) — another decision, as they change the physics every client predicts with.

## D-089 — Engine classes: 50, 100, 150 and 200 cc

Date: 2026-11-13 · toward Mario Kart's playability: a slow class to learn a track, a fast one for those who know it.

- **Decision:** a race is run in an engine class, 150 cc by default. The physics' track carries it (`Track.class`): every top speed and acceleration — the kart's, a mini-turbo's, a rocket's — times 0.7 at 50 cc, 0.85 at 100, 1 at 150 (bit for bit the old physics), 1.15 at 200; braking and grip are the same in every class, so 200 cc asks for earlier braking. The host chooses it between races (G in the native client; `Choice.class`), from the next race; `--class` sets it on the server; `Race.class` says it; the bots' pace follows it; both clients predict in it (`track_class` in WebAssembly).
- **Why:** speed is the difficulty a player feels most, and the one that grows with them.
- **Rejected:** scaling the karts' builds (they are kept within ±15 %, a flavour, D-055); faster turning at 200 cc (the handling a player has learnt should not change with the class).

## D-090 — A failing tier loses one technique at a time, not all of them

Date: 2026-11-13 · the reference RTX PC shows a grey screen at T3 and has raced at T2 since (D-075), its report not yet sent: it lost ambient occlusion, temporal anti-aliasing and sharp shadows for what is likely one technique.

- **Decision:** after a failure at a tier (a run that went wrong, or a picture of one colour), the next start runs the same tier without one technique more, the likeliest culprit first — the sky as a light (an atmosphere computed on the GPU), post-processing (temporal anti-aliasing, motion blur, which need a depth prepass), ambient occlusion, shadows — and keeps the first set that works for the machine (`kart-reports/trimmed`, e.g. `T3 sky-light`); only with all four left out is the tier marked failed and the next lower one used. A tier marked failed before this is tried again once in this way. The log says what is left out.
- **Why:** each failed start costs ten seconds; four of them buy the machine the best picture it can draw, and tell us, through the report, which technique fails on which hardware.
- **Rejected:** trying each technique alone (more starts for the same answer when one is the culprit); asking the player (they cannot know).

## D-091 — The world's leaderboard, as a page

Date: 2026-11-13 · the maintainer's goal names it: "tous les extras comme leaderboard".

- **Decision:** the world publishes every track's records (`kart.leaderboard/1`: each as `kart.records/1` has the current track's), and the browser client has a second page, `leaderboard.html`, that connects as an observer — the world gives an observer no kart and no place on the grid — and shows, live, every track's best laps, races and time trials (the best five, who, with which driver and kart, when, new ones marked) and the race being run (its phase, laps, class, bots, night, rain, the order). On Windows, `play.bat` and `play-two.bat` serve it at `http://localhost:8080/leaderboard.html`, and `host.bat` at the host's address for everyone on the network.
- **Why:** records seen only between races on one's own screen are a list; a page anyone can open, on a phone at the side of the sofa or on the next computer, is a leaderboard.
- **Rejected:** a leaderboard shared between worlds over the internet (it would need an account system and a server of ours; each world's own is the next step's foundation).

## D-092 — Painted walls and kerbs on the grain of cast concrete

Date: 2026-11-13 · seen at T3 up close: the red and white barriers looked stained.

- **Decision:** the photographed circuit's painted walls and kerbs take the grain of Poly Haven's `concrete_wall_008` (fine, even, the texture of cast concrete under paint) in place of `painted_concrete`'s, a photograph of peeling green paint whose patches read as stains from the road; the latter stays the fallback.
- **Why:** paint on a circuit's wall is renewed every season; what shows through is the concrete's pores, not peeling.

## D-093 — T4: light traced by the hardware (Bevy Solari)

Date: 2026-11-14 · the maintainer's choice for the next step toward "GTA V" lighting: "Lumière RT (RTX)".

- **Decision:** T4 adds Bevy Solari (the `raytracing` feature of `osage-render` and `osage-kart-native`): direct and indirect light traced through the scene on hardware with ray queries (an RTX card, a Radeon from RDNA 2, an Intel Arc). Solari traces meshes of exactly position, normal, texture coordinates and tangent, shaded with the standard material: each mesh drawn with it gets a double for the rays alone (the drawn mesh keeps its vertex colours), and each ground drawn with the ground material a stand-in — its double under a standard material of the ground's colour. Solari draws every opaque material deferred, so the ground material gains its deferred shader (the same WESL, writing the G-buffer) and the T4 camera always has the deferred prepass: on hardware without ray queries Solari stays out and T4 is a deferred raster picture. The traced light is one more suspect in D-090's search (the first at T4). The cloud bench (D-042) builds with the feature, so its T4 run on the NVIDIA L4 is the traced picture.
- **Why:** bounced light and true reflections are what separate a rendered scene from a photographed one; hardware that traces them is in the players' machines.
- **Measured** on the cloud bench's NVIDIA L4 at 2560×1440 (2026-11-14): the first run stopped on an empty mesh (no double is made of one now); the second had black karts — Solari traces hidden meshes too, and each kart's hidden guard bubble enclosed it (hidden meshes now leave the traced scene); the third, 16.7 ms a frame but the sunlit asphalt sparkling with noise; with ReSTIR on, 26.3 ms (the 30-frame budget is 33.3) and a soft, sky-lit picture. T3 on the same machine: 12.9 ms.
- **Consequences:** the Windows build has the feature: on an RTX card `play.bat --tier T4` shows it; the game does not choose T4 by itself until a machine like the testers' has run it. Skinned drivers are traced in their rest pose; no learned denoiser.


## D-094 — Night under traced light: lamps are shining surfaces

Date: 2026-11-14 · after the cloud bench's first night-and-rain run of T3 and T4 (D-093).

- **Decision:** a drawn mesh can be marked a lamp (`osage_render::Emitter`, a luminance in cd/m²): traced light counts it as a light source of that luminance — its double, shining, beside the drawn face — whatever the drawn material shows. The floodlights' faces shine at 500 lux on the road 20 m off (about 1.8 × 10⁵ cd/m² over 1.6 × 0.7 m); each kart gets lenses — white at its nose, 30 lux 10 m ahead, red at its tail — drawn as dim glowing faces. A raster picture keeps lighting with the spot and point lights (D-085) and ignores the mark. Two fixes from the same run: the rain's haze is exposed like the rest (it glowed grey at night), and the flames' and sparks' glow takes two thirds of the night's exposure (it was the full daylight value, blown out).
- **Why:** Solari has no punctual lights — only the sun, the sky and emissive surfaces: the night picture at T4 was black but for the lamps' faces. A lamp's face that lit the road would be far too bright for the eye at night's exposure, so the eye and the rays get different faces.
- **Consequences:** a world's lamps light T4 only if it marks them. The headlamps add two small meshes per kart at night.

## D-095 — T4: glows exposed as the raster shows them; the traced light's grain filtered

Date: 2026-11-14 · after the night-and-rain cloud run of D-094: the rear lamps did not glow at T4 and the picture was grainy.

- **Decision:** (1) while light is traced, each opaque glowing standard material's glow is set to the luminance that the camera, exposing it, shows as the raster does (the raster draws a glow by its `emissive_exposure_weight`, unexposed by default; Solari exposes every glow), recomputed when the camera's exposure changes; the traced lamps of D-094 are luminances already and are left alone. (2) An edge-avoiding à-trous filter (osage's own, `denoise.wesl`: three passes of a 3 × 3 kernel 1, 2, 4 pixels apart, weighted by facing, plane and the light's local spread as SVGF weighs them) smooths the traced light divided by each surface's colour, after the opaque pass and before see-through things; temporal anti-aliasing steadies it. `OSAGE_DENOISE=0` turns it off, `=force` runs it on hardware without ray queries.
- **Why:** with the camera exposed for the sun (night's exposure, D-085, is a grade after it), Solari showed a lamp's glow 40 000 times darker than the raster. DLSS ray reconstruction, Solari's denoiser, is a proprietary SDK (AGENTS.md rule 2); a filter of published methods is not.
- **Measured** on the cloud bench's L4 at 2560×1440, night and rain: 34.3 ms a frame with the filter decoding every tap (3.7 ms, over the 33.3 budget), 32.4 ms once each workgroup decodes its pixels once into workgroup memory (1.9 ms). The kart is no longer speckled; low, blotchy grain remains on the wet asphalt.
- **Consequences:** a world's glows look the same at T3 and T4. T4 at night in the rain has under 1 ms left on the L4.

## D-096 — A kart's body tumbles: lies on the road's slope, rolls through a trick, bounces on landing

Date: 2026-11-14 · the maintainer, watching the bench: spun round on a crest a wheel sank into the road; a trick's roll stopped short before landing, "pas très réaliste … une physique plus précise de rebond", and a wheel still went into the road.

- **Decision:** the drawn body, apart from the kart's path (which the physics keeps), (1) lies on the road whichever way it points: the road's rise taken along its nose pitches it and taken across rolls it, where the nose's pitch came from the climb divided by the forward speed (near zero sideways, so up to 29° about the wrong axis); (2) rolls through a trick as a rigid slab (about 160 kg, 1.4 × 1 m, its centre of mass 0.35 m up) turning freely in the air, at the rate that lands it upright when the jump is long enough (two turns a second at most); landing, it keeps falling as the kart fell, its outline (tyres, shoulders, helmet) pushed out of the road and bounced (30 % of the speed into the road given back) — a tyre landing first rights it or tips it; on its side or back it is rocked over onto its wheels. Dust rises from each rear tyre on grass, not from both when the kart's centre leaves the road; motion blur takes four samples (one, dithered, showed the road through a spinning tyre).
- **Why:** an arcade kart's tricks read as physical when the body obeys inertia and contact; the race's physics stays the simple, deterministic one the servers share.
- **Consequences:** drawn only: a kart's hit box and path are unchanged; bodies of karts seen through a replay or a network tumble the same way from the same path.

## D-097 — T4: the traced light averaged over the frames before

Date: 2026-11-14 · after D-095: low, blotchy grain stayed on the wet asphalt at night.

- **Decision:** before its spatial passes, the denoiser averages each pixel's light (divided by its surface's colour) with what the same surface showed over up to 16 frames: found by the pixel's motion vector, kept only where last frame's G-buffer (kept for ReSTIR) faced the same way and stood on the same plane, and pulled within two standard deviations of the light round the pixel now (variance clipping), so that a moving shadow or lamp leaves no trail. Without last frame's G-buffer the step is skipped; `OSAGE_DENOISE=force` keeps it on hardware without ray queries, to check reprojection on a raster picture.
- **Why:** spatial filtering alone trades grain for blotches; a few frames of the same surface are samples for free (Schied et al. 2017's temporal accumulation, Salvi 2016's variance clipping).
- **Measured** on the cloud bench's L4 at 2560×1440, night and rain: 32.8 ms a frame (0.4 ms for the step, within T4's 33.3); the blotches on the wet asphalt are gone, the floodlights' reflections smooth.
- **Rejected:** feeding the anti-aliasing's history (clamped to the noisy neighbourhood, it kept the grain).

## D-098 — A kart that looks raced: rims, scrubbed tyres, a broken livery, dust

Date: 2026-11-14 · the maintainer chose karts and drivers as the first step towards a daylight picture "digne de GTA V"; up close the karts read as toys: plain disc rims, one-colour bodywork, a helmet that was a ball with a band.

- **Decision:** the wheels get a cast rim (a lip, six spokes over the rim's dark well, a hub with three nuts, a valve) and a tyre whose tread is scrubbed grey between black walls; the livery breaks the bodywork's colour (a slash across each pod's flank, a chevron over the nose) on pieces meshed in 8 mm cells so the colours meet cleanly, with more stickers (pod tops, the nose's flanks); the body's and the tyres' vertex colours are soiled once built — tinted towards road dust lower down, mottled all over; the helmet gets a full-face chin bar in its lower colour, a chin vent and two brow slots.
- **Why:** a used racing machine is never one clean colour; detail where the chasing camera looks (wheels, flanks, helmet) is what reads as real.
- **Consequences:** building the moulded bodywork once takes longer (the model's tests 15 → 22 s in a debug build); the kart is unchanged to the physics and the network.

## D-099 — The race suit cut as one: stripes down the limbs, not rings

Date: 2026-11-14 · after D-098, up close: the driver's suit read as a toy's, its upper arms and knees coloured all round and the chest's band circling the arms.

- **Decision:** the team's colour runs in a stripe down the outside of each sleeve (shoulder to wrist) and each leg (hip to ankle), with the panels down the trunk's sides; the second colour's band stays across the chest, no wider than the trunk.
- **Why:** race suits are cut in long panels; coloured rings round the limbs are a cartoon's.

## D-100 — Moulded bodywork with crisp edges; a wrapping visor

Date: 2026-11-14 · after D-099: the nose and the side pods looked inflated, like cushions, and the visor a flat band.

- **Decision:** the nose's and the pods' pieces blend over 2 cm (from 5 and 4) with 2 cm corners (from 4 and 4.5): the crisp edges of moulded plastic. The visor is taller, wraps 140° of the shell (from 120°), its edges bevelled, smoked rather than black.
- **Why:** rotomoulded kart bodywork has defined creases and flat faces; wide blends read as inflatable.

## D-101 — Mown grass by the circuit, in stripes that change with the eye

Date: 2026-11-14 · daylight photorealism (the maintainer: "la tonte ne fait sens que proche du circuit, à l'extérieur personne ne tond").

- **Decision:** the ground material gains mown stripes (`Ground::mown`: strength, width, the mower's direction), applied where the vertex colour's alpha says the grass is mown; each stripe is lighter or darker by whether its blades lean away from or towards the eye. The kart world mows the land out to 6 m past the barriers (fading over 3 m), not on shores or cliffs; beyond, meadow.
- **Why:** the light and dark bands of a mown lawn, which swap as one walks round, are among the first cues that a venue is real and kept.

## D-102 — Scrub on the hills between the circuit and the woods

Date: 2026-11-14 · daylight photorealism: between the track and the woods the hills were bare, a few trees on smooth grass.

- **Decision:** at T3 and above, the roadside places up to 90 thickets (fewer in sparse scenery) 60 to 160 m from the track, each three to eight bushes within 3 m, where a slow noise (34 m) says the ground was left to grow; none elsewhere.
- **Why:** a hillside is never evenly sown: scrub gathers in thickets, and their patches give distant slopes depth and scale.
- **Consequences:** a few hundred more bushes, drawn up to 260 m with coarser meshes far off; to be measured on the cloud bench.

## D-103 — A crowd that follows the race, each spectator their own way

Date: 2026-11-14 · the maintainer: "il sera aussi important que les NPC (spectateurs) soient animés et réagissent aux événements de la route"; then, seeing it, "ça fait plus troupeau de moutons qu'humains", "les humains s'assoient de façon trop rapide", "tous ne doivent pas lever le même bras et de la même façon".

- **Decision:** every spectator in the grandstand and along the fences is a figure of its own, built calm and cheering, and moved as a whole by the client (`crowd`), from the karts' poses: excitement from the karts near (the faster, the more; a spin or a jump most); eyes on the nearest kart within 45 m (a quarter of them the one behind it), as far as their own reach; past their own threshold they cheer — standing up over half a second (the seated figure lifting and leaning, then the standing one straightening) and sitting back over one, or cheering from the seat, or not at all; cheering standing they wave, raise both arms, pump a fist, point or clap, half of them with the other arm. Each has a temper of their own — reaction time 0.2 to 1.5 s, threshold, how long they keep cheering, turning speed, bounce and sway — drawn anew for each race from the clock, the same on the bench. The crowd crate gains poses: standing with both arms up, a fist, pointing, clapping; seated clapping and pointing.
- **Why:** a crowd that does nothing reads as scenery; one that moves in step reads as a flock. People differ in when, how and how long they react.
- **Rejected:** animating in a vertex shader (a full vertex shader for the main pass and the prepass, with Bevy's vertex compression: fragile); arms swinging between poses (no skeleton on the figures).
- **Consequences:** a few hundred more entities and two meshes per spectator; to be measured on the cloud bench.

## D-104 — Birds and animals in the country round the circuit

Date: 2026-11-14 · the maintainer: "ce serait bien aussi qu'il y ait des oiseaux et quelques animaux dans la nature"; then "les ailes carrées c'est moyen".

- **Decision:** the client (`wildlife`) sculpts and moves, from the circuit's seeded dice and the race's time: flocks of six to fourteen small birds wheeling 20 to 45 m over the hills, each on its own loop, wings beating; two birds of prey gliding in wide circles 60 to 85 m up, beating now and then; flocks feeding on the ground 14 to 45 m from the track, pecking and hopping, put up by a kart within 25 m, wheeling and settling again some seconds later; ten rabbits (hind feet flat under the haunches, thin forelegs) nibbling and hopping in the meadows, bolting in long bounds from a kart within 20 m; four deer grazing 70 to 140 m off, raising and turning their heads to a kart within 60 m, walking on now and then. Wings have a bird's outline (rounded leading edge, pointed tip), folded back along the body on the ground.
- **Why:** an empty countryside reads as a set; animals that mind the race make it a place.
- **Consequences:** a hundred or so small entities; drawn at every tier; no sound yet.

## D-105 — The project's site, osage-engine.org, on GitHub Pages

Date: 2026-11-14 · the maintainer: "peux-tu créer le site web qui sera sur GitHub Pages accessible à osage-engine.org", "qualité pro".

- **Decision:** the project's public site is a static page in `site/` (HTML and CSS, no build step, no script; the game's own screenshots as WebP, about 0.5 MB), published by the `site` workflow on GitHub Pages whenever `site/` changes on `main`, under the domain osage-engine.org (the maintainer's, at Gandi; its DNS points at GitHub Pages). It presents the engine and its first world as they are — every picture rendered by the bench, every figure measured and dated in this log — links the releases, the source, the decisions and the knowledge, and carries the README's note about the name. Licence: CC BY-SA 4.0, as the documentation.
- **Why:** a project that asks people to play, read and contribute needs one address that says what it is, with the proof beside each claim.
- **Consequences:** a change that makes the site wrong (a figure, a feature, a tier's budget) updates it in the same change, as the knowledge cards are.

## D-106 — Grime at the foot of the painted walls

Date: 2026-11-14 · daylight photorealism: the red and white walls read as new, one clean colour from the ground up.

- **Decision:** the painted face of the walls is drawn in two bands; the lower 30 cm fades from a browner, darker tone at the ground (the track's spray and dust) to the paint, more on some stretches than others (a 9 m noise) and most on the outside of the bends, where karts brush the wall.
- **Why:** a circuit's walls are washed by every rain and sprayed by every kart: their feet are never the paint's colour.

## D-107 — A nearer haze: the landscape recedes into the air

Date: 2026-11-14 · daylight photorealism: the hills and the woods 300 m off were as crisp and as green as the verge.

- **Decision:** the photographed sky's haze has a visibility of 1.6 km (from 3 km): the land a few hundred metres off takes a little of the air's colour.
- **Why:** aerial perspective is how the eye reads distance; a circuit's landscape is compressed into a few hundred metres, so its haze must be nearer than the real air's for the hills to read as far.

## D-108 — Clouds' shadows drifting over the land

Date: 2026-11-14 · the kart programme's M6 (V2.3, "clouds that move and shade the ground"); the clouds' shade was painted, still, into the meadows' colours.

- **Decision:** the ground material darkens the ground under the shadows of fair-weather clouds — soft-edged patches of about 180 m, covering about a third of the land — carried by the wind (4.8 m/s), which the world advances every frame. 30 % darker by day; none at night or in the rain. The still shade painted into the meadows goes.
- **Why:** moving cloud shadows are among the strongest cues that a landscape is alive and outdoors.
- **Limits:** only the ground takes them, not the karts, walls or woods; the shadows do not follow the photographed sky's own clouds.

## D-109 — Marshals wave their flags at the karts

Date: 2026-11-14 · the kart programme's M5 (V1.3, "marshals waving flags").

- **Decision:** each marshal and their flag stand apart from the static scenery; the flag, in the hand, is waved side to side when a kart comes within 45 m — harder at a kart spinning round — easing off as the karts go, each marshal at their own rhythm.
- **Why:** marshals at their posts are the circuit's people closest to the race; still, they read as statues.

## D-110 — A flag of cloth; hair painted on the skull

Date: 2026-11-14 · the maintainer, after D-109: the flag was not along its staff and moved as one solid ("il doit osciller"); the marshal's hair looked odd.

- **Decision:** a marshal's flag is a staff pivoting in the hand and, hung along the staff's top, a cloth cut in 10 × 6 cells whose points follow a wave travelling out from the staff — a stir at rest, a flutter when waved, the free end sagging a little — seen from both sides. The crowd's figures have short hair painted on the top and back of the skull rather than a blended cap of matter, which bulged like a woollen hat.

## D-111 — Drivers flinch when knocked

Date: 2026-11-14 · the kart programme's M5 (V1.2, "hit reaction").

- **Decision:** a knock — the kart's spin changing by more than 2.5 rad/s, or its speed dropping by more than 5 m/s, in one frame (an item, a kart, a wall) — throws the driver's trunk aside and forward on its spring, and both hands come off the wheel halfway for about half a second before going back; no second flinch while one is under way.
- **Why:** a driver who does not react to being hit reads as a mannequin in a seat.

## D-112 — The site redesigned as an engine's home page

Date: 2026-11-14 · the maintainer: "redesigner le site pour un niveau vraiment professionnel en t'inspirant du site de Unreal Engine".

- **Decision:** osage-engine.org takes the structure of an engine vendor's home page, in osage's own identity (its mark, its amber, its pictures, its words): a split hero with a framed stage that cross-fades three bench pictures; what's new (releases and milestones, each linking to its release or card); the kart world as image tiles linking to the knowledge cards; light, with the same bench frame at T3 and T4; the four tiers as cards with their budgets and measures; how to play in three steps; the engine's pillars; the documentation and its counts; the community; the note about the name; a footer of links. Still static HTML and CSS with no script (D-105): the slideshow, the menu and the reveals are CSS.
- **Why:** the first page showed the project honestly but read as a project page; people decide in seconds whether an engine is serious.
- **Consequences:** "What's new" and the counts are written by hand: a release worth announcing updates them in the same change.

## D-113 — The `raytracing` build and Intel's integrated GPUs under Mesa: found, fenced, not fixed

Date: 2026-11-14 · T3 lost the GPU (`DeviceLost`) on the reference laptop (Intel Iris Xe RPL-U, Mesa 26.2.2, ANV) on and off all day; other programs on the GPU were suspected and cleared (the maintainer closed everything: still lost).

- **Found:** the `raytracing` feature brings in `bevy_solari`, which turns on `bevy_pbr/dfg_lut`: every PBR shader then reads a DFG lookup texture, at every tier, whether or not light is traced. On this laptop, the same commit at T3: lost 5 times of 5 with the feature, 2 of 2 with `bevy/dfg_lut` alone, 0 of 3 without either. With the feature it passes without the sky light (`--without sky-light`), and at T2 (a constant ambient light). The cloud's NVIDIA L4 runs T3 and T4 with the feature. A bisection that morning had found no guilty commit because its builds were made without the feature.
- **Decision:** nothing in osage's rendering changes for it. The Windows build keeps the feature (T4 is opt-in, D-093): a machine it fails on is caught by D-090 — the sky light is the first technique left out of a failing T3 — and integrated GPUs start at T2 (D-075). On the reference laptop, T3 is benched from a build without the feature. One thing found on the way is kept: the clouds' shadows (D-108) no longer touch every ground material every frame — the wind is applied in the shader from the engine's clock, and a material is touched only when the weather changes; a marshal's flag is cut again only within 120 m of a camera.
- **Open:** whether it is Mesa's or Bevy's to fix is not known; reporting it upstream needs the maintainer's word.

## D-114 — The site's hero plays a film of the game

Date: 2026-11-14 · the maintainer, of D-112's cross-fading pictures: "tu as mis une vidéo ?"

- **Decision:** the site's stage plays ten seconds of a race — the start and the first bends — muted, looping, with a poster while it loads (2.4 MB, H.264, 1280×720, 30 frames a second). It is filmed by the bench itself: `--film FROM,TO[,EVERY]` saves a picture of every frame of a stretch, and since the bench steps time by a fixed amount the frames are those of a smooth film whatever the machine's speed; `ffmpeg` joins them. Where the reader asks for less motion, the poster stands in. Still no script.
- **Why:** a game engine's first proof is the game moving.
- **Consequences:** the film is a file in `site/video/`, made again by hand when the game's look changes much; its caption names the tier it was filmed at.

## D-115 — The bench's frames are not the same from one tier's run to another's

Date: 2026-11-14 · seen while comparing pictures for the site (D-112 said "the same bench frame at T3 and T4").

- **Found:** on the same machine and build, frame 1500 of the T3 run and of the T4 run show the followed kart a few metres apart; a film of frames 240 to 840 made on the laptop at T3 and on the cloud's L4 at T4 shows a different kart leading at frame 400. The bench's race is therefore not identical from run to run, as [the render bench](../knowledge/oracles/render-bench.md) says it is ("the same frames on every machine").
- **Decision:** the site says "the same bend of the same bench", not "the same frame". The cause is not yet known (when the race starts relative to the scenery's loading, or arithmetic that differs between machines): until it is found, timings compare runs of the same stretch of track, not of identical frames.
- **Open:** find what differs and make the race identical again, with a test that hashes the karts' places at fixed frames.

## D-116 — The bench's race is the same at every tier again, and says so

Date: 2026-11-14 · closes D-115's open point.

- **Found:** the bench stepped and knocked its karts in the order the engine iterated their entities. That order is the engine's own: at T4, where more is attached to each kart, it differed, and the race with it. At T2 and T3 it happened to be the grid's.
- **Decision:** the bench orders its karts by their place on the grid before stepping them, and its report carries `race`: a hash of every kart's place and heading at the last step. Verified the same — `2a62476cae77818b` after 2400 frames — on the laptop at T2 and on the cloud's L4 at T3 and at T4.
- **Consequences:** timings and pictures from before this entry compare T3 and T4 on the same stretch of track, not on identical frames; from here, a comparison quotes the `race` of its runs. The site's T3 and T4 pictures are taken again from runs with the same `race`.

## D-117 — A finer driver was tried and not kept

Date: 2026-11-14 · a close picture of a driver shows the suit's stripes with toothed edges: the sculpted body's colours meet along the cells of its mesh (12 mm).

- **Tried:** cells of 8 mm (17 500 → about 38 000 vertices a driver). The bench at T2 on the Iris Xe went from 20.9 to 23.3 ms a frame (same `race`): eight skinned drivers.
- **Decision:** not kept. From the chase camera, four metres back, the teeth are under two pixels; only the photo mode and the replays' close cameras see them.
- **Next:** colours that meet along the painted shape's own edge rather than the cells' (vertices moved onto the boundary when the surface is meshed), which costs nothing at run time; or a finer body for the tiers that can afford it.

## D-118 — Colours meet along the painted shapes' edges, not the cells'

Date: 2026-11-14 · D-117's "next": the sculpted surfaces' colours met in teeth one cell long (the driver's suit, the bodywork's livery).

- **Decision:** when a sculpture is meshed, a vertex on an edge that joins two groups is moved to where the group changes along that edge (found by bisection; the mean of such places when several edges cross; then back onto the surface). Every sculpted surface gains it: the drivers, the bodywork, the crowd's figures, the animals.
- **Measured:** the driver's stripes, chest band and back patch are lines at the same 12 mm cells and the same 17 500 vertices; the bodywork's cells go back from 8 mm to 12 and 10 mm (D-098 had made them finer for this). The bench at T2 on the Iris Xe: 20.9 ms before, 20.9 ms after, the same `race`.
- **Why:** it costs nothing when the game runs, where finer cells cost 2.3 ms (D-117).

## D-122 — Content under any open licence, its terms honoured

Date: 2026-11-14 · the maintainer, when asked which human model to take: "pour moi c'est pas un souci d'utiliser dans le jeu tout type de modèle/texture/son open-source indépendamment de sa licence".

- **Decision:** the game's content may come from any source under an open licence — not only CC0: attribution (CC BY), share-alike (CC BY-SA), free-software licences, and publishers' free-use licences that allow shipping in a game without allowing the files to be offered apart from it. Each file still enters through the asset manifest with its licence, author and source, and what its licence asks is done: authors credited with the game, a share-alike file kept under its licence, a file that may not be redistributed alone shipped only inside the game's content.
- **Why:** CC0 alone left the game without what open libraries do well — people, animals, animation; the sculpted figures showed it.
- **Not changed:** nothing from commercial games or proprietary SDKs (clean sources); licences that forbid commercial use or changes are not open and stay out; the engine's and the game's own code licences.
- **Consequences:** releases carry a credits file made from the manifest once the first such file enters; the manifest gains a field for what a licence asks.

## D-119 — The rays skip what is not worth them: small plants, and all but one level of detail

Date: 2026-11-14 · T4 at night in the rain had gone over its budget (35.3 ms for 33.3) with the day's additions.

- **Found:** every drawn mesh with the standard material was given a double for the rays (D-093) — among them each of the three levels of detail of every tree, bush and rock, all in the traced scene at once and in the same place (a level out of its range is not hidden, only not drawn), and every tuft of the grass carpet round the camera.
- **Decision:** a world marks what traced light leaves out (`osage_render::NotTraced`); the placed models say which level of detail each mesh is (`osage_props::Level`). The kart world leaves out what is seen no farther than 100 m (grass, flowers, ferns, stones) and, of a model with levels, all but the middle one.
- **Measured** on the cloud's L4 at 2560×1440, night and rain, the same `race`: 35.27 → 31.40 ms a frame; the 95th percentile 57.7 → 47.9 ms.
- **Consequences:** what is left out is drawn and lit as before but casts no traced shadow and bounces no light; a near tree's traced shadow is its thinned level's.

## D-121 — Under traced light, the raster's own light and shade are not drawn; the bench says where the time goes

Date: 2026-11-14 · the maintainer, of T4 over its budget at night in the rain: "il faut réfléchir aux vraies causes, GTA 5 le fait très bien lui".

- **Found:** at T4 the raster's light was still drawn under the traced light, which reads none of it: four cascades of sun shadow maps at 4096² (twice T3's), contact shadows, and screen-space ambient occlusion at its highest quality.
- **Decision:** where light is traced (`osage_render::TracedLight`, set when Solari's plugins load on hardware with ray queries), the sun casts no shadow maps, and the camera has neither contact shadows nor screen-space ambient occlusion. On hardware without ray queries, T4 keeps them all. The bench records the GPU time of each pass of the renderer (Bevy's render diagnostics) and writes it to its report (`passes_ms`).
- **Measured** on the cloud's L4 at 2560×1440, night and rain, the same `race`: 31.40 → 27.36 ms a frame (35.27 before D-119); the 95th percentile 47.9 → 44.3 ms. Where T4's time goes now: Solari's lighting 17.4 ms, the deferred prepass 3.4 ms, Solari's world cache 0.8 ms, the rest under 0.5 ms each. T3 on the same run: 16.6 ms, its opaque pass 3.0 ms, its prepass 1.6 ms, ambient occlusion 0.9 ms.
- **Next:** the traced lighting is two thirds of T4 and is done at every pixel of 1440p; games trace at a lower resolution and bring the picture up. And the 95th percentile stays far above the mean: one stretch of the lap (frames 900 to 925) costs half as much again.

## D-123 — T4's slow stretch is the processor's, not the GPU's

Date: 2026-11-14 · after D-121: T4's 95th percentile (44 ms by night, 38 by day) far above its mean (27, 24), the slowest frames always between 890 and 930.

- **Measured:** the bench's slowest frames now carry the GPU's time in their passes (the innermost spans, Solari's included), the main world's processor time (from `First` to `Last`) and the render world's (extraction to rendering). On the cloud's L4 at T4 by day (24.3 ms mean, the same `race`): frame 909, 50.0 ms — GPU 25.5, main world 33.6, render world 5.0; frames 890 and 928 alike. The time is the main world's.
- **On the laptop** (T4 without ray queries, so with shadow maps), a Chrome trace of 960 frames (Bevy's `trace_chrome`, built apart, not committed) puts the main world's spikes in Bevy's `check_dir_light_mesh_visibility` (130–150 ms against 6 ms on most frames), on the same frames as the bench's slowest; the system walks every mesh entity against each shadow cascade.
- **Next:** take the same trace on the cloud (where traced light leaves no shadow maps, D-121, so the cause there is another), and cut the number of entities the engine walks every frame — each placed model is three entities, one per level of detail (D-119), and the crowd, the wildlife and the marshals are each their own since D-103.

## D-124 — One entity per placed model, its level swapped by distance: tried, slower, not kept

Date: 2026-11-15 · D-123's next step: the bench now counts what the engine walks every frame — at T3 and T4, 65 400 entities, 56 100 meshes, 45 300 of them the levels of detail of placed models, three entities a mesh, one per level; at T2, 3 000.

- **Tried:** one entity per placed mesh, drawing the level its distance calls for (5 % hysteresis), the traced light given the middle level. Entities 65 400 → 50 200, placed levels 45 300 → 7 600.
- **Measured** on the laptop at T3, the same `race`, the same build otherwise: 119.9 ms a frame before, 149.7 after. A mesh swapped on an entity sends it back through the renderer's preparation (its specialisation and bins); the camera crossing the levels' boundaries swaps hundreds a frame. Entities culled by a range cost less than that.
- **Not kept.** Fewer entities must come from merging what never moves — the far levels of a patch of woods into one mesh (as the woods' cards are, D-054), the crowd into a few skinned batches — not from swapping meshes.

## D-125 — The farthest level merged by patch of land: tried, slower, not kept

Date: 2026-11-15 · after D-124.

- **Tried:** each scatter's farthest level of detail merged into one mesh per 32 m patch of land and mesh, ranged from the patch's centre. Entities 65 400 → 59 200 (most far levels are thinned foliage whose levels start within 70 m, few beyond).
- **Measured** on the laptop at T3, the same `race`: 119.9 → 131.6 ms a frame. A patch is drawn whole as soon as any of it is in view and in range: more triangles than the trees it replaced, on a GPU-bound machine.
- **Not kept.** What remains to try for T4's processor spikes (D-123) is to find, on the cloud's own trace, which system spends the 20 ms there: the laptop is bound by its GPU and by its shadow maps, and does not show the cloud's cause.

## D-126 — The cloud bench can trace every system and summarise the trace where it ran

Date: 2026-11-15 · D-123 and D-125: the laptop, bound by its GPU and its shadow maps, does not show the processor spikes the L4 shows at T4; a Chrome trace of 1 000 frames is 1.6 GB, too large to bring back.

- **Decision:** the cloud bench takes a `profile` input: the game is then built with Bevy's `trace_chrome`, and on the bench machine `tools/cloud-bench/trace-summary.py` reduces the trace to every system's mean, 95th percentile and worst time over the race, and the systems that took longest in its slowest main-world frames; the summary comes back as `out/<tier>-trace.txt`, the trace is deleted there. Its timings are not a bench's (tracing costs time); only the summary is read.
- **First reading** (the L4, T4 by day, 1 100 frames, traced): the main world's `PostUpdate` takes 15.3 ms a frame on average and 22.4 at the 95th percentile, nearly all in Bevy's bookkeeping of every mesh entity — re-specialising the meshes that come into view or whose material or mesh changed (two systems, up to 9 ms each at the 95th percentile, 15 ms at worst), visibility ranges (7.5 ms at the 95th, 16 at worst), bounds (14 ms at worst). Its cost follows the entities: about 56 000 meshes at T3 and T4 against 2 300 at T2, and its spikes come where a bend brings thousands into view at once. osage's own systems cost under 0.5 ms each. What would remove it is instancing the vegetation — one entity per set of plants, its places in a buffer the GPU reads — rather than one entity per plant and level; D-124 and D-125 showed that swapping meshes or merging patches does not.

## D-127 — The kart for macOS: a folder like Windows', for Apple silicon

Date: 2026-11-14 · proposed by a contributor for the maintainer's decision: a macOS release of the kart beside the Windows one.

- **Decision:** each `kart-v…` release also carries `osage-kart-macos-arm64.zip`, built by the `release-macos` workflow on GitHub's Apple-silicon runners: the same programs as Windows' (`kart-native`, `kart-server`, `kart-web`, `kart-update`), the browser client, and launchers `play.command`, `play-two.command`, `host.command` and `join.command`, which Finder opens in Terminal. The folder updates itself as Windows' does (D-047): `kart-update` takes the zip of its own system and, on a Mac, gives its place to the launcher again with `exec`. The content is the release's one zip, which the folder downloads at its first start: the player unzips one file, not two (Archive Utility would unzip the second beside the first, as "osage-kart 2").
- **Unsigned, as on Windows:** the linker signs the programs ad hoc, as Apple silicon requires, with no developer identity and no notarisation. macOS stops the first launcher the player opens; once they allow it (System Settings, Privacy & Security, "Open Anyway"), the launcher lifts the quarantine mark from the folder's files, which macOS would otherwise stop one by one. Updates come through `curl`, which sets no such mark. Trust rests, as on Windows, on GitHub's HTTPS and published digests.
- **Kept out for now:** Intel Macs (sold until 2020; macOS 26 is their last); the traced light at T4 (wgpu offers ray queries on Metal, but no Mac has run Bevy Solari yet, and the `raytracing` feature changes every tier's shaders, D-113); an application bundle (`.app`), which opens without warnings only with a developer identity, and which macOS runs from a read-only copy when it is opened where it was unzipped, so that it could not update itself.
- **Why:** players on a Mac could only build the game from source. Apple silicon needs nothing beside the game (wgpu translates the shaders to Metal itself, where Windows needs Microsoft's compiler) and no change to it.
- **Later, by the maintainer's choice:** a Developer ID and notarisation (an Apple developer account, 99 USD a year, its keys as repository secrets) would spare players the System Settings step; an Intel build would be one more target in the same job.

## D-128 — Bevy's `AssetChanged` patched: tried, no faster, not kept; the bench machine's cores in question

Date: 2026-11-15 · D-126's next step, the maintainer's go-ahead to change Bevy where it costs ("quitte à forker Bevy si besoin").

- **Tried:** `bevy_asset` 0.20.1 carried in `third_party/` with a patch: when one mesh or material changes in a frame (a tyre mark, a flag, the lake's ripples — every frame in a race), Bevy's `AssetChanged` filter looks every entity up in the map of every asset ever changed; patched, it compared each entity's asset with the few changed since the query last ran. Bevy's own tests passed.
- **Measured** on the L4, T4, the same `race`: the systems it serves were no faster (specialization 2.8 → 3.1 ms and 2.3 → 2.4 ms, bounds 2.4 → 2.7 ms, mean); frames 24.1 → 23.5 ms, within one run's noise of the next. **Not kept**: reverted the same night.
- **What it shows:** the lookups are not where those systems' time goes. Their means are small and their 95th percentiles large (8–10 ms), all on the same frames, on a machine of 4 vCPUs (2 cores) running the main world, the render world and their parallel systems at once: the time may be waiting for a core, not work per entity. The cloud bench now takes the machine as an input (`machine`: `g2-standard-4`, the reference, or `g2-standard-8`) to tell the two apart; the reference stays the 4-vCPU machine until the maintainer says otherwise.
- **Measured on 8 vCPUs** (g2-standard-8, the same L4, T4, traced, the same `race`): the systems that spiked on 4 halve and their spikes go — specialization p95 9–10 → 1.6 ms, bounds 7 → 1.5 ms, culling 4.7 → 2.8 ms; the main world's own work falls from about 20 to 13 ms a frame, and its wait for the GPU rises from 7 to 13 ms: the frame (23.2 ms) is the GPU's, the traced light's above all. The processor spikes of D-123 and D-126 were mostly the bench machine's two cores, which a player's computer with an RTX card (8 cores or more) does not share. One system still spikes on its own, Bevy's visibility ranges (p95 13 ms, 45 000 levels of placed models each with a range): the next to look at, in Bevy or in how osage gives its levels their ranges.

## D-120 — Sculpted figures get a head of their own, with a face

Date: 2026-11-15 · the maintainer: the marshal needed a human head, more detail in the torso and the head. (Number kept for this entry when it was planned.)

- **Decision:** the crowd's figures keep one sculpted body per pose, but the head is sculpted apart and finer — 1.4 cm, 0.6 cm for people seen from close (marshals, mechanics) — and set on the neck: a skull, a face narrowing to the chin, a brow, a nose, cheekbones, ears, hair as matter over the top and back, eyes, eyebrows and lips painted on.
- **Why:** at the body's 3.5 cm a head is a ball; a face needs features a centimetre or two across. Sculpting only the head finer costs little, the body staying shared.
- **Superseded where people are installed:** see D-129.

## D-129 — People from open rigged models: Quaternius's bodies, hair and moves

Date: 2026-11-15 · the maintainer: "pourquoi tu prends pas un model d'humain open source", and any open content (D-122).

- **Decision:** a new crate, `osage-humans`, makes people from Quaternius's Universal Base Characters and Universal Animation Library (CC0): two bodies, two skin tones, five hairstyles, 43 moves, one skeleton. The two zips are downloaded by a person (itch.io keeps automated clients out); `tools/quaternius.py` takes out what is used; the manifest lists every file. Without them, worlds keep their sculpted figures.
- **Dressing by joints:** the bodies are bare. The joint that weighs most on a vertex says what it wears — head, neck, hands skin; torso and arms a top; hips and legs the bottom; feet shoes — and the body's texture is painted once per body and tone (fabric grey, flat, rough where worn), each outfit's colours set on the mesh's vertices. Worn parts are smoothed (Taubin's steps), their normals made again and pushed out 1.4–1.8 cm: cloth does not follow every muscle.
- **Moves:** every bone is a target by its path of names, the library's paths, so its clips play on the people unchanged; a changed move crossfades.
- **First use:** the kart's marshals, in orange, holding the flag up (`Idle_Torch_Loop`), the flag following the raised hand. Next, mechanics, then the crowd (each person has 65 bones: tens near the camera, not thousands).

## D-130 — Headlamps aimed level; a replay shows the karts that line up next, and its cameras keep out of the land

Date: 2026-11-15 · the maintainer, after playing: at night the headlamp lit "a small very bright spot just in front of the kart"; in a replay the karts were not seen, and the camera was at times under the land or above a tunnel.

- **Headlamps:** 120 000 lm aimed 6° down from 45 cm put 5 700 lux on the road 2 m ahead and 49 at 10 m. Now 12 000 lm, aimed level in a narrow beam (inner 2°, outer 18°) that fades below it: about 640 lux at 2 m, 17 at 10 m — the pool spread down the road.
- **Replay's karts:** when the next race lines up, the world replaces every kart (new entities); a replay started on the grid moved karts that were gone. The tape now keeps each kart's colour, and a kart gone is shown by the live one of its colour.
- **Replay's cameras:** a camera by the track no longer stands within 30 m of a tunnel's mouth (it stood in the hill), and every camera of the show — trackside, helicopter, the flyover — is raised out of the land if its shot put it there, except inside a tunnel, where the land is the hill above.

## D-131 — Choosing by sight: a selection screen between races, a splash at start

Date: 2026-11-15 · the maintainer, after playing: "les choix dans les sélections [doivent être] visuels (images de driver, de kart…); pour le choix des events, des cartes montrant les terrains, qu'on puisse choisir directement celui qu'on veut; un spinner pendant leur chargement"; "un splash screen osage puis osage-kart pendant le chargement initial (écran noir à présent)"; and the kart heard during loading with sound off.

- **Selection screen** (`garage`): events as cards showing their circuits from above, a cup's tracks side by side, under tabs by kind; drivers as portraits — each driver's kart parked far from the circuit and filmed for a few seconds by a camera of its own into a picture, on a render layer of its own, lit by two studio lights at a fixed exposure, so that a portrait looks the same by day, by night and in the rain; karts as cards with bars for speed, acceleration, handling and grip. A click picks (the host's event is raced, the others' are votes, as with T); the keys still work, and a pick made with them shows its section. C hides the screen; while it shows, the records, the map and the speed step aside.
- **Loading:** a spinner while the circuit or its models load; at start a splash — osage's mark (the site's) and name, then osage kart and "building the circuit" — from the first frame until the circuit is there, then it fades.
- **Sound off** is off from a sound's first sample: game sounds are started muted, not muted once their sink exists — a loading frame lasts seconds, and the kart was heard through it.
- **Corrected the same night:** the studio's first lights were lights of the whole sky on the studio's render layer — but render layers keep a light out of other meshes' shadows, not off them: they lit the circuit with a flat grey light. The studio now has short-range lamps beside each kart, 20 km from any circuit. The splash's fade faded every picture of the interface, not only its own (portraits vanished); and since the interface is drawn only once its shaders are made (two or three seconds in on an integrated GPU), the splash is held at least 7 s from the first frame, osage's screen 4.5 s of it.
- **Mechanics** (D-129): with people installed, the two mechanics in the pits are people in their team's colour, one kneeling at the kart, the other standing talking.
- **Not yet:** pictures of the kart bodies themselves (they share one model; their character is in the bars), a portrait of the driver's face (the karts' drivers are helmeted).

## D-132 — Holograms in a world: a second row of apartment blocks behind Harbour Town

Date: 2026-11-17 · the maintainer's holographic textures (issue #7, research/09), which they asked to work on today.

- **Decision:** where the apartment block's views are installed (`holograms/facade-256`, baked by `hologram-bake` from Poly Haven's urban apartments facade, CC0), a town circuit gets, beyond its sculpted houses, a second row of apartment blocks 40–55 m from the road, facing it, drawn as holograms lit by the world's lights (`osage-hologram`); behind each facade a solid block in its brick, so that the kit's panels read as one building. No model is needed: a kart's camera never comes within 40 m of them.
- **Measured** at T2 on the reference laptop, Harbour Town, the same `race`: 19.60 → 19.75 ms a frame (p95 25.05 → 25.10); loading 0.8 s longer (the views stacked).
- **Limits:** one block, repeated; the light is the world's but the normals are the geometry's alone; no shadow cast; 105 MB of views at 256 px, shared by every block.

## D-133 — In 3D only what physics needs; everything else a hologram

Date: 2026-11-17 · the maintainer: "l'idée est de n'avoir en 3D que ce qui est véritablement nécessaire pour le physique, tout le reste en hologrammes"; and the targets: buildings ("ils sont une cible aussi"), a forest computed with millions or billions of faces, the sea with ultra-realistic waves, boats, dolphins — each "produced once and for all", its angles limited to those the karts' and the game's other cameras can see.

- **Direction:** worlds keep as geometry only what the simulation touches — the road, walls and barriers, the karts, what they can hit. Scenery is baked offline at any detail from open models into holograms (`osage-hologram`, research/09), photographed only from the directions the game's cameras can see it from (a narrow band of angles from a circuit: fine angular steps, which compress best along the angle), and drawn lit by the world.
- **First steps:** apartment and factory blocks behind Harbour Town (D-132); next, a forest of hundreds of millions of faces as holograms; then the sea, which moves — views along time as well as angle.
- **Open:** what a scenery that moves (waves, boats, animals) costs as views along time; GPU memory for many holograms (a compact form, or views decoded near the camera); shadows from holograms.

## D-134 — A world of nested cubes, its holograms streamed by the server

Date: 2026-11-18 · the maintainer: "nous divisons le monde en blocks (cube dans cette version) de 81m de côté, ensuite pour le cube dans lequel se trouve l'utilisateur on divise en cubes de 27m, puis […] en cubes de 3m de côté. Finalement pour le cube de 3m de côté dans lequel il est, le modèle 3d est chargé. Pour les autres cubes le serveur fournit les hologrammes"; "il faut que le serveur ne stream que les angles nécessaires pour la vitesse de marche et concernant la position courante"; "quand l'utilisateur passe dans un cube de 3m adjacent à celui où il était, ce nouveau cube devient le centre"; the test world, "un exemple photoréaliste à 100 %", from "des sources très détaillées", without computing a Gaussian splatting of our own: "tu peux pas éviter de calculer du 3d splatting et trouver des données disponibles ?".

- **Cubes:** a world is cut into cubes of 81, 27, 9 and 3 m (the 9 m step agreed with the maintainer, so that each level has 26 neighbours). At each level the cube the viewer is in is the centre of a neighbourhood of 3 × 3 × 3; the finest centre is drawn in 3D, every other cube is a hologram; stepping into a neighbour makes it the centre. A coarser neighbourhood holds the finer one without lining up with it, so a hologram is drawn without what lies in the finer neighbourhood, told by its depth (`osage-cubes`).
- **Streaming:** the server keeps every view; from where the viewer is and how fast it goes, it answers the blocks of views it can see each hologram from within a horizon, and the client fetches those it lacks. A block is 3 × 4 views of the grid, one short AV1 video along the viewing angle per series — the measured gain of research/09 kept in transit — starting with a whole picture so that it decodes alone; depth lossy, its outline apart and exact; decoded in Rust (rav1d, BSD-2-Clause), its assembly built with nasm (`osage-hologram-stream`, `osage-hologram`'s `stream`). AV1 rather than H.265, which patents encumber.
- **Measured:** a forest plot's 102 MB of views become 20.8 MB of blocks; turned round it, a camera was sent 51 of 105 views (7.8 MB). One thread decodes a view of 1024 × 256 px in about 40 ms (colour, depth, outline).
- **Test world:** a street of Paris, Amazon Lumberyard's Bistro (NVIDIA ORCA, CC BY 4.0: 2.8 million triangles, the café inside), its views photographed offline by a path tracer (Blender's Cycles, a tool outside the repository) for their realism, its walls and ground kept for physics (D-133). A real street (IGN's BD TOPO, Panoramax's photographs, both under the Licence Ouverte) was looked at first and left: it would have needed a scene learnt from photographs.
- **Open:** the carving in the hologram's shader; how many views a world's plan holds, and its baking time; the light of the 3D cube matching that baked in the holograms; normals' cost; decoding on the GPU.

## D-135 — The 3 m cubes around the viewer in 3D, ray traced; holograms from the 9 m cubes on

Date: 2026-11-18 · the maintainer: "pour moi il faut faire du raytracing sur les 9 cubes internes et utiliser le flux hologramme sur les 6 faces des cubes de 9m entourant les 9 cubes de 3m".

- **Decision (changes D-134):** the 3 m cube the viewer is in and its 26 neighbours — a block of 9 m centred on the viewer, 9 cubes on the floor it walks — are drawn in 3D, ray traced where the hardware can (T4) and rasterised elsewhere. Holograms begin with the 9 m cubes around that block, drawn on their six faces, then 27 m and 81 m as before; no hologram is nearer than a 3 m cube, where holograms are weakest (research/09: close up they need far more resolution). The 9 m cubes keep their grid, carved where the 3D block reaches into them, rather than 9 m blocks re-centred every 3 m (27 times as many to bake).
- **So:** no 3 m hologram is ever baked, the most numerous; the 3D block and the holograms, both path traced, light alike.

## D-136 — What the client receives: the light coming into the block of 3 m cubes

Date: 2026-11-18 · the maintainer: "est-on d'accord que la seule chose à streamer par le serveur au client ce sont les hologrammes de lumière entrante des surfaces externes des 9 cubes de 3m ? pour les 9 cubes eux-mêmes on trace le modèle" — "oui je confirme"; and, to a proposal of three layers: "il suffit d'en tracer 9".

- **Decision (changes D-134 and D-135):** the client traces the 3D model of the 3 m cubes around the viewer — one layer of 3 × 3, 9 cubes, a block of 9 × 9 m and 3 m high centred on the viewer's cube. What rises above the block, or falls below it, comes in through its top and bottom faces like the rest. What lies outside the block reaches the client only as the light coming into it through its outer surface: a ray that leaves the block takes the light that comes in where, and in the direction, it crosses the surface. That light is both what is seen beyond the block and what lights the inside (the sky, the facades across the street, reflections in a shop window); lighting needs only a coarse version of it.
- **Form:** the outer surface is cut into tiles, the faces of the 3 m cubes; each tile carries panoramas taken from points on it looking out, with their depth — a distant building sharp without a great many directions, a near one put in place by its depth. When the block slides, the tiles it keeps stay, and new ones come ahead of the viewer as its speed asks; transport as in D-134 (AV1, the panoramas of neighbouring points being alike).
- **The 9, 27 and 81 m cubes** no longer go to the client: they remain the server's means of computing the incoming light, baked or live.
- **First experiment:** one block of Bistro — the light coming into it rendered by Cycles, the inside traced, composed and compared with a render of the whole scene.

## D-137 — The light coming into the block, taken inside the viewer's cube

Date: 2026-11-18 · the maintainer, shown the client against Cycles with both (research 10), chose "Oui, 5 sondes par cube".

- **Decision (changes D-136's form, not what is sent):** the light coming into the viewer's block is taken by five probes inside the viewer's 3 m cube, no longer by tiles on the block's outer surface: full spheres with their depth, at the eyes' height (1.65 m over the ground under each), at the cube's centre and 0.75 m from it towards each corner, the block's 9 cubes transparent to their camera's rays from 30 cm over the ground (the street, its pavements and kerbs stay; the rest still casts its shadows and its light). What is sent is still only the light coming into the block (D-136); it is taken from nearly where the eye is.
- **Why:** a probe on the block's surface stands a metre from what lies just outside the block, the eye 3 to 5 m back; the eye sees behind a bollard, a kerb or a door's jamb what no such probe sees (3.8 % of the measured picture), and those holes can only be guessed. From inside the cube, 0.04 %; 28.4 dB against Cycles from 26.2.
- **Cost:** a probe weighs about 860 KiB (4096 × 2048, AV1), 4.3 MB a cube, about 16 Mbit/s walking; a cube's five take 15 min of an L4 at 256 samples.
- **Transition:** where a cube's probes are not baked yet, the client falls back on the block's surface tiles (D-136).

## D-138 — Traced, the rays see the block alone, under a dome of the light coming in

Date: 2026-11-19 · the traced block (T4) lit by its faces as lamps came out at 0.71 of Cycles on its floor (research 10); "le client trace en ray tracing les cubes autour du promeneur ; le serveur ne lui envoie que la lumière qui entre par la surface de ce bloc".

- **Decision (D-135's tracing, D-137's light):** at T4 the rays see the walker's block alone: a mesh of the street that reaches out of it is cut at its surface for the rays and the eye (from inside the block the faces hide all beyond it anyway). What lies beyond comes in from a dome of 40 m radius round the block, facing in and shining what the cube's centre probe saw come in from each direction (a 1024 × 512 picture of it); a ray leaving the block meets the dome and takes the light that came in from its own direction. The faces are no longer lamps where the cube has probes; a cube without them keeps D-136's lamps of its tiles.
- **Why:** the probes were taken with the block transparent (D-137): what they saw is all that lies beyond it, lit as Cycles lit it. Traced whole, a building whose corner stood in the block hid that light from the floor and lit it again from Solari's light instead; lamps of a face's mean light lost the light's direction and had to be scaled by hand. Under the dome and cut to the block: the floor at 0.88 of Cycles from 0.71, 28.1 dB over the picture from 27.3 (T3 28.5).
- **Radius:** within the 50 m Solari's light cache traces its rays to: farther, a ray of the cache that met nothing would take the camera's environment map besides the dome, the light counted twice.
- **Left:** the traced floor still 10 % darker than T3's under the same light (0.895 in radiance); not the denoiser (unfiltered, 0.81 through AgX's curve), to be found in Solari's realtime estimator.

## D-139 — The light sent along the walker's way: the centres first, a cube whole when standing

Date: 2026-11-19 · the maintainer: "ça lagge un peu" on the laptop, then "réfléchissons au problème des saccades" and chose to work on the sending rule first.

- **Decision (changes D-137's sending, not its bake):** the server of light sends a walker, in this order: the centre probe of its own cube; the centre probes of the cubes it meets within the horizon going straight on at its velocity, a metre either side of its way, in the order it meets them; the centre probes of the four cubes beside its own, into which it could turn; the surface tiles of those cubes without probes (D-136); and, only when it stands (under 0.5 m/s), its own cube's four other probes. Walkers are told to the server with their velocity, not their speed alone.
- **Why:** sending every cube within reach of the walker in every direction, five probes each, comes, down the whole street baked, to 9.3 probes a second walking and 31 running: fifteen and more of a laptop's cores to decode (a probe takes about 0.57 s of four). Along the way and centres only: 1.4 probes a second walking, 3.7 running; standing, nine probes. A cube drawn from its centre's probe alone measured 27.2 dB against Cycles, from all five 28.4: walking shows the first, standing the second.
- **Measured** on the laptop's film (five cubes baked): nine probes decoded instead of twenty-five, the frames' median 17 ms instead of 25–44.

## D-140 — Lighter probes: packed at quality 22, and at half their size for a runner

Date: 2026-11-19 · the maintainer chose it, adding: "à part le centre de l'écran, le reste peut être un peu flou quand on court".

- **Decision (changes D-137's packing, not its bake):** probes and tiles are packed at SVT-AV1's constant quality 22 instead of 10; each probe is also packed at half its size (2048 × 1024: the radiance the mean of four pixels, the distance the nearest of them), as `h_X_Y_Z_K` (`light-pack --half`; `--again` packs again what is packed already). The server sends a running walker (over 2.5 m/s) the centres' probes at half their size; a client draws a cube with the best size of its probes it has, its arrays made again when the size changes.
- **Why:** from the scene camera, against Cycles: 28.3 dB at quality 10, 28.1 at 18, 28.0 at 26, 27.6 at 34, the colour 729 KiB a probe at 10, 366 at 18, 237 at 26, but its decoding only 10–25 % shorter (it goes with the pixels). At half the size, 26.9 dB (blurred) for 3.2 times faster a decoding (169 ms a probe). A runner's eyes are on the middle of the picture; the rest may blur.
- **Measured:** the street's light 34 MB → 26 MB with the halves; standing, 28.0 dB; running on the laptop, each cube's centre in time, the frames' median 21 ms.
- **Next:** the middle of the picture sharp when running too (view-dependent probes).

## D-141 — Probes as tiles, decoded where the walker looks

Date: 2026-11-19 · the maintainer chose it: the half in the background, then tiles at full size decoded from the middle of the screen outwards, all of them when standing.

- **Decision (changes D-140's packing and sending, not the bake):** each probe inside a cube is packed as 16 × 8 tiles of 256 pixels (`light-pack --tiles`: `tiles-colour.ivf`, `tiles-depth.ivf`, `tiles-mask.ivf`, `"tiles": [16, 8]` in its `meta.json`), every tile a key frame with its sequence header, so that any of them decodes alone; their radiance at quality 16; its half (`h_`, D-140) packed beside it. The server sends the centres' halves first, then the surface tiles of the cubes without probes, then the centres whole; standing, its own cube's other probes, the halves first. The client draws a probe's half at once, doubled into its layer of the cube's arrays, then decodes its tiles four at a time from where the walker looks outwards — running (over 2.5 m/s) those within 35° of the middle of the view, walking within 70°, standing all — then those of the centre of the cube ahead, and writes each tile over the half once the half is written.
- **Why:** a view reads 15–26 % of a probe's panorama, its middle third 5–11 %. The half decodes three to four times faster than a whole probe; the 128 tiles together cost about what the whole probe does (1.07 s of one thread), so a runner's middle, a dozen tiles, costs a tenth of it.
- **Measured:** standing, every tile decoded, 27.9 dB against Cycles (the whole probes 28.0; the halves alone, doubled, 26.6); at the whole picture's quality the tiles came back a quarter further off, at 16 as close for 6 % more bytes. Filmed on the laptop against D-140, alternately (load 3.6–11): no smoother — the frames' medians 17–26 ms both ways, as many over 50 ms — but running, the middle of the view at full size for 16 tiles a cube; 26 % more bytes down the five cubes (9.8 MB against 7.8), the halves and the tiles both sent. Its gain is sharpness at the same cost, not fluidity.
- **Set aside:** fetching only the tiles wanted (byte ranges by an index of the tiles' offsets: for the stream, later); a half array beside the whole one, the shader choosing (seams where a decoded tile meets one that is not); tiles at the whole picture's quality (blurrier).

## D-142 — The probes' layers made once, the light of the cubes ahead made ready

Date: 2026-11-22 · the maintainer asked whether the light of the adjacent blocks should not be loaded ahead, where the walker goes, more or less urgently by its speed; and chose the lot.

- **Decision (changes how the client keeps and fetches the probes' light, and the order of D-139's answer):**
  1. The probes' light goes into two texture arrays of six layers made once, not two of five made anew for each cube. A layer is lent to each probe of the cube the eyes are in, and to the centre of each of the first two cubes on the walker's way (standing, of those it faces, after a second): its half doubled, then its tiles where the walker looks, written before it gets there. A cube left keeps its layers, its light there at once if the walker turns back, until another probe needs them, the farthest behind first.
  2. `needed_light` answers `Needed { way, beside }`: what the cubes the walker goes through are drawn with, then their centres whole; then the same of the cubes beside its way, and, standing, the rest of its own cube. The server of light sends both lists. The client's light threads take what is on the way first, then the tiles to decode where the walker looks, then what is beside, and are told again four times a second: what the walker no longer needs is not fetched.
- **Why:** the films' logs read frame by frame. On 2026-11-21 (0.5 GB of the laptop's memory free), each time the walker entered a cube the picture stopped for 1 to 7 s, in D-140's films as in D-141's, right as the cube's two arrays were made (160 MiB each) and those of the cube left freed. And running, the tiles of the next cube's centre were decoded 1.8 s late, behind the surface tiles of the cubes without probes beside the way.
- **The films' measure, mended:** it had hidden this. The count of a film's frames ran unordered with the walker's systems and left out the frames in which it saw the walker standing (about one in three): films of 7 s walked 10, out of the baked cubes. A frame's time came from the walk's clock, capped at 250 ms. And without a window nothing paced the frames: the processor ran ahead of the GPU until the driver held it 0.47 s, about once a second while a cube's five probes were drawn; a frame now waits for the GPU to have drawn the one two before it, as showing it would. A film starts a second after the street is there.
- **Measured (2026-11-22, 16 GB free, the client before with the same measure, three films each, alternately):** walking, a cube's light showed 0.21–0.49 s after the eyes entered it, now at once (0 ms, twelve of twelve); running, 0.22–0.93 s and two cubes of four never shown, now at once but for the cube whose corner the way cuts (0.17–0.49 s: every layer is lent as the walker sets out). The longest frame walking 209–332 ms → 150–248; running 283–486 → 63–184; frames over 50 ms walking 55–57 → 8–27. Standing, 28.0 dB against Cycles, as before. The stops of seconds did not come back that day with the client before either: that the arrays made for each cube caused them is what their timing says, not a measure with and without.
- **Also learnt:** paced by the GPU, a frame standing among five probes takes 53 ms on the Iris Xe (the 12–25 ms told so far were the processor's alone).
- **Left:** a block lit by surface tiles still makes its arrays for each block (35 MiB); the store keeps every probe's half decoded (16 MiB each) behind the walker.

## D-143 — The Paris walk released as the kart is: one zip, a launcher, a folder that updates itself

Date: 2026-11-22 · the maintainer asked for a release of the Paris walk that runs from a download, as simple to install as osage kart.

- **Decision:** releases tagged `paris-v…` carry `osage-paris-windows-x64.zip` and `osage-paris-macos-arm64.zip`, built by the `release-paris` workflow: `paris`, `paris-update`, a launcher (`walk.bat`, `walk.command`) and a README; on Windows, Microsoft's shader compiler beside them as for the kart. The launcher walks alone (`--offline --ring --block-probes`, setting out where the street is seen down its length, tier T2 unless `--tier` says otherwise): the street's meshes for 13.5 m round the walker, lit by the nine centres of its block. The content is two more zips of the release (`tools/bistro/content-zip.sh`), which the folder downloads at its first start and whenever one changes, on Windows too: the walker unzips one file. `osage-paris-street.zip` is what comes from Amazon's scene and does not change (the exported street, its walk grid, the sky, Blender's AgX table, the manifest; about 1 GB): made once, the same file on every release. `osage-paris-light.zip` is the light baked into the blocks (the probes and tiles), which a new bake changes: a probe changed is not the street downloaded again.
- **How:** the kart updater's policy moves into `osage-update` (`folder::run`, given a `Folder`: name, repository, tag prefix, the programs' zip and the content's packages, each with its own line in `release.txt`); `kart-update` and `paris-update` only name theirs. Each world's releases have their own tag prefix, so a kart folder never takes a Paris release nor the reverse. The kart's two release workflows now run only for `kart-v…` tags.
- **Licence:** the street and all that is made from it (the export, the baked light) are CC BY 4.0, not CC0 as the kart's content: the street's zip carries `orca/bistro/ATTRIBUTION.txt` (author, source, licence, what was changed) and each README says the same.
- **Not in it:** the world's server and the server of light (the launcher walks alone); the traced block of T4 (the programs are built without `raytracing`).
- **Unknown at this decision:** `paris` has run on Linux only. Whether it builds and draws on Windows and on a Mac is learnt from the first release.
- **Learnt at the first build:** Windows built at once. The Mac did not: rav1d 1.1.0 as published leaves out a header its arm64 assembly includes. The assembly is now asked for on x86-64 only; a Mac decodes the light with rav1d's Rust code alone, slower. The kart's Mac build would have failed the same way since it took the streamed holograms.


## D-144 — An independent city experience with portable test inputs

Date: 2026-11-22

`ville-paris` owns the city-scale scenarios and starts with the two decks of Pont de Bir-Hakeim. Its native viewer, CPU replay and render bench consume one self-contained glTF scene and a separate fixed-tick input file. The files can be supplied to another implementation. Each measurement identifies both by SHA-256. The existing Bistro walk supplies the S0 lighting control. The first fixture has authored city geometry and fixed deck heights. Geographic acceptance requires a survey. [World module](../knowledge/modules/ville-paris.md) · [City programme](07-city-scale-quality.md)


## D-145 — Compare rigid solvers on portable city inputs

On 2026-11-22 the city pilot gained a shared CPU interface to Rapier and Jolt. One solver owns each replay. The default interactive choice is Rapier. The original point sweep supplies the flat-surface control. Both rigid backends use the same mesh triangles, skeleton, masses, collision clock and scene units. The viewer draws the poses each solver produces. Receipts include per-part rotations, contact points and the motion of the box struck by the actor. A claim about another engine needs comparable observations from it. [Dynamics module](../knowledge/modules/osage-dynamics.md) · [City experiment](../knowledge/modules/ville-paris.md)

## D-146 — Three geometry channels for a static object

On 2026-11-22 the city pilot adopted a versioned glTF object profile. A node may expose a visible mesh, separate physical surfaces and independently authored acoustic surfaces. A sound proxy can explicitly reuse the collision mesh. Each channel has stable identities. Unknown sound coefficients are preserved. The static-world converter records source digests and losses. A slab with a visible top, a lower collision plane and a separate sound surface checks the convention in the native reader. [Object format](08-object-format.md) · [Module](../knowledge/modules/osage-objects.md)

## D-147 — Compare impacts with a shared articulated object

On 2026-11-23 the city pilot added a parent-first articulated rig to its GLB object profile. Both physics backends receive the same masses, principal moments, joint anchors and angular limits. The comparison measures contact time from solver events and penetration and rebound from the sampled geometry of the moving bodies. This keeps contact margins and reporting latency visible in the results. [Object format](08-object-format.md) · [Impact oracle](../knowledge/oracles/pedestrian-impact.md)

## D-148 — Independent district pages for the city descent

On 2026-11-23 the city experiment gained independently readable visual and collision tiles. The coarse parent provides the distant picture and loaded collision files create real solver surfaces. The request clocks have separate bandwidth and radius settings. The radius sweep measures the time available before the first supporting contact on both bridge decks and a roof, alongside a deliberately late tile in each channel. [District paging](../knowledge/algorithms/district-paging.md)

## D-149 — Keep the model tree through runtime queries

On 2026-11-23 the city adopted the model node as the owner of a placed part across drawing, physical contact and acoustic queries. Each node retains its local transform and child list. Mesh instances can share bytes while a query identifies the particular part. The static viewer preserves the entity hierarchy, and jointed objects can express their shapes and joints in local frames. District pages retain the source owner across splits and fallback. [Object model](06-objects.md) · [File format](08-object-format.md) · [Nested fixture](../tools/object-convert/test_hierarchy.py)

## D-150 — Measure the first physical instant on a ballistic arc

On 2026-11-23 the city added a continuous query for the first contact of an upright capsule on a fixed triangle mesh. It returns the instant, surface, source triangle, point, normal and owning node where one exists. This gives the fixed-step physical replay an independently testable geometric prediction on the same inputs. A falling actor may request exact support triangles from the district catalogue while the full physical tile travels. [Ballistic query](../knowledge/algorithms/ballistic-impact.md) · [District paging](../knowledge/algorithms/district-paging.md)

## D-151 — Bind automatic visual detail to projected geometric error

On 2026-11-23 the object viewer adopted independent selection per model node with an optional bound on local surface deviation. The viewport, lens, node transform and camera pose determine its projected error. The two thresholds allow distinct outward and inward transitions. Existing files can keep their fixed mesh choices. [Format and compatibility](08-object-format.md) · [Method and oracle](../knowledge/algorithms/projected-mesh-error.md)

## D-152 — Certify the open space between missing collision faces

On 2026-11-23 the Paris district cooker added bounds for collision faces outside its temporary support mesh. A client may advance through a verified opening while the full page is in transit. A threatened static surface keeps the capsule at its preceding simulated pose until that page is ready. Earlier support catalogues use the page bounds when any faces are absent. [District paging](../knowledge/algorithms/district-paging.md) · [Doorway oracle](../worlds/ville-paris/check-clearance.py)

## D-153 — Trigger the city impact voice from the physical replay

On 2026-11-23 Ville-Paris connected the bridge sound prototype to its point and rigid-body drops. The collision binding identifies the physical surface in the object tree. The renderer schedules the direct wave at the moving observer on the same sample clock for the window and for recorded output. This makes the first audible frame reproducible across both software endpoints. [Method and controls](../knowledge/algorithms/city-contact-sound.md)

## D-154 — Apply acoustic surface transmission to physical impacts

On 2026-11-23 Ville-Paris connected the direct impact wave to the acoustic bindings selected by the object tree and to the companion sound scene of the bundled bridge. The physical contact supplies the origin and the observer supplies the endpoint at reception. The receipt lists the intervening bindings and their combined transmitted energy. The window and stereo exporter use the same filters. [Method and controls](../knowledge/algorithms/direct-sound-transmission.md)

## D-155 — Follow city impacts around the edges of obstacles

On 2026-11-23 the city contact player gained an indirect arrival from the acoustic geometry of the object tree. A direct ray identifies potential obstacles. A second search follows exposed edges, including a second wall encountered on a bent leg. The path to the camera at reception determines time, band response and apparent direction. Each receipt retains the ordered nodes and all segment lengths. The same material voice and ear model feed the live and recorded mixes. [Geometry and oracle](../knowledge/algorithms/diffracted-city-contacts.md)

## D-156 — Drive articulated impact sound from measured contact impulses

On 2026-11-23 Ville-Paris began dating a ragdoll impact at the end of the native interval in which the actor receives its measured reaction. The recording keeps the physical evidence, including the limb and the owning collision surface where both can be identified. Several simultaneous feet can excite a single material voice. An unresolved balance on the same support retains all candidate limbs and their geometric witnesses. The renderer applies the same sound propagation and stereo processing to these events as to the other city impacts. [Contact impulse API and oracle](../knowledge/algorithms/contact-impulses.md) · [City material voice](../knowledge/algorithms/city-contact-sound.md)

## D-157 — Distinguish exterior diffraction from escape through a closed room

On 2026-11-23 the city added an exterior-wedge index for closed convex acoustic components. Each junction retains both incident surfaces across the object tree and both outward normals. The two legs of a bend must remain outside the solid or on its boundary. A path along a face uses the tolerance of that specific component to handle single-precision geometry. [Exterior edge algorithm and checks](../knowledge/algorithms/exterior-acoustic-edges.md)


## D-158 — Scale an articulated impact by its measured reaction

On 2026-11-23 the city connected the level of its articulated impact voice to the positive impulse on its participating limbs and surface. The source accumulates 4 ms of measured reactions. Incoming closing speed and accumulated impulse set a bounded gain using a documented work proxy. Each increment acts at its own simulation time. The receiver applies it after the corresponding propagation delay. The first reacting interval starts the sound even if later measurements arrive on the next public tick. A shared balance across two feet contributes once and retains both candidates. [Acoustic excitation and limits](../knowledge/algorithms/city-contact-sound.md) · [Source and causality test](../worlds/ville-paris/src/impact/excitation.rs)

## D-159 — Read acoustic geometry for finite district segments

On 2026-11-23 Ville-Paris began indexing the world-space bounds of source acoustic bindings before requesting their geometry. An ordered series shares each verified mesh until its last candidate binding, while every hit retains its original owner and material. The 100-by-100 metre diagonal control went from 66 file reads totalling 10,676 bytes to one 108-byte mesh. [Method and limits](../knowledge/algorithms/paged-acoustic-rays.md) · [Disk and HTTP oracle](../worlds/ville-paris/check-acoustic-reads.py)

## D-160 — Preload audible geometry before a district landing

On 2026-11-23 the district capsule began reserving acoustic mesh files along the predicted source-to-listener path. A solved contact starts the material voice. Direct and diffracted paths can become available on different ticks. Each retains its original emission and arrival times, including when the wavefront has passed before the required file arrives. [Scheduling, shared playback and checks](../knowledge/algorithms/paged-impact-sound.md)


## D-161 — Preserve the orientation and bounds of transformed district geometry

On 2026-11-23 the district cooker began composing a shared world transform per source node. A reflected collision placement reverses its local triangle indices once when producing world-space faces. A pair of reflections preserves the original order. This lets the full collider and the temporary support beneath a falling actor agree with the resident object. Districts cooked with the old reversed physical faces need a fresh cook from their source. Acoustic bounds now enclose the actual placed vertices, including points where cancellation moves an edge outside a previous directory's double-precision estimate. The reader accounts for earlier bounds during a query. [Object format](08-object-format.md) · [Collision checks](../worlds/ville-paris/check-mirrored-collision.py) · [Acoustic checks](../knowledge/algorithms/paged-acoustic-rays.md)

## D-162 — Keep the articulated actor alive across district terrain changes

On 2026-11-23 Ville-Paris began running the same Rapier rig and joint solver on streamed and resident terrain. The district requests physical pages for individual limbs, carries the authored owner through terrain replacement and displays the solved part poses. The acoustic channel measures the reacting limbs at their actual solver substep. [Algorithm, scope and controls](../knowledge/algorithms/paged-ragdolls.md)

## D-163 — Give static terrain and the articulated actor a consistent creation order

On 2026-11-23 the resident Rapier scene with a multi-body rig and exclusively static terrain began creating the rig before adding the terrain. This matches the district, where the player can already exist when the ground arrives. A four-way control varying terrain insertion and the page edge registry independently reproduced both earlier trajectories. This changes historical resident poses after impact in that configuration. Holding an actor pose fixed also provides an independent control for the renderer. [Dynamics implementation](../crates/osage-dynamics/src/rapier.rs) · [Terrain controls](../worlds/ville-paris/check-paged-ragdoll.py) · [Two supporting pages and arrival order](../worlds/ville-paris/check-ragdoll-contact-order.py)

## D-164 — Retain active contacts on identical certified terrain

When the detailed collision page arrives, validate the complete page. At each original shape slot, compare the owner, surface ID, material, ordered indices and every vertex component with the installed certified geometry. An exact match keeps its Rapier collider and current contacts. Build genuinely different colliders before publishing the page, then remove the obsolete handles. On two adjacent supports a post-impact replacement previously moved a limb by up to 18.85 cm relative to the fully resident path. Keeping the identical colliders made all 79 states of both rigs match. A mixed page keeps one floor, installs its neighbour's new side wall and can later be evicted. [Terrain publication](../worlds/ville-paris/src/paging/support.rs) · [Collider handoff oracle](../worlds/ville-paris/check-collider-handoff.py)

## D-165 — Preserve simultaneous impact voices across terrain delivery orders

On 2026-11-23 the district began reserving the acoustic paths of all limb samples tied at the next predicted contact tick. The shared impact measurement processes simultaneous surfaces in authored order before selecting each material excitation seed. This preserves distinct, independently arriving sounds when two feet strike two different materials. [Method and cost](../knowledge/algorithms/paged-impact-sound.md) · [Two-panel comparison](../worlds/ville-paris/check-simultaneous-impacts.py)

## D-166 — Keep the struck moving support in the district simulation

On 2026-11-23 Ville-Paris began stepping an actor and a movable solid together on paged terrain. Both bodies contribute to collision reads and to the decision to advance a tick. The renderer uses the solved transform of the solid. Sound measures the relative normal velocity at the contact, including rotation, and reserves geometry for the path from a possible moving impact to the listener. [Physical comparison](../worlds/ville-paris/check-paged-bonnet.py) · [Moving contact and stereo recording](../worlds/ville-paris/check-paged-bonnet-audio.py)

## D-167 — Store original collision triangles independently of visual tile borders

On 2026-11-23 Ville-Paris began placing compact source colliders in a single physical page. Larger meshes can assign complete triangles to the cell containing each triangle's centroid. Bounds cover the full geometry in the page. The source node, material, winding and each ordered triangle survive the cook. The renderer still clips visible geometry at tile borders. Existing districts can be read as originally cooked. [Collision geometry oracle](../worlds/ville-paris/check-collision-topology.py) · [Object geometry](08-object-format.md)

## D-168 — Preserve the complete indexed collider of repeated box-shaped supports

On 2026-11-23 the safety catalogue gained an optional corner pattern for repeated small solids. Each instance reconstructs its exact source vertices, indices, winding and duplicate vertices from the original corner order and coordinates. The reader checks the reconstructed upper faces and residual triangle bounds. It checks the complete physical page against the ordered collider before promotion. Older catalogues still publish their available upper faces. Identical support holds keep their native manifold while a person and a moving object settle. [Geometry and replay checks](../worlds/ville-paris/check-corner-support.py)

## D-169 — Encode streamed collision pages with exact indexed geometry

On 2026-11-23 the district cooker adopted a binary collision page. Each shape takes its owner and material from the same ordered slot in the tile directory. General meshes store float32 positions and triangle indices. A mesh matching the repeated block topology can store its eight corners, two horizontal rectangles or six box coordinates when reconstruction preserves every source bit, index and winding. The ordinary paging path reads the packed pages and earlier JSON districts. Across the 226 physical pages of Bir-Hakeim the payload falls from 566,789 to 19,777 bytes for the same 5,292 triangles. The directory, fallback catalogue and visual resources have separate byte counts. [Writer](../worlds/ville-paris/paging_collision_codec.py) · [Reader](../worlds/ville-paris/src/paging/tiles/collision.rs) · [Replay and compatibility oracle](../worlds/ville-paris/check-packed-collision-reader.py)

## D-170 — Preserve the original triangle identity of solved mesh contacts

The identity of a physical face consists of its source tree node, its named collision surface and the zero-based triangle ordinal on that surface. The cooker assigns the ordinal before partitioning the mesh. Fragmented shapes carry increasing ranges of original faces. The subset installed for certified support has an additional mapping to the full shape. In a Rapier contact the manifold identifies the touched triangle on the mesh side. Both the full and paged readers report its original ordinal. Unsupported historical mappings and analytic shapes leave the field absent. Version 3 resident traces record the new field and the verifier can reproduce the earlier trace layouts. [Source order and validation](../worlds/ville-paris/src/paging/tiles/faces.rs) · [Reflected terrace and ramp replay](../worlds/ville-paris/check-source-faces.py) · [Catalogue validation and recovery](../worlds/ville-paris/check-source-face-validation.py)

## D-171 — Reduce the district's first transfer while preserving the initial scene

The district reader negotiates gzip for its JSON directory over HTTP. A static origin may return identity. A verified cache entry stores the decoded bytes with the validator of the origin's chosen representation. The coarse visual scene also has a lossless representation that groups repeated glTF fields and binary buffer views. The cooker compares its size with the existing parent representation. This keeps primitive attributes, material assignments, placements and source owners available on the first frame. The old representation remains readable. [Directory and cold route check](../worlds/ville-paris/check-index-transfer.py) · [Coarse scene writer](../worlds/ville-paris/paging_coarse.py) · [First-frame comparisons](../worlds/ville-paris/check-bootstrap-frames.py)

## D-172 — Test the swept bounds against certified residual triangles

An omitted collision face can occupy half its bounding rectangle. A falling capsule above the other half used to wait for a missing physical page. When the three vertices of a single omitted face lie on corners of its bounding box, the support catalogue can store their corner indices. The reader tests the swept bounds against that triangle after the bounding-box filter. Overlap and tangency keep the hold. An unrepresented or multi-face group retains its original conservative bound. The full page validates the certified vertices when it arrives. The 441-shape Paris district needs no extra records. [Triangle clearance and validation](../worlds/ville-paris/src/paging/support/clearance.rs) · [Ten measured approaches](../worlds/ville-paris/check-residual-triangle-paths.py) · [Corruption tests](../worlds/ville-paris/check-residual-triangle-certificates.py)

## D-173 — Report the original triangles and the coverage of a finite segment

A resident object's visual, physical and acoustic surfaces can each yield several crossings, including two authored triangles sharing an edge or occupying the same position. Keep the original surface binding and report each crossed face with its ordinal, world position and distance. A district query examines the installed static collision state at the chosen tick. It reports which missing pages or residual regions could still add hits. Queries reuse the current geometry without scheduling a page load or advancing the simulation. [Finite segments and coverage](../knowledge/algorithms/scene-ray-coverage.md) · [Resident object checks](../worlds/ville-paris/check-object-rays.py) · [Paged district checks](../worlds/ville-paris/check-paged-collision-rays.py)

## D-174 — Share resident object triangle indexes across placements

Each source visual mesh supplies one local triangle hierarchy. An outer hierarchy selects its placements, preserving each source owner, transform and surface binding. A collision or authored acoustic surface stored in world space shares a placement only when the ordered triangles and transformed vertex bits agree. A divergent surface keeps its own index. The inspector creates results only for bindings crossed by the segment. Across 1,024 placements of an 81-face model, triangle and placement BVHs occupied 88,272 bytes, down from 16,588,736. The differential checks include a one-ULP height difference, a changed face order, oblique and reflected placements and repeated owners under the district tree. [Method and measurements](../knowledge/algorithms/instanced-object-rays.md) · [Scaling check](../worlds/ville-paris/check-object-ray-scaling.py) · [Transform controls](../worlds/ville-paris/check-instanced-ray-affines.py)

## D-175 — Build resident acoustic triangle trees when a path reaches their surface

The listener's direct path and the edge search for diffraction first test the world bounds of the placed acoustic surfaces. Only a candidate surface builds its triangle tree. The source binding owns that tree, allowing separate streamed impact paths to reuse an already prepared placement and release it with the underlying geometry. The stereo recording reports which geometry its search trees retained. In the 1,024-screen replay the sound touches one screen. The reader indexes 130 triangles across that screen and one other acoustic binding. [Technique and costs](../knowledge/algorithms/acoustic-surface-index.md) · [Direct and diffracted stereo comparison](../worlds/ville-paris/check-acoustic-surface-index.py)

## D-176 — Account for native speculative contacts when certifying an upright capsule step

When a missing page would hold an isolated upright Rapier capsule, compare its swept capsule and the native speculative contact distance with attributed missing triangles. Preview contested motion against the installed terrain. A centred new support must also persist into the next step. A coupled body, uncertain geometry or a failed preview keeps the hold. On an oblique approach the page can become necessary two ticks earlier than the old swept box indicated. [Native geometry and preview](../crates/osage-dynamics/src/streamed/clearance.rs) · [District reservation](../worlds/ville-paris/src/paging/support.rs) · [Drop and oblique trajectory comparison](../worlds/ville-paris/check-capsule-residual-clearance.py)

## D-177 — Certify the closed acoustic surfaces reached by a sound path

An actual crossing of a blocking surface selects the neighbouring surfaces that may complete its volume. The resident hierarchy finds the transitive closure of overlapping world bounds. Certification of convex closed shells remains exact within that group. Silent frames and clear paths can finish without certifying a room. Each checked placement reuses the result for later contacts in the same snapshot. [Algorithm and limits](../knowledge/algorithms/acoustic-solid-regions.md) · [Silent, audible and two-obstacle replay](../worlds/ville-paris/check-acoustic-solid-regions.py)

## D-178 — Share distant native terrain shapes and keep the original contact frame

An independently placed fixed surface keeps its native collider, material and source face ordering. When an isolated body is far enough away, an exactly translated copy can share the source native triangle shape. Before its original bounds enter the next substep’s reach, restore the original world-space triangle array on the same handle. Directly using the translated native frame at contact changed oblique trajectories even though all world vertices had the same bits. Jointed rigs and moving-box scenes can share shapes at the identical world position. In the 1,024-placement high drop, both engines began with one shape and restored two neighbouring copies. [Method and measurements](../knowledge/algorithms/native-static-instances.md) · [Native storage oracle](../worlds/ville-paris/check-native-shape-storage.py) · [Pose, material and source-face oracle](../worlds/ville-paris/check-native-mesh-instances.py)

## D-179 — Route a sound path through finite openings in authored rooms

Attach room bounds and ordered portal quadrilaterals to nodes of the object forest. The district directory carries the same bindings and transforms as the resident model. For a geometric query, locate both endpoints in the room hierarchy and search their open connections. Optimize every crossing on its finite aperture in world metres. Return the feasible polyline, a lower bound, the ordered owners and whether the search finished. A dense cyclic graph retains a feasible route when the search reaches its work limit. Full closure disconnects its two sides through that opening. The first implementation supplies geometry to the resident and paged inspectors. [Object format](08-object-format.md) · [Algorithm and oracle](../knowledge/algorithms/acoustic-room-portals.md)

## D-180 — Sound becomes crates of the engine: osage-sound, its voices, its analyser

Date: 2026-11-24

The prototype of the Pont de Bir-Hakeim had its own Cargo workspace under `tools/paris-soundscape/`. It now lives in three crates of the root workspace. `paris-soundscape` remains a tool: the command and the bridge's scene. (Numbered after the entries written in the working copy for the city programme, D-145 to D-179, not yet committed, so that no two decisions share a number.)

- **Decision:**
  - `crates/osage-sound` holds the scene read from a glTF asset's extras, the ways, the ears, the engine and the offline render.
  - `crates/osage-sound-voices` holds the voices synthesised from their state.
  - `crates/osage-sound-analysis` holds what a recording tells of where its sounds are, read by the oracles (`verify`, `measure`) and by the tests.
  - `tools/paris-soundscape` keeps the command (`render`, `verify`, `measure`, `bench`, `tree`) and the scene of the bridge, which leaves the engine (`paris_soundscape::bridge()`).
- **Why:** other crates and worlds can depend on the engine's sound properly, and continuous integration runs its tests in a job of its own. The city world (`worlds/ville-paris`) already uses the ears, the faces, the loss round an edge and the contact voice.
- **Licence:** the engine's (AGPL with the osage exceptions) for all three crates and for the tool. The voices stay out of `worlds/` and out of the specification, SDK and example-world directories.
- **Kept:** the renders do not change. Built in the root workspace, without the prototype's own release profile (one codegen unit, thin LTO), the command gives the same bytes: the two digests of its README.
- **For the code that used the prototype by path:** the packages are renamed: `osage-paris-soundscape` becomes `osage-sound`, `osage-paris-voices` becomes `osage-sound-voices`, `osage-sound-analysis` keeps its name. Renaming them in a manifest keeps its code as it is: `osage-paris-soundscape = { package = "osage-sound", path = "../../crates/osage-sound" }`. `Scene::bundled()` is gone with the bridge's scene; code that used it depends on `paris-soundscape = { path = "../../tools/paris-soundscape" }` and calls `paris_soundscape::bridge()`, which returns the same `Scene`. [Prototype](../tools/paris-soundscape/DESIGN.md)

## D-181 — Drive a vertical support on the fixed physics clock

A resident city replay can prescribe one horizontal lift by its bottom centre, half-extents, vertical travel, period and friction. A native kinematic rigid body follows the cosine stroke in metres at each solver substep. Both adapters expose its pose and its contact surface through the existing rigid trace. The standing capsule is free under gravity. The lift check replays the trip with each solver, reconstructs the foot and the contact in the local platform frame and measures the largest excursion from the starting point. [Fixed-step contacts](07-city-scale-quality.md#q10--fixed-step-contacts-and-a-multilayer-character-controller) · [Physics interface](../knowledge/modules/osage-dynamics.md) · [Lift oracle](../worlds/ville-paris/check-lift.py)

## D-182 — Retain a kinematic support while the district replaces terrain

The district holds the lift in the same Rapier world as the passenger. A stable route surface identifies its contacts across the addition, replacement and removal of cell colliders. Native collider order is established before the first solver step. Each subsequent step uses the same prescribed motion as the resident replay. A passenger moving with the support can be grounded at a nonzero world velocity. The replay exposes the poses, contact point, normal and stable surface so that the streamed trip can be checked against the fully loaded city. [Streamed terrain contacts](../knowledge/algorithms/paged-ragdolls.md) · [Lift replay comparison](../worlds/ville-paris/check-paged-lift.py)

## D-183 — Limit sustained limb penetration into a box supported by terrain

On 2026-11-24 the Rapier adapter began distinguishing the transient compression of a limb against a supported dynamic box from a resting contact. A terrain impulse opposite to gravity establishes support. The collision solver can use its stiffer terrain coefficients when a limb approaches the deep-overlap range. The box retains its mass, inertia and full dynamic response. Shallow resting contacts retain their ordinary stiffness. The city checks both the number of consecutive deep samples at 240 Hz and the time to settle. [Solver](../crates/osage-dynamics/src/rapier.rs) · [Supported-foot regression](../crates/osage-dynamics/tests/contacts.rs) · [Pedestrian impact measurements](../knowledge/oracles/pedestrian-impact.md)

## D-184 — Damp the relative swing of a free limb on a supported branch

A passive Rapier rig uses its parent–child tree to find a terminal segment that has carried no contact impulse for the native sleep interval. When its parent is quiet and carries a reaction against gravity, the free joint axes dissipate relative angular velocity at a rate set by gravity and the size of the segment. The city’s drop oracle checks motion in flight, co-rotation, the onset and duration of rest, and the depth and timing of impact. [Solver](../crates/osage-dynamics/src/rapier.rs) · [Measurements](../knowledge/oracles/pedestrian-impact.md)

## D-185 — Sound is synthesised from state and placed in the object tree

Date: 2026-11-24

How osage makes and places sound, as the prototype of the Pont de Bir-Hakeim built it and its review checked it (D-180, issues #13 to #22). (Numbered after the city programme's entries written in the working copy up to D-184, not yet committed, so that no two decisions share a number.)

- **Decision:**
  - A sound is **synthesised from the state of what makes it** — speed, acceleration, load, a blow — by a voice, a generator with parameters. No audio file is read or sent for it; recorded voices may be added where synthesis falls short (issue #22), as `osage-kart-sound` already does.
  - A sound is **placed in the object tree**: an emitter is a node path and a position local to that node, so that it follows the node and its parents. Nothing else locates it.
  - What an object carries for sound has three sides: what it does to sound that reaches it (the acoustic material the object format already has), what it emits by itself (an **emitter** on a node), and what it emits when touched (a **contact sound** on a collision material). Emitters and contact sounds are to become fields of the object format; until its reader and specification take them, they are written in sibling `osageSound` keys, which the reader steps over.
  - A contact sounds as **two objects**: the softer of the two sets the push, each rings its own modes.
  - The scene's order is no part of it, and what a thing sounds like does not depend on who else is there.
- **Why:** a server already sends where things are; with sound made from state and placed by node, it sends nothing more for sound, and the same scene is heard on every device at what that device affords. Placing by node path gives sound the tree's cut for free: from afar a node's own emitters stand for its branch, as `subtreeProxy` does for the eye.
- **Not decided here:** where a contact's modes are set (object or material), when the fields move into the object format and in which version, how far sound is carried as a field (design/09 §6). [Design note](09-sound.md)

## D-186 — Reconstruct a moving sound from eight samples with a shared phase table

The reference tier of the sound engine reads each direct or reflected path at the fractional position supplied by its delay line. Seventh-order Lagrange weights replace the previous Blackman–Harris window to reduce the dominant spectral image of a moving source. Both readers span the same eight samples. A shared compile-time table holds 257 sets of weights. The end-to-end test checks the pitch and image level on the rendered loudspeaker signal. [Delay lines and Doppler](../knowledge/algorithms/sound-delay-lines.md) · [Reader, measurements and trade-offs](../knowledge/algorithms/moving-sound-delay.md) · [Physics tests](../crates/osage-sound/tests/physics.rs)

## D-187 — Use the inertia of a hanging branch to settle a supported ragdoll

On the parked bonnet a supported chest can leave the upper arm, forearm and hand hanging together. D-184 covered a free end attached directly to a supported segment. The Rapier adapter now propagates a quiet supporting contact down an unloaded branch, including through fixed joints. Each movable joint carrying descendants uses their combined bind-pose mass and inertia to set a viscous angular torque. The existing terminal-joint rate still applies at a free end. A load on any descendant withholds damping at its upstream joint. The three-part pendulum test checks that a compound limb swings under gravity and settles below the edge. [Branch damping](../knowledge/algorithms/hanging-ragdoll-branches.md) · [City impact oracle](../knowledge/oracles/pedestrian-impact.md)

## D-188 — Resolve the first inward crossing of an articulated body and a supported box

A passive joint tree falling onto a dynamic box can travel several centimetres per solver step. For an inelastic impact, the Rapier adapter estimates the time of the first inward crossing, checks the capsule against the oriented box, and advances the native solver on both sides of that instant. It spends extra stabilization iterations during the remaining fraction. To qualify again, the unloaded rig must clear the entire box along the previous impact normal. This distinguishes a second flight from a body sliding over an edge. The public replay records the first reported contact and the first geometric overlap separately, with the velocity of each surface at its closest point. [Supported impact algorithm](../knowledge/algorithms/supported-box-impact.md) · [Contact measurements](../knowledge/oracles/pedestrian-impact.md) · [Oblique, elastic and second-flight tests](../crates/osage-dynamics/tests/supported_impact.rs)

## D-189 — Reuse contact geometry across sleeping articulated steps

Continue advancing the native Rapier world at every substep. When all limbs and dynamic supports sleep through a complete substep, retain its geometric witnesses and reuse the supported-contact stiffness at the same step duration. A change of terrain or impulse observation invalidates the stored state. A waking rig reads fresh geometry on the step that wakes it. A kinematic lift takes the ordinary moving-contact path. [Algorithm](../knowledge/algorithms/resting-contact-witnesses.md) · [Wake regression](../crates/osage-dynamics/tests/resting_contacts.rs) · [City measurements](../knowledge/oracles/pedestrian-impact.md)

## D-190 — Refine supported contact while the articulated load is still approaching

After the first resolved inelastic crossing, use the box’s fixed support reaction and an inward load on its impacted face to identify compression. Measure projected travel from the relative normal momentum of the entire rig. If one interval would exceed the native linear error, bisect it within the articulated CCD budget and solve the joints and contacts together at each boundary. Once the load slows within tolerance on the face, end refinement for that impact. The 60 Hz replay and its 240 Hz pose observations keep their clocks. This reduces the largest foot overlap across the two tested rigs. Some landing postures take longer to settle. [Supported-box impact](../knowledge/algorithms/supported-box-impact.md) · [Offset landing and rest measurements](../knowledge/oracles/pedestrian-impact.md)

## D-191 — Index physical page bounds for trajectory and proximity requests

The district reader builds a hierarchy over the actual collision bounds of each physical page. A trajectory visits branches within the largest possible service horizon, then computes the deadline from each candidate’s own bytes. An actor leaving a page can still cancel a queued read or release a reservation because the reader includes the previous interests in the update. The exhaustive calculation serves as the test oracle. [Planning and measurements](../knowledge/algorithms/collision-read-planning.md) · [Reservations](../knowledge/algorithms/physical-page-reservations.md)

## D-192 — Search local physical pages before testing missing support

Query the district’s physical hierarchy separately for each swept body part. Only intersecting pages need the existing checks on their omitted faces. For support installation, visit the pages intersecting the wider sweep together with the previously active pages, in the original tile order. This preserves the retention margin, collider lifetimes and source identities when an actor leaves a region. [District paging and doorway controls](../knowledge/algorithms/district-paging.md) · [Support guard](../worlds/ville-paris/src/paging/support.rs)

## D-193 — Choose the smaller lossless encoding for the collision support catalogue

Measure the compressed bytes of the completed support JSON. Emit the shorter of zlib and a sized Zstandard frame when both encoders are available. Keep zlib as the portable writer fallback and explicit compatibility option. The native reader uses the reference Zstandard implementation through its safe Rust interface. It checks the declared size and frame boundary, decodes directly into the output buffer, and verifies the catalogue-to-directory binding before installing support. [Sizes, transport and replay](../knowledge/algorithms/district-paging.md) · [Writer checks](../worlds/ville-paris/test_paging_support_storage.py)

## D-194 — Let a headless simulation stream only the channels it consumes

A Ville-Paris replay started with `--no-visuals` reads the common district index, the support catalogue and the requested collision pages. The same solver, page planner, support handoff and source tree serve interactive and headless clients. The headless client skips the coarse drawing, visual page queue, model bank and image resources. Collision queries and explicitly requested acoustics keep their own data paths. A rendering client still verifies the coarse parent and resolves visual detail after choosing its texture format. [District HTTP measurements](../knowledge/algorithms/district-http.md) · [Channel and trajectory checks](../worlds/ville-paris/check-headless-collision.py)

## D-195 — Select an immutable district directory by digest and byte length

A district publisher can run `cook-pages.py --pin-index` and distribute the resulting digest and length with a world version. A client given `--index-pin SHA256:BYTES` reads the corresponding compressed directory through the same verified cache as the physical pages. This lets a static host serve a compact directory and lets a returning client start offline when all its requested files are cached. The cooker also publishes the ordinary JSON directory for clients that follow updates at the district URL. Both paths decode the same JSON and run the same district validation. [Transfer and replay measurements](../knowledge/algorithms/district-http.md) · [Snapshot oracle](../worlds/ville-paris/check-pinned-directory.py)

## D-196 — Load collision support from the live actor’s swept region

Allow a publisher to partition the certified support catalogue into immutable groups of physical tiles. Keep the whole catalogue available for consumers that require district-wide geometry. The native reader establishes support at spawn, can start reads ahead of an approaching body and waits for verified geometry before solving a threatened step. An optional physics-only pin carries physical page metadata and the object tree, including its acoustic bindings. [Selection, storage and limits](../knowledge/algorithms/district-http.md) · [Native HTTP oracle](../worlds/ville-paris/check-support-regions.py)


## D-197 — Keep the replay clock running while regional collision data is in flight

After spawn, the district client polls regional support reads on each wall-clock tick. It treats each unknown physical page as potentially solid throughout its declared bounds. The actor can advance through proven clear space. When unknown terrain reaches the conservative movement or speculative-contact bounds, the solver holds the pose and velocity until the required data validates. A completed detail read can wait for its region while the other lanes progress. The receipt records that wait separately from network service time. [Regional HTTP and contact tests](../worlds/ville-paris/check-support-regions.py)


## D-198 — Store physics-directory addresses and bounds in columns

For a headless district pin, the cooker measures two complete lossless encodings and publishes the smaller one. The ODP2 layout groups full SHA-256 digests and exact binary32 coordinates into columns while preserving the object tree, properties and order of the physical rows. The native reader restores those rows before the usual district validation and paging decisions. Wider source coordinates retain the JSON encoding. [Directory format and transfer measurements](../knowledge/algorithms/district-http.md) · [Native replay and malformed directory tests](../worlds/ville-paris/check-support-regions.py)

## D-199 — Deliver detailed collision addresses with regional support

A publisher can choose `--pin-regional-physics` for headless consumers. The pinned root carries the object tree, lengths and spatial bounds needed for selection. Each authenticated regional support file carries the full SHA-256 addresses of its detail pages. Nearby consumers resolve those addresses before admitting detail reads. A consumer that needs complete addresses, including a district-wide simulation, can fetch a separately pinned table. The ordinary index and previous immutable pins still open their original content. The default group width for this option is four tiles, based on the measured cold transfers for four Bir-Hakeim routes. [Format, timing and grouping trade-off](../knowledge/algorithms/district-http.md) · [HTTP qualification](../worlds/ville-paris/check-regional-addresses.py)

## D-200 — Carry exact collision pages when they cost less than their regional certificate

A headless publisher can use `--pin-bundled-physics` to choose the representation independently for each region. Pack the complete OCP1 tile payloads when they take fewer transfer bytes and fit within the previous decoded catalogue length. The client verifies the coordinate and page digest binding, decodes the shapes against the root's object and triangle metadata, and publishes them through the existing physics interest and support paths. Keep the sparse certificate when it wins or the page format requires it. [District transfer measurements](../knowledge/algorithms/district-http.md) · [Native HTTP oracle](../worlds/ville-paris/check-bundled-regions.py)

## D-201 — Let rendered districts open with regional collision data

A publisher can select `--pin-visual-district` when the camera needs all visual selection metadata and the actor needs only nearby terrain. The ODI2 root retains the source object tree, object profiles, visual row order, model variants, material and texture references. It carries visual tile digests in one binary column and uses the same regional certificates or full collision pages as the headless pin. The coarse parent and fine assets keep their immutable addresses across releases. A district without physical regions can publish the ordinary full snapshot. [Transfer and frame measurements](../knowledge/algorithms/district-http.md) · [Visual and contact oracle](../worlds/ville-paris/check-visual-district.py)

## D-202 — Package a district's first frame with its visual directory

A publisher can use `--pin-first-frame` to place the directory and coarse parent in one authenticated launch file. A known starting tile can contribute its collision region via `--first-frame-cell X Z`. Keep fine assets separately addressable. When the parent uses OVC2, measure both its existing bytes and its original lossless columns inside the outer compression frame. Preserve the full model and source ownership in both representations. The returned reference can be shared by multiple cameras or starting points. [Storage and cache trade-offs](../knowledge/algorithms/district-http.md) · [Native rendering and transport checks](../worlds/ville-paris/check-first-frame.py)

## D-203 — Use a cached first-frame package as the dictionary for a district update

A publisher can offer an optional patch alongside the complete ODI3 file. Use the previous decoded package as the compression dictionary so a small edit can reuse identical geometry across different wire encodings. Give the encoded file and the decoded result their own authenticated addresses. A cold client reads the complete file in one request. A returning client verifies the patch and the reconstructed payload, and stores the latter atomically for later releases. A failed update falls back to the complete current file. [Protocol and cost](../knowledge/algorithms/district-http.md) · [Release-chain oracle](../worlds/ville-paris/check-launch-updates.py)

## D-204 — Query moving supports at their current physics pose

A physical segment in a streamed district can cross a kinematic lift or a freely moving bonnet. Include these owners in the spatial index at the solver pose for that tick. Preserve local box face identities through translation and rotation. Return every transverse crossing and the clipped interval for travel along a face. Terrain gaps continue to describe the uncertainty from missing pages even when a moving support is known. [Segment coverage](../knowledge/algorithms/scene-ray-coverage.md) · [Lift and bonnet oracle](../worlds/ville-paris/check-moving-surface-rays.py)

## D-205 — Return occupied intervals on the posed physical actor

Callers can opt into the actor on a district segment query. A posed sphere or capsule contributes its whole occupied interval, its boundary points and their outward normals. Keep the slot and parent of each body even where several joint neighbours overlap. Bind imported slots to the node and material of the identified actor GLB. Keep the environment-only default for terrain and support queries. [Segment coverage](../knowledge/algorithms/scene-ray-coverage.md) · [Actor oracle](../worlds/ville-paris/check-actor-rays.py)

## D-206 — Measure the extent of support from the reported contact points

Project the mass centre and the upward witnesses of one numbered substep onto the horizontal plane. Keep the body slots in the support set and report a signed Euclidean margin. Two rear paws on a ledge can then give a negative margin while contact persists. With no witnesses the distance is undefined. Solvers supply the contacts and the measurement has no feedback into the dynamics. [Support footprint](../knowledge/algorithms/articulated-support.md)

## D-207 — Project the current support points with the solids that own them

Sum the momentum of every rigid body in the actor. Over an explicit interval, extrapolate the centre of mass and carry each current contact point with its supporting solid. Report the resulting horizontal margin alongside the measured one. This lets a world observe the change during a slide without applying a balance controller. [Support geometry](../knowledge/algorithms/articulated-support.md)

## D-208 — Keep the choice of contact radius explicit on sphere import

An articulated source sphere can carry a declared extent and a different contact margin. Preserve the established declared-radius import by default. Allow an explicit selection of either field and record the original values, selected radius and bone. A zero contact radius cannot instantiate a sphere in the object profile. [Conversion options](../tools/object-convert/README.md)

## D-209 — Spend a finer solver clock around the predicted first lift contact

Use the actual swept capsule and moving box to predict the first inward encounter of an airborne rig. A route may spend additional native solves near that encounter and record the first interval with a nonpositive manifold. Preserve the geometric time, the native witness time and the number of additional solves separately. [Method and oracle](../knowledge/algorithms/lift-contact-clock.md)

## D-210 — Bound local load-transfer sampling after the first lift impact

A route can request a travel target for the tick following first contact. Choose native subdivisions from both closing velocity at the witness and incoming momentum of the rig. Bound the response window to one replay tick so a later slide off an edge cannot consume an open-ended number of extra solves. Record the intervals and measure their effect from the posed geometry. [Method and oracle](../knowledge/algorithms/lift-contact-response.md)

## D-211 — Preserve the direction and twist axis of imported joint frames

Accept explicit body-to-joint matrices with twist on source Z. Keep the established joint-to-body convention as the default. Resolve parent and child orientations individually, retain their local anchors, and map the signed hinge and both principal swing extents to the portable frame. Record the selected convention and the original fields. [Frame conversion](../knowledge/algorithms/articulated-joint-frames.md)

## D-212 — Carry the swing boundary in the joint definition

A joint may specify an angular ellipse for tilt about its two transverse axes. Twist keeps its own interval. The converter marks eligible joints on explicit request. Both native solvers consume the same portable field and the replay measures first response and residual error from identical compound poses. [Swing envelope](../knowledge/algorithms/articulated-swing-envelope.md)

## D-213 — Apply a loaded twist stop along the bisector of its moving axes

For the Rapier angular envelope, form both angular constraint frames from half of the current swing and the original signed twist. The bisector gives the direction of the twist derivative during a tumble. The pivot and swing limits retain their own bind information. A falling quadruped on a rear-paw ledge checks the stop after the contact ends. [Constraint and oracle](../knowledge/algorithms/articulated-swing-envelope.md)

## D-214 — Repeat the impulse solve when a joint exceeds its angular range

For an articulated Rapier joint with an angular envelope, expand each directional half-range by one sixteenth to form a threshold for additional work. A breach requests three additional projected Gauss–Seidel passes on the child for the next native solve. Rapier takes the maximum request in the connected component. The elapsed time, authored limits and contact clock stay fixed. Rear-paw landings qualify this choice against uniform time division. [Angular stops](../knowledge/algorithms/articulated-swing-envelope.md)

## D-215 — Refresh lift contacts when a limb can cross a finite face edge

A cached face normal can extend past the edge and divert a falling limb that clears the actual box. Use expanded limb bounds to request native manifold refresh on a level kinematic lift near an edge. A central rider keeps the usual contact persistence. Compare an oblique pass with an identical free flight and certify positive geometric separation. [Lift contact response](../knowledge/algorithms/lift-contact-response.md)

## D-216 — Constrain transverse projections during the angular solve

For a joint with unequal swing widths, give Rapier fixed, two-sided angular stops between the child twist axis and four transverse directions in the parent bind frame. Each range contains every tilt admitted by the authored angular ellipse. The native solver can enforce those directions as an impact propagates along the chain, including during iterations between direction updates. Keep the circular bound and the directional stop for the exact boundary. Release the projection rows when the child axis faces backwards so the joint can pass the equator during recovery. On that side the original directional stop and circular cone guide recovery. The loaded quadruped oracle measures angular error at the existing public and native frequencies, including one-paw landings and five sliding speeds.

## D-217 — Increase the joint solve budget during sustained contact

When an unequal-width swing envelope belongs to a rig that received a positive normal contact impulse in the preceding native step, size its twist refinement margin using the smaller of its available twist range and its narrowest tilt range. A stop beyond the margin requests three extra projected Gauss–Seidel passes. On consecutive breaches under load the child doubles its request up to 24 passes. Inside the margin it releases the request. Without a contact impulse the ordinary refinement rule applies. The marked joint keeps its authored angular limits and the existing simulation clock. The loaded quadruped replay checks the maximum excess on a small pad without horizontal motion. Paired free-flight and isolated-pose replays exercise the contact condition.

## D-218 — Keep transverse stop directions through departure from a support

For unequal tilt widths, retain the two principal transverse projection directions throughout forward swing. A newly placed Rapier actor uses ordinary diagonals during unforced pose recovery. After the first measured contact impulse, use the normals of the two diagonals of a unit disk transformed into the transverse ellipse. Divide each component of the disk normal by the matching transverse half-width and normalise. Keep these directions while the actor is moving, including when it leaves a ledge. A sleeping actor or an explicit starting pose clears the selection. Prebuild the alternative rows and enable at most four fixed projections at once. Every projection still contains the authored envelope. The extra iteration budget continues to follow the current contact and limit error. The loaded quadruped replay checks an oblique landing five centimetres off centre and the departure from the ledge alongside the 300 isolated activation poses.

## D-219 — Test capsule endpoints before partitioning the spine

Use the derivative of squared distance to the solid box along the capsule spine to certify an endpoint minimum. At the first endpoint accept a nonnegative derivative. At the last endpoint require a strictly negative derivative so that a flat minimum keeps its earliest witness. For an interior minimum, partition at the face planes and minimize each quadratic as before. Free rigid pairs, jointly translating actors and lift flight queries use the same witness calculation. The solver clock and the contact response continue to use the resulting points.

## D-220 — Refine simultaneous twist and swing stops under load

A circular swing cone can press against its rim while the joint twists past a signed stop. After a measured external normal impulse, request additional impulse passes when the tilt has reached fifteen sixteenths of the permitted cone angle and twist has crossed its authored limit. Use three extra passes for the circular cone. Keeping the request fixed bounds the cost of sustained pressure at the rim. The ordinary angular error band applies again when either condition clears. A full-turn swing has no rim. Solving both constraints uses the existing native timestep. The regression drops a sixteen-part quadruped on a frictionless moving platform and checks angular error and penetration after touchdown at two local sampling rates.

## D-221 — Preserve stride phase when a person changes pace

Apply a changed human playback speed to the active clip at its current time. Starting another move still chooses its authored start phase and crossfades. Zero pace holds the current pose. The animation-system regression accelerates, pauses and resumes a single walking clip and checks its position each time.

## D-222 — Match a replicated walker's gait to its displayed travel

Measure horizontal distance in metres between the rendered root before and after interpolation. Divide by the elapsed frame time to choose the gait and divide that speed by the clip's nominal forward speed to set playback. At unit actor scale the installed walking and jogging clips give 0.979 and 5.919 m/s respectively, measured from the forefeet within two centimetres of the lowest point in the cycle. The world owns the walk and jog thresholds. Vertical correction and rotation in place contribute no ground travel. A frame with no elapsed time leaves the actor in place.

## D-223 — Plant each forefoot with a two-link leg correction on level ground

A walking or jogging world actor may keep each low forefoot at its touchdown position in the horizontal plane. Apply the correction after sampling the clip, within the reach of the hip and knee, preserving the shoe orientation. Fade out the offset as the foot rises. Restore the source rotations before evaluating the next frame. The world supplies the root path and opts in per actor. The flat-ground pedestrian replay compares both feet on the same input frames and exercises removing the component.

## D-224 — Adapt animated feet to the world's collision surface

A world may supply a stationary support height beneath each forefoot and the maximum step it considers traversable. Place the foot relative to its own support. When a lower ankle target exceeds the available leg span, lower the visual armature enough to recover reach and solve both legs from that pose. Leave the controller trajectory and animation clock intact. Missing support releases the affected foot. The city replay samples the same collision triangles under the actor and both feet while crossing both edges of a 15 cm raised pavement.

## D-225 — Bind ground contacts to paths in the model tree

Let the model identify an armature and any number of independent limb contacts. For each contact, its three direct ancestors provide the ankle and two driven links. Shared attachment nodes can move with the body. Each limb solves in its own parent's coordinates after the clip is sampled. The gait supplies a contact height and can release all contacts during flight. The human adapter binds the installed walking models by name. A four-legged fixture uses the persistent object IDs and separate fore and hind attachment nodes. A folded equal-length limb keeps its current knee direction when the target reaches its hip.

## D-226 — Sample a planted foot across its rigid sole

A foot may carry several points in the ankle frame. The horizontal anchor follows the authored contact. For vertical clearance, sample the ground at the translated position of each sole point and use the greatest required lift. A supported corner still contributes when the central marker is over a hole. The animation keeps its ankle orientation. In a replay of an animated fox, the skin above a curb can intersect its top while the central marker already clears the edge. The replay measures the deformed vertices of each paw at their own positions and reports stance gaps and sliding alongside penetration.

## D-227 — Check the deformed sole over the ground it covers

A nearby actor can use the drawn limb’s skin weights and triangle indices for ground clearance. Apply the weights after evaluating the leg correction. Where terrain changes height within a face, compare the overlapping sole and terrain surfaces. Bound the number of extra vertical corrections and retain the reachable pose with the least penetration. The original glTF positions, skin palette, inverse bind poses and mesh indices are sufficient to bind the samples. The fox replay measures the deformed vertices and crossings of the curb edge separately, and checks the visible gap and horizontal slip.

## D-228 — Choose a footprint from the whole stance of a looping gait

A model can supply a contact trajectory sampled from its clip and the root travel per cycle. Store horizontal corrections relative to the centre of each low interval’s swept bounds, then interpolate through swing. During playback the live foot anchor corrects residual motion. A planned low contact can require a visual body drop even on level ground. Keep the actor trajectory and both segment lengths. Allow up to eight post-solve skin refinements when a foot face approaches a ledge slowly. The fox replay tracks the rig marker and the same point of skin from one frame to the next. [Cyclic foot contact](../knowledge/algorithms/cyclic-foot-contact.md)

## D-229 — Drive pedestrian contacts from the playing clip and its model tree

A pedestrian can request planned steps on its loaded human rig. Calibrate a walk or jog from the body’s actual armature-to-contact paths, its nominal travel, and the installed animation curves. Reuse the sampled tables across actors of that body and gait. While one clip owns the pose, use its live seek time and duration. During a mixture of grounded clips, check the shoes against terrain using the mixed pose and suspend the single-clip plan. Keep the sole bindings when a walker stands idle. The Paris native replica opts in. [Cyclic contacts](../knowledge/algorithms/cyclic-foot-contact.md)

## D-230 — Resume a fading locomotion loop at its live time

A replicated change of gait can reverse before an earlier walk, jog or idle has finished fading. Retain the still-active animation state when Bevy promotes that loop back to the foreground. A retired loop uses the actor’s configured entry phase on its next start. The transition manager keeps ownership of weights and clip expiry. The fox replay isolates the command at zero elapsed time and compares the four distal joints on eight entry phases. [Loop redirection](../knowledge/algorithms/reversible-loop-crossfade.md)

## D-231 — Allocate pedestrian foot correction by its projected displacement

The Paris client keeps the full gait and skeleton running on each remote walker. It switches the optional foot plan, terrain sampling and leg solve together. Project the installed body's working box through the same perspective calculation as the visible object levels. Enable the solve above two pixels and keep it above one. Paris uses a 0.20 m displacement allowance from its curb replays and keeps the four metres around the eye active through changes in heading. A missing camera or viewport selects the detailed path. This applies to visual walkers; authoritative movement and contacts keep their own clocks. [Camera projection](../knowledge/algorithms/projected-mesh-error.md) · [Crowd replay](../worlds/paris/native/examples/pedestrian_crowd.rs)

## D-232 — Predict pedestrian encounters on the server

Paris moves its two autonomous passers after controlled walkers, in stable entity order. Use relative ground-plane velocity to check closest approach within both the current tick and a one-second forecast. This accounts for the arrival times of two walkers at a crossing. Choose among steps the existing grid can realise. Clients receive the resulting pose through the existing walker protocol. The replay checks continuous clearance and arrival at both ends. [Method and limits](../knowledge/algorithms/pedestrian-encounters.md)

## D-233 — Admit a Paris walker when its footprint fits at the entrance

After controlled movement, reserve space for new walkers in arrival order. Include the current positions of residents and all earlier reservations made in this tick. If the entrance is full, retain the session's request until a position becomes available or the session leaves. Spectators can follow the same replicated residents throughout. The region and SDK replay checks the positions that both an existing observer and a late observer actually receive. [Paris world](../knowledge/modules/paris-world.md) · [Replay](../worlds/paris/server/tests/arrivals.rs)

## D-234 — Plan Paris patrols on the same grid that supports their feet

A street can supply a pair of horizontal endpoints for each resident. Test direct travel first, then search neighbouring ground cells and remove waypoints only where a footprint can pass along the shortcut. Plan the stationary terrain at startup. Each tick the resident follows the route and steers around moving neighbours. The movement step checks the swept disc against cell edges and corners. A bit per centre cell caches its fully supported neighbourhood. This lets the world describe an intended destination without hand-authoring its corners. [Algorithm and oracle](../knowledge/algorithms/walk-grid-routing.md)

## D-235 — Include diagonal travel when choosing a pedestrian passage

Paris searches eight neighbours per walk-grid cell. Axial and diagonal steps have costs proportional to their length. Each candidate step must support the entire walker. When an obstacle offers two passages, the server can account for the distance covered along a diagonal before it chooses which side to follow. The two-passage replay checks the distance walked against a lower bound for the competing passage. [Walk-grid routing](../knowledge/algorithms/walk-grid-routing.md)

## D-236 — Visit walk-grid crossings in travel order

Merge the crossings of the two grid axes while testing the walker’s footprint. A blocked interval ends the query immediately. Terrain stays immutable, so the neighbourhood certificate used for a step can also serve each interval of a long shortcut when the full swept bounds fit inside the certificate. When rounding could extend the disc beyond a three-by-three neighbourhood, the grid uses the geometric check. Temporary storage per sweep is constant for both patrol planning and movement. [Algorithm and geometric checks](../knowledge/algorithms/walk-grid-routing.md)

## D-237 — Bound pedestrian encounters by relative travel on each axis

Paris keeps two sorted position arrays for a crowd, using the same starting positions throughout a tick. A candidate velocity and the greatest neighbour speed bound how far separation can close on each axis. Only people who can reach the comfort distance on both axes need a closest-approach calculation. Each accepted velocity updates the speed bound before the next person decides. The [steering oracle](../knowledge/algorithms/pedestrian-encounters.md) compares the indexed and exhaustive distances that affect movement.

## D-238 — Keep a safe standing position for every undecided pedestrian

Score the path already committed by each earlier walker and the standing position of each later walker before considering where the later walker might move. In a convergence involving several neighbours, give the candidate velocities a common passing side. A plaza replay follows all 64 residents at the fixed simulation rate. It checks the continuous distance between every pair and requires everyone to cross and return. [Encounter scoring and limits](../knowledge/algorithms/pedestrian-encounters.md)

## D-239 — Let a blocked pedestrian yield space at a busy destination

When a passer remains almost stationary for three quarters of a second, offer a sideways or retreating step for up to six tenths of a second. Use the same ground and body clearance checks as a normal step. Take the direct step whenever it clears. The offset-destination plaza checks that walkers sharing a small arrival area can finish both legs of their routes. [Movement and oracle](../knowledge/algorithms/pedestrian-encounters.md)

## D-240 — Let an opposing patrol retreat to a supported passing place

A pedestrian stalled in single-file traffic can follow the walk grid back to a place where its body fits beside the lane. It waits for the oncoming resident and then resumes the same patrol. Give priority to the resident that has yielded more often, using replicated IDs to resolve ties. Terrain support and simultaneous body clearance constrain the retreat and the return. [Algorithm and limits](../knowledge/algorithms/pedestrian-encounters.md) · [Street replays](../worlds/paris/world/src/passers/tests/single_file.rs)

## D-241 — Refine pedestrian search positions at narrow bends

A 40 cm body can follow a 50 cm passage even when the centres of its 25 cm floor cells lie too close to the walls. Paris first tries the direct path and the existing cell-centre route. If both fail, check whether the endpoint cells have a chain of touching floor cells within stepping height. On connected floor, retry A* once at half the cell spacing. Check every candidate and shortcut against the original floor with the same swept body. The [street replay](../worlds/paris/world/src/passers/tests/single_file.rs) sends two residents around 50 cm and 75 cm bends in both directions and rejects the 25 cm bend. [Walk-grid routing](../knowledge/algorithms/walk-grid-routing.md)

## D-242 — Measured head responses for headphones: SADIE II's KEMAR, through virtual loudspeakers

For headphones the ears were a sphere and five echoes (Brown and Duda): left and right are right, height is weak and behind sounds like its mirror in front. Use measured head-related impulse responses where a client has them (issue #18):

- **The set:** subject D2 (a KEMAR mannequin, 8 802 directions) of the SADIE II database of the University of York, in SOFA. Its licence is Apache-2.0: redistribution is allowed with its notice and a reference to the dataset, which the manifest entry and the card give. A dummy head rather than one person, since a game cannot measure each player.
- **How it is shipped:** the archive is an asset with its address and hash, fetched by `tools/fetch-assets.py` and kept out of git like every other. A tool (`tools/make-hrir.py`, reading the SOFA file with HDF5's own `h5dump`) derives a file of a few hundred kilobytes: for each of a set of virtual loudspeakers, the two ears' responses with their onsets taken out and cut short, and a table of the time between the ears over the sphere. The engine reads that file when it is given; without it, the sphere model stays, so the engine and the game run with no asset at all.
- **How it is applied:** each way is panned to the virtual loudspeakers around its direction, and each loudspeaker is filtered by its two measured responses, once for all the ways: the cost depends on the loudspeakers, not on the ways (128 at high quality). The time between the ears stays a delay of each way's own, read from the measured table, so that it is not blurred by the panning. The far field's virtual loudspeakers (#17) are heard through the same responses.

## D-243 — Judge osage's sound against reference A by controlled scenes and a blind test written down first

Every claim about osage's sound was a measurement of osage alone. Set it beside the external reference of the city programme (reference A, design/07) in two ways, both reproducible by anyone with the captures:

- **Controlled scenes:** small geometries that each ask one thing (`tools/paris-soundscape/scenes/controlled-*.json`), played by both engines. Each one's figures are read alike from every take by `paris-soundscape against`, against what the geometry asks: where the ears hear a sound, its pitch as it passes, when a click arrives, what a screen or a floor does to it. A surface's effect is read as the scene with it over the scene without it, so that the two engines' own voices and levels do not enter. The figures are committed in research/11. Reference A's captures stay in the evaluation track's private notes, observed from outside as a player hears them.
- **A blind listening test** of paired preferences, with its protocol written down before it runs (`tools/paris-soundscape/listening/PROTOCOL.md`). Both takes of a trial are at the same loudness, in the same kind of file, under random codes. Which take is X is balanced across listeners, and each page holds a catch trial. There are four questions: place, distance, the sources, the space. The analysis is decided beforehand: the share preferring osage with ties as half, and a bootstrap over listeners made simultaneous across the questions. A verdict needs its interval clear of 50 %. There are at least 8 listeners, and nobody is excluded.

What either part finds worse in osage becomes a ticket of its own.

## D-244 — The sound engine's documented limits taken in (#21)

The reviews of the prototype left seven limits written down rather than fixed. Six are fixed in the engine, each with a test against the physics it stands for:

- **A face that moves.** It mirrors from where it was when the sound met it: its node keeps a trail, as an emitter does. The faces that hide are still looked at where they are now.
- **A hidden contact.** A contact that does not reach the ears when it is struck takes a voice no other contact holds, and rings out there. A listener who comes round what hid it hears what it still makes. It is worth nothing against a heard contact until it is heard itself.
- **More orders.** Ways are mirrored twice, by the six sheets that mirror an emitter loudest once, at the qualities that check what hides a way (eight ways at most for each emitter). Where no single edge leads to the head, the way goes round two edges, with ISO 9613-2's double diffraction.
- **What faces scatter.** It feeds the reverberation's eight lines by the direction it comes from, each line standing for a corner of a cube round the head, with as much power as before. Two walls' copies of a tone then neither add nor cancel as one. An emitter none of whose ways reaches the ears, hidden or left out by the budget, still feeds the reverberation: the loudest, as many as a quarter of the budget.
- **How long the space rings.** A scene may ask for this to be found from the faces round the listener (`"secondsT60": "fromFaces"`). Rays from the head are followed from face to face until they leave for the sky, and the energy still in flight gives each band's decay. The estimate is kept up to date a few rays a block. A scene that gives its decays keeps them, and the bridge does: its seven faces do not stand for the city round it.
- **Ties at the budget's edge.** Ways as loud as one another at the edge of the budget are kept or left out together, so that a scene mirrored about the listener is heard alike on both sides. They are all kept if that oversteps the budget by an eighth of it at most.

The seventh limit, one thread, is left for a ticket of its own. Sharing emitters between cores asks how many cores a game gives its sound, how the live thread and WebAssembly take part, and how the samples stay the same whatever the number of threads.

## D-245 — Voices from recordings, and levels of detail for what is heard from afar (#22)

Every voice was synthesised; a world could not give a car or a step a recorded sound. And what is far cost as much as what is near, though the air had taken its top. Decided (issue #22):

- **Recordings are content.** A scene names its clips (`asset.extras.osageSound.clips`: an Ogg Vorbis or WAV file from the assets' root, and whether it loops), each one in the asset manifest with its source, licence and hash. An emitter or a contact sound plays them with `recorded`. For an emitter, these are loops each recorded at a known speed or load, crossfaded and played faster or slower as the state goes, as the kart's engine is. For a contact, they are one-shots, one drawn from the seed for each blow. The synthesised voice may sound under them as a layer, for what was not recorded. A recording that was not read leaves the synthesised voice alone. Reading Ogg takes `lewton`, already the workspace's.
- **Levels of detail like a texture's mipmaps.** Each clip is kept at its own rate and at each half of it. The engine tells each voice the frequency at which the air takes 24 dB on its straight way. The voice plays the coarsest level that still holds that frequency with a fifth to spare, crossfading over 50 ms when the level changes. A clip given at a lower rate has no finer level, so a world may ship a quarter of the bytes for what is only heard from afar. Far off, a voice also plays one loop where the other is 25 dB down, one shot of its set and no layer. At reference quality nothing is made more simply.
- **The rule is what can still be told, measured.** A level is used where it stays within 1 dB of the whole recording in every third of an octave the air leaves. One loop is used instead of two where it stays within 2.5 dB. Leaving out a loop 16 dB down changed a third by 10 dB: two recordings differ in colour, so the rule is −25 dB. A level of rate saves no processor time, since a voice makes 48 000 samples a second whatever it reads. It saves bytes when a clip is shipped at a lower rate. One loop instead of two halves a loop voice's cost.

## D-246 — The sound engine shares its emitters among threads, with the same samples whatever their number (#23)

The engine rendered on one thread, though its emitters are independent. Decided (issue #23):

- **How many threads.** A game tells the engine how many threads to share its emitters among (`Options::threads`, 1 by default); the engine never takes cores by itself. In a browser's WebAssembly, which has no threads, it is one.
- **A pool kept from block to block.** Starting three threads each block cost 235 µs, a tenth of a block. A pool of threads that wait between blocks is woken for some 25 µs, so it is written into the engine: `osage_sound::pool`, a few dozen lines, with no new dependency. As `std::thread::scope` does, the caller lends a task and waits until every thread is done with it. Doing so takes one `unsafe` erasure of a lifetime, the first in the sound crates; it is confined to that module, its reasons are written beside it, and the module is tested, a panic on another thread included.
- **What is shared.** The ways of the emitters due them are found by each thread taking the next emitter not yet taken: a few costly emitters do not hold one thread while the others wait. The voices and the ways carried to the ears are shared by groups of eight emitters in their order, each into a mix of its own. What depends on every emitter stays on one thread: the tree, the contacts, the ranking under the budget, the reverberation, the virtual loudspeakers' responses.
- **The same samples whatever the number.** What an emitter finds depends on it alone. The groups' mixes are added in the groups' order, not the threads': 1, 2 or 4 threads give the same bytes (tested on the bridge, with measured ears, and with a far field). The live engine's thread uses the same pool.

