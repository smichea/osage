// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The relay: beside the world, it hands each player's voice to the
//! others. A player is known by the address they send from, once they
//! said hello; one silent for [`GONE`] is forgotten. The relay hears
//! everything it relays and keeps nothing.

use std::{
    collections::BTreeMap,
    io,
    net::{SocketAddr, UdpSocket},
    time::{Duration, Instant},
};

use crate::wire::Packet;

/// How long a player may be silent (no hello, no voice) before being forgotten.
pub const GONE: Duration = Duration::from_secs(8);
/// How often the roster is sent to everyone.
const ROSTER: Duration = Duration::from_secs(2);

struct Speaker {
    id: u32,
    name: String,
    seen: Instant,
}

/// A relay running on its thread.
pub struct Relay {
    pub port: u16,
}

/// Relays the voices that come to `socket`, on a thread.
pub fn serve(socket: UdpSocket) -> io::Result<Relay> {
    let port = socket.local_addr()?.port();
    socket.set_read_timeout(Some(Duration::from_millis(250)))?;
    std::thread::Builder::new().name("osage-voice-relay".into()).spawn(move || run(&socket))?;
    Ok(Relay { port })
}

fn run(socket: &UdpSocket) {
    let mut speakers: BTreeMap<SocketAddr, Speaker> = BTreeMap::new();
    let mut next = 1u32;
    let mut sent = Instant::now();
    let mut changed = false;
    let mut buffer = [0u8; 1500];
    loop {
        if let Ok((length, from)) = socket.recv_from(&mut buffer) {
            match Packet::decode(&buffer[..length]) {
                Some(Packet::Hello { name }) => {
                    let speaker = speakers.entry(from).or_insert_with(|| {
                        changed = true;
                        next += 1;
                        Speaker { id: next - 1, name: String::new(), seen: Instant::now() }
                    });
                    if speaker.name != name {
                        speaker.name = name;
                        changed = true;
                    }
                    speaker.seen = Instant::now();
                }
                Some(Packet::Voice { seq, frame }) => {
                    if let Some(speaker) = speakers.get_mut(&from) {
                        speaker.seen = Instant::now();
                        let heard = Packet::Heard { speaker: speaker.id, seq, frame }.encode();
                        for to in speakers.keys().filter(|to| **to != from) {
                            let _ = socket.send_to(&heard, to);
                        }
                    }
                }
                Some(Packet::Bye) => changed |= speakers.remove(&from).is_some(),
                _ => {}
            }
        }
        let before = speakers.len();
        speakers.retain(|_, s| s.seen.elapsed() < GONE);
        changed |= speakers.len() != before;
        if changed || sent.elapsed() >= ROSTER {
            let roster = Packet::Roster(speakers.values().map(|s| (s.id, s.name.clone())).collect()).encode();
            for to in speakers.keys() {
                let _ = socket.send_to(&roster, to);
            }
            (sent, changed) = (Instant::now(), false);
        }
    }
}
