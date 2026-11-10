---
type: Module
title: osage-region-link
description: The message contract between the replication gateway and a region server, the same in one process and across machines.
resource: https://github.com/smichea/osage/tree/main/crates/osage-region-link
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [server, gateway, region, messages, boundary]
generated: { by: "claude-code/claude-opus-5-5", at: "2026-11-10T13:00:00Z" }
---

# Purpose

The gateway and a region are two components of the server (D-037). This crate is everything they may say to each other. Nothing else passes between them: no shared state, and no handle on a client's connection. A region can therefore move to another machine without the gateway or the region changing; only the way the messages travel changes.

# Concepts

- **Session:** a client connection the gateway has admitted, known to the region by a number.
- **Member:** what the region knows of a session: its kind (human, agent, bridge), its role, its controller, and for a bridge the owner of its devices.
- **To a region:** a session joins, leaves, or sends a client message.
- **To the gateway:** the region is ready (its world's schemas and tick rate), it has taken a session in or let it go, or it sends a server message to one session or to all its members.
- **Route:** where the gateway delivers a session's messages: that session's connection.

# Architecture

Each end of a link is a pair of channels. The region reads what came in once a tick, without waiting; the gateway reads asynchronously. In one process the two ends are joined directly (`in_process`). Across machines (`network`), each side keeps its usual end, and two threads carry the other end over a TCP connection, one each way. Each message is a frame: a 4-byte big-endian length, then the message encoded in Protocol Buffers, which carry the client protocol's messages as they are. The gateway and the region never see which it is. When a side stops, it closes the connection; when the connection ends, the channels close, as if the other side had stopped.

The server's command line runs them apart: `--region-only ADDR` runs a region that waits for its gateway, and `--region-at ADDR` runs a gateway linked to it.

The gateway keeps a `Router`. It opens a session's route before telling the region the session joined. Then it delivers the region's messages in order: one session's messages to that session's connection, a broadcast to every session the region has taken in. The world-test harness uses the same router, so tests and the server deliver alike.

# Interfaces

`ToRegion` (`Join`, `Leave`, `Client`), `ToGateway` (`Ready`, `Joined`, `Left`, `Send`, `Broadcast`), `Member`, `SessionKind`, `in_process`, `GatewayEnd`, `RegionEnd`, `Router` (`open`, `deliver`), `network::region_end`, `network::gateway_end`.

# Invariants

- **Each direction is one ordered stream.** `Ready` comes first.
- **A broadcast reaches the sessions joined when it was sent.** The region sends `Joined` before anything addressed to the session. The router counts a session among the broadcast's receivers only from `Joined` to `Left`. So a session that joins late is told about each entity once, by its snapshot, and never by a broadcast from before it joined.
- **A route closes when the region lets the session go.** Messages for a session that has left are dropped.
- **What is not a message is refused.** Decoding an empty message, garbage or an unknown session kind is an error, not a default. Over a connection, such a frame, or one larger than 64 MiB, ends the link rather than being skipped.

The crate's tests check each of these. The region's tests check that a late session sees each entity once. An end-to-end test of the server runs the region on the other end of a TCP connection and plays through the gateway with real clients.

# Knowledge

- Decision: D-037 in `design/decisions.md`; the server's architecture in `design/adr/0001-architecture.md`.
- Oracle: the crate's tests (`cargo test -p osage-region-link`), the region's, and the server's end-to-end test with the region apart (`cargo test -p osage-server --test region_elsewhere`).

# Reuse

The crate depends only on the protocol's types and on channels. It does not depend on Bevy: a gateway can be built without the engine. The pattern is not tied to osage: a stream of addressed messages in each direction, and membership announced in the stream itself, replaces shared connection handles between any front end and its simulation.

# Limits

One gateway talks to one region, and a region serves one gateway connection: if it ends, the region does not wait for another. The link is not authenticated or encrypted (trust by default, D-038): it belongs on a private network. A broadcast goes to every member: interest management (which session sees what) comes with the second region, and will address messages to a list of sessions.
