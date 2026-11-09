// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! osage server, phase 1: the replication gateway and one region server in a
//! single process (ADR-0001 §4). The gateway runs on a Tokio runtime; the
//! region is a headless Bevy app on the calling thread.

mod gateway;
mod region;
mod schema;

use std::{net::TcpListener, thread, time::SystemTime};

use anyhow::Result;

pub use region::RegionInput;

#[derive(Clone, Debug)]
pub struct Config {
    pub world_name: String,
    pub tick_hz: u32,
    /// Corresponding Source of this server as it runs (AGPL section 13).
    pub source_url: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            world_name: "osage-demo".to_owned(),
            tick_hz: 30,
            source_url: "https://github.com/smichea/osage".to_owned(),
        }
    }
}

/// Serves the world on `listener` until the process ends.
pub fn run(listener: TcpListener, config: Config) -> Result<()> {
    listener.set_nonblocking(true)?;
    let (region_tx, region_rx) = crossbeam_channel::unbounded();

    let gateway_config = config.clone();
    thread::Builder::new().name("osage-gateway".to_owned()).spawn(move || {
        let runtime = tokio::runtime::Runtime::new().expect("gateway runtime");
        if let Err(error) = runtime.block_on(gateway::serve(listener, gateway_config, region_tx)) {
            tracing::error!(%error, "gateway stopped");
        }
    })?;

    region::app(region_rx, config.tick_hz).run();
    Ok(())
}

/// Microseconds since the Unix epoch: the protocol's server time.
pub(crate) fn now_us() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_micros() as u64)
}
