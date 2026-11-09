// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! A real server on a free port, a real client through the Rust SDK.

use std::{net::TcpListener, thread, time::Duration};

use osage_sdk::{
    World, WorldEvent, components, connect, hello, types,
    v0::{Role, Tier},
};
use tokio::time::timeout;

fn start_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    thread::spawn(move || osage_server::run(listener, osage_server::Config::default()));
    url
}

#[tokio::test]
async fn a_client_sees_the_server_driven_cube_move() {
    let url = start_server();
    let mut session = connect(&url, hello("osage-test", "0", Role::Observer, Tier::T0Headless)).await.unwrap();

    let welcome = session.welcome().clone();
    assert_eq!(welcome.protocol_version, "0.1");
    let world_descriptor = welcome.world.clone().unwrap();
    assert_eq!(world_descriptor.source_offer.unwrap().url, "https://github.com/smichea/osage");
    let schemas = welcome.schemas.clone().unwrap();
    assert!(schemas.descriptors.is_some(), "the schema bundle carries the descriptors inline");

    let mut world = World::new(&welcome);
    assert!(world.types().id(types::TRANSFORM).is_some());

    let mut cube = None;
    let mut positions = Vec::new();
    timeout(Duration::from_secs(10), async {
        while positions.len() < 3 {
            let message = session.next_message().await.expect("connection open").unwrap();
            for event in world.apply(&message) {
                match event {
                    WorldEvent::Entered(entity) => cube = Some(entity),
                    WorldEvent::Changed { entity, type_name }
                        if Some(entity) == cube && type_name == types::TRANSFORM =>
                    {
                        let transform: components::Transform = world.get(entity, types::TRANSFORM).unwrap();
                        positions.push(transform.translation.unwrap());
                    }
                    _ => {}
                }
            }
        }
    })
    .await
    .expect("three state updates within 10 s");

    let cube = cube.unwrap();
    let content: components::ContentRef = world.get(cube, types::CONTENT_REF).unwrap();
    assert_eq!(content.uri, types::BUILTIN_CUBE);
    let controller: components::Controller = world.get(cube, types::CONTROLLER).unwrap();
    assert_eq!(controller.kind(), components::ControllerKind::System);
    assert_ne!(positions.first(), positions.last(), "the cube moves");
}

#[tokio::test]
async fn an_unknown_protocol_version_is_refused() {
    let url = start_server();
    let mut old = hello("osage-test", "0", Role::Observer, Tier::T0Headless);
    old.protocol_versions = vec!["9.9".to_owned()];
    let error = connect(&url, old).await.err().expect("refused");
    assert!(error.to_string().contains("Version"), "{error}");
}

async fn drain(
    receiver: &mut osage_sdk::SessionReceiver,
    world: &mut World,
    for_how_long: Duration,
) -> Vec<WorldEvent> {
    let mut events = Vec::new();
    let _ = timeout(for_how_long, async {
        while let Some(Ok(message)) = receiver.next_message().await {
            events.extend(world.apply(&message));
        }
    })
    .await;
    events
}

fn x_of(world: &World, entity: u64) -> f32 {
    world.get::<components::Transform>(entity, types::TRANSFORM).unwrap().translation.unwrap().x
}

/// Sends +x movement intents for `entity` every 50 ms for `duration`.
async fn walk_east(
    sender: &mut osage_sdk::SessionSender,
    receiver: &mut osage_sdk::SessionReceiver,
    world: &mut World,
    entity: u64,
    duration: Duration,
) {
    let move_type = world.types().id(types::MOVE_INTENT).unwrap();
    let east = components::MoveIntent { direction: Some(osage_sdk::math::Vec3 { x: 1.0, y: 0.0, z: 0.0 }) };
    let start = tokio::time::Instant::now();
    let mut sequence = 0;
    while start.elapsed() < duration {
        sequence += 1;
        sender.send(osage_sdk::intent(sequence, entity, move_type, &east)).await.unwrap();
        drain(receiver, world, Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn a_player_moves_its_avatar_and_others_see_it_come_and_go() {
    let url = start_server();
    let observer = connect(&url, hello("observer", "0", Role::Observer, Tier::T0Headless)).await.unwrap();
    let (observer_welcome, _observer_sender, mut observer_rx) = observer.split();
    let mut observed = World::new(&observer_welcome);

    let player = connect(&url, hello("player", "0", Role::Player, Tier::T2Standard)).await.unwrap();
    let me = player.welcome().identity.clone().unwrap().per_world_id;
    let (welcome, mut sender, mut receiver) = player.split();
    let mut world = World::new(&welcome);

    drain(&mut receiver, &mut world, Duration::from_millis(300)).await;
    let avatar = world.controlled_by(&me).expect("the player gets an avatar");
    let content: components::ContentRef = world.get(avatar, types::CONTENT_REF).unwrap();
    assert_eq!(content.uri, types::BUILTIN_AVATAR);

    let start_x = x_of(&world, avatar);
    walk_east(&mut sender, &mut receiver, &mut world, avatar, Duration::from_secs(1)).await;
    let end_x = x_of(&world, avatar);
    assert!(end_x > start_x + 2.0, "the avatar walked east: {start_x} → {end_x}");

    drain(&mut observer_rx, &mut observed, Duration::from_millis(300)).await;
    assert_eq!(observed.controlled_by(&me), Some(avatar), "the observer sees the player's avatar");

    sender.send(osage_sdk::v0::client_message::Message::Goodbye(Default::default())).await.unwrap();
    drop((sender, receiver));
    let events = drain(&mut observer_rx, &mut observed, Duration::from_secs(2)).await;
    assert!(events.contains(&WorldEvent::Left(avatar)), "the avatar leaves with its player");
}

#[tokio::test]
async fn intents_for_entities_one_does_not_own_are_ignored() {
    let url = start_server();
    let alice = connect(&url, hello("alice", "0", Role::Player, Tier::T2Standard)).await.unwrap();
    let alice_id = alice.welcome().identity.clone().unwrap().per_world_id;
    let (alice_welcome, _alice_sender, mut alice_rx) = alice.split();
    let mut alice_world = World::new(&alice_welcome);

    let mallory = connect(&url, hello("mallory", "0", Role::Player, Tier::T2Standard)).await.unwrap();
    let (mallory_welcome, mut mallory_sender, mut mallory_rx) = mallory.split();
    let mut mallory_world = World::new(&mallory_welcome);
    drain(&mut mallory_rx, &mut mallory_world, Duration::from_millis(300)).await;
    let alices_avatar = mallory_world.controlled_by(&alice_id).expect("mallory sees alice's avatar");

    drain(&mut alice_rx, &mut alice_world, Duration::from_millis(100)).await;
    let before = x_of(&alice_world, alices_avatar);
    walk_east(&mut mallory_sender, &mut mallory_rx, &mut mallory_world, alices_avatar, Duration::from_millis(600))
        .await;
    drain(&mut alice_rx, &mut alice_world, Duration::from_millis(200)).await;
    assert_eq!(x_of(&alice_world, alices_avatar), before, "nobody but its owner moves an avatar");
}

#[tokio::test]
async fn an_abrupt_disconnection_removes_the_avatar() {
    let url = start_server();
    let observer = connect(&url, hello("observer", "0", Role::Observer, Tier::T0Headless)).await.unwrap();
    let (observer_welcome, _observer_sender, mut observer_rx) = observer.split();
    let mut observed = World::new(&observer_welcome);

    let player = connect(&url, hello("player", "0", Role::Player, Tier::T2Standard)).await.unwrap();
    let me = player.welcome().identity.clone().unwrap().per_world_id;
    drain(&mut observer_rx, &mut observed, Duration::from_millis(300)).await;
    let avatar = observed.controlled_by(&me).expect("the observer sees the avatar");

    drop(player); // no Goodbye: the TCP connection just goes away
    let events = drain(&mut observer_rx, &mut observed, Duration::from_secs(2)).await;
    assert!(events.contains(&WorldEvent::Left(avatar)), "the avatar leaves with its vanished player");
}
