// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Decodes a block: its AV1 frames (rav1d, dav1d's Rust port) back into
//! 8-bit RGBA pictures of the kind `hologram-bake` writes — colours in
//! sRGB, normals as they were, depths as 16 bits over red and green with
//! blue telling where the object is.

use std::{mem::MaybeUninit, ptr::NonNull};

use anyhow::{Context as _, bail, ensure};
use rav1d::include::dav1d::{
    data::Dav1dData,
    dav1d::{Dav1dContext, Dav1dSettings},
    headers::{
        DAV1D_MC_BT470BG, DAV1D_MC_BT601, DAV1D_MC_IDENTITY, DAV1D_PIXEL_LAYOUT_I400, DAV1D_PIXEL_LAYOUT_I420,
        DAV1D_PIXEL_LAYOUT_I422,
    },
    picture::Dav1dPicture,
};
use rav1d::src::lib::{
    dav1d_close, dav1d_data_create, dav1d_data_unref, dav1d_default_settings, dav1d_get_picture, dav1d_open,
    dav1d_picture_unref, dav1d_send_data,
};

use crate::{Series, grey_to_depth, ivf};

/// A decoded view: 8-bit RGBA, row after row.
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The depth texels of a block, from its depth and mask blocks' files.
pub fn depths(depth: &[u8], mask: &[u8]) -> anyhow::Result<Vec<Picture>> {
    let mut depths = block(depth, Series::Depth)?;
    let masks = block(mask, Series::Mask)?;
    ensure!(depths.len() == masks.len(), "{} depths and {} masks", depths.len(), masks.len());
    for (d, m) in depths.iter_mut().zip(&masks) {
        for (texel, there) in d.rgba.as_chunks_mut::<4>().0.iter_mut().zip(m.rgba.as_chunks::<4>().0.iter()) {
            if there[0] <= 127 {
                texel.copy_from_slice(&[0, 0, 0, 255]);
            }
        }
    }
    Ok(depths)
}

/// The pictures of a block of `series`, from its IVF file, in the block's
/// order; a depth block's are all taken as the object ([`depths`] joins
/// them to their mask).
pub fn block(file: &[u8], series: Series) -> anyhow::Result<Vec<Picture>> {
    let mut decoder = Decoder::new()?;
    let mut pictures = Vec::new();
    for frame in ivf::frames(file)? {
        decoder.send(frame, |picture| pictures.push(convert(picture, series)))?;
    }
    decoder.drain(|picture| pictures.push(convert(picture, series)))?;
    pictures.into_iter().collect()
}

/// dav1d's "try again": a picture waits to be taken, or more data is needed.
fn again(result: i32) -> bool {
    result == -libc::EAGAIN
}

struct Decoder {
    context: Option<Dav1dContext>,
}

impl Decoder {
    fn new() -> anyhow::Result<Self> {
        let mut settings = MaybeUninit::<Dav1dSettings>::uninit();
        // SAFETY: the settings are written whole by dav1d_default_settings.
        let mut settings = unsafe {
            dav1d_default_settings(NonNull::new(settings.as_mut_ptr()).context("settings")?);
            settings.assume_init()
        };
        // One thread a block: blocks are decoded side by side.
        settings.n_threads = 1;
        settings.max_frame_delay = 1;
        let mut context: Option<Dav1dContext> = None;
        // SAFETY: both pointers are to live locals.
        let result = unsafe { dav1d_open(NonNull::new(&mut context), NonNull::new(&mut settings)) };
        ensure!(result.0 == 0 && context.is_some(), "the AV1 decoder did not open ({})", result.0);
        Ok(Self { context })
    }

    /// Sends one frame's data, handing every picture that comes out to `take`.
    fn send(&mut self, frame: &[u8], mut take: impl FnMut(&Dav1dPicture)) -> anyhow::Result<()> {
        let mut data = Dav1dData::default();
        // SAFETY: dav1d_data_create allocates `frame.len()` bytes for `data`
        // and returns where they start; they are written before use.
        unsafe {
            let at = dav1d_data_create(NonNull::new(&mut data), frame.len());
            ensure!(!at.is_null(), "no memory for an AV1 frame");
            std::ptr::copy_nonoverlapping(frame.as_ptr(), at, frame.len());
        }
        while data.sz > 0 {
            // SAFETY: the context is open; `data` is dav1d's own.
            let result = unsafe { dav1d_send_data(self.context, NonNull::new(&mut data)) };
            if again(result.0) {
                // A picture waits: take it, then send the rest.
                if !self.take(&mut take)? {
                    bail!("the AV1 decoder neither takes data nor gives a picture");
                }
            } else if result.0 != 0 {
                // SAFETY: the data is dav1d's and is released once.
                unsafe { dav1d_data_unref(NonNull::new(&mut data)) };
                bail!("AV1 data refused ({})", result.0);
            }
        }
        while self.take(&mut take)? {}
        Ok(())
    }

    /// The pictures still inside once all the data is in.
    fn drain(&mut self, mut take: impl FnMut(&Dav1dPicture)) -> anyhow::Result<()> {
        while self.take(&mut take)? {}
        Ok(())
    }

    /// Takes one picture out, if one is ready.
    fn take(&mut self, take: &mut impl FnMut(&Dav1dPicture)) -> anyhow::Result<bool> {
        let mut picture = Dav1dPicture::default();
        // SAFETY: the context is open; `picture` is a live local dav1d fills.
        let result = unsafe { dav1d_get_picture(self.context, NonNull::new(&mut picture)) };
        if again(result.0) {
            return Ok(false);
        }
        ensure!(result.0 == 0, "an AV1 picture did not decode ({})", result.0);
        take(&picture);
        // SAFETY: the picture is dav1d's and is released once.
        unsafe { dav1d_picture_unref(NonNull::new(&mut picture)) };
        Ok(true)
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // SAFETY: the context was opened by dav1d_open and is closed once.
        unsafe { dav1d_close(NonNull::new(&mut self.context)) };
    }
}

/// One plane of a picture.
struct Plane {
    at: *const u8,
    stride: isize,
    wide: bool,
}

impl Plane {
    /// Row `y`'s first `width` samples, as numbers.
    fn row(&self, y: u32, width: u32, out: &mut Vec<f32>) {
        out.clear();
        // SAFETY: row `y` is inside the plane, `width` samples long, rows
        // `stride` bytes apart; samples are bytes, or 16-bit words above 8 bits.
        unsafe {
            let row = self.at.offset(self.stride * y as isize);
            if self.wide {
                let samples = std::slice::from_raw_parts(row.cast::<u16>(), width as usize);
                out.extend(samples.iter().map(|&s| f32::from(s)));
            } else {
                let samples = std::slice::from_raw_parts(row, width as usize);
                out.extend(samples.iter().map(|&s| f32::from(s)));
            }
        }
    }
}

/// Where an output pixel falls between two samples of a plane halved (or
/// not) along an axis: the two samples and how far from the first, the
/// samples at the centre of the pixels they cover.
fn between(n: u32, halved: u32) -> Vec<(usize, usize, f32)> {
    let samples = (n + halved) >> halved;
    (0..n)
        .map(|i| {
            if halved == 0 {
                return (i as usize, i as usize, 0.0);
            }
            let f = ((i as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, (samples - 1) as f32);
            let first = f as u32;
            (first as usize, (first + 1).min(samples - 1) as usize, f - first as f32)
        })
        .collect()
}

/// A decoded picture as 8-bit RGBA of `series`.
fn convert(picture: &Dav1dPicture, series: Series) -> anyhow::Result<Picture> {
    let (width, height) = (picture.p.w as u32, picture.p.h as u32);
    let bits = picture.p.bpc as u32;
    let wide = bits > 8;
    let plane = |i: usize, stride: isize| -> anyhow::Result<Plane> {
        Ok(Plane { at: picture.data[i].context("a plane is missing")?.as_ptr().cast(), stride, wide })
    };
    let luma = plane(0, picture.stride[0])?;
    let max = ((1u32 << bits) - 1) as f32;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    let mut l = Vec::with_capacity(width as usize);
    if series == Series::Depth {
        // Grey of 12 bits (any depth of bits taken to 12).
        let to_twelve = (1u32 << 12) as f32 / (1u32 << bits) as f32;
        for y in 0..height {
            luma.row(y, width, &mut l);
            for &grey in &l {
                rgba.extend_from_slice(&grey_to_depth((grey * to_twelve) as u16, true));
            }
        }
        return Ok(Picture { width, height, rgba });
    }
    if picture.p.layout == DAV1D_PIXEL_LAYOUT_I400 {
        for y in 0..height {
            luma.row(y, width, &mut l);
            for &v in &l {
                let v = (v / max * 255.0).round() as u8;
                rgba.extend_from_slice(&[v, v, v, 255]);
            }
        }
        return Ok(Picture { width, height, rgba });
    }
    let (cb, cr) = (plane(1, picture.stride[1])?, plane(2, picture.stride[1])?);
    let (sx, sy) = match picture.p.layout {
        DAV1D_PIXEL_LAYOUT_I420 => (1, 1),
        DAV1D_PIXEL_LAYOUT_I422 => (1, 0),
        _ => (0, 0),
    };
    // SAFETY: a decoded picture carries its sequence header.
    let header = unsafe { picture.seq_hdr.context("no sequence header")?.as_ref() };
    let full = header.color_range != 0;
    let scale = (1u32 << (bits - 8)) as f32;
    let (y_low, y_span) = if full { (0.0, max) } else { (16.0 * scale, 219.0 * scale) };
    let (c_mid, c_span) = if full { ((1u32 << (bits - 1)) as f32, max) } else { (128.0 * scale, 224.0 * scale) };
    let identity = header.mtrx == DAV1D_MC_IDENTITY;
    let (kr, kb) = if header.mtrx == DAV1D_MC_BT601 || header.mtrx == DAV1D_MC_BT470BG {
        (0.299, 0.114)
    } else {
        (0.2126, 0.0722)
    };
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    let (columns, rows) = (between(width, sx), between(height, sy));
    let chroma_width = (width + sx) >> sx;
    let (mut b0, mut b1, mut r0, mut r1) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    // The chroma of a row, between its two sample rows.
    let (mut u, mut v) = (vec![0.0f32; chroma_width as usize], vec![0.0f32; chroma_width as usize]);
    for (y, &(first, second, ty)) in rows.iter().enumerate() {
        luma.row(y as u32, width, &mut l);
        cb.row(first as u32, chroma_width, &mut b0);
        cb.row(second as u32, chroma_width, &mut b1);
        cr.row(first as u32, chroma_width, &mut r0);
        cr.row(second as u32, chroma_width, &mut r1);
        for i in 0..chroma_width as usize {
            u[i] = b0[i] + (b1[i] - b0[i]) * ty;
            v[i] = r0[i] + (r1[i] - r0[i]) * ty;
        }
        for (x, &(a, b, tx)) in columns.iter().enumerate() {
            let (pu, pv) = (u[a] + (u[b] - u[a]) * tx, v[a] + (v[b] - v[a]) * tx);
            let yy = (l[x] - y_low) / y_span;
            if identity {
                // Green in luma, blue and red in the chroma planes.
                rgba.extend_from_slice(&[byte((pv - y_low) / y_span), byte(yy), byte((pu - y_low) / y_span), 255]);
                continue;
            }
            let (pb, pr) = ((pu - c_mid) / c_span, (pv - c_mid) / c_span);
            let r = yy + 2.0 * (1.0 - kr) * pr;
            let b = yy + 2.0 * (1.0 - kb) * pb;
            let g = (yy - kr * r - kb * b) / (1.0 - kr - kb);
            rgba.extend_from_slice(&[byte(r), byte(g), byte(b), 255]);
        }
    }
    Ok(Picture { width, height, rgba })
}
