// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Framing on WebTransport streams (spec/README.md, "Transport"): each message
//! is preceded by its length as a protobuf varint — protobuf's
//! "length-delimited" format. A datagram carries one message, unframed.

use std::fmt;

use prost::Message;

/// Longest frame accepted on a stream; a longer one ends the connection.
pub const MAX_FRAME: usize = 4 << 20;

/// `message` preceded by its length.
pub fn frame(message: &impl Message) -> Vec<u8> {
    message.encode_length_delimited_to_vec()
}

/// A frame longer than [`MAX_FRAME`], or a malformed length.
#[derive(Debug, PartialEq, Eq)]
pub struct FrameTooLarge;

impl fmt::Display for FrameTooLarge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "frame longer than {MAX_FRAME} bytes")
    }
}

impl std::error::Error for FrameTooLarge {}

/// Reassembles frames from the chunks a stream delivers, however it cuts them.
#[derive(Default)]
pub struct FrameReader {
    buffer: Vec<u8>,
}

impl FrameReader {
    pub fn push(&mut self, chunk: &[u8]) {
        self.buffer.extend_from_slice(chunk);
    }

    /// The payload of the next complete frame, if it has arrived.
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, FrameTooLarge> {
        let mut length = 0u64;
        for (i, byte) in self.buffer.iter().enumerate().take(10) {
            length |= u64::from(byte & 0x7f) << (7 * i);
            if byte & 0x80 != 0 {
                continue;
            }
            let length = usize::try_from(length).map_err(|_| FrameTooLarge)?;
            if length > MAX_FRAME {
                return Err(FrameTooLarge);
            }
            let end = i + 1 + length;
            if self.buffer.len() < end {
                return Ok(None);
            }
            let payload = self.buffer[i + 1..end].to_vec();
            self.buffer.drain(..end);
            return Ok(Some(payload));
        }
        if self.buffer.len() >= 10 { Err(FrameTooLarge) } else { Ok(None) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v0::{ClientMessage, Goodbye, client_message};

    fn goodbye(detail: &str) -> ClientMessage {
        let goodbye = Goodbye { reason: 0, detail: detail.to_owned() };
        ClientMessage { message: Some(client_message::Message::Goodbye(goodbye)) }
    }

    #[test]
    fn frames_survive_any_cut() {
        let (first, second) = (goodbye("first"), goodbye(&"x".repeat(300)));
        let bytes = [frame(&first), frame(&second)].concat();
        for cut in 0..bytes.len() {
            let mut reader = FrameReader::default();
            let mut decoded = Vec::new();
            for chunk in [&bytes[..cut], &bytes[cut..]] {
                reader.push(chunk);
                while let Some(payload) = reader.next_frame().unwrap() {
                    decoded.push(ClientMessage::decode(payload.as_slice()).unwrap());
                }
            }
            assert_eq!(decoded, [first.clone(), second.clone()], "cut at {cut}");
        }
    }

    #[test]
    fn a_frame_too_large_is_refused_before_it_arrives() {
        let mut reader = FrameReader::default();
        let mut header = Vec::new();
        prost::encoding::encode_varint(MAX_FRAME as u64 + 1, &mut header);
        reader.push(&header);
        assert_eq!(reader.next_frame(), Err(FrameTooLarge));
        let mut reader = FrameReader::default();
        reader.push(&[0xff; 10]);
        assert_eq!(reader.next_frame(), Err(FrameTooLarge));
    }
}
