// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! IVF, the simplest container of AV1 frames: a 32-byte header (`DKIF`,
//! the codec, the picture's size, the time base, the number of frames),
//! then each frame as its size, its time stamp and its bytes, all little
//! endian.

use anyhow::{Context as _, bail, ensure};

/// An IVF file's frames, each the AV1 data of one picture (a temporal unit).
pub fn frames(file: &[u8]) -> anyhow::Result<Vec<&[u8]>> {
    ensure!(file.len() >= 32 && &file[..4] == b"DKIF", "not an IVF file");
    let header = u16::from_le_bytes([file[6], file[7]]) as usize;
    ensure!(&file[8..12] == b"AV01", "not AV1 ({:?})", String::from_utf8_lossy(&file[8..12]));
    let mut at = header.max(32);
    let mut frames = Vec::new();
    while at < file.len() {
        let head = file.get(at..at + 12).context("an IVF frame header cut short")?;
        let size = u32::from_le_bytes([head[0], head[1], head[2], head[3]]) as usize;
        at += 12;
        let Some(data) = file.get(at..at + size) else { bail!("an IVF frame cut short") };
        frames.push(data);
        at += size;
    }
    Ok(frames)
}

/// An IVF file of AV1 `frames` of `width` by `height`, one a tick.
pub fn write(width: u16, height: u16, frames: &[&[u8]]) -> Vec<u8> {
    let mut file = Vec::with_capacity(32 + frames.iter().map(|f| f.len() + 12).sum::<usize>());
    file.extend_from_slice(b"DKIF");
    file.extend_from_slice(&0u16.to_le_bytes());
    file.extend_from_slice(&32u16.to_le_bytes());
    file.extend_from_slice(b"AV01");
    file.extend_from_slice(&width.to_le_bytes());
    file.extend_from_slice(&height.to_le_bytes());
    file.extend_from_slice(&30u32.to_le_bytes());
    file.extend_from_slice(&1u32.to_le_bytes());
    file.extend_from_slice(&(frames.len() as u32).to_le_bytes());
    file.extend_from_slice(&0u32.to_le_bytes());
    for (i, frame) in frames.iter().enumerate() {
        file.extend_from_slice(&(frame.len() as u32).to_le_bytes());
        file.extend_from_slice(&(i as u64).to_le_bytes());
        file.extend_from_slice(frame);
    }
    file
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_come_back_as_written() {
        let (a, b) = (vec![1u8, 2, 3], vec![9u8; 40]);
        let file = write(256, 128, &[&a, &b]);
        assert_eq!(frames(&file).unwrap(), vec![&a[..], &b[..]]);
        assert!(frames(&file[..file.len() - 1]).is_err());
        assert!(frames(b"RIFF0000000000000000000000000000").is_err());
    }
}
