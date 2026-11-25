// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Reconstruct the exact coarse GLB, including decimal metadata and raw image/vertex bytes.
mod columns;

use anyhow::{Context, Result, bail, ensure};
use flate2::{Decompress, FlushDecompress, Status};
use std::ops::Range;

const LIMIT: usize = 256 << 20;
const RESULTS: usize = 8 << 20;

struct Input<'a>(&'a [u8]);

impl<'a> Input<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let (part, rest) = self.0.split_at_checked(n).context("truncated coarse data")?;
        self.0 = rest;
        Ok(part)
    }

    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn count(&mut self, limit: usize) -> Result<usize> {
        let n = u32::from_le_bytes(self.take(4)?.try_into()?) as usize;
        ensure!(n <= limit, "excessive coarse count");
        Ok(n)
    }

    fn blob(&mut self) -> Result<&'a [u8]> {
        let n = self.count(LIMIT)?;
        self.take(n)
    }

    fn end(&self) -> Result<()> {
        ensure!(self.0.is_empty(), "trailing coarse data");
        Ok(())
    }
}

fn append(out: &mut Vec<u8>, bytes: &[u8], limit: usize) -> Result<()> {
    ensure!(bytes.len() <= limit.saturating_sub(out.len()), "coarse output exceeds its declared length");
    out.extend_from_slice(bytes);
    Ok(())
}

struct Pattern {
    corners: Vec<usize>,
    distinct: usize,
}

fn points(input: &mut Input<'_>, spans: &[Range<usize>], output: &mut [u8]) -> Result<()> {
    let point_count = spans.iter().map(|s| s.len() / 12).sum();
    let mut budget = point_count;
    let mut patterns = Vec::new();
    for _ in 0..input.count(spans.len())? {
        let corners = columns::indices(input.blob()?, budget)?;
        ensure!(!corners.is_empty(), "empty coarse corner pattern");
        budget -= corners.len();
        let mut distinct = 0;
        for &i in &corners {
            ensure!(i <= distinct, "invalid coarse local corner");
            if i == distinct {
                distinct += 1;
            }
        }
        patterns.push(Pattern { corners, distinct });
    }
    let selected = columns::indices(input.blob()?, spans.len())?;
    ensure!(selected.len() == spans.len(), "coarse position span count mismatch");
    let mut distinct = 0;
    for (&id, span) in selected.iter().zip(spans) {
        let pat = patterns.get(id).context("invalid coarse position pattern")?;
        ensure!(pat.corners.len().checked_mul(12) == Some(span.len()), "coarse position span length mismatch");
        distinct += pat.distinct;
    }
    let mut xyz = vec![[0u8; 12]; distinct];
    for axis in 0..3 {
        let table = input.blob()?;
        ensure!(table.len().is_multiple_of(4) && table.len() / 4 <= distinct, "invalid coarse axis dictionary");
        let refs = columns::indices(input.blob()?, distinct)?;
        ensure!(refs.len() == distinct, "coarse coordinate count mismatch");
        for (dst, id) in xyz.iter_mut().zip(refs) {
            let word = id
                .checked_mul(4)
                .and_then(|offset| table.get(offset..)?.get(..4))
                .context("invalid coarse coordinate reference")?;
            dst[axis * 4..axis * 4 + 4].copy_from_slice(word);
        }
    }
    let mut start = 0;
    for (id, span) in selected.into_iter().zip(spans) {
        let pattern = &patterns[id];
        for (dst, i) in output[span.clone()].as_chunks_mut::<12>().0.iter_mut().zip(&pattern.corners) {
            dst.copy_from_slice(&xyz[start + i]);
        }
        start += pattern.distinct;
    }
    Ok(())
}

fn binary(data: &[u8], output: &mut [u8]) -> Result<()> {
    let mut input = Input(data);
    let groups = columns::indices(input.blob()?, output.len())?;
    let sizes = columns::indices(input.blob()?, output.len())?;
    ensure!(groups.len() == sizes.len(), "coarse range count mismatch");
    let mut spans = vec![Vec::new(); input.count(groups.len())?];
    let mut offset = 0usize;
    for (id, size) in groups.into_iter().zip(sizes) {
        let end = offset.checked_add(size).context("coarse range size overflow")?;
        ensure!(size != 0 && end <= output.len(), "invalid coarse binary range");
        spans.get_mut(id).context("invalid coarse range group")?.push(offset..end);
        offset = end;
    }
    ensure!(offset == output.len(), "incomplete coarse binary coverage");
    for group in spans {
        ensure!(!group.is_empty(), "empty coarse binary group");
        let mut content = Input(input.blob()?);
        match content.byte()? {
            0 => {
                for span in group {
                    output[span.clone()].copy_from_slice(content.take(span.len())?);
                }
            }
            1 => {
                let refs = columns::indices(content.blob()?, group.len())?;
                ensure!(refs.len() == group.len(), "coarse view count mismatch");
                let mut views = Vec::new();
                for _ in 0..content.count(group.len())? {
                    views.push(content.blob()?);
                }
                for (span, id) in group.into_iter().zip(refs) {
                    let view = views.get(id).context("invalid coarse view reference")?;
                    ensure!(view.len() == span.len(), "coarse view length mismatch");
                    output[span].copy_from_slice(view);
                }
            }
            2 => points(&mut content, &group, output)?,
            mode => bail!("unknown coarse binary mode {mode}"),
        }
        content.end()?;
    }
    input.end()
}

pub(super) fn unpack(data: &[u8]) -> Result<Vec<u8>> {
    let mut header = Input(data);
    let signature = header.take(4)?;
    ensure!(matches!(signature, b"OVC2" | b"OVCU"), "invalid coarse signature");
    let intermediate = header.count(LIMIT)?;
    let json = header.count(LIMIT)?;
    let bin = header.count(LIMIT)?;
    let padded = json.next_multiple_of(4);
    let total = 28 + padded + bin;
    ensure!(total <= LIMIT, "coarse GLB exceeds size limit");
    let mut inflated;
    let plain = if signature == b"OVCU" {
        ensure!(header.0.len() == intermediate, "invalid uncompressed coarse column length");
        header.0
    } else {
        inflated = vec![0; intermediate];
        let mut decoder = Decompress::new(true);
        let status = decoder.decompress(header.0, &mut inflated, FlushDecompress::Finish)?;
        ensure!(
            status == Status::StreamEnd
                && decoder.total_in() == header.0.len() as u64
                && decoder.total_out() == intermediate as u64,
            "invalid or incomplete coarse zlib stream"
        );
        &inflated
    };
    let mut input = Input(plain);
    let mut meta = Input(input.blob()?);
    let mut geometry = Input(input.blob()?);
    input.end()?;

    let mut glb = Vec::new();
    glb.try_reserve_exact(total)?;
    glb.extend_from_slice(b"glTF");
    glb.extend_from_slice(&2u32.to_le_bytes());
    glb.extend_from_slice(&(total as u32).to_le_bytes());
    glb.extend_from_slice(&(padded as u32).to_le_bytes());
    glb.extend_from_slice(b"JSON");
    match meta.byte()? {
        0 => append(&mut glb, meta.0, 20 + json)?,
        1 => columns::metadata(meta.0, &mut glb, 20 + json)?,
        mode => bail!("unknown coarse JSON encoding {mode}"),
    }
    ensure!(glb.len() == 20 + json, "coarse JSON length mismatch");
    glb.resize(20 + padded, b' ');
    glb.extend_from_slice(&(bin as u32).to_le_bytes());
    glb.extend_from_slice(b"BIN\0");
    match geometry.byte()? {
        0 => append(&mut glb, geometry.0, total)?,
        1 => {
            glb.resize(total, 0);
            binary(geometry.0, &mut glb[28 + padded..])?;
        }
        mode => bail!("unknown coarse geometry encoding {mode}"),
    }
    ensure!(glb.len() == total, "coarse GLB length mismatch");
    Ok(glb)
}

#[cfg(test)]
mod verification;
