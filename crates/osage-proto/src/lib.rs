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

/// Names of the core component types and their message types.
pub mod types {
    pub const TRANSFORM: &str = "osage.transform/1";
    pub const TRANSFORM_MESSAGE: &str = "osage.components.v1.Transform";
    pub const CONTENT_REF: &str = "osage.content_ref/1";
    pub const CONTENT_REF_MESSAGE: &str = "osage.components.v1.ContentRef";
    pub const CONTROLLER: &str = "osage.controller/1";
    pub const CONTROLLER_MESSAGE: &str = "osage.components.v1.Controller";
    /// Built-in unit cube, drawn by every client.
    pub const BUILTIN_CUBE: &str = "osage:builtin/cube";
}
