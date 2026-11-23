// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! WAV files: written as 16-bit PCM, read whatever their sample format
//! (8, 16, 24 or 32-bit integers, 32 or 64-bit floats), any number of
//! channels.

use std::{io, path::Path};

/// Sound read from a file: a list of samples per channel, each −1…1.
#[derive(Clone, Debug, PartialEq)]
pub struct Audio {
    /// Samples a second.
    pub rate: u32,
    pub channels: Vec<Vec<f32>>,
}

impl Audio {
    /// Samples in each channel.
    pub fn frames(&self) -> usize {
        self.channels.first().map_or(0, Vec::len)
    }

    pub fn seconds(&self) -> f64 {
        self.frames() as f64 / f64::from(self.rate.max(1))
    }
}

/// The bytes of a WAV file of 16-bit PCM: `samples` interleaved, `channels`
/// of them a frame.
pub fn pcm16(rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
    out.extend_from_slice(&(channels * 2).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

fn bad(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.to_owned())
}

/// The sound in the bytes of a WAV file.
pub fn parse(bytes: &[u8]) -> io::Result<Audio> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(bad("not a WAV file"));
    }
    let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let u32_at = |at: usize| u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    // (format tag, channels, rate, bits a sample)
    let mut format = None;
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let (id, size) = (&bytes[at..at + 4], u32_at(at + 4) as usize);
        let body = at + 8;
        // A data chunk may say more than the file holds (a recording cut short).
        let end = (body + size).min(bytes.len());
        if id == b"fmt " {
            if size < 16 || end < body + 16 {
                return Err(bad("a format chunk too short"));
            }
            let mut tag = u16_at(body);
            if tag == 0xfffe && size >= 26 {
                // Extensible: the true format starts its sub-format.
                tag = u16_at(body + 24);
            }
            format = Some((tag, u16_at(body + 2), u32_at(body + 4), u16_at(body + 14)));
        } else if id == b"data" {
            let (tag, channels, rate, bits) = format.ok_or_else(|| bad("data before its format"))?;
            if channels == 0 || rate == 0 {
                return Err(bad("no channel or no rate"));
            }
            let width = usize::from(bits / 8);
            let sample: fn(&[u8]) -> f32 = match (tag, bits) {
                (1, 8) => |b| (f32::from(b[0]) - 128.0) / 128.0,
                (1, 16) => |b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32_768.0,
                (1, 24) => |b| (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0,
                (1, 32) => |b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0,
                (3, 32) => |b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                (3, 64) => |b| f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]) as f32,
                _ => return Err(bad("a sample format this reader does not know")),
            };
            let frame = width * usize::from(channels);
            let frames = (end - body) / frame;
            let mut out = vec![Vec::with_capacity(frames); usize::from(channels)];
            for f in 0..frames {
                for (c, channel) in out.iter_mut().enumerate() {
                    let start = body + f * frame + c * width;
                    channel.push(sample(&bytes[start..start + width]));
                }
            }
            return Ok(Audio { rate, channels: out });
        }
        // Chunks are padded to an even length.
        at = body + size + (size & 1);
    }
    Err(bad("no data chunk"))
}

/// The sound in a WAV file.
pub fn read(path: impl AsRef<Path>) -> io::Result<Audio> {
    parse(&std::fs::read(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_written_is_read_back() {
        let samples: Vec<i16> = (0..2_000).map(|i| ((i * 37) % 65_536 - 32_768) as i16).collect();
        let audio = parse(&pcm16(48_000, 2, &samples)).unwrap();
        assert_eq!((audio.rate, audio.channels.len(), audio.frames()), (48_000, 2, 1_000));
        for (f, pair) in samples.chunks(2).enumerate() {
            assert_eq!(audio.channels[0][f], f32::from(pair[0]) / 32_768.0);
            assert_eq!(audio.channels[1][f], f32::from(pair[1]) / 32_768.0);
        }
        assert!((audio.seconds() - 1_000.0 / 48_000.0).abs() < 1e-12);
    }

    /// A WAV file with one format chunk and one data chunk.
    fn file(tag: u16, channels: u16, bits: u16, data: &[u8], extensible: bool) -> Vec<u8> {
        let mut fmt = Vec::new();
        fmt.extend_from_slice(&(if extensible { 0xfffe } else { tag }).to_le_bytes());
        fmt.extend_from_slice(&channels.to_le_bytes());
        fmt.extend_from_slice(&44_100u32.to_le_bytes());
        fmt.extend_from_slice(&(44_100 * u32::from(channels) * u32::from(bits / 8)).to_le_bytes());
        fmt.extend_from_slice(&(channels * bits / 8).to_le_bytes());
        fmt.extend_from_slice(&bits.to_le_bytes());
        if extensible {
            fmt.extend_from_slice(&22u16.to_le_bytes());
            fmt.extend_from_slice(&bits.to_le_bytes());
            fmt.extend_from_slice(&3u32.to_le_bytes());
            fmt.extend_from_slice(&tag.to_le_bytes());
            fmt.extend_from_slice(&[0, 0, 0, 0, 16, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71]);
        }
        let mut out = b"RIFF\0\0\0\0WAVE".to_vec();
        // A chunk a reader must step over, of odd length.
        out.extend_from_slice(b"LIST\x03\0\0\0abc\0");
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        out.extend_from_slice(&fmt);
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
        out
    }

    #[test]
    fn other_sample_formats_are_read() {
        let floats: Vec<u8> = [0.5f32, -0.25, 1.0, 0.0].iter().flat_map(|s| s.to_le_bytes()).collect();
        let audio = parse(&file(3, 2, 32, &floats, false)).unwrap();
        assert_eq!(audio.channels, vec![vec![0.5, 1.0], vec![-0.25, 0.0]]);
        assert_eq!(parse(&file(3, 2, 32, &floats, true)).unwrap().channels, audio.channels);

        let half = [0x00, 0x00, 0x40, 0x00, 0x00, 0xc0];
        let audio = parse(&file(1, 1, 24, &half, false)).unwrap();
        assert_eq!(audio.channels, vec![vec![0.5, -0.5]]);
        assert_eq!(audio.rate, 44_100);

        let audio = parse(&file(1, 1, 8, &[128, 255, 0], false)).unwrap();
        assert_eq!(audio.channels[0][0], 0.0);
        assert!(audio.channels[0][1] > 0.99 && audio.channels[0][2] == -1.0);
    }

    #[test]
    fn what_is_not_a_wav_file_is_told() {
        assert!(parse(b"OggS").is_err());
        assert!(parse(b"RIFF\0\0\0\0WAVEdata\x04\0\0\0\0\0\0\0").is_err());
        assert!(parse(&file(85, 2, 16, &[0; 8], false)).is_err());
    }
}
