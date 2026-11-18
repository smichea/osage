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
//!   off; lossy, its sky padded, and where the sky is travels apart, exact
//!   (a mask).

use std::{
    io::{Read as _, Write as _},
    process::{Command, Stdio},
};

use anyhow::{Context as _, ensure};

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
    /// libaom's for distance.
    pub distance: u8,
    pub preset: u8,
}

impl Default for Quality {
    fn default() -> Self {
        Self { radiance: 10, distance: 12, preset: 6 }
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
        ],
        radiance,
    )?;
    let depth = ffmpeg(
        &["-f", "rawvideo", "-pix_fmt", "gray12le", "-s", &size, "-r", "30", "-i", "pipe:0"],
        &["-c:v", "libaom-av1", "-crf", &quality.distance.to_string(), "-b:v", "0", "-cpu-used", "6", "-row-mt", "1"],
        distance,
    )?;
    let mask = ffmpeg(
        &["-f", "rawvideo", "-pix_fmt", "gray", "-s", &size, "-r", "30", "-i", "pipe:0"],
        &["-c:v", "libaom-av1", "-crf", "0", "-b:v", "0", "-cpu-used", "6", "-row-mt", "1"],
        sky,
    )?;
    Ok(Encoded { radiance: colour, distance: depth, sky: mask })
}

fn ffmpeg(input: &[&str], codec: &[&str], raw: Vec<u8>) -> anyhow::Result<Vec<u8>> {
    let mut child = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error"])
        .args(input)
        .args(codec)
        .args(["-g", "9999", "-f", "ivf", "pipe:1"])
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
    writer.join().map_err(|_| anyhow::anyhow!("writing to ffmpeg panicked"))??;
    ensure!(status.success(), "ffmpeg: {errors}");
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
    let colours = crate::decode::planes(radiance)?;
    let depths = crate::decode::planes(distance)?;
    let masks = crate::decode::planes(sky)?;
    ensure!(colours.len() == depths.len() && depths.len() == masks.len(), "files of different lengths");
    let mut out = Vec::new();
    for ((c, d), m) in colours.iter().zip(&depths).zip(&masks) {
        let (width, height) = (c.width, c.height);
        let rgb = c.rgb();
        let mut radiance = Vec::with_capacity(rgb.len());
        for [r, g, b] in rgb {
            let q = |v: f32| (v.clamp(0.0, 1.0) * 1023.0).round() as u32;
            radiance.push(q(r) | (q(g) << 10) | (q(b) << 20) | (3 << 30));
        }
        let grey = d.grey();
        let open = m.grey();
        let distance = grey.iter().zip(&open).map(|(&g, &o)| if o > 0.5 { -1.0 } else { decode_distance(g) }).collect();
        out.push(Drawn { width, height, radiance, distance });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(mean < 0.06 && p99 < 0.2, "radiance off by {:.1} % on average, {:.1} % at the 99th centile", mean * 100.0, p99 * 100.0);
        for (i, &d) in drawn.distance.iter().enumerate() {
            if distance[i].is_finite() {
                assert!((d - distance[i]).abs() < 0.05 * distance[i] + 0.05, "{d} for {}", distance[i]);
            } else {
                assert!(d < 0.0, "the sky stays the sky");
            }
        }
    }
}
