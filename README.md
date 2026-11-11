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
cargo run -p osage-server                     # gateway + Bevy region: WebSocket ws://127.0.0.1:7878, WebTransport 127.0.0.1:7879 (UDP)
cargo run -p osage-client                     # native Bevy client
npm install && npm run build                  # TypeScript SDK and three.js client
npm run dev -w @osage/client-three            # browser client (open the printed URL)
node packages/sdk-ts/examples/watch.mjs       # headless observer printing the replication
cargo test --workspace                        # end-to-end tests
```

**Transports.** Clients given a `ws://` address ask the gateway for its WebTransport endpoint and use it — browsers, the Bevy client, agents and bridges alike — and fall back to WebSocket when it is missing or blocked; the browser client shows which one it uses. By default the server makes a self-signed certificate valid 14 days, which clients accept through its announced hash; for a public world pass `--certificate chain.pem --private-key key.pem`, or `--webtransport off` for WebSocket only.

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

To **open** a function to the world, the owner says so explicitly: `--open-led tpacpi::kbd_backlight` makes that LED's brightness pre-approved (any participant may set it, within 0…max, checked by the bridge itself); it is switched off when the bridge stops — world link lost, Ctrl+C, SIGTERM or SIGHUP. A SIGKILL or a power cut cannot be caught by any process: a device whose safe state matters needs its own fail-safe as well. The bridge writes `/sys/class/leds/NAME/brightness` directly if it may, otherwise asks systemd-logind (allowed for the user of the active session, no root needed). Agents use the MCP tool `set_device`.

## Race in the kart world

The first game on osage: an original kart race for humans in the browser, AI agents and bots ([D-031](design/decisions.md#d-031--the-kart-world-shared-physics-exact-prediction-a-world-that-never-makes-up-a-step)). The world is a module ([D-030](design/decisions.md#d-030--world-modules-the-engine-provides-the-mechanisms-the-world-its-rules)) in [`worlds/kart`](worlds/kart), MIT OR Apache-2.0.

```sh
cargo run --release -p osage-kart-server       # the kart world (WebSocket 7878, WebTransport 7879)
npm run dev -w @osage/kart-client              # open the printed URL: the race starts with you
```

Drive with the arrows or WASD, drift with Space or Shift: hold it through a bend and release it for a mini-turbo (blue, then orange sparks). A gamepad works too. `?debug` shows the prediction diagnostics.

**On a phone** on the same Wi-Fi, the left thumb steers on a pad, the right one holds DRIFT or BRAKE, and the kart accelerates by itself. Serve on the local network — `cargo run --release -p osage-kart-server -- --listen 0.0.0.0:7878 --webtransport 0.0.0.0:7879` and `npm run dev -w @osage/kart-client -- --host` — open the "Network" URL Vite prints, and let the firewall accept TCP 5173 and 7878 (and UDP 7879) from the local network. A page served over plain HTTP from a LAN address is not a secure context, so the phone plays over WebSocket; WebTransport needs the page over HTTPS.

**The native client** (Bevy) joins the same races and is the testbed of the [rendering programme](design/03-rendering.md): a physical sky, cascaded shadows, HDR and bloom, hills and trees, a textured road, rounded karts that lean and hop, sparks, smoke and flames — each technique chosen per fidelity tier, so that an integrated GPU holds 60 frames a second.

```sh
python3 tools/fetch-assets.py     # optional: CC0 road and grass textures (6 MB, listed in assets/manifest.toml)
```

```sh
cargo run --release -p osage-kart-native -- --tier T2             # play: same controls, same world (C: view; Q/E/X: look left, right, back; F12: screenshot; --autodrive: demo; --sky evening: late afternoon)
cargo run --release -p osage-kart-native -- --bench --tier T2     # the bench: same frames on every machine, timed
```

The bench is the programme's oracle — see [`bench/README.md`](bench/README.md).

**An AI agent can race.** Start the server with `--trust-dev-issuer`, give the agent a delegation as above, and let it call `world_info`. It finds `kart.autopilot/1` with its description and drives through `send_intent`, e.g. `{"type": "kart.autopilot/1", "value": {"pace": 0.9, "line": 0.2, "drift": true}}`.

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
| [07 — Ocarina of Time reverse engineering](research/07-zelda-reverse-engineering.md) | What exists around the decompilation, what is legally usable (MIT renderer and tools: ideas), red lines, and what its community teaches about organising many contributors and AI agents |
| [08 — Recent kart racers](research/08-kart-racers.md) | What Mario Kart World, Sonic Racing: CrossWorlds and open-source kart games do; reusable code and CC0 assets; ten prioritised features for kart v2; how items fit exact prediction; red lines |

Each survey is a dated snapshot (2026-11-09 or 2026-11-10) with sources; claims that could not be verified are marked as such.

## Knowledge

The ideas behind the code — architectures, algorithms, rendering techniques and the checks that prove their claims — are kept in [`knowledge/`](knowledge/index.md), written to be read **without the code** and reused in other engines. It is a bundle in the [Open Knowledge Format](https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md) v0.2 with an [osage profile](knowledge/profile.md): one card per module, technique, algorithm and oracle, each with its sources, its invariants, its limits and what it costs. A machine checks it (`python3 tools/check-knowledge.py`), and it can join an [OKF Federation](https://github.com/manaty/okf-federation) as a member bundle ([D-034](design/decisions.md#d-034--knowledge-is-kept-as-an-okf-bundle-with-an-osage-profile)). The first cards cover rendering and the kart world; the other modules follow.

## Design notes

Architecture decision records: [ADR-0001 — Architecture](design/adr/0001-architecture.md) · [ADR-0002 — Protocol v0](design/adr/0002-protocol-v0.md). All decisions: [decision log](design/decisions.md).

| Note | Subject |
|---|---|
| [01 — Accounts, uniqueness and the agent mesh](design/01-accounts-and-agents.md) | Per-world identifiers and assurance levels against fraud and ban evasion; AI agents as accountable participants of the server mesh. Reference adapters: [nexlink](https://www.nexlink.ph) (socially validated identities) and [ameesh](https://ameesh.org) (accountable teams of humans and AI agents) — neither is imposed |
| [02 — Mixed worlds, real devices and OpenDeviceDB](design/02-mixed-worlds-and-devices.md) | Real places captured and geo-anchored, real devices as live entities, AR participants, safe physical actuation through a local device bridge. Reference device layer: [OpenDeviceDB](https://opendevicedb.org) (open directory of 29,000+ devices, composable WASM components, MCP server) — not imposed |
| [03 — Rendering programme](design/03-rendering.md) | Towards the GTA 6 bar, together: one plugin per technique, fidelity tiers T1–T4, oracles (the bench, reference images), workstreams R1–R14, content rules, how to contribute a technique |

## First world

The first consumer is **2030workspace**, a 3D virtual workspace for IT teams whose web client runs on three.js. It is the target of the phase-1 vertical slice: one world joined at the same time by a browser client, a native client and an AI agent.

## About the name

**OSAGE** stands for **O**pen **S**ource **A**dvanced **G**ame **E**ngine. The project was called *opage* until November 2026.

Osage is also the name of the **Osage Nation** (Wahzhazhe), a sovereign Native American nation headquartered in Pawhuska, Oklahoma. **This project is not affiliated with, endorsed by, or speaking for the Osage Nation.** The project rejects cultural appropriation, so:

- osage does **not** use Osage symbols, imagery, language, stories or any other cultural element — in its branding, its art or its world content — unless the Osage Nation chooses otherwise;
- the maintainer wrote to the Osage Nation (Wahzhazhe Communications) on 2026-11-09 to tell them about the name and ask for their view; **no answer yet** — their position will be published here and respected;
- in the long term, if the project grows, it intends to support the Osage Nation's interests — on the Nation's own terms, and only in ways the Nation asks for.

## Licence

- **Engine, server, tools, device bridge:** [GNU AGPL-3.0-or-later](LICENSE) with the additional permissions of [`LICENSE-EXCEPTIONS.md`](LICENSE-EXCEPTIONS.md) — GPU vendor components, application stores, world modules, platform components.
- **World Module Interface** (`osage-world-module-api`) and **example worlds** (`worlds/`): MIT OR Apache-2.0 — a world module may carry its authors' licence ([`LICENSE-EXCEPTIONS.md`](LICENSE-EXCEPTIONS.md) §C).
- **Protocol specification:** Community Specification License 1.0; `.proto` schemas and examples: Apache-2.0. **Client SDKs and interface bindings:** MIT OR Apache-2.0 — so that any client, including proprietary and app-store apps, can join an osage world ([D-003](design/decisions.md#d-003--licences-open-protocol-and-permissive-sdks-agpl-engine-and-server), [D-020 to D-023](design/decisions.md#d-020--agpl-30-or-later), [licensing review](research/06-licensing-review.md)).
- **Documentation** (including `research/` and `design/`): [CC BY-SA 4.0](LICENSE-DOCS).

Decisions taken so far are in the [decision log](design/decisions.md).
