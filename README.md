# osage — Open Source Advanced Game Engine

An open-source engine for large open worlds, built by a community of humans and AI agents.

> **Status: research phase (November 2026).** There is no engine code yet. This repository holds the survey of what exists and the proposed direction. Corrections and counter-arguments are welcome — with sources.

## Goals

1. **Top rendering** — on high-end hardware, the technical level of Rockstar's RAGE (GTA 6).
2. **Any hardware** — scales down to old and low-end machines.
3. **Modular** — many teams of humans and AI agents work in parallel on the engine and on all its tools.
4. **Any client joins the same world** — native, browser, mobile, low-end, headless AI agents, each at its own fidelity.
5. **Trust built in from day one** — account uniqueness against fraud and bad players, and accountable AI agents (each with a responsible human), both through open, pluggable interfaces.
6. **Mixed worlds** — real places, real devices and people in AR share the world with virtual content; in-world actions can drive real devices, with safety enforced next to the device.

## Direction in one paragraph

Neither a fork nor a rewrite from scratch: build on **Bevy + wgpu** for the world server and the native client, contribute upstream what belongs there, and write osage's own value where nothing open exists — **an open world protocol, content streaming in open formats (glTF, 3D Tiles), server meshing for seamless large worlds, fidelity tiers from headless agents to ray tracing, an agent interface, and world tools**. Details, architecture, module map and open decisions: [research/00-synthesis.md](research/00-synthesis.md).

## Try the phase-1 skeleton

A server-driven cube, replicated by the osage protocol v0 to three kinds of client.

```sh
cargo run -p osage-server                     # gateway + Bevy region on ws://127.0.0.1:7878
cargo run -p osage-client                     # native Bevy client
npm install && npm run build                  # TypeScript SDK and three.js client
npm run dev -w @osage/client-three            # browser client (open the printed URL)
node packages/sdk-ts/examples/watch.mjs       # headless observer printing the replication
cargo test --workspace                        # end-to-end tests
```

Requirements: Rust ≥ 1.90, Node ≥ 22. Licences of each part: see [Licence](#licence).

**Identity and agents.** Start the world with `--trust-dev-issuer` (an **insecure** local issuer, for development only), then:

```sh
CRED=$(cargo run -q -p osage-credentials --bin osage-dev-credential -- alice@demo A2)   # alice's identity (A2)
export OSAGE_DELEGATION=$(cargo run -q -p osage-agent -- delegate --credential "$CRED")  # a delegation for one agent
cargo run -q -p osage-agent                                                             # MCP server on stdio
```

To let an AI agent harness use it, declare the MCP server, e.g. in a `.mcp.json`:

```json
{ "mcpServers": { "osage": { "command": "osage-agent-mcp", "args": ["--world", "ws://127.0.0.1:7878"],
                             "env": { "OSAGE_DELEGATION": "<token printed by osage-agent-mcp delegate>" } } } }
```

Tools: `world_info`, `observe` (entities and components as JSON, decoded from the world's own schemas), `move`.

**A real device.** Mirror this machine's sensors (CPU temperature, battery — Linux, read-only) into the world:

```sh
export OSAGE_BRIDGE_DELEGATION=$(cargo run -q -p osage-bridge -- delegate --credential "$CRED")
cargo run -q -p osage-bridge                      # observers, clients and agents now see the device and its readings
```

## Research

| Document | Question |
|---|---|
| [00 — Synthesis and recommendation](research/00-synthesis.md) | From scratch or not? Architecture, tiers, modules, roadmap, open decisions |
| [01 — Native engines](research/01-native-engines.md) | Godot, Bevy, O3DE, Stride, Wicked, Fyrox, Flax; Unreal 5.8 as benchmark |
| [02 — Browser clients](research/02-web-engines-webgpu.md) | WebGPU coverage, three.js, Babylon.js, PlayCanvas, Bevy/Godot on the web |
| [03 — World server & replication](research/03-world-server-replication.md) | SpacetimeDB, replicon/lightyear, metaverse platforms, transports, content standards, agents |
| [04 — Rendering vs RAGE](research/04-rendering-vs-rage.md) | What GTA 6 does, open renderers and libraries, ranked gaps |
| [05 — Mixed-reality standards](research/05-mixed-reality-standards.md) | WebXR/OpenXR, georeferencing, capturing real places, Matter/Home Assistant/ROS 2/MAVLink, WoT, safety and privacy rules |
| [06 — Licensing review](research/06-licensing-review.md) | Validity of the AGPL additional permissions, GPU vendor licences (NVIDIA, AMD, Intel), app stores, in-process vs out-of-process components, spec patents, AI-assisted contributions |

Each survey is a dated snapshot (2026-11-09) with sources; claims that could not be verified are marked as such.

## Design notes

Architecture decision records: [ADR-0001 — Architecture](design/adr/0001-architecture.md) · [ADR-0002 — Protocol v0](design/adr/0002-protocol-v0.md). All decisions: [decision log](design/decisions.md).

| Note | Subject |
|---|---|
| [01 — Accounts, uniqueness and the agent mesh](design/01-accounts-and-agents.md) | Per-world identifiers and assurance levels against fraud and ban evasion; AI agents as accountable participants of the server mesh. Reference adapters: [nexlink](https://www.nexlink.ph) (socially validated identities) and [ameesh](https://ameesh.org) (accountable teams of humans and AI agents) — neither is imposed |
| [02 — Mixed worlds, real devices and OpenDeviceDB](design/02-mixed-worlds-and-devices.md) | Real places captured and geo-anchored, real devices as live entities, AR participants, safe physical actuation through a local device bridge. Reference device layer: [OpenDeviceDB](https://opendevicedb.org) (open directory of 29,000+ devices, composable WASM components, MCP server) — not imposed |

## First world

The first consumer is **2030workspace**, a 3D virtual workspace for IT teams whose web client runs on three.js. It is the target of the phase-1 vertical slice: one world joined at the same time by a browser client, a native client and an AI agent.

## About the name

**OSAGE** stands for **O**pen **S**ource **A**dvanced **G**ame **E**ngine. The project was called *opage* until November 2026.

Osage is also the name of the **Osage Nation** (Wahzhazhe), a sovereign Native American nation headquartered in Pawhuska, Oklahoma. **This project is not affiliated with, endorsed by, or speaking for the Osage Nation.** The project rejects cultural appropriation, so:

- osage does **not** use Osage symbols, imagery, language, stories or any other cultural element — in its branding, its art or its world content — unless the Osage Nation chooses otherwise;
- the maintainer wrote to the Osage Nation (Wahzhazhe Communications) on 2026-11-09 to tell them about the name and ask for their view; **no answer yet** — their position will be published here and respected;
- in the long term, if the project grows, it intends to support the Osage Nation's interests — on the Nation's own terms, and only in ways the Nation asks for.

## Licence

- **Engine, server, tools, device bridge:** [GNU AGPL-3.0-or-later](LICENSE) with the additional permissions of [`LICENSE-EXCEPTIONS.md`](LICENSE-EXCEPTIONS.md) — GPU vendor components, application stores, world modules, platform components (*draft, pending legal review*).
- **Protocol specification:** Community Specification License 1.0; `.proto` schemas and examples: Apache-2.0. **Client SDKs and interface bindings:** MIT OR Apache-2.0 — so that any client, including proprietary and app-store apps, can join an osage world ([D-003](design/decisions.md#d-003--licences-open-protocol-and-permissive-sdks-agpl-engine-and-server), [D-020 to D-023](design/decisions.md#d-020--agpl-30-or-later), [licensing review](research/06-licensing-review.md)).
- **Documentation** (including `research/` and `design/`): [CC BY-SA 4.0](LICENSE-DOCS).

Decisions taken so far are in the [decision log](design/decisions.md).
