// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `kart-server`: the kart world. `[--records FILE]` keeps the world's
//! records in that file across restarts (D-053); `[--track ID|FILE]` races
//! on a built-in track (paddock, coast, forest, harbour) or one read from a
//! track file (D-056), race after race; without it the world races the
//! Grand Tour cup (D-057), and the host may choose another event in the
//! game; the other options: see `osage_server::cli`.

use anyhow::Context as _;
use osage_server::Config;
use osage_world_kart::{KartWorld, TICK_HZ};

fn main() -> anyhow::Result<()> {
    let config = Config { world_name: "osage-kart".to_owned(), tick_hz: TICK_HZ, ..Config::default() };
    let mut world = KartWorld { event: Some("cup:grand-tour".to_owned()), ..KartWorld::default() };
    let mut rest = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--records" {
            world.records = Some(args.next().context("--records needs a file")?.into());
        } else if arg == "--track" {
            let wanted = args.next().context("--track needs a track id or file")?;
            world.track = match osage_kart_tracks::by_id(&wanted) {
                Some(track) => track,
                None => {
                    let text = std::fs::read_to_string(&wanted).with_context(|| format!("no track {wanted}"))?;
                    osage_kart_tracks::TrackDef::parse(&text).with_context(|| format!("track {wanted}"))?
                }
            };
            world.event = None;
            let problems = world.track.problems();
            anyhow::ensure!(problems.is_empty(), "track {wanted}: {}", problems.join("; "));
        } else {
            rest.push(arg);
        }
    }
    osage_server::cli::run_with(world, config, rest)
}
