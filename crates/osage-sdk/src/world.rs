// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{BTreeMap, HashMap};

use osage_proto::v0::{ComponentType, ServerMessage, Welcome, server_message};

/// Component types announced in a `Welcome`, by compact id and by name.
#[derive(Clone, Debug, Default)]
pub struct TypeTable {
    by_id: HashMap<u32, ComponentType>,
    by_name: HashMap<String, u32>,
}

impl TypeTable {
    pub fn from_welcome(welcome: &Welcome) -> Self {
        let mut table = Self::default();
        for component_type in welcome.schemas.iter().flat_map(|schemas| &schemas.types) {
            table.by_name.insert(component_type.name.clone(), component_type.id);
            table.by_id.insert(component_type.id, component_type.clone());
        }
        table
    }

    pub fn get(&self, id: u32) -> Option<&ComponentType> {
        self.by_id.get(&id)
    }

    pub fn id(&self, name: &str) -> Option<u32> {
        self.by_name.get(name).copied()
    }
}

/// What a replication message changed in the [`World`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorldEvent {
    Entered(u64),
    Left(u64),
    Changed { entity: u64, type_name: String },
}

/// The client's view of the world: entities in scope and their encoded
/// component values, keyed by type name.
#[derive(Clone, Debug, Default)]
pub struct World {
    types: TypeTable,
    entities: BTreeMap<u64, HashMap<String, Vec<u8>>>,
    tick: u64,
}

impl World {
    pub fn new(welcome: &Welcome) -> Self {
        Self {
            types: TypeTable::from_welcome(welcome),
            ..Self::default()
        }
    }

    pub fn types(&self) -> &TypeTable {
        &self.types
    }

    /// Last server tick seen in a `StateDatagram`.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn entities(&self) -> impl Iterator<Item = u64> + '_ {
        self.entities.keys().copied()
    }

    /// Decodes the value of component `type_name` (e.g. `osage.transform/1`)
    /// on `entity`, if present and well-formed.
    pub fn get<M: prost::Message + Default>(&self, entity: u64, type_name: &str) -> Option<M> {
        let bytes = self.entities.get(&entity)?.get(type_name)?;
        M::decode(bytes.as_slice()).ok()
    }

    /// Applies one server message; messages that do not change the world
    /// (Welcome, TimeSync, ActionResult…) return no event.
    pub fn apply(&mut self, message: &ServerMessage) -> Vec<WorldEvent> {
        let mut events = Vec::new();
        match &message.message {
            Some(server_message::Message::EntityEnter(enter)) => {
                let components = enter
                    .components
                    .iter()
                    .filter_map(|c| Some((self.types.get(c.r#type)?.name.clone(), c.value.clone())))
                    .collect();
                self.entities.insert(enter.entity, components);
                events.push(WorldEvent::Entered(enter.entity));
            }
            Some(server_message::Message::EntityLeave(leave)) => {
                if self.entities.remove(&leave.entity).is_some() {
                    events.push(WorldEvent::Left(leave.entity));
                }
            }
            Some(server_message::Message::ComponentChanged(changed)) => {
                if let Some(component) = &changed.component {
                    self.set(
                        changed.entity,
                        component.r#type,
                        &component.value,
                        changed.removed,
                        &mut events,
                    );
                }
            }
            Some(server_message::Message::State(state)) => {
                self.tick = state.tick;
                for value in &state.values {
                    if let Some(component) = &value.component {
                        self.set(value.entity, component.r#type, &component.value, false, &mut events);
                    }
                }
            }
            _ => {}
        }
        events
    }

    fn set(&mut self, entity: u64, type_id: u32, value: &[u8], removed: bool, events: &mut Vec<WorldEvent>) {
        let (Some(component_type), Some(components)) = (self.types.get(type_id), self.entities.get_mut(&entity)) else {
            return;
        };
        if removed {
            components.remove(&component_type.name);
        } else {
            components.insert(component_type.name.clone(), value.to_vec());
        }
        events.push(WorldEvent::Changed {
            entity,
            type_name: component_type.name.clone(),
        });
    }
}
