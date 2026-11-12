// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The player: a thread that runs a program — a playlist, a station, a
//! host's share — and hands its samples to a feed, a second and a half
//! ahead. A new program, or a skip, is heard at once: what was fed of the
//! old one is dropped, not played out.

use std::{
    io::{self, Cursor},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
    time::Duration,
};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, TrySendError, bounded, unbounded};

use crate::{Decoded, RATE, icy::Icy, radio::Station, share::Remote};

/// Samples a chunk: a tenth of a second.
const CHUNK: usize = RATE as usize / 10 * 2;
/// Chunks fed ahead: a second and a half.
const AHEAD: usize = 15;
/// How many times a station that stopped is called again before giving up.
const REDIALS: u32 = 5;

/// A piece of music to play.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub title: String,
    pub artist: String,
    /// Its licence, as it is shown (`CC0-1.0`); empty if not known.
    pub licence: String,
    pub origin: Origin,
    /// How loud it is played, times: pieces mastered louder are turned down.
    pub gain: f32,
}

impl Track {
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self {
            title: String::new(),
            artist: String::new(),
            licence: String::new(),
            origin: Origin::File(path.into()),
            gain: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Origin {
    File(PathBuf),
    /// A file fetched whole, then played.
    Url(String),
}

/// What the player plays.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Program {
    #[default]
    Silence,
    /// Tracks one after the other from `first`, round and round.
    Playlist {
        tracks: Vec<Track>,
        first: usize,
    },
    Radio(Station),
    /// A host's playlist, where the host is in it.
    Shared(Remote),
}

/// What is heard.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    /// Where it comes from: a station's name, `shared by the host`, a licence.
    pub from: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Status {
    pub playing: Option<NowPlaying>,
    /// What went wrong last, until something plays again.
    pub problem: Option<String>,
    /// Counts each change of what is heard, for showing it.
    pub changes: u64,
}

enum Command {
    Play(Program),
    Skip(isize),
}

struct Chunk {
    generation: u64,
    samples: Vec<f32>,
}

/// The samples the player feeds, for the audio output: read without
/// waiting; silence when nothing has come.
#[derive(Clone)]
pub struct Feed {
    chunks: Receiver<Chunk>,
    generation: Arc<AtomicU64>,
    gain: Arc<AtomicU32>,
}

impl Feed {
    pub fn listen(&self) -> Listening {
        Listening { feed: self.clone(), chunk: Vec::new(), generation: 0, at: 0 }
    }
}

/// A feed being heard: an endless run of interleaved stereo samples.
pub struct Listening {
    feed: Feed,
    chunk: Vec<f32>,
    generation: u64,
    at: usize,
}

impl Listening {
    pub fn sample(&mut self) -> f32 {
        let now = self.feed.generation.load(Ordering::Acquire);
        if self.at >= self.chunk.len() || self.generation < now {
            loop {
                match self.feed.chunks.try_recv() {
                    Ok(chunk) if chunk.generation < now => continue,
                    Ok(chunk) => {
                        self.chunk = chunk.samples;
                        self.generation = chunk.generation;
                        self.at = 0;
                        break;
                    }
                    Err(_) => {
                        self.chunk.clear();
                        return 0.0;
                    }
                }
            }
            if self.chunk.is_empty() {
                return 0.0;
            }
        }
        let sample = self.chunk[self.at];
        self.at += 1;
        sample * f32::from_bits(self.feed.gain.load(Ordering::Relaxed))
    }
}

impl Iterator for Listening {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        Some(self.sample())
    }
}

pub struct Player {
    commands: Sender<Command>,
    status: Arc<Mutex<Status>>,
    feed: Feed,
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}

impl Player {
    /// A player, silent until given a program.
    pub fn new() -> Self {
        let (commands, orders) = unbounded();
        let (chunks, heard) = bounded(AHEAD);
        let generation = Arc::new(AtomicU64::new(0));
        let status = Arc::new(Mutex::new(Status::default()));
        let feed =
            Feed { chunks: heard, generation: generation.clone(), gain: Arc::new(AtomicU32::new(1f32.to_bits())) };
        let mut thread = Thread { orders, chunks, generation, status: status.clone() };
        std::thread::Builder::new()
            .name("osage-music".into())
            .spawn(move || thread.run())
            .expect("a thread for the music");
        Self { commands, status, feed }
    }

    pub fn play(&self, program: Program) {
        let _ = self.commands.send(Command::Play(program));
    }

    /// The next track, or with `-1` the one before (a playlist only).
    pub fn skip(&self, by: isize) {
        let _ = self.commands.send(Command::Skip(by));
    }

    /// How loud the music is heard, 0 to 1.
    pub fn set_gain(&self, gain: f32) {
        self.feed.gain.store(gain.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn feed(&self) -> Feed {
        self.feed.clone()
    }

    pub fn status(&self) -> Status {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

/// How a piece being played came to stop.
enum Stopped {
    Ended,
    Ordered(Command),
    Failed(io::Error),
    /// The player is gone.
    Gone,
}

struct Thread {
    orders: Receiver<Command>,
    chunks: Sender<Chunk>,
    generation: Arc<AtomicU64>,
    status: Arc<Mutex<Status>>,
}

impl Thread {
    fn run(&mut self) {
        let mut program = Program::Silence;
        let mut index = 0usize;
        loop {
            let stopped = match &program {
                Program::Silence => match self.orders.recv() {
                    Ok(order) => Stopped::Ordered(order),
                    Err(_) => Stopped::Gone,
                },
                Program::Playlist { tracks, .. } if tracks.is_empty() => {
                    self.problem("an empty playlist".into());
                    program = Program::Silence;
                    continue;
                }
                Program::Playlist { tracks, .. } => {
                    let track = tracks[index % tracks.len()].clone();
                    self.track(&track)
                }
                Program::Radio(station) => {
                    let station = station.clone();
                    self.station(&station)
                }
                Program::Shared(remote) => {
                    let remote = remote.clone();
                    self.shared(&remote)
                }
            };
            match stopped {
                Stopped::Gone => return,
                Stopped::Ended => index += 1,
                Stopped::Failed(e) => {
                    let failed_playlist = matches!(&program, Program::Playlist { .. });
                    self.problem(e.to_string());
                    if failed_playlist {
                        index += 1;
                    }
                    // A pause before trying again, that an order cuts short.
                    match self.orders.recv_timeout(Duration::from_secs(2)) {
                        Ok(order) => self.obey(order, &mut program, &mut index),
                        Err(RecvTimeoutError::Disconnected) => return,
                        Err(RecvTimeoutError::Timeout) => {}
                    }
                }
                Stopped::Ordered(order) => self.obey(order, &mut program, &mut index),
            }
        }
    }

    fn obey(&mut self, order: Command, program: &mut Program, index: &mut usize) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        match order {
            Command::Play(next) => {
                *index = match &next {
                    Program::Playlist { first, .. } => *first,
                    _ => 0,
                };
                if next == Program::Silence {
                    self.heard(None);
                }
                *program = next;
            }
            Command::Skip(by) => {
                if let Program::Playlist { tracks, .. } = program {
                    let n = tracks.len().max(1) as isize;
                    *index = (*index as isize + by).rem_euclid(n) as usize;
                }
            }
        }
    }

    fn heard(&self, playing: Option<NowPlaying>) {
        if let Ok(mut status) = self.status.lock() {
            if playing.is_some() {
                status.problem = None;
            }
            status.playing = playing;
            status.changes += 1;
        }
    }

    fn problem(&self, problem: String) {
        if let Ok(mut status) = self.status.lock() {
            status.problem = Some(problem);
            status.playing = None;
            status.changes += 1;
        }
    }

    fn track(&mut self, track: &Track) -> Stopped {
        let decoded = match &track.origin {
            Origin::File(path) => std::fs::File::open(path)
                .and_then(|file| Decoded::open(file, path.extension().and_then(|e| e.to_str()))),
            Origin::Url(url) => fetch(url).and_then(|bytes| Decoded::open(Cursor::new(bytes), extension(url))),
        };
        let decoded = match decoded {
            Ok(decoded) => decoded,
            Err(e) => return Stopped::Failed(io::Error::new(e.kind(), format!("{}: {e}", track.title))),
        };
        let tags = decoded.tags().clone();
        self.heard(Some(NowPlaying {
            title: if track.title.is_empty() { tags.title.unwrap_or_default() } else { track.title.clone() },
            artist: if track.artist.is_empty() { tags.artist.unwrap_or_default() } else { track.artist.clone() },
            from: track.licence.clone(),
        }));
        self.pump(decoded, track.gain)
    }

    fn station(&mut self, station: &Station) -> Stopped {
        for _ in 0..REDIALS {
            let response = crate::agent().get(&station.url).header("Icy-MetaData", "1").call();
            let response = match response {
                Ok(response) => response,
                Err(e) => return Stopped::Failed(io::Error::other(format!("{}: {e}", station.name))),
            };
            let every = response
                .headers()
                .get("icy-metaint")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(0);
            let named = response.headers().get("icy-name").and_then(|v| v.to_str().ok()).map(str::to_owned);
            let by_type = response.headers().get("content-type").and_then(|v| v.to_str().ok()).and_then(|t| {
                if t.contains("mpeg") {
                    Some("mp3")
                } else if t.contains("aac") {
                    Some("aac")
                } else if t.contains("ogg") {
                    Some("ogg")
                } else {
                    None
                }
            });
            let from = named.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| station.name.clone());
            let status = self.status.clone();
            let shown = from.clone();
            let titles = move |title: String| {
                if let Ok(mut status) = status.lock() {
                    let (title, artist) = split_title(&title);
                    status.playing = Some(NowPlaying { title, artist, from: shown.clone() });
                    status.changes += 1;
                }
            };
            let body = Icy::new(response.into_body().into_reader(), every, titles);
            let decoded = match Decoded::stream(body, by_type.or(station.extension())) {
                Ok(decoded) => decoded,
                Err(e) => return Stopped::Failed(io::Error::new(e.kind(), format!("{}: {e}", station.name))),
            };
            self.heard(Some(NowPlaying { title: String::new(), artist: String::new(), from }));
            match self.pump(decoded, 1.0) {
                // A station has no end: it dropped, call again.
                Stopped::Ended | Stopped::Failed(_) => continue,
                other => return other,
            }
        }
        Stopped::Failed(io::Error::other(format!("{} keeps stopping", station.name)))
    }

    fn shared(&mut self, remote: &Remote) -> Stopped {
        let played = remote.now().and_then(|(index, offset)| {
            let pieces = remote.list()?;
            let bytes = remote.track(index)?;
            let mut decoded = Decoded::open(Cursor::new(bytes), None)?;
            // The time the file took to come is time the host played on.
            let (_, again) = remote.now()?;
            decoded.seek(if again >= offset { again } else { offset })?;
            Ok((pieces.get(index).cloned(), decoded))
        });
        match played {
            Ok((piece, decoded)) => {
                let piece = piece.unwrap_or_else(|| crate::share::Piece {
                    title: String::new(),
                    artist: String::new(),
                    seconds: 0.0,
                });
                self.heard(Some(NowPlaying {
                    title: piece.title,
                    artist: piece.artist,
                    from: "shared by the host".into(),
                }));
                self.pump(decoded, 1.0)
            }
            Err(e) => Stopped::Failed(io::Error::new(e.kind(), format!("the host's music: {e}"))),
        }
    }

    /// Decodes and feeds until the piece ends, fails, or an order comes.
    fn pump(&mut self, mut decoded: Decoded, gain: f32) -> Stopped {
        let generation = self.generation.load(Ordering::Acquire);
        let mut pending: Vec<f32> = Vec::with_capacity(CHUNK * 2);
        loop {
            match self.orders.try_recv() {
                Ok(order) => return Stopped::Ordered(order),
                Err(crossbeam_channel::TryRecvError::Disconnected) => return Stopped::Gone,
                Err(crossbeam_channel::TryRecvError::Empty) => {}
            }
            let end = match decoded.samples() {
                Ok(Some(samples)) => {
                    pending.extend(samples.into_iter().map(|s| s * gain));
                    false
                }
                Ok(None) => true,
                Err(e) => return Stopped::Failed(e),
            };
            while pending.len() >= CHUNK || (end && !pending.is_empty()) {
                let take = pending.len().min(CHUNK);
                let mut chunk = Chunk { generation, samples: pending.drain(..take).collect() };
                loop {
                    match self.chunks.try_send(chunk) {
                        Ok(()) => break,
                        Err(TrySendError::Disconnected(_)) => return Stopped::Gone,
                        Err(TrySendError::Full(back)) => {
                            chunk = back;
                            match self.orders.recv_timeout(Duration::from_millis(20)) {
                                Ok(order) => return Stopped::Ordered(order),
                                Err(RecvTimeoutError::Disconnected) => return Stopped::Gone,
                                Err(RecvTimeoutError::Timeout) => {}
                            }
                        }
                    }
                }
            }
            if end {
                return Stopped::Ended;
            }
        }
    }
}

fn fetch(url: &str) -> io::Result<Vec<u8>> {
    let mut response = crate::agent().get(url).call().map_err(io::Error::other)?;
    response.body_mut().with_config().limit(200 << 20).read_to_vec().map_err(io::Error::other)
}

fn extension(url: &str) -> Option<&str> {
    let path = url.split(['?', '#']).next()?;
    let (_, extension) = path.rsplit_once('.')?;
    (!extension.contains('/')).then_some(extension)
}

/// A station's `Artist - Title`, as (title, artist).
fn split_title(text: &str) -> (String, String) {
    match text.split_once(" - ") {
        Some((artist, title)) => (title.trim().to_owned(), artist.trim().to_owned()),
        None => (text.trim().to_owned(), String::new()),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::decode::tests::wav;

    fn file(name: &str, seconds: f32) -> Track {
        let folder = std::env::temp_dir().join(format!("osage-music-player-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join(name);
        std::fs::write(&path, wav(44_100, 2, seconds)).unwrap();
        Track { title: name.into(), artist: "someone".into(), licence: "CC0-1.0".into(), ..Track::file(path) }
    }

    /// Waits up to five seconds for the player to say something.
    fn until(player: &Player, what: impl Fn(&Status) -> bool) -> Status {
        let start = Instant::now();
        loop {
            let status = player.status();
            if what(&status) || start.elapsed() > Duration::from_secs(5) {
                return status;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_playlist_is_fed_and_heard_and_a_skip_is_heard_at_once() {
        let player = Player::new();
        let mut ear = player.feed().listen();
        assert_eq!(ear.sample(), 0.0, "silent before anything plays");
        player.play(Program::Playlist { tracks: vec![file("one.wav", 5.0), file("two.wav", 5.0)], first: 0 });
        let status = until(&player, |s| s.playing.is_some());
        assert_eq!(status.playing.unwrap().title, "one.wav");
        std::thread::sleep(Duration::from_millis(200));
        let loud = (0..RATE as usize).map(|_| ear.sample().abs()).fold(0f32, f32::max);
        assert!(loud > 0.3, "heard: {loud}");
        player.set_gain(0.5);
        let half = (0..2_000).map(|_| ear.sample().abs()).fold(0f32, f32::max);
        assert!(half < 0.3 && half > 0.1, "{half}");
        player.skip(1);
        let status = until(&player, |s| s.playing.as_ref().is_some_and(|p| p.title == "two.wav"));
        assert_eq!(status.playing.unwrap().title, "two.wav");
        player.play(Program::Silence);
        until(&player, |s| s.playing.is_none());
        std::thread::sleep(Duration::from_millis(50));
        let after = (0..20_000).map(|_| ear.sample().abs()).fold(0f32, f32::max);
        assert_eq!(after, 0.0, "what was fed before is dropped");
    }

    #[test]
    fn a_track_that_does_not_play_is_told_and_the_next_one_plays() {
        let player = Player::new();
        let missing = Track { origin: Origin::File("/nowhere/at/all.ogg".into()), ..file("x.wav", 0.1) };
        player.play(Program::Playlist { tracks: vec![missing, file("good.wav", 3.0)], first: 0 });
        let status = until(&player, |s| s.problem.is_some());
        assert!(status.problem.unwrap().contains("x.wav"));
        let status = until(&player, |s| s.playing.is_some());
        assert_eq!(status.playing.unwrap().title, "good.wav");
        assert_eq!(extension("http://h/a/b.mp3?x=1"), Some("mp3"));
        assert_eq!(extension("http://h/live"), None);
        assert_eq!(split_title("Someone - Night Drive"), ("Night Drive".into(), "Someone".into()));
    }

    #[test]
    fn a_hosts_share_is_tuned_in_to_where_the_host_is() {
        let piece = file("shared.wav", 4.0);
        let Origin::File(path) = &piece.origin else { unreachable!() };
        let measured = crate::share::scan(path.parent().unwrap()).unwrap().0;
        let measured: Vec<_> = measured.into_iter().filter(|(p, _)| p == path).collect();
        let share = crate::share::serve(measured, std::net::TcpListener::bind("127.0.0.1:0").unwrap()).unwrap();
        let player = Player::new();
        let mut ear = player.feed().listen();
        player.play(Program::Shared(Remote::new("127.0.0.1", share.port)));
        let status = until(&player, |s| s.playing.is_some() || s.problem.is_some());
        assert_eq!(status.problem, None);
        assert_eq!(status.playing.unwrap().from, "shared by the host");
        std::thread::sleep(Duration::from_millis(200));
        let loud = (0..RATE as usize / 2).map(|_| ear.sample().abs()).fold(0f32, f32::max);
        assert!(loud > 0.3, "heard: {loud}");
    }
}
