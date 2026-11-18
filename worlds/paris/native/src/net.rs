// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The world's connection: the walker's strolls go to the server, which
//! moves it with the same steps the client took; the other walkers come
//! back, drawn as people; the world tells where its server of light is.

use std::{collections::HashMap, thread};

use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender};
use osage_sdk::{
    World, WorldEvent, components, connect, hello, intent, types,
    v0::{Role, ServerMessage, Tier, Welcome, client_message},
};
use osage_world_paris::{proto, types as paris};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{
    Options,
    light::{LightServer, WalkerId},
    walker::Me,
};

/// Strolls are sent this often, seconds.
const PERIOD: f32 = 0.05;
/// The server's place for the walker is taken over the client's beyond this, m.
const PARTED: f32 = 1.0;

pub struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Replica>()
            .add_systems(Startup, start)
            .add_systems(Update, (replicate, stroll, others).chain());
    }
}

enum Net {
    Welcome(Box<Welcome>),
    Message(Box<ServerMessage>),
    Closed(String),
}

#[derive(Resource)]
struct Link {
    inbox: Receiver<Net>,
    outbox: UnboundedSender<client_message::Message>,
    host: String,
}

#[derive(Resource, Default)]
struct Replica {
    world: Option<World>,
    me: String,
    sequence: u64,
    since: f32,
    /// The other walkers' people, by entity.
    people: HashMap<u64, Entity>,
}

fn start(mut commands: Commands, options: Res<Options>) {
    let Some(url) = options.server.clone() else { return };
    let host = url
        .trim_start_matches("ws://")
        .trim_start_matches("wss://")
        .split([':', '/'])
        .next()
        .unwrap_or("127.0.0.1")
        .to_owned();
    let (tx, rx) = crossbeam_channel::unbounded();
    let (out_tx, out_rx) = tokio::sync::mpsc::unbounded_channel();
    thread::Builder::new()
        .name("paris-net".to_owned())
        .spawn(move || network(url, tx, out_rx))
        .expect("the network thread");
    commands.insert_resource(Link { inbox: rx, outbox: out_tx, host });
}

fn network(url: String, tx: Sender<Net>, mut outbox: UnboundedReceiver<client_message::Message>) {
    let runtime = tokio::runtime::Runtime::new().expect("the network runtime");
    runtime.block_on(async move {
        let hello = hello("osage-paris", env!("CARGO_PKG_VERSION"), Role::Player, Tier::T3High);
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
                    None => return drop(tx.send(Net::Closed("the connection closed".to_owned()))),
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

fn to_bevy(t: Option<components::Transform>) -> Transform {
    let Some(t) = t else { return Transform::default() };
    let v = |v: Option<osage_sdk::math::Vec3>| v.map_or(Vec3::ZERO, |v| Vec3::new(v.x, v.y, v.z));
    Transform {
        translation: v(t.translation),
        rotation: t.rotation.map_or(Quat::IDENTITY, |q| Quat::from_xyzw(q.x, q.y, q.z, q.w)),
        scale: Vec3::ONE,
    }
}

fn replicate(
    link: Option<Res<Link>>,
    mut replica: ResMut<Replica>,
    mut light: ResMut<LightServer>,
    mut walker: ResMut<WalkerId>,
    mut me: ResMut<Me>,
) {
    let Some(link) = link else { return };
    for net in link.inbox.try_iter() {
        match net {
            Net::Welcome(welcome) => {
                replica.me = welcome.identity.clone().unwrap_or_default().per_world_id;
                replica.world = Some(World::new(&welcome));
                info!("paris: in the world as {}", replica.me);
            }
            Net::Closed(reason) => warn!("paris: disconnected: {reason}"),
            Net::Message(message) => {
                let Some(world) = replica.world.as_mut() else { continue };
                for event in world.apply(&message) {
                    if let WorldEvent::Entered(entity) = event
                        && let Some(about) = world.get::<proto::World>(entity, paris::WORLD)
                        && about.light_port != 0
                    {
                        light.0 = Some(format!("{}:{}", link.host, about.light_port));
                        info!("paris: light from {}", light.0.as_deref().unwrap_or(""));
                    }
                }
            }
        }
    }
    let Some(world) = replica.world.as_ref() else { return };
    if let Some(mine) = world.controlled_by(&replica.me) {
        if walker.0 != Some(mine) {
            walker.0 = Some(mine);
            // Where the server put the walker on arrival.
            me.feet = to_bevy(world.get(mine, types::TRANSFORM)).translation;
        } else {
            let there = to_bevy(world.get(mine, types::TRANSFORM)).translation;
            if there.distance(me.feet) > PARTED {
                me.feet = there;
            }
        }
    }
}

fn stroll(time: Res<Time>, link: Option<Res<Link>>, me: Res<Me>, mut replica: ResMut<Replica>) {
    let Some(link) = link else { return };
    replica.since += time.delta_secs();
    if replica.since < PERIOD {
        return;
    }
    replica.since = 0.0;
    let Some(world) = replica.world.as_ref() else { return };
    let (Some(mine), Some(type_id)) = (world.controlled_by(&replica.me), world.types().id(paris::STROLL)) else {
        return;
    };
    replica.sequence += 1;
    let s = me.stride;
    let value = proto::Stroll { forward: s.forward, right: s.right, heading: s.heading, run: s.run };
    let _ = link.outbox.send(intent(replica.sequence, mine, type_id, &value));
}

/// The other walkers, as people walking where the server says.
fn others(
    mut commands: Commands,
    mut replica: ResMut<Replica>,
    mut places: Query<(&mut Transform, Option<&mut osage_humans::Human>)>,
    time: Res<Time>,
) {
    let Some(world) = replica.world.as_ref() else { return };
    let mine = world.controlled_by(&replica.me);
    let walkers: Vec<(u64, Transform, u64)> = world
        .entities()
        .filter(|&e| Some(e) != mine)
        .filter_map(|e| {
            world.get::<proto::Walker>(e, paris::WALKER).map(|w| (e, to_bevy(world.get(e, types::TRANSFORM)), w.look))
        })
        .collect();
    let present: std::collections::HashSet<u64> = walkers.iter().map(|w| w.0).collect();
    let gone: Vec<u64> = replica.people.keys().filter(|e| !present.contains(e)).copied().collect();
    for e in gone {
        if let Some(entity) = replica.people.remove(&e) {
            commands.entity(entity).despawn();
        }
    }
    let dt = time.delta_secs().max(1e-3);
    for (e, target, look) in walkers {
        match replica.people.get(&e) {
            Some(&entity) => {
                if let Ok((mut place, human)) = places.get_mut(entity) {
                    let speed = place.translation.distance(target.translation) / dt;
                    // Eased to the server's place, turned the way it faces.
                    place.translation = place.translation.lerp(target.translation, (dt * 10.0).min(1.0));
                    place.rotation = target.rotation * Quat::from_rotation_y(std::f32::consts::PI);
                    if let Some(mut human) = human {
                        let act = if speed > 2.5 {
                            osage_humans::Move::JOG
                        } else if speed > 0.2 {
                            osage_humans::Move::WALK
                        } else {
                            osage_humans::Move::IDLE
                        };
                        if human.act != act {
                            human.act = act;
                        }
                    }
                }
            }
            None => {
                let pick = |n: u64, k: u64| ((look.wrapping_mul(2_654_435_761).wrapping_add(k * 97)) >> 7) % n;
                let palette = [
                    Color::srgb(0.12, 0.16, 0.28),
                    Color::srgb(0.55, 0.12, 0.14),
                    Color::srgb(0.85, 0.82, 0.74),
                    Color::srgb(0.18, 0.32, 0.22),
                    Color::srgb(0.08, 0.08, 0.09),
                ];
                let human = osage_humans::Human {
                    body: if pick(2, 1) == 0 { osage_humans::Body::Man } else { osage_humans::Body::Woman },
                    tone: if pick(2, 2) == 0 { osage_humans::Tone::Light } else { osage_humans::Tone::Dark },
                    hair: Some(osage_humans::Hair::ALL[pick(5, 3) as usize]),
                    hair_colour: Color::srgb(0.12, 0.08, 0.05),
                    outfit: osage_humans::Outfit {
                        top: palette[pick(5, 4) as usize],
                        bottom: Color::srgb(0.15, 0.16, 0.2),
                        shoes: Color::srgb(0.06, 0.05, 0.05),
                    },
                    act: osage_humans::Move::IDLE,
                    pace: 1.0,
                    phase: pick(100, 5) as f32 / 100.0,
                };
                let place =
                    Transform { rotation: target.rotation * Quat::from_rotation_y(std::f32::consts::PI), ..target };
                let entity = commands.spawn((human, place)).id();
                replica.people.insert(e, entity);
            }
        }
    }
}
