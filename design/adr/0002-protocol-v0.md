# ADR-0002 — osage protocol v0

Status: **accepted** for the choices below (D-016 to D-019); the message details are a **draft** to be refined while the phase-1 vertical slice is built · Date: 2026-11-09

The osage protocol is the public contract between any client and an osage world ([ADR-0001](0001-architecture.md)). Its specification will live in `spec/` under Apache-2.0 (D-003). This ADR fixes its shape; the normative text and `.proto` files come with the first code.

## 1. Decisions

| # | Decision | Why |
|---|---|---|
| D-016 | Clients connect to a **replication gateway**, never directly to region servers | region boundaries and handoffs invisible to clients; one place for identity, agents, rate limits |
| D-017 | **osage specifies its own protocol**; the server takes ideas from bevy_replicon and lightyear (visibility, deltas, interpolation) but not their wire format | language-independent spec for Rust and TypeScript SDKs; their formats are Rust-only and change with each release |
| D-018 | Messages are **Protocol Buffers (proto3)**; the **canonical JSON mapping** is used by the agent gateway and debugging tools | neutral schemas, generated code for Rust (prost) and TypeScript (protobuf-es), proven evolution rules, one schema in binary and JSON |
| D-019 | Worlds are **self-describing**: at join, the world sends the schemas of its component types | generic clients and agents can show and use components they were not compiled with; worlds add types without client updates |

## 2. Terms

**World** (one logical space, one operator) · **region** (area simulated by one region server) · **gateway** · **session** (one connected client) · **entity** · **component type** · **controller kind** (`human`, `agent`, `system`) · **role** (`player`, `agent`, `bridge`, `observer`) · **tier** (T0 headless … T4 ray tracing).

## 3. Transport

- **WebTransport over HTTP/3** for native and browser clients (one endpoint, TLS mandatory); **WebSocket** fallback where WebTransport is unavailable.
- Channels:
  - a **control stream** (bidirectional, reliable, ordered): handshake, actions, subscriptions, errors;
  - **reliable streams** for entity events (enter, leave, discrete component changes);
  - **datagrams** (unreliable) for volatile, high-frequency components (transforms); over WebSocket they travel on the reliable channel, latest value wins.
- Framing: one length-prefixed protobuf message per frame on streams; one message per datagram.

## 4. Session lifecycle

1. **`Hello`** (client → gateway): supported protocol versions; client implementation and version; role; capabilities (tier, renderer features, maximum datagram size, supported content kinds); credentials — a verifiable presentation (identity, D-013), a delegation credential (agent), or a bridge credential (device owner); requested world.
2. **`Welcome`** (gateway → client): chosen version; session id; per-world identifier, assurance level and privileges; server time; **world descriptor** (tick rate, content service URL, optional Earth frame); **schema bundle** — the world's component types as a protobuf `FileDescriptorSet` with their type ids, sent inline or as a hash + URL for caching (D-019); the session's compact numeric ids for those types.
3. **Steady state** (§5–§7).
4. **`Goodbye`** with a reason code (normal, kicked, sanctioned, version, overload, server shutdown).

## 5. Entities and components

- **Entity id:** 64-bit, unique in the world and **stable across region handoffs**.
- **Component type id:** a namespaced string with a major version, mapped to a compact numeric id per session:
  - `osage.*` — core types (`osage.transform/1`, `osage.controller/1`, `osage.content_ref/1`, `osage.avatar/1`…);
  - `odb.*` — device contracts from OpenDeviceDB (`odb.measurement/1`, `odb.video.frame/1`…; D-011);
  - `world.<namespace>.*` — types defined by a world.
- Each type's schema declares whether it is **volatile** (datagrams, latest value wins) or **discrete** (reliable).
- **`osage.controller/1`** is carried by every participant-controlled entity: controller kind, and for agents the per-world identifier of the responsible human (P5 of [design/01](../01-accounts-and-agents.md)).
- **`osage.content_ref/1`**: content by reference — URI, content hash, kind (`gltf`, `3dtiles`, `splat`), available tiers. **No geometry in the protocol.**
- **Device components** carry source timestamp and clock domain, as the OpenDeviceDB contracts require ([design/02](../02-mixed-worlds-and-devices.md)).

## 6. Server → client

| Message | Channel | Content |
|---|---|---|
| `EntityEnter` | reliable | entity id and its current components, when it enters the client's scope |
| `EntityLeave` | reliable | entity id and reason (out of scope, destroyed, permission) |
| `ComponentChanged` | reliable | discrete component: full new value (v0 has no field-level delta) |
| `StateDatagram` | datagram | server tick and a batch of volatile component values |
| `TimeSync` | control | server time, for clock offset and latency estimation |
| `ActionResult` | control | see §7 |

**Interest management is server-side:** spatial cells per region, explicit subscriptions, owner permissions (device and camera streams, D-010). The gateway merges the scopes of the regions a client's area touches. A client never decides what it is allowed to see.

## 7. Client → server

| Message | Channel | Content |
|---|---|---|
| `Intent` | datagram or reliable | sequence number + input for the entity the client controls (movement, look); the server simulates |
| `ActionRequest` | control | request id, target entity, action type, arguments |
| `Subscribe` / `Unsubscribe` | control | explicit subscription (agents, inspectors, tools), filtered by permissions |

`ActionResult` returns `ok`, `rejected` (reason), `failed`, `unknown` (to reconcile, never retried blindly), or **`pending_approval`** with an approval reference — for gated actions and physical actuation (design/01 P6, D-009). **Actions are never replayed automatically** after a reconnection; the request id makes them idempotent.

## 8. Time, georeference

- All protocol timestamps are server time in microseconds; device components also carry their source timestamp and clock domain.
- The world descriptor may declare an **Earth frame** (WGS84 origin and orientation); poses in it map to OGC GeoPose. Purely virtual worlds have none.

## 9. Versioning

- Protocol version negotiated in `Hello` / `Welcome`. **v0.x may break** between minor versions; stability starts at v1.
- Component types: additive changes only within a major version; a breaking change is a new major (`/2`), and worlds may serve both during a transition.
- Unknown fields are preserved and ignored (protobuf rules).

## 10. Security

TLS everywhere (inherent to WebTransport; required for WebSocket); credentials verified at the gateway; per-session rate limits and message size limits; a client's intents never write state directly. Signing schema bundles with the world operator's key is left for v1.

## 11. Conformance

The spec ships **test vectors and a conformance suite** that every SDK (Rust, TypeScript, later others) and every server implementation must pass; this is what keeps several client implementations consistent (D-002).

## 12. Open for v0 iterations

Field-level delta compression and acknowledged baselines; client-side prediction and reconciliation semantics; compression (zstd) of large messages; the internal gateway ↔ region protocol and handoff mechanics; caching and signing of schema bundles; message catalogue for the agent gateway's MCP tools.
