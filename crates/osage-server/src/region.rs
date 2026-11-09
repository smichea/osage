// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Region server: authoritative simulation of one area as a headless Bevy app.
//! It replicates every entity to every session; interest management by cells
//! comes with the second region.

use std::{
    collections::{BTreeMap, HashMap},
    time::Duration,
};

use bevy::{app::ScheduleRunnerPlugin, prelude::*};
use crossbeam_channel::Receiver;
use osage_proto::{
    components, math, types,
    v0::{
        ActionResult, ActionStatus, Component as WireComponent, ComponentChanged, DeviceState, EntityComponent,
        EntityEnter, EntityLeave, Intent, LeaveReason, Role, ServerMessage, StateDatagram, Tier, TimeSync,
        client_message, server_message,
    },
};
use prost::Message as _;
use tokio::sync::mpsc::UnboundedSender;

use crate::{identity::SessionKind, now_us, schema};

/// Avatar walking speed, metres per second.
const AVATAR_SPEED: f32 = 4.0;
/// An avatar stops when its controller has sent no intent for this long.
const INTENT_LIFETIME: Duration = Duration::from_millis(500);
/// Avatars stay inside this square around the origin, metres.
const WORLD_HALF_SIZE: f32 = 20.0;
/// Component types a bridge may publish on its devices.
const BRIDGE_TYPES: [u32; 4] = [schema::DEVICE, schema::MEASUREMENTS, schema::TRANSFORM, schema::CONTENT_REF];

/// Who a session is, as far as the region needs to know.
pub struct Member {
    pub kind: SessionKind,
    pub role: Role,
    pub controller: components::Controller,
    /// For a bridge: the per-world identifier of the devices' owner.
    pub owner: String,
}

/// What the gateway tells the region.
pub enum RegionInput {
    Join { session: u64, member: Member, out: UnboundedSender<ServerMessage> },
    Leave { session: u64 },
    Message { session: u64, message: client_message::Message },
}

pub fn app(inbox: Receiver<RegionInput>, tick_hz: u32) -> App {
    let tick = Duration::from_secs_f64(1.0 / f64::from(tick_hz.max(1)));
    let mut app = App::new();
    app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(tick)))
        .insert_resource(Inbox(inbox))
        .init_resource::<Sessions>()
        .init_resource::<Devices>()
        .init_resource::<EntityIds>()
        .init_resource::<Tick>()
        .add_systems(Startup, spawn_demo)
        .add_systems(Update, (receive, orbit, walk, broadcast_state).chain());
    app
}

#[derive(Resource)]
struct Inbox(Receiver<RegionInput>);

struct Connected {
    out: UnboundedSender<ServerMessage>,
    member: Member,
}

#[derive(Resource, Default)]
struct Sessions(HashMap<u64, Connected>);

impl Sessions {
    fn broadcast(&self, message: &ServerMessage) {
        for session in self.0.values() {
            let _ = session.out.send(message.clone());
        }
    }

    fn reply(&self, session: u64, message: server_message::Message) {
        if let Some(connected) = self.0.get(&session) {
            let _ = connected.out.send(ServerMessage { message: Some(message) });
        }
    }
}

/// Devices published by bridges: the region's copy of their components,
/// keyed by (bridge session, the bridge's local id).
#[derive(Resource, Default)]
struct Devices(HashMap<(u64, String), Published>);

struct Published {
    entity: Entity,
    id: u64,
    components: BTreeMap<u32, Vec<u8>>,
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

/// Discrete (reliable) components of an entity, encoded, by type id.
#[derive(Component, Default)]
struct Wire(BTreeMap<u32, Vec<u8>>);

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

fn builtin(uri: &str) -> components::ContentRef {
    components::ContentRef {
        uri: uri.to_owned(),
        kind: components::ContentKind::Builtin.into(),
        tiers: vec![Tier::T0Headless.into()],
        ..default()
    }
}

fn spawn_demo(mut commands: Commands, mut ids: ResMut<EntityIds>) {
    let system = components::Controller { kind: components::ControllerKind::System.into(), ..default() };
    let wire = BTreeMap::from([
        (schema::CONTENT_REF, builtin(types::BUILTIN_CUBE).encode_to_vec()),
        (schema::CONTROLLER, system.encode_to_vec()),
    ]);
    commands.spawn((Replicated(ids.next()), Transform::default(), Wire(wire), Orbit { radius: 3.0, speed: 0.8 }));
}

#[allow(clippy::too_many_arguments)]
fn receive(
    mut commands: Commands,
    time: Res<Time>,
    inbox: Res<Inbox>,
    mut sessions: ResMut<Sessions>,
    mut devices: ResMut<Devices>,
    mut ids: ResMut<EntityIds>,
    entities: Query<(&Replicated, &Transform, &Wire)>,
    mut owned: Query<(Entity, &Replicated, &Owner, &mut MoveInput)>,
) {
    for input in inbox.0.try_iter() {
        match input {
            RegionInput::Join { session, member, out } => {
                for (id, transform, wire) in &entities {
                    let _ = out.send(enter(id.0, transform, &wire.0));
                }
                for device in devices.0.values() {
                    let _ = out.send(enter(device.id, &device_transform(&device.components), &device.components));
                }
                let avatar = member.role == Role::Player || member.kind == SessionKind::Agent;
                let controller = member.controller.clone();
                sessions.0.insert(session, Connected { out, member });
                if avatar {
                    let id = ids.next();
                    let transform = spawn_point(session);
                    let wire = BTreeMap::from([
                        (schema::CONTENT_REF, builtin(types::BUILTIN_AVATAR).encode_to_vec()),
                        (schema::CONTROLLER, controller.encode_to_vec()),
                    ]);
                    sessions.broadcast(&enter(id, &transform, &wire));
                    commands.spawn((Replicated(id), transform, Wire(wire), Owner(session), MoveInput::default()));
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
                devices.0.retain(|(bridge, _), device| {
                    if *bridge != session {
                        return true;
                    }
                    commands.entity(device.entity).despawn();
                    sessions.broadcast(&leave(device.id, LeaveReason::Destroyed));
                    false
                });
            }
            RegionInput::Message { session, message } => match message {
                client_message::Message::Intent(intent) => apply_intent(session, &intent, &time, &mut owned),
                client_message::Message::DeviceState(state) => {
                    publish_device(session, state, &mut commands, &sessions, &mut devices, &mut ids)
                }
                client_message::Message::TimeSync(sync) => sessions
                    .reply(session, server_message::Message::TimeSync(TimeSync { server_time_us: now_us(), ..sync })),
                client_message::Message::ActionRequest(request) => sessions.reply(
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
    let Ok(move_intent) = components::MoveIntent::decode(input.value.as_slice()) else { return };
    for (_, id, owner, mut state) in owned.iter_mut() {
        if id.0 == intent.entity && owner.0 == session && intent.sequence > state.sequence {
            let d = move_intent.direction.unwrap_or_default();
            state.direction = Vec3::new(d.x, 0.0, d.z).clamp_length_max(1.0);
            state.sequence = intent.sequence;
            state.received = time.elapsed();
        }
    }
}

/// Mirrors a bridge's device (design/02 M1). Only bridges may publish, only
/// device component types are accepted, and the owner and bridge recorded in
/// `osage.device/1` are the authenticated ones, whatever the bridge claims.
fn publish_device(
    session: u64,
    state: DeviceState,
    commands: &mut Commands,
    sessions: &Sessions,
    devices: &mut Devices,
    ids: &mut EntityIds,
) {
    let Some(bridge) = sessions.0.get(&session).filter(|c| c.member.kind == SessionKind::Bridge) else { return };
    let key = (session, state.local_id.clone());
    if state.removed {
        if let Some(device) = devices.0.remove(&key) {
            commands.entity(device.entity).despawn();
            sessions.broadcast(&leave(device.id, LeaveReason::Destroyed));
        }
        return;
    }

    let mut incoming = BTreeMap::new();
    for component in state.components.into_iter().filter(|c| BRIDGE_TYPES.contains(&c.r#type)) {
        let mut value = component.value;
        if component.r#type == schema::DEVICE {
            let Ok(mut device) = components::Device::decode(value.as_slice()) else { continue };
            device.owner_per_world_id = bridge.member.owner.clone();
            device.bridge_per_world_id = bridge.member.controller.per_world_id.clone();
            value = device.encode_to_vec();
        }
        incoming.insert(component.r#type, value);
    }

    match devices.0.get_mut(&key) {
        Some(device) => {
            for (type_id, value) in incoming {
                if device.components.get(&type_id) == Some(&value) {
                    continue;
                }
                device.components.insert(type_id, value.clone());
                if type_id != schema::TRANSFORM {
                    sessions.broadcast(&changed(device.id, type_id, value));
                }
            }
            commands
                .entity(device.entity)
                .insert((device_transform(&device.components), Wire(discrete(&device.components))));
        }
        None => {
            incoming.entry(schema::CONTENT_REF).or_insert_with(|| builtin(types::BUILTIN_DEVICE).encode_to_vec());
            let id = ids.next();
            let transform = device_transform(&incoming);
            sessions.broadcast(&enter(id, &transform, &discrete(&incoming)));
            let entity = commands.spawn((Replicated(id), transform, Wire(discrete(&incoming)))).id();
            devices.0.insert(key, Published { entity, id, components: incoming });
        }
    }
}

/// A device's components without its transform, which travels separately.
fn discrete(components: &BTreeMap<u32, Vec<u8>>) -> BTreeMap<u32, Vec<u8>> {
    components.iter().filter(|(t, _)| **t != schema::TRANSFORM).map(|(t, v)| (*t, v.clone())).collect()
}

fn device_transform(components: &BTreeMap<u32, Vec<u8>>) -> Transform {
    components
        .get(&schema::TRANSFORM)
        .and_then(|bytes| components::Transform::decode(bytes.as_slice()).ok())
        .map_or_else(Transform::default, |t| from_proto(&t))
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
            component: Some(wire(schema::TRANSFORM, to_proto(transform).encode_to_vec())),
        })
        .collect();
    if values.is_empty() {
        return;
    }
    sessions.broadcast(&ServerMessage {
        message: Some(server_message::Message::State(StateDatagram { tick: tick.0, server_time_us: now_us(), values })),
    });
}

/// Avatars appear on a ring around the origin, one slot per session.
fn spawn_point(session: u64) -> Transform {
    let angle = session as f32 * 2.399_963; // golden angle, radians
    Transform::from_xyz(6.0 * angle.cos(), 0.0, 6.0 * angle.sin())
}

fn enter(id: u64, transform: &Transform, discrete: &BTreeMap<u32, Vec<u8>>) -> ServerMessage {
    let mut components = vec![wire(schema::TRANSFORM, to_proto(transform).encode_to_vec())];
    components.extend(discrete.iter().map(|(type_id, value)| wire(*type_id, value.clone())));
    ServerMessage { message: Some(server_message::Message::EntityEnter(EntityEnter { entity: id, components })) }
}

fn changed(id: u64, type_id: u32, value: Vec<u8>) -> ServerMessage {
    ServerMessage {
        message: Some(server_message::Message::ComponentChanged(ComponentChanged {
            entity: id,
            component: Some(wire(type_id, value)),
            removed: false,
        })),
    }
}

fn leave(id: u64, reason: LeaveReason) -> ServerMessage {
    ServerMessage {
        message: Some(server_message::Message::EntityLeave(EntityLeave { entity: id, reason: reason.into() })),
    }
}

fn wire(type_id: u32, value: Vec<u8>) -> WireComponent {
    WireComponent { r#type: type_id, value }
}

fn to_proto(transform: &Transform) -> components::Transform {
    let vec3 = |v: Vec3| math::Vec3 { x: v.x, y: v.y, z: v.z };
    let q = transform.rotation;
    components::Transform {
        translation: Some(vec3(transform.translation)),
        rotation: Some(math::Quat { x: q.x, y: q.y, z: q.z, w: q.w }),
        scale: Some(vec3(transform.scale)),
    }
}

fn from_proto(t: &components::Transform) -> Transform {
    let vec3 = |v: Option<math::Vec3>, default: Vec3| v.map_or(default, |v| Vec3::new(v.x, v.y, v.z));
    Transform {
        translation: vec3(t.translation, Vec3::ZERO),
        rotation: t.rotation.map_or(Quat::IDENTITY, |q| Quat::from_xyzw(q.x, q.y, q.z, q.w)),
        scale: vec3(t.scale, Vec3::ONE),
    }
}
