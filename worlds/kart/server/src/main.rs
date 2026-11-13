// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `kart-server`: the kart world. `[--records FILE]` keeps the world's
//! records in that file across restarts (D-053), `[--ghosts DIR]` its time
//! trials' ghosts in that folder (D-062); `[--track ID|FILE]` races
//! on a built-in track (paddock, coast, forest, harbour) or one read from a
//! track file (D-056), race after race; without it the world races the
//! Grand Tour cup (D-057) — or `[--event cup:ID|track:ID|trial:ID]`, a
//! time trial say (D-062) — and the host may choose another event in the
//! game; `[--laps N]` laps a race (3); `[--weather track|dry|rain]` each
//! track's chance of rain, rolled for every race, or always dry or wet
//! (D-068); `[--music DIR [--music-listen
//! ADDR]]` shares the host's music files in DIR with the players on the network (osage-music's share, on
//! 0.0.0.0:7880 unless told otherwise), each hearing the same piece at the
//! same point; the players talk to one another through a voice relay
//! (osage-voice, UDP 0.0.0.0:7881; `--voice-listen ADDR` to move it,
//! `--no-voice` for none); the other options: see `osage_server::cli`.

use anyhow::Context as _;
use osage_server::Config;
use osage_world_kart::{KartWorld, TICK_HZ};

fn main() -> anyhow::Result<()> {
    let config = Config { world_name: "osage-kart".to_owned(), tick_hz: TICK_HZ, ..Config::default() };
    let mut world = KartWorld { event: Some("cup:grand-tour".to_owned()), ..KartWorld::default() };
    let mut rest = Vec::new();
    let (mut music, mut music_listen) = (None, "0.0.0.0:7880".to_owned());
    let mut voice_listen = Some("0.0.0.0:7881".to_owned());
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--records" {
            world.records = Some(args.next().context("--records needs a file")?.into());
        } else if arg == "--event" {
            let event = args.next().context("--event needs an event: cup:ID, track:ID or trial:ID")?;
            let known = osage_world_kart::events();
            anyhow::ensure!(known.contains(&event), "no event {event}: there are {}", known.join(", "));
            world.event = Some(event);
        } else if arg == "--laps" {
            let laps = args.next().and_then(|laps| laps.parse().ok()).filter(|laps| (1..=20).contains(laps));
            world.laps = laps.context("--laps needs a number of laps, 1 to 20")?;
        } else if arg == "--weather" {
            world.weather = match args.next().as_deref() {
                Some("track") => osage_world_kart::Weather::Track,
                Some("dry") => osage_world_kart::Weather::Dry,
                Some("rain") => osage_world_kart::Weather::Rain,
                _ => anyhow::bail!("--weather takes track, dry or rain"),
            };
        } else if arg == "--bots" {
            let wanted = args.next().unwrap_or_default();
            world.bots =
                osage_kart_racecraft::Difficulty::parse(&wanted).context("--bots takes easy, normal or hard")?;
        } else if arg == "--time" {
            world.time = match args.next().as_deref() {
                Some("track") => osage_world_kart::TimeOfDay::Track,
                Some("day") => osage_world_kart::TimeOfDay::Day,
                Some("night") => osage_world_kart::TimeOfDay::Night,
                _ => anyhow::bail!("--time takes track, day or night"),
            };
        } else if arg == "--ghosts" {
            world.ghosts = Some(args.next().context("--ghosts needs a folder")?.into());
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
        } else if arg == "--music" {
            music = Some(std::path::PathBuf::from(args.next().context("--music needs a folder of music files")?));
        } else if arg == "--voice-listen" {
            voice_listen = Some(args.next().context("--voice-listen needs an address")?);
        } else if arg == "--no-voice" {
            voice_listen = None;
        } else if arg == "--music-listen" {
            music_listen = args.next().context("--music-listen needs an address")?;
        } else {
            rest.push(arg);
        }
    }
    if let Some(address) = voice_listen {
        let socket = std::net::UdpSocket::bind(&address).with_context(|| format!("voice relay on {address}"))?;
        let relay = osage_voice::relay::serve(socket)?;
        eprintln!("voice: relay on port {}", relay.port);
        world.voice_port = Some(relay.port);
    }
    if let Some(folder) = music {
        let (pieces, skipped) =
            osage_music::share::scan(&folder).with_context(|| format!("music folder {}", folder.display()))?;
        for (path, why) in skipped {
            eprintln!("music: left out {}: {why}", path.display());
        }
        anyhow::ensure!(!pieces.is_empty(), "no music to share in {}", folder.display());
        let listener =
            std::net::TcpListener::bind(&music_listen).with_context(|| format!("music on {music_listen}"))?;
        let share = osage_music::share::serve(pieces, listener)?;
        let minutes = share.timeline.pieces.iter().map(|p| p.seconds).sum::<f64>() / 60.0;
        eprintln!("music: {} pieces ({minutes:.0} min) shared on port {}", share.timeline.pieces.len(), share.port);
        // Its threads run for as long as the world.
        world.music_port = Some(share.port);
    }
    osage_server::cli::run_with(world, config, rest)
}
