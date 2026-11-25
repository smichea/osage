// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use bevy::prelude::Srgba;
use image::{ImageBuffer, Rgba, imageops};
use std::sync::LazyLock;

type LinearImage = ImageBuffer<Rgba<u16>, Vec<u16>>;

static TO_LINEAR: LazyLock<[u16; 256]> =
    LazyLock::new(|| std::array::from_fn(|n| (Srgba::gamma_function(n as f32 / 255.) * 65535.).round() as u16));

// First linear u16 whose rounded Bevy sRGB value is 1, 2, ..., 255.
// The exhaustive unit test regenerates every output with the original transfer function.
const SRGB8_START: [u16; 255] = [
    10, 30, 50, 70, 90, 110, 130, 150, 170, 189, 209, 230, 253, 276, 301, 327, 354, 382, 412, 443, 475, 509, 544, 580,
    618, 657, 698, 740, 783, 828, 875, 923, 972, 1023, 1075, 1129, 1185, 1242, 1300, 1360, 1422, 1486, 1551, 1617,
    1685, 1755, 1827, 1900, 1975, 2052, 2130, 2210, 2292, 2376, 2461, 2548, 2637, 2727, 2820, 2914, 3010, 3108, 3208,
    3309, 3412, 3518, 3625, 3734, 3844, 3957, 4072, 4188, 4307, 4427, 4550, 4674, 4800, 4928, 5059, 5191, 5325, 5461,
    5599, 5740, 5882, 6026, 6173, 6321, 6471, 6624, 6778, 6935, 7094, 7255, 7418, 7583, 7750, 7919, 8091, 8265, 8440,
    8618, 8798, 8981, 9165, 9352, 9541, 9732, 9925, 10121, 10318, 10518, 10720, 10925, 11132, 11341, 11552, 11765,
    11981, 12199, 12420, 12643, 12868, 13095, 13325, 13557, 13791, 14028, 14267, 14508, 14752, 14998, 15247, 15498,
    15751, 16007, 16265, 16525, 16788, 17054, 17321, 17592, 17864, 18139, 18417, 18697, 18980, 19264, 19552, 19842,
    20134, 20429, 20727, 21027, 21329, 21634, 21942, 22252, 22564, 22880, 23197, 23518, 23840, 24166, 24494, 24824,
    25158, 25493, 25832, 26173, 26516, 26862, 27211, 27563, 27917, 28273, 28633, 28995, 29359, 29727, 30097, 30469,
    30845, 31223, 31603, 31987, 32373, 32762, 33153, 33547, 33944, 34344, 34747, 35152, 35560, 35970, 36384, 36800,
    37219, 37640, 38065, 38492, 38922, 39355, 39790, 40229, 40670, 41114, 41561, 42011, 42463, 42918, 43377, 43838,
    44301, 44768, 45238, 45710, 46185, 46663, 47144, 47628, 48115, 48605, 49097, 49593, 50091, 50592, 51096, 51604,
    52114, 52627, 53142, 53661, 54183, 54708, 55235, 55766, 56300, 56836, 57376, 57918, 58464, 59012, 59564, 60118,
    60675, 61236, 61799, 62366, 62935, 63508, 64083, 64662, 65244,
];

static TO_SRGB: [u8; 65536] = {
    let mut table = [0; 65536];
    let mut value = 0;
    let mut i = 0;
    while i < table.len() {
        if value < SRGB8_START.len() && i == SRGB8_START[value] as usize {
            value += 1;
        }
        table[i] = value as u8;
        i += 1;
    }
    table
};

fn halves_exactly(width: u32, height: u32) -> bool {
    // On this domain the edge coordinates used by imageops::thumbnail are exact f32 integers.
    width.is_multiple_of(2) && height.is_multiple_of(2) && width <= 1 << 24 && height <= 1 << 24
}

fn half<T: Copy>(
    pixels: &[T],
    width: u32,
    height: u32,
    rgb: impl Fn(T) -> u32,
    alpha: impl Fn(T) -> u32,
) -> LinearImage {
    let stride = width as usize * 4;
    let mut out = Vec::with_capacity((width / 2) as usize * (height / 2) as usize * 4);
    for rows in pixels.chunks_exact(2 * stride) {
        let (top, bottom) = rows.split_at(stride);
        for (a, b) in top.as_chunks::<8>().0.iter().zip(bottom.as_chunks::<8>().0) {
            for k in 0..3 {
                let sum = rgb(a[k]) + rgb(a[k + 4]) + rgb(b[k]) + rgb(b[k + 4]);
                out.push(((sum + 2) / 4) as u16);
            }
            let sum = alpha(a[3]) + alpha(a[7]) + alpha(b[3]) + alpha(b[7]);
            out.push(((sum + 2) / 4) as u16);
        }
    }
    ImageBuffer::from_raw(width / 2, height / 2, out).expect("one RGBA value per 2 by 2 input block")
}

fn append_level(bytes: &mut Vec<u8>, level: &LinearImage) {
    for p in level.pixels() {
        bytes.extend_from_slice(&[
            TO_SRGB[p[0] as usize],
            TO_SRGB[p[1] as usize],
            TO_SRGB[p[2] as usize],
            ((u32::from(p[3]) + 128) / 257) as u8,
        ]);
    }
}

pub(super) fn append(bytes: &mut Vec<u8>, width: u32, height: u32) -> u32 {
    if width == 1 && height == 1 {
        return 1;
    }
    let next = |w: u32, h: u32| ((w / 2).max(1), (h / 2).max(1));
    let (mut w, mut h) = (width, height);
    let mut extra = 0;
    let mut levels = 1;
    while w > 1 || h > 1 {
        (w, h) = next(w, h);
        extra += w as usize * h as usize * 4;
        levels += 1;
    }
    bytes.reserve_exact(extra);
    let linear = &*TO_LINEAR;
    let mut level = if halves_exactly(width, height) {
        // Build the first reduced level directly from the original bytes.
        half(bytes, width, height, |c| u32::from(linear[usize::from(c)]), |a| u32::from(a) * 257)
    } else {
        let full = ImageBuffer::from_fn(width, height, |x, y| {
            let i = (y as usize * width as usize + x as usize) * 4;
            Rgba([
                linear[bytes[i] as usize],
                linear[bytes[i + 1] as usize],
                linear[bytes[i + 2] as usize],
                u16::from(bytes[i + 3]) * 257,
            ])
        });
        let (w, h) = next(width, height);
        imageops::thumbnail(&full, w, h)
    };
    append_level(bytes, &level);
    while level.width() > 1 || level.height() > 1 {
        let (w, h) = level.dimensions();
        level = if halves_exactly(w, h) {
            half(level.as_raw(), w, h, u32::from, u32::from)
        } else {
            let (w, h) = next(w, h);
            imageops::thumbnail(&level, w, h)
        };
        append_level(bytes, &level);
    }
    levels
}

#[cfg(test)]
#[test]
fn all_linear_u16_codes_match_the_bevy_transfer_function() {
    for c in 0..=u16::MAX {
        let expected = (Srgba::gamma_function_inverse(f32::from(c) / 65535.) * 255.).round() as u8;
        assert_eq!(TO_SRGB[usize::from(c)], expected, "linear component {c}");
    }
}
