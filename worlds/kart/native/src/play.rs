// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Playing online: this client joins a kart world like any other client.
//! Everything it knows about the circuit and the race comes from the world;
//! its own kart is predicted with the world's physics (D-031), the others
//! are drawn from the world's states.

use std::{collections::HashMap, thread, time::SystemTime};

use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};
use crossbeam_channel::{Receiver, Sender};
use osage_kart_items::wants;
use osage_kart_physics::{Controls, Kart, Spot, wrap};
use osage_sdk::{
    Transport, World, WorldEvent, components, connect, hello, intent, types as core,
    v0::{self, Role, ServerMessage, Welcome, client_message, server_message},
};
use osage_world_kart::{
    Circuit, autopilot,
    proto::{self, Phase},
    types,
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{
    hud::{Entry, HudPlugin, Lights, RaceView},
    paddock::{PaddockPlugin, PaddockView, item},
    predict::Predictor,
    scene::{Course, Followed, Pose, Stage},
    settings::Menu,
};

/// How fast keyboard steering swings from one side to the other, per second.
const STEER_RATE: f32 = 6.0;

pub struct PlayPlugin {
    pub url: String,
    pub tier: v0::Tier,
    /// The world's autopilot holds the wheel: a demo, and a way to check
    /// prediction end to end without a driver.
    pub autodrive: bool,
    /// Saves a screenshot this many seconds after the start.
    pub screenshot_after: Option<f32>,
}

impl Plugin for PlayPlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = crossbeam_channel::unbounded();
        let (out_tx, out_rx) = tokio::sync::mpsc::unbounded_channel();
        let (url, tier) = (self.url.clone(), self.tier);
        thread::Builder::new()
            .name("osage-net".to_owned())
            .spawn(move || network(url, tier, tx, out_rx))
            .expect("network thread");

        app.insert_resource(Time::<Fixed>::from_hz(60.0))
            .insert_resource(NetInbox(rx))
            .insert_resource(NetOutbox(out_tx))
            .insert_resource(Replica { status: format!("connecting to {}…", self.url), ..default() })
            .insert_resource(Driver { autodrive: self.autodrive, ..default() })
            .insert_resource(Shots { after: self.screenshot_after })
            .add_plugins((HudPlugin, PaddockPlugin))
            .add_systems(FixedUpdate, drive)
            .add_systems(Update, (receive, send, poses, follow_someone, summarise).chain().in_set(Stage::Simulate))
            .add_systems(Update, screenshots);
    }
}

/// What the network thread hands to the Bevy app.
enum Net {
    Welcome(Box<Welcome>, Transport),
    Message(Box<ServerMessage>),
    Closed(String),
}

#[derive(Resource)]
struct NetInbox(Receiver<Net>);

/// Messages for the gateway, sent by the network thread.
#[derive(Resource)]
struct NetOutbox(UnboundedSender<client_message::Message>);

/// What this client knows of the world.
#[derive(Resource, Default)]
struct Replica {
    world: Option<World>,
    me: String,
    circuit: Option<Circuit>,
    /// Kart entities of the world and their models here.
    karts: HashMap<u64, Entity>,
    mine: Option<u64>,
    followed: Option<Entity>,
    controls_type: Option<u32>,
    /// The world's clock minus ours, microseconds.
    clock_offset: i64,
    status: String,
}

impl Replica {
    fn race(&self) -> Option<proto::Race> {
        let world = self.world.as_ref()?;
        world.entities().find_map(|entity| world.get(entity, types::RACE))
    }

    fn racer(&self, kart: u64) -> Option<proto::Racer> {
        self.world.as_ref()?.get(kart, types::RACER)
    }

    fn paddock(&self) -> Option<proto::Paddock> {
        let world = self.world.as_ref()?;
        world.entities().find_map(|entity| world.get(entity, types::PADDOCK))
    }

    /// Every kart on the track, and which of them is this player's.
    fn field(&self, circuit: &Circuit) -> (Vec<Spot>, Option<usize>) {
        let Some(world) = &self.world else { return (Vec::new(), None) };
        let mut spots = Vec::new();
        let mut me = None;
        for &id in self.karts.keys() {
            let Some(state) = world.get::<proto::KartState>(id, types::STATE) else { continue };
            if Some(id) == self.mine {
                me = Some(spots.len());
            }
            spots.push(circuit.track.locate(state.x, state.z));
        }
        (spots, me)
    }

    fn finished(&self, kart: u64) -> bool {
        self.racer(kart).is_some_and(|racer| racer.finish_ms > 0)
    }

    /// The world's clock now, microseconds.
    fn world_time(&self) -> i64 {
        now_us() + self.clock_offset
    }
}

/// The player's own kart: predicted here, step by step.
#[derive(Resource, Default)]
struct Driver {
    predictor: Predictor,
    /// Steps taken since the last message to the world.
    fresh: usize,
    steer: f32,
    autodrive: bool,
}

#[derive(Resource)]
struct Shots {
    after: Option<f32>,
}

/// The world's entity a kart model stands for.
#[derive(Component)]
struct Remote(u64);

fn now_us() -> i64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_micros() as i64)
}

fn network(url: String, tier: v0::Tier, tx: Sender<Net>, mut outbox: UnboundedReceiver<client_message::Message>) {
    let runtime = tokio::runtime::Runtime::new().expect("network runtime");
    runtime.block_on(async move {
        let hello = hello("osage-kart-native", env!("CARGO_PKG_VERSION"), Role::Player, tier);
        let session = match connect(&url, hello).await {
            Ok(session) => session,
            Err(error) => return drop(tx.send(Net::Closed(error.to_string()))),
        };
        let transport = session.transport();
        let (welcome, mut sender, mut receiver) = session.split();
        let _ = tx.send(Net::Welcome(Box::new(welcome), transport));
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

fn kart_of(state: &proto::KartState) -> Kart {
    Kart {
        x: state.x,
        z: state.z,
        heading: state.heading,
        speed: state.speed,
        slide: state.slide,
        drift: state.drift,
        charge: state.charge,
        boost: state.boost,
        spin: state.spin,
        rocket: state.rocket,
    }
}

/// Applies what the world sent: the circuit, karts coming and going, and the
/// state of this player's kart, which the predictor restarts from.
fn receive(mut commands: Commands, inbox: Res<NetInbox>, mut replica: ResMut<Replica>, mut driver: ResMut<Driver>) {
    for net in inbox.0.try_iter() {
        let message = match net {
            Net::Welcome(welcome, transport) => {
                let world = World::new(&welcome);
                replica.me = welcome.identity.as_ref().map(|i| i.per_world_id.clone()).unwrap_or_default();
                replica.controls_type = world.types().id(types::CONTROLS);
                let descriptor = welcome.world.clone().unwrap_or_default();
                // AGPL section 13: show where the server's Corresponding Source is.
                let source = descriptor.source_offer.map(|s| format!("\nserver source: {} ({})", s.url, s.version));
                // (The default font has few symbols: plain separators.)
                replica.status = format!(
                    "world {} | you are {} | via {transport:?}{}",
                    descriptor.name,
                    replica.me,
                    source.unwrap_or_default()
                );
                replica.world = Some(world);
                continue;
            }
            Net::Closed(reason) => {
                replica.status = format!("disconnected: {reason}");
                continue;
            }
            Net::Message(message) => message,
        };
        if let Some(server_message::Message::State(state)) = &message.message {
            replica.clock_offset = state.server_time_us as i64 - now_us();
        }
        let Replica { world: Some(world), me, circuit, karts, mine, .. } = &mut *replica else { continue };
        for event in world.apply(&message) {
            match event {
                WorldEvent::Entered(entity) => {
                    if let Some(track) = world.get::<proto::Track>(entity, types::TRACK) {
                        commands.insert_resource(Course(Circuit::from_proto(&track)));
                        *circuit = Some(Circuit::from_proto(&track));
                    }
                    let (Some(racer), Some(state)) = (
                        world.get::<proto::Racer>(entity, types::RACER),
                        world.get::<proto::KartState>(entity, types::STATE),
                    ) else {
                        continue;
                    };
                    let kart = kart_of(&state);
                    karts.insert(entity, commands.spawn((Pose { kart, color: racer.color }, Remote(entity))).id());
                    let controller = world.get::<components::Controller>(entity, core::CONTROLLER);
                    if controller.is_some_and(|c| c.per_world_id == *me) {
                        *mine = Some(entity);
                        driver.predictor = Predictor::default();
                        if let Some(circuit) = circuit {
                            driver.predictor.reconcile(kart, state.applied, &circuit.track);
                        }
                    }
                }
                WorldEvent::Left(entity) => {
                    if let Some(model) = karts.remove(&entity) {
                        commands.entity(model).despawn();
                    }
                    if *mine == Some(entity) {
                        *mine = None;
                    }
                }
                WorldEvent::Changed { entity, type_name } if *mine == Some(entity) && type_name == types::STATE => {
                    if let (Some(state), Some(circuit)) =
                        (world.get::<proto::KartState>(entity, types::STATE), &circuit)
                    {
                        driver.predictor.reconcile(kart_of(&state), state.applied, &circuit.track);
                    }
                }
                WorldEvent::Changed { .. } => {}
            }
        }
    }
}

/// One physics step of this player's kart: read the controls, predict.
fn drive(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    replica: Res<Replica>,
    menu: Res<Menu>,
    mut driver: ResMut<Driver>,
) {
    let (Some(mine), Some(circuit)) = (replica.mine, &replica.circuit) else { return };
    // While the settings are open, the keys are theirs.
    let keys = if menu.0 { &ButtonInput::default() } else { &*keys };
    if replica.finished(mine) {
        return;
    }
    let key = |codes: &[KeyCode]| if keys.any_pressed(codes.iter().copied()) { 1.0 } else { 0.0 };
    let mut throttle = key(&[KeyCode::ArrowUp, KeyCode::KeyW]) - key(&[KeyCode::ArrowDown, KeyCode::KeyS]);
    let wanted = key(&[KeyCode::ArrowRight, KeyCode::KeyD]) - key(&[KeyCode::ArrowLeft, KeyCode::KeyA]);
    let mut drift = keys.any_pressed([KeyCode::Space, KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let mut using = keys.any_pressed([KeyCode::KeyF, KeyCode::ControlLeft, KeyCode::ControlRight]);
    let swing = STEER_RATE * time.delta_secs();
    driver.steer += (wanted - driver.steer).clamp(-swing, swing);

    if let Some(pad) = gamepads.iter().next().filter(|_| !menu.0) {
        let stick = pad.get(GamepadAxis::LeftStickX).unwrap_or(0.0);
        if stick.abs() > 0.15 {
            driver.steer = stick.clamp(-1.0, 1.0);
        }
        let value = |button| pad.get(button).unwrap_or(0.0);
        let triggers = value(GamepadButton::RightTrigger2) - value(GamepadButton::LeftTrigger2);
        let buttons = value(GamepadButton::South) - value(GamepadButton::East);
        let pedal = if triggers != 0.0 { triggers } else { buttons };
        if pedal != 0.0 {
            throttle = pedal;
        }
        drift |= pad.any_pressed([GamepadButton::RightTrigger, GamepadButton::LeftTrigger, GamepadButton::West]);
        using |= pad.pressed(GamepadButton::North);
    }
    let mut controls = Controls { throttle, steer: driver.steer, drift };
    if let (true, Some(kart)) = (driver.autodrive, driver.predictor.kart) {
        controls = autopilot(&kart, &proto::Autopilot { pace: 0.9, line: 0.0, drift: true, use_items: false }, circuit);
        // And uses what it picks up as the world's drivers do.
        let held = replica.racer(mine).and_then(|racer| item(racer.held()));
        let (spots, me) = replica.field(circuit);
        using = held.zip(me).is_some_and(|(held, me)| wants(held, me, &spots, &kart, &circuit.track));
    }
    // The lights hold the karts: the world ignores the throttle until the start.
    if replica.race().is_none_or(|race| race.phase() != Phase::Racing) {
        controls.throttle = 0.0;
    }
    let controls = controls.sanitized();
    driver.predictor.step(controls, using, &circuit.track);
    driver.fresh += 1;
}

/// The frame's steps to the world, in one message.
fn send(replica: Res<Replica>, outbox: Res<NetOutbox>, mut driver: ResMut<Driver>) {
    let fresh = std::mem::take(&mut driver.fresh);
    let (Some(mine), Some(controls_type)) = (replica.mine, replica.controls_type) else { return };
    let steps = driver.predictor.to_send(fresh);
    if let (true, Some(last)) = (fresh > 0, steps.last()) {
        let _ = outbox.0.send(intent(last.sequence, mine, controls_type, &proto::Controls { steps: steps.clone() }));
    }
}

/// Where every kart is drawn this frame: this player's from the prediction,
/// between two steps; the others gliding towards the world's latest state.
fn poses(
    time: Res<Time>,
    fixed: Res<Time<Fixed>>,
    replica: Res<Replica>,
    mut driver: ResMut<Driver>,
    mut karts: Query<(&Remote, &mut Pose)>,
) {
    let Some(world) = &replica.world else { return };
    driver.predictor.fade(time.delta_secs());
    let glide = 1.0 - (-time.delta_secs() * 18.0).exp();
    for (remote, mut pose) in &mut karts {
        let Some(server) = world.get::<proto::KartState>(remote.0, types::STATE).map(|s| kart_of(&s)) else { continue };
        let predicted = (replica.mine == Some(remote.0) && !replica.finished(remote.0))
            .then(|| driver.predictor.shown(fixed.overstep_fraction()))
            .flatten();
        pose.kart = predicted.unwrap_or(Kart {
            x: pose.kart.x + (server.x - pose.kart.x) * glide,
            z: pose.kart.z + (server.z - pose.kart.z) * glide,
            heading: pose.kart.heading + wrap(server.heading - pose.kart.heading) * glide,
            ..server
        });
    }
}

/// The camera follows this player's kart, or the leader's when watching.
fn follow_someone(mut commands: Commands, mut replica: ResMut<Replica>) {
    let leader = replica.race().and_then(|race| race.order.first().copied());
    let wanted = replica.mine.or(leader).and_then(|kart| replica.karts.get(&kart).copied());
    if wanted != replica.followed {
        if let Some(old) = replica.followed.filter(|old| replica.karts.values().any(|k| k == old)) {
            commands.entity(old).remove::<Followed>();
        }
        if let Some(new) = wanted {
            commands.entity(new).insert(Followed);
        }
        replica.followed = wanted;
    }
}

/// F12, or the delay asked on the command line, saves what the window shows
/// to `kart-native-<time>.png` in the current directory.
fn screenshots(mut commands: Commands, time: Res<Time>, keys: Res<ButtonInput<KeyCode>>, mut shots: ResMut<Shots>) {
    let due = shots.after.is_some_and(|after| time.elapsed_secs() >= after);
    if due {
        shots.after = None;
    }
    if due || keys.just_pressed(KeyCode::F12) {
        let name = format!("kart-native-{}.png", now_us() / 1_000_000);
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(name));
    }
}

/// What the interface shows this frame, from what the world sent.
fn summarise(
    replica: Res<Replica>,
    driver: Res<Driver>,
    karts: Query<(&Remote, &Pose)>,
    mut view: ResMut<RaceView>,
    mut paddock: ResMut<PaddockView>,
) {
    if let Some(now) = replica.paddock().filter(|now| *now != paddock.paddock) {
        paddock.paddock = now;
    }
    let (lead, me) = (driver.predictor.lead(), driver.predictor.kart.filter(|_| replica.mine.is_some()));
    if paddock.lead != lead || paddock.me != me {
        paddock.lead = lead;
        paddock.me = me;
    }
    view.held = replica.mine.and_then(|mine| replica.racer(mine)).and_then(|racer| item(racer.held()));
    let race = replica.race();
    let now = replica.world_time();
    let seconds_left = |race: &proto::Race| ((race.phase_ends_us as i64 - now) as f32 / 1e6).ceil() as i64;
    view.status.clone_from(&replica.status);
    view.laps = race.as_ref().map_or(0, |race| race.laps);
    view.clock_ms = race.as_ref().map_or(0, |race| (now - race.started_us as i64) / 1000);
    view.stage = match &race {
        None => Lights::Off,
        Some(race) => match race.phase() {
            Phase::Waiting | Phase::Unspecified => Lights::Waiting,
            Phase::Countdown => Lights::Count(seconds_left(race)),
            Phase::Racing if view.clock_ms < 1000 => Lights::Go,
            Phase::Racing => Lights::Racing,
            Phase::Finished => Lights::Results(seconds_left(race)),
        },
    };
    let order = race.map(|race| race.order).unwrap_or_default();
    view.karts = order
        .iter()
        .filter_map(|id| {
            let racer = replica.racer(*id)?;
            let (_, pose) = karts.iter().find(|(remote, _)| remote.0 == *id)?;
            Some(Entry {
                id: *id,
                name: racer.name,
                bot: racer.bot,
                color: racer.color,
                position: racer.position,
                lap: racer.lap,
                finish_ms: racer.finish_ms,
                at: Vec2::new(pose.kart.x, pose.kart.z),
            })
        })
        .collect();
    view.me = replica.mine.and_then(|mine| view.karts.iter().position(|kart| kart.id == mine));
    let kart = driver.predictor.kart.unwrap_or_default();
    (view.speed, view.charge, view.boosting) = (kart.speed, kart.charge, kart.boost > 0.0);
}
