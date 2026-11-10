// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! osage server, phase 1: the replication gateway and one region server in a
//! single process (ADR-0001 §4). The gateway runs on a Tokio runtime and
//! accepts WebSocket and WebTransport clients; the region is a headless Bevy
//! app on the calling thread, running a world module ([`world`]). They talk
//! only by the messages of `osage-region-link` (D-037), here through
//! channels.

pub mod cli;
pub mod demo;
mod gateway;
mod identity;
mod region;
mod websocket;
mod webtransport;
pub mod world;

pub(crate) use osage_world_module_api::schema;

use std::{
    collections::HashSet,
    net::{SocketAddr, TcpListener},
    path::PathBuf,
    thread,
    time::SystemTime,
};

use anyhow::Result;
use osage_credentials::TrustedIssuer;
pub use osage_proto::actions::{DELEGATE_AGENT, DELEGATE_BRIDGE};
/// The messages between the gateway and a region (D-037), for serving them apart.
pub use osage_region_link as link;

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
    /// WebTransport endpoint (ADR-0002 §3), announced on the WebSocket host;
    /// `None`: WebSocket only.
    pub webtransport: Option<WebTransportConfig>,
}

#[derive(Clone, Debug)]
pub struct WebTransportConfig {
    /// UDP address to listen on; port 0 picks a free port.
    pub address: SocketAddr,
    /// PEM certificate chain and private key. `None`: a self-signed
    /// certificate valid for 14 days, announced with its SHA-256 hash, which
    /// browsers accept for development (`serverCertificateHashes`).
    pub certificate: Option<(PathBuf, PathBuf)>,
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
            webtransport: None,
        }
    }
}

/// Serves the demo world ([`demo::DemoWorld`]) on `listener` until the
/// process ends.
pub fn run(listener: TcpListener, config: Config) -> Result<()> {
    serve(listener, config, demo::DemoWorld)
}

/// Serves `world` on `listener` until the process ends: the gateway on a
/// thread of its own, the region on the calling thread, linked in the process.
pub fn serve(listener: TcpListener, config: Config, world: impl world::WorldModule) -> Result<()> {
    let (gateway, region) = osage_region_link::in_process();
    spawn_gateway(listener, config.clone(), gateway)?;
    run_region(region, config.tick_hz, world);
    Ok(())
}

/// The gateway alone, on a thread of its own, for the region at the other end
/// of `link` — in this process or, through `osage_region_link::network`, on
/// another machine. It serves sessions once the region says it is ready.
pub fn spawn_gateway(
    listener: TcpListener,
    config: Config,
    link: osage_region_link::GatewayEnd,
) -> Result<thread::JoinHandle<()>> {
    listener.set_nonblocking(true)?;
    Ok(thread::Builder::new().name("osage-gateway".to_owned()).spawn(move || {
        let runtime = tokio::runtime::Runtime::new().expect("gateway runtime");
        if let Err(error) = runtime.block_on(gateway::serve(listener, config, link)) {
            tracing::error!(%error, "gateway stopped");
        }
    })?)
}

/// A region alone, running `world` on the calling thread until the process
/// ends, for the gateway at the other end of `link`.
pub fn run_region(link: osage_region_link::RegionEnd, tick_hz: u32, world: impl world::WorldModule) {
    let schema = schema::Schema::new(&world);
    region::app(link, tick_hz, schema, world).run();
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
