// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The component types a world announces in its Welcome (D-019): the core
//! ones, then the world module's own.

use std::collections::{HashMap, HashSet};

use bevy::prelude::Resource;
use osage_proto::{
    DESCRIPTORS, types,
    v0::{ComponentType, SchemaBundle},
};
use prost::Message;
use prost_types::FileDescriptorSet;

use crate::{TypeSpec, WorldModule};

pub const TRANSFORM: u32 = 1;
pub const CONTENT_REF: u32 = 2;
pub const CONTROLLER: u32 = 3;
pub const MOVE_INTENT: u32 = 4;
pub const DEVICE: u32 = 5;
pub const MEASUREMENTS: u32 = 6;
pub const DEVICE_SET: u32 = 7;

/// The world's component types and their schemas; worlds look ids up by name.
#[derive(Resource, Clone, Debug)]
pub struct Schema {
    bundle: SchemaBundle,
    ids: HashMap<String, u32>,
}

impl Schema {
    pub fn new(world: &impl WorldModule) -> Self {
        let core = [
            (
                TRANSFORM,
                types::TRANSFORM,
                types::TRANSFORM_MESSAGE,
                true,
                "Position, rotation and scale (metres, Y up).",
            ),
            (CONTENT_REF, types::CONTENT_REF, types::CONTENT_REF_MESSAGE, false, "What the entity looks like."),
            (
                CONTROLLER,
                types::CONTROLLER,
                types::CONTROLLER_MESSAGE,
                false,
                "Who drives the entity: human, agent or system.",
            ),
            (
                MOVE_INTENT,
                types::MOVE_INTENT,
                types::MOVE_INTENT_MESSAGE,
                false,
                "Intent: walk your avatar in a direction; it stops 0.5 s after the last one.",
            ),
            (DEVICE, types::DEVICE, types::DEVICE_MESSAGE, false, "A real device mirrored by its owner's bridge."),
            (MEASUREMENTS, types::MEASUREMENTS, types::MEASUREMENTS_MESSAGE, false, "A device's latest readings."),
            (DEVICE_SET, types::DEVICE_SET, types::DEVICE_SET_MESSAGE, false, "Arguments of osage.device.set."),
        ];
        let mut types: Vec<ComponentType> = core
            .into_iter()
            .map(|(id, name, message_type, volatile, description)| ComponentType {
                id,
                name: name.to_owned(),
                message_type: message_type.to_owned(),
                volatile,
                description: description.to_owned(),
            })
            .collect();
        for (offset, TypeSpec { name, message_type, volatile, description }) in
            world.component_types().into_iter().enumerate()
        {
            assert!(types.iter().all(|t| t.name != name), "component type {name} declared twice");
            let id = DEVICE_SET + 1 + offset as u32;
            types.push(ComponentType { id, name, message_type, volatile, description });
        }

        let mut descriptors = FileDescriptorSet::decode(DESCRIPTORS).expect("embedded descriptors");
        let world_files = FileDescriptorSet::decode(world.descriptors().as_slice()).expect("the world's descriptors");
        let known: HashSet<_> = descriptors.file.iter().map(|f| f.name.clone()).collect();
        descriptors.file.extend(world_files.file.into_iter().filter(|f| !known.contains(&f.name)));

        let ids = types.iter().map(|t| (t.name.clone(), t.id)).collect();
        let bundle = SchemaBundle { descriptors: Some(descriptors), types, url: String::new(), sha256: Vec::new() };
        Self { bundle, ids }
    }

    /// The compact id of component type `name` (e.g. `osage.transform/1`).
    pub fn id(&self, name: &str) -> Option<u32> {
        self.ids.get(name).copied()
    }

    /// The schema bundle sent in the Welcome.
    pub fn bundle(&self) -> SchemaBundle {
        self.bundle.clone()
    }
}
