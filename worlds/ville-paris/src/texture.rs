// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

mod mipmap;

use anyhow::{Context, Result, ensure};
use base64::Engine as _;
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::{Assets, Handle, Image},
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use gltf::json::texture as gl;
use image::{ImageFormat, ImageReader};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Cursor, sync::Arc};

pub(crate) const PAGED_LABEL: &str = "Ville-Paris district base color";

#[derive(Clone, Copy, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(default, rename_all = "camelCase")]
pub struct Sampler {
    mag_filter: u32,
    min_filter: u32,
    wrap_s: u32,
    wrap_t: u32,
}

impl Default for Sampler {
    fn default() -> Self {
        Self { mag_filter: gl::LINEAR, min_filter: gl::LINEAR_MIPMAP_LINEAR, wrap_s: gl::REPEAT, wrap_t: gl::REPEAT }
    }
}

impl Sampler {
    pub fn from_gltf(s: gltf::texture::Sampler) -> Self {
        let default = Self::default();
        Self {
            mag_filter: s.mag_filter().map_or(default.mag_filter, |f| f.as_gl_enum()),
            min_filter: s.min_filter().map_or(default.min_filter, |f| f.as_gl_enum()),
            wrap_s: s.wrap_s().as_gl_enum(),
            wrap_t: s.wrap_t().as_gl_enum(),
        }
    }

    pub fn validate(self) -> Result<()> {
        ensure!(
            gl::VALID_MAG_FILTERS.contains(&self.mag_filter)
                && gl::VALID_MIN_FILTERS.contains(&self.min_filter)
                && [self.wrap_s, self.wrap_t].iter().all(|v| gl::VALID_WRAPPING_MODES.contains(v)),
            "invalid texture sampler"
        );
        Ok(())
    }

    pub(crate) fn descriptor(self) -> ImageSampler {
        let address = |mode| match mode {
            gl::CLAMP_TO_EDGE => ImageAddressMode::ClampToEdge,
            gl::MIRRORED_REPEAT => ImageAddressMode::MirrorRepeat,
            _ => ImageAddressMode::Repeat,
        };
        let filter = |linear| if linear { ImageFilterMode::Linear } else { ImageFilterMode::Nearest };
        ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: address(self.wrap_s),
            address_mode_v: address(self.wrap_t),
            mag_filter: filter(self.mag_filter == gl::LINEAR),
            min_filter: filter(self.linear_minification()),
            mipmap_filter: filter(matches!(self.min_filter, gl::NEAREST_MIPMAP_LINEAR | gl::LINEAR_MIPMAP_LINEAR)),
            ..Default::default()
        })
    }

    fn linear_minification(self) -> bool {
        matches!(self.min_filter, gl::LINEAR | gl::LINEAR_MIPMAP_LINEAR | gl::LINEAR_MIPMAP_NEAREST)
    }

    pub(crate) fn uses_mips(self) -> bool {
        !matches!(self.min_filter, gl::NEAREST | gl::LINEAR)
    }
}

pub struct Pixels {
    format: TextureFormat,
    digest: [u8; 32],
    size: Extent3d,
    levels: u32,
    bytes: Vec<u8>,
    pub min_alpha: u8,
}

impl Pixels {
    pub fn read(encoded: &[u8], mime: &str) -> Result<Arc<Self>> {
        let format = match mime {
            "image/png" => ImageFormat::Png,
            "image/jpeg" => ImageFormat::Jpeg,
            _ => anyhow::bail!("unsupported base colour image {mime}"),
        };
        let rgba = ImageReader::with_format(Cursor::new(encoded), format).decode()?.into_rgba8();
        let (width, height) = rgba.dimensions();
        let min_alpha = rgba.pixels().map(|p| p[3]).min().context("empty texture")?;
        let size = Extent3d { width, height, depth_or_array_layers: 1 };
        let mut bytes = rgba.into_raw();
        let levels = mipmap::append(&mut bytes, width, height);
        Ok(Arc::new(Self {
            format: TextureFormat::Rgba8UnormSrgb,
            digest: Sha256::digest(encoded).into(),
            size,
            levels,
            bytes,
            min_alpha,
        }))
    }

    pub(crate) fn read_resource(bytes: &[u8], mime: &str, bc7_limit: u32) -> Result<Arc<Self>> {
        if mime != "image/ktx2" {
            return Self::read(bytes, mime);
        }
        let image = crate::compressed_texture::bc7_image(bytes, bc7_limit, Sampler::default())?;
        Ok(Arc::new(Self {
            format: image.texture_descriptor.format,
            digest: Sha256::digest(bytes).into(),
            size: image.texture_descriptor.size,
            levels: image.texture_descriptor.mip_level_count,
            bytes: image.data.context("missing transcoded blocks")?,
            // A masked surface can only certify occlusion when every texel is known opaque.
            min_alpha: 0,
        }))
    }

    pub fn byte_len(&self) -> usize {
        self.bytes.len()
    }
}

#[derive(Clone)]
pub struct Texture {
    pub pixels: Arc<Pixels>,
    pub sampler: Sampler,
}
pub type TextureKey = ([u8; 32], Sampler, Option<u8>);
pub type Images = BTreeMap<TextureKey, Handle<Image>>;

impl Texture {
    pub fn embedded(info: gltf::texture::Info, bytes: &[u8], cache: &mut BTreeMap<usize, Arc<Pixels>>) -> Result<Self> {
        ensure!(info.tex_coord() == 0, "base colour image requires TEXCOORD_0");
        let texture = info.texture();
        let image = texture.source();
        let entry = cache.entry(image.index());
        let pixels = match entry {
            std::collections::btree_map::Entry::Occupied(o) => o.into_mut(),
            std::collections::btree_map::Entry::Vacant(v) => {
                let pixels = match image.source() {
                    gltf::image::Source::View { view, mime_type } => {
                        ensure!(view.stride().is_none(), "unexpected image view stride");
                        Pixels::read(&bytes[view.offset()..view.offset() + view.length()], mime_type)?
                    }
                    gltf::image::Source::Uri { uri, mime_type } => {
                        let (header, base64) = uri
                            .strip_prefix("data:")
                            .and_then(|v| v.split_once(','))
                            .context("expected embedded image or district resource")?;
                        let mime = header.strip_suffix(";base64").context("expected base64 image data")?;
                        ensure!(mime_type.is_none_or(|m| m == mime), "conflicting image type");
                        let raw = base64::engine::general_purpose::STANDARD.decode(base64)?;
                        Pixels::read(&raw, mime)?
                    }
                };
                v.insert(pixels)
            }
        };
        Ok(Self { pixels: pixels.clone(), sampler: Sampler::from_gltf(texture.sampler()) })
    }

    pub fn key(&self, cutoff: Option<u8>) -> TextureKey {
        (
            self.pixels.digest,
            self.sampler,
            cutoff.filter(|_| self.sampler.uses_mips() && self.pixels.format == TextureFormat::Rgba8UnormSrgb),
        )
    }

    pub(crate) fn upload_image(&self, cutoff: Option<u8>) -> Image {
        let p = &self.pixels;
        let mut img = Image::new_uninit(p.size, TextureDimension::D2, p.format, RenderAssetUsages::RENDER_WORLD);
        if p.format == TextureFormat::Bc7RgbaUnormSrgb {
            let levels = if self.sampler.uses_mips() { p.levels } else { 1 };
            let count =
                crate::compressed_texture::payload_bytes(p.format, p.size.width, p.size.height, levels) as usize;
            img.texture_descriptor.mip_level_count = levels;
            img.data = Some(p.bytes[..count].to_vec());
            img.sampler = self.sampler.descriptor();
            return img;
        }
        let base = p.size.width as usize * p.size.height as usize;
        let mut bytes = if self.sampler.uses_mips() {
            img.texture_descriptor.mip_level_count = p.levels;
            p.bytes.clone()
        } else {
            p.bytes[..base * 4].to_vec()
        };
        if let Some(cut) = cutoff
            && p.min_alpha < cut
            && bytes.len() > base * 4
        {
            let (mut w, mut h) = (p.size.width as usize, p.size.height as usize);
            let reference = alpha_histogram(&bytes[..base * 4], w, h, self.sampler);
            let passed: u64 = reference[usize::from(cut)..].iter().sum();
            let mut next = base * 4;
            while w > 1 || h > 1 {
                w = (w / 2).max(1);
                h = (h / 2).max(1);
                let end = next + w * h * 4;
                let histogram = alpha_histogram(&bytes[next..end], w, h, self.sampler);
                match_alpha_fraction(
                    &mut bytes[next..end],
                    &histogram,
                    cut,
                    passed,
                    base as u64,
                    self.sampler.linear_minification(),
                );
                next = end;
            }
        }
        img.data = Some(bytes);
        img.sampler = self.sampler.descriptor();
        img
    }

    pub fn handle(&self, cutoff: Option<u8>, cache: &mut Images, images: &mut Assets<Image>) -> Handle<Image> {
        cache
            .entry(self.key(cutoff))
            .or_insert_with(|| {
                let mut image = self.upload_image(cutoff);
                image.texture_descriptor.label = Some(PAGED_LABEL);
                images.add(image)
            })
            .clone()
    }

    pub fn levels(&self, cutoff: Option<u8>) -> Vec<serde_json::Value> {
        let img = self.upload_image(cutoff);
        let adjusted = img.data.as_ref().expect("constructed image payload");
        let mut offset = 0;
        let mut rows = Vec::new();
        for mip in 0..img.texture_descriptor.mip_level_count {
            let width = (self.pixels.size.width >> mip).max(1);
            let height = (self.pixels.size.height >> mip).max(1);
            if self.pixels.format == TextureFormat::Bc7RgbaUnormSrgb {
                let bytes = crate::compressed_texture::payload_bytes(self.pixels.format, width, height, 1) as usize;
                rows.push(serde_json::json!({"width":width,"height":height,"format":"Bc7RgbaUnormSrgb",
                    "bytes":bytes,"sha256":crate::bench::sha(&adjusted[offset..offset+bytes])}));
                offset += bytes;
                continue;
            }
            let end = offset + width as usize * height as usize * 4;
            let corrected = &adjusted[offset..end];
            let standard = &self.pixels.bytes[offset..end];
            let alpha: Vec<_> = corrected.iter().skip(3).step_by(4).copied().collect();
            let passing_samples = |rgba: &[u8]| {
                cutoff.map(|cut| {
                    let histogram = alpha_histogram(rgba, width as usize, height as usize, self.sampler);
                    histogram[usize::from(cut)..].iter().sum::<u64>()
                })
            };
            rows.push(serde_json::json!({
                "width":width, "height":height,
                "samplesPerTexel":if self.sampler.linear_minification() { 4 } else { 1 },
                "passingSamples":passing_samples(corrected), "unadjustedPassingSamples":passing_samples(standard),
                "alphaSha256":crate::bench::sha(&alpha), "rgbaSha256":crate::bench::sha(corrected),
                "passing":cutoff.map(|cut|alpha_count(corrected,cut)),
                "unadjustedPassing":cutoff.map(|cut|alpha_count(standard,cut))
            }));
            offset = end;
        }
        rows
    }
}

fn alpha_count(rgba: &[u8], cut: u8) -> usize {
    rgba.iter().skip(3).step_by(4).filter(|&&a| a >= cut).count()
}

// Four cell midpoints per texel for bilinear minification. Edge and mirror sampling both
// extend the edge texel across the half texel at each end of the normalized UV domain.
fn alpha_histogram(rgba: &[u8], width: usize, height: usize, sampler: Sampler) -> [u64; 256] {
    let mut histogram = [0; 256];
    if !sampler.linear_minification() {
        for &alpha in rgba.iter().skip(3).step_by(4) {
            histogram[usize::from(alpha)] += 1;
        }
        return histogram;
    }
    let neighbours = |i: usize, size: usize, wrap: u32| {
        let before = if i == 0 && wrap == gl::REPEAT { size - 1 } else { i.saturating_sub(1) };
        let after = if i + 1 < size {
            i + 1
        } else if wrap == gl::REPEAT {
            0
        } else {
            i
        };
        [before, after]
    };
    let alpha = |x: usize, y: usize| usize::from(rgba[4 * (y * width + x) + 3]);
    for y in 0..height {
        let rows = neighbours(y, height, sampler.wrap_t);
        for x in 0..width {
            let columns = neighbours(x, width, sampler.wrap_s);
            let center = 9 * alpha(x, y);
            for row in rows {
                let vertical = center + 3 * alpha(x, row);
                for column in columns {
                    let value = (vertical + 3 * alpha(column, y) + alpha(column, row) + 8) / 16;
                    histogram[value] += 1;
                }
            }
        }
    }
    histogram
}

// Both levels have the same number of samples per texel, so their area ratio uses
// the number of texels. Prefer the original threshold on a tie.
fn match_alpha_fraction(
    rgba: &mut [u8],
    histogram: &[u64; 256],
    cut: u8,
    base_pass: u64,
    base_texels: u64,
    protect_bilinear: bool,
) {
    let target = u128::from(base_pass) * (rgba.len() / 4) as u128;
    let mut above = 0;
    let mut best = (u128::MAX, u8::MAX);
    let mut pivot = cut;
    for threshold in (1..=255u8).rev() {
        above += histogram[threshold as usize];
        let rank = ((u128::from(above) * u128::from(base_texels)).abs_diff(target), threshold.abs_diff(cut));
        if rank < best {
            best = rank;
            pivot = threshold;
        }
    }
    // A sampled cell with extra opacity can also contain thin edges that vanish under projection.
    // With bilinear minification only restore lost opacity, leaving such cells at their original values.
    if pivot == cut || protect_bilinear && pivot > cut {
        return;
    }
    let pivot = u32::from(pivot);
    let threshold = u32::from(cut);
    let remap = std::array::from_fn::<u8, 256, _>(|alpha| {
        let alpha = alpha as u32;
        if alpha == 0 || alpha == 255 {
            alpha as u8
        } else if alpha < pivot {
            let lower = (alpha * threshold + pivot / 2) / pivot;
            lower.min(threshold - 1) as u8
        } else {
            let span = 255 - pivot;
            (threshold + ((alpha - pivot) * (255 - threshold) + span / 2) / span) as u8
        }
    });
    for alpha in rgba.iter_mut().skip(3).step_by(4) {
        *alpha = remap[*alpha as usize];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(alpha: &[u8]) -> Vec<u8> {
        alpha.iter().flat_map(|&a| [17, 29, 53, a]).collect()
    }

    #[test]
    fn cutout_preserves_endpoints_rgb_and_closest_reachable_count() {
        for cutoff in [1u8, 2, 127, 128, 254, 255] {
            for desired in 0..=13 {
                let mut rgba = level(&[0, 1, 13, 13, 64, 85, 126, 127, 127, 128, 200, 200, 255]);
                let original = rgba.clone();
                let sampler = Sampler { min_filter: gl::NEAREST_MIPMAP_NEAREST, ..Sampler::default() };
                let histogram = alpha_histogram(&rgba, 13, 1, sampler);
                match_alpha_fraction(&mut rgba, &histogram, cutoff, desired as u64, 13, false);
                let optimum = (1..=255u8).map(|cut| alpha_count(&original, cut).abs_diff(desired)).min().unwrap();
                assert_eq!(alpha_count(&rgba, cutoff).abs_diff(desired), optimum);
                let a: Vec<_> = rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|p| {
                        assert_eq!(p[..3], [17, 29, 53]);
                        p[3]
                    })
                    .collect();
                assert_eq!((a[0], a[12]), (0, 255));
                assert!(a.is_sorted());
                if alpha_count(&original, cutoff) == desired {
                    assert_eq!(rgba, original);
                }
            }
        }
    }

    #[test]
    fn bilinear_samples_follow_each_uv_edge_and_preserve_constant_alpha() {
        // A 2-by-1 grid has a single horizontal transition and no vertical variation.
        let rgba = level(&[0, 255]);
        let wrapped = alpha_histogram(&rgba, 2, 1, Sampler::default());
        assert_eq!((wrapped[64], wrapped[191], wrapped.iter().sum::<u64>()), (4, 4, 8));
        for mode in [gl::CLAMP_TO_EDGE, gl::MIRRORED_REPEAT] {
            let sampler = Sampler { wrap_s: mode, ..Sampler::default() };
            let h = alpha_histogram(&rgba, 2, 1, sampler);
            assert_eq!((h[0], h[64], h[191], h[255], h.iter().sum::<u64>()), (2, 2, 2, 2, 8));
            assert_eq!(alpha_histogram(&level(&[173]), 1, 1, sampler)[173], 4);
        }
    }

    #[test]
    fn bilinear_correction_never_thins_blended_edges() {
        let src = level(&[0, 32, 64, 84, 105, 128, 170, 240]);
        let histogram = alpha_histogram(&src, 4, 2, Sampler::default());
        let mut extra = src.clone();
        match_alpha_fraction(&mut extra, &histogram, 64, 3, 8, true);
        assert_eq!(extra, src);
        let mut missing = src.clone();
        match_alpha_fraction(&mut missing, &histogram, 128, 24, 8, true);
        assert!(missing.iter().zip(&src).all(|(a, b)| a >= b));
        assert_ne!(missing, src);
    }

    #[test]
    fn authored_top_stays_identical_and_unfiltered_materials_upload_only_that_level() {
        let base = level(&[255, 0, 0, 0, 0, 0, 0, 0]);
        let mut raw = base.clone();
        raw.extend(level(&[80, 0, 40]));
        let texture = Texture {
            pixels: Arc::new(Pixels {
                format: TextureFormat::Rgba8UnormSrgb,
                digest: [0; 32],
                size: Extent3d { width: 4, height: 2, depth_or_array_layers: 1 },
                levels: 3,
                bytes: raw.clone(),
                min_alpha: 0,
            }),
            sampler: Sampler::default(),
        };
        let mask = texture.upload_image(Some(128));
        assert_eq!(&mask.data.as_ref().unwrap()[..base.len()], &base);
        assert_eq!(texture.upload_image(None).data, Some(raw));
        let single = Texture { sampler: Sampler { min_filter: gl::NEAREST, ..Sampler::default() }, ..texture };
        let image = single.upload_image(Some(128));
        assert_eq!(image.texture_descriptor.mip_level_count, 1);
        assert_eq!(image.data, Some(base));
        assert_eq!(single.key(Some(128)).2, None);
    }
}

#[cfg(test)]
mod verification;
