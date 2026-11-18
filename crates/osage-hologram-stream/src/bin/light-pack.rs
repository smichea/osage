// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Packs tiles of light into AV1 (D-136):
//!
//!     light-pack RAW OUT [CRF]
//!
//! RAW holds a folder a tile, as `tools/bistro/tiles.py` writes them
//! (`radiance.f32`, `distance.f32`, `meta.json`); OUT receives a folder a
//! tile with `colour.ivf` (radiance, 10-bit through its curve), `depth.ivf`
//! (distance, 12-bit through its curve) and `mask.ivf` (the sky), and
//! `index.json`, the tiles there. A tile already packed is left as it is.
//! Each tile is decoded back as a client would, and the packer reports what
//! the tiles weigh and how close they come back.

use std::{path::PathBuf, sync::Mutex};

use anyhow::Context as _;
use osage_hologram_stream::light::{self, Light, Quality};

fn floats(path: &std::path::Path) -> anyhow::Result<Vec<f32>> {
    let bytes = std::fs::read(path).with_context(|| format!("{}", path.display()))?;
    Ok(bytes.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect())
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let usage = "light-pack RAW OUT [CRF]";
    let raw = PathBuf::from(args.next().context(usage)?);
    let out = PathBuf::from(args.next().context(usage)?);
    let quality = match args.next() {
        Some(crf) => Quality { radiance: crf.parse()?, ..Quality::default() },
        None => Quality::default(),
    };
    std::fs::create_dir_all(&out)?;
    let mut keys: Vec<String> = std::fs::read_dir(&raw)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join("distance.f32").exists())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    keys.sort();
    let next = Mutex::new(keys.iter());
    let totals = Mutex::new((0u64, 0u64, 0u64, 0u64, 0usize, 0.0f32));
    std::thread::scope(|scope| -> anyhow::Result<()> {
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(6);
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| -> anyhow::Result<()> {
                    loop {
                        let Some(key) = next.lock().unwrap().next() else { return Ok(()) };
                        let target = out.join(key);
                        if target.join("mask.ivf").exists() {
                            continue;
                        }
                        let folder = raw.join(key);
                        let meta: serde_json::Value =
                            serde_json::from_str(&std::fs::read_to_string(folder.join("meta.json"))?)?;
                        let (width, height) =
                            (meta["width"].as_u64().unwrap_or(0) as u32, meta["height"].as_u64().unwrap_or(0) as u32);
                        let radiance = floats(&folder.join("radiance.f32"))?;
                        let distance = floats(&folder.join("distance.f32"))?;
                        let light = Light {
                            width,
                            height,
                            radiance: radiance.as_chunks::<3>().0.to_vec(),
                            distance: distance.clone(),
                        };
                        let encoded = light::encode(std::slice::from_ref(&light), quality)?;
                        std::fs::create_dir_all(&target)?;
                        std::fs::write(target.join("colour.ivf"), &encoded.radiance)?;
                        std::fs::write(target.join("depth.ivf"), &encoded.distance)?;
                        std::fs::write(target.join("mask.ivf"), &encoded.sky)?;
                        std::fs::copy(folder.join("meta.json"), target.join("meta.json"))?;
                        // How close it comes back: the radiance's error,
                        // relative, where it is not dark.
                        let back = light::decode(&encoded.radiance, &encoded.distance, &encoded.sky)?;
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
                        let mut totals = totals.lock().unwrap();
                        totals.0 += (radiance.len() * 4 + distance.len() * 4) as u64;
                        totals.1 += encoded.radiance.len() as u64;
                        totals.2 += encoded.distance.len() as u64;
                        totals.3 += encoded.sky.len() as u64;
                        totals.4 += 1;
                        totals.5 += sum / n.max(1) as f32;
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
    let packed: Vec<String> = keys.iter().filter(|k| out.join(k).join("mask.ivf").exists()).cloned().collect();
    std::fs::write(out.join("index.json"), serde_json::to_string(&packed)?)?;
    let (raw_bytes, colour, depth, mask, n, error) = *totals.lock().unwrap();
    if n > 0 {
        println!(
            "light-pack: {n} tiles packed: colour {:.0} KiB, distance {:.0} KiB, sky {:.0} KiB a tile ({:.0} KiB of floats); \
             radiance within {:.1} % on average; {} tiles in {}",
            colour as f64 / n as f64 / 1024.0,
            depth as f64 / n as f64 / 1024.0,
            mask as f64 / n as f64 / 1024.0,
            raw_bytes as f64 / n as f64 / 1024.0,
            100.0 * error / n as f32,
            packed.len(),
            out.display()
        );
    } else {
        println!("light-pack: nothing new; {} tiles in {}", packed.len(), out.display());
    }
    Ok(())
}
