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
            .add_systems(Update, (replicate, stroll, others.after(crate::walker::Walking)).chain());
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

/// Move the replica towards its latest target, then match the feet to the horizontal
/// distance it actually travelled during this frame. Turning and height corrections
/// alone leave a walker idle.
fn advance_walker(place: &mut Transform, target: Transform, dt: f32) -> (osage_humans::Move, f32) {
    use osage_humans::Move;
    let previous = place.translation;
    place.translation = previous.lerp(target.translation, (dt * 10.0).min(1.0));
    place.rotation = target.rotation * Quat::from_rotation_y(std::f32::consts::PI);
    let speed = (place.translation - previous).xz().length() / dt;
    let act = if speed > 2.5 {
        Move::JOG
    } else if speed > 0.2 {
        Move::WALK
    } else {
        Move::IDLE
    };
    (act, act.ground_speed_m_s().map_or(1.0, |unit_speed| speed / unit_speed))
}

type WalkerCamera = (With<osage_render::OsageCamera>, Without<osage_humans::Human>);

/// The other walkers, as people walking where the server says.
fn others(
    mut commands: Commands,
    mut replica: ResMut<Replica>,
    mut places: Query<(&mut Transform, &mut osage_humans::Human, Has<osage_humans::PlannedHumanSteps>)>,
    cameras: Query<(&Camera, &Projection, &Transform), WalkerCamera>,
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
    let view = cameras.single().ok().and_then(|(camera, projection, transform)| {
        let Projection::Perspective(p) = projection else { return None };
        let height = camera.physical_viewport_size()?.y;
        osage_objects::DetailView::new(
            transform.translation.as_dvec3(),
            transform.forward().as_dvec3(),
            p.fov.into(),
            height,
            p.near.into(),
        )
    });
    let dt = time.delta_secs();
    for (e, target, look) in walkers {
        match replica.people.get(&e) {
            Some(&entity) => {
                if let Ok((mut place, mut human, planned)) = places.get_mut(entity) {
                    if dt > 0.0 {
                        let (act, pace) = advance_walker(&mut place, target, dt);
                        if human.act != act || human.pace != pace {
                            human.act = act;
                            human.pace = pace;
                        }
                    }
                    crate::foot_detail::update(&mut commands, entity, view, place.translation, planned);
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
                crate::foot_detail::update(&mut commands, entity, view, place.translation, false);
                replica.people.insert(e, entity);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use osage_humans::Move;

    #[test]
    fn remote_steps_follow_the_distance_displayed_between_snapshots() {
        for hz in [30, 60, 120] {
            for packet_hz in [15, 30] {
                for (speed, expected) in [(1.4, Move::WALK), (4.0, Move::JOG)] {
                    let dt = 1.0 / hz as f32;
                    let mut place = Transform::IDENTITY;
                    for frame in 1..=3 * hz {
                        let packet = frame * packet_hz / hz;
                        let target = Transform::from_xyz(0., 0., speed * packet as f32 / packet_hz as f32);
                        let before = place.translation;
                        let (act, pace) = advance_walker(&mut place, target, dt);
                        if frame > hz {
                            assert_eq!(act, expected);
                            let stride = act.ground_speed_m_s().unwrap() * pace * dt;
                            assert!((stride - before.distance(place.translation)).abs() < 1e-6);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_pure_height_or_heading_change_keeps_the_idle_clip_at_its_own_pace() {
        let target = Transform::from_xyz(0., 0.3, 0.).with_rotation(Quat::from_rotation_y(0.5));
        let mut place = Transform::IDENTITY;
        assert_eq!(advance_walker(&mut place, target, 1. / 60.), (Move::IDLE, 1.));
        assert!(place.translation.y > 0.);
        assert_ne!(place.rotation, Quat::IDENTITY);
    }
}
