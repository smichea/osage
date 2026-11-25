// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::super::output::PcmFile;
use anyhow::{Context, Result, ensure};
use osage_paris_voices::RATE;
use std::{
    fs,
    io::{BufWriter, Seek, Write},
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

pub(super) enum Output {
    Pcm(PcmFile),
    Float { writer: BufWriter<NamedTempFile>, target: PathBuf, frames: u32, first: Option<u64> },
}

impl Output {
    pub(super) fn new(path: &Path, float: bool) -> Result<Self> {
        if !float {
            return Ok(Self::Pcm(PcmFile::new(path)?));
        }
        let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        fs::create_dir_all(dir)?;
        let mut writer = BufWriter::new(NamedTempFile::new_in(dir)?);
        writer.write_all(&[0; 58])?;
        Ok(Self::Float { writer, target: path.to_owned(), frames: 0, first: None })
    }

    pub(super) fn write(&mut self, samples: Vec<f32>) -> Result<()> {
        match self {
            Self::Pcm(file) => file.write(samples),
            Self::Float { writer, frames, first, .. } => {
                let (rows, rest) = samples.as_chunks::<2>();
                ensure!(rest.is_empty(), "incomplete stereo float frame");
                for pair in rows {
                    if first.is_none() && *pair != [0.; 2] {
                        *first = Some(u64::from(*frames));
                    }
                    for value in pair {
                        writer.write_all(&value.to_le_bytes())?;
                    }
                    *frames += 1;
                }
                Ok(())
            }
        }
    }

    pub(super) fn finish(self) -> Result<(u64, Option<u64>)> {
        match self {
            Self::Pcm(file) => file.finish(),
            Self::Float { writer, target, frames, first } => {
                let bytes = frames.checked_mul(8).context("float WAV too large")?;
                let mut file = writer.into_inner().map_err(|e| e.into_error())?;
                file.rewind()?;
                file.write_all(b"RIFF")?;
                file.write_all(&(50 + bytes).to_le_bytes())?;
                file.write_all(b"WAVEfmt ")?;
                file.write_all(&18u32.to_le_bytes())?;
                file.write_all(&3u16.to_le_bytes())?; // WAVE_FORMAT_IEEE_FLOAT
                file.write_all(&2u16.to_le_bytes())?;
                file.write_all(&RATE.to_le_bytes())?;
                file.write_all(&(RATE * 8).to_le_bytes())?;
                file.write_all(&8u16.to_le_bytes())?;
                file.write_all(&32u16.to_le_bytes())?;
                file.write_all(&0u16.to_le_bytes())?;
                file.write_all(b"fact")?;
                file.write_all(&4u32.to_le_bytes())?;
                file.write_all(&frames.to_le_bytes())?;
                file.write_all(b"data")?;
                file.write_all(&bytes.to_le_bytes())?;
                file.persist(&target).with_context(|| format!("publishing {}", target.display()))?;
                Ok((u64::from(frames), first))
            }
        }
    }
}
