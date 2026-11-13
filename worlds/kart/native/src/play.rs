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
    hud::{Best, Entry, HudPlugin, Lights, RaceView, Records},
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
    /// Saves a screenshot at each of these many seconds after the start.
    pub screenshot_after: Vec<f32>,
    /// Players at this computer: 2 splits the screen (D-086).
    pub players: u8,
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
        // A second player: a connection of their own (D-086).
        if self.players >= 2 {
            let (tx, rx) = crossbeam_channel::unbounded();
            let (out_tx, out_rx) = tokio::sync::mpsc::unbounded_channel();
            let (url, tier) = (self.url.clone(), self.tier);
            thread::Builder::new()
                .name("osage-net-2".to_owned())
                .spawn(move || network(url, tier, tx, out_rx))
                .expect("network thread");
            app.insert_resource(Second {
                inbox: rx,
                outbox: out_tx,
                me: String::new(),
                controls_type: None,
                mine: None,
                followed: None,
                driver: Driver::default(),
            })
            .insert_resource(crate::scene::Split(true));
        }

        app.insert_resource(Time::<Fixed>::from_hz(60.0))
            .insert_resource(NetInbox(rx))
            .insert_resource(NetOutbox(out_tx))
            .insert_resource(Replica { status: format!("connecting to {}…", self.url), ..default() })
            .insert_resource(Host { host: host_of(&self.url), ..default() })
            .insert_resource(Driver { autodrive: self.autodrive, ..default() })
            .insert_resource(Shots { after: self.screenshot_after.clone() })
            .init_resource::<CallWanted>()
            .init_resource::<Chosen>()
            .init_resource::<ChosenEvent>()
            .insert_resource(Kept::load())
            .add_plugins((HudPlugin, PaddockPlugin, crate::show::ShowPlugin, crate::photo::PhotoPlugin))
            .add_systems(FixedUpdate, drive)
            .add_systems(
                Update,
                (receive, choose, send, poses, follow_someone, summarise.in_set(Summarised))
                    .chain()
                    .in_set(Stage::Simulate),
            )
            .add_systems(Update, screenshots);
    }
}

/// After the race is summarised for the HUD: what shows the race reads it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Summarised;

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
    choice_type: Option<u32>,
    /// Connections so far: the profile's choice is sent once in each.
    connections: u32,
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

    fn records(&self) -> Option<proto::Records> {
        let world = self.world.as_ref()?;
        world.entities().find_map(|entity| world.get(entity, types::RECORDS))
    }

    fn roster(&self) -> Option<proto::Roster> {
        let world = self.world.as_ref()?;
        world.entities().find_map(|entity| world.get(entity, types::ROSTER))
    }

    fn paddock(&self) -> Option<proto::Paddock> {
        let world = self.world.as_ref()?;
        world.entities().find_map(|entity| world.get(entity, types::PADDOCK))
    }

    /// Every kart on the track, and which of them is this player's.
    /// Where every kart is on the circuit, and which of them is `mine`.
    fn field_of(&self, circuit: &Circuit, mine: u64) -> (Vec<Spot>, Option<usize>) {
        let Some(world) = &self.world else { return (Vec::new(), None) };
        let mut spots = Vec::new();
        let mut me = None;
        for &id in self.karts.keys() {
            let Some(state) = world.get::<proto::KartState>(id, types::STATE) else { continue };
            if id == mine {
                me = Some(spots.len());
            }
            spots.push(circuit.track.locate(state.x, state.z));
        }
        (spots, me)
    }

    /// Each kart's place, in the order of [`Replica::field`]'s spots (gaps
    /// unknown here: 0).
    fn standings(&self, spots: &[Spot]) -> Vec<osage_kart_items::Standing> {
        let Some(world) = &self.world else { return Vec::new() };
        self.karts
            .keys()
            .filter(|&&id| world.get::<proto::KartState>(id, types::STATE).is_some())
            .map(|&id| osage_kart_items::Standing {
                place: self.racer(id).map_or(0, |racer| racer.position.saturating_sub(1) as usize),
                behind: 0.0,
            })
            .take(spots.len())
            .collect()
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
    /// A call about the race to send, and how many more messages carry it
    /// (a message may be lost).
    call: proto::RaceCall,
    calls_left: u8,
    /// The use button at the last step, and whether what the kart holds
    /// is behind it (as the world will have it): to see when the kart lets
    /// go of a tyre, a cone or oil, and draw it at once (D-071).
    button: bool,
    trailing: bool,
}

/// A call about the race the player asked for elsewhere (the settings).
#[derive(Resource, Default)]
pub struct CallWanted(pub Option<proto::RaceCall>);

/// Messages that carry a call.
const CALL_SENDS: u8 = 6;

#[derive(Resource)]
struct Shots {
    after: Vec<f32>,
}

/// The world's entity a kart model stands for.
#[derive(Component)]
pub struct Remote(pub u64);

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
    osage_world_kart::kart_of(state)
}

/// The controls of the last step the world applied to a kart (D-076).
fn controls_of(state: &proto::KartState) -> Controls {
    Controls { throttle: state.throttle, steer: state.steer, drift: state.held != 0.0 }.sanitized()
}

/// The kart as its own driver built it (D-055): what the predictor steps.
fn built(state: &proto::KartState, racer: Option<&proto::Racer>) -> Kart {
    Kart { build: racer.map_or(osage_kart_physics::Build::STANDARD, osage_world_kart::build_of), ..kart_of(state) }
}

/// Applies what the world sent: the circuit, karts coming and going, and the
/// state of this player's kart, which the predictor restarts from.
fn receive(
    mut commands: Commands,
    inbox: Res<NetInbox>,
    mut replica: ResMut<Replica>,
    mut driver: ResMut<Driver>,
    mut second: Option<ResMut<Second>>,
    poses: Query<&Pose>,
) {
    // The second player's connection (D-086): who they are; the world's
    // state comes through the first's.
    if let Some(second) = second.as_deref_mut() {
        for net in second.inbox.try_iter() {
            if let Net::Welcome(welcome, _) = net {
                let world = World::new(&welcome);
                second.me = welcome.identity.as_ref().map(|i| i.per_world_id.clone()).unwrap_or_default();
                second.controls_type = world.types().id(types::CONTROLS);
                second.mine = None;
            }
        }
    }
    for net in inbox.0.try_iter() {
        let message = match net {
            Net::Welcome(welcome, transport) => {
                let world = World::new(&welcome);
                replica.me = welcome.identity.as_ref().map(|i| i.per_world_id.clone()).unwrap_or_default();
                replica.controls_type = world.types().id(types::CONTROLS);
                replica.choice_type = world.types().id(types::CHOICE);
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
                replica.connections += 1;
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
            // The second player's kart, kept as the first's is.
            if let Some(second) = second.as_deref_mut() {
                match &event {
                    WorldEvent::Left(entity) if second.mine == Some(*entity) => second.mine = None,
                    WorldEvent::Changed { entity, type_name }
                        if second.mine == Some(*entity) && *type_name == types::STATE =>
                    {
                        if let (Some(state), Some(circuit)) =
                            (world.get::<proto::KartState>(*entity, types::STATE), &circuit)
                        {
                            let racer = world.get::<proto::Racer>(*entity, types::RACER);
                            second.driver.predictor.reconcile(
                                built(&state, racer.as_ref()),
                                state.applied,
                                &circuit.track,
                            );
                        }
                    }
                    WorldEvent::Changed { type_name, .. } if *type_name == types::TRACK => {
                        second.driver.predictor = Predictor::default();
                    }
                    _ => {}
                }
            }
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
                    karts.insert(
                        entity,
                        commands
                            .spawn((
                                Pose {
                                    kart,
                                    color: racer.color,
                                    held: None,
                                    smoke: None,
                                    trailing: false,
                                    guard: 0.0,
                                    ghost: racer.ghost || racer.out,
                                },
                                Remote(entity),
                            ))
                            .id(),
                    );
                    let controller = world.get::<components::Controller>(entity, core::CONTROLLER);
                    if controller.is_some_and(|c| c.per_world_id == *me) {
                        *mine = Some(entity);
                        driver.predictor = Predictor::default();
                        if let Some(circuit) = circuit {
                            driver.predictor.reconcile(built(&state, Some(&racer)), state.applied, &circuit.track);
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
                        let racer = world.get::<proto::Racer>(entity, types::RACER);
                        driver.predictor.reconcile(built(&state, racer.as_ref()), state.applied, &circuit.track);
                    }
                }
                // Another track (a cup's next race, D-056): the scene is
                // built again round it, and the player's kart restarts from
                // the world's.
                WorldEvent::Changed { entity, type_name } if type_name == types::TRACK => {
                    if let Some(track) = world.get::<proto::Track>(entity, types::TRACK) {
                        commands.insert_resource(Course(Circuit::from_proto(&track)));
                        *circuit = Some(Circuit::from_proto(&track));
                        driver.predictor = Predictor::default();
                    }
                }
                // Another driver, another colour (D-055): the kart's model
                // is built again in it.
                WorldEvent::Changed { entity, type_name } if type_name == types::RACER => {
                    let Some(racer) = world.get::<proto::Racer>(entity, types::RACER) else { continue };
                    let Some(&model) = karts.get(&entity) else { continue };
                    let Ok(pose) = poses.get(model) else { continue };
                    // Out of a battle (D-065): drawn as a ghost is.
                    let ghost = racer.ghost || racer.out;
                    if pose.color != racer.color || pose.ghost != ghost {
                        let pose = Pose { color: racer.color, ghost, ..*pose };
                        commands.entity(model).despawn();
                        karts.insert(entity, commands.spawn((pose, Remote(entity))).id());
                    }
                }
                WorldEvent::Changed { .. } => {}
            }
        }
    }
    // The second player's kart, once both the world and who they are are known.
    if let (Some(second), Some(world)) = (second.as_deref_mut(), replica.world.as_ref())
        && second.mine.is_none()
        && !second.me.is_empty()
    {
        let theirs = world.entities().find(|&entity| {
            world.get::<proto::KartState>(entity, types::STATE).is_some()
                && world
                    .get::<components::Controller>(entity, core::CONTROLLER)
                    .is_some_and(|c| c.per_world_id == second.me)
        });
        if let (Some(entity), Some(circuit)) = (theirs, &replica.circuit)
            && let Some(state) = world.get::<proto::KartState>(entity, types::STATE)
        {
            second.mine = Some(entity);
            second.driver.predictor = Predictor::default();
            let racer = world.get::<proto::Racer>(entity, types::RACER);
            second.driver.predictor.reconcile(built(&state, racer.as_ref()), state.applied, &circuit.track);
        }
    }
    // The road is as wet as the race says (D-068): the prediction steps on it.
    let wet = if replica.race().is_some_and(|race| race.rain) { 1.0 } else { 0.0 };
    if let Some(circuit) = &mut replica.circuit
        && circuit.track.wet != wet
    {
        circuit.track.wet = wet;
    }
}

/// Which player at this computer (D-086): the first, or the second of a
/// split screen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Seat {
    First,
    Second,
}

/// A player's controls from the keyboard and their gamepad. Alone, the
/// player has every key; on a split screen the first has W A S D, Space or
/// left Shift to drift, F or left Ctrl for the item, and the first gamepad;
/// the second the arrows, right Shift to drift, right Ctrl or / for the item,
/// and the second gamepad.
#[allow(clippy::type_complexity)]
fn controls_for(
    seat: Seat,
    split: bool,
    keys: &ButtonInput<KeyCode>,
    pad: Option<&Gamepad>,
    steer: &mut f32,
    seconds: f32,
) -> (Controls, bool) {
    use KeyCode as K;
    let (up, down, left, right, drifts, items): (&[K], &[K], &[K], &[K], &[K], &[K]) = match (split, seat) {
        (false, _) => (
            &[K::ArrowUp, K::KeyW],
            &[K::ArrowDown, K::KeyS],
            &[K::ArrowLeft, K::KeyA],
            &[K::ArrowRight, K::KeyD],
            &[K::Space, K::ShiftLeft, K::ShiftRight],
            &[K::KeyF, K::ControlLeft, K::ControlRight],
        ),
        (true, Seat::First) => {
            (&[K::KeyW], &[K::KeyS], &[K::KeyA], &[K::KeyD], &[K::Space, K::ShiftLeft], &[K::KeyF, K::ControlLeft])
        }
        (true, Seat::Second) => (
            &[K::ArrowUp],
            &[K::ArrowDown],
            &[K::ArrowLeft],
            &[K::ArrowRight],
            &[K::ShiftRight],
            &[K::ControlRight, K::Slash],
        ),
    };
    let key = |codes: &[K]| if keys.any_pressed(codes.iter().copied()) { 1.0 } else { 0.0 };
    let mut throttle = key(up) - key(down);
    let wanted = key(right) - key(left);
    let mut drift = key(drifts) > 0.0;
    let mut using = key(items) > 0.0;
    let swing = STEER_RATE * seconds;
    *steer += (wanted - *steer).clamp(-swing, swing);
    if let Some(pad) = pad {
        let stick = pad.get(GamepadAxis::LeftStickX).unwrap_or(0.0);
        if stick.abs() > 0.15 {
            *steer = stick.clamp(-1.0, 1.0);
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
    (Controls { throttle, steer: *steer, drift }, using)
}

/// One physics step of each player's kart at this computer: read the
/// controls, predict.
#[allow(clippy::too_many_arguments)]
fn drive(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    replica: Res<Replica>,
    menu: Res<Menu>,
    photo: Res<crate::photo::Photo>,
    mut driver: ResMut<Driver>,
    second: Option<ResMut<Second>>,
    mut paddock: ResMut<PaddockView>,
) {
    let Some(circuit) = &replica.circuit else { return };
    // While the settings are open, or a picture is taken, the keys are theirs.
    let menu = menu.0 || photo.is_on();
    let none = ButtonInput::default();
    let keys = if menu { &none } else { &*keys };
    let split = second.is_some();
    let seconds = time.delta_secs();
    if let Some(mine) = replica.mine {
        let autodrive = driver.autodrive;
        let mut steer = driver.steer;
        let pad = gamepads.iter().next();
        let (controls, using) = controls_for(Seat::First, split, keys, pad, &mut steer, seconds);
        driver.steer = steer;
        step_seat(&mut driver, mine, autodrive, controls, using, &replica, circuit, &mut paddock);
    }
    if let Some(mut second) = second
        && let Some(mine) = second.mine
    {
        let autodrive = driver.autodrive;
        let mut steer = second.driver.steer;
        // The second gamepad is the second player's.
        let pad = gamepads.iter().nth(1);
        let (controls, using) = controls_for(Seat::Second, split, keys, pad, &mut steer, seconds);
        second.driver.steer = steer;
        step_seat(&mut second.driver, mine, autodrive, controls, using, &replica, circuit, &mut paddock);
    }
}

/// One step of the kart `mine`, driven by `driver` from `controls`.
#[allow(clippy::too_many_arguments)]
fn step_seat(
    driver: &mut Driver,
    mine: u64,
    autodrive: bool,
    mut controls: Controls,
    mut using: bool,
    replica: &Replica,
    circuit: &Circuit,
    paddock: &mut PaddockView,
) {
    if replica.finished(mine) {
        return;
    }
    if let (true, Some(kart)) = (autodrive, driver.predictor.kart) {
        controls = autopilot(&kart, &proto::Autopilot { pace: 0.9, line: 0.0, drift: true, use_items: false }, circuit);
        // And uses what it picks up as the world's drivers do.
        let racer = replica.racer(mine);
        let held = racer.as_ref().and_then(|racer| item(racer.held()));
        let trailing = racer.as_ref().is_some_and(|racer| racer.trailing);
        let (spots, me) = replica.field_of(circuit, mine);
        let standings = replica.standings(&spots);
        using = held.zip(me).is_some_and(|(held, me)| {
            let wanted = wants(held, me, &spots, &standings, &kart, &circuit.track);
            // What trails goes behind on the way down and is used on the way up.
            if held.trails() && trailing { !wanted } else { wanted }
        });
    }
    // The lights hold the karts: the world ignores the throttle until the start.
    if replica.race().is_none_or(|race| race.phase() != Phase::Racing) {
        controls.throttle = 0.0;
    }
    let controls = controls.sanitized();
    // The karts that can be touched (D-076): not in a time trial, not
    // ghosts, not those out of a battle.
    let trial = replica.race().is_some_and(|race| race.trial);
    driver.predictor.others = if trial {
        Vec::new()
    } else {
        replica
            .karts
            .keys()
            .filter(|&&id| id != mine)
            .filter(|&&id| replica.racer(id).is_none_or(|r| !r.ghost && !r.out))
            .filter_map(|&id| {
                let state = replica.world.as_ref()?.get::<proto::KartState>(id, types::STATE)?;
                Some((built(&state, replica.racer(id).as_ref()), controls_of(&state)))
            })
            .collect()
    };
    driver.predictor.step(controls, using, &circuit.track);
    driver.fresh += 1;
    // What the kart lets go of, drawn at once where the world will put it
    // (D-071): the world's arrives a round trip later.
    let held = replica.racer(mine).and_then(|racer| item(racer.held()));
    let (pressed, let_go) = (using && !driver.button, !using && driver.button);
    driver.button = using;
    match held {
        Some(held) if held.trails() => {
            if pressed {
                driver.trailing = true;
            }
            if let_go
                && std::mem::take(&mut driver.trailing)
                && let Some(kart) = driver.predictor.kart
                && let Some((kind, x, z, vx, vz)) = osage_kart_items::released(held, &kart)
            {
                paddock.mine.push(crate::paddock::Provisional { kind, x, z, vx, vz, age: 0.0 });
            }
        }
        _ => driver.trailing = false,
    }
}

/// The second player of a split screen (D-086): a connection of their own
/// to the world — they are a player of it, with their own kart — whose
/// messages are not needed for the world's state, which the first's
/// connection keeps; their kart predicted as the first's is.
#[derive(Resource)]
pub struct Second {
    inbox: Receiver<Net>,
    outbox: UnboundedSender<client_message::Message>,
    /// Who they are in the world, and the type of the controls they send.
    me: String,
    controls_type: Option<u32>,
    /// Their kart in the world, its model here.
    pub mine: Option<u64>,
    followed: Option<Entity>,
    driver: Driver,
}

/// The player's profile (D-064, [`osage_kart_profile`]), kept in
/// `osage/kart-profile.txt` beside the settings.
#[derive(Resource, Default)]
struct Kept {
    profile: osage_kart_profile::Profile,
    /// The connection the profile's choice was sent in.
    sent: u32,
    /// The cup whose end was counted: (cup, standings), so it counts once.
    counted: Option<(String, Vec<proto::Standing>)>,
    /// What the last cup unlocked, to say.
    news: String,
}

impl Kept {
    fn file() -> Option<std::path::PathBuf> {
        Some(crate::settings::folder()?.join("kart-profile.txt"))
    }

    fn load() -> Self {
        let text = Self::file().and_then(|path| std::fs::read_to_string(path).ok()).unwrap_or_default();
        Self { profile: osage_kart_profile::Profile::from_text(&text), ..default() }
    }

    fn save(&self) {
        let Some(path) = Self::file() else { return };
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, self.profile.to_text()));
        if let Err(error) = written {
            warn!("profile not kept in {}: {error}", path.display());
        }
    }

    /// What the profile asks of the world: its driver, kart and paint.
    fn choice(&self) -> proto::Choice {
        proto::Choice {
            driver: self.profile.driver.clone(),
            kart: self.profile.kart.clone(),
            paint: Some(self.profile.paint.map_or(0, |p| p + 1)),
            ..default()
        }
    }

    /// A cup over, with `me` in its standings: its points counted, once.
    fn count_cup(&mut self, race: Option<&proto::Race>, me: &str) {
        let Some(cup) = race.and_then(|race| race.cup.as_ref()) else { return };
        let over = cup.race as usize > cup.tracks.len();
        let key = (cup.id.clone(), cup.standings.clone());
        if !over || me.is_empty() || self.counted.as_ref() == Some(&key) {
            return;
        }
        self.counted = Some(key);
        let Some(place) = cup.standings.iter().position(|s| s.name == me) else { return };
        let unlocked = self.profile.count_cup(cup.standings[place].points, place);
        let names: Vec<&str> = unlocked.iter().filter_map(|u| osage_kart_profile::paint_name(u.0)).collect();
        self.news = if names.is_empty() { String::new() } else { format!("NEW PAINT: {}  ", names.join(", ")) };
        self.save();
    }

    /// The profile as the HUD says it, with how to change the paint.
    fn line(&self) -> String {
        let p = &self.profile;
        let paint = p.paint.and_then(osage_kart_profile::paint_name).unwrap_or("the driver's colour");
        let next = p.next().map_or_else(String::new, |(paint, needs)| {
            format!(" – {} at {needs} pts", osage_kart_profile::paint_name(paint).unwrap_or("?"))
        });
        format!("Paint: {paint}\n{}{} pts · {} cups · {} won{next}", self.news, p.points, p.cups, p.wins)
    }
}

/// The driver and kart this player picked (D-055), as indices into the
/// world's roster; none until they pick.
#[derive(Resource, Default)]
struct Chosen(Option<(usize, usize)>);

/// The event this player picked to race next (D-057), as an index into the
/// race's events; none until they pick.
#[derive(Resource, Default)]
struct ChosenEvent(Option<usize>);

/// Between races, the player picks who drives and in what: left and right
/// (or a pad's d-pad) the driver, up and down the kart. On the grid the world
/// changes the kart at once; after a race, from the next one.
#[allow(clippy::too_many_arguments)]
fn choose(
    replica: Res<Replica>,
    outbox: Res<NetOutbox>,
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    menu: Res<Menu>,
    mut chosen: ResMut<Chosen>,
    mut chosen_event: ResMut<ChosenEvent>,
    mut kept: ResMut<Kept>,
    photo: Res<crate::photo::Photo>,
    show: Res<crate::show::Show>,
) {
    let (Some(mine), Some(choice_type), Some(roster)) = (replica.mine, replica.choice_type, replica.roster()) else {
        return;
    };
    // The photo mode's keys, or the whole race shown again's (D-082).
    if photo.is_on() || show.watching() {
        return;
    }
    // Once a connection: the driver, kart and paint the player raced with
    // last (D-064).
    if kept.sent != replica.connections {
        kept.sent = replica.connections;
        let _ = outbox.0.send(intent(0, mine, choice_type, &kept.choice()));
    }
    let race = replica.race().unwrap_or_default();
    let between = matches!(race.phase(), Phase::Waiting | Phase::Ready | Phase::Finished);
    if !between || menu.0 || roster.drivers.is_empty() || roster.karts.is_empty() {
        return;
    }
    // T: the next event, a cup or a track alone; the host's is raced, the
    // others' are votes.
    if keys.just_pressed(KeyCode::KeyT) && !race.events.is_empty() {
        let now = chosen_event.0.unwrap_or_else(|| race.events.iter().position(|e| *e == race.event).unwrap_or(0));
        let next = (now + 1) % race.events.len();
        chosen_event.0 = Some(next);
        let choice = proto::Choice { event: race.events[next].clone(), ..default() };
        let _ = outbox.0.send(intent(0, mine, choice_type, &choice));
    }
    let pressed = |key: KeyCode, button: GamepadButton| {
        keys.just_pressed(key) || gamepads.iter().any(|pad| pad.just_pressed(button))
    };
    // P: the next paint the player has won (D-064).
    if pressed(KeyCode::KeyP, GamepadButton::West) {
        kept.profile.paint = kept.profile.next_paint();
        kept.save();
        let choice = proto::Choice { paint: Some(kept.choice().paint.unwrap_or(0)), ..default() };
        let _ = outbox.0.send(intent(0, mine, choice_type, &choice));
    }
    let by = |forward: bool, back: bool| i64::from(forward) - i64::from(back);
    let driver = by(
        pressed(KeyCode::ArrowRight, GamepadButton::DPadRight),
        pressed(KeyCode::ArrowLeft, GamepadButton::DPadLeft),
    );
    let kart =
        by(pressed(KeyCode::ArrowDown, GamepadButton::DPadDown), pressed(KeyCode::ArrowUp, GamepadButton::DPadUp));
    if driver == 0 && kart == 0 {
        return;
    }
    let (now_driver, now_kart) = chosen.0.unwrap_or_else(|| {
        let racer = replica.racer(mine).unwrap_or_default();
        let at = |ids: Vec<&str>, id: &str| ids.iter().position(|i| *i == id).unwrap_or(0);
        (
            at(roster.drivers.iter().map(|d| d.id.as_str()).collect(), &racer.driver),
            at(roster.karts.iter().map(|k| k.id.as_str()).collect(), &racer.kart),
        )
    });
    let turn = |at: usize, by: i64, of: usize| (at as i64 + by).rem_euclid(of as i64) as usize;
    let picked = (turn(now_driver, driver, roster.drivers.len()), turn(now_kart, kart, roster.karts.len()));
    chosen.0 = Some(picked);
    let choice = proto::Choice {
        driver: roster.drivers[picked.0].id.clone(),
        kart: roster.karts[picked.1].id.clone(),
        ..default()
    };
    (kept.profile.driver, kept.profile.kart) = (choice.driver.clone(), choice.kart.clone());
    kept.save();
    let _ = outbox.0.send(intent(0, mine, choice_type, &choice));
}

/// A track's name from its id; the id when it is not built in.
fn track_name(id: &str) -> String {
    osage_kart_tracks::by_id(id).map_or_else(|| id.to_owned(), |track| track.name)
}

/// An event as the HUD names it: a cup by its name, a track alone by its,
/// a time trial by its track's.
fn event_name(event: &str) -> String {
    if let Some(id) = event.strip_prefix("cup:") {
        let name = osage_kart_cup::built_in().into_iter().find(|c| c.id == id).map_or(id.to_owned(), |c| c.name);
        format!("{name} (cup)")
    } else if let Some(id) = event.strip_prefix("trial:") {
        format!("{} (time trial)", track_name(id))
    } else if let Some(id) = event.strip_prefix("battle:") {
        let name = osage_kart_tracks::arena(id).map_or_else(|| id.to_owned(), |arena| arena.name);
        format!("{name} (battle)")
    } else {
        format!("{} (one track)", track_name(event.strip_prefix("track:").unwrap_or(event)))
    }
}

/// The cup and its standings, and what is raced next, as the HUD says them
/// between races (D-057).
fn cup_lines(
    race: Option<proto::Race>,
    me: Option<String>,
    picked: Option<usize>,
    mine: Option<u64>,
) -> (String, String) {
    let Some(race) = race else { return (String::new(), String::new()) };
    let cup = race.cup.as_ref().map_or_else(String::new, |cup| {
        let races = cup.tracks.len();
        let over = cup.race as usize > races;
        let head = if over {
            format!("{} - FINAL STANDINGS", cup.name.to_uppercase())
        } else {
            let track = cup.tracks.get(cup.race as usize - 1).map(|id| track_name(id)).unwrap_or_default();
            format!("{} - race {} of {races}: {track}", cup.name.to_uppercase(), cup.race)
        };
        let rows = cup.standings.iter().take(8).enumerate().map(|(i, s)| {
            let mark = if me.as_ref() == Some(&s.name) { ">" } else { " " };
            let places: Vec<String> =
                s.places.iter().map(|p| if *p == 0 { "-".to_owned() } else { p.to_string() }).collect();
            let podium = match (over, i) {
                (true, 0) => "  WINNER",
                (true, 1 | 2) => "  podium",
                _ => "",
            };
            format!("{mark}{:>2} {:<14.14} {:>3} pts  {}{podium}", i + 1, s.name, s.points, places.join(" "))
        });
        std::iter::once(head).chain(rows).collect::<Vec<_>>().join("\n")
    });
    let hosting = mine.is_some_and(|mine| mine == race.host);
    let shown = picked.and_then(|i| race.events.get(i)).unwrap_or(&race.event);
    let votes: Vec<String> = race.votes.iter().map(|v| format!("{} {}", event_name(&v.event), v.drivers)).collect();
    let mut event = if race.events.is_empty() {
        String::new()
    } else if hosting || race.host == 0 {
        format!("Next: {}", event_name(shown))
    } else {
        format!("Next: {}\nYour vote: {}", event_name(&race.event), event_name(shown))
    };
    if !votes.is_empty() && !event.is_empty() {
        event.push_str(&format!("\nVotes: {}", votes.join(", ")));
    }
    (cup, event)
}

/// Who drives the player's kart and in what, as the HUD says it.
fn pick(roster: Option<proto::Roster>, racer: Option<&proto::Racer>, chosen: Option<(usize, usize)>) -> String {
    let (Some(roster), Some(racer)) = (roster, racer) else { return String::new() };
    let driver = chosen
        .and_then(|(d, _)| roster.drivers.get(d))
        .or_else(|| roster.drivers.iter().find(|d| d.id == racer.driver));
    let kart =
        chosen.and_then(|(_, k)| roster.karts.get(k)).or_else(|| roster.karts.iter().find(|k| k.id == racer.kart));
    let (Some(driver), Some(kart)) = (driver, kart) else { return String::new() };
    format!("Driver: {} ({}) – {}\nKart: {} – {}", driver.name, driver.class, driver.about, kart.name, kart.about)
}

/// The frame's steps to the world, in one message.
#[allow(clippy::too_many_arguments)]
fn send(
    replica: Res<Replica>,
    outbox: Res<NetOutbox>,
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    menu: Res<Menu>,
    mut wanted: ResMut<CallWanted>,
    mut driver: ResMut<Driver>,
    second: Option<ResMut<Second>>,
) {
    // The second player's steps, through their own connection (D-086);
    // the second player may be the host, whom the world lets start the race.
    if let Some(mut second) = second {
        let fresh = std::mem::take(&mut second.driver.fresh);
        if let (Some(mine), Some(controls_type)) = (second.mine, second.controls_type) {
            let race = replica.race();
            let hosting = race.as_ref().is_some_and(|race| race.phase() == Phase::Ready && race.host == mine);
            let pressed =
                keys.just_pressed(KeyCode::Enter) || gamepads.iter().any(|pad| pad.just_pressed(GamepadButton::North));
            if hosting && (pressed || driver.autodrive) && !menu.0 && second.driver.calls_left == 0 {
                (second.driver.call, second.driver.calls_left) = (proto::RaceCall::Start, CALL_SENDS);
            }
            let mut steps = second.driver.predictor.to_send(fresh);
            if second.driver.calls_left > 0 && fresh > 0 {
                second.driver.calls_left -= 1;
                if let Some(last) = steps.last_mut() {
                    last.call = second.driver.call.into();
                }
            }
            if let (true, Some(last)) = (fresh > 0, steps.last()) {
                let message = intent(last.sequence, mine, controls_type, &proto::Controls { steps: steps.clone() });
                let _ = second.outbox.send(message);
            }
        }
    }
    let fresh = std::mem::take(&mut driver.fresh);
    let (Some(mine), Some(controls_type)) = (replica.mine, replica.controls_type) else { return };
    // On the grid, the host starts the race: Enter, or a gamepad's North.
    let race = replica.race();
    let hosting = race.as_ref().is_some_and(|race| race.phase() == Phase::Ready && race.host == mine);
    let pressed =
        keys.just_pressed(KeyCode::Enter) || gamepads.iter().any(|pad| pad.just_pressed(GamepadButton::North));
    // Driven by the autopilot (a demonstration, a screenshot), the host starts at once.
    if hosting && (pressed || driver.autodrive) && !menu.0 {
        wanted.0 = Some(proto::RaceCall::Start);
    }
    if let Some(call) = wanted.0.take() {
        (driver.call, driver.calls_left) = (call, CALL_SENDS);
    }
    let mut steps = driver.predictor.to_send(fresh);
    if driver.calls_left > 0 && fresh > 0 {
        driver.calls_left -= 1;
        if let Some(last) = steps.last_mut() {
            last.call = driver.call.into();
        }
    }
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
    mut second: Option<ResMut<Second>>,
    mut karts: Query<(&Remote, &mut Pose)>,
) {
    let Some(world) = &replica.world else { return };
    driver.predictor.fade(time.delta_secs());
    if let Some(second) = second.as_deref_mut() {
        second.driver.predictor.fade(time.delta_secs());
    }
    let glide = 1.0 - (-time.delta_secs() * 14.0).exp();
    // The others are drawn where they are at the moment the player's kart
    // is (D-076): the world's state stepped on, with the controls of its
    // last step, by as many steps as the prediction is ahead; what changes
    // from one state to the next is eased in.
    let ahead = if replica.mine.is_some() { driver.predictor.steps_ahead().min(30) } else { 0 };
    for (remote, mut pose) in &mut karts {
        let Some(state) = world.get::<proto::KartState>(remote.0, types::STATE) else { continue };
        let server = kart_of(&state);
        let theirs = second.as_deref().filter(|second| second.mine == Some(remote.0));
        let predicted = match theirs {
            // The second player's kart, from their prediction (D-086).
            Some(second) if !replica.finished(remote.0) => second.driver.predictor.shown(fixed.overstep_fraction()),
            _ => (replica.mine == Some(remote.0) && !replica.finished(remote.0))
                .then(|| driver.predictor.shown(fixed.overstep_fraction()))
                .flatten(),
        };
        let racer = replica.racer(remote.0);
        let there = match (&replica.circuit, racer.as_ref().is_some_and(|r| r.ghost)) {
            (Some(circuit), false) => {
                let built = Kart { build: racer.as_ref().map_or(server.build, osage_world_kart::build_of), ..server };
                crate::predict::stepped_on(built, controls_of(&state), ahead, &circuit.track)
            }
            _ => server,
        };
        let far = (there.x - pose.kart.x).hypot(there.z - pose.kart.z) > 6.0;
        pose.kart = predicted.unwrap_or(Kart {
            x: if far { there.x } else { pose.kart.x + (there.x - pose.kart.x) * glide },
            z: if far { there.z } else { pose.kart.z + (there.z - pose.kart.z) * glide },
            heading: pose.kart.heading + wrap(there.heading - pose.kart.heading) * glide,
            ..there
        });
        let racer = replica.racer(remote.0);
        pose.held = racer.as_ref().and_then(|racer| item(racer.held()));
        pose.trailing = racer.as_ref().is_some_and(|racer| racer.trailing);
        pose.guard = racer.as_ref().map_or(0.0, |racer| racer.guard);
        pose.smoke = racer.map(|racer| racer.smoke).filter(|colour| *colour != 0);
    }
}

/// The camera follows this player's kart, or the leader's when watching;
/// the second player's camera theirs (D-086).
fn follow_someone(mut commands: Commands, mut replica: ResMut<Replica>, second: Option<ResMut<Second>>) {
    if let Some(mut second) = second {
        let wanted = second.mine.and_then(|kart| replica.karts.get(&kart).copied());
        if wanted != second.followed {
            if let Some(old) = second.followed.filter(|old| replica.karts.values().any(|k| k == old)) {
                commands.entity(old).remove::<crate::scene::FollowedSecond>();
            }
            if let Some(new) = wanted {
                commands.entity(new).insert(crate::scene::FollowedSecond);
            }
            second.followed = wanted;
        }
    }
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
/// Playing without a window (`--headless` with `--screenshot-after`): the
/// pictures are taken of the off-screen picture, the scene without the
/// interface.
#[derive(Resource)]
pub struct OffScreen;

fn screenshots(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut shots: ResMut<Shots>,
    off_screen: Option<Res<OffScreen>>,
    picture: Option<Res<osage_render::resolution::Picture>>,
) {
    let elapsed = time.elapsed_secs();
    let before = shots.after.len();
    shots.after.retain(|after| elapsed < *after);
    let due = shots.after.len() < before;
    if due || keys.just_pressed(KeyCode::F12) {
        let name = format!("kart-native-{}.png", now_us() / 1_000);
        let shot = match (off_screen, picture) {
            (Some(_), Some(picture)) => Screenshot::image(picture.image.clone()),
            _ => Screenshot::primary_window(),
        };
        commands.spawn(shot).observe(save_to_disk(name));
    }
}

/// What the world's machine offers beside the world, on the ports the
/// world announces (0: none): the host's shared music (D-059), the voice
/// relay; and the player's name there.
#[derive(Resource, Debug, Clone, PartialEq, Default)]
pub struct Host {
    pub host: String,
    pub music: u16,
    pub voice: u16,
    /// The player's driver's name, once the world has given one.
    pub me: String,
    /// Whether the world has said (its race is known).
    pub known: bool,
}

/// The machine in a world's address (`ws://192.168.0.100:7878`, `[::1]`).
fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?']).next().unwrap_or(rest);
    let authority = authority.rsplit_once('@').map_or(authority, |(_, a)| a);
    if let Some(inside) = authority.strip_prefix('[') {
        return inside.split(']').next().unwrap_or(inside).to_owned();
    }
    authority.split(':').next().unwrap_or(authority).to_owned()
}

/// What the interface shows this frame, from what the world sent.
#[allow(clippy::too_many_arguments)]
fn summarise(
    replica: Res<Replica>,
    mut beside: ResMut<Host>,
    driver: Res<Driver>,
    chosen: Res<Chosen>,
    chosen_event: Res<ChosenEvent>,
    mut kept: ResMut<Kept>,
    mut rain: ResMut<crate::rain::Rain>,
    mut night: ResMut<crate::scene::Night>,
    karts: Query<(&Remote, &Pose)>,
    mut view: ResMut<RaceView>,
    mut paddock: ResMut<PaddockView>,
    second: Option<Res<Second>>,
) {
    if let Some(now) = replica.paddock().filter(|now| *now != paddock.paddock) {
        paddock.paddock = now;
    }
    let (lead, me) = (driver.predictor.lead(), driver.predictor.kart.filter(|_| replica.mine.is_some()));
    if paddock.lead != lead || paddock.me != me {
        paddock.lead = lead;
        paddock.me = me;
    }
    let port = |port: u32| port.min(u32::from(u16::MAX)) as u16;
    let (music, voice) = replica.race().map_or((0, 0), |race| (port(race.music_port), port(race.voice_port)));
    let known = replica.race().is_some();
    let racer = replica.mine.and_then(|mine| replica.racer(mine));
    let me = racer.as_ref().map_or("", |racer| racer.name.as_str());
    if beside.music != music || beside.voice != voice || beside.known != known || beside.me != me {
        (beside.music, beside.voice, beside.known, beside.me) = (music, voice, known, me.to_owned());
    }
    view.held = racer.as_ref().and_then(|racer| item(racer.held()));
    view.count = racer.as_ref().map_or(0, |racer| racer.held_count.min(9) as u8);
    view.trailing = racer.as_ref().is_some_and(|racer| racer.trailing);
    view.guard = racer.as_ref().map_or(0.0, |racer| racer.guard);
    kept.count_cup(replica.race().as_ref(), me);
    view.pick = pick(replica.roster(), racer.as_ref(), chosen.0);
    if !view.pick.is_empty() {
        view.pick.push('\n');
        view.pick.push_str(&kept.line());
    }
    let (cup, event) = cup_lines(replica.race(), racer.as_ref().map(|r| r.name.clone()), chosen_event.0, replica.mine);
    if view.cup != cup || view.event != event {
        (view.cup, view.event) = (cup, event);
    }
    let race = replica.race();
    let now = replica.world_time();
    let seconds_left = |race: &proto::Race| ((race.phase_ends_us as i64 - now) as f32 / 1e6).ceil() as i64;
    view.status.clone_from(&replica.status);
    view.laps = race.as_ref().map_or(0, |race| race.laps);
    view.trial = race.as_ref().is_some_and(|race| race.trial);
    // The race's weather (D-068): seen here, predicted in `receive`.
    let raining = race.as_ref().is_some_and(|race| race.rain);
    if rain.falling != raining {
        rain.falling = raining;
    }
    // And the time of day (D-085).
    let dark = race.as_ref().is_some_and(|race| race.night);
    if night.0 != dark {
        night.0 = dark;
    }
    view.battle = race.as_ref().is_some_and(|race| race.battle);
    view.clock_ms = race.as_ref().map_or(0, |race| match race.phase() {
        // A battle counts down to when time is up (D-065).
        Phase::Racing if race.battle => ((race.phase_ends_us as i64 - now) / 1000).max(0),
        _ => (now - race.started_us as i64) / 1000,
    });
    view.stage = match &race {
        None => Lights::Off,
        Some(race) => match race.phase() {
            Phase::Waiting | Phase::Unspecified => Lights::Waiting,
            Phase::Ready => Lights::Ready(replica.mine.is_some_and(|mine| mine == race.host)),
            Phase::Countdown => Lights::Count(seconds_left(race)),
            Phase::Racing if view.clock_ms < 1000 => Lights::Go,
            Phase::Racing => Lights::Racing,
            Phase::Finished => Lights::Results(seconds_left(race)),
        },
    };
    let race_host = race.as_ref().map(|race| race.host).filter(|host| *host != 0);
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
                ghost: racer.ghost,
                lives: racer.lives,
                out: racer.out,
                color: racer.color,
                position: racer.position,
                lap: racer.lap,
                finish_ms: racer.finish_ms,
                at: Vec2::new(pose.kart.x, pose.kart.z),
            })
        })
        .collect();
    view.me = replica.mine.and_then(|mine| view.karts.iter().position(|kart| kart.id == mine));
    // The second player's line (D-086).
    view.second = second.as_deref().map(|second| {
        let theirs = second.mine.and_then(|mine| replica.racer(mine));
        crate::hud::SecondLine {
            position: theirs.as_ref().map_or(0, |racer| racer.position),
            lap: theirs.as_ref().map_or(0, |racer| racer.lap),
            finish_ms: theirs.as_ref().map_or(0, |racer| racer.finish_ms),
            speed: second.driver.predictor.kart.map_or(0.0, |kart| kart.speed),
            held: theirs.as_ref().and_then(|racer| item(racer.held())),
            count: theirs.as_ref().map_or(0, |racer| racer.held_count.min(9) as u8),
        }
    });
    let host = race_host.and_then(|host| view.karts.iter().find(|kart| kart.id == host)).map(|kart| kart.name.clone());
    view.host = host.unwrap_or_default();
    view.alone = view.me.is_some() && view.karts.iter().filter(|kart| !kart.bot).count() == 1;
    let my_name = racer.as_ref().map(|racer| racer.name.clone());
    let records = replica.records().unwrap_or_default();
    let bests = |list: &[proto::Record]| {
        list.iter()
            .map(|r| Best { name: r.name.clone(), ms: r.ms, fresh: r.fresh, mine: my_name.as_ref() == Some(&r.name) })
            .collect()
    };
    let records = Records {
        track: records.track.clone(),
        race_laps: records.race_laps,
        laps: bests(&records.laps),
        races: bests(&records.races),
        trials: bests(&records.trials),
    };
    if view.records != records {
        view.records = records;
    }
    let kart = driver.predictor.kart.unwrap_or_default();
    (view.speed, view.charge, view.boosting) = (kart.speed, kart.charge, kart.boost > 0.0);
    view.stalled = kart.stall > 0.0;
}

#[cfg(test)]
mod tests {
    use super::{ButtonInput, KeyCode, Seat, controls_for, host_of};

    #[test]
    fn on_a_split_screen_each_player_has_their_own_keys() {
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::KeyW);
        keys.press(KeyCode::ArrowLeft);
        keys.press(KeyCode::ControlRight);
        let (mut first, mut second) = (0.0, 0.0);
        let (one, one_item) = controls_for(Seat::First, true, &keys, None, &mut first, 1.0);
        let (two, two_item) = controls_for(Seat::Second, true, &keys, None, &mut second, 1.0);
        assert_eq!((one.throttle, one.steer, one_item), (1.0, 0.0, false), "W is the first's");
        assert_eq!((two.throttle, two.steer, two_item), (0.0, -1.0, true), "the arrows and right Ctrl the second's");
        // Alone, the player has every key.
        let mut alone = 0.0;
        let (all, item) = controls_for(Seat::First, false, &keys, None, &mut alone, 1.0);
        assert_eq!((all.throttle, all.steer, item), (1.0, -1.0, true));
    }

    #[test]
    fn the_worlds_machine_is_read_from_its_address() {
        assert_eq!(host_of("ws://192.168.0.100:7878"), "192.168.0.100");
        assert_eq!(host_of("wss://kart.example.org/world?x=1"), "kart.example.org");
        assert_eq!(host_of("ws://[fe80::1]:7878"), "fe80::1");
        assert_eq!(host_of("localhost:7878"), "localhost");
    }
}
