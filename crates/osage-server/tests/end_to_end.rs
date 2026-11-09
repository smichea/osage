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
    let mut session = connect(&url, hello("osage-test", "0", Role::Observer, Tier::T0Headless))
        .await
        .unwrap();

    let welcome = session.welcome().clone();
    assert_eq!(welcome.protocol_version, "0.1");
    let world_descriptor = welcome.world.clone().unwrap();
    assert_eq!(
        world_descriptor.source_offer.unwrap().url,
        "https://github.com/smichea/osage"
    );
    let schemas = welcome.schemas.clone().unwrap();
    assert!(
        schemas.descriptors.is_some(),
        "the schema bundle carries the descriptors inline"
    );

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
