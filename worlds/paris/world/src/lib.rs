// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Paris world: a walk in a street of Paris, Amazon Lumberyard's Bistro
//! (D-136). A world module (D-030): the engine replicates, the rules are
//! here.
//!
//! Walkers — people and AI agents — stroll on foot over the street's walk
//! grid ([`walk`]), told by `paris.stroll/1` intents where to go and where
//! to face; each client predicts its own with the same steps. Around each
//! walker, its client traces the 9 cubes of 3 m of its block; all else
//! reaches it as the light coming into that block ([`tiles`]), which a
//! server beside the world sends as the walker's place and speed ask —
//! [`Walkers`] tells it where each is.

pub mod tiles;
pub mod walk;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use bevy::prelude::*;
use osage_world_module_api::{
    Discrete, EntityIds, IntentReceived, Joined, Owner, RegionSet, Replicated, Schema, SessionKind, TypeSpec,
    WorldModule, builtin,
    osage_proto::{types as core, v0::Role},
};
use prost::Message as _;

/// Generated from `proto/paris/v1/paris.proto`.
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/paris.v1.rs"));
}

/// Encoded descriptor set of the Paris messages.
pub const DESCRIPTORS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/paris_descriptors.bin"));

/// Names of the Paris world's component types.
pub mod types {
    pub const STROLL: &str = "paris.stroll/1";
    pub const WALKER: &str = "paris.walker/1";
    pub const WORLD: &str = "paris.world/1";
}

/// Region ticks a second.
pub const TICK_HZ: u32 = 30;
/// A walker stops this long after its last intent.
const INTENT_LIFETIME: Duration = Duration::from_millis(500);

/// Where a walker's eyes are and how fast it goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    pub eyes: Vec3,
    pub speed: f32,
}

/// Every walker as last seen, by its replicated id: for the server that
/// sends them the light, on a thread of its own.
#[derive(Resource, Clone, Default)]
pub struct Walkers(pub Arc<Mutex<HashMap<u64, Seen>>>);

/// The Paris world on a walk grid.
pub struct ParisWorld {
    pub walk: Arc<walk::Walk>,
    pub walkers: Walkers,
    /// The port of the server of light beside the world, announced to
    /// clients (0: none), and how far ahead it sends, seconds.
    pub light_port: u16,
    pub horizon: f32,
}

#[derive(Resource)]
struct Grid(Arc<walk::Walk>);

#[derive(Resource, Clone, Copy)]
struct Ids {
    stroll: u32,
    walker: u32,
    content_ref: u32,
    controller: u32,
}

#[derive(Resource, Clone, Copy)]
struct Light {
    port: u16,
    horizon: f32,
}

/// A walker's latest stride, and when it came.
#[derive(Component, Default)]
struct Striding {
    stride: walk::Stride,
    sequence: u64,
    received: Duration,
    speed: f32,
}

impl Plugin for ParisWorld {
    fn build(&self, app: &mut App) {
        app.insert_resource(Grid(self.walk.clone()))
            .insert_resource(self.walkers.clone())
            .insert_resource(Light { port: self.light_port, horizon: self.horizon })
            .add_systems(Startup, ids)
            .add_systems(Update, (arrive, stroll, step, tell).chain().in_set(RegionSet::World));
    }
}

impl WorldModule for ParisWorld {
    fn component_types(&self) -> Vec<TypeSpec> {
        vec![
            TypeSpec {
                name: types::STROLL.to_owned(),
                message_type: "paris.v1.Stroll".to_owned(),
                volatile: false,
                description: "Intent for your walker: forward and right (-1..1) relative to where it faces, the \
                              heading it faces (radians about y, 0 facing -z), and whether it runs. It stops 0.5 s \
                              after the last one."
                    .to_owned(),
            },
            TypeSpec {
                name: types::WALKER.to_owned(),
                message_type: "paris.v1.Walker".to_owned(),
                volatile: false,
                description: "Who a walker is: a seed for its looks, whether it is a passer-by of the world."
                    .to_owned(),
            },
            TypeSpec {
                name: types::WORLD.to_owned(),
                message_type: "paris.v1.World".to_owned(),
                volatile: false,
                description: "The world: the port of its server of light (HTTP: /need?walker=ID, then /h/TILE/...), \
                              how far ahead it sends."
                    .to_owned(),
            },
        ]
    }

    fn descriptors(&self) -> Vec<u8> {
        DESCRIPTORS.to_vec()
    }
}

fn ids(mut commands: Commands, schema: Res<Schema>, mut entity_ids: ResMut<EntityIds>, light: Res<Light>) {
    let id = |name| schema.id(name).unwrap_or_else(|| panic!("{name} is announced"));
    commands.insert_resource(Ids {
        stroll: id(types::STROLL),
        walker: id(types::WALKER),
        content_ref: id(core::CONTENT_REF),
        controller: id(core::CONTROLLER),
    });
    let world = proto::World { light_port: u32::from(light.port), horizon: light.horizon };
    commands.spawn((entity_ids.next_id(), Transform::default(), Discrete::default().with(id(types::WORLD), &world)));
}

/// A walker for each person and agent that joins, where the street starts.
fn arrive(
    mut commands: Commands,
    mut entity_ids: ResMut<EntityIds>,
    ids: Res<Ids>,
    grid: Res<Grid>,
    mut joined: MessageReader<Joined>,
) {
    for joined in joined.read() {
        if joined.role != Role::Player && joined.kind != SessionKind::Agent {
            continue;
        }
        // Side by side, not on top of one another.
        let mut at = grid.0.start;
        for k in 0..16 {
            let angle = (joined.session + k) as f32 * 2.399_963;
            let candidate = grid.0.start + Vec3::new(angle.cos(), 0.0, angle.sin()) * (0.8 + 0.2 * k as f32);
            if grid.0.holds(Vec2::new(candidate.x, candidate.z)) {
                at = Vec3::new(
                    candidate.x,
                    grid.0.ground(Vec2::new(candidate.x, candidate.z)).unwrap_or(at.y),
                    candidate.z,
                );
                break;
            }
        }
        let discrete = Discrete::default()
            .with(ids.content_ref, &builtin(core::BUILTIN_AVATAR))
            .with(ids.controller, &joined.controller)
            .with(ids.walker, &proto::Walker { look: joined.session, passer_by: false });
        commands.spawn((
            entity_ids.next_id(),
            Transform::from_translation(at),
            discrete,
            Owner(joined.session),
            Striding::default(),
        ));
    }
}

/// Takes each walker's newest stroll.
fn stroll(
    time: Res<Time>,
    ids: Res<Ids>,
    mut intents: MessageReader<IntentReceived>,
    mut walkers: Query<&mut Striding>,
) {
    for intent in intents.read().filter(|i| i.type_id == ids.stroll) {
        let Ok(mut striding) = walkers.get_mut(intent.entity) else { continue };
        let Ok(stroll) = proto::Stroll::decode(intent.value.as_slice()) else { continue };
        if intent.sequence > striding.sequence {
            striding.stride = walk::Stride {
                forward: stroll.forward.clamp(-1.0, 1.0),
                right: stroll.right.clamp(-1.0, 1.0),
                heading: stroll.heading,
                run: stroll.run,
            };
            striding.sequence = intent.sequence;
            striding.received = time.elapsed();
        }
    }
}

/// Moves each walker one tick on the walk grid.
fn step(time: Res<Time>, grid: Res<Grid>, mut walkers: Query<(&mut Striding, &mut Transform)>) {
    let dt = time.delta_secs();
    for (mut striding, mut transform) in &mut walkers {
        let fresh = time.elapsed().saturating_sub(striding.received) < INTENT_LIFETIME;
        let stride = if fresh { striding.stride } else { walk::Stride { forward: 0.0, right: 0.0, ..striding.stride } };
        let from = transform.translation;
        transform.translation = grid.0.step(from, &stride, dt);
        transform.rotation = Quat::from_rotation_y(stride.heading);
        striding.speed = if dt > 0.0 { from.distance(transform.translation) / dt } else { 0.0 };
    }
}

/// Tells the light's server where each walker is.
fn tell(walkers: Res<Walkers>, seen: Query<(&Replicated, &Transform, &Striding)>) {
    let Ok(mut all) = walkers.0.lock() else { return };
    all.clear();
    for (id, transform, striding) in &seen {
        all.insert(id.0, Seen { eyes: transform.translation + Vec3::Y * walk::EYES, speed: striding.speed });
    }
}
