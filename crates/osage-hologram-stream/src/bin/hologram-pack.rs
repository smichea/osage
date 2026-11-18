// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Packs a baked hologram into blocks to stream (D-134):
//!
//!     hologram-pack HOLOGRAM OUT [ROWSxCOLUMNS] [CRF]
//!
//! HOLOGRAM is a folder written by `hologram-bake` (its `hologram.json` and
//! its pictures, `view-rRR-cCC.png` and the others); OUT receives one IVF
//! file per block and series and `stream.json`. Blocks are ROWS by COLUMNS
//! views (3x4 unless given), colours encoded at CRF (30 unless given).
//! Each block is decoded back as a client would, and the packer reports
//! what it weighs against the pictures, how close the colours come back
//! (PSNR) and how far the depths moved, and how long a block takes to
//! decode. Needs ffmpeg with libsvtav1 and libaom.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Instant,
};

use anyhow::{Context as _, bail};
use osage_hologram_stream::{
    Block, Grid, Packed, Series, Tiling,
    decode::{self, Picture},
    encode::{self, Quality},
};

/// How a hologram was framed, as `hologram-bake` writes it.
#[derive(serde::Deserialize)]
struct Framing {
    centre: [f32; 3],
    radius: f32,
    aspect: f32,
    grid: [f32; 6],
    at: [f32; 7],
}

/// What packing a series came to.
#[derive(Default)]
struct Tally {
    pictures: u64,
    blocks: u64,
    views: u64,
    /// Sum of squared colour errors and of samples compared.
    squared: f64,
    samples: u64,
    /// The largest depth error (of 65535) and the texels that changed side
    /// (object or nothing).
    depth_error: u32,
    outline: u64,
    decoding: f64,
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let usage = "hologram-pack HOLOGRAM OUT [ROWSxCOLUMNS] [CRF]";
    let hologram = PathBuf::from(args.next().context(usage)?);
    let out = PathBuf::from(args.next().context(usage)?);
    let tiling = match args.next() {
        Some(text) => {
            let (rows, columns) = text.split_once('x').context("ROWSxCOLUMNS, e.g. 3x4")?;
            Tiling { rows: rows.parse()?, columns: columns.parse()? }
        }
        None => Tiling::default(),
    };
    let quality = match args.next() {
        Some(crf) => Quality { colour: crf.parse()?, ..Quality::default() },
        None => Quality::default(),
    };
    // A hologram baked before `hologram.json` was written: its grid from
    // HOLOGRAM_GRID ("AZ_FROM,AZ_TO,AZ_STEP,EL_FROM,EL_TO,EL_STEP"), its
    // framing left for whoever draws it.
    let framing: Framing = match std::fs::read_to_string(hologram.join("hologram.json")) {
        Ok(text) => serde_json::from_str(&text)?,
        Err(_) => {
            let text = std::env::var("HOLOGRAM_GRID").context("no hologram.json: give the grid in HOLOGRAM_GRID")?;
            let n: Vec<f32> = text.split(',').map(str::parse).collect::<Result<_, _>>()?;
            let grid: [f32; 6] = n.try_into().map_err(|_| anyhow::anyhow!("HOLOGRAM_GRID takes six numbers"))?;
            Framing { centre: [0.0; 3], radius: 1.0, aspect: 1.0, grid, at: [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0] }
        }
    };
    let grid = Grid::from_steps(framing.grid);
    let (rows, columns) = (grid.elevations.len() as u16, grid.azimuths.len() as u16);
    let mut series: Vec<Series> = Series::ALL.into_iter().filter(|s| hologram.join(s.picture(0, 0)).exists()).collect();
    // The masks first: a depth block is checked joined to its mask.
    series.sort_by_key(|s| *s != Series::Mask);
    if !series.contains(&Series::Depth) {
        bail!("{}: no depth pictures", hologram.display());
    }
    std::fs::create_dir_all(&out)?;
    let size = picture(&hologram.join(series[0].picture(0, 0)))?;
    let blocks: Vec<Block> = tiling.blocks(rows, columns).collect();
    println!(
        "{}: {rows} x {columns} views of {} x {} px, {:?}, in {} blocks of {}x{}",
        hologram.display(),
        size.width,
        size.height,
        series.iter().map(|s| s.name()).collect::<Vec<_>>(),
        blocks.len(),
        tiling.rows,
        tiling.columns
    );
    let tallies: Mutex<BTreeMap<Series, Tally>> = Mutex::new(BTreeMap::new());
    let bytes: Mutex<BTreeMap<String, u64>> = Mutex::new(BTreeMap::new());
    let next = Mutex::new(blocks.iter());
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(8);
    std::thread::scope(|scope| -> anyhow::Result<()> {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| -> anyhow::Result<()> {
                    loop {
                        let Some(&block) = next.lock().unwrap().next() else { return Ok(()) };
                        let views = tiling.views(block, rows, columns);
                        let file_of = |kind: Series| -> Vec<PathBuf> {
                            views.iter().map(|&(r, c)| hologram.join(kind.picture(r, c))).collect()
                        };
                        // Where the object is, from the depths: the ground of
                        // colour-like pictures is padded, and only the object
                        // is compared.
                        let masks: Vec<Vec<bool>> = file_of(Series::Depth)
                            .iter()
                            .map(|f| Ok(picture(f)?.rgba.as_chunks::<4>().0.iter().map(|t| t[2] > 127).collect()))
                            .collect::<anyhow::Result<_>>()?;
                        for &kind in &series {
                            let files = file_of(kind);
                            let mut originals: Vec<Picture> =
                                files.iter().map(|f| picture(f)).collect::<anyhow::Result<_>>()?;
                            if !matches!(kind, Series::Depth | Series::Mask) {
                                for (original, mask) in originals.iter_mut().zip(&masks) {
                                    encode::pad(original, mask);
                                }
                            }
                            let ivf = encode::block(kind, &originals, quality)?;
                            let file = block.file(kind);
                            std::fs::write(out.join(&file), &ivf)?;
                            // A depth comes back with its mask, packed first.
                            let back = if kind == Series::Depth {
                                decode::depths(&ivf, &std::fs::read(out.join(block.file(Series::Mask)))?)?
                            } else {
                                decode::block(&ivf, kind)?
                            };
                            if back.len() != originals.len() {
                                bail!("{file}: {} pictures back of {}", back.len(), originals.len());
                            }
                            let mut tallies = tallies.lock().unwrap();
                            let tally = tallies.entry(kind).or_default();
                            tally.pictures +=
                                files.iter().map(|f| std::fs::metadata(f).map_or(0, |m| m.len())).sum::<u64>();
                            tally.blocks += ivf.len() as u64;
                            tally.views += views.len() as u64;
                            compare(kind, &originals, &back, &masks, tally);
                            bytes.lock().unwrap().insert(file, ivf.len() as u64);
                        }
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().map_err(|_| anyhow::anyhow!("a packing thread panicked"))??;
        }
        Ok(())
    })?;
    // Decoding timed alone, one block after another, as a client would.
    let mut tallies = tallies.into_inner().unwrap();
    for (kind, tally) in tallies.iter_mut() {
        let started = Instant::now();
        for block in &blocks {
            decode::block(&std::fs::read(out.join(block.file(*kind)))?, *kind)?;
        }
        tally.decoding = started.elapsed().as_secs_f64();
    }
    let packed = Packed {
        centre: framing.centre,
        radius: framing.radius,
        aspect: framing.aspect,
        at: framing.at,
        grid: framing.grid,
        tiling,
        size: [size.width, size.height],
        series: series.clone(),
        bytes: bytes.into_inner().unwrap(),
    };
    std::fs::write(out.join(Packed::FILE), serde_json::to_string_pretty(&packed)?)?;
    println!("| series | pictures (PNG) | blocks (AV1) | smaller | per view | quality | decoding a block |");
    println!("|---|---|---|---|---|---|---|");
    let (mut all_pictures, mut all_blocks) = (0, 0);
    for (kind, tally) in tallies {
        let quality = match kind {
            Series::Depth => format!("depth within {} / 65535", tally.depth_error),
            Series::Mask => format!("{} texels changed side", tally.outline),
            _ => {
                let mse = tally.squared / tally.samples.max(1) as f64;
                format!("{:.1} dB", 10.0 * (255.0f64 * 255.0 / mse.max(1e-9)).log10())
            }
        };
        println!(
            "| {} | {:.0} KiB | {:.0} KiB | ×{:.1} | {:.1} KiB | {quality} | {:.0} ms |",
            kind.name(),
            tally.pictures as f64 / 1024.0,
            tally.blocks as f64 / 1024.0,
            tally.pictures as f64 / tally.blocks.max(1) as f64,
            tally.blocks as f64 / 1024.0 / tally.views.max(1) as f64,
            1000.0 * tally.decoding / blocks.len().max(1) as f64,
        );
        all_pictures += tally.pictures;
        all_blocks += tally.blocks;
    }
    println!(
        "all: {:.1} MiB of pictures, {:.1} MiB of blocks (×{:.1}), in {}",
        all_pictures as f64 / 1048576.0,
        all_blocks as f64 / 1048576.0,
        all_pictures as f64 / all_blocks.max(1) as f64,
        out.display()
    );
    Ok(())
}

/// A baked picture as 8-bit RGBA.
fn picture(path: &Path) -> anyhow::Result<Picture> {
    let file = std::io::BufReader::new(std::fs::File::open(path).with_context(|| format!("{}", path.display()))?);
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let mut data = vec![0; reader.output_buffer_size().context("a picture too large")?];
    let info = reader.next_frame(&mut data)?;
    data.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => data,
        png::ColorType::Rgb => data.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|&v| [v, v, v, 255]).collect(),
        png::ColorType::GrayscaleAlpha => {
            data.as_chunks::<2>().0.iter().flat_map(|p| [p[0], p[0], p[0], p[1]]).collect()
        }
        other => bail!("{}: pictures of {other:?} are not handled", path.display()),
    };
    Ok(Picture { width: info.width, height: info.height, rgba })
}

/// Adds how far `back` came from `originals` to `tally`: colours where the
/// object is (`masks`), depths everywhere.
fn compare(kind: Series, originals: &[Picture], back: &[Picture], masks: &[Vec<bool>], tally: &mut Tally) {
    for ((a, b), mask) in originals.iter().zip(back).zip(masks) {
        for ((x, y), &there) in a.rgba.as_chunks::<4>().0.iter().zip(b.rgba.as_chunks::<4>().0.iter()).zip(mask) {
            if kind == Series::Mask {
                if (x[2] > 127) != (y[0] > 127) {
                    tally.outline += 1;
                }
            } else if kind == Series::Depth {
                let (there, back_there) = (x[2] > 127, y[2] > 127);
                if there != back_there {
                    tally.outline += 1;
                } else if there {
                    let (d0, d1) = (u16::from_be_bytes([x[0], x[1]]), u16::from_be_bytes([y[0], y[1]]));
                    tally.depth_error = tally.depth_error.max(u32::from(d0.abs_diff(d1)));
                }
            } else if there {
                for i in 0..3 {
                    let e = f64::from(x[i]) - f64::from(y[i]);
                    tally.squared += e * e;
                }
                tally.samples += 3;
            }
        }
    }
}
