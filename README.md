# osage — Open Source Advanced Game Engine

An open-source engine for large open worlds, built in the open by people and AI agents. Site: **[osage-engine.org](https://osage-engine.org)**.

> **Status: implementation, pre-release (November 2026).** The engine's core — the world protocol, the server, native and browser clients, SDKs, accountable AI agents, a device bridge, rendering by fidelity tier — runs, and its first world, **osage kart**, is playable: [Windows builds](https://github.com/smichea/osage/releases), from integrated graphics to ray tracing. Interfaces may still change between releases. Corrections and counter-arguments are welcome — with sources.

## Goals

1. **Top rendering** — on high-end hardware, the technical level of Rockstar's RAGE (GTA 6).
2. **Any hardware** — scales down to old and low-end machines.
3. **Modular** — many teams of humans and AI agents work in parallel on the engine and on all its tools.
4. **Any client joins the same world** — native, browser, mobile, low-end, headless AI agents, each at its own fidelity.
5. **Trust built in from day one** — account uniqueness against fraud and bad players, and accountable AI agents (each with a responsible human), both through open, pluggable interfaces.
6. **Mixed worlds** — real places, real devices and people in AR share the world with virtual content; in-world actions can drive real devices, with safety enforced next to the device.

## How it is built

Neither a fork nor a rewrite from scratch: osage builds on **Bevy + wgpu** for the world server and the native client, contributes upstream what belongs there, and writes its own value where nothing open exists — **an open world protocol, fidelity tiers from headless agents to ray tracing, accountable agents, world modules, content in open formats, and the tools to measure all of it**. Server meshing for seamless large worlds and content streaming (glTF, 3D Tiles) are next. The survey behind this direction: [research/00-synthesis.md](research/00-synthesis.md).

Every change carries its reason and its measure: a [decision log](design/decisions.md) (one entry per decision), [knowledge cards](knowledge/index.md) (one per module, technique and algorithm), and [the bench](bench/README.md) (the same frames on every machine, timed — on the developer's laptop and on a cloud GPU).

## Play osage kart

The first world: an original arcade kart racer — drifts, tricks, items, six circuits, cups, time trials with ghosts, battles, bots, two players on one screen, a crowd that follows the race, rain and night races, a live leaderboard page.

**On Windows**: download `osage-kart-windows-x64.zip` and `osage-kart-content.zip` from the [releases](https://github.com/smichea/osage/releases), unzip both into one folder and run `play.bat` (the folder updates itself before each game). `play-two.bat` splits the screen for two players; `host.bat` serves the race to the local network. On an RTX card, `play.bat --tier T4` traces the light.

**On a Mac with Apple silicon**: download `osage-kart-macos-arm64.zip`, unzip it and double-click `play.command`; the first time, allow it in System Settings › Privacy & Security ("Open Anyway"), and the folder downloads its content by itself. `play-two.command`, `host.command` and `join.command` do as on Windows, and the folder updates itself the same way.

**From source** (Rust ≥ 1.90, Node ≥ 22):

```sh
python3 tools/fetch-assets.py                                       # optional: the CC0 content (textures, models, skies, sounds), listed in assets/manifest.toml
cargo run --release -p osage-kart-server                            # the kart world (WebSocket 7878, WebTransport 7879)
cargo run --release -p osage-kart-native                            # the native client; the game picks its tier (or --tier T1..T4)
npm install && npm run dev -w @osage/kart-client                    # or the browser client: open the printed URL
```

Drive with WASD or the arrows, drift with Space or Shift (hold it through a bend, release it for a mini-turbo), use an item with F or Ctrl; a gamepad works too. While the world runs, its leaderboard is at `http://localhost:8080/leaderboard.html` when served by `play.bat`.

**On a phone** on the same Wi-Fi, the left thumb steers on a pad, the right one holds DRIFT or BRAKE, and the kart accelerates by itself. Serve on the local network — `cargo run --release -p osage-kart-server -- --listen 0.0.0.0:7878 --webtransport 0.0.0.0:7879` and `npm run dev -w @osage/kart-client -- --host` — open the "Network" URL Vite prints, and let the firewall accept TCP 5173 and 7878 (and UDP 7879) from the local network. A page served over plain HTTP from a LAN address is not a secure context, so the phone plays over WebSocket; WebTransport needs the page over HTTPS.

**The bench**: `cargo run --release -p osage-kart-native -- --bench --tier T2` plays the same 2400 frames on every machine and times them — the rendering programme's oracle ([`bench/README.md`](bench/README.md)).

**An AI agent can race.** Start the server with `--trust-dev-issuer`, give the agent a delegation (below), and let it call `world_info`. It finds `kart.autopilot/1` with its description and drives through `send_intent`, e.g. `{"type": "kart.autopilot/1", "value": {"pace": 0.9, "line": 0.2, "drift": true}}`.

## The engine's core

A world replicated by the osage protocol v0 to every kind of client — here the engine's own demo world, a server-driven cube:

```sh
cargo run -p osage-server                     # gateway + Bevy region: WebSocket ws://127.0.0.1:7878, WebTransport 127.0.0.1:7879 (UDP)
cargo run -p osage-client                     # native Bevy client
npm install && npm run build                  # TypeScript SDK and three.js client
npm run dev -w @osage/client-three            # browser client (open the printed URL)
node packages/sdk-ts/examples/watch.mjs       # headless observer printing the replication
cargo test --workspace                        # end-to-end tests
```

**Transports.** Clients given a `ws://` address ask the gateway for its WebTransport endpoint and use it — browsers, the Bevy client, agents and bridges alike — and fall back to WebSocket when it is missing or blocked; the browser client shows which one it uses. By default the server makes a self-signed certificate valid 14 days, which clients accept through its announced hash; for a public world pass `--certificate chain.pem --private-key key.pem`, or `--webtransport off` for WebSocket only.

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

Tools: `world_info`, `observe` (entities and components as JSON, decoded from the world's own schemas), `send_intent` (any intent the world describes), `move`, `set_device`.

**A real device.** Mirror this machine's sensors (CPU temperature, battery — Linux, read-only) into the world:

```sh
export OSAGE_BRIDGE_DELEGATION=$(cargo run -q -p osage-bridge -- delegate --credential "$CRED")
cargo run -q -p osage-bridge                      # observers, clients and agents now see the device and its readings
```

To **open** a function to the world, the owner says so explicitly: `--open-led tpacpi::kbd_backlight` makes that LED's brightness pre-approved (any participant may set it, within 0…max, checked by the bridge itself); it is switched off when the bridge stops — world link lost, Ctrl+C, SIGTERM or SIGHUP. A SIGKILL or a power cut cannot be caught by any process: a device whose safe state matters needs its own fail-safe as well. The bridge writes `/sys/class/leds/NAME/brightness` directly if it may, otherwise asks systemd-logind (allowed for the user of the active session, no root needed). Agents use the MCP tool `set_device`.

## Repository

| Path | What |
|---|---|
| `crates/osage-server`, `osage-region-link` | The replication gateway and the region server, and the contract between them |
| `crates/osage-client`, `osage-sdk`, `packages/sdk-ts`, `packages/client-three` | Native client (Bevy), Rust and TypeScript SDKs, browser client (three.js) |
| `crates/osage-world-module-api` | What a world's rules see of their region: worlds are modules, apart from the engine |
| `crates/osage-credentials`, `osage-agent`, `osage-bridge` | Identities and delegations; AI agents through MCP; real devices mirrored into a world |
| `crates/osage-render` | Rendering, one plugin per technique, configured per fidelity tier T1–T4 (sky, shadows, ambient occlusion, ray-traced light and its denoiser, post-processing, resolution, frame pacing) |
| `crates/osage-content`, `osage-props`, `osage-procedural` | Content from the asset manifest; models placed by the thousand with levels of detail; scenery sculpted from simple shapes |
| `crates/osage-update`, `osage-report`, `osage-music`, `osage-voice` | Self-updating releases, consented crash reports, music and radio, voice chat |
| `worlds/kart` | osage kart: the world (`world`, `physics`, `server`), its clients (`native`, `client`) and its reusable components (`components/*`: tracks, items, bots, crowd, roadside, sound, cups, ghosts, records…) |
| `spec/` | The protocol: specification text and `.proto` schemas |
| `design/`, `knowledge/`, `research/` | Decisions and programmes; one card per module and technique; the surveys behind the direction |
| `bench/`, `tools/`, `site/` | The rendering bench, the content and cloud-bench tools, the project's site |

## Documentation

**Knowledge.** The ideas behind the code — architectures, algorithms, rendering techniques and the checks that prove their claims — are kept in [`knowledge/`](knowledge/index.md), written to be read **without the code** and reused in other engines. It is a bundle in the [Open Knowledge Format](https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md) v0.2 with an [osage profile](knowledge/profile.md): one card per module, technique, algorithm and oracle, each with its sources, its invariants, its limits and what it costs. A machine checks it (`python3 tools/check-knowledge.py`), and it can join an [OKF Federation](https://github.com/manaty/okf-federation) as a member bundle ([D-034](design/decisions.md#d-034--knowledge-is-kept-as-an-okf-bundle-with-an-osage-profile)).

**Decisions and design.** Every decision, with its reason and what it measured: the [decision log](design/decisions.md). Architecture decision records: [ADR-0001 — Architecture](design/adr/0001-architecture.md) · [ADR-0002 — Protocol v0](design/adr/0002-protocol-v0.md).

| Note | Subject | State |
|---|---|---|
| [01 — Accounts, uniqueness and the agent mesh](design/01-accounts-and-agents.md) | Per-world identifiers and assurance levels against fraud and ban evasion; AI agents as accountable participants. Reference adapters: [nexlink](https://www.nexlink.ph) and [ameesh](https://ameesh.org) — neither is imposed | In part: credentials, delegations, agents through MCP; the adapters are to come |
| [02 — Mixed worlds, real devices and OpenDeviceDB](design/02-mixed-worlds-and-devices.md) | Real places captured and geo-anchored, real devices as live entities, AR participants, safe physical actuation through a local device bridge. Reference device layer: [OpenDeviceDB](https://opendevicedb.org) — not imposed | In part: the device bridge with read-only and pre-approved functions; places, AR and OpenDeviceDB are to come |
| [03 — Rendering programme](design/03-rendering.md) | Towards the GTA 6 bar, together: one plugin per technique, fidelity tiers T1–T4, oracles, workstreams R1–R14, how to contribute a technique | Under way: every workstream started, T4 traces its light |
| [04 — Kart programme](design/04-kart-programme.md) | The kart world to the play of the best arcade racers and a GTA 5–class look | Under way: play milestones M1–M4 done, M5–M6 in progress |

**Research.** The dated surveys (2026-11-09 to 11-10) that chose the direction, with their sources; claims that could not be verified are marked as such.

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

## Worlds

**osage kart** is the first world and the engine's proving ground. The first outside consumer planned is **2030workspace**, a 3D virtual workspace for IT teams whose web client runs on three.js: one world joined at the same time by a browser client, a native client and an AI agent.

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
