// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! World Module Interface, phase 1 (D-030, LICENSE-EXCEPTIONS §C): what a
//! world's rules see of the region. These bindings are permissive (MIT OR
//! Apache-2.0): a world module that uses only them, Bevy and its own code may
//! carry the licence of its authors' choice. A world is a Bevy plugin plus the
//! component types it adds to the core ones; the region replicates what it
//! spawns, routes intents to it and tells it who comes and goes. The engine
//! provides these mechanisms; the game is the world's.
//!
//! A world spawns entities with [`Replicated`] (an id from [`EntityIds`]), a
//! `Transform`, and its component values in [`Discrete`] (reliable) and
//! [`Volatile`] (latest value wins, sent as datagrams). An entity with an
//! [`Owner`] receives that session's intents and leaves with it. World
//! systems go in [`RegionSet::World`].

use std::collections::BTreeMap;

pub mod schema;

use std::time::SystemTime;

pub use bevy;
use bevy::prelude::*;
pub use osage_proto;
use osage_proto::{components, v0::Role};
pub use schema::Schema;

/// What kind of participant a session is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionKind {
    Human,
    Agent,
    Bridge,
}

/// Microseconds since the Unix epoch on the server's clock: the time clients
/// receive in state datagrams and `TimeSync`.
pub fn server_time_us() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_micros() as u64)
}

/// A world's rules: a Bevy plugin plus the component types it adds.
pub trait WorldModule: Plugin {
    /// Component types the world adds to the core ones; their ids follow the
    /// core ids, in this order.
    fn component_types(&self) -> Vec<TypeSpec> {
        Vec::new()
    }

    /// Encoded `FileDescriptorSet` with the messages of those types (files the
    /// core already has are ignored).
    fn descriptors(&self) -> Vec<u8> {
        Vec::new()
    }
}

/// A component type announced in the Welcome.
pub struct TypeSpec {
    /// Namespaced and versioned, e.g. `kart.state/1`.
    pub name: String,
    /// Fully qualified protobuf message, e.g. `kart.v1.KartState`.
    pub message_type: String,
    pub volatile: bool,
    /// What it means, and for an intent what sending it does: read by people
    /// and AI agents.
    pub description: String,
}

/// Phases of a region tick: the region handles its inputs, then the world's
/// systems run, then the region replicates what changed.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum RegionSet {
    Receive,
    World,
    Replicate,
}

/// World-unique id of a replicated entity, stable across region handoffs
/// (ADR-0002 §5); take it from [`EntityIds`].
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Replicated(pub u64);

/// Hands out entity ids.
#[derive(Resource, Default)]
pub struct EntityIds(u64);

impl EntityIds {
    pub fn next_id(&mut self) -> Replicated {
        self.0 += 1;
        Replicated(self.0)
    }
}

/// Reliable components of an entity, encoded, by type id: clients receive
/// them on entering and every change after.
#[derive(Component, Clone, Debug, Default)]
pub struct Discrete(pub BTreeMap<u32, Vec<u8>>);

/// Volatile components besides the transform, encoded, by type id: sent in
/// state datagrams when they change, latest value wins.
#[derive(Component, Clone, Debug, Default)]
pub struct Volatile(pub BTreeMap<u32, Vec<u8>>);

macro_rules! encoded_map {
    ($type:ident) => {
        impl $type {
            pub fn with(mut self, type_id: u32, value: &impl prost::Message) -> Self {
                self.0.insert(type_id, value.encode_to_vec());
                self
            }

            pub fn set(&mut self, type_id: u32, value: &impl prost::Message) {
                self.0.insert(type_id, value.encode_to_vec());
            }

            pub fn get<M: prost::Message + Default>(&self, type_id: u32) -> Option<M> {
                M::decode(self.0.get(&type_id)?.as_slice()).ok()
            }
        }
    };
}
encoded_map!(Discrete);
encoded_map!(Volatile);

/// The session whose intents drive this entity; the entity leaves with it.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Owner(pub u64);

/// The current region tick, as stamped on state datagrams.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct Tick(pub u64);

/// A session joined the world.
#[derive(Message, Clone, Debug)]
pub struct Joined {
    pub session: u64,
    pub kind: SessionKind,
    pub role: Role,
    /// Its `osage.controller/1`: kind and per-world identifier.
    pub controller: components::Controller,
}

/// A session left; the entities it owned are gone.
#[derive(Message, Clone, Copy, Debug)]
pub struct Left {
    pub session: u64,
}

/// An intent from a session, for an entity it owns.
#[derive(Message, Clone, Debug)]
pub struct IntentReceived {
    pub session: u64,
    pub entity: Entity,
    pub sequence: u64,
    pub type_id: u32,
    pub value: Vec<u8>,
}

/// The core encoding of a Bevy transform.
pub fn transform_to_proto(transform: &Transform) -> components::Transform {
    let vec3 = |v: Vec3| osage_proto::math::Vec3 { x: v.x, y: v.y, z: v.z };
    let q = transform.rotation;
    components::Transform {
        translation: Some(vec3(transform.translation)),
        rotation: Some(osage_proto::math::Quat { x: q.x, y: q.y, z: q.z, w: q.w }),
        scale: Some(vec3(transform.scale)),
    }
}

pub fn transform_from_proto(t: &components::Transform) -> Transform {
    let vec3 = |v: Option<osage_proto::math::Vec3>, default: Vec3| v.map_or(default, |v| Vec3::new(v.x, v.y, v.z));
    Transform {
        translation: vec3(t.translation, Vec3::ZERO),
        rotation: t.rotation.map_or(Quat::IDENTITY, |q| Quat::from_xyzw(q.x, q.y, q.z, q.w)),
        scale: vec3(t.scale, Vec3::ONE),
    }
}

/// A built-in shape every client draws (`osage:builtin/…`).
pub fn builtin(uri: &str) -> components::ContentRef {
    components::ContentRef {
        uri: uri.to_owned(),
        kind: components::ContentKind::Builtin.into(),
        tiers: vec![osage_proto::v0::Tier::T0Headless.into()],
        ..default()
    }
}
