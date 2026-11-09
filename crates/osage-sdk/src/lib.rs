// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Rust client SDK for the osage protocol (spec/README.md, ADR-0002).
//!
//! [`connect`] opens a session — over WebTransport when the gateway announces
//! it and it answers, WebSocket otherwise — sends the `Hello` and waits for
//! the `Welcome`; [`World`] applies replication messages and decodes
//! component values with the types announced in the `Welcome`.

mod session;
mod world;

pub use osage_proto::{
    PROTOCOL_VERSION,
    actions::{DELEGATE_AGENT, DELEGATE_BRIDGE, DEVICE_SET},
    components, math, types, v0,
};
pub use session::{
    Refused, Session, SessionReceiver, SessionSender, Transport, WebTransportEndpoint, connect, connect_websocket,
    connect_webtransport, device_command_result, device_set, device_state, discover, hello, intent, request_delegation,
    with_bridge, with_delegation, with_identity,
};
pub use world::{TypeTable, World, WorldEvent};
