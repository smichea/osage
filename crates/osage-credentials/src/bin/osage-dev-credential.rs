// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Prints an identity credential signed by the **insecure** development issuer.
//! `osage-dev-credential <per-world-id> [A1|A2|A3] [world] [minutes]`
//! Worlds accept it only when started with `--trust-dev-issuer`.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use osage_credentials::{Level, TestIssuer};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let subject = args.next().context("usage: osage-dev-credential <per-world-id> [A1|A2|A3] [world] [minutes]")?;
    let level = match args.next().as_deref().unwrap_or("A2") {
        "A1" => Level::A1,
        "A2" => Level::A2,
        "A3" => Level::A3,
        other => bail!("unknown level {other}"),
    };
    let world = args.next().unwrap_or_else(|| "osage-demo".to_owned());
    let minutes: u64 = args.next().map_or(Ok(30), |m| m.parse()).context("minutes")?;
    println!("{}", TestIssuer::insecure_dev().issue(&world, &subject, level, Duration::from_secs(minutes * 60))?);
    Ok(())
}
