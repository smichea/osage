---
type: Algorithm
title: Exact client-side prediction
description: A client predicts its own vehicle with no visible correction, because the server applies exactly the steps the client sent and never makes one up.
resource: https://github.com/smichea/osage/blob/main/worlds/kart/native/src/predict.rs
licence: MIT OR Apache-2.0
status: draft
tags: [netcode, prediction, determinism]
sources:
  - id: fiedler
    resource: https://gafferongames.com/post/networked_physics_2004
    title: "Glenn Fiedler, Networked Physics, 2004"
  - id: gambetta
    resource: https://www.gabrielgambetta.com/client-side-prediction-server-reconciliation.html
    title: "Gabriel Gambetta, Client-Side Prediction and Server Reconciliation"
  - id: own
    resource: "osage's own design and measurements, 2026-11-09 and 2026-11-10"
    title: osage design
generated: { by: "claude-code/claude-fable-5-2", at: "2026-11-10T06:45:00Z" }
---

# Problem

The server decides where every kart is, so that nobody can cheat. But a driver who waited for the server's answer before seeing the kart turn would feel the network's delay in the steering.

# Idea

Classic prediction and reconciliation[^fiedler][^gambetta]: the client applies its own controls at once, and replays the unconfirmed ones on top of each state the server sends back. What makes it **exact** here is three additions:

1. The server and the client run the **same physics, bit for bit** ([osage-kart-physics](/modules/kart-physics.md)).
2. Controls are **numbered steps** of one sixtieth of a second, not a stream of "current input".
3. The server applies those steps in order and **never makes one up**[^own].

# Method

Client, every physics step:
1. Read the controls; give the step the next sequence number.
2. Apply it to the predicted kart; keep it in a list of pending steps.

Client, every frame: send **one message** with the steps taken this frame plus the three before them (at most 24). The message is a datagram; repeating recent steps covers a lost one.

Server, every tick, for each human's kart:
1. Add one unit of credit, up to 8.
2. If more than 6 steps are waiting, take two this tick; else one. Each step taken costs one credit.
3. Apply the steps taken, in sequence order, and record the last sequence applied.
4. If no step is waiting, apply **nothing**: the kart keeps its state. After 8 ticks without one, the driver is taken to be gone and the kart coasts.

Server state sent to the client: the kart's physics state and the last sequence applied.

Client, on each state for its own kart:
1. Drop pending steps up to that sequence.
2. Start from the server's state and re-apply the pending steps.
3. The difference between what was shown and the new prediction is not snapped: it is kept as a visual offset that fades in about a quarter of a second. Beyond 4 m, snap.

Drawing: between two physics steps, draw the kart part of the way from the previous predicted state to the present one, so that display rates other than 60 Hz stay smooth.

# Invariants

- If no other kart touches the player's, the replayed prediction equals the previous one exactly: the correction is zero.
- A client cannot go faster than 60 steps a second however many it sends: the credit bounds catching up.
- A step is applied at most once, in order.

# Cost

One physics step per pending step on each server state: about 15 steps in play. Negligible.

# Failure modes

- **Repeating the last controls when a step is late** (what the first version did): every late step becomes a misprediction, seen as a correction of up to a metre.
- **One datagram per step:** a browser may send only about one datagram per rendered frame; the server then sees a fraction of the steps and falls seconds behind. Measured: 90 m behind after 8 s.
- **Contact with other karts** is predicted in the native client since D-076: each kart's state carries the controls of its last step; the client steps the others on with them, held, by as many steps as it is ahead, and knocks its own kart into them there at every step (replayed ones too). It is exact only while the others hold their controls and are not knocked themselves: what is left is a correction, faded. The browser client does not predict contacts.

# Alternatives

Rollback of the whole world on the client (as SuperTuxKart does) predicts contacts too, at the cost of simulating every kart on every client. Input delay avoids prediction but is felt.

# Oracle

`a_client_predicts_its_kart_exactly` in the kart world's tests: 600 steps with a drift and a wall, then server and client states compared for equality. Run by `cargo test -p osage-world-kart`.

# Reuse

Any server-authoritative game with a deterministic simulation can use it. The rule that transfers: **the server waits rather than guesses**.

[^fiedler]: Networked Physics
[^gambetta]: Client-Side Prediction and Server Reconciliation
[^own]: osage design
