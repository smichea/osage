// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! osage server, phase 1: the replication gateway and one region server in a
//! single process (ADR-0001 §4). The gateway runs on a Tokio runtime; the
//! region is a headless Bevy app on the calling thread.

mod gateway;
mod identity;
mod region;
mod schema;

use std::{collections::HashSet, net::TcpListener, thread, time::SystemTime};

use anyhow::Result;
use osage_credentials::TrustedIssuer;
pub use osage_proto::actions::DELEGATE_AGENT;

pub use region::RegionInput;

#[derive(Clone, Debug)]
pub struct Config {
    pub world_name: String,
    pub tick_hz: u32,
    /// Corresponding Source of this server as it runs (AGPL section 13).
    pub source_url: String,
    /// Identity issuers this world trusts (D-013).
    pub trusted_issuers: Vec<TrustedIssuer>,
    /// Per-world identifiers sanctioned in this world; their agents too.
    pub sanctioned: HashSet<String>,
    /// Agents one responsible human may run at once (a world rule, D-008).
    pub max_agents_per_human: usize,
    /// Secret signing this world's agent delegations.
    pub delegation_secret: Vec<u8>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            world_name: "osage-demo".to_owned(),
            tick_hz: 30,
            source_url: "https://github.com/smichea/osage".to_owned(),
            trusted_issuers: Vec::new(),
            sanctioned: HashSet::new(),
            max_agents_per_human: 3,
            delegation_secret: random_secret(),
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

fn random_secret() -> Vec<u8> {
    let mut secret = vec![0; 32];
    getrandom::fill(&mut secret).expect("operating system randomness");
    secret
}

/// Microseconds since the Unix epoch: the protocol's server time.
pub(crate) fn now_us() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_micros() as u64)
}
