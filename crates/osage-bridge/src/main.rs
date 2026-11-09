// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! `osage-bridge [--world ws://127.0.0.1:7878] [--period-ms 1000] [--open-led NAME]… [--sysfs /sys/class]` —
//! mirrors this machine's sensors; the delegation is read from
//! `OSAGE_BRIDGE_DELEGATION`. `--open-led NAME` is the owner opening the
//! brightness of `/sys/class/leds/NAME` to the world (pre-approved, D-009);
//! it is switched off when the bridge stops — when the world link ends, and on
//! SIGINT (Ctrl+C), SIGTERM or SIGHUP.
//! `osage-bridge delegate --credential TOKEN [--world URL]` — prints a bridge
//! delegation for the owner holding the identity credential TOKEN.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use osage_bridge::{
    Placement, Sensor,
    linux::{Led, ThisMachine},
    run,
};
use osage_sdk::{DELEGATE_BRIDGE, math::Vec3, request_delegation};

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1).peekable();
    let delegate = args.next_if_eq("delegate").is_some();
    let (mut world, mut credential, mut period_ms, mut leds, mut sysfs) =
        ("ws://127.0.0.1:7878".to_owned(), None, 1000, Vec::new(), "/sys/class".to_owned());
    while let Some(flag) = args.next() {
        let value = args.next().with_context(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--world" => world = value,
            "--credential" => credential = Some(value),
            "--period-ms" => period_ms = value.parse().context("--period-ms")?,
            "--open-led" => leds.push(value),
            "--sysfs" => sysfs = value,
            _ => bail!("unknown argument {flag}"),
        }
    }
    if delegate {
        let credential = credential.context("delegate needs --credential")?;
        println!("{}", request_delegation(&world, &credential, DELEGATE_BRIDGE).await?);
        return Ok(());
    }
    let delegation = std::env::var("OSAGE_BRIDGE_DELEGATION")
        .context("set OSAGE_BRIDGE_DELEGATION (see `osage-bridge delegate`)")?;
    let mut devices: Vec<(Box<dyn Sensor>, Placement)> =
        vec![(Box::new(ThisMachine::detect_in(&sysfs)), Placement { position: Vec3 { x: 0.0, y: 0.0, z: -4.0 } })];
    for (n, name) in leds.iter().enumerate() {
        let led = Led::open_in(&sysfs, name).map_err(anyhow::Error::msg)?;
        eprintln!(
            "osage-bridge: {name} brightness is OPEN to the world (pre-approved); it is switched off when the bridge stops"
        );
        devices.push((Box::new(led), Placement { position: Vec3 { x: 1.5 * (n + 1) as f32, y: 0.0, z: -4.0 } }));
    }
    eprintln!("osage-bridge: mirroring this machine into {world}");
    // The watchdog runs when the bridge's devices are dropped; a signal's
    // default action would end the process without dropping them, so a
    // signal ends the bridge instead.
    tokio::select! {
        result = run(&world, &delegation, devices, Duration::from_millis(period_ms)) => result,
        signal = stop_signal() => {
            eprintln!("osage-bridge: {}, devices back in their safe state", signal?);
            Ok(())
        }
    }
}

#[cfg(unix)]
async fn stop_signal() -> Result<&'static str> {
    use tokio::signal::unix::{SignalKind, signal};
    let (mut interrupt, mut terminate, mut hangup) =
        (signal(SignalKind::interrupt())?, signal(SignalKind::terminate())?, signal(SignalKind::hangup())?);
    Ok(tokio::select! {
        _ = interrupt.recv() => "interrupted",
        _ = terminate.recv() => "terminated",
        _ = hangup.recv() => "hung up",
    })
}

#[cfg(not(unix))]
async fn stop_signal() -> Result<&'static str> {
    tokio::signal::ctrl_c().await?;
    Ok("interrupted")
}
