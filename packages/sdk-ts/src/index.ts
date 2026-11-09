// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

/** TypeScript client SDK for the osage protocol (spec/README.md, ADR-0002). */

export { DELEGATE_AGENT, PROTOCOL_VERSION, Session, connect, hello, withDelegation, withIdentity } from "./session.js";
export { World, type WorldEvent, types } from "./world.js";
export * as components from "./gen/osage/components/v1/components_pb.js";
export * as math from "./gen/osage/math/v1/math_pb.js";
export * as v0 from "./gen/osage/v0/protocol_pb.js";
