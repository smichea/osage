// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `kart-server`: the kart world. `[--records FILE]` keeps the world's
//! records in that file across restarts (D-053); the other options: see
//! `osage_server::cli`.

use anyhow::Context as _;
use osage_server::Config;
use osage_world_kart::{KartWorld, TICK_HZ};

fn main() -> anyhow::Result<()> {
    let config = Config { world_name: "osage-kart".to_owned(), tick_hz: TICK_HZ, ..Config::default() };
    let mut world = KartWorld::default();
    let mut rest = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--records" {
            world.records = Some(args.next().context("--records needs a file")?.into());
        } else {
            rest.push(arg);
        }
    }
    osage_server::cli::run_with(world, config, rest)
}
