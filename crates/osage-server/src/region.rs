// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Region server: authoritative simulation of one area as a headless Bevy app.
//! The region provides the mechanisms — sessions, replication, intents routed
//! to their entity's owner, devices — and the world module the rules
//! ([`crate::world`]). It replicates every entity to every session; interest
//! management by cells comes with the second region.

use std::{
    collections::{BTreeMap, HashMap},
    time::Duration,
};

use bevy::{app::ScheduleRunnerPlugin, prelude::*};
use crossbeam_channel::Receiver;
use osage_proto::{
    actions, components, types,
    v0::{
        ActionRequest, ActionResult, ActionStatus, Component as WireComponent, ComponentChanged, DeviceCommand,
        DeviceState, EntityComponent, EntityEnter, EntityLeave, LeaveReason, Role, ServerMessage, StateDatagram,
        TimeSync, client_message, server_message,
    },
};
use prost::Message as _;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    identity::SessionKind,
    now_us,
    schema::{self, Schema},
    world::{
        Discrete, EntityIds, IntentReceived, Joined, Left, Owner, RegionSet, Replicated, Tick, Volatile, WorldModule,
        builtin, transform_from_proto, transform_to_proto,
    },
};

/// A relayed device command without an answer after this long has an
/// unknown outcome.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
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

pub fn app(inbox: Receiver<RegionInput>, tick_hz: u32, schema: Schema, world: impl WorldModule) -> App {
    let tick = Duration::from_secs_f64(1.0 / f64::from(tick_hz.max(1)));
    let mut app = App::new();
    app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(tick)))
        .insert_resource(Inbox(inbox))
        .insert_resource(schema)
        .init_resource::<Sessions>()
        .init_resource::<Devices>()
        .init_resource::<PendingCommands>()
        .init_resource::<EntityIds>()
        .init_resource::<Index>()
        .init_resource::<Tick>()
        .insert_resource(Keyframe(u64::from(tick_hz.max(1))))
        .add_message::<Joined>()
        .add_message::<Left>()
        .add_message::<IntentReceived>()
        .configure_sets(Update, (RegionSet::Receive, RegionSet::World, RegionSet::Replicate).chain())
        .add_systems(Update, (receive, expire_commands).chain().in_set(RegionSet::Receive))
        .add_systems(
            Update,
            (despawn_orphans, announce, retire, publish_changes, broadcast_state).chain().in_set(RegionSet::Replicate),
        )
        .add_plugins(world);
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

/// Replicated entities clients have been told about, both ways.
#[derive(Resource, Default)]
struct Index {
    ids: HashMap<Entity, u64>,
    entities: HashMap<u64, Entity>,
}

/// The discrete values clients have, to send only what changes.
#[derive(Component, Default)]
struct DiscreteSent(BTreeMap<u32, Vec<u8>>);

/// Devices published by bridges: the region's copy of their components,
/// keyed by (bridge session, the bridge's local id).
#[derive(Resource, Default)]
struct Devices(HashMap<(u64, String), Published>);

struct Published {
    entity: Entity,
    id: u64,
    components: BTreeMap<u32, Vec<u8>>,
}

/// Device commands relayed to bridges and not answered yet, by command id.
/// Ids only grow, so a late answer can never match a newer command.
#[derive(Resource, Default)]
struct PendingCommands(HashMap<u64, PendingCommand>, u64);

struct PendingCommand {
    requester: u64,
    request_id: u64,
    bridge: u64,
    deadline: Duration,
}

/// A replicated entity as clients first see it.
type Snapshot<'a> = (Entity, &'a Replicated, &'a Transform, Option<&'a Discrete>, Option<&'a Volatile>);

/// A replicated entity's volatile values, with their change ticks.
type Moving<'a> = (&'a Replicated, Ref<'a, Transform>, Option<Ref<'a, Volatile>>);

/// Every this many ticks (one second), a state carries every volatile value,
/// not only the changed ones: a lost datagram is repaired within a second.
#[derive(Resource)]
struct Keyframe(u64);

#[allow(clippy::too_many_arguments)]
fn receive(
    mut commands: Commands,
    time: Res<Time>,
    inbox: Res<Inbox>,
    index: Res<Index>,
    mut tick: ResMut<Tick>,
    mut sessions: ResMut<Sessions>,
    mut devices: ResMut<Devices>,
    mut commands_out: ResMut<PendingCommands>,
    mut ids: ResMut<EntityIds>,
    entities: Query<Snapshot>,
    owners: Query<&Owner>,
    (mut joined, mut left, mut intents): (MessageWriter<Joined>, MessageWriter<Left>, MessageWriter<IntentReceived>),
) {
    tick.0 += 1;
    for input in inbox.0.try_iter() {
        match input {
            RegionInput::Join { session, member, out } => {
                for (entity, id, transform, discrete, volatile) in &entities {
                    if index.ids.contains_key(&entity) {
                        let _ = out.send(enter(id.0, transform, discrete, volatile));
                    }
                }
                joined.write(Joined {
                    session,
                    kind: member.kind,
                    role: member.role,
                    controller: member.controller.clone(),
                });
                sessions.0.insert(session, Connected { out, member });
            }
            RegionInput::Leave { session } => {
                sessions.0.remove(&session);
                commands_out.0.retain(|_, pending| {
                    if pending.bridge != session {
                        return true;
                    }
                    let reason = "the bridge left before answering: reconcile, do not retry blindly";
                    sessions.reply(pending.requester, result(pending.request_id, ActionStatus::Unknown, reason));
                    false
                });
                // Its entities, devices included, go in `despawn_orphans`.
                devices.0.retain(|(bridge, _), _| *bridge != session);
                left.write(Left { session });
            }
            RegionInput::Message { session, message } => match message {
                // Only an entity's owner may send it intents; the world decides what they do.
                client_message::Message::Intent(intent) => {
                    let Some(input) = intent.input else { continue };
                    let Some(&entity) = index.entities.get(&intent.entity) else { continue };
                    if owners.get(entity).is_ok_and(|owner| owner.0 == session) {
                        intents.write(IntentReceived {
                            session,
                            entity,
                            sequence: intent.sequence,
                            type_id: input.r#type,
                            value: input.value,
                        });
                    }
                }
                client_message::Message::DeviceState(state) => {
                    publish_device(session, state, &mut commands, &sessions, &mut devices, &mut ids)
                }
                client_message::Message::TimeSync(sync) => sessions
                    .reply(session, server_message::Message::TimeSync(TimeSync { server_time_us: now_us(), ..sync })),
                client_message::Message::ActionRequest(request) if request.action == actions::DEVICE_SET => {
                    device_action(session, request, &time, &sessions, &devices, &mut commands_out)
                }
                client_message::Message::ActionRequest(request) => {
                    sessions.reply(session, result(request.request_id, ActionStatus::Rejected, "unknown action"))
                }
                // Only the bridge the command went to may answer it.
                client_message::Message::DeviceCommandResult(done)
                    if commands_out.0.get(&done.command_id).is_some_and(|p| p.bridge == session) =>
                {
                    let pending = commands_out.0.remove(&done.command_id).expect("checked");
                    sessions.reply(pending.requester, result(pending.request_id, done.status(), &done.reason));
                }
                _ => {}
            },
        }
    }
}

/// Entities whose owner left go with it — also those the world spawned for a
/// session that left in the same tick.
fn despawn_orphans(mut commands: Commands, sessions: Res<Sessions>, owned: Query<(Entity, &Owner)>) {
    for (entity, owner) in &owned {
        if !sessions.0.contains_key(&owner.0) {
            commands.entity(entity).despawn();
        }
    }
}

/// Tells every session about the entities that appeared this tick.
fn announce(
    mut commands: Commands,
    sessions: Res<Sessions>,
    mut index: ResMut<Index>,
    added: Query<Snapshot, Added<Replicated>>,
) {
    for (entity, id, transform, discrete, volatile) in &added {
        sessions.broadcast(&enter(id.0, transform, discrete, volatile));
        index.ids.insert(entity, id.0);
        index.entities.insert(id.0, entity);
        commands.entity(entity).insert(DiscreteSent(discrete.map(|d| d.0.clone()).unwrap_or_default()));
    }
}

/// Tells every session about the entities that are gone, however they went.
fn retire(sessions: Res<Sessions>, mut index: ResMut<Index>, mut removed: RemovedComponents<Replicated>) {
    for entity in removed.read() {
        if let Some(id) = index.ids.remove(&entity) {
            index.entities.remove(&id);
            sessions.broadcast(&leave(id, LeaveReason::Destroyed));
        }
    }
}

/// Sends the discrete values that changed since clients last had them.
fn publish_changes(
    sessions: Res<Sessions>,
    mut changed: Query<(&Replicated, &Discrete, &mut DiscreteSent), Changed<Discrete>>,
) {
    for (id, discrete, mut sent) in &mut changed {
        for (type_id, value) in &discrete.0 {
            if sent.0.get(type_id) != Some(value) {
                sessions.broadcast(&component_changed(id.0, *type_id, value.clone(), false));
            }
        }
        for type_id in sent.0.keys().filter(|t| !discrete.0.contains_key(t)) {
            sessions.broadcast(&component_changed(id.0, *type_id, Vec::new(), true));
        }
        sent.0 = discrete.0.clone();
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
            device.components.extend(incoming);
            commands
                .entity(device.entity)
                .insert((device_transform(&device.components), Discrete(discrete(&device.components))));
        }
        None => {
            incoming.entry(schema::CONTENT_REF).or_insert_with(|| builtin(types::BUILTIN_DEVICE).encode_to_vec());
            let id = ids.next_id();
            let parts = (id, device_transform(&incoming), Discrete(discrete(&incoming)), Owner(session));
            let entity = commands.spawn(parts).id();
            devices.0.insert(key, Published { entity, id: id.0, components: incoming });
        }
    }
}

/// `osage.device.set` on a device: the world checks the function's risk class
/// as declared by the device's bridge (D-009), then relays pre-approved
/// commands to the bridge, which has the last word (design/02 M2).
fn device_action(
    session: u64,
    request: ActionRequest,
    time: &Time,
    sessions: &Sessions,
    devices: &Devices,
    pending: &mut PendingCommands,
) {
    let refuse = |status, reason: &str| sessions.reply(session, result(request.request_id, status, reason));
    let Some(((bridge, local_id), published)) = devices.0.iter().find(|(_, d)| d.id == request.target) else {
        return refuse(ActionStatus::Rejected, "no such device");
    };
    let Ok(set) = components::DeviceSet::decode(request.arguments.as_slice()) else {
        return refuse(ActionStatus::Rejected, "arguments must be an osage.device_set/1");
    };
    let device = published.components.get(&schema::DEVICE).and_then(|b| components::Device::decode(b.as_slice()).ok());
    let class = device
        .and_then(|d| d.functions.into_iter().find(|f| f.name == set.function))
        .map_or(components::RiskClass::Unspecified, |f| f.risk_class());
    match class {
        components::RiskClass::PreApproved => {
            pending.1 += 1;
            let command_id = pending.1;
            pending.0.insert(
                command_id,
                PendingCommand {
                    requester: session,
                    request_id: request.request_id,
                    bridge: *bridge,
                    deadline: time.elapsed() + COMMAND_TIMEOUT,
                },
            );
            let requester =
                sessions.0.get(&session).map(|c| c.member.controller.per_world_id.clone()).unwrap_or_default();
            sessions.reply(
                *bridge,
                server_message::Message::DeviceCommand(DeviceCommand {
                    command_id,
                    local_id: local_id.clone(),
                    action: request.action,
                    arguments: request.arguments,
                    requester_per_world_id: requester,
                }),
            );
        }
        components::RiskClass::Gated => sessions.reply(
            session,
            server_message::Message::ActionResult(ActionResult {
                request_id: request.request_id,
                status: ActionStatus::PendingApproval.into(),
                reason: "waiting for the owner's approval receipt (not available in v0)".to_owned(),
                approval_reference: format!("device-{}/{}", request.target, set.function),
                ..default()
            }),
        ),
        components::RiskClass::OnSiteOperator => {
            refuse(ActionStatus::Rejected, "this function needs an on-site operator")
        }
        components::RiskClass::Unspecified => refuse(ActionStatus::Rejected, "this device has no such function"),
        components::RiskClass::ReadOnly | components::RiskClass::Forbidden => {
            refuse(ActionStatus::Rejected, "the owner has not opened this function")
        }
    }
}

/// A bridge that does not answer in time: the outcome is unknown (never
/// retried blindly).
fn expire_commands(time: Res<Time>, sessions: Res<Sessions>, mut pending: ResMut<PendingCommands>) {
    let now = time.elapsed();
    pending.0.retain(|_, command| {
        if command.deadline > now {
            return true;
        }
        let reason = "the bridge did not answer in time: reconcile, do not retry blindly";
        sessions.reply(command.requester, result(command.request_id, ActionStatus::Unknown, reason));
        false
    });
}

fn result(request_id: u64, status: ActionStatus, reason: &str) -> server_message::Message {
    server_message::Message::ActionResult(ActionResult {
        request_id,
        status: status.into(),
        reason: reason.to_owned(),
        ..default()
    })
}

/// A device's components without its transform, which travels separately.
fn discrete(components: &BTreeMap<u32, Vec<u8>>) -> BTreeMap<u32, Vec<u8>> {
    components.iter().filter(|(t, _)| **t != schema::TRANSFORM).map(|(t, v)| (*t, v.clone())).collect()
}

fn device_transform(components: &BTreeMap<u32, Vec<u8>>) -> Transform {
    components
        .get(&schema::TRANSFORM)
        .and_then(|bytes| components::Transform::decode(bytes.as_slice()).ok())
        .map_or_else(Transform::default, |t| transform_from_proto(&t))
}

fn broadcast_state(tick: Res<Tick>, keyframe: Res<Keyframe>, sessions: Res<Sessions>, replicated: Query<Moving>) {
    if sessions.0.is_empty() {
        return;
    }
    let all = tick.0.is_multiple_of(keyframe.0);
    let mut values = Vec::new();
    for (id, transform, volatile) in &replicated {
        if all || transform.is_changed() {
            let value = transform_to_proto(&transform).encode_to_vec();
            values.push(EntityComponent { entity: id.0, component: Some(wire(schema::TRANSFORM, value)) });
        }
        if let Some(volatile) = volatile.filter(|v| all || v.is_changed()) {
            for (type_id, value) in &volatile.0 {
                values.push(EntityComponent { entity: id.0, component: Some(wire(*type_id, value.clone())) });
            }
        }
    }
    if values.is_empty() {
        return;
    }
    sessions.broadcast(&ServerMessage {
        message: Some(server_message::Message::State(StateDatagram { tick: tick.0, server_time_us: now_us(), values })),
    });
}

fn enter(id: u64, transform: &Transform, discrete: Option<&Discrete>, volatile: Option<&Volatile>) -> ServerMessage {
    let mut components = vec![wire(schema::TRANSFORM, transform_to_proto(transform).encode_to_vec())];
    for values in [discrete.map(|d| &d.0), volatile.map(|v| &v.0)].into_iter().flatten() {
        components.extend(values.iter().map(|(type_id, value)| wire(*type_id, value.clone())));
    }
    ServerMessage { message: Some(server_message::Message::EntityEnter(EntityEnter { entity: id, components })) }
}

fn component_changed(id: u64, type_id: u32, value: Vec<u8>, removed: bool) -> ServerMessage {
    ServerMessage {
        message: Some(server_message::Message::ComponentChanged(ComponentChanged {
            entity: id,
            component: Some(wire(type_id, value)),
            removed,
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

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

    use super::*;
    use crate::demo::DemoWorld;

    fn join(inbox: &crossbeam_channel::Sender<RegionInput>, session: u64) -> UnboundedReceiver<ServerMessage> {
        let (out, messages) = unbounded_channel();
        let controller = components::Controller { per_world_id: format!("guest-{session}"), ..Default::default() };
        let member = Member { kind: SessionKind::Human, role: Role::Player, controller, owner: String::new() };
        inbox.send(RegionInput::Join { session, member, out }).unwrap();
        messages
    }

    fn entered(messages: &mut UnboundedReceiver<ServerMessage>) -> Vec<u64> {
        let mut entered: Vec<u64> = std::iter::from_fn(|| messages.try_recv().ok())
            .filter_map(|m| match m.message {
                Some(server_message::Message::EntityEnter(enter)) => Some(enter.entity),
                _ => None,
            })
            .collect();
        entered.sort();
        entered
    }

    /// Found by a flaky end-to-end test: inputs handled in the same tick did
    /// not see the avatars spawned earlier in that tick.
    #[test]
    fn inputs_of_one_tick_see_the_avatars_spawned_in_it() {
        let (inbox, rx) = crossbeam_channel::unbounded();
        let mut app = app(rx, 30, Schema::new(&DemoWorld), DemoWorld);
        let mut alice = join(&inbox, 1);
        let mut bob = join(&inbox, 2);
        let carol = join(&inbox, 3);
        inbox.send(RegionInput::Leave { session: 3 }).unwrap();
        drop(carol);
        app.update();
        app.update();

        let seen_by_alice = entered(&mut alice);
        let seen_by_bob = entered(&mut bob);
        assert_eq!(seen_by_alice, seen_by_bob, "alice and bob see the same entities");
        assert_eq!(seen_by_alice.len(), 3, "the cube and two avatars: {seen_by_alice:?}");
        let avatars = app.world_mut().query::<&Owner>().iter(app.world()).map(|o| o.0).collect::<Vec<_>>();
        assert_eq!(avatars.len(), 2, "carol's avatar left with her: {avatars:?}");
    }
}
