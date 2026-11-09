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
        ActionResult, ActionStatus, Component as WireComponent, EntityComponent, EntityEnter, ServerMessage,
        StateDatagram, Tier, TimeSync, client_message, server_message,
    },
};
use tokio::sync::mpsc::UnboundedSender;

use crate::{now_us, schema};

/// What the gateway tells the region.
pub enum RegionInput {
    Join {
        session: u64,
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
        .add_systems(Update, (receive, orbit, broadcast_state).chain());
    app
}

#[derive(Resource)]
struct Inbox(Receiver<RegionInput>);

#[derive(Resource, Default)]
struct Sessions(HashMap<u64, UnboundedSender<ServerMessage>>);

#[derive(Resource, Default)]
struct EntityIds(u64);

#[derive(Resource, Default)]
struct Tick(u64);

/// World-unique id, stable across region handoffs (ADR-0002 §5).
#[derive(Component)]
struct Replicated(u64);

#[derive(Component)]
struct Content(components::ContentRef);

#[derive(Component)]
struct Controlled(components::Controller);

#[derive(Component)]
struct Orbit {
    radius: f32,
    speed: f32,
}

fn spawn_demo(mut commands: Commands, mut ids: ResMut<EntityIds>) {
    ids.0 += 1;
    commands.spawn((
        Replicated(ids.0),
        Transform::default(),
        Content(components::ContentRef {
            uri: types::BUILTIN_CUBE.to_owned(),
            kind: components::ContentKind::Builtin.into(),
            tiers: vec![Tier::T0Headless.into()],
            ..default()
        }),
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

fn receive(
    inbox: Res<Inbox>,
    mut sessions: ResMut<Sessions>,
    entities: Query<(&Replicated, &Transform, &Content, &Controlled)>,
) {
    for input in inbox.0.try_iter() {
        match input {
            RegionInput::Join { session, out } => {
                for (id, transform, content, controller) in &entities {
                    let _ = out.send(enter(id.0, transform, content, controller));
                }
                sessions.0.insert(session, out);
            }
            RegionInput::Leave { session } => {
                sessions.0.remove(&session);
            }
            RegionInput::Message { session, message } => {
                let Some(out) = sessions.0.get(&session) else { continue };
                let reply = match message {
                    client_message::Message::TimeSync(sync) => server_message::Message::TimeSync(TimeSync {
                        server_time_us: now_us(),
                        ..sync
                    }),
                    client_message::Message::ActionRequest(request) => {
                        server_message::Message::ActionResult(ActionResult {
                            request_id: request.request_id,
                            status: ActionStatus::Rejected.into(),
                            reason: "this world has no actions yet".to_owned(),
                            ..default()
                        })
                    }
                    _ => continue,
                };
                let _ = out.send(ServerMessage { message: Some(reply) });
            }
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
    let state = ServerMessage {
        message: Some(server_message::Message::State(StateDatagram {
            tick: tick.0,
            server_time_us: now_us(),
            values,
        })),
    };
    for out in sessions.0.values() {
        let _ = out.send(state.clone());
    }
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
