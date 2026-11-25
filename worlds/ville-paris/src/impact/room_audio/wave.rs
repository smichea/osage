// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use anyhow::{Context, Result, ensure};
use osage_paris_voices::RATE;
use std::{fs::File, io::Read, path::Path};

const MAX_FRAMES: usize = RATE as usize * 60;
// Eight bytes per mono frame for float64 and space for ancillary chunks.
const MAX_BYTES: usize = MAX_FRAMES * 8 + 1024 * 1024;

pub(super) fn read(path: &Path) -> Result<Vec<f32>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= MAX_BYTES, "source WAV exceeds the 60-second file budget");
    ensure!(bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE", "expected RIFF WAVE");
    let size = |v: &[u8]| u32::from_le_bytes(v.try_into().unwrap()) as usize;
    let end = 8 + size(&bytes[4..8]);
    ensure!(end >= 12 && end <= bytes.len(), "truncated or invalid RIFF length");
    // The shared analysis decoder tolerates recordings cut off in transit. A published render
    // requires complete samples and validates the declared format before allocating channels.
    let mut stride = None;
    let mut frames = None;
    let mut pos = 12;
    while pos < end {
        ensure!(end - pos >= 8, "truncated WAV chunk header");
        let id = &bytes[pos..pos + 4];
        let n = size(&bytes[pos + 4..pos + 8]);
        pos += 8;
        ensure!(n <= end - pos, "truncated WAV chunk");
        let data = &bytes[pos..pos + n];
        if id == b"fmt " {
            ensure!(stride.is_none() && n >= 16, "invalid WAV format chunk");
            let word = |off| u16::from_le_bytes([data[off], data[off + 1]]);
            ensure!(word(2) == 1 && size(&data[4..8]) == RATE as usize, "source WAV must be mono at 48000 Hz");
            if word(0) == 0xfffe {
                ensure!(n >= 40 && word(16) >= 22, "truncated extensible WAV format");
            }
            let bits = word(14);
            ensure!(matches!(bits, 8 | 16 | 24 | 32 | 64) && word(12) == bits / 8, "invalid WAV frame size");
            stride = Some(usize::from(bits / 8));
        } else if id == b"data" {
            let stride = stride.context("WAV data precedes its format")?;
            ensure!(frames.is_none() && n.is_multiple_of(stride), "incomplete or duplicate WAV sample data");
            ensure!(n / stride <= MAX_FRAMES, "source WAV exceeds 60 seconds");
            frames = Some(n / stride);
        }
        pos += n;
        if n % 2 == 1 && pos < end {
            pos += 1;
        }
    }
    let frames = frames.context("missing WAV sample data")?;
    ensure!(frames != 0, "empty source WAV");
    let mut audio = osage_sound_analysis::wav::parse(&bytes[..end])?;
    let mono = audio.channels.pop().context("missing WAV channel")?;
    ensure!(mono.len() == frames && mono.iter().all(|v| v.is_finite()), "invalid source samples");
    Ok(mono)
}
