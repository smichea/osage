// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! What goes between the players and the relay, one UDP datagram each,
//! starting `OSV1` and a kind:
//!
//! - player → relay: `1` hello (a name: who speaks, and that they are
//!   still there), `2` voice (a sequence number, a frame), `3` bye;
//! - relay → player: `4` heard (who, a sequence number, a frame), `5` the
//!   roster (each speaker's number and name).

use crate::FRAME_BYTES;

const MAGIC: &[u8; 4] = b"OSV1";
/// The longest name sent, bytes.
pub const NAME: usize = 32;

#[derive(Debug, Clone, PartialEq)]
pub enum Packet {
    Hello { name: String },
    Voice { seq: u32, frame: Vec<u8> },
    Bye,
    Heard { speaker: u32, seq: u32, frame: Vec<u8> },
    Roster(Vec<(u32, String)>),
}

/// `name` cut to [`NAME`] bytes at a character's edge.
fn short(name: &str) -> &str {
    let mut end = name.len().min(NAME);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    &name[..end]
}

impl Packet {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        match self {
            Self::Hello { name } => {
                out.push(1);
                out.extend(short(name).as_bytes());
            }
            Self::Voice { seq, frame } => {
                out.push(2);
                out.extend(seq.to_le_bytes());
                out.extend(frame);
            }
            Self::Bye => out.push(3),
            Self::Heard { speaker, seq, frame } => {
                out.push(4);
                out.extend(speaker.to_le_bytes());
                out.extend(seq.to_le_bytes());
                out.extend(frame);
            }
            Self::Roster(speakers) => {
                out.push(5);
                let speakers = &speakers[..speakers.len().min(32)];
                out.push(speakers.len() as u8);
                for (id, name) in speakers {
                    let name = short(name);
                    out.extend(id.to_le_bytes());
                    out.push(name.len() as u8);
                    out.extend(name.as_bytes());
                }
            }
        }
        out
    }

    /// A datagram read; `None` if it is not one of these.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let rest = bytes.strip_prefix(MAGIC)?;
        let (kind, rest) = rest.split_first()?;
        let u32_at = |b: &[u8], at: usize| Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?));
        match kind {
            1 if rest.len() <= NAME => Some(Self::Hello { name: String::from_utf8_lossy(rest).trim().to_owned() }),
            2 if rest.len() == 4 + FRAME_BYTES => {
                Some(Self::Voice { seq: u32_at(rest, 0)?, frame: rest[4..].to_vec() })
            }
            3 => Some(Self::Bye),
            4 if rest.len() == 8 + FRAME_BYTES => {
                Some(Self::Heard { speaker: u32_at(rest, 0)?, seq: u32_at(rest, 4)?, frame: rest[8..].to_vec() })
            }
            5 => {
                let (count, mut rest) = rest.split_first()?;
                let mut speakers = Vec::new();
                for _ in 0..*count {
                    let id = u32_at(rest, 0)?;
                    let length = usize::from(*rest.get(4)?);
                    let name = rest.get(5..5 + length)?;
                    speakers.push((id, String::from_utf8_lossy(name).into_owned()));
                    rest = &rest[5 + length..];
                }
                Some(Self::Roster(speakers))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_packet_reads_back_and_others_are_refused() {
        let frame = vec![7u8; FRAME_BYTES];
        let long = "Ä".repeat(40);
        for packet in [
            Packet::Hello { name: "ana".into() },
            Packet::Voice { seq: 9, frame: frame.clone() },
            Packet::Bye,
            Packet::Heard { speaker: 3, seq: 10, frame },
            Packet::Roster(vec![(1, "ana".into()), (2, "bo".into())]),
        ] {
            assert_eq!(Packet::decode(&packet.encode()), Some(packet));
        }
        let Some(Packet::Hello { name }) = Packet::decode(&Packet::Hello { name: long }.encode()) else { panic!() };
        assert_eq!(name.chars().count(), 16, "cut at a character's edge");
        assert_eq!(Packet::decode(b"OSV1\x02short"), None);
        assert_eq!(Packet::decode(b"HTTP/1.1 200"), None);
        assert_eq!(Packet::decode(b"OSV1\x05\x03\x01\x00"), None);
    }
}
