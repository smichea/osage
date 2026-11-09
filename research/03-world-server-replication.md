# 03 — World server, replication and shared content

Survey date: **2026-11-09**, from primary sources (official docs, release notes, GitHub). Several GitHub release pages show only day and month; years were inferred from version context. Items marked *unverified* could not be confirmed.

Question: what open building blocks exist for an **authoritative world server with an open protocol**, so that heterogeneous clients — native high-end renderer, browser, mobile, old hardware, headless AI agents — join the **same** world, each at its own fidelity?

## 1. Database-as-game-server / world-state servers

**SpacetimeDB** (Clockwork Labs) — latest v2.14.1 (2026-11-04), master at 2.15.0; 2.0 shipped Feb 2026; 25.5k stars.

- **Not open source today.** BSL 1.1: one production instance only, may not be offered as a database service. Each version converts to **AGPLv3 with a linking exception four years later** (Change Date 2031-11-05 for 2.15.0).
- Server logic ("reducers") in Rust, C#, TypeScript or C++.
- Client SDKs: TypeScript (browser, Node, Bun, Deno), Rust, C#/Unity, C++ for Unreal, an official Godot package (.NET builds, Godot ≥ 4.6.2) and a community pure-GDScript SDK.
- **Interest management by SQL subscription**: each client receives only the rows its queries select.
- Single-threaded per database; v2.14.0 (2026-10-30) added async calls between databases. It scales by splitting the world into region databases.
- **BitCraft** (the MMO built on it) published its server code under **Apache-2.0** (Phase 1, Jan 2026; client not included): one "global" database plus "region" databases.

**Nakama** (Heroic Labs) — Apache-2.0, v3.42.0, 13.7k stars. WebSocket or reliable UDP; official clients for JavaScript, Unity, Unreal, Godot, Defold, Swift, Java. Native browser support. Social/matchmaking/storage backend more than a spatial world server.

**Colyseus** — MIT, v0.18.12, 7.5k stars. Delta-compressed binary state over WebSocket; SDKs for TypeScript, Unity, Godot, Defold, Haxe, GameMaker, C. Native browser support. Room-oriented, not a large-world server.

**Convex** — self-hostable reactive database, FSL-1.1 (each release becomes Apache-2.0 after two years). Runs **AI Town** (MIT, ~10.8k stars).

## 2. Engine-level networking

**Bevy 0.20** (2026-10-08) still has **no official networking**. Community stack:

- **bevy_replicon** (now github.com/simgine) v0.45.5, MIT/Apache — server-authoritative replication with per-client visibility filters; pluggable transports (renet, quinnet, aeronet, matchbox).
- **lightyear** 0.31.0 (2026-10-21, Bevy 0.20), MIT/Apache — prediction, rollback and snapshot interpolation on top of replicon; room-based visibility; UDP plus WebTransport, WebSocket and Steam via aeronet; **runs in the browser (WASM over WebTransport)**.
- **naia** — Rust, UDP native / WebRTC on WASM, rooms and per-user scope.

**Godot** 4.7.2 (2026-08-18; 4.8 at beta2):

- High-level multiplayer API over ENet, WebSocket, WebRTC (web exports: WebSocket and WebRTC only).
- `--headless` on any binary; "Export as dedicated server" strips textures and materials.
- `MultiplayerSynchronizer` has per-peer visibility filters and `delta_interval`: basic interest management.

## 3. Open-source "metaverse" / social-world platforms

| Project | Status (verified) |
|---|---|
| **Overte** | Apache-2.0, release 2026.10.1 (2026-10-24). Native clients on Windows, Linux, macOS. **No web client found**; the old Vircadia web SDK (WebRTC) was last published at v2024.1.2 |
| **OpenSimulator** | Commits as recent as 2026-11-07; 0.9.3.x requires .NET 8. Exact version and licence *unverified* (official site unreachable) |
| **Hyperfy v2** | GPL-3.0, three.js + PhysX. v0.16.0 (Dec 2025), last commits Apr 2026 on dev; README says alpha; activity slowing |
| **Third Room** | Apache-2.0; paused for lack of funding since 2023 |
| **Ambient** | MIT/Apache; "paused indefinitely" |
| **Croquet / Multisynq** | Croquet Apache-2.0 with a Node.js "reflector" relay; Multisynq client repos archived mid-2025 during a move to a decentralised network; 2026 activity unclear |
| **Webaverse** | Status unclear; no licence or activity found |
| **Hubs Community Edition** | Self-host on Kubernetes; docs partly stale; no 2026 release found |

**Why the failed ones failed** (our reading): each was a single funded company with its own engine + protocol + format stack and a revenue model tied to hosting or crypto. When the money stopped, nobody else could maintain it. **OpenSimulator and Overte survive because they are community-governed and federated.**

**Worth reusing as ideas:** OpenSimulator's hypergrid federation, Overte's domain-server architecture, Croquet's reflector. **No 2025–2026 newcomer with real adoption was found.**

## 4. Large-scale open-world server tech

- **No mature open-source server meshing exists.** SpatialOS is proprietary; open "meshing" repos are prototypes.
- Closest building block: **SpacetimeDB region databases** (the BitCraft pattern), with inter-database async calls since v2.14.0.
- **Agones** v1.61.0 — Kubernetes fleets of dedicated game servers, moving into the CNCF.
- **Interest management**: replicon, lightyear, naia, Godot synchroniser filters, SpacetimeDB subscriptions. Grid area-of-interest example: godot-tiny-mmo (MIT).
- **Snapshot / delta**: lightyear (interpolation, rollback); @geckos.io/snapshot-interpolation (BSD-3) with typed-array-buffer-schema for binary encoding; Colyseus schema deltas.

## 5. Transports usable by native and browser clients

- **WebTransport**: Chrome 97+, Edge 98+, Firefox 114+, **Safari 26.4+ on macOS and iOS (since Mar 2026)** — about 92 % global coverage. IETF draft-17 (2026-11-02), in working-group last call, not yet an RFC; browsers still send the legacy protocol token; one report of Safari failing against webtransport-go.
- Servers: Rust **quinn** (quinn-proto 0.11.19) and **wtransport** (MIT/Apache, "not completely production-ready"); Go **quic-go/webtransport-go** (MIT, draft-16).
- **WebRTC data channels**: libdatachannel (C/C++, v0.24.6, also WebSocket); naia's WebRTC socket.
- **WebSocket**: the fallback that works everywhere.

## 6. Open standards for shared world content

- **glTF 2.0 (Khronos)**: **KHR_interactivity and KHR_gaussian_splatting are ratified** (splat compression left to separate extensions; SPZ still draft). KHR_physics_rigid_bodies is a review draft. **OMI extensions**: OMI_physics_* and spawn_point at stage 1, OMI_seat at stage 2.
- **OGC 3D Tiles 1.1**: OGC Community Standard — HLOD streaming plus metadata. Splats with HLOD proposed for **3D Tiles 2.0** (Cesium, Apr 2026). Browser: 3DTilesRendererJS (Apache-2.0) for three.js, Babylon.js, PlayCanvas.
- **OpenUSD**: AOUSD Core Spec 1.0 ratified late 2025, 1.1 planned for 2026; covers composition only (not materials or physics). **No light browser runtime confirmed.**
- **MaterialX** v1.39.5 (May 2026): OpenPBR 1.1.1, conversion graphs to glTF PBR, a JavaScript web viewer.
- **Metaverse Standards Forum**: "Web of Worlds" whitepaper (Jun 2026) proposes URIs for worlds and APIs for worlds, identity and assets — proposals only.

**Realistic path:** **glTF + KTX2 textures + meshopt/Draco, packaged as 3D Tiles HLOD**, serves both a high-end native client and a low-end browser from the same content; add KHR splats where useful. **USD + MaterialX for authoring**, baked to glTF for runtime.

## 7. Agents as first-class clients

- Headless clients: SpacetimeDB TypeScript/Rust/C# SDKs, Godot `--headless`, Mineflayer (behind **Mindcraft**, MIT, ~6k stars).
- **Bevy Remote Protocol**: bevy_remote 0.20.1, JSON-RPC 2.0 over HTTP, `world.query` plus "+watch" change streams.
- MCP wrappers: bevy_brp_mcp 0.23; a community SpacetimeDB MCP server (MIT; tables, reducers, SQL).
- Hyperfy + ElizaOS starter: an agent joins a world over WebSocket like a normal user.
- **Gap: no standard observation schema for agents.** Most game MCP servers are developer tools, not in-world clients; worldbox-mcp is the exception (several agents in one world under fog-of-war).

## Takeaways for opage

1. **Nothing open source does "one world, any client, any fidelity" end to end.** The pieces exist separately: replication (replicon/lightyear), state servers (SpacetimeDB, Nakama, Colyseus), transports (WebTransport + WebSocket fallback), content standards (glTF + 3D Tiles).
2. **The protocol and content format are the durable assets**, not a particular engine: the projects that survived are federated and community-governed; the ones that died were single-vendor stacks.
3. **SpacetimeDB is technically the closest fit but its licence (BSL 1.1) conflicts with an AGPL community project** until each version converts four years later. Using it as an optional backend is fine; making it the core is a governance risk.
4. **Server meshing / sharding for a large seamless world is the biggest server-side gap** — a natural place for opage to contribute.
5. **An agent observation/action schema** (structured, interest-managed, MCP-exposed) is unclaimed territory and fits opage's "humans + agents" goal.

## Not verified

Years on several GitHub releases (Nakama, Colyseus, quinn, libdatachannel, Agones); OpenSimulator's exact version and licence; Webaverse and Hubs activity; whether Multisynq is active in 2026; exact KHR_gaussian_splatting ratification date (2026-09-03 from one secondary source); built-in 3D Tiles support in Godot or Bevy; reasons for Ambient's pause (reported pivot to AI, secondary sources).

## Sources

- SpacetimeDB: https://github.com/clockworklabs/SpacetimeDB · https://github.com/clockworklabs/SpacetimeDB/releases · https://raw.githubusercontent.com/clockworklabs/SpacetimeDB/master/LICENSE.txt · https://spacetimedb.com/docs/tutorials/godot
- BitCraft: https://github.com/clockworklabs/BitCraftPublic · https://bitcraftonline.com/news/bitcraft-open-sourcing-update
- Nakama: https://github.com/heroiclabs/nakama
- Colyseus: https://github.com/colyseus/colyseus
- Convex: https://github.com/get-convex/convex-backend · AI Town: https://github.com/a16z-infra/ai-town
- Bevy 0.20: https://bevy.org/news/bevy-0-20/
- bevy_replicon: https://github.com/simgine/bevy_replicon · lightyear: https://github.com/cBournhonesque/lightyear · https://lib.rs/crates/lightyear · naia: https://github.com/naia-lib/naia
- Godot: https://godotengine.org/download/archive/ · https://docs.godotengine.org/en/stable/tutorials/networking/high_level_multiplayer.html · https://docs.godotengine.org/en/stable/classes/class_multiplayersynchronizer.html · https://docs.godotengine.org/en/stable/tutorials/export/exporting_for_dedicated_servers.html
- Overte: https://github.com/overte-org/overte · OpenSimulator: https://github.com/opensim/opensim/tags · Hyperfy: https://github.com/hyperfy-xyz/hyperfy · Third Room: https://github.com/matrix-org/thirdroom · Ambient: https://github.com/AmbientRun/Ambient · Croquet / Multisynq: https://github.com/croquet/croquet · https://github.com/multisynq
- Agones: https://github.com/googleforgames/agones/releases
- Snapshot interpolation: https://github.com/geckosio/snapshot-interpolation
- WebTransport: https://caniuse.com/webtransport · https://datatracker.ietf.org/doc/draft-ietf-webtrans-http3/ · https://webrtc.ventures/2026/04/webtransport-is-now-baseline-what-it-means-for-real-time-media/
- Transport libraries: https://github.com/BiagioFesta/wtransport · https://github.com/quic-go/webtransport-go · https://github.com/quinn-rs/quinn/releases · https://github.com/paullouisageneau/libdatachannel/releases
- glTF / OMI: https://github.com/KhronosGroup/glTF/tree/main/extensions · https://github.com/omigroup/gltf-extensions
- 3D Tiles and splats: https://cesium.com/blog/2026/04/27/3d-gaussian-splats-lod/ · https://github.com/NASA-AMMOS/3DTilesRendererJS
- OpenUSD: https://aousd.org/uncategorized/foundations-of-open-3d-development-introducing-aousd-core-specification-1-0/
- MaterialX: https://github.com/AcademySoftwareFoundation/MaterialX/releases
- Metaverse Standards Forum: https://metaverse-standards.org/news/blog/announcing-the-web-of-worlds-whitepaper-a-concrete-path-to-the-open-metaverse/
- Agent tooling: https://docs.rs/bevy_remote/latest/bevy_remote/ · https://docs.rs/bevy_brp_mcp · https://lobehub.com/mcp/fractaloutlook-spacetimedb-mcp-server · https://github.com/mindcraft-bots/mindcraft · https://github.com/fullya99/worldbox-mcp
