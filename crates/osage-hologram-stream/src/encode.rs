// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Encodes a block: its pictures, in the block's order, as one AV1 video
//! (IVF), through ffmpeg — an offline tool of the packer, not a dependency
//! of the engine.
//!
//! - Colours (as seen, or without light): SVT-AV1, 10-bit 4:2:0, BT.709.
//! - Normals: libaom, 10-bit 4:4:4 RGB (the identity matrix): their three
//!   components are worth the same, none may be halved as chroma is.
//! - Depths: libaom, 12-bit grey, lossy, the ground padded.
//! - Masks (where the object is): libaom, 8-bit grey at its finest
//!   quantizer, nearly lossless; a picture of two values costs little.

use std::{
    io::{Read as _, Write as _},
    process::{Command, Stdio},
};

use anyhow::{Context as _, ensure};

use crate::{Series, decode::Picture, depth_to_grey};

/// How hard and how well to encode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quality {
    /// Constant quality of colours (0 best, 63 worst).
    pub colour: u8,
    /// Constant quality of normals.
    pub normal: u8,
    /// Constant quality of depths.
    pub depth: u8,
    /// SVT-AV1's preset (0 slowest and best, 13 fastest).
    pub preset: u8,
}

impl Default for Quality {
    fn default() -> Self {
        Self { colour: 30, normal: 24, depth: 16, preset: 6 }
    }
}

/// Fills what lies around the object (`mask`: where it is) with the
/// object's own values, spread smoothly outwards (push-pull: averages over
/// ever larger squares, brought back down where nothing was).
pub(crate) fn fill(width: usize, height: usize, mask: &[bool], values: &mut [[f32; 3]]) {
    if mask.len() != width * height || mask.iter().all(|&m| m) || !mask.iter().any(|&m| m) {
        return;
    }
    // Each level: (sum of values, weight) per texel, halved each time.
    let first: Vec<[f32; 4]> =
        values.iter().zip(mask).map(|(v, &m)| if m { [v[0], v[1], v[2], 1.0] } else { [0.0; 4] }).collect();
    let mut levels: Vec<(usize, usize, Vec<[f32; 4]>)> = vec![(width, height, first)];
    loop {
        let (w, h, texels) = levels.last().expect("a level");
        if *w == 1 && *h == 1 {
            break;
        }
        let (nw, nh) = (w.div_ceil(2), h.div_ceil(2));
        let mut next = vec![[0.0f32; 4]; nw * nh];
        for y in 0..*h {
            for x in 0..*w {
                let t = texels[y * w + x];
                let n = &mut next[(y / 2) * nw + x / 2];
                for i in 0..4 {
                    n[i] += t[i];
                }
            }
        }
        levels.push((nw, nh, next));
    }
    // Back down: a texel with no weight takes its parent's average.
    for i in (0..levels.len() - 1).rev() {
        let (upper, lower) = levels.split_at_mut(i + 1);
        let (w, h, texels) = &mut upper[i];
        let (pw, _, parents) = &lower[0];
        for y in 0..*h {
            for x in 0..*w {
                let t = &mut texels[y * *w + x];
                if t[3] <= 0.0 {
                    let p = parents[(y / 2) * pw + x / 2];
                    *t = if p[3] > 0.0 { [p[0] / p[3], p[1] / p[3], p[2] / p[3], 1.0] } else { [0.0; 4] };
                }
            }
        }
    }
    for ((v, &m), t) in values.iter_mut().zip(mask).zip(&levels[0].2) {
        if !m {
            let w = t[3].max(1e-6);
            *v = [t[0] / w, t[1] / w, t[2] / w];
        }
    }
}

/// Fills the ground of a colour-like picture (`mask`: where the object is)
/// with the object's own colours spread outwards: the codec then spends
/// nothing on the ground and bleeds no foreign colour into the object's
/// edges when it halves the chroma; the outline comes back from the mask.
pub fn pad(picture: &mut Picture, mask: &[bool]) {
    let mut values: Vec<[f32; 3]> =
        picture.rgba.as_chunks::<4>().0.iter().map(|p| [f32::from(p[0]), f32::from(p[1]), f32::from(p[2])]).collect();
    fill(picture.width as usize, picture.height as usize, mask, &mut values);
    for (p, v) in picture.rgba.as_chunks_mut::<4>().0.iter_mut().zip(&values) {
        for i in 0..3 {
            p[i] = v[i].round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// The IVF file of a block of `series` made of `pictures` (8-bit RGBA, all
/// one size), in their order.
pub fn block(series: Series, pictures: &[Picture], quality: Quality) -> anyhow::Result<Vec<u8>> {
    let first = pictures.first().context("a block without pictures")?;
    let (width, height) = (first.width, first.height);
    ensure!(pictures.iter().all(|p| (p.width, p.height) == (width, height)), "pictures of different sizes");
    let mut raw = Vec::new();
    for picture in pictures {
        match series {
            Series::Depth => {
                // The depth where the object is, spread over the ground.
                let mask: Vec<bool> = picture.rgba.as_chunks::<4>().0.iter().map(|t| t[2] > 127).collect();
                let mut values: Vec<[f32; 3]> = picture
                    .rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|t| [f32::from(depth_to_grey([t[0], t[1], t[2], t[3]])), 0.0, 0.0])
                    .collect();
                fill(width as usize, height as usize, &mask, &mut values);
                for v in values {
                    raw.extend_from_slice(&(v[0].round().clamp(0.0, 4095.0) as u16).to_le_bytes());
                }
            }
            Series::Mask => {
                raw.extend(picture.rgba.as_chunks::<4>().0.iter().map(|t| if t[2] > 127 { 255u8 } else { 0 }));
            }
            _ => {
                for texel in picture.rgba.as_chunks::<4>().0.iter() {
                    raw.extend_from_slice(&texel[..3]);
                }
            }
        }
    }
    let size = format!("{width}x{height}");
    let mut args: Vec<String> = ["-hide_banner", "-loglevel", "error", "-f", "rawvideo"].map(String::from).to_vec();
    let codec: Vec<String> = match series {
        Series::Depth => {
            args.extend(["-pix_fmt", "gray12le"].map(String::from));
            let crf = quality.depth.to_string();
            ["-c:v", "libaom-av1", "-crf", &crf, "-b:v", "0", "-cpu-used", "6", "-row-mt", "1"]
                .map(String::from)
                .to_vec()
        }
        Series::Mask => {
            args.extend(["-pix_fmt", "gray"].map(String::from));
            ["-c:v", "libaom-av1", "-crf", "0", "-b:v", "0", "-cpu-used", "6", "-row-mt", "1"]
                .map(String::from)
                .to_vec()
        }
        Series::Normal => {
            args.extend(["-pix_fmt", "rgb24"].map(String::from));
            [
                "-pix_fmt",
                "gbrp10le",
                "-c:v",
                "libaom-av1",
                "-crf",
                &quality.normal.to_string(),
                "-b:v",
                "0",
                "-cpu-used",
                "6",
                "-row-mt",
                "1",
            ]
            .map(String::from)
            .to_vec()
        }
        Series::View | Series::Albedo => {
            args.extend(["-pix_fmt", "rgb24"].map(String::from));
            [
                "-vf",
                "scale=out_color_matrix=bt709:out_range=tv,format=yuv420p10le",
                "-color_primaries",
                "bt709",
                "-color_trc",
                "iec61966-2-1",
                "-colorspace",
                "bt709",
                "-color_range",
                "tv",
                "-c:v",
                "libsvtav1",
                "-preset",
                &quality.preset.to_string(),
                "-crf",
                &quality.colour.to_string(),
            ]
            .map(String::from)
            .to_vec()
        }
    };
    args.extend(["-s", &size, "-r", "30", "-i", "pipe:0"].map(String::from));
    args.extend(codec);
    // One whole picture, at the start; the rest small turns from it.
    args.extend(["-g", "9999", "-f", "ivf", "pipe:1"].map(String::from));
    let mut ffmpeg = Command::new("ffmpeg")
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("ffmpeg (with libsvtav1 and libaom) is needed to pack holograms")?;
    let mut stdin = ffmpeg.stdin.take().context("ffmpeg's input")?;
    let writer = std::thread::spawn(move || stdin.write_all(&raw));
    let mut ivf = Vec::new();
    ffmpeg.stdout.take().context("ffmpeg's output")?.read_to_end(&mut ivf)?;
    let mut errors = String::new();
    ffmpeg.stderr.take().context("ffmpeg's errors")?.read_to_string(&mut errors)?;
    let status = ffmpeg.wait()?;
    writer.join().map_err(|_| anyhow::anyhow!("writing to ffmpeg panicked"))??;
    ensure!(status.success(), "ffmpeg failed on a {} block: {errors}", series.name());
    Ok(ivf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode;

    fn ffmpeg() -> bool {
        Command::new("ffmpeg").arg("-version").stdout(Stdio::null()).status().is_ok_and(|s| s.success())
    }

    /// Smooth colours, a little different from view to view.
    fn smooth(n: usize) -> Vec<Picture> {
        (0..n)
            .map(|k| {
                let mut rgba = Vec::new();
                for y in 0..128u32 {
                    for x in 0..128u32 {
                        let (fx, fy) = (x as f32 / 127.0, y as f32 / 127.0);
                        let shift = k as f32 * 0.02;
                        rgba.extend_from_slice(&[
                            (255.0 * (0.5 + 0.45 * (fx * 2.0 + shift).sin())) as u8,
                            (255.0 * fy) as u8,
                            (128.0 + 100.0 * ((fx * 6.0 + fy * 3.0 + shift * 10.0).sin())) as u8,
                            255,
                        ]);
                    }
                }
                Picture { width: 128, height: 128, rgba }
            })
            .collect()
    }

    fn psnr(a: &[Picture], b: &[Picture]) -> (f64, [f64; 3]) {
        let mut squared = [0.0f64; 3];
        let mut n = 0.0;
        for (a, b) in a.iter().zip(b) {
            for (x, y) in a.rgba.as_chunks::<4>().0.iter().zip(b.rgba.as_chunks::<4>().0.iter()) {
                for i in 0..3 {
                    squared[i] += (f64::from(x[i]) - f64::from(y[i])).powi(2);
                }
                n += 1.0;
            }
        }
        let db = |s: f64| 10.0 * (255.0f64 * 255.0 / (s / n).max(1e-9)).log10();
        (db(squared.iter().sum::<f64>() / 3.0), squared.map(db))
    }

    #[test]
    fn smooth_colours_come_back_close() {
        if !ffmpeg() {
            return;
        }
        let pictures = smooth(6);
        let back = decode::block(&block(Series::View, &pictures, Quality::default()).unwrap(), Series::View).unwrap();
        assert_eq!(back.len(), pictures.len());
        let (all, channels) = psnr(&pictures, &back);
        assert!(all > 38.0, "{all:.1} dB (red, green, blue: {channels:?})");
        let normals =
            decode::block(&block(Series::Normal, &pictures, Quality::default()).unwrap(), Series::Normal).unwrap();
        let (all, channels) = psnr(&pictures, &normals);
        assert!(all > 38.0, "normals {all:.1} dB ({channels:?})");
    }

    /// Our decoder against ffmpeg's (libdav1d and its colour conversion)
    /// on the same file: they differ only by rounding and chroma upsampling.
    #[test]
    fn decoding_matches_ffmpeg() {
        if !ffmpeg() {
            return;
        }
        let pictures = smooth(3);
        let ivf = block(Series::View, &pictures, Quality::default()).unwrap();
        let ours = decode::block(&ivf, Series::View).unwrap();
        let mut theirs = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-c:v", "libdav1d", "-i", "pipe:0"])
            .args(["-vf", "scale=in_color_matrix=bt709:in_range=tv", "-pix_fmt", "rgb24", "-f", "rawvideo", "pipe:1"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = theirs.stdin.take().unwrap();
        let writer = std::thread::spawn(move || stdin.write_all(&ivf));
        let mut raw = Vec::new();
        theirs.stdout.take().unwrap().read_to_end(&mut raw).unwrap();
        writer.join().unwrap().unwrap();
        assert!(theirs.wait().unwrap().success());
        let theirs: Vec<Picture> = raw
            .as_chunks::<{ 128 * 128 * 3 }>()
            .0
            .iter()
            .map(|frame| Picture {
                width: 128,
                height: 128,
                rgba: frame.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
            })
            .collect();
        assert_eq!(theirs.len(), ours.len());
        let (all, channels) = psnr(&theirs, &ours);
        assert!(all > 38.0, "ours against ffmpeg's: {all:.1} dB ({channels:?})");
    }

    #[test]
    fn padding_keeps_the_object_and_smooths_the_ground() {
        let mut picture = Picture { width: 4, height: 4, rgba: vec![0; 64] };
        let mut mask = vec![false; 16];
        for (i, texel) in picture.rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            if i % 4 < 2 {
                texel.copy_from_slice(&[200, 100, 50, 255]);
                mask[i] = true;
            } else {
                texel.copy_from_slice(&[255, 0, 255, 255]);
            }
        }
        pad(&mut picture, &mask);
        for texel in picture.rgba.as_chunks::<4>().0.iter() {
            assert_eq!(&texel[..3], &[200, 100, 50]);
        }
    }
}
