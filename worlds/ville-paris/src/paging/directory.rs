// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::{source::Source, tiles::File};
use anyhow::{Context, Result, ensure};
use flate2::{Decompress, FlushDecompress, Status};
use std::str::FromStr;

const LIMIT: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Format {
    Full,
    VisualRegional,
    FirstFrame,
    Physics,
    PhysicsColumns,
    RegionalAddresses,
}

pub(super) struct Pin {
    pub file: File,
    update: Option<Box<Update>>,
}

struct Update {
    target: File,
    previous: File,
    base: File,
    patch: File,
}

impl FromStr for Pin {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        let mut fields = value.split('~');
        let file = address(fields.next().unwrap_or_default(), "district")?;
        let update = if let Some(target) = fields.next() {
            let mut next = |ext| address(fields.next().context("incomplete first-frame update")?, ext);
            let update = Update {
                target: address(target, "frame")?,
                previous: next("district")?,
                base: next("frame")?,
                patch: next("delta")?,
            };
            ensure!(fields.next().is_none(), "unexpected first-frame update field");
            Some(Box::new(update))
        } else {
            None
        };
        Ok(Self { file, update })
    }
}

fn address(value: &str, ext: &str) -> Result<File> {
    let (sha256, length) = value.split_once(':').context("expected SHA256:BYTES")?;
    let bytes = length.parse::<usize>().context("invalid pinned length")?;
    let file = File { file: format!("{sha256}.{ext}"), sha256: sha256.to_owned(), bytes };
    file.validate(ext)?;
    ensure!(bytes <= LIMIT, "pinned content exceeds size limit");
    Ok(file)
}

impl Update {
    fn apply(&self, source: &Source) -> Result<Option<Vec<u8>>> {
        let base = if let Some(raw) = source.cached(&self.base)? {
            raw
        } else if let Some(wire) = source.cached(&self.previous)? {
            let (kind, raw) = decode(&wire)?;
            ensure!(kind == Format::FirstFrame, "wrong previous package kind");
            self.base.check(&raw)?;
            raw
        } else {
            return Ok(None);
        };
        let patch = self.patch.read(source)?;
        let raw = decode_dictionary(&patch, self.target.bytes, &base)?;
        self.target.check(&raw)?;
        source.store_verified(&self.target, &raw)?;
        Ok(Some(raw))
    }
}

impl Pin {
    pub fn read(&self, source: &Source) -> Result<(Format, Vec<u8>)> {
        let Some(update) = &self.update else { return decode(&self.file.read(source)?) };
        if let Some(raw) = source.cached(&update.target)? {
            return Ok((Format::FirstFrame, raw));
        }
        let wire = if let Some(wire) = source.cached(&self.file)? {
            wire
        } else if update.patch.bytes < self.file.bytes
            && let Ok(Some(raw)) = update.apply(source)
        {
            return Ok((Format::FirstFrame, raw));
        } else {
            self.file.read(source)?
        };
        let (kind, raw) = decode(&wire)?;
        ensure!(kind == Format::FirstFrame, "update requires first-frame package");
        update.target.check(&raw)?;
        Ok((kind, raw))
    }
}

fn decode(wire: &[u8]) -> Result<(Format, Vec<u8>)> {
    let format = match wire.get(..4) {
        Some(b"ODI1") => Format::Full,
        Some(b"ODI2") => Format::VisualRegional,
        Some(b"ODI3") => Format::FirstFrame,
        Some(b"ODP1") => Format::Physics,
        Some(b"ODP2") => Format::PhysicsColumns,
        Some(b"ODP3") => Format::RegionalAddresses,
        _ => anyhow::bail!("invalid pinned directory signature"),
    };
    let size = u32::from_le_bytes(wire.get(4..8).context("missing directory length")?.try_into()?) as usize;
    ensure!(size > 0 && size <= LIMIT, "invalid decoded directory length");
    let body = &wire[8..];
    if body.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
        return Ok((format, decode_zstd(body, size)?));
    }
    let mut raw = vec![0; size];
    let mut decoder = Decompress::new(true);
    let status = decoder.decompress(body, &mut raw, FlushDecompress::Finish)?;
    ensure!(
        status == Status::StreamEnd && decoder.total_in() == body.len() as u64 && decoder.total_out() == size as u64,
        "invalid or incomplete directory zlib stream"
    );
    Ok((format, raw))
}

pub(super) fn decode_zstd(bytes: &[u8], size: usize) -> Result<Vec<u8>> {
    decode_dictionary(bytes, size, &[])
}

fn decode_dictionary(bytes: &[u8], size: usize, dictionary: &[u8]) -> Result<Vec<u8>> {
    let declared = zstd::zstd_safe::get_frame_content_size(bytes)
        .map_err(|_| anyhow::anyhow!("invalid Zstandard catalogue header"))?;
    ensure!(declared == Some(size as u64), "catalogue size mismatch");
    let frame = zstd::zstd_safe::find_frame_compressed_size(bytes)
        .map_err(|_| anyhow::anyhow!("incomplete Zstandard catalogue frame"))?;
    ensure!(frame == bytes.len(), "trailing Zstandard catalogue data");
    let mut raw = Vec::new();
    raw.try_reserve_exact(size).context("cannot allocate catalogue")?;
    zstd::bulk::Decompressor::with_dictionary(dictionary)?.decompress_to_buffer(bytes, &mut raw)?;
    ensure!(raw.len() == size, "catalogue size mismatch");
    Ok(raw)
}
