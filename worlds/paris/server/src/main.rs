// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `paris-server`: the Paris world (D-136, D-137) and, beside it, its
//! server of light.
//!
//!     paris-server [--street DIR] [--light-listen ADDR] [--horizon SECONDS] [osage-server options]
//!
//! DIR holds what `tools/bistro/export.py`, `tools/bistro/cube-probes.py`
//! and `tools/bistro/tiles.py` made of the street (default:
//! `orca/bistro/out` under the asset manifest's content): its walk grid
//! (`walk.json`, `walk.bin`) and its light (`tiles/`, listed in
//! `tiles/index.json`): the probes inside its cubes, and the blocks'
//! surface tiles where those are not baked. The server of light listens on
//! ADDR (0.0.0.0:7883 unless told) and answers `/need?walker=ID` with the
//! probes and tiles of the cubes that walker can reach within the horizon
//! (2 s unless told), from where it is in the world and how fast it goes —
//! not from what its client says — then sends each at
//! `/h/KEY/{colour,depth,mask}.ivf` (and `meta.json`, where a probe
//! stands). Clients learn its port from the world's `paris.world/1`.

use std::{collections::HashSet, net::TcpListener, path::PathBuf, sync::Arc};

use anyhow::Context as _;
use osage_hologram_stream::http;
use osage_server::Config;
use osage_world_paris::{ParisWorld, TICK_HZ, Walkers, tiles, walk::Walk};

fn main() -> anyhow::Result<()> {
    let mut street: Option<PathBuf> = None;
    let mut light_listen = "0.0.0.0:7883".to_owned();
    let mut horizon = 2.0f32;
    let mut rest = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--street" => street = Some(args.next().context("--street needs a folder")?.into()),
            "--light-listen" => light_listen = args.next().context("--light-listen needs an address")?,
            "--horizon" => horizon = args.next().and_then(|h| h.parse().ok()).context("--horizon needs seconds")?,
            _ => rest.push(arg),
        }
    }
    let street = match street {
        Some(folder) => folder,
        None => osage_content::root().context("no asset content here: give --street")?.join("orca/bistro/out"),
    };
    let walk = Walk::parse(
        &std::fs::read_to_string(street.join("walk.json")).context("walk.json (tools/bistro/export.py)")?,
        &std::fs::read(street.join("walk.bin")).context("walk.bin")?,
    )
    .map_err(anyhow::Error::msg)?;
    let tiles_folder = street.join("tiles");
    let present: HashSet<String> = std::fs::read_to_string(tiles_folder.join("index.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Vec<String>>(&text).ok())
        .unwrap_or_default()
        .into_iter()
        .collect();
    eprintln!("paris: walk grid of {}, {} tiles of light", street.display(), present.len());
    let walkers = Walkers::default();
    let listener =
        TcpListener::bind(&light_listen).with_context(|| format!("the server of light on {light_listen}"))?;
    let port = listener.local_addr()?.port();
    let seen = walkers.clone();
    let need: Arc<http::Need> = Arc::new(move |query: &str| {
        let id = query.split('&').find_map(|kv| kv.strip_prefix("walker="))?.parse::<u64>().ok()?;
        let walker = *seen.0.lock().ok()?.get(&id)?;
        // The probes inside the cubes on the walker's way (D-137, D-139), or
        // their blocks' surface tiles where those are not baked (D-136).
        let wanted = tiles::needed_light(walker.eyes, walker.velocity, horizon, |k| present.contains(k));
        serde_json::to_string(&wanted).ok()
    });
    http::serve(listener, tiles_folder, need)?;
    eprintln!("paris: light on port {port}, {horizon} s ahead");
    let world = ParisWorld { walk: Arc::new(walk), walkers, light_port: port, horizon };
    let config = Config { world_name: "osage-paris".to_owned(), tick_hz: TICK_HZ, ..Config::default() };
    osage_server::cli::run_with(world, config, rest)
}
