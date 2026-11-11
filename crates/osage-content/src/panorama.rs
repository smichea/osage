// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Panoramas of the sky: equirectangular images in Radiance's HDR format
//! (`.hdr`, RGBE), as Poly Haven publishes them, decoded to linear light.

use std::path::Path;

/// An equirectangular image in linear light: `width` by `height` pixels, row
/// by row from the top (straight up) down; across, a full turn.
pub struct Equirect {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<[f32; 3]>,
}

/// Resolutions a panorama is looked for in, best first.
const RESOLUTIONS: [&str; 3] = ["4k", "2k", "1k"];

/// Poly Haven panorama `id` at the best resolution there is, or `None` if it
/// is not there or does not decode.
pub fn load(root: &Path, id: &str) -> Option<Equirect> {
    let folder = root.join("polyhaven").join(id);
    let path = RESOLUTIONS.iter().map(|r| folder.join(format!("{id}_{r}.hdr"))).find(|p| p.exists())?;
    let bytes = std::fs::read(&path).ok()?;
    decode(&bytes).inspect_err(|error| eprintln!("{}: {error}", path.display())).ok()
}

/// Decodes a Radiance HDR image: a text header, a line giving the size
/// (`-Y height +X width`), then the pixels as RGBE — flat, or run-length
/// encoded one channel at a time per scanline.
pub fn decode(bytes: &[u8]) -> Result<Equirect, String> {
    let mut at = 0;
    let mut line = || -> Result<&[u8], String> {
        let end = bytes[at..].iter().position(|&b| b == b'\n').ok_or("the header does not end")? + at;
        let text = &bytes[at..end];
        at = end + 1;
        Ok(text)
    };
    let first = line()?;
    if !first.starts_with(b"#?") {
        return Err("not a Radiance HDR image".to_owned());
    }
    loop {
        let header = line()?;
        if header.is_empty() {
            break;
        }
        if header.starts_with(b"FORMAT=") && header != b"FORMAT=32-bit_rle_rgbe" {
            return Err(format!("unsupported format {}", String::from_utf8_lossy(header)));
        }
    }
    let size = String::from_utf8_lossy(line()?).into_owned();
    let words: Vec<&str> = size.split_whitespace().collect();
    let (height, width) = match words.as_slice() {
        ["-Y", h, "+X", w] => {
            (h.parse::<usize>().map_err(|e| e.to_string())?, w.parse::<usize>().map_err(|e| e.to_string())?)
        }
        _ => return Err(format!("unsupported orientation {size}")),
    };

    let data = &bytes[at..];
    let mut rgbe = vec![[0u8; 4]; width * height];
    let mut read = 0;
    for row in rgbe.chunks_mut(width) {
        let header = data.get(read..read + 4).ok_or("pixels end early")?;
        let encoded = (8..=0x7fff).contains(&width) && header[0] == 2 && header[1] == 2 && header[2] & 0x80 == 0;
        if !encoded {
            for pixel in row.iter_mut() {
                let bytes = data.get(read..read + 4).ok_or("pixels end early")?;
                *pixel = [bytes[0], bytes[1], bytes[2], bytes[3]];
                read += 4;
            }
            continue;
        }
        if (usize::from(header[2]) << 8 | usize::from(header[3])) != width {
            return Err("a scanline has the wrong width".to_owned());
        }
        read += 4;
        for channel in 0..4 {
            let mut x = 0;
            while x < width {
                let count = *data.get(read).ok_or("pixels end early")?;
                read += 1;
                if count > 128 {
                    // A run: one value, repeated.
                    let run = usize::from(count - 128);
                    let value = *data.get(read).ok_or("pixels end early")?;
                    read += 1;
                    for pixel in row.get_mut(x..x + run).ok_or("a run overflows its scanline")? {
                        pixel[channel] = value;
                    }
                    x += run;
                } else {
                    // Literal values.
                    let count = usize::from(count);
                    let values = data.get(read..read + count).ok_or("pixels end early")?;
                    for (pixel, &value) in
                        row.get_mut(x..x + count).ok_or("values overflow their scanline")?.iter_mut().zip(values)
                    {
                        pixel[channel] = value;
                    }
                    read += count;
                    x += count;
                }
            }
        }
    }
    let pixels = rgbe.iter().map(|&[r, g, b, e]| linear(r, g, b, e)).collect();
    Ok(Equirect { width, height, pixels })
}

/// A shared exponent: each mantissa times 2^(e - 136).
fn linear(r: u8, g: u8, b: u8, e: u8) -> [f32; 3] {
    if e == 0 {
        return [0.0; 3];
    }
    let scale = (f32::from(e) - 136.0).exp2();
    [f32::from(r) * scale, f32::from(g) * scale, f32::from(b) * scale]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(width: usize, height: usize) -> Vec<u8> {
        format!("#?RADIANCE\nFORMAT=32-bit_rle_rgbe\nEXPOSURE=1.0\n\n-Y {height} +X {width}\n").into_bytes()
    }

    #[test]
    fn flat_pixels_decode_to_linear_light() {
        let mut bytes = header(2, 1);
        // 128 × 2^(129 - 136) = 1; 128 × 2^(130 - 136) = 2.
        bytes.extend_from_slice(&[128, 64, 0, 129, 128, 128, 128, 130]);
        let image = decode(&bytes).unwrap();
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.pixels, vec![[1.0, 0.5, 0.0], [2.0, 2.0, 2.0]]);
    }

    #[test]
    fn run_length_scanlines_decode_channel_by_channel() {
        let width = 10;
        let mut bytes = header(width, 1);
        bytes.extend_from_slice(&[2, 2, 0, width as u8]);
        // Red: a run of 10 × 128. Green: literals 0..10. Blue: a run of 4, then 6 literals. Exponent: a run.
        bytes.extend_from_slice(&[128 + 10, 128]);
        bytes.push(10);
        bytes.extend(0..10u8);
        bytes.extend_from_slice(&[128 + 4, 16, 6, 1, 2, 3, 4, 5, 6]);
        bytes.extend_from_slice(&[128 + 10, 129]);
        let image = decode(&bytes).unwrap();
        assert_eq!(image.pixels.len(), width);
        assert_eq!(image.pixels[0], [1.0, 0.0, 0.125]);
        assert_eq!(image.pixels[9], [1.0, 9.0 / 128.0, 6.0 / 128.0]);
    }

    #[test]
    fn what_is_not_an_hdr_image_is_refused() {
        assert!(decode(b"P6\n2 2\n255\n").is_err());
        assert!(decode(&header(4, 4)).is_err(), "no pixels");
        let mut sideways = b"#?RADIANCE\n\n+X 4 -Y 4\n".to_vec();
        sideways.extend_from_slice(&[0; 64]);
        assert!(decode(&sideways).is_err(), "an orientation it does not read");
    }
}

/// The real sky of the asset manifest, when it has been fetched:
/// `cargo test -p osage-content --release -- --ignored`.
#[cfg(test)]
#[test]
#[ignore = "needs the asset manifest's content"]
fn the_manifest_sky_decodes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let sky = load(&root, "kloofendal_48d_partly_cloudy_puresky").expect("fetched with tools/fetch-assets.py");
    assert_eq!((sky.width, sky.height), (4096, 2048));
    let luminance = |p: &[f32; 3]| 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2];
    let brightest = sky.pixels.iter().map(luminance).fold(0.0, f32::max);
    let mut sorted: Vec<f32> = sky.pixels[..sky.width * sky.height / 2].iter().map(luminance).collect();
    sorted.sort_by(f32::total_cmp);
    let median = sorted[sorted.len() / 2];
    println!("brightest {brightest}, median of the upper half {median}, ratio {}", brightest / median);
    // Unclipped: the sun is thousands of times brighter than the sky.
    assert!(brightest / median > 1000.0);
}
