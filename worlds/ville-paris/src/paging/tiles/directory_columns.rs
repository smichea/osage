// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::{File, PhysicsDirectory};
use anyhow::{Context, Result, ensure};

/// The header retains the ordinary source tree, rows and layer properties. Every
/// physical row contributes one coordinate to each plane. Digests follow in
/// whole-support, regional-support, then physical-row order.
pub(super) fn read(raw: &[u8], regional: bool) -> Result<PhysicsDirectory> {
    let (size, body) = raw.split_first_chunk::<4>().context("missing column directory header")?;
    let (json, columns) = body.split_at_checked(u32::from_le_bytes(*size) as usize).context("short column header")?;
    let mut directory: PhysicsDirectory = serde_json::from_slice(json)?;
    let count = directory.tiles.iter().filter(|(_, _, l)| l.is_some()).count();
    let plane_bytes = count.checked_mul(24).context("district bounds length overflow")?;
    let (planes, hashes) = columns.split_at_checked(plane_bytes).context("short district bounds columns")?;
    let descriptors = 1 + directory.support_regions.len() + if regional { 0 } else { count };
    let (digests, trailing) = hashes.as_chunks::<32>();
    ensure!(trailing.is_empty() && digests.len() == descriptors, "invalid district digest column length");
    let mut hashes = digests.iter();
    let mut restore = |file: &mut File, extensions: &[&str]| -> Result<()> {
        ensure!(file.sha256.is_empty() && extensions.contains(&file.file.as_str()), "invalid column file descriptor");
        file.sha256 = crate::bench::hex(hashes.next().context("missing district digest")?);
        file.file = format!("{}.{}", file.sha256, file.file);
        Ok(())
    };
    restore(&mut directory.support.content, &["zlib", "zst"])?;
    for region in &mut directory.support_regions {
        restore(&mut region.file.content, &["zlib", "zst"])?;
    }
    for (row, layer) in directory.tiles.iter_mut().filter_map(|(_, _, l)| l.as_mut()).enumerate() {
        if regional {
            ensure!(
                layer.content.sha256.is_empty() && matches!(layer.content.file.as_str(), "cpage" | "json"),
                "invalid regional file descriptor"
            );
            layer.content.file.insert(0, '.');
        } else {
            restore(&mut layer.content, &["cpage", "json"])?;
        }
        for (axis, value) in layer.bounds.iter_mut().flatten().enumerate() {
            ensure!(value.to_bits() == 0, "conflicting district bounds");
            let start = (axis * count + row) * 4;
            *value = f32::from_le_bytes(planes[start..start + 4].try_into()?);
        }
    }
    Ok(directory)
}

/// Keep visual addresses in tile order so the first frame need only open the pinned root.
pub(super) fn read_visual(raw: &[u8]) -> Result<super::Index> {
    let (size, body) = raw.split_first_chunk::<4>().context("missing visual directory header")?;
    let (json, column) =
        body.split_at_checked(u32::from_le_bytes(*size) as usize).context("short visual directory header")?;
    let mut index: super::Index = serde_json::from_slice(json)?;
    let (digests, trailing) = column.as_chunks::<32>();
    ensure!(trailing.is_empty() && digests.len() == index.tiles.len(), "visual address count mismatch");
    let extension = if index.shared() { "vpage" } else { "glb" };
    for (tile, hash) in index.tiles.iter_mut().zip(digests) {
        let content = &mut tile.visual.content;
        ensure!(content.sha256.is_empty() && content.file == extension, "invalid visual address column");
        content.sha256 = crate::bench::hex(hash);
        content.file = format!("{}.{}", content.sha256, extension);
    }
    Ok(index)
}

pub(super) fn read_first_frame(raw: &[u8], visuals: bool) -> Result<super::Index> {
    let (header, payload) = raw.split_first_chunk::<12>().context("short first-frame header")?;
    let size = u32::from_le_bytes(header[4..8].try_into()?) as usize;
    let region = u32::from_le_bytes(header[8..12].try_into()?);
    let (metadata, files) = payload.split_at_checked(size).context("truncated first-frame directory")?;
    let mut index: super::Index = match &header[..4] {
        b"ODI2" => read_visual(metadata)?,
        b"ODI1" => serde_json::from_slice(metadata)?,
        _ => anyhow::bail!("invalid first-frame directory format"),
    };
    ensure!(index.physical_addresses.is_some() == (&header[..4] == b"ODI2"), "misplaced physical address table");
    let (parent, tail) = files.split_at_checked(index.parent.bytes).context("truncated first-frame parent")?;
    index.parent.check(parent)?;
    if visuals {
        index.inline_parent = Some(parent.to_vec());
    }
    let tail = if let Some(library) = &index.models {
        let (bytes, rest) =
            tail.split_at_checked(library.content.bytes).context("truncated first-frame model library")?;
        library.content.check(bytes)?;
        if visuals {
            index.inline_models = Some(bytes.to_vec());
        }
        rest
    } else {
        tail
    };
    if region == u32::MAX {
        if index.support_regions.is_empty()
            && let Some(global) = &index.support
        {
            global.content.check(tail)?;
            index.inline_global_support = Some(tail.to_vec());
        } else {
            ensure!(tail.is_empty(), "trailing first-frame data");
        }
    } else {
        let id = region as usize;
        let row = index.support_regions.get(id).context("unknown first-frame region")?;
        row.file.content.check(tail)?;
        index.inline_region = Some((id, tail.to_vec()));
    }
    Ok(index)
}
