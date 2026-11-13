---
type: Module
title: osage-kart-client
description: The browser client of the kart world, in TypeScript with three.js — the kart predicted exactly by the shared physics compiled to WebAssembly, the others smoothed, the paddock's things and the rain drawn.
resource: https://github.com/smichea/osage/tree/main/worlds/kart/client
licence: MIT OR Apache-2.0
status: draft
tags: [kart, client, browser, three.js, webassembly]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-13T13:30:00Z" }
---

# Purpose

Let anyone race in the kart world from a browser — a computer, a phone on the same Wi-Fi — with nothing to install, on the same world as the native client and the agents.

# Concepts

- **The world's replica:** the SDK (`@osage/sdk`) keeps every entity's components as the world sends them; the client reads them by type name (`kart.state/1`, `kart.racer/1`, `kart.race/1`, `kart.paddock/1`, …).
- **The player's kart, predicted:** its physics is the world's own, `osage-kart-physics` compiled to WebAssembly, stepped at 60 steps a second from the player's controls and reconciled with each state the world confirms ([exact client prediction](/algorithms/exact-client-prediction.md)).
- **The others, smoothed:** drawn gliding towards the state the world last sent.

# Architecture

`main.ts` connects through the SDK (WebTransport where the page may use it, else WebSocket), applies the world's messages to the replica, builds the circuit when a track arrives (`scene.ts`: road, kerbs, verges, barriers, trees; over hills the road's height is the physics'), and each frame predicts the player's kart (`prediction.ts`, `physics.ts`), poses every kart, follows the player's with a chase camera and writes the HUD and the panels in HTML. `input.ts` reads the keyboard, a gamepad, or the touch pads of a phone. `paddock.ts` (D-084) draws what the world sends of its paddock — crates bobbing and turning, hidden while broken; tyres rolling where they go; cones standing, or sliding and tumbling once knocked (D-079); oil spreading; drones flying with their rotors turning — each placed on the road's height under it (`Physics::roadHeightAt`: the WebAssembly `track_along`, then `track_lift`), from pools of meshes reused frame to frame; and the rain when the race is wet (D-068): 1 400 streaks falling round the camera, kept in a box about it, and the sky and haze greyed, easing in and out over four seconds.

# Interfaces

The page's query: `?server=ws://HOST:PORT` (default: the page's host, 7878), `?debug` (the prediction's diagnostics). Keys: arrows or WASD, Space or Shift to drift, F or Ctrl (a gamepad's North button, ITEM on a touch screen) for the item — held, what trails waits behind the kart, let go, it goes (D-052) — Enter for the host to start, T (or a button) for the next event; between races the driver and the kart are picked with buttons. The HUD shows the item held, how many and how to use it.

# Invariants

- The predicted kart steps with the world's physics, bit for bit: the same Rust, compiled to WebAssembly.
- What the world has not sent is not drawn as if it had: things are placed where the world says, moved on by their own velocity only.

# Knowledge

[osage-kart-physics](/modules/kart-physics.md) (the WebAssembly interface), [osage-kart-items](/modules/kart-items.md) (what the things are), [exact client prediction](/algorithms/exact-client-prediction.md).

# Reuse

The replica-and-prediction pattern serves any world whose physics compiles to WebAssembly; `paddock.ts`'s pools serve any set of world objects that come and go each frame.

# Limits

Plainer than the native client: flat colours, no photographed materials, no sound, no replay. Things are drawn at the world's time, not as far ahead as the player's kart is predicted, so a tyre the player throws shows a moment behind the kart. The browser's own kart does not knock into the others in its prediction (D-076 is the native client's).
