// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
use super::Pixels;
use super::mipmap;
use bevy::prelude::Srgba;
#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
use bevy::render::render_resource::{Extent3d, TextureFormat};
use image::{ImageBuffer, Rgba, imageops};
#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
use image::{ImageFormat, ImageReader};
#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
use sha2::{Digest, Sha256};
#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
use std::{io::Cursor, sync::Arc};

// This is the original linear-light algorithm, retained as an independent byte oracle.
// imageops computes its own footprint and rounding for every size.
fn reference_mips(bytes: &mut Vec<u8>, width: u32, height: u32) -> u32 {
    let linear = std::array::from_fn::<_, 256, _>(|v| (Srgba::gamma_function(v as f32 / 255.) * 65535.).round() as u16);
    let mut level = ImageBuffer::from_fn(width, height, |x, y| {
        let at = (y as usize * width as usize + x as usize) * 4;
        Rgba([
            linear[bytes[at] as usize],
            linear[bytes[at + 1] as usize],
            linear[bytes[at + 2] as usize],
            u16::from(bytes[at + 3]) * 257,
        ])
    });
    let mut count = 1;
    while level.width() > 1 || level.height() > 1 {
        level = imageops::thumbnail(&level, (level.width() / 2).max(1), (level.height() / 2).max(1));
        for p in level.pixels() {
            for &c in &p.0[..3] {
                bytes.push((Srgba::gamma_function_inverse(f32::from(c) / 65535.) * 255.).round() as u8);
            }
            bytes.push(((u32::from(p[3]) + 128) / 257) as u8);
        }
        count += 1;
    }
    count
}

#[test]
fn rgba_and_level_count_match_for_odd_even_and_one_dimensional_inputs() {
    let mut seed = 0x1742abc3_u32;
    let sizes = (1..=28).flat_map(|w| (1..=28).map(move |h| (w, h))).chain([(1, 513), (1025, 1), (259, 63), (64, 256)]);
    for (width, height) in sizes {
        let mut data: Vec<_> = (0..width * height * 4)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                (seed >> 24) as u8
            })
            .collect();
        let mut control = data.clone();
        let old = reference_mips(&mut control, width, height);
        let new = mipmap::append(&mut data, width, height);
        assert_eq!(new, old, "level count of {width}×{height}");
        assert_eq!(data, control, "RGBA pyramid of {width}×{height}");
    }
}

#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
fn reference_read(encoded: &[u8], format: ImageFormat) -> Arc<Pixels> {
    let rgba = ImageReader::with_format(Cursor::new(encoded), format).decode().unwrap().into_rgba8();
    let (width, height) = rgba.dimensions();
    let min_alpha = rgba.pixels().map(|p| p[3]).min().unwrap();
    let mut bytes = rgba.into_raw();
    let levels = reference_mips(&mut bytes, width, height);
    Arc::new(Pixels {
        format: TextureFormat::Rgba8UnormSrgb,
        digest: Sha256::digest(encoded).into(),
        size: Extent3d { width, height, depth_or_array_layers: 1 },
        min_alpha,
        levels,
        bytes,
    })
}

// The optional Linux replay pins its test thread. Its CPU clock excludes time
// spent waiting for unrelated processes on the same core.
#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
fn thread_ns() -> u64 {
    use std::ffi::{c_int, c_long};
    #[repr(C)]
    struct Timespec {
        seconds: c_long,
        nanoseconds: c_long,
    }
    unsafe extern "C" {
        fn clock_gettime(clock: c_int, time: *mut Timespec) -> c_int;
    }
    const CLOCK_THREAD_CPUTIME_ID: c_int = 3;
    let mut t = Timespec { seconds: 0, nanoseconds: 0 };
    // SAFETY: Linux LP64 uses two longs for timespec; the output is writable.
    assert_eq!(unsafe { clock_gettime(CLOCK_THREAD_CPUTIME_ID, &mut t) }, 0);
    t.seconds as u64 * 1_000_000_000 + t.nanoseconds as u64
}

#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
#[test]
#[ignore = "measured in fresh processes by check-mip-prepare.py"]
fn measure_pixels_read() {
    use std::{env, fs, hint::black_box, time::Instant};
    let encoded = fs::read(env::var_os("OSAGE_MIP_INPUT").expect("encoded image path")).unwrap();
    let mode = env::var("OSAGE_MIP_METHOD").unwrap();
    let format = image::guess_format(&encoded).unwrap();
    let mime = match format {
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::Png => "image/png",
        _ => panic!("expected PNG or JPEG"),
    };
    let old = || reference_read(&encoded, format);
    let new = || Pixels::read(&encoded, mime).unwrap();
    let count = env::var("OSAGE_MIP_COUNT").unwrap().parse::<usize>().unwrap();
    assert!(count > 0);
    let mut samples = Vec::with_capacity(count);
    let mut cpu = Vec::with_capacity(count);
    let mut value = None;
    for i in 0..count {
        let active = thread_ns();
        let start = Instant::now();
        let output = match mode.as_str() {
            "scalar" => old(),
            "current" => new(),
            _ => panic!("unknown method"),
        };
        samples.push(start.elapsed().as_nanos() as u64);
        cpu.push(thread_ns() - active);
        if i + 1 == count {
            value = Some(output);
        } else {
            black_box(output);
        }
    }
    let actual = value.unwrap();
    let control = if mode == "scalar" { new() } else { old() };
    assert_eq!(
        (actual.digest, actual.levels, actual.size, actual.min_alpha),
        (control.digest, control.levels, control.size, control.min_alpha)
    );
    assert_eq!(actual.bytes, control.bytes, "complete decoded RGBA pyramid differs");
    println!(
        "OSAGE_MIP_MEASUREMENT {}",
        serde_json::json!({"samples_ns":samples,"cpu_ns":cpu,"dimensions":[actual.size.width,actual.size.height],"levels":actual.levels,"min_alpha":actual.min_alpha, "encoded_bytes":encoded.len(),"rgba_bytes":actual.bytes.len(),"rgba_sha256":crate::bench::sha(&actual.bytes),"encoded_sha256":crate::bench::sha(&encoded)})
    );
}
