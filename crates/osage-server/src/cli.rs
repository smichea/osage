// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Command line of a world server, shared by every world's binary:
//! `[--listen ADDR] [--webtransport ADDR|off] [--certificate PEM --private-key PEM]
//!   [--tick HZ] [--world NAME] [--source-url URL]
//!   [--issuers FILE.json] [--trust-dev-issuer] [--sanction PER_WORLD_ID]… [--max-agents N]
//!   [--region-only ADDR | --region-at ADDR]`
//!
//! WebSocket on `--listen` (default 127.0.0.1:7878), WebTransport on
//! `--webtransport` (default 127.0.0.1:7879, UDP) with the given certificate,
//! or a self-signed one for development.
//!
//! The gateway and the region run in one process unless told otherwise
//! (D-037): `--region-only ADDR` runs the region alone and waits for its
//! gateway on `ADDR` (TCP); `--region-at ADDR` runs the gateway alone, linked
//! to the region at `ADDR`. The link is not authenticated (D-038): keep it on
//! a private network.

use std::net::{TcpListener, TcpStream};

use anyhow::{Context, Result, bail};
use tracing_subscriber::EnvFilter;

use crate::{Config, WebTransportConfig, link::network, world::WorldModule};

/// Serves `world` with `config` changed by the command line, until the
/// process ends.
pub fn run(world: impl WorldModule, mut config: Config) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let mut listen = "127.0.0.1:7878".to_owned();
    let (mut webtransport, mut certificate, mut private_key) = ("127.0.0.1:7879".to_owned(), None, None);
    let (mut region_only, mut region_at) = (None, None);
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = || args.next().with_context(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--listen" => listen = value()?,
            "--webtransport" => webtransport = value()?,
            "--certificate" => certificate = Some(value()?.into()),
            "--private-key" => private_key = Some(value()?.into()),
            "--tick" => config.tick_hz = value()?.parse().context("--tick")?,
            "--world" => config.world_name = value()?,
            "--source-url" => config.source_url = value()?,
            "--issuers" => {
                let path = value()?;
                let file = std::fs::read_to_string(&path).with_context(|| format!("reading {path}"))?;
                config.trusted_issuers.extend(serde_json::from_str::<Vec<_>>(&file).context("--issuers")?);
            }
            "--trust-dev-issuer" => {
                tracing::warn!("trusting the INSECURE development issuer: never do this in a real world");
                config.trusted_issuers.push(osage_credentials::TestIssuer::insecure_dev().trusted());
            }
            "--sanction" => {
                config.sanctioned.insert(value()?);
            }
            "--max-agents" => config.max_agents_per_human = value()?.parse().context("--max-agents")?,
            "--region-only" => region_only = Some(value()?),
            "--region-at" => region_at = Some(value()?),
            _ => bail!("unknown argument {flag}"),
        }
    }

    if webtransport != "off" {
        let address = webtransport.parse().context("--webtransport takes ADDRESS:PORT or off")?;
        let certificate = match (certificate, private_key) {
            (Some(chain), Some(key)) => Some((chain, key)),
            (None, None) => None,
            _ => bail!("--certificate and --private-key go together"),
        };
        config.webtransport = Some(WebTransportConfig { address, certificate });
    }
    match (region_only, region_at) {
        (Some(_), Some(_)) => bail!("--region-only and --region-at exclude each other"),
        (Some(address), None) => {
            let links = TcpListener::bind(&address).with_context(|| format!("binding {address}"))?;
            tracing::info!(%address, "region waiting for its gateway");
            let (stream, gateway) = links.accept()?;
            tracing::info!(%gateway, "gateway linked");
            crate::run_region(network::region_end(stream)?, config.tick_hz, world);
            Ok(())
        }
        (None, Some(address)) => {
            let stream = TcpStream::connect(&address).with_context(|| format!("reaching the region at {address}"))?;
            tracing::info!(%address, "linked to the region");
            let listener = TcpListener::bind(&listen).with_context(|| format!("binding {listen}"))?;
            let gateway = crate::spawn_gateway(listener, config, network::gateway_end(stream)?)?;
            gateway.join().map_err(|_| anyhow::anyhow!("the gateway panicked"))
        }
        (None, None) => {
            let listener = TcpListener::bind(&listen).with_context(|| format!("binding {listen}"))?;
            crate::serve(listener, config, world)
        }
    }
}
