// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! D-037: the gateway and the region talk only by messages, so they can run
//! apart. Here the region serves its link on a TCP connection, as it would on
//! another machine, and real clients use the gateway as usual.

use std::{
    net::{TcpListener, TcpStream},
    thread,
    time::Duration,
};

use osage_sdk::{
    World, WorldEvent, components, connect, hello, types,
    v0::{Role, Tier},
};
use osage_server::{Config, demo::DemoWorld, link::network};
use tokio::time::timeout;

/// A region behind a TCP connection, and a gateway that reaches it through
/// that connection; returns the gateway's WebSocket URL.
fn start_apart() -> String {
    let links = TcpListener::bind("127.0.0.1:0").unwrap();
    let region_at = links.local_addr().unwrap();
    thread::spawn(move || {
        let (stream, _) = links.accept().unwrap();
        osage_server::run_region(network::region_end(stream).unwrap(), 30, DemoWorld);
    });

    let clients = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}", clients.local_addr().unwrap());
    let link = network::gateway_end(TcpStream::connect(region_at).unwrap()).unwrap();
    osage_server::spawn_gateway(clients, Config::default(), link).unwrap();
    url
}

#[tokio::test]
async fn clients_play_through_a_region_on_the_other_end_of_a_connection() {
    let url = start_apart();
    let observer = connect(&url, hello("observer", "0", Role::Observer, Tier::T0Headless)).await.unwrap();
    let welcome = observer.welcome().clone();
    assert_eq!(welcome.world.as_ref().unwrap().tick_rate_hz, 30, "the tick rate the region announced");
    assert!(welcome.schemas.as_ref().unwrap().descriptors.is_some(), "the schemas the region announced");
    let (_, _observer_sender, mut observer_rx) = observer.split();
    let mut observed = World::new(&welcome);

    let player = connect(&url, hello("player", "0", Role::Player, Tier::T2Standard)).await.unwrap();
    let me = player.welcome().identity.clone().unwrap().per_world_id;
    let (_, _player_sender, _player_rx) = player.split();

    // The observer sees the cube move and the player's avatar arrive, each
    // entity entering once.
    let mut entered = Vec::new();
    let mut cube_moves = 0;
    timeout(Duration::from_secs(10), async {
        while cube_moves < 3 || observed.controlled_by(&me).is_none() {
            let message = observer_rx.next_message().await.expect("connection open").unwrap();
            for event in observed.apply(&message) {
                match event {
                    WorldEvent::Entered(entity) => entered.push(entity),
                    WorldEvent::Changed { type_name, .. } if type_name == types::TRANSFORM => cube_moves += 1,
                    _ => {}
                }
            }
        }
    })
    .await
    .expect("the cube moves and the avatar arrives within 10 s");

    let avatar = observed.controlled_by(&me).unwrap();
    let content: components::ContentRef = observed.get(avatar, types::CONTENT_REF).unwrap();
    assert_eq!(content.uri, types::BUILTIN_AVATAR);
    let mut once = entered.clone();
    once.sort();
    once.dedup();
    assert_eq!(once.len(), entered.len(), "no entity entered twice: {entered:?}");
}
