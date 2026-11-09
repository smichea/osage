// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Region server: authoritative simulation of one area as a headless Bevy app.
//! It replicates every entity to every session; interest management by cells
//! comes with the second region.

use std::{collections::HashMap, time::Duration};

use bevy::{app::ScheduleRunnerPlugin, prelude::*};
use crossbeam_channel::Receiver;
use osage_proto::{
    components, math, types,
    v0::{
        ActionResult, ActionStatus, Component as WireComponent, EntityComponent, EntityEnter, EntityLeave, Intent,
        LeaveReason, Role, ServerMessage, StateDatagram, Tier, TimeSync, client_message, server_message,
    },
};
use prost::Message as _;
use tokio::sync::mpsc::UnboundedSender;

use crate::{now_us, schema};

/// Avatar walking speed, metres per second.
const AVATAR_SPEED: f32 = 4.0;
/// An avatar stops when its controller has sent no intent for this long.
const INTENT_LIFETIME: Duration = Duration::from_millis(500);
/// Avatars stay inside this square around the origin, metres.
const WORLD_HALF_SIZE: f32 = 20.0;

/// What the gateway tells the region.
pub enum RegionInput {
    Join {
        session: u64,
        role: Role,
        per_world_id: String,
        out: UnboundedSender<ServerMessage>,
    },
    Leave {
        session: u64,
    },
    Message {
        session: u64,
        message: client_message::Message,
    },
}

pub fn app(inbox: Receiver<RegionInput>, tick_hz: u32) -> App {
    let tick = Duration::from_secs_f64(1.0 / f64::from(tick_hz.max(1)));
    let mut app = App::new();
    app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(tick)))
        .insert_resource(Inbox(inbox))
        .init_resource::<Sessions>()
        .init_resource::<EntityIds>()
        .init_resource::<Tick>()
        .add_systems(Startup, spawn_demo)
        .add_systems(Update, (receive, orbit, walk, broadcast_state).chain());
    app
}

#[derive(Resource)]
struct Inbox(Receiver<RegionInput>);

#[derive(Resource, Default)]
struct Sessions(HashMap<u64, UnboundedSender<ServerMessage>>);

impl Sessions {
    fn broadcast(&self, message: &ServerMessage) {
        for out in self.0.values() {
            let _ = out.send(message.clone());
        }
    }
}

#[derive(Resource, Default)]
struct EntityIds(u64);

impl EntityIds {
    fn next(&mut self) -> u64 {
        self.0 += 1;
        self.0
    }
}

#[derive(Resource, Default)]
struct Tick(u64);

/// World-unique id, stable across region handoffs (ADR-0002 §5).
#[derive(Component)]
struct Replicated(u64);

#[derive(Component)]
struct Content(components::ContentRef);

#[derive(Component)]
struct Controlled(components::Controller);

/// The session whose intents drive this entity.
#[derive(Component)]
struct Owner(u64);

/// Latest movement intent of an owned entity.
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

fn builtin(uri: &str) -> Content {
    Content(components::ContentRef {
        uri: uri.to_owned(),
        kind: components::ContentKind::Builtin.into(),
        tiers: vec![Tier::T0Headless.into()],
        ..default()
    })
}

fn spawn_demo(mut commands: Commands, mut ids: ResMut<EntityIds>) {
    commands.spawn((
        Replicated(ids.next()),
        Transform::default(),
        builtin(types::BUILTIN_CUBE),
        Controlled(components::Controller {
            kind: components::ControllerKind::System.into(),
            ..default()
        }),
        Orbit {
            radius: 3.0,
            speed: 0.8,
        },
    ));
}

#[allow(clippy::too_many_arguments)]
fn receive(
    mut commands: Commands,
    time: Res<Time>,
    inbox: Res<Inbox>,
    mut sessions: ResMut<Sessions>,
    mut ids: ResMut<EntityIds>,
    entities: Query<(&Replicated, &Transform, &Content, &Controlled)>,
    mut owned: Query<(Entity, &Replicated, &Owner, &mut MoveInput)>,
) {
    for input in inbox.0.try_iter() {
        match input {
            RegionInput::Join {
                session,
                role,
                per_world_id,
                out,
            } => {
                for (id, transform, content, controller) in &entities {
                    let _ = out.send(enter(id.0, transform, content, controller));
                }
                sessions.0.insert(session, out);
                if role == Role::Player {
                    let id = ids.next();
                    let transform = spawn_point(session);
                    let content = builtin(types::BUILTIN_AVATAR);
                    let controller = Controlled(components::Controller {
                        kind: components::ControllerKind::Human.into(),
                        per_world_id,
                        ..default()
                    });
                    sessions.broadcast(&enter(id, &transform, &content, &controller));
                    commands.spawn((
                        Replicated(id),
                        transform,
                        content,
                        controller,
                        Owner(session),
                        MoveInput::default(),
                    ));
                }
            }
            RegionInput::Leave { session } => {
                sessions.0.remove(&session);
                for (entity, id, owner, _) in &owned {
                    if owner.0 == session {
                        commands.entity(entity).despawn();
                        sessions.broadcast(&leave(id.0, LeaveReason::Destroyed));
                    }
                }
            }
            RegionInput::Message { session, message } => match message {
                client_message::Message::Intent(intent) => apply_intent(session, &intent, &time, &mut owned),
                client_message::Message::TimeSync(sync) => reply(
                    &sessions,
                    session,
                    server_message::Message::TimeSync(TimeSync {
                        server_time_us: now_us(),
                        ..sync
                    }),
                ),
                client_message::Message::ActionRequest(request) => reply(
                    &sessions,
                    session,
                    server_message::Message::ActionResult(ActionResult {
                        request_id: request.request_id,
                        status: ActionStatus::Rejected.into(),
                        reason: "this world has no actions yet".to_owned(),
                        ..default()
                    }),
                ),
                _ => {}
            },
        }
    }
}

/// Accepts a movement intent only for an entity the session owns, and only if
/// it is newer than the last one applied.
fn apply_intent(
    session: u64,
    intent: &Intent,
    time: &Time,
    owned: &mut Query<(Entity, &Replicated, &Owner, &mut MoveInput)>,
) {
    let Some(input) = &intent.input else { return };
    if input.r#type != schema::MOVE_INTENT {
        return;
    }
    let Ok(move_intent) = components::MoveIntent::decode(input.value.as_slice()) else {
        return;
    };
    for (_, id, owner, mut state) in owned.iter_mut() {
        if id.0 == intent.entity && owner.0 == session && intent.sequence > state.sequence {
            let d = move_intent.direction.unwrap_or_default();
            state.direction = Vec3::new(d.x, 0.0, d.z).clamp_length_max(1.0);
            state.sequence = intent.sequence;
            state.received = time.elapsed();
        }
    }
}

fn reply(sessions: &Sessions, session: u64, message: server_message::Message) {
    if let Some(out) = sessions.0.get(&session) {
        let _ = out.send(ServerMessage { message: Some(message) });
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

fn broadcast_state(
    mut tick: ResMut<Tick>,
    sessions: Res<Sessions>,
    changed: Query<(&Replicated, &Transform), Changed<Transform>>,
) {
    tick.0 += 1;
    if sessions.0.is_empty() {
        return;
    }
    let values: Vec<_> = changed
        .iter()
        .map(|(id, transform)| EntityComponent {
            entity: id.0,
            component: Some(wire(schema::TRANSFORM, &to_proto(transform))),
        })
        .collect();
    if values.is_empty() {
        return;
    }
    sessions.broadcast(&ServerMessage {
        message: Some(server_message::Message::State(StateDatagram {
            tick: tick.0,
            server_time_us: now_us(),
            values,
        })),
    });
}

/// Avatars appear on a ring around the origin, one slot per session.
fn spawn_point(session: u64) -> Transform {
    let angle = session as f32 * 2.399_963; // golden angle, radians
    Transform::from_xyz(6.0 * angle.cos(), 0.0, 6.0 * angle.sin())
}

fn enter(id: u64, transform: &Transform, content: &Content, controller: &Controlled) -> ServerMessage {
    ServerMessage {
        message: Some(server_message::Message::EntityEnter(EntityEnter {
            entity: id,
            components: vec![
                wire(schema::TRANSFORM, &to_proto(transform)),
                wire(schema::CONTENT_REF, &content.0),
                wire(schema::CONTROLLER, &controller.0),
            ],
        })),
    }
}

fn leave(id: u64, reason: LeaveReason) -> ServerMessage {
    ServerMessage {
        message: Some(server_message::Message::EntityLeave(EntityLeave {
            entity: id,
            reason: reason.into(),
        })),
    }
}

fn wire(type_id: u32, value: &impl prost::Message) -> WireComponent {
    WireComponent {
        r#type: type_id,
        value: value.encode_to_vec(),
    }
}

fn to_proto(transform: &Transform) -> components::Transform {
    let vec3 = |v: Vec3| math::Vec3 { x: v.x, y: v.y, z: v.z };
    let q = transform.rotation;
    components::Transform {
        translation: Some(vec3(transform.translation)),
        rotation: Some(math::Quat {
            x: q.x,
            y: q.y,
            z: q.z,
            w: q.w,
        }),
        scale: Some(vec3(transform.scale)),
    }
}
