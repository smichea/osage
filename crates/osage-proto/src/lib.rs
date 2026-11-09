// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Rust types of the osage protocol, generated from `spec/proto`.

#[allow(clippy::all)]
pub mod osage {
    pub mod v0 {
        include!(concat!(env!("OUT_DIR"), "/osage.v0.rs"));
    }
    pub mod math {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/osage.math.v1.rs"));
        }
    }
    pub mod components {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/osage.components.v1.rs"));
        }
    }
}

pub use osage::{components::v1 as components, math::v1 as math, v0};

/// Protocol version spoken by this crate.
pub const PROTOCOL_VERSION: &str = "0.1";

/// Encoded `FileDescriptorSet` of every schema in `spec/proto`.
pub const DESCRIPTORS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/osage_descriptors.bin"));

/// Names of core actions.
pub mod actions {
    /// A human asks the world for a delegation for one agent; the token is in
    /// `ActionResult.result` (spec/README.md, "Credentials").
    pub const DELEGATE_AGENT: &str = "osage.delegate_agent";
    /// A device owner asks the world for a delegation for one bridge.
    pub const DELEGATE_BRIDGE: &str = "osage.delegate_bridge";
}

/// Names of the core component types and their message types.
pub mod types {
    pub const TRANSFORM: &str = "osage.transform/1";
    pub const TRANSFORM_MESSAGE: &str = "osage.components.v1.Transform";
    pub const CONTENT_REF: &str = "osage.content_ref/1";
    pub const CONTENT_REF_MESSAGE: &str = "osage.components.v1.ContentRef";
    pub const CONTROLLER: &str = "osage.controller/1";
    pub const CONTROLLER_MESSAGE: &str = "osage.components.v1.Controller";
    pub const DEVICE: &str = "osage.device/1";
    pub const DEVICE_MESSAGE: &str = "osage.components.v1.Device";
    /// Provisional, to be replaced by OpenDeviceDB's contract (opendevicedb#33).
    pub const MEASUREMENTS: &str = "osage.measurements/1";
    pub const MEASUREMENTS_MESSAGE: &str = "osage.components.v1.Measurements";
    pub const MOVE_INTENT: &str = "osage.move_intent/1";
    pub const MOVE_INTENT_MESSAGE: &str = "osage.components.v1.MoveIntent";
    /// Built-in unit cube, drawn by every client.
    pub const BUILTIN_CUBE: &str = "osage:builtin/cube";
    /// Built-in avatar shape (a capsule about 1.8 m tall), drawn by every client.
    pub const BUILTIN_AVATAR: &str = "osage:builtin/avatar";
    /// Built-in marker of a real device, drawn by every client.
    pub const BUILTIN_DEVICE: &str = "osage:builtin/device";
}
