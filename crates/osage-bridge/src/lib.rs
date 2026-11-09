// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! osage device bridge (design/02): runs next to its owner's devices, joins a
//! world with the `bridge` role and the owner's delegation, and mirrors the
//! devices — the bridge is authoritative over their state (M1).
//!
//! Phase 1 is read-only: every function is published as `READ_ONLY`, the
//! class a bridge ships with (D-009). Actuation comes next, behind the local
//! safety envelope.

pub mod linux;

use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use osage_sdk::{
    components::{self, Device, Measurement, Measurements},
    connect, device_state, hello,
    math::Vec3,
    types,
    v0::{Role, Tier},
    with_bridge,
};
use prost::Message as _;

/// A source of measurements for one device.
pub trait Sensor: Send {
    /// The bridge's own, stable name for this device.
    fn local_id(&self) -> &str;
    /// What the device is (name, OpenDeviceDB id, functions).
    fn describe(&self) -> Device;
    /// Current readings, timestamped at the source.
    fn read(&mut self) -> Vec<Measurement>;
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
    mut sensors: Vec<(Box<dyn Sensor>, Placement)>,
    period: Duration,
) -> Result<()> {
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
    for (sensor, placement) in &mut sensors {
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
                for (sensor, _) in &mut sensors {
                    let readings = Measurements { values: sensor.read() };
                    sender.send(device_state(sensor.local_id(), vec![(measurements_t, readings.encode_to_vec())])).await?;
                }
            }
            message = receiver.next_message() => match message {
                Some(Ok(_)) => {}
                Some(Err(error)) => return Err(error),
                None => return Ok(()),
            },
        }
    }
}

/// A read-only function of a device.
pub fn read_only(name: &str) -> components::DeviceFunction {
    components::DeviceFunction { name: name.to_owned(), risk_class: components::RiskClass::ReadOnly.into() }
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
