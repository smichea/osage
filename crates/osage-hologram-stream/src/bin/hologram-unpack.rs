// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Unpacks a packed hologram back into pictures, as a client decodes them
//! (D-134):
//!
//!     hologram-unpack PACKED OUT
//!
//! PACKED is a folder written by `hologram-pack` (its `stream.json` and its
//! blocks); OUT receives the views, their colours without light, normals
//! and depths as `hologram-bake` names them, and `hologram.json`: a
//! hologram drawn from it, where it was drawn from the baked pictures,
//! shows what streaming costs to the eye.

use std::path::PathBuf;

use anyhow::Context as _;
use osage_hologram_stream::{Packed, Series, decode};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let usage = "hologram-unpack PACKED OUT";
    let packed_dir = PathBuf::from(args.next().context(usage)?);
    let out = PathBuf::from(args.next().context(usage)?);
    let packed: Packed = serde_json::from_str(&std::fs::read_to_string(packed_dir.join(Packed::FILE))?)?;
    let grid = packed.grid();
    let (rows, columns) = (grid.elevations.len() as u16, grid.azimuths.len() as u16);
    std::fs::create_dir_all(&out)?;
    let mut written = 0;
    for block in packed.tiling.blocks(rows, columns) {
        let views = packed.tiling.views(block, rows, columns);
        for &kind in packed.series.iter().filter(|s| **s != Series::Mask) {
            let ivf = std::fs::read(packed_dir.join(block.file(kind)))?;
            let pictures = if kind == Series::Depth {
                decode::depths(&ivf, &std::fs::read(packed_dir.join(block.file(Series::Mask)))?)?
            } else {
                decode::block(&ivf, kind)?
            };
            for (&(row, column), picture) in views.iter().zip(&pictures) {
                let file = std::io::BufWriter::new(std::fs::File::create(out.join(kind.picture(row, column)))?);
                let mut encoder = png::Encoder::new(file, picture.width, picture.height);
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                encoder.write_header()?.write_image_data(&picture.rgba)?;
                written += 1;
            }
        }
    }
    let framing = serde_json::json!({
        "centre": packed.centre, "radius": packed.radius, "aspect": packed.aspect,
        "grid": packed.grid, "at": packed.at,
    });
    std::fs::write(out.join("hologram.json"), serde_json::to_string(&framing)?)?;
    println!("{written} pictures unpacked into {}", out.display());
    Ok(())
}
