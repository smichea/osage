// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The component types this world announces in its Welcome (D-019).

use osage_proto::{
    DESCRIPTORS, types,
    v0::{ComponentType, SchemaBundle},
};
use prost::Message;

pub const TRANSFORM: u32 = 1;
pub const CONTENT_REF: u32 = 2;
pub const CONTROLLER: u32 = 3;
pub const MOVE_INTENT: u32 = 4;
pub const DEVICE: u32 = 5;
pub const MEASUREMENTS: u32 = 6;

pub fn bundle() -> SchemaBundle {
    let component_type = |id, name: &str, message_type: &str, volatile| ComponentType {
        id,
        name: name.to_owned(),
        message_type: message_type.to_owned(),
        volatile,
    };
    SchemaBundle {
        descriptors: Some(prost_types::FileDescriptorSet::decode(DESCRIPTORS).expect("embedded descriptors")),
        types: vec![
            component_type(TRANSFORM, types::TRANSFORM, types::TRANSFORM_MESSAGE, true),
            component_type(CONTENT_REF, types::CONTENT_REF, types::CONTENT_REF_MESSAGE, false),
            component_type(CONTROLLER, types::CONTROLLER, types::CONTROLLER_MESSAGE, false),
            component_type(MOVE_INTENT, types::MOVE_INTENT, types::MOVE_INTENT_MESSAGE, false),
            component_type(DEVICE, types::DEVICE, types::DEVICE_MESSAGE, false),
            component_type(MEASUREMENTS, types::MEASUREMENTS, types::MEASUREMENTS_MESSAGE, false),
        ],
        url: String::new(),
        sha256: Vec::new(),
    }
}
