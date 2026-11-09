// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Rust client SDK for the osage protocol (spec/README.md, ADR-0002).
//!
//! [`connect`] opens a session (WebSocket for now), sends the `Hello` and
//! waits for the `Welcome`; [`World`] applies replication messages and decodes
//! component values with the types announced in the `Welcome`.

mod session;
mod world;

pub use osage_proto::{PROTOCOL_VERSION, components, math, types, v0};
pub use session::{Session, SessionReceiver, SessionSender, connect, hello, intent, with_delegation, with_identity};
pub use world::{TypeTable, World, WorldEvent};
