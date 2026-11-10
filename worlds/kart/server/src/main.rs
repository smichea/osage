// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `kart-server`: the kart world. Options: see `osage_server::cli`.

use osage_server::Config;
use osage_world_kart::{KartWorld, TICK_HZ};

fn main() -> anyhow::Result<()> {
    let config = Config { world_name: "osage-kart".to_owned(), tick_hz: TICK_HZ, ..Config::default() };
    osage_server::cli::run(KartWorld::default(), config)
}
