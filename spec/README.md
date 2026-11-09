# osage protocol specification

Status: **v0, draft** — grows with the phase-1 vertical slice. Design and reasons: [ADR-0002](../design/adr/0002-protocol-v0.md).

| Part | Path | Licence |
|---|---|---|
| Specification text (this folder's Markdown) | `spec/*.md` | [Community Specification License 1.0](../LICENSES/Community-Spec-1.0.txt) |
| Schemas | [`proto/osage/v0/protocol.proto`](proto/osage/v0/protocol.proto), [`proto/osage/math/v1/math.proto`](proto/osage/math/v1/math.proto), [`proto/osage/components/v1/components.proto`](proto/osage/components/v1/components.proto) | Apache-2.0 |

## v0 in one page

- **Transport:** WebSocket today (one binary message = one `ClientMessage` or `ServerMessage`); WebTransport next, with length-prefixed frames on streams and datagrams for `StateDatagram`.
- **Session:** the client sends `Hello` (versions, client info, role, capabilities, credentials, world); the gateway answers `Welcome` (chosen version, session id, identity, server time, world descriptor with `source_offer`, schema bundle).
- **Component types** are announced in the `Welcome` schema bundle: a `FileDescriptorSet` plus a table mapping compact ids to names such as `osage.transform/1` and message types such as `osage.components.v1.Transform`. A `Component` on the wire is that id plus the encoded message.
- **Replication:** `EntityEnter` (entity + all its components), `EntityLeave`, `ComponentChanged` (reliable, discrete types), `StateDatagram` (volatile types, latest value wins).
- **Client input:** `Intent` for controlled entities, `ActionRequest` → `ActionResult` (never replayed automatically; may be `PENDING_APPROVAL`).
- **Core component types (v1):** `osage.transform/1` (volatile), `osage.content_ref/1` (built-in shapes such as `osage:builtin/cube`, glTF, 3D Tiles, splats), `osage.controller/1` (human, agent or system).
- **Version string:** `"0.1"`.
