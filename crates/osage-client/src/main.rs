// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! osage native client, phase 1: renders what a world replicates and moves
//! the player's avatar with WASD or the arrow keys.
//! `osage-client [ws://127.0.0.1:7878]`

use std::{collections::HashMap, thread};

use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender};
use osage_sdk::{
    World, WorldEvent, components, connect, hello, intent, types,
    v0::{Role, ServerMessage, Tier, Welcome, client_message},
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

/// Intents are sent at most this often, seconds.
const INTENT_PERIOD: f32 = 0.05;

/// What the network thread hands to the Bevy app.
enum Net {
    Welcome(Box<Welcome>),
    Message(Box<ServerMessage>),
    Closed(String),
}

#[derive(Resource)]
struct NetInbox(Receiver<Net>);

/// Messages for the gateway, sent by the network thread.
#[derive(Resource)]
struct NetOutbox(UnboundedSender<client_message::Message>);

#[derive(Resource, Default)]
struct Replica {
    world: Option<World>,
    me: String,
    objects: HashMap<u64, Entity>,
}

#[derive(Resource, Default)]
struct Movement {
    sequence: u64,
    moving: bool,
    since_last: f32,
}

#[derive(Resource)]
struct Shapes {
    cube: Handle<Mesh>,
    cube_material: Handle<StandardMaterial>,
    avatar: Handle<Mesh>,
    my_material: Handle<StandardMaterial>,
    others_material: Handle<StandardMaterial>,
}

fn main() {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ws://127.0.0.1:7878".to_owned());
    let (tx, rx) = crossbeam_channel::unbounded();
    let (out_tx, out_rx) = tokio::sync::mpsc::unbounded_channel();
    thread::Builder::new()
        .name("osage-net".to_owned())
        .spawn(move || network(url, tx, out_rx))
        .expect("network thread");

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "osage — connecting…".to_owned(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(NetInbox(rx))
        .insert_resource(NetOutbox(out_tx))
        .init_resource::<Replica>()
        .init_resource::<Movement>()
        .add_systems(Startup, setup)
        .add_systems(Update, (replicate, steer))
        .run();
}

fn network(url: String, tx: Sender<Net>, mut outbox: UnboundedReceiver<client_message::Message>) {
    let runtime = tokio::runtime::Runtime::new().expect("network runtime");
    runtime.block_on(async move {
        let hello = hello("osage-client", env!("CARGO_PKG_VERSION"), Role::Player, Tier::T3High);
        let session = match connect(&url, hello).await {
            Ok(session) => session,
            Err(error) => return drop(tx.send(Net::Closed(error.to_string()))),
        };
        let (welcome, mut sender, mut receiver) = session.split();
        let _ = tx.send(Net::Welcome(Box::new(welcome)));
        loop {
            tokio::select! {
                message = receiver.next_message() => match message {
                    Some(Ok(message)) => {
                        if tx.send(Net::Message(Box::new(message))).is_err() {
                            return;
                        }
                    }
                    Some(Err(error)) => return drop(tx.send(Net::Closed(error.to_string()))),
                    None => return drop(tx.send(Net::Closed("connection closed".to_owned()))),
                },
                Some(message) = outbox.recv() => {
                    if let Err(error) = sender.send(message).await {
                        return drop(tx.send(Net::Closed(error.to_string())));
                    }
                }
            }
        }
    });
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(6.0, 5.0, 8.0).looking_at(Vec3::new(0.0, 0.5, 0.0), Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 8_000.0,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(40.0, 40.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.13, 0.15, 0.18))),
    ));
    commands.insert_resource(Shapes {
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        cube_material: materials.add(Color::srgb(0.25, 0.73, 0.31)),
        avatar: meshes.add(Capsule3d::new(0.35, 1.1)),
        my_material: materials.add(Color::srgb(0.35, 0.65, 1.0)),
        others_material: materials.add(Color::srgb(0.82, 0.6, 0.13)),
    });
}

fn replicate(
    mut commands: Commands,
    inbox: Res<NetInbox>,
    mut replica: ResMut<Replica>,
    shapes: Res<Shapes>,
    mut windows: Query<&mut Window>,
    mut transforms: Query<&mut Transform>,
) {
    for net in inbox.0.try_iter() {
        match net {
            Net::Welcome(welcome) => {
                let descriptor = welcome.world.clone().unwrap_or_default();
                if let Some(source) = &descriptor.source_offer {
                    // AGPL section 13: tell the user where the server's source is.
                    info!("server source: {} ({})", source.url, source.version);
                }
                for mut window in &mut windows {
                    window.title = format!("osage — {} (move with WASD or arrows)", descriptor.name);
                }
                replica.me = welcome.identity.clone().unwrap_or_default().per_world_id;
                replica.world = Some(World::new(&welcome));
            }
            Net::Closed(reason) => {
                warn!("disconnected: {reason}");
                for mut window in &mut windows {
                    window.title = format!("osage — disconnected ({reason})");
                }
            }
            Net::Message(message) => {
                let Some(world) = replica.world.as_mut() else { continue };
                for event in world.apply(&message) {
                    match event {
                        WorldEvent::Entered(entity) => {
                            if let Some(object) = spawn_object(&mut commands, &replica, &shapes, entity) {
                                replica.objects.insert(entity, object);
                            }
                        }
                        WorldEvent::Changed { entity, type_name } if type_name == types::TRANSFORM => {
                            let (Some(&object), Some(world)) = (replica.objects.get(&entity), replica.world.as_ref())
                            else {
                                continue;
                            };
                            if let Ok(mut transform) = transforms.get_mut(object) {
                                *transform = to_bevy(world.get(entity, types::TRANSFORM));
                            }
                        }
                        WorldEvent::Left(entity) => {
                            if let Some(object) = replica.objects.remove(&entity) {
                                commands.entity(object).despawn();
                            }
                        }
                        WorldEvent::Changed { .. } => {}
                    }
                }
            }
        }
    }
}

/// Spawns the Bevy object of a replicated entity with a built-in shape.
fn spawn_object(commands: &mut Commands, replica: &Replica, shapes: &Shapes, entity: u64) -> Option<Entity> {
    let world = replica.world.as_ref()?;
    let content = world.get::<components::ContentRef>(entity, types::CONTENT_REF)?;
    let transform = to_bevy(world.get(entity, types::TRANSFORM));
    match content.uri.as_str() {
        types::BUILTIN_CUBE => Some(
            commands
                .spawn((
                    Mesh3d(shapes.cube.clone()),
                    MeshMaterial3d(shapes.cube_material.clone()),
                    transform,
                ))
                .id(),
        ),
        // Avatars stand on their origin: the capsule is lifted.
        types::BUILTIN_AVATAR => {
            let mine = world
                .get::<components::Controller>(entity, types::CONTROLLER)
                .is_some_and(|c| c.per_world_id == replica.me);
            let material = if mine {
                &shapes.my_material
            } else {
                &shapes.others_material
            };
            Some(
                commands
                    .spawn((transform, Visibility::default()))
                    .with_child((
                        Mesh3d(shapes.avatar.clone()),
                        MeshMaterial3d(material.clone()),
                        Transform::from_xyz(0.0, 0.9, 0.0),
                    ))
                    .id(),
            )
        }
        _ => None,
    }
}

/// Sends movement intents for the player's avatar while a key is held, and
/// once more when released so that it stops at once.
fn steer(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    replica: Res<Replica>,
    outbox: Res<NetOutbox>,
    mut movement: ResMut<Movement>,
) {
    movement.since_last += time.delta_secs();
    let Some(world) = replica.world.as_ref() else { return };
    let (Some(avatar), Some(move_type)) = (world.controlled_by(&replica.me), world.types().id(types::MOVE_INTENT))
    else {
        return;
    };
    let held = |a, b| keys.pressed(a) || keys.pressed(b);
    let x = f32::from(held(KeyCode::KeyD, KeyCode::ArrowRight)) - f32::from(held(KeyCode::KeyA, KeyCode::ArrowLeft));
    let z = f32::from(held(KeyCode::KeyS, KeyCode::ArrowDown)) - f32::from(held(KeyCode::KeyW, KeyCode::ArrowUp));
    let direction = Vec3::new(x, 0.0, z).normalize_or_zero();
    let moving = direction != Vec3::ZERO;
    if (!moving && !movement.moving) || movement.since_last < INTENT_PERIOD {
        return;
    }
    movement.moving = moving;
    movement.since_last = 0.0;
    movement.sequence += 1;
    let value = components::MoveIntent {
        direction: Some(osage_sdk::math::Vec3 {
            x: direction.x,
            y: 0.0,
            z: direction.z,
        }),
    };
    let _ = outbox.0.send(intent(movement.sequence, avatar, move_type, &value));
}

fn to_bevy(transform: Option<components::Transform>) -> Transform {
    let Some(t) = transform else {
        return Transform::default();
    };
    let vec3 = |v: Option<osage_sdk::math::Vec3>, default: Vec3| v.map_or(default, |v| Vec3::new(v.x, v.y, v.z));
    Transform {
        translation: vec3(t.translation, Vec3::ZERO),
        rotation: t
            .rotation
            .map_or(Quat::IDENTITY, |q| Quat::from_xyzw(q.x, q.y, q.z, q.w)),
        scale: vec3(t.scale, Vec3::ONE),
    }
}
