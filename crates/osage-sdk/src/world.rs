// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{BTreeMap, HashMap};

use osage_proto::{
    components, types,
    v0::{ComponentType, ServerMessage, Welcome, server_message},
};

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
    /// Tick of the last state applied to each (entity, type): datagrams may
    /// arrive out of order, and an older value never replaces a newer one.
    ticks: HashMap<(u64, u32), u64>,
}

impl World {
    pub fn new(welcome: &Welcome) -> Self {
        Self { types: TypeTable::from_welcome(welcome), ..Self::default() }
    }

    pub fn types(&self) -> &TypeTable {
        &self.types
    }

    /// Latest server tick seen in a `StateDatagram`.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn entities(&self) -> impl Iterator<Item = u64> + '_ {
        self.entities.keys().copied()
    }

    /// Encoded component values of `entity`, by type name.
    pub fn components(&self, entity: u64) -> impl Iterator<Item = (&str, &[u8])> + '_ {
        self.entities
            .get(&entity)
            .into_iter()
            .flat_map(|c| c.iter().map(|(name, value)| (name.as_str(), value.as_slice())))
    }

    /// Decodes the value of component `type_name` (e.g. `osage.transform/1`)
    /// on `entity`, if present and well-formed.
    pub fn get<M: prost::Message + Default>(&self, entity: u64, type_name: &str) -> Option<M> {
        let bytes = self.entities.get(&entity)?.get(type_name)?;
        M::decode(bytes.as_slice()).ok()
    }

    /// The entity whose `osage.controller/1` names `per_world_id` — for a
    /// player, its avatar (pass `Welcome.identity.per_world_id`).
    pub fn controlled_by(&self, per_world_id: &str) -> Option<u64> {
        self.entities().find(|&entity| {
            self.get::<components::Controller>(entity, types::CONTROLLER)
                .is_some_and(|controller| controller.per_world_id == per_world_id)
        })
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
                self.ticks.retain(|(entity, _), _| *entity != leave.entity);
                if self.entities.remove(&leave.entity).is_some() {
                    events.push(WorldEvent::Left(leave.entity));
                }
            }
            Some(server_message::Message::ComponentChanged(changed)) => {
                if let Some(component) = &changed.component {
                    self.set(changed.entity, component.r#type, &component.value, changed.removed, &mut events);
                }
            }
            Some(server_message::Message::State(state)) => {
                self.tick = self.tick.max(state.tick);
                for value in &state.values {
                    let Some(component) = &value.component else { continue };
                    if !self.entities.contains_key(&value.entity) {
                        continue;
                    }
                    let last = self.ticks.entry((value.entity, component.r#type)).or_default();
                    if state.tick >= *last {
                        *last = state.tick;
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
        events.push(WorldEvent::Changed { entity, type_name: component_type.name.clone() });
    }
}

#[cfg(test)]
mod tests {
    use osage_proto::{
        components::Transform,
        math::Vec3,
        v0::{Component, EntityComponent, EntityEnter, SchemaBundle, StateDatagram},
    };
    use prost::Message as _;

    use super::*;

    fn at(x: f32) -> Vec<u8> {
        Transform { translation: Some(Vec3 { x, y: 0.0, z: 0.0 }), ..Default::default() }.encode_to_vec()
    }

    fn state(tick: u64, entity: u64, x: f32) -> ServerMessage {
        let value = EntityComponent { entity, component: Some(Component { r#type: 1, value: at(x) }) };
        ServerMessage {
            message: Some(server_message::Message::State(StateDatagram {
                tick,
                values: vec![value],
                ..Default::default()
            })),
        }
    }

    #[test]
    fn a_late_datagram_never_replaces_a_newer_value() {
        let transform =
            ComponentType { id: 1, name: types::TRANSFORM.to_owned(), volatile: true, ..Default::default() };
        let welcome = Welcome {
            schemas: Some(SchemaBundle { types: vec![transform], ..Default::default() }),
            ..Default::default()
        };
        let mut world = World::new(&welcome);
        for entity in [1, 2] {
            let components = vec![Component { r#type: 1, value: at(0.0) }];
            world.apply(&ServerMessage {
                message: Some(server_message::Message::EntityEnter(EntityEnter { entity, components })),
            });
        }
        let x =
            |world: &World, entity| world.get::<Transform>(entity, types::TRANSFORM).unwrap().translation.unwrap().x;

        world.apply(&state(10, 1, 10.0));
        world.apply(&state(9, 1, 9.0)); // late
        world.apply(&state(9, 2, 9.0)); // late, but entity 2 has nothing newer
        assert_eq!((x(&world, 1), x(&world, 2), world.tick()), (10.0, 9.0, 10));
    }
}
