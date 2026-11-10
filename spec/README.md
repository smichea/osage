# osage protocol specification

Status: **v0, draft** — grows with the phase-1 vertical slice. Design and reasons: [ADR-0002](../design/adr/0002-protocol-v0.md).

| Part | Path | Licence |
|---|---|---|
| Specification text (this folder's Markdown) | `spec/*.md` | [Community Specification License 1.0](../LICENSES/Community-Spec-1.0.txt) |
| Schemas | [`proto/osage/v0/protocol.proto`](proto/osage/v0/protocol.proto), [`proto/osage/math/v1/math.proto`](proto/osage/math/v1/math.proto), [`proto/osage/components/v1/components.proto`](proto/osage/components/v1/components.proto) | Apache-2.0 |

## v0 in one page

- **Transport:** WebTransport over HTTP/3, with WebSocket as the fallback ([D-028](../design/decisions.md#d-028--webtransport-in-v0-one-reliable-stream-datagrams-discovery-on-the-websocket-host)).
  - **WebSocket:** one binary message = one `ClientMessage` or `ServerMessage`; everything is reliable.
  - **WebTransport:** the client opens one bidirectional stream, which carries every reliable message in both directions, each preceded by its length as a protobuf varint (protobuf's length-delimited format; frames longer than 4 MiB end the connection). `StateDatagram`s travel as datagrams — one `ServerMessage` per datagram, a tick's values cut into several datagrams of the same tick when needed — and the client's `Intent`s may too; any other message in a datagram is dropped.
  - **Discovery:** the WebSocket host answers `GET /.well-known/osage/webtransport` with `{"url": "https://host:port/", "certificateHashes": ["<hex SHA-256>", …]}` (404: WebSocket only). The hashes are present only for a self-signed development certificate (valid 14 days, ECDSA P-256), which browsers then accept through `serverCertificateHashes`; a world with a web-PKI certificate announces none.
  - **Losses and order:** datagrams may be lost or reordered. A client keeps, per entity and component type, the tick of the last value applied and ignores older ones; every second (one tick-rate's worth of ticks), a `StateDatagram` carries every volatile value, so a lost value is repaired within a second. The server keeps the latest intent sequence.
  - **Liveness:** a WebTransport peer silent for 10 s has left; both sides send keep-alives every 3 s.
- **Session:** the client sends `Hello` (versions, client info, role, capabilities, credentials, world); the gateway answers `Welcome` (chosen version, session id, identity, server time, world descriptor with `source_offer`, schema bundle).
- **Component types** are announced in the `Welcome` schema bundle: a `FileDescriptorSet` plus a table mapping compact ids to names such as `osage.transform/1` and message types such as `osage.components.v1.Transform`. A `Component` on the wire is that id plus the encoded message. Each type carries a `description` — for an intent, what sending it does — so that people and AI agents can act in a world they have never seen. The core types come first; a world module's own types (D-030) follow.
- **Replication:** `EntityEnter` (entity + all its components), `EntityLeave`, `ComponentChanged` (reliable, discrete types), `StateDatagram` (volatile types, latest value wins).
- **Client input:** `Intent` for controlled entities, `ActionRequest` → `ActionResult` (never replayed automatically; may be `PENDING_APPROVAL`).
- **Core component types (v1):** `osage.transform/1` (volatile), `osage.content_ref/1` (built-in shapes `osage:builtin/cube` and `osage:builtin/avatar`, glTF, 3D Tiles, splats), `osage.controller/1` (human, agent or system).
- **Avatars:** a `player` session gets an avatar entity whose `osage.controller/1` carries the session's `per_world_id`; the client moves it with `Intent` messages of type `osage.move_intent/1` (a direction; the server clamps it and stops the avatar when intents stop arriving for 0.5 s). Other sessions see it enter, move and leave.
- **Version string:** `"0.1"`.

## Credentials (v0)

**Identity** (`Credentials.verifiable_presentation`, D-013): a W3C Verifiable Credential 2.0 secured as a compact JWS with ES256 (`typ: vc+jwt`, `kid` naming the issuer's key). Required claims: `iss` (an issuer the world trusts), `aud` (the world name), `exp` (at most one hour after `iat`), `sub` and `credentialSubject.id` (the **per-world identifier** — a pairwise pseudonym, never a civil identity), `credentialSubject.assuranceLevel` (`A1`, `A2` or `A3`). Each world lists the issuers and public keys it trusts.

*Known limit of v0:* the credential is a short-lived bearer token bound to one world; holder binding (a presentation signed with the holder's key, OpenID4VP) comes next.

**Agents** (`Credentials.delegation`, design/01 §3): a human session whose privileges include `run_agents` sends `ActionRequest { action: "osage.delegate_agent" }`; the `ActionResult.result` is a delegation token the world signed, naming the responsible human and an expiry. The agent joins with role `agent` and that token. Its entities carry `osage.controller/1` with kind `agent` and the responsible human's `per_world_id`. The world limits agents per responsible human.

**Gateway refusals:** `UNAUTHENTICATED` (credential invalid, expired, wrong world or untrusted issuer), `DUPLICATE_SESSION` (the identity is already connected), `SANCTIONED` (the identity, or an agent's responsible human, is sanctioned in this world), `QUOTA` (e.g. too many agents for one human).

**Bridges** (`Credentials.bridge`, design/02): a human with an account (A1) sends `ActionRequest { action: "osage.delegate_bridge" }` and gives the token to a bridge, which joins with role `bridge`. Only bridges send `DeviceState`; the world accepts only `osage.device/1`, `osage.measurements/1` (provisional, opendevicedb#33), `osage.transform/1` and `osage.content_ref/1` on devices, records the **authenticated** owner and bridge in `osage.device/1` whatever the bridge claims, and removes the devices when their bridge leaves. Every function is `READ_ONLY` in phase 1 (D-009).

**Acting on devices** (design/02 M2–M4, D-009): a participant sends `ActionRequest { action: "osage.device.set", target: <device entity>, arguments: osage.components.v1.DeviceSet { function, value } }`. The world reads the function's risk class in the device's `osage.device/1`: `PRE_APPROVED` → relayed to the bridge as a `DeviceCommand`; `GATED` → `PENDING_APPROVAL` (approval receipts are not in v0); `READ_ONLY`, `FORBIDDEN`, `ON_SITE_OPERATOR` or unknown → `REJECTED`. The bridge answers with `DeviceCommandResult` after checking **locally** its own declared class and the allowed range of values (it may still refuse); the world relays it as the `ActionResult`. No answer within 5 s, or the bridge leaving, gives `UNKNOWN` — never retried automatically. When its world link ends, a bridge puts every device in its safe state.

**Privileges by level** (default policy of D-006, changeable per world): A0 `visit`; A1 adds `persist`; A2 adds `trade`, `own`, `ranked`, `run_agents`; A3 adds `withdraw`, `moderate`, `vouch`.
