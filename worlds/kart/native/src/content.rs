// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Content from the asset manifest (`assets/manifest.toml`), when it has been
//! fetched (`tools/fetch-assets.py`). The game runs without it, in procedural
//! form; with it, surfaces get real materials.
//!
//! Textures are decoded here and given a full chain of mipmaps, repetition
//! and anisotropic filtering: a road is seen at a grazing angle, where a
//! texture without them shimmers.

use std::path::{Path, PathBuf};

use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    math::Affine2,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use zune_jpeg::JpegDecoder;

/// Where the manifest's files are: `OSAGE_ASSETS` if it is set (`none` to
/// do without), else `assets/` in the current directory, else next to the
/// build (`target/<profile>/../../assets`).
pub fn root() -> Option<PathBuf> {
    let beside_build = || Some(std::env::current_exe().ok()?.parent()?.parent()?.parent()?.join("assets"));
    let places = match std::env::var_os("OSAGE_ASSETS") {
        Some(chosen) => vec![PathBuf::from(chosen)],
        None => [PathBuf::from("assets")].into_iter().chain(beside_build()).collect(),
    };
    places.into_iter().find(|path| path.join("manifest.toml").exists())
}

/// The three maps of a surface: colour, normal, and occlusion-roughness-metal.
pub struct Surface {
    colour: Handle<Image>,
    normal: Handle<Image>,
    arm: Handle<Image>,
}

impl Surface {
    /// Loads Poly Haven texture set `id` at 1k; `grade` adjusts each colour
    /// pixel (sRGB bytes) as it is loaded. `None` if the set is not there.
    pub fn load(root: &Path, id: &str, images: &mut Assets<Image>, grade: impl Fn([u8; 3]) -> [u8; 3]) -> Option<Self> {
        let file = |map: &str| root.join("polyhaven").join(id).join(format!("{id}_{map}_1k.jpg"));
        Some(Self {
            colour: images.add(texture(&file("diff"), Kind::Colour, grade)?),
            normal: images.add(texture(&file("nor_gl"), Kind::Normal, |p| p)?),
            arm: images.add(texture(&file("arm"), Kind::Data, |p| p)?),
        })
    }

    /// A material of this surface, repeated every `tile` metres on meshes
    /// whose texture coordinates are in metres.
    pub fn material(&self, tile: f32) -> StandardMaterial {
        StandardMaterial {
            base_color_texture: Some(self.colour.clone()),
            normal_map_texture: Some(self.normal.clone()),
            // Occlusion in red, roughness in green, metal in blue: one map for both.
            metallic_roughness_texture: Some(self.arm.clone()),
            occlusion_texture: Some(self.arm.clone()),
            perceptual_roughness: 1.0,
            metallic: 0.0,
            uv_transform: Affine2::from_scale(Vec2::splat(1.0 / tile)),
            ..default()
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// sRGB colour: mipmaps are averaged in linear light.
    Colour,
    /// Unit vectors: mipmaps are renormalised.
    Normal,
    /// Linear data.
    Data,
}

/// A JPEG as a repeating texture with its mipmaps.
fn texture(path: &Path, kind: Kind, grade: impl Fn([u8; 3]) -> [u8; 3]) -> Option<Image> {
    let bytes = std::fs::read(path).ok()?;
    let mut decoder = JpegDecoder::new(&bytes);
    let rgb = decoder.decode().inspect_err(|error| warn!("{}: {error}", path.display())).ok()?;
    let (width, height) = decoder.dimensions()?;
    let mut level: Vec<[f32; 3]> = rgb
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| grade(*p).map(|c| if kind == Kind::Colour { to_linear(c) } else { f32::from(c) / 255.0 }))
        .collect();

    let (mut w, mut h) = (width, height);
    let mut data = Vec::with_capacity(width * height * 4 * 4 / 3);
    let mut levels = 0;
    loop {
        for pixel in &level {
            let pixel = pixel.map(|c| if kind == Kind::Colour { to_srgb(c) } else { (c * 255.0 + 0.5) as u8 });
            data.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
        }
        levels += 1;
        if w == 1 && h == 1 {
            break;
        }
        // Halve: each pixel is the mean of the (up to) four it covers.
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = Vec::with_capacity(nw * nh);
        for y in 0..nh {
            for x in 0..nw {
                let mut sum = [0.0; 3];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let source = level[(2 * y + dy).min(h - 1) * w + (2 * x + dx).min(w - 1)];
                    sum = [sum[0] + source[0], sum[1] + source[1], sum[2] + source[2]];
                }
                let mut mean = sum.map(|c| c / 4.0);
                if kind == Kind::Normal {
                    let n = Vec3::from(mean.map(|c| c * 2.0 - 1.0)).normalize_or(Vec3::Z);
                    mean = n.to_array().map(|c| c * 0.5 + 0.5);
                }
                next.push(mean);
            }
        }
        (level, w, h) = (next, nw, nh);
    }

    let format = if kind == Kind::Colour { TextureFormat::Rgba8UnormSrgb } else { TextureFormat::Rgba8Unorm };
    let size = Extent3d { width: width as u32, height: height as u32, depth_or_array_layers: 1 };
    let top = data[..width * height * 4].to_vec();
    let mut image = Image::new(size, TextureDimension::D2, top, format, RenderAssetUsages::RENDER_WORLD);
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = levels;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    });
    Some(image)
}

fn to_linear(c: u8) -> f32 {
    let c = f32::from(c) / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn to_srgb(c: f32) -> u8 {
    let c = if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}
