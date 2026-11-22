// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tiles of light (D-136): what a probe on a block's surface sees coming
//! in — its radiance in high dynamic range, and how far each pixel lies —
//! as AV1 to send, and back.
//!
//! - **Radiance** is light as a renderer computes it, its three bands (red,
//!   green, blue) unbounded: a sky tens of times brighter than a facade in
//!   shade, a lamp brighter still. It travels as 10-bit colour through a
//!   logarithmic curve ([`encode_radiance`]): about a hundred levels for
//!   every doubling of brightness, from a thousandth of white to
//!   [`BRIGHTEST`].
//! - **Distance**, m, travels as 12-bit grey through a logarithmic curve
//!   ([`encode_distance`]): millimetres near, decimetres a hundred metres
//!   off; nearly exact (libaom at constant quality 1: within 7 levels of
//!   4096, a decimetre at 10 m), for a client that reprojects what a probe
//!   saw puts it where the distance says; at 12, the codec smeared it at
//!   edges by up to 74 levels, a metre and a half at 10 m, for 3 KiB less a
//!   tile. Its sky is padded, and where the sky is travels apart, exact (a
//!   mask).

use std::{
    io::{Read as _, Write as _},
    process::{Command, Stdio},
};

use anyhow::{Context as _, ensure};

use crate::decode::Planes;

/// The brightest radiance kept; brighter is clipped.
pub const BRIGHTEST: f32 = 64.0;
/// How much the curve stretches the dark: a level at L = 1 / CURVE_K is
/// about where it turns from linear to logarithmic.
const CURVE_K: f32 = 64.0;
/// The farthest distance kept, m; the sky is apart.
pub const FARTHEST: f32 = 2000.0;

/// What a probe saw: radiance and distance per pixel, rows from the top.
#[derive(Clone, Debug, PartialEq)]
pub struct Light {
    pub width: u32,
    pub height: u32,
    pub radiance: Vec<[f32; 3]>,
    /// Metres; infinite where the sky is.
    pub distance: Vec<f32>,
}

impl Light {
    /// Back from what a client draws: the radiance off its 10-bit curve,
    /// the sky's distance infinite (to pack a light again, D-140).
    pub fn from_drawn(drawn: &Drawn) -> Self {
        let band = |packed: u32, shift: u32| decode_radiance(((packed >> shift) & 1023) as f32 / 1023.0);
        Self {
            width: drawn.width,
            height: drawn.height,
            radiance: drawn.radiance.iter().map(|&p| [band(p, 0), band(p, 10), band(p, 20)]).collect(),
            distance: drawn.distance.iter().map(|&d| if d < 0.0 { f32::INFINITY } else { d }).collect(),
        }
    }

    /// The light cut into `columns` × `rows` tiles of equal size, row by
    /// row from the top left (D-141: a probe's tiles, each decoded alone).
    pub fn tiles(&self, columns: u32, rows: u32) -> Vec<Self> {
        let (tw, th) = (self.width / columns, self.height / rows);
        let mut out = Vec::with_capacity((columns * rows) as usize);
        for row in 0..rows {
            for column in 0..columns {
                let (mut radiance, mut distance) = (Vec::new(), Vec::new());
                for y in row * th..(row + 1) * th {
                    let from = (y * self.width + column * tw) as usize;
                    radiance.extend_from_slice(&self.radiance[from..from + tw as usize]);
                    distance.extend_from_slice(&self.distance[from..from + tw as usize]);
                }
                out.push(Self { width: tw, height: th, radiance, distance });
            }
        }
        out
    }

    /// Half as wide and high (D-140: sent to a running walker): each
    /// pixel the mean radiance of the four under it, and the distance of the
    /// nearest of them (what the march must not see through); the sky only
    /// where all four are.
    pub fn half(&self) -> Self {
        let (width, height) = (self.width / 2, self.height / 2);
        let w = self.width as usize;
        let mut radiance = Vec::with_capacity((width * height) as usize);
        let mut distance = Vec::with_capacity((width * height) as usize);
        for y in 0..height as usize {
            for x in 0..width as usize {
                let at = [
                    (2 * y) * w + 2 * x,
                    (2 * y) * w + 2 * x + 1,
                    (2 * y + 1) * w + 2 * x,
                    (2 * y + 1) * w + 2 * x + 1,
                ];
                let mut sum = [0.0f32; 3];
                for &i in &at {
                    for (s, v) in sum.iter_mut().zip(self.radiance[i]) {
                        *s += v / 4.0;
                    }
                }
                radiance.push(sum);
                distance.push(at.iter().map(|&i| self.distance[i]).fold(f32::INFINITY, f32::min));
            }
        }
        Self { width, height, radiance, distance }
    }
}

/// A radiance band to the curve's 0..1.
pub fn encode_radiance(l: f32) -> f32 {
    ((1.0 + CURVE_K * l.clamp(0.0, BRIGHTEST)).ln() / (1.0 + CURVE_K * BRIGHTEST).ln()).clamp(0.0, 1.0)
}

/// The curve's 0..1 back to radiance.
pub fn decode_radiance(c: f32) -> f32 {
    ((c.clamp(0.0, 1.0) * (1.0 + CURVE_K * BRIGHTEST).ln()).exp() - 1.0) / CURVE_K
}

/// A distance to the curve's 0..1.
pub fn encode_distance(d: f32) -> f32 {
    ((1.0 + d.clamp(0.0, FARTHEST)).ln() / (1.0 + FARTHEST).ln()).clamp(0.0, 1.0)
}

/// The curve's 0..1 back to a distance.
pub fn decode_distance(g: f32) -> f32 {
    (g.clamp(0.0, 1.0) * (1.0 + FARTHEST).ln()).exp() - 1.0
}

/// How well to encode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quality {
    /// SVT-AV1's constant quality for radiance (0 best, 63 worst).
    pub radiance: u8,
    /// libaom's for distance; 0 keeps it exact, 1 within a few levels.
    pub distance: u8,
    pub preset: u8,
    /// Every frame a key frame, its sequence header with it: each decodes
    /// alone (a probe's tiles, D-141).
    pub intra: bool,
}

impl Default for Quality {
    /// Radiance at 22 (D-140): against Cycles, 0.2 dB under 10 for a third
    /// of its bytes.
    fn default() -> Self {
        Self { radiance: 22, distance: 1, preset: 6, intra: false }
    }
}

/// The three AV1 files of `lights` (one frame each, in order): radiance,
/// distance, and the sky's mask.
pub struct Encoded {
    pub radiance: Vec<u8>,
    pub distance: Vec<u8>,
    pub sky: Vec<u8>,
}

pub fn encode(lights: &[Light], quality: Quality) -> anyhow::Result<Encoded> {
    let first = lights.first().context("no light to encode")?;
    let (width, height) = (first.width, first.height);
    ensure!(lights.iter().all(|l| (l.width, l.height) == (width, height)), "lights of different sizes");
    let (mut radiance, mut distance, mut sky) = (Vec::new(), Vec::new(), Vec::new());
    for light in lights {
        for rgb in &light.radiance {
            for band in rgb {
                let level = (encode_radiance(*band) * 65535.0).round() as u16;
                radiance.extend_from_slice(&level.to_le_bytes());
            }
        }
        // The sky's distance padded with what is near it, so that the codec
        // spends nothing on the edge.
        let open: Vec<bool> = light.distance.iter().map(|d| !d.is_finite()).collect();
        let mut levels: Vec<[f32; 3]> = light
            .distance
            .iter()
            .map(|&d| [if d.is_finite() { encode_distance(d) * 4095.0 } else { 0.0 }, 0.0, 0.0])
            .collect();
        let solid: Vec<bool> = open.iter().map(|o| !o).collect();
        crate::encode::fill(width as usize, height as usize, &solid, &mut levels);
        for v in levels {
            distance.extend_from_slice(&(v[0].round().clamp(0.0, 4095.0) as u16).to_le_bytes());
        }
        sky.extend(open.iter().map(|&o| if o { 255u8 } else { 0 }));
    }
    let size = format!("{width}x{height}");
    // Key frames only (D-141): `-g 1` for libaom, which must not look
    // ahead (it failed to allocate its noise filter's buffers, and the
    // frames ahead are other tiles); SVT-AV1 is told apart, and filters
    // nothing across frames when every one is a key frame.
    let gop = if quality.intra { "1" } else { "9999" };
    let svt: &[&str] = if quality.intra { &["-svtav1-params", "keyint=1"] } else { &[] };
    let aom: &[&str] = if quality.intra { &["-lag-in-frames", "0"] } else { &[] };
    let colour = ffmpeg(
        &["-f", "rawvideo", "-pix_fmt", "rgb48le", "-s", &size, "-r", "30", "-i", "pipe:0"],
        &[
            "-vf",
            "scale=out_color_matrix=bt709:out_range=tv,format=yuv420p10le",
            "-colorspace",
            "bt709",
            "-color_range",
            "tv",
            "-c:v",
            "libsvtav1",
            "-preset",
            &quality.preset.to_string(),
            "-crf",
            &quality.radiance.to_string(),
        ]
        .iter()
        .chain(svt)
        .copied()
        .collect::<Vec<_>>(),
        gop,
        radiance,
    )?;
    let depth = ffmpeg(
        &["-f", "rawvideo", "-pix_fmt", "gray12le", "-s", &size, "-r", "30", "-i", "pipe:0"],
        &["-c:v", "libaom-av1", "-crf", &quality.distance.to_string(), "-b:v", "0", "-cpu-used", "6", "-row-mt", "1"]
            .iter()
            .chain(aom)
            .copied()
            .collect::<Vec<_>>(),
        gop,
        distance,
    )?;
    let mask = ffmpeg(
        &["-f", "rawvideo", "-pix_fmt", "gray", "-s", &size, "-r", "30", "-i", "pipe:0"],
        &["-c:v", "libaom-av1", "-crf", "0", "-b:v", "0", "-cpu-used", "6", "-row-mt", "1"]
            .iter()
            .chain(aom)
            .copied()
            .collect::<Vec<_>>(),
        gop,
        sky,
    )?;
    Ok(Encoded { radiance: colour, distance: depth, sky: mask })
}

/// `gop`: the most frames from a key frame to the next.
fn ffmpeg(input: &[&str], codec: &[&str], gop: &str, raw: Vec<u8>) -> anyhow::Result<Vec<u8>> {
    let mut child = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error"])
        .args(input)
        .args(codec)
        .args(["-g", gop, "-f", "ivf", "pipe:1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("ffmpeg (with libsvtav1 and libaom) is needed to pack light")?;
    let mut stdin = child.stdin.take().context("ffmpeg's input")?;
    let writer = std::thread::spawn(move || stdin.write_all(&raw));
    let mut out = Vec::new();
    child.stdout.take().context("ffmpeg's output")?.read_to_end(&mut out)?;
    let mut errors = String::new();
    child.stderr.take().context("ffmpeg's errors")?.read_to_string(&mut errors)?;
    let status = child.wait()?;
    let written = writer.join().map_err(|_| anyhow::anyhow!("writing to ffmpeg panicked"))?;
    // What ffmpeg says first: its input breaks off when it stops.
    ensure!(status.success(), "ffmpeg: {errors}");
    written?;
    Ok(out)
}

/// A tile of light as a client draws it: radiance on the curve, 10 bits a
/// band packed as RGB10A2 (red lowest), and distance in metres (negative
/// where the sky is); rows from the top.
#[derive(Clone, Debug, PartialEq)]
pub struct Drawn {
    pub width: u32,
    pub height: u32,
    pub radiance: Vec<u32>,
    pub distance: Vec<f32>,
}

/// Back from the three files: each frame's light, as drawn.
pub fn decode(radiance: &[u8], distance: &[u8], sky: &[u8]) -> anyhow::Result<Vec<Drawn>> {
    // The colour beside the distance and the sky.
    let (colours, rest) = std::thread::scope(|scope| {
        let others = scope
            .spawn(|| -> anyhow::Result<_> { Ok((crate::decode::planes(distance)?, crate::decode::planes(sky)?)) });
        let colours = crate::decode::planes(radiance);
        (colours, others.join())
    });
    let colours = colours?;
    let (depths, masks) = rest.map_err(|_| anyhow::anyhow!("decoding the distance panicked"))??;
    drawn(&colours, &depths, &masks)
}

/// Some tiles of a probe packed as key frames (D-141), each decoded alone,
/// in the order asked.
pub fn decode_tiles(radiance: &[u8], distance: &[u8], sky: &[u8], which: &[usize]) -> anyhow::Result<Vec<Drawn>> {
    let colours = crate::decode::planes_of(radiance, which)?;
    let depths = crate::decode::planes_of(distance, which)?;
    let masks = crate::decode::planes_of(sky, which)?;
    drawn(&colours, &depths, &masks)
}

/// The decoded pictures as a client draws them.
fn drawn(colours: &[Planes], depths: &[Planes], masks: &[Planes]) -> anyhow::Result<Vec<Drawn>> {
    ensure!(colours.len() == depths.len() && depths.len() == masks.len(), "files of different lengths");
    let mut out = Vec::new();
    for ((c, d), m) in colours.iter().zip(depths).zip(masks) {
        let (width, height) = (c.width, c.height);
        // The radiance packed in its 10 bits a band as it is converted.
        let q = |v: f32| (v.clamp(0.0, 1.0) * 1023.0 + 0.5) as u32;
        let (radiance, (grey, open)) = std::thread::scope(|scope| {
            let greys = scope.spawn(|| (d.grey(), m.grey()));
            (c.rgb_as(0u32, |r, g, b| q(r) | (q(g) << 10) | (q(b) << 20) | (3 << 30)), greys.join().expect("greys"))
        });
        let distance = grey.iter().zip(&open).map(|(&g, &o)| if o > 0.5 { -1.0 } else { decode_distance(g) }).collect();
        out.push(Drawn { width, height, radiance, distance });
    }
    Ok(out)
}

impl Drawn {
    /// Tiles of one size put back together, `columns` a row, row by row from
    /// the top left (as [`Light::tiles`] cut them).
    pub fn joined(tiles: &[Self], columns: u32) -> anyhow::Result<Self> {
        let first = tiles.first().context("no tile")?;
        let (tw, th) = (first.width as usize, first.height as usize);
        ensure!(tiles.iter().all(|t| (t.width, t.height) == (first.width, first.height)), "tiles of different sizes");
        ensure!(
            columns > 0 && tiles.len().is_multiple_of(columns as usize),
            "{} tiles in rows of {columns}",
            tiles.len()
        );
        let (columns, rows) = (columns as usize, tiles.len() / columns as usize);
        let (width, height) = (tw * columns, th * rows);
        let mut radiance = vec![0; width * height];
        let mut distance = vec![0.0; width * height];
        for (k, tile) in tiles.iter().enumerate() {
            let (x0, y0) = ((k % columns) * tw, (k / columns) * th);
            for y in 0..th {
                let to = (y0 + y) * width + x0;
                radiance[to..to + tw].copy_from_slice(&tile.radiance[y * tw..(y + 1) * tw]);
                distance[to..to + tw].copy_from_slice(&tile.distance[y * tw..(y + 1) * tw]);
            }
        }
        Ok(Self { width: width as u32, height: height as u32, radiance, distance })
    }

    /// Twice as wide and high, each pixel four (D-141: a probe at half its
    /// size under its tiles, until they come).
    pub fn doubled(&self) -> Self {
        // A row doubled across, then copied to the row under it.
        fn double<T: Copy + Default>(from: &[T], width: usize) -> Vec<T> {
            let mut out = vec![T::default(); 4 * from.len()];
            for (row, two) in from.chunks_exact(width).zip(out.chunks_exact_mut(4 * width)) {
                let (first, second) = two.split_at_mut(2 * width);
                for (pair, &v) in first.as_chunks_mut::<2>().0.iter_mut().zip(row) {
                    *pair = [v, v];
                }
                second.copy_from_slice(first);
            }
            out
        }
        let width = self.width as usize;
        Self {
            width: self.width * 2,
            height: self.height * 2,
            radiance: double(&self.radiance, width),
            distance: double(&self.distance, width),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_light_is_cut_in_tiles_row_by_row() {
        let light = Light {
            width: 4,
            height: 2,
            radiance: (0..8).map(|i| [i as f32, 0.0, 0.0]).collect(),
            distance: (0..8).map(|i| i as f32).collect(),
        };
        let tiles = light.tiles(2, 2);
        assert_eq!(tiles.len(), 4);
        assert_eq!(tiles[1].distance, vec![2.0, 3.0], "the top row's second tile");
        assert_eq!(tiles[2].distance, vec![4.0, 5.0]);
        let drawn = Drawn { width: 2, height: 1, radiance: vec![1, 2], distance: vec![5.0, -1.0] };
        let doubled = drawn.doubled();
        assert_eq!((doubled.width, doubled.height), (4, 2));
        assert_eq!(doubled.radiance, vec![1, 1, 2, 2, 1, 1, 2, 2]);
    }

    #[test]
    fn a_light_at_half_its_size_keeps_its_mean_and_its_nearest() {
        let light = Light {
            width: 4,
            height: 2,
            radiance: (0..8).map(|i| [i as f32, 1.0, 0.0]).collect(),
            distance: vec![5.0, 2.0, f32::INFINITY, f32::INFINITY, 3.0, 4.0, f32::INFINITY, 9.0],
        };
        let half = light.half();
        assert_eq!((half.width, half.height), (2, 1));
        assert_eq!(half.radiance[0], [(0.0 + 1.0 + 4.0 + 5.0) / 4.0, 1.0, 0.0]);
        assert_eq!(half.distance, vec![2.0, 9.0], "the nearest; the sky only where all four are");
    }

    #[test]
    fn the_curves_keep_a_hundred_levels_a_doubling_and_come_back() {
        for l in [0.0f32, 0.001, 0.05, 0.5, 1.0, 10.0, 63.0] {
            let back = decode_radiance(encode_radiance(l));
            assert!((back - l).abs() <= 1e-4 + l * 1e-4, "{l}: {back}");
        }
        // Levels of 10 bits between 1 and 2, and between 10 and 20.
        let levels = |a: f32, b: f32| (encode_radiance(b) - encode_radiance(a)) * 1023.0;
        assert!(levels(1.0, 2.0) > 80.0, "{}", levels(1.0, 2.0));
        assert!(levels(10.0, 20.0) > 80.0);
        for d in [0.0f32, 0.3, 3.0, 100.0, 1500.0] {
            let back = decode_distance(encode_distance(d));
            assert!((back - d).abs() <= 1e-3 + d * 1e-4, "{d}: {back}");
        }
        // Twelve bits: millimetres at a metre, decimetres at a hundred.
        let step = |d: f32| decode_distance(encode_distance(d) + 1.0 / 4095.0) - d;
        assert!(step(1.0) < 0.004 && step(100.0) < 0.2, "{} {}", step(1.0), step(100.0));
    }

    #[test]
    fn light_comes_back_through_av1() {
        let ffmpeg = Command::new("ffmpeg").arg("-version").stdout(Stdio::null()).status();
        if !ffmpeg.is_ok_and(|s| s.success()) {
            return;
        }
        let (width, height) = (96u32, 64u32);
        let mut radiance = Vec::new();
        let mut distance = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let t = x as f32 / width as f32;
                radiance.push([0.2 + 3.0 * t, 0.5 + t, 12.0 * (1.0 - t) + 0.1]);
                distance.push(if y < 16 { f32::INFINITY } else { 2.0 + 20.0 * t });
            }
        }
        let light = Light { width, height, radiance: radiance.clone(), distance: distance.clone() };
        let encoded = encode(&[light], Quality::default()).unwrap();
        let back = decode(&encoded.radiance, &encoded.distance, &encoded.sky).unwrap();
        assert_eq!(back.len(), 1);
        let drawn = &back[0];
        // Relative errors: small on the whole; a few more on the most
        // saturated colours, whose chroma 4:2:0 halves.
        let mut errors = Vec::new();
        for (i, packed) in drawn.radiance.iter().enumerate() {
            let band = |shift: u32| decode_radiance(((packed >> shift) & 1023) as f32 / 1023.0);
            let rgb = [band(0), band(10), band(20)];
            for k in 0..3 {
                let truth = radiance[i][k];
                errors.push((rgb[k] - truth).abs() / truth.max(0.05));
            }
        }
        errors.sort_by(f32::total_cmp);
        let mean = errors.iter().sum::<f32>() / errors.len() as f32;
        let p99 = errors[errors.len() * 99 / 100];
        assert!(
            mean < 0.06 && p99 < 0.2,
            "radiance off by {:.1} % on average, {:.1} % at the 99th centile",
            mean * 100.0,
            p99 * 100.0
        );
        for (i, &d) in drawn.distance.iter().enumerate() {
            if distance[i].is_finite() {
                // Within a few of the curve's 4096 levels.
                assert!((d - distance[i]).abs() <= 0.015 * (1.0 + distance[i]), "{d} for {}", distance[i]);
            } else {
                assert!(d < 0.0, "the sky stays the sky");
            }
        }
    }

    #[test]
    fn a_probe_s_tiles_decode_alone_in_any_order() {
        let ffmpeg = Command::new("ffmpeg").arg("-version").stdout(Stdio::null()).status();
        if !ffmpeg.is_ok_and(|s| s.success()) {
            return;
        }
        let (width, height) = (256u32, 128u32);
        let mut radiance = Vec::new();
        let mut distance = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let t = x as f32 / width as f32;
                radiance.push([0.2 + 3.0 * t, 0.5 + y as f32 / 64.0, 0.1 + 0.05 * (x / 64) as f32]);
                distance.push(if y < 64 && x < 64 { f32::INFINITY } else { 2.0 + 20.0 * t });
            }
        }
        let light = Light { width, height, radiance, distance };
        let tiles = light.tiles(4, 2);
        let encoded = encode(&tiles, Quality { intra: true, ..Quality::default() }).unwrap();
        let all = decode(&encoded.radiance, &encoded.distance, &encoded.sky).unwrap();
        assert_eq!(all.len(), 8);
        let some = decode_tiles(&encoded.radiance, &encoded.distance, &encoded.sky, &[6, 0, 3]).unwrap();
        assert_eq!(some.len(), 3);
        for (got, k) in some.iter().zip([6, 0, 3]) {
            assert_eq!(got, &all[k], "tile {k} decoded alone as among all");
        }
        let whole = Drawn::joined(&all, 4).unwrap();
        assert_eq!((whole.width, whole.height), (width, height));
        assert_eq!(whole.distance[64 * 256 + 70], all[5].distance[6], "a pixel of the second row's second tile");
        assert!(some[1].distance.iter().all(|&d| d < 0.0), "the first tile is all sky");
    }
}

#[cfg(test)]
mod bench {
    /// Where a probe's decoding spends its time (`OSAGE_PROBE=DIR`, a packed
    /// probe's folder): `cargo test --release -p osage-hologram-stream
    /// where_a_probe_spends -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn where_a_probe_spends_its_decoding() {
        let Some(dir) = std::env::var_os("OSAGE_PROBE").map(std::path::PathBuf::from) else { return };
        let read = |name: &str| std::fs::read(dir.join(name)).expect("a packed probe");
        let (c, d, m) = (read("colour.ivf"), read("depth.ivf"), read("mask.ivf"));
        let time = |what: &str, f: &mut dyn FnMut()| {
            let started = std::time::Instant::now();
            f();
            println!("{what}: {:.0} ms", started.elapsed().as_secs_f64() * 1000.0);
        };
        let (mut colours, mut depths, mut masks) = (Vec::new(), Vec::new(), Vec::new());
        time("AV1, colour", &mut || colours = crate::decode::planes(&c).unwrap());
        time("AV1, distance", &mut || depths = crate::decode::planes(&d).unwrap());
        time("AV1, sky", &mut || masks = crate::decode::planes(&m).unwrap());
        let mut rgb = Vec::new();
        time("YUV to RGB, as floats", &mut || rgb = colours[0].rgb());
        let q = |v: f32| (v.clamp(0.0, 1.0) * 1023.0 + 0.5) as u32;
        time("YUV to RGB, packed in 10 bits as converted", &mut || {
            assert!(!colours[0].rgb_as(0u32, |r, g, b| q(r) | (q(g) << 10) | (q(b) << 20)).is_empty())
        });
        let (mut grey, mut open) = (Vec::new(), Vec::new());
        time("distance and sky to floats", &mut || (grey, open) = (depths[0].grey(), masks[0].grey()));
        time("radiance packed in 10 bits", &mut || {
            let q = |v: f32| (v.clamp(0.0, 1.0) * 1023.0).round() as u32;
            let packed: Vec<u32> =
                rgb.iter().map(|&[r, g, b]| q(r) | (q(g) << 10) | (q(b) << 20) | (3 << 30)).collect();
            assert!(!packed.is_empty());
        });
        time("distance through its curve", &mut || {
            let d: Vec<f32> =
                grey.iter().zip(&open).map(|(&g, &o)| if o > 0.5 { -1.0 } else { super::decode_distance(g) }).collect();
            assert!(!d.is_empty());
        });
        time("all of decode()", &mut || assert_eq!(super::decode(&c, &d, &m).unwrap().len(), 1));
    }

    /// What a tiled probe's decoding costs (`OSAGE_PROBE=DIR`, a folder
    /// packed with `light-pack --tiles`, D-141): every tile, on one thread
    /// and on four; and the tiles nearest a view.
    #[test]
    #[ignore]
    fn what_a_probe_s_tiles_cost_to_decode() {
        let Some(dir) = std::env::var_os("OSAGE_PROBE").map(std::path::PathBuf::from) else { return };
        let read = |name: &str| std::fs::read(dir.join(name)).expect("a probe packed as tiles");
        let (c, d, m) = (read("tiles-colour.ivf"), read("tiles-depth.ivf"), read("tiles-mask.ivf"));
        let count = crate::ivf::frames(&c).unwrap().len();
        let time = |what: &str, f: &mut dyn FnMut()| {
            let started = std::time::Instant::now();
            f();
            println!("{what}: {:.0} ms", started.elapsed().as_secs_f64() * 1000.0);
        };
        let all: Vec<usize> = (0..count).collect();
        time(&format!("{count} tiles, one thread"), &mut || {
            assert_eq!(super::decode_tiles(&c, &d, &m, &all).unwrap().len(), count)
        });
        time(&format!("{count} tiles, four threads"), &mut || {
            std::thread::scope(|scope| {
                for part in all.chunks(count.div_ceil(4)) {
                    let (c, d, m) = (&c, &d, &m);
                    scope.spawn(move || super::decode_tiles(c, d, m, part).unwrap());
                }
            })
        });
        for n in [1, 8, 16, 32] {
            time(&format!("{n} tiles, one thread"), &mut || {
                assert_eq!(super::decode_tiles(&c, &d, &m, &all[..n]).unwrap().len(), n)
            });
        }
    }
}
