// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! `osage-server [--listen ADDR] [--tick HZ] [--world NAME] [--source-url URL]
//!   [--issuers FILE.json] [--trust-dev-issuer] [--sanction PER_WORLD_ID]… [--max-agents N]`

use std::net::TcpListener;

use anyhow::{Context, Result, bail};
use osage_server::Config;
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let mut listen = "127.0.0.1:7878".to_owned();
    let mut config = Config::default();
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = || args.next().with_context(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--listen" => listen = value()?,
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
            _ => bail!("unknown argument {flag}"),
        }
    }

    let listener = TcpListener::bind(&listen).with_context(|| format!("binding {listen}"))?;
    osage_server::run(listener, config)
}
