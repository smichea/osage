// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! A bridge mirrors a device into a real world; observers see it, its owner is
//! the authenticated one, and it leaves with its bridge.

use std::{net::TcpListener, thread, time::Duration};

use osage_bridge::{Placement, Sensor, measured_now, read_only};
use osage_credentials::{Level, TestIssuer};
use osage_sdk::{
    DELEGATE_AGENT, DELEGATE_BRIDGE, World, WorldEvent,
    components::{self, Device, Measurement},
    connect, device_state, hello,
    math::Vec3,
    request_delegation, types,
    v0::{Role, Tier},
    with_bridge, with_delegation, with_identity,
};
use tokio::time::timeout;

struct Thermometer(f64);

impl Sensor for Thermometer {
    fn local_id(&self) -> &str {
        "thermometer"
    }
    fn describe(&self) -> Device {
        // A bridge cannot pick the owner: the world records the authenticated one.
        Device {
            name: "test thermometer".to_owned(),
            owner_per_world_id: "mallory@demo".to_owned(),
            functions: vec![read_only("temperature")],
            ..Default::default()
        }
    }
    fn read(&mut self) -> Vec<Measurement> {
        self.0 += 1.0;
        vec![measured_now("temperature", self.0, "Cel")]
    }
}

fn start_world() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let config =
        osage_server::Config { trusted_issuers: vec![TestIssuer::insecure_dev().trusted()], ..Default::default() };
    thread::spawn(move || osage_server::run(listener, config));
    url
}

async fn drain(receiver: &mut osage_sdk::SessionReceiver, world: &mut World, how_long: Duration) -> Vec<WorldEvent> {
    let mut events = Vec::new();
    let _ = timeout(how_long, async {
        while let Some(Ok(message)) = receiver.next_message().await {
            events.extend(world.apply(&message));
        }
    })
    .await;
    events
}

fn find_device(world: &World) -> Option<u64> {
    world.entities().find(|&e| world.get::<Device>(e, types::DEVICE).is_some())
}

#[tokio::test]
async fn a_bridge_mirrors_a_device_that_leaves_with_it() {
    let url = start_world();
    let owner =
        TestIssuer::insecure_dev().issue("osage-demo", "alice@demo", Level::A1, Duration::from_secs(600)).unwrap();
    let delegation = request_delegation(&url, &owner, DELEGATE_BRIDGE).await.unwrap();

    let observer = connect(&url, hello("observer", "0", Role::Observer, Tier::T0Headless)).await.unwrap();
    let (welcome, _sender, mut receiver) = observer.split();
    let mut world = World::new(&welcome);

    let placement = Placement { position: Vec3 { x: 0.0, y: 0.0, z: -4.0 } };
    let bridge = tokio::spawn({
        let url = url.clone();
        async move {
            osage_bridge::run(
                &url,
                &delegation,
                vec![(Box::new(Thermometer(20.0)), placement)],
                Duration::from_millis(100),
            )
            .await
        }
    });

    let events = drain(&mut receiver, &mut world, Duration::from_millis(800)).await;
    let device = find_device(&world).expect("the observer sees the device");
    let described: Device = world.get(device, types::DEVICE).unwrap();
    assert_eq!(described.owner_per_world_id, "alice@demo", "the authenticated owner, not the claimed one");
    assert!(
        described.bridge_per_world_id.starts_with("bridge-") && described.bridge_per_world_id.ends_with(".alice@demo")
    );
    assert_eq!(described.functions[0].risk_class(), components::RiskClass::ReadOnly);
    let content: components::ContentRef = world.get(device, types::CONTENT_REF).unwrap();
    assert_eq!(content.uri, types::BUILTIN_DEVICE);

    let updates = events
        .iter()
        .filter(|e| **e == WorldEvent::Changed { entity: device, type_name: types::MEASUREMENTS.to_owned() })
        .count();
    assert!(updates >= 3, "measurements stream in: {updates} updates");
    let latest: components::Measurements = world.get(device, types::MEASUREMENTS).unwrap();
    assert!(latest.values[0].value > 21.0 && latest.values[0].unit == "Cel" && latest.values[0].source_time_us > 0);

    bridge.abort();
    let events = drain(&mut receiver, &mut world, Duration::from_secs(2)).await;
    assert!(events.contains(&WorldEvent::Left(device)), "the device leaves with its bridge");
}

#[tokio::test]
async fn only_bridges_publish_devices_and_delegations_keep_their_role() {
    let url = start_world();
    let alice =
        TestIssuer::insecure_dev().issue("osage-demo", "alice@demo", Level::A2, Duration::from_secs(600)).unwrap();

    let bridge_delegation = request_delegation(&url, &alice, DELEGATE_BRIDGE).await.unwrap();
    let error = connect(&url, with_delegation(hello("x", "0", Role::Agent, Tier::T0Headless), &bridge_delegation))
        .await
        .err()
        .unwrap();
    assert!(error.to_string().contains("Unauthenticated"), "a bridge delegation is not an agent's: {error}");
    let agent_delegation = request_delegation(&url, &alice, DELEGATE_AGENT).await.unwrap();
    let error = connect(&url, with_bridge(hello("x", "0", Role::Bridge, Tier::T0Headless), &agent_delegation))
        .await
        .err()
        .unwrap();
    assert!(error.to_string().contains("Unauthenticated"), "an agent delegation is not a bridge's: {error}");
    let error = connect(&url, hello("x", "0", Role::Bridge, Tier::T0Headless)).await.err().unwrap();
    assert!(error.to_string().contains("Unauthenticated"), "bridges need a delegation: {error}");

    let observer = connect(&url, hello("observer", "0", Role::Observer, Tier::T0Headless)).await.unwrap();
    let (welcome, _sender, mut receiver) = observer.split();
    let mut world = World::new(&welcome);
    let mut player =
        connect(&url, with_identity(hello("p", "0", Role::Player, Tier::T2Standard), &alice)).await.unwrap();
    let device_type = World::new(player.welcome()).types().id(types::DEVICE).unwrap();
    let fake = Device { name: "not a bridge".to_owned(), ..Default::default() };
    player.send(device_state("fake", vec![(device_type, prost::Message::encode_to_vec(&fake))])).await.unwrap();
    drain(&mut receiver, &mut world, Duration::from_millis(500)).await;
    assert!(find_device(&world).is_none(), "a player cannot publish devices");
}
