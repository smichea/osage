// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! A host's playlist, shared with the players of their game on the local
//! network, and heard by everyone at the same point: the playlist plays on
//! the host's clock, track after track, round and round, whether anyone
//! listens or not; a player who tunes in asks where it is, fetches that
//! track and starts from there.
//!
//! The share is a small HTTP server that serves the music files of one
//! folder, and nothing else:
//!
//! - `GET /music/list`: `osage-music/1`, then `track INDEX SECONDS TITLE — ARTIST` a line;
//! - `GET /music/now`: `osage-music/1`, then `now INDEX OFFSET` (seconds into the track);
//! - `GET /music/track/INDEX`: the file.
//!
//! Whether the host may share their music with their friends is theirs to
//! know: the engine shares what it is given.

use std::{
    fmt::Write as _,
    io::{self, BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use crate::Decoded;

/// The first line of the share's answers.
pub const HEADER: &str = "osage-music/1";
/// The largest file shared: a long piece in a lossless format.
const LARGEST: u64 = 200 << 20;

/// A piece of the shared playlist.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    pub title: String,
    pub artist: String,
    pub seconds: f64,
}

impl Piece {
    /// The piece as heard: `title — artist`, or the title alone.
    pub fn heard(&self) -> String {
        if self.artist.is_empty() { self.title.clone() } else { format!("{} — {}", self.title, self.artist) }
    }
}

/// The playlist on the host's clock.
#[derive(Debug, Clone)]
pub struct Timeline {
    pub pieces: Vec<Piece>,
    started: Instant,
}

impl Timeline {
    pub fn new(pieces: Vec<Piece>) -> Self {
        Self { pieces, started: Instant::now() }
    }

    /// Where the playlist is `elapsed` after it started: (piece, seconds into it).
    pub fn at(&self, elapsed: Duration) -> Option<(usize, f64)> {
        let total: f64 = self.pieces.iter().map(|p| p.seconds).sum();
        if total <= 0.0 {
            return None;
        }
        let mut t = elapsed.as_secs_f64() % total;
        for (index, piece) in self.pieces.iter().enumerate() {
            if t < piece.seconds {
                return Some((index, t));
            }
            t -= piece.seconds;
        }
        Some((self.pieces.len() - 1, 0.0))
    }

    pub fn now(&self) -> Option<(usize, f64)> {
        self.at(self.started.elapsed())
    }
}

/// What a folder has to share: its pieces, and the files left out with why.
pub type Scanned = (Vec<(PathBuf, Piece)>, Vec<(PathBuf, String)>);

/// The music files of `folder`, by name, each measured; those that do not
/// decode are left out, with why.
pub fn scan(folder: &Path) -> io::Result<Scanned> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(folder)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.is_file() && crate::is_music(path))
        .collect();
    paths.sort();
    let mut pieces = Vec::new();
    let mut skipped = Vec::new();
    for path in paths {
        match measure(&path) {
            Ok(piece) if piece.seconds > 1.0 => pieces.push((path, piece)),
            Ok(_) => skipped.push((path, "shorter than a second".to_owned())),
            Err(e) => skipped.push((path, e.to_string())),
        }
    }
    Ok((pieces, skipped))
}

fn measure(path: &Path) -> io::Result<Piece> {
    let extension = path.extension().and_then(|e| e.to_str()).map(str::to_owned);
    let decoded = Decoded::open(std::fs::File::open(path)?, extension.as_deref())?;
    let tags = decoded.tags().clone();
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("?").replace(['_'], " ");
    Ok(Piece {
        title: tags.title.unwrap_or(stem),
        artist: tags.artist.unwrap_or_default(),
        seconds: decoded.measure()?,
    })
}

/// A share running: it stops with the process.
pub struct Share {
    pub timeline: Arc<Timeline>,
    pub port: u16,
}

/// Shares `pieces` on `listener`, a thread answering each player.
pub fn serve(pieces: Vec<(PathBuf, Piece)>, listener: TcpListener) -> io::Result<Share> {
    let port = listener.local_addr()?.port();
    let (paths, pieces): (Vec<_>, Vec<_>) = pieces.into_iter().unzip();
    let timeline = Arc::new(Timeline::new(pieces));
    let paths = Arc::new(paths);
    let shared = timeline.clone();
    std::thread::Builder::new().name("osage-music-share".into()).spawn(move || {
        for stream in listener.incoming().flatten() {
            let (timeline, paths) = (shared.clone(), paths.clone());
            let _ = std::thread::Builder::new()
                .name("osage-music-player".into())
                .spawn(move || answer(stream, &timeline, &paths));
        }
    })?;
    Ok(Share { timeline, port })
}

fn answer(mut stream: TcpStream, timeline: &Timeline, paths: &[PathBuf]) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    // The headers say nothing the share needs.
    let mut line = String::new();
    while reader.read_line(&mut line)? > 2 {
        line.clear();
    }
    let mut parts = request.split_whitespace();
    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    if method != "GET" {
        return reply(&mut stream, "405 Method Not Allowed", "text/plain", b"GET only\n");
    }
    match path {
        "/music/list" => {
            let mut text = format!("{HEADER}\n");
            for (index, piece) in timeline.pieces.iter().enumerate() {
                let _ = writeln!(text, "track {index} {:.3} {}", piece.seconds, piece.heard().replace('\n', " "));
            }
            reply(&mut stream, "200 OK", "text/plain; charset=utf-8", text.as_bytes())
        }
        "/music/now" => match timeline.now() {
            Some((index, offset)) => {
                reply(&mut stream, "200 OK", "text/plain", format!("{HEADER}\nnow {index} {offset:.3}\n").as_bytes())
            }
            None => reply(&mut stream, "404 Not Found", "text/plain", b"nothing shared\n"),
        },
        _ => {
            let file =
                path.strip_prefix("/music/track/").and_then(|i| i.parse::<usize>().ok()).and_then(|i| paths.get(i));
            let Some(file) = file else {
                return reply(&mut stream, "404 Not Found", "text/plain", b"no such thing\n");
            };
            let mut bytes = Vec::new();
            std::fs::File::open(file)?.take(LARGEST).read_to_end(&mut bytes)?;
            reply(&mut stream, "200 OK", "application/octet-stream", &bytes)
        }
    }
}

fn reply(stream: &mut TcpStream, status: &str, kind: &str, body: &[u8]) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()
}

/// A share as a player sees it: `http://HOST:PORT`.
#[derive(Debug, Clone, PartialEq)]
pub struct Remote(pub String);

impl Remote {
    pub fn new(host: &str, port: u16) -> Self {
        let host = if host.contains(':') && !host.starts_with('[') { format!("[{host}]") } else { host.to_owned() };
        Self(format!("http://{host}:{port}"))
    }

    fn get(&self, path: &str, limit: u64) -> io::Result<Vec<u8>> {
        let mut response = crate::agent().get(format!("{}{path}", self.0)).call().map_err(io::Error::other)?;
        response.body_mut().with_config().limit(limit).read_to_vec().map_err(io::Error::other)
    }

    /// The shared playlist.
    pub fn list(&self) -> io::Result<Vec<Piece>> {
        let text = String::from_utf8_lossy(&self.get("/music/list", 1 << 20)?).into_owned();
        Ok(read_list(&text))
    }

    /// Where the playlist is: (piece, seconds into it).
    pub fn now(&self) -> io::Result<(usize, f64)> {
        let text = String::from_utf8_lossy(&self.get("/music/now", 4096)?).into_owned();
        read_now(&text).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "not a share's answer"))
    }

    /// A piece's file.
    pub fn track(&self, index: usize) -> io::Result<Vec<u8>> {
        self.get(&format!("/music/track/{index}"), LARGEST)
    }
}

fn read_list(text: &str) -> Vec<Piece> {
    text.lines()
        .filter_map(|line| {
            let mut words = line.strip_prefix("track ")?.splitn(3, ' ');
            let _index = words.next()?;
            let seconds = words.next()?.parse().ok()?;
            let heard = words.next().unwrap_or("");
            let (title, artist) = heard.split_once(" — ").unwrap_or((heard, ""));
            Some(Piece { title: title.to_owned(), artist: artist.to_owned(), seconds })
        })
        .collect()
}

fn read_now(text: &str) -> Option<(usize, f64)> {
    let line = text.lines().find_map(|l| l.strip_prefix("now "))?;
    let (index, offset) = line.split_once(' ')?;
    Some((index.parse().ok()?, offset.trim().parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::tests::wav;

    #[test]
    fn the_playlist_goes_round_on_the_hosts_clock() {
        let piece = |seconds| Piece { title: "t".into(), artist: String::new(), seconds };
        let timeline = Timeline::new(vec![piece(10.0), piece(5.0)]);
        assert_eq!(timeline.at(Duration::from_secs(3)), Some((0, 3.0)));
        assert_eq!(timeline.at(Duration::from_secs(12)), Some((1, 2.0)));
        assert_eq!(timeline.at(Duration::from_secs(16)), Some((0, 1.0)));
        assert_eq!(Timeline::new(vec![]).now(), None);
    }

    #[test]
    fn a_folder_is_shared_and_a_player_tunes_in() {
        let folder = std::env::temp_dir().join(format!("osage-music-share-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("b_second.wav"), wav(22_050, 1, 2.0)).unwrap();
        std::fs::write(folder.join("a_first.wav"), wav(44_100, 2, 3.0)).unwrap();
        std::fs::write(folder.join("notes.txt"), "not music").unwrap();
        std::fs::write(folder.join("broken.ogg"), "not music either").unwrap();
        let (pieces, skipped) = scan(&folder).unwrap();
        assert_eq!(pieces.iter().map(|p| p.1.title.as_str()).collect::<Vec<_>>(), ["a first", "b second"]);
        assert_eq!(skipped.len(), 1, "{skipped:?}");
        let share = serve(pieces, TcpListener::bind("127.0.0.1:0").unwrap()).unwrap();
        let remote = Remote::new("127.0.0.1", share.port);
        let list = remote.list().unwrap();
        assert_eq!(list.iter().map(|p| p.seconds).collect::<Vec<_>>(), [3.0, 2.0]);
        let (index, offset) = remote.now().unwrap();
        assert_eq!(index, 0);
        assert!(offset < 2.0, "{offset}");
        assert_eq!(remote.track(1).unwrap(), std::fs::read(folder.join("b_second.wav")).unwrap());
        assert!(remote.track(2).is_err(), "only what is shared");
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!(Remote::new("fe80::1", 4435).0, "http://[fe80::1]:4435");
    }
}
