// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Acting on a real device from a world (design/02 M2–M4, D-009): the world
//! checks the risk class, the bridge applies its local envelope, and the
//! device returns to its safe state when the bridge stops.

use std::{
    net::TcpListener,
    ops::RangeInclusive,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use osage_bridge::{Placement, Sensor, measured_now, pre_approved, read_only};
use osage_credentials::{Level, TestIssuer};
use osage_sdk::{
    DELEGATE_BRIDGE, World,
    components::{Device, DeviceFunction, Measurement, RiskClass},
    connect, device_set, hello,
    math::Vec3,
    request_delegation, types,
    v0::{ActionStatus, Role, Tier, server_message},
};
use tokio::time::timeout;

/// A dimmable lamp: brightness 0–2, off is its safe state.
struct Lamp(Arc<Mutex<f64>>);

impl Sensor for Lamp {
    fn local_id(&self) -> &str {
        "lamp"
    }
    fn describe(&self) -> Device {
        let gated = DeviceFunction { name: "mode".to_owned(), risk_class: RiskClass::Gated.into() };
        Device {
            name: "test lamp".to_owned(),
            functions: vec![pre_approved("brightness"), gated, read_only("power_save")],
            ..Default::default()
        }
    }
    fn read(&mut self) -> Vec<Measurement> {
        vec![measured_now("brightness", *self.0.lock().unwrap(), "1")]
    }
    fn allowed(&self, function: &str) -> Option<RangeInclusive<f64>> {
        (function == "brightness").then_some(0.0..=2.0)
    }
    fn set(&mut self, _function: &str, value: f64) -> Result<(), String> {
        *self.0.lock().unwrap() = value;
        Ok(())
    }
    fn safe_state(&mut self) {
        *self.0.lock().unwrap() = 0.0;
    }
}

#[tokio::test]
async fn a_pre_approved_function_is_set_within_the_envelope_and_reset_by_the_watchdog() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let config =
        osage_server::Config { trusted_issuers: vec![TestIssuer::insecure_dev().trusted()], ..Default::default() };
    thread::spawn(move || osage_server::run(listener, config));

    let owner =
        TestIssuer::insecure_dev().issue("osage-demo", "alice@demo", Level::A1, Duration::from_secs(600)).unwrap();
    let delegation = request_delegation(&url, &owner, DELEGATE_BRIDGE).await.unwrap();
    let level = Arc::new(Mutex::new(0.0));
    let lamp =
        (Box::new(Lamp(level.clone())) as Box<dyn Sensor>, Placement { position: Vec3 { x: 2.0, y: 0.0, z: -4.0 } });
    let bridge = tokio::spawn({
        let url = url.clone();
        async move { osage_bridge::run(&url, &delegation, vec![lamp], Duration::from_millis(200)).await }
    });

    // Any participant of the world — here an anonymous guest — may use a pre-approved function.
    let guest = connect(&url, hello("guest", "0", Role::Player, Tier::T2Standard)).await.unwrap();
    let (welcome, mut sender, mut receiver) = guest.split();
    let mut world = World::new(&welcome);
    let mut lamp_entity = None;
    let _ = timeout(Duration::from_millis(600), async {
        while let Some(Ok(message)) = receiver.next_message().await {
            world.apply(&message);
        }
    })
    .await;
    for entity in world.entities() {
        if world.get::<Device>(entity, types::DEVICE).is_some_and(|d| d.name == "test lamp") {
            lamp_entity = Some(entity);
        }
    }
    let lamp = lamp_entity.expect("the guest sees the lamp");

    let mut request_id = 0;
    let mut act = async |function: &str, value: f64| {
        request_id += 1;
        sender.send(device_set(request_id, lamp, function, value)).await.unwrap();
        timeout(Duration::from_secs(5), async {
            loop {
                let message = receiver.next_message().await.unwrap().unwrap();
                world.apply(&message);
                if let Some(server_message::Message::ActionResult(result)) = message.message
                    && result.request_id == request_id
                {
                    return (result.status(), result.reason);
                }
            }
        })
        .await
        .unwrap()
    };

    assert_eq!(act("brightness", 2.0).await.0, ActionStatus::Ok);
    assert_eq!(*level.lock().unwrap(), 2.0, "the lamp is on");

    let (status, reason) = act("brightness", 5.0).await;
    assert_eq!(status, ActionStatus::Rejected, "outside the local envelope: {reason}");
    assert_eq!(*level.lock().unwrap(), 2.0);

    let (status, reason) = act("power_save", 1.0).await;
    assert_eq!(status, ActionStatus::Rejected, "read-only: {reason}");
    assert_eq!(act("mode", 1.0).await.0, ActionStatus::PendingApproval, "gated functions wait for the owner");
    assert_eq!(act("unplugged", 1.0).await.0, ActionStatus::Rejected);

    bridge.abort();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(*level.lock().unwrap(), 0.0, "the watchdog puts the lamp in its safe state");
}
