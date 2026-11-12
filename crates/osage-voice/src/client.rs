// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! A player in the conference: what they say goes to the relay while
//! they talk; what the others say comes back, each voice queued a few
//! frames deep to ride out the network's unevenness, and mixed for the
//! audio output, which never waits: a voice that has not come is silence.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    io,
    net::{ToSocketAddrs, UdpSocket},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};

use crate::{FRAME, adpcm, wire::Packet};

/// Frames a voice waits for before it is heard: 40 ms.
const START: usize = 2;
/// Frames a voice may fall behind before the oldest are dropped: 160 ms.
const MOST: usize = 8;
/// How often a player says hello: they are still there, and who.
const HELLO: Duration = Duration::from_secs(2);
/// How long after their last frame a speaker is shown as talking.
const TALKING: Duration = Duration::from_millis(300);

struct Speaker {
    queue: VecDeque<f32>,
    playing: bool,
    seq: Option<u32>,
    heard: Instant,
}

#[derive(Default)]
struct Shared {
    roster: BTreeMap<u32, String>,
    muted: BTreeSet<String>,
    speakers: BTreeMap<u32, Speaker>,
}

/// One speaker as shown: who, whether talking now, whether muted.
#[derive(Debug, Clone, PartialEq)]
pub struct Heard {
    pub name: String,
    pub talking: bool,
    pub muted: bool,
}

/// A player's end of the conference. Dropping it says goodbye.
pub struct Voice {
    socket: Arc<UdpSocket>,
    shared: Arc<Mutex<Shared>>,
    seq: AtomicU32,
    gone: Arc<AtomicBool>,
    gain: Arc<AtomicU32>,
}

impl Voice {
    /// Joins the conference of the relay at `relay` as `name`.
    pub fn join(relay: impl ToSocketAddrs, name: &str) -> io::Result<Self> {
        let relay = relay.to_socket_addrs()?.next().ok_or_else(|| io::Error::other("no address for the relay"))?;
        let any = if relay.is_ipv6() { "[::]:0" } else { "0.0.0.0:0" };
        let socket = Arc::new(UdpSocket::bind(any)?);
        socket.connect(relay)?;
        socket.set_read_timeout(Some(Duration::from_millis(250)))?;
        let shared = Arc::new(Mutex::new(Shared::default()));
        let gone = Arc::new(AtomicBool::new(false));
        let hello = Packet::Hello { name: name.to_owned() }.encode();
        let _ = socket.send(&hello);
        let (rx, state, stop) = (socket.clone(), shared.clone(), gone.clone());
        std::thread::Builder::new().name("osage-voice".into()).spawn(move || listen(&rx, &state, &stop, &hello))?;
        Ok(Self { socket, shared, seq: AtomicU32::new(0), gone, gain: Arc::new(AtomicU32::new(1f32.to_bits())) })
    }

    /// Says a frame ([`FRAME`] samples at [`RATE`](crate::RATE)).
    pub fn say(&self, frame: &[i16]) {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        let _ = self.socket.send(&Packet::Voice { seq, frame: adpcm::encode(frame) }.encode());
    }

    /// Mutes or unmutes a speaker, by name.
    pub fn mute(&self, name: &str, muted: bool) {
        if let Ok(mut shared) = self.shared.lock() {
            if muted {
                shared.muted.insert(name.to_owned());
            } else {
                shared.muted.remove(name);
            }
        }
    }

    /// How loud the others are heard, 0 to 2.
    pub fn set_gain(&self, gain: f32) {
        self.gain.store(gain.clamp(0.0, 2.0).to_bits(), Ordering::Relaxed);
    }

    /// The others, by name.
    pub fn heard(&self) -> Vec<Heard> {
        let Ok(shared) = self.shared.lock() else { return Vec::new() };
        shared
            .roster
            .iter()
            .map(|(id, name)| Heard {
                name: name.clone(),
                talking: shared.speakers.get(id).is_some_and(|s| s.heard.elapsed() < TALKING),
                muted: shared.muted.contains(name),
            })
            .collect()
    }

    /// The voices, mixed, for the audio output.
    pub fn feed(&self) -> Feed {
        Feed { shared: self.shared.clone(), gain: self.gain.clone(), mixed: Vec::new(), at: 0 }
    }
}

impl Drop for Voice {
    fn drop(&mut self) {
        self.gone.store(true, Ordering::Relaxed);
        let _ = self.socket.send(&Packet::Bye.encode());
    }
}

fn listen(socket: &UdpSocket, shared: &Mutex<Shared>, gone: &AtomicBool, hello: &[u8]) {
    let mut said = Instant::now();
    let mut buffer = [0u8; 1500];
    while !gone.load(Ordering::Relaxed) {
        if said.elapsed() >= HELLO {
            let _ = socket.send(hello);
            said = Instant::now();
        }
        let Ok(length) = socket.recv(&mut buffer) else { continue };
        let Ok(mut shared) = shared.lock() else { return };
        match Packet::decode(&buffer[..length]) {
            Some(Packet::Roster(roster)) => {
                shared.roster = roster.into_iter().collect();
                let known: BTreeSet<u32> = shared.roster.keys().copied().collect();
                shared.speakers.retain(|id, _| known.contains(id));
            }
            Some(Packet::Heard { speaker, seq, frame }) => {
                let muted = shared.roster.get(&speaker).is_some_and(|name| shared.muted.contains(name));
                let Some(samples) = adpcm::decode(&frame, FRAME).filter(|_| !muted) else { continue };
                let voice = shared.speakers.entry(speaker).or_insert_with(|| Speaker {
                    queue: VecDeque::new(),
                    playing: false,
                    seq: None,
                    heard: Instant::now(),
                });
                // Late or twice: dropped (a wrap of the count starts again).
                if voice.seq.is_some_and(|last| seq <= last && last - seq < 1 << 30) {
                    continue;
                }
                voice.seq = Some(seq);
                voice.heard = Instant::now();
                voice.queue.extend(samples.iter().map(|s| f32::from(*s) / 32_768.0));
                if voice.queue.len() > MOST * FRAME {
                    let over = voice.queue.len() - START * FRAME;
                    voice.queue.drain(..over);
                }
            }
            _ => {}
        }
    }
}

/// The voices mixed: an endless run of mono samples at [`RATE`](crate::RATE).
pub struct Feed {
    shared: Arc<Mutex<Shared>>,
    gain: Arc<AtomicU32>,
    mixed: Vec<f32>,
    at: usize,
}

impl Feed {
    pub fn sample(&mut self) -> f32 {
        if self.at >= self.mixed.len() {
            self.mix();
        }
        let sample = self.mixed.get(self.at).copied().unwrap_or(0.0);
        self.at += 1;
        sample
    }

    /// A frame's worth of every voice heard, added.
    fn mix(&mut self) {
        self.mixed.clear();
        self.mixed.resize(FRAME, 0.0);
        self.at = 0;
        let gain = f32::from_bits(self.gain.load(Ordering::Relaxed));
        let Ok(mut shared) = self.shared.lock() else { return };
        for voice in shared.speakers.values_mut() {
            if !voice.playing {
                voice.playing = voice.queue.len() >= START * FRAME;
                if !voice.playing {
                    continue;
                }
            }
            for out in &mut self.mixed {
                match voice.queue.pop_front() {
                    Some(s) => *out += s * gain,
                    None => {
                        voice.playing = false;
                        break;
                    }
                }
            }
        }
        for out in &mut self.mixed {
            // Several at once: kept from clipping, softly.
            *out = out.tanh();
        }
    }
}

impl Iterator for Feed {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        Some(self.sample())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn until(what: impl Fn() -> bool) {
        let start = Instant::now();
        while !what() && start.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn what_one_says_the_others_hear_unless_they_muted_them() {
        let relay = crate::relay::serve(UdpSocket::bind("127.0.0.1:0").unwrap()).unwrap();
        let at = ("127.0.0.1", relay.port);
        let (ana, bo, cy) =
            (Voice::join(at, "ana").unwrap(), Voice::join(at, "bo").unwrap(), Voice::join(at, "cy").unwrap());
        until(|| bo.heard().len() == 3 && cy.heard().len() == 3);
        assert_eq!(bo.heard().iter().map(|h| h.name.as_str()).collect::<Vec<_>>(), ["ana", "bo", "cy"]);
        cy.mute("ana", true);
        let tone: Vec<i16> = (0..FRAME).map(|n| ((n as f32 * 0.2).sin() * 12_000.0) as i16).collect();
        let (mut bo_ear, mut cy_ear) = (bo.feed(), cy.feed());
        for _ in 0..6 {
            ana.say(&tone);
        }
        until(|| bo.heard().iter().any(|h| h.name == "ana" && h.talking));
        std::thread::sleep(Duration::from_millis(50));
        let loud = |ear: &mut Feed| (0..FRAME * 4).map(|_| ear.sample().abs()).fold(0f32, f32::max);
        assert!(loud(&mut bo_ear) > 0.2, "bo hears ana");
        assert_eq!(loud(&mut cy_ear), 0.0, "cy muted ana");
        assert!(cy.heard().iter().any(|h| h.name == "ana" && h.muted));
        assert_eq!(loud(&mut ana.feed()), 0.0, "nobody hears themselves");
        drop(cy);
        until(|| bo.heard().len() == 2);
        assert_eq!(bo.heard().len(), 2, "a player who leaves leaves the roster");
    }
}
