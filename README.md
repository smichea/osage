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

## Research

| Document | Question |
|---|---|
| [00 — Synthesis and recommendation](research/00-synthesis.md) | From scratch or not? Architecture, tiers, modules, roadmap, open decisions |
| [01 — Native engines](research/01-native-engines.md) | Godot, Bevy, O3DE, Stride, Wicked, Fyrox, Flax; Unreal 5.8 as benchmark |
| [02 — Browser clients](research/02-web-engines-webgpu.md) | WebGPU coverage, three.js, Babylon.js, PlayCanvas, Bevy/Godot on the web |
| [03 — World server & replication](research/03-world-server-replication.md) | SpacetimeDB, replicon/lightyear, metaverse platforms, transports, content standards, agents |
| [04 — Rendering vs RAGE](research/04-rendering-vs-rage.md) | What GTA 6 does, open renderers and libraries, ranked gaps |
| [05 — Mixed-reality standards](research/05-mixed-reality-standards.md) | WebXR/OpenXR, georeferencing, capturing real places, Matter/Home Assistant/ROS 2/MAVLink, WoT, safety and privacy rules |

Each survey is a dated snapshot (2026-11-09) with sources; claims that could not be verified are marked as such.

## Design notes

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

- **Code:** [GNU AGPL-3.0](LICENSE).
- **Documentation** (including `research/` and `design/`): [CC BY-SA 4.0](LICENSE-DOCS).

Whether the protocol specification and client SDKs should use a permissive licence, so that any client can join an osage world, is an [open decision](research/00-synthesis.md#open-decisions).
