// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! osage device bridge (design/02): runs next to its owner's devices, joins a
//! world with the `bridge` role and the owner's delegation, and mirrors the
//! devices — the bridge is authoritative over their state (M1).
//!
//! Functions are `READ_ONLY` unless the owner opens them (D-009). Even for a
//! pre-approved function the bridge has the last word (design/02 M2): it
//! checks its own declared class and the allowed range of values locally, and
//! puts every device in its safe state when the world link ends (watchdog).

pub mod linux;

use std::{
    ops::RangeInclusive,
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result};
use osage_sdk::{
    DEVICE_SET,
    components::{self, Device, DeviceSet, Measurement, Measurements, RiskClass},
    connect, device_command_result, device_state, hello,
    math::Vec3,
    types,
    v0::{ActionStatus, DeviceCommand, Role, ServerMessage, Tier, server_message},
    with_bridge,
};
use prost::Message as _;

/// One device the bridge mirrors: its readings and, if its owner opened
/// them, its settable functions.
pub trait Sensor: Send {
    /// The bridge's own, stable name for this device.
    fn local_id(&self) -> &str;
    /// What the device is (name, OpenDeviceDB id, functions and their classes).
    fn describe(&self) -> Device;
    /// Current readings, timestamped at the source.
    fn read(&mut self) -> Vec<Measurement>;
    /// The values `function` may take — the local safety envelope. `None`:
    /// the function cannot be set.
    fn allowed(&self, _function: &str) -> Option<RangeInclusive<f64>> {
        None
    }
    /// Sets `function`; only called within `allowed` and for a pre-approved function.
    fn set(&mut self, function: &str, _value: f64) -> Result<(), String> {
        Err(format!("{function} cannot be set"))
    }
    /// Puts the device in its safe state (e.g. a light off); called when the
    /// world link ends, however it ends.
    fn safe_state(&mut self) {}
}

/// The bridge's devices; dropping it puts every device in its safe state —
/// also when the bridge task is aborted (the watchdog of design/02 M2).
struct Devices(Vec<(Box<dyn Sensor>, Placement)>);

impl Drop for Devices {
    fn drop(&mut self) {
        for (sensor, _) in &mut self.0 {
            sensor.safe_state();
        }
    }
}

/// The local decision on a device command (design/02 M2).
fn execute(sensor: &mut dyn Sensor, command: &DeviceCommand) -> (ActionStatus, String) {
    if command.action != DEVICE_SET {
        return (ActionStatus::Rejected, format!("unsupported action {}", command.action));
    }
    let Ok(set) = DeviceSet::decode(command.arguments.as_slice()) else {
        return (ActionStatus::Rejected, "malformed arguments".to_owned());
    };
    let class = sensor.describe().functions.into_iter().find(|f| f.name == set.function).map(|f| f.risk_class());
    if class != Some(RiskClass::PreApproved) {
        return (ActionStatus::Rejected, format!("{} is not pre-approved on this bridge", set.function));
    }
    match sensor.allowed(&set.function) {
        Some(range) if range.contains(&set.value) => match sensor.set(&set.function, set.value) {
            Ok(()) => (ActionStatus::Ok, String::new()),
            Err(error) => (ActionStatus::Failed, error),
        },
        Some(range) => (ActionStatus::Rejected, format!("{} must stay within {range:?}", set.function)),
        None => (ActionStatus::Rejected, format!("{} cannot be set", set.function)),
    }
}

/// Where a device appears in the world.
pub struct Placement {
    pub position: Vec3,
}

/// Joins `url` as a bridge and publishes every sensor every `period` until the
/// connection ends.
pub async fn run(
    url: &str,
    delegation: &str,
    sensors: Vec<(Box<dyn Sensor>, Placement)>,
    period: Duration,
) -> Result<()> {
    let mut devices = Devices(sensors);
    let hello =
        with_bridge(hello("osage-bridge", env!("CARGO_PKG_VERSION"), Role::Bridge, Tier::T0Headless), delegation);
    let (welcome, mut sender, mut receiver) = connect(url, hello).await?.split();
    let world = osage_sdk::World::new(&welcome);
    let type_id = |name| world.types().id(name).with_context(|| format!("this world does not accept {name}"));
    let (device_t, measurements_t, transform_t, content_t) = (
        type_id(types::DEVICE)?,
        type_id(types::MEASUREMENTS)?,
        type_id(types::TRANSFORM)?,
        type_id(types::CONTENT_REF)?,
    );

    // The first state describes each device and places it.
    for (sensor, placement) in &mut devices.0 {
        let transform = components::Transform { translation: Some(placement.position), ..Default::default() };
        let content = components::ContentRef {
            uri: types::BUILTIN_DEVICE.to_owned(),
            kind: components::ContentKind::Builtin.into(),
            ..Default::default()
        };
        let readings = Measurements { values: sensor.read() };
        let state = device_state(
            sensor.local_id(),
            vec![
                (device_t, sensor.describe().encode_to_vec()),
                (transform_t, transform.encode_to_vec()),
                (content_t, content.encode_to_vec()),
                (measurements_t, readings.encode_to_vec()),
            ],
        );
        sender.send(state).await?;
    }

    let mut ticker = tokio::time::interval(period);
    ticker.tick().await;
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                for (sensor, _) in &mut devices.0 {
                    let readings = Measurements { values: sensor.read() };
                    sender.send(device_state(sensor.local_id(), vec![(measurements_t, readings.encode_to_vec())])).await?;
                }
            }
            message = receiver.next_message() => match message {
                Some(Ok(ServerMessage { message: Some(server_message::Message::DeviceCommand(command)) })) => {
                    let Some((sensor, _)) = devices.0.iter_mut().find(|(s, _)| s.local_id() == command.local_id) else {
                        sender.send(device_command_result(command.command_id, ActionStatus::Rejected, "no such device")).await?;
                        continue;
                    };
                    let (status, reason) = execute(sensor.as_mut(), &command);
                    sender.send(device_command_result(command.command_id, status, &reason)).await?;
                    let readings = Measurements { values: sensor.read() };
                    sender.send(device_state(sensor.local_id(), vec![(measurements_t, readings.encode_to_vec())])).await?;
                }
                Some(Ok(_)) => {}
                Some(Err(error)) => return Err(error),
                None => return Ok(()),
            },
        }
    }
}

/// A read-only function of a device.
pub fn read_only(name: &str) -> components::DeviceFunction {
    components::DeviceFunction { name: name.to_owned(), risk_class: RiskClass::ReadOnly.into() }
}

/// A function the device's owner opened to the world's participants (D-009).
pub fn pre_approved(name: &str) -> components::DeviceFunction {
    components::DeviceFunction { name: name.to_owned(), risk_class: RiskClass::PreApproved.into() }
}

/// A measurement taken now, on this machine's clock.
pub fn measured_now(name: &str, value: f64, unit: &str) -> Measurement {
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_micros() as u64);
    Measurement {
        name: name.to_owned(),
        value,
        unit: unit.to_owned(),
        source_time_us: now,
        clock_domain: "unix-utc".to_owned(),
    }
}
