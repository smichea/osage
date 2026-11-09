// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The watchdog also runs when the bridge process is stopped by a signal
//! (Ctrl+C, `kill`, a closed terminal): found on a real keyboard backlight,
//! which stayed on after SIGTERM.

#![cfg(unix)]

use std::{fs, net::TcpListener, path::Path, process::Command, thread, time::Duration};

use osage_credentials::{Level, TestIssuer};
use osage_sdk::{
    DELEGATE_BRIDGE, World,
    components::Device,
    connect, device_set, hello, request_delegation, types,
    v0::{ActionStatus, Role, Tier, server_message},
};
use tokio::time::{sleep, timeout};

#[tokio::test]
async fn a_signal_puts_an_opened_led_back_in_its_safe_state() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let config =
        osage_server::Config { trusted_issuers: vec![TestIssuer::insecure_dev().trusted()], ..Default::default() };
    thread::spawn(move || osage_server::run(listener, config));
    let owner =
        TestIssuer::insecure_dev().issue("osage-demo", "alice@demo", Level::A1, Duration::from_secs(600)).unwrap();

    for signal in ["INT", "TERM", "HUP"] {
        let sysfs = std::env::temp_dir().join(format!("osage-bridge-signals-{}-{signal}", std::process::id()));
        let led = sysfs.join("leds/test::led");
        fs::create_dir_all(&led).unwrap();
        fs::write(led.join("max_brightness"), "2\n").unwrap();
        fs::write(led.join("brightness"), "0\n").unwrap();

        let delegation = request_delegation(&url, &owner, DELEGATE_BRIDGE).await.unwrap();
        let mut bridge = Command::new(env!("CARGO_BIN_EXE_osage-bridge"))
            .args(["--world", &url, "--period-ms", "200", "--open-led", "test::led", "--sysfs"])
            .arg(&sysfs)
            .env("OSAGE_BRIDGE_DELEGATION", delegation)
            .spawn()
            .unwrap();

        let (welcome, mut sender, mut receiver) =
            connect(&url, hello("guest", "0", Role::Player, Tier::T2Standard)).await.unwrap().split();
        let mut world = World::new(&welcome);
        let entity = timeout(Duration::from_secs(10), async {
            loop {
                world.apply(&receiver.next_message().await.unwrap().unwrap());
                let found = world
                    .entities()
                    .find(|&e| world.get::<Device>(e, types::DEVICE).is_some_and(|d| d.name == "test::led"));
                if let Some(entity) = found {
                    return entity;
                }
            }
        })
        .await
        .expect("the guest sees the LED");

        sender.send(device_set(1, entity, "brightness", 2.0)).await.unwrap();
        let status = timeout(Duration::from_secs(5), async {
            loop {
                if let Some(server_message::Message::ActionResult(result)) =
                    receiver.next_message().await.unwrap().unwrap().message
                {
                    return result.status();
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(status, ActionStatus::Ok);
        assert_eq!(brightness(&led), "2", "the LED is on");

        let killed = Command::new("kill").arg(format!("-{signal}")).arg(bridge.id().to_string()).status().unwrap();
        assert!(killed.success());
        let exited = timeout(Duration::from_secs(5), async {
            loop {
                if let Some(status) = bridge.try_wait().unwrap() {
                    return status;
                }
                sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("SIG{signal} stops the bridge"));
        assert!(exited.success(), "SIG{signal}: the bridge stops cleanly ({exited})");
        assert_eq!(brightness(&led), "0", "SIG{signal}: the watchdog switched the LED off");
        fs::remove_dir_all(&sysfs).unwrap();
    }
}

fn brightness(led: &Path) -> String {
    fs::read_to_string(led.join("brightness")).unwrap().trim().to_owned()
}
