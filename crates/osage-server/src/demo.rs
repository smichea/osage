// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The demo world: a server-driven cube, and an avatar for every player and
//! agent, walked by `osage.move_intent/1` intents.

use std::time::Duration;

use bevy::prelude::*;
use osage_proto::{components, types, v0::Role};
use prost::Message as _;

use crate::{
    schema,
    world::{Discrete, EntityIds, IntentReceived, Joined, Owner, RegionSet, SessionKind, WorldModule, builtin},
};

/// Avatar walking speed, metres per second.
const AVATAR_SPEED: f32 = 4.0;
/// An avatar stops when its controller has sent no intent for this long.
const INTENT_LIFETIME: Duration = Duration::from_millis(500);
/// Avatars stay inside this square around the origin, metres.
const WORLD_HALF_SIZE: f32 = 20.0;

pub struct DemoWorld;

impl Plugin for DemoWorld {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_cube)
            .add_systems(Update, (spawn_avatars, steer, orbit, walk).chain().in_set(RegionSet::World));
    }
}

impl WorldModule for DemoWorld {}

/// Latest movement intent of an avatar.
#[derive(Component, Default)]
struct MoveInput {
    direction: Vec3,
    sequence: u64,
    received: Duration,
}

#[derive(Component)]
struct Orbit {
    radius: f32,
    speed: f32,
}

fn spawn_cube(mut commands: Commands, mut ids: ResMut<EntityIds>) {
    let system = components::Controller { kind: components::ControllerKind::System.into(), ..default() };
    let discrete =
        Discrete::default().with(schema::CONTENT_REF, &builtin(types::BUILTIN_CUBE)).with(schema::CONTROLLER, &system);
    commands.spawn((ids.next(), Transform::default(), discrete, Orbit { radius: 3.0, speed: 0.8 }));
}

fn spawn_avatars(mut commands: Commands, mut ids: ResMut<EntityIds>, mut joined: MessageReader<Joined>) {
    for joined in joined.read() {
        if joined.role != Role::Player && joined.kind != SessionKind::Agent {
            continue;
        }
        let discrete = Discrete::default()
            .with(schema::CONTENT_REF, &builtin(types::BUILTIN_AVATAR))
            .with(schema::CONTROLLER, &joined.controller);
        let place = spawn_point(joined.session);
        commands.spawn((ids.next(), place, discrete, Owner(joined.session), MoveInput::default()));
    }
}

/// Applies an avatar's movement intents, newest sequence only.
fn steer(time: Res<Time>, mut intents: MessageReader<IntentReceived>, mut avatars: Query<&mut MoveInput>) {
    for intent in intents.read().filter(|i| i.type_id == schema::MOVE_INTENT) {
        let Ok(mut state) = avatars.get_mut(intent.entity) else { continue };
        let Ok(move_intent) = components::MoveIntent::decode(intent.value.as_slice()) else { continue };
        if intent.sequence > state.sequence {
            let d = move_intent.direction.unwrap_or_default();
            state.direction = Vec3::new(d.x, 0.0, d.z).clamp_length_max(1.0);
            state.sequence = intent.sequence;
            state.received = time.elapsed();
        }
    }
}

fn orbit(time: Res<Time>, mut bodies: Query<(&Orbit, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (orbit, mut transform) in &mut bodies {
        let angle = t * orbit.speed;
        transform.translation = Vec3::new(orbit.radius * angle.cos(), 1.0, orbit.radius * angle.sin());
        transform.rotation = Quat::from_rotation_y(-angle);
    }
}

fn walk(time: Res<Time>, mut walkers: Query<(&MoveInput, &mut Transform)>) {
    for (input, mut transform) in &mut walkers {
        let fresh = time.elapsed().saturating_sub(input.received) < INTENT_LIFETIME;
        if !fresh || input.direction == Vec3::ZERO {
            continue;
        }
        let next = transform.translation + input.direction * AVATAR_SPEED * time.delta_secs();
        transform.translation = next.clamp(
            Vec3::new(-WORLD_HALF_SIZE, 0.0, -WORLD_HALF_SIZE),
            Vec3::new(WORLD_HALF_SIZE, 0.0, WORLD_HALF_SIZE),
        );
        transform.rotation = Quat::from_rotation_arc(Vec3::NEG_Z, input.direction.normalize());
    }
}

/// Avatars appear on a ring around the origin, one slot per session.
fn spawn_point(session: u64) -> Transform {
    let angle = session as f32 * 2.399_963; // golden angle, radians
    Transform::from_xyz(6.0 * angle.cos(), 0.0, 6.0 * angle.sin())
}
