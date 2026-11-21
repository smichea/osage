// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Packs tiles of light into AV1 (D-136, D-137):
//!
//!     light-pack [--again] [--half] [--tiles[=CRF]] RAW OUT [CRF [DEPTH_CRF]]
//!
//! RAW holds a folder a tile or probe, as `tools/bistro/tiles.py` and
//! `tools/bistro/cube-probes.py` write them (`radiance.f32`, `distance.f32`,
//! `meta.json`); OUT receives a folder each with `colour.ivf` (radiance,
//! 10-bit through its curve), `depth.ivf` (distance, 12-bit through its
//! curve), `mask.ivf` (the sky) and `meta.json`, and `index.json`, all that
//! is packed there, from this RAW or another. One already packed is left
//! as it is.
//! Each tile is decoded back as a client would, and the packer reports what
//! the tiles weigh and how close they come back.
//!
//! `--again` packs again what is packed already (RAW's folders packed, not
//! raw: decoded and encoded at CRF, D-140), in place if OUT is RAW. `--half`
//! also packs each probe inside a cube (`c_X_Y_Z_K`) at half its size, as
//! `h_X_Y_Z_K`, for a running walker (D-140). `--tiles` packs each probe
//! inside a cube as [`COLUMNS`] × [`ROWS`] tiles instead, each a key frame
//! that decodes alone (`tiles-colour.ivf`, `tiles-depth.ivf`,
//! `tiles-mask.ivf`; `"tiles": [COLUMNS, ROWS]` in its `meta.json`): a
//! client decodes those it looks at over the half (D-141), which it packs
//! too (as `--half`). Their radiance is at its own CRF, [`TILE_CRF`] unless
//! told: at a whole picture's CRF, key frames each alone came back a
//! quarter further off.

use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use anyhow::Context as _;
use osage_hologram_stream::light::{self, Drawn, Light, Quality};

/// A probe's tiles across and down (`--tiles`): 256 pixels square for a
/// probe of 4096 × 2048.
const COLUMNS: u32 = 16;
const ROWS: u32 = 8;
/// The tiles' radiance CRF: as close as a whole probe at 22 (D-140), for
/// 8 % more bytes.
const TILE_CRF: u8 = 16;

/// The files of a packed light, whole or as tiles.
const WHOLE: [&str; 3] = ["colour.ivf", "depth.ivf", "mask.ivf"];
const TILED: [&str; 3] = ["tiles-colour.ivf", "tiles-depth.ivf", "tiles-mask.ivf"];

/// Whether a folder holds a packed light, whole or as tiles.
fn packed(folder: &Path) -> bool {
    folder.join(WHOLE[2]).exists() || folder.join(TILED[2]).exists()
}

/// A packed light decoded back, whole or from its tiles.
fn decoded(folder: &Path, meta: &serde_json::Value) -> anyhow::Result<Drawn> {
    let read =
        |name: &str| std::fs::read(folder.join(name)).with_context(|| format!("{}", folder.join(name).display()));
    if folder.join(TILED[2]).exists() {
        let columns = meta["tiles"][0].as_u64().context("a tiled light's columns")? as u32;
        let tiles = light::decode(&read(TILED[0])?, &read(TILED[1])?, &read(TILED[2])?)?;
        return Drawn::joined(&tiles, columns);
    }
    light::decode(&read(WHOLE[0])?, &read(WHOLE[1])?, &read(WHOLE[2])?)?.pop().context("no picture")
}

/// What a folder of RAW holds: raw floats (`tiles.py`, `cube-probes.py`),
/// or, packed already, its light decoded back (`--again`).
fn load(folder: &Path, meta: &serde_json::Value) -> anyhow::Result<Light> {
    if folder.join("distance.f32").exists() {
        let (width, height) = (meta["width"].as_u64().unwrap_or(0) as u32, meta["height"].as_u64().unwrap_or(0) as u32);
        let radiance = floats(&folder.join("radiance.f32"))?;
        return Ok(Light {
            width,
            height,
            radiance: radiance.as_chunks::<3>().0.to_vec(),
            distance: floats(&folder.join("distance.f32"))?,
        });
    }
    Ok(Light::from_drawn(&decoded(folder, meta)?))
}

/// A packed light's files and its `meta.json` in `target`, whole or as
/// tiles (`meta` says), the other form's files gone.
fn write(target: &Path, encoded: &light::Encoded, meta: &serde_json::Value) -> anyhow::Result<()> {
    std::fs::create_dir_all(target)?;
    let (names, others) = if meta.get("tiles").is_some() { (TILED, WHOLE) } else { (WHOLE, TILED) };
    for (name, bytes) in names.iter().zip([&encoded.radiance, &encoded.distance, &encoded.sky]) {
        std::fs::write(target.join(name), bytes)?;
    }
    for name in others {
        if target.join(name).exists() {
            std::fs::remove_file(target.join(name))?;
        }
    }
    std::fs::write(target.join("meta.json"), serde_json::to_string_pretty(meta)? + "\n")?;
    Ok(())
}

fn floats(path: &Path) -> anyhow::Result<Vec<f32>> {
    let bytes = std::fs::read(path).with_context(|| format!("{}", path.display()))?;
    Ok(bytes.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect())
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1).peekable();
    let usage = "light-pack [--again] [--half] [--tiles[=CRF]] RAW OUT [CRF [DEPTH_CRF]]";
    let (mut again, mut half, mut tiled) = (false, false, None);
    while let Some(flag) = args.next_if(|a| a.starts_with("--")) {
        match flag.as_str() {
            "--again" => again = true,
            "--half" => half = true,
            "--tiles" => (tiled, half) = (Some(TILE_CRF), true),
            tiles if tiles.starts_with("--tiles=") => (tiled, half) = (Some(tiles["--tiles=".len()..].parse()?), true),
            other => anyhow::bail!("{other}: {usage}"),
        }
    }
    let raw = PathBuf::from(args.next().context(usage)?);
    let out = PathBuf::from(args.next().context(usage)?);
    let mut quality = Quality::default();
    if let Some(crf) = args.next() {
        quality.radiance = crf.parse()?;
    }
    if let Some(crf) = args.next() {
        quality.distance = crf.parse()?;
    }
    std::fs::create_dir_all(&out)?;
    let mut keys: Vec<String> = std::fs::read_dir(&raw)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join("distance.f32").exists() || (again && packed(&e.path())))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|key| !key.starts_with("h_"))
        .collect();
    keys.sort();
    let next = Mutex::new(keys.iter());
    let totals = Mutex::new((0u64, 0u64, 0u64, 0u64, 0usize, 0.0f32, 0.0f32));
    std::thread::scope(|scope| -> anyhow::Result<()> {
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(6);
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| -> anyhow::Result<()> {
                    loop {
                        let Some(key) = next.lock().unwrap().next() else { return Ok(()) };
                        let target = out.join(key);
                        if !again && packed(&target) {
                            continue;
                        }
                        let folder = raw.join(key);
                        let mut meta: serde_json::Value =
                            serde_json::from_str(&std::fs::read_to_string(folder.join("meta.json"))?)?;
                        let light = load(&folder, &meta)?;
                        let (radiance, distance) = (light.radiance.len() * 3, light.distance.len());
                        let probe = key.starts_with("c_");
                        let encoded = if let Some(crf) = tiled.filter(|_| probe) {
                            meta["tiles"] = serde_json::json!([COLUMNS, ROWS]);
                            let tiles = Quality { radiance: crf, intra: true, ..quality };
                            light::encode(&light.tiles(COLUMNS, ROWS), tiles)?
                        } else {
                            if let Some(told) = meta.as_object_mut() {
                                told.remove("tiles");
                            }
                            light::encode(std::slice::from_ref(&light), quality)?
                        };
                        write(&target, &encoded, &meta)?;
                        // A probe inside a cube, at half its size too.
                        if half && probe {
                            let small = light.half();
                            let mut told = meta.clone();
                            if let Some(told) = told.as_object_mut() {
                                told.remove("tiles");
                            }
                            told["width"] = small.width.into();
                            told["height"] = small.height.into();
                            told["key"] = key.replacen("c_", "h_", 1).into();
                            let encoded = light::encode(std::slice::from_ref(&small), quality)?;
                            write(&out.join(key.replacen("c_", "h_", 1)), &encoded, &told)?;
                        }
                        // How close it comes back: the radiance's error,
                        // relative, where it is not dark.
                        let back = [decoded(&target, &meta)?];
                        let mut worst: f32 = 0.0;
                        let mut sum = 0.0f32;
                        let mut n = 0;
                        for (packed, truth) in back[0].radiance.iter().zip(&light.radiance) {
                            for (k, shift) in [0u32, 10, 20].into_iter().enumerate() {
                                let v = light::decode_radiance(((packed >> shift) & 1023) as f32 / 1023.0);
                                let t = truth[k].min(light::BRIGHTEST);
                                if t > 0.02 {
                                    let e = (v - t).abs() / t;
                                    worst = worst.max(e);
                                    sum += e;
                                    n += 1;
                                }
                            }
                        }
                        // And the distance's, relative, where it is not the sky.
                        let (mut off, mut solid) = (0.0f32, 0usize);
                        for (back, truth) in back[0].distance.iter().zip(&light.distance) {
                            if truth.is_finite() && *back >= 0.0 {
                                off += (back - truth).abs() / (1.0 + truth);
                                solid += 1;
                            }
                        }
                        let mut totals = totals.lock().unwrap();
                        totals.0 += (radiance * 4 + distance * 4) as u64;
                        totals.1 += encoded.radiance.len() as u64;
                        totals.2 += encoded.distance.len() as u64;
                        totals.3 += encoded.sky.len() as u64;
                        totals.4 += 1;
                        totals.5 += sum / n.max(1) as f32;
                        totals.6 += off / solid.max(1) as f32;
                        let _ = worst;
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().map_err(|_| anyhow::anyhow!("a packing thread panicked"))??;
        }
        Ok(())
    })?;
    // Everything packed in OUT, from this RAW or another (the probes inside
    // the cubes beside the blocks' tiles).
    let mut packed: Vec<String> = std::fs::read_dir(&out)?
        .filter_map(|e| e.ok())
        .filter(|e| packed(&e.path()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    packed.sort();
    std::fs::write(out.join("index.json"), serde_json::to_string(&packed)?)?;
    let (raw_bytes, colour, depth, mask, n, error, distance_error) = *totals.lock().unwrap();
    if n > 0 {
        println!(
            "light-pack: {n} tiles packed: colour {:.0} KiB, distance {:.0} KiB, sky {:.0} KiB a tile ({:.0} KiB of floats); \
             radiance within {:.1} % on average, distance within {:.2} % of 1 m + itself; {} tiles in {}",
            colour as f64 / n as f64 / 1024.0,
            depth as f64 / n as f64 / 1024.0,
            mask as f64 / n as f64 / 1024.0,
            raw_bytes as f64 / n as f64 / 1024.0,
            100.0 * error / n as f32,
            100.0 * distance_error / n as f32,
            packed.len(),
            out.display()
        );
    } else {
        println!("light-pack: nothing new; {} tiles in {}", packed.len(), out.display());
    }
    Ok(())
}
