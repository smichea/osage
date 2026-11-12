// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Looks at music the way the player hears it.
//!
//! - `music probe FILE…`: each file's length, tags, peak and loudness (RMS, dBFS);
//! - `music radio TAG`: asks the radio-browser directory (this goes on the
//!   network) and listens three seconds to the first station that plays;
//! - `music share DIR`: shares a folder on port 4435 until stopped;
//! - `music listen HOST PORT`: tunes in to a share and says what it hears.

use std::time::Duration;

use osage_music::{Decoded, Player, Program, radio, share};

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("probe") => {
            for path in &args[1..] {
                let extension = std::path::Path::new(path).extension().and_then(|e| e.to_str());
                let mut decoded = Decoded::open(std::fs::File::open(path)?, extension)?;
                let tags = decoded.tags().clone();
                let (mut peak, mut sum, mut n) = (0f32, 0f64, 0usize);
                while let Some(samples) = decoded.samples()? {
                    for s in samples {
                        peak = peak.max(s.abs());
                        sum += f64::from(s * s);
                        n += 1;
                    }
                }
                let rms = (sum / n.max(1) as f64).sqrt();
                println!(
                    "{path}: {:.1} s, {tags:?}, peak {peak:.3}, rms {:.1} dBFS",
                    n as f64 / 2.0 / f64::from(osage_music::RATE),
                    20.0 * rms.log10()
                );
            }
        }
        Some("radio") => {
            let tag = args.get(1).cloned().unwrap_or_else(|| "electronic".into());
            let stations = radio::search(&radio::Search { tag, limit: 10, ..Default::default() })?;
            for station in &stations {
                println!(
                    "{} [{} {} kb/s {}] {}",
                    station.name, station.codec, station.bitrate, station.country, station.url
                );
            }
            let player = Player::new();
            let mut ear = player.feed().listen();
            for station in stations.into_iter().take(3) {
                player.play(Program::Radio(station.clone()));
                std::thread::sleep(Duration::from_secs(4));
                let loud = (0..osage_music::RATE).map(|_| ear.sample().abs()).fold(0f32, f32::max);
                println!("{}: {:?}, loudest {loud:.3}", station.name, player.status());
                if loud > 0.01 {
                    break;
                }
            }
        }
        Some("share") => {
            let folder = args.get(1).map(String::as_str).unwrap_or(".");
            let (pieces, skipped) = share::scan(std::path::Path::new(folder))?;
            for (path, why) in skipped {
                eprintln!("left out {}: {why}", path.display());
            }
            let share = share::serve(pieces, std::net::TcpListener::bind("0.0.0.0:4435")?)?;
            for piece in &share.timeline.pieces {
                println!("{:.1} s  {}", piece.seconds, piece.heard());
            }
            loop {
                std::thread::sleep(Duration::from_secs(3600));
            }
        }
        Some("listen") => {
            let (host, port) =
                (args.get(1).map_or("127.0.0.1", String::as_str), args.get(2).map_or("4435", String::as_str));
            let remote = share::Remote::new(host, port.parse().map_err(std::io::Error::other)?);
            let start = std::time::Instant::now();
            let now = remote.now()?;
            println!("now {now:?} after {:?}", start.elapsed());
            let bytes = remote.track(now.0)?;
            println!("{} bytes after {:?}", bytes.len(), start.elapsed());
            let mut decoded = Decoded::open(std::io::Cursor::new(bytes), None)?;
            println!("opened after {:?}", start.elapsed());
            decoded.seek(now.1)?;
            println!("sought after {:?}", start.elapsed());
        }
        _ => eprintln!("music probe FILE… | music radio TAG | music share DIR | music listen HOST PORT"),
    }
    Ok(())
}
