// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Content from the asset manifest (`assets/manifest.toml`), when it has been
//! fetched (`tools/fetch-assets.py`). The game runs without it, in procedural
//! form; with it, surfaces get real materials.
//!
//! Textures are decoded here and given a full chain of mipmaps, repetition
//! and anisotropic filtering: a road is seen at a grazing angle, where a
//! texture without them shimmers.

pub mod panorama;

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
/// build (`target/<profile>/../../assets`); as an absolute path.
pub fn root() -> Option<PathBuf> {
    let beside_build = || Some(std::env::current_exe().ok()?.parent()?.parent()?.parent()?.join("assets"));
    let places = match std::env::var_os("OSAGE_ASSETS") {
        Some(chosen) => vec![PathBuf::from(chosen)],
        None => [PathBuf::from("assets")].into_iter().chain(beside_build()).collect(),
    };
    // Absolute: an asset server resolves a relative root from the program's folder.
    places.into_iter().find(|path| path.join("manifest.toml").exists()).and_then(|path| path.canonicalize().ok())
}

/// The three maps of a surface: colour, normal, and occlusion-roughness-metal.
#[derive(Clone)]
pub struct Surface {
    colour: Handle<Image>,
    normal: Handle<Image>,
    arm: Handle<Image>,
}

/// Resolutions a Poly Haven texture set is looked for in, best first.
const RESOLUTIONS: [&str; 2] = ["2k", "1k"];

/// The file of map `map` of Poly Haven texture set `id`, at the best
/// resolution there is.
fn map_file(root: &Path, id: &str, map: &str) -> PathBuf {
    let at = |resolution: &str| root.join("polyhaven").join(id).join(format!("{id}_{map}_{resolution}.jpg"));
    RESOLUTIONS.iter().map(|r| at(r)).find(|path| path.exists()).unwrap_or_else(|| at(RESOLUTIONS[0]))
}

impl Surface {
    /// Loads Poly Haven texture set `id` at the best resolution there is;
    /// `grade` adjusts each colour pixel (sRGB bytes) as it is loaded.
    /// `None` if the set is not there.
    pub fn load(root: &Path, id: &str, images: &mut Assets<Image>, grade: impl Fn([u8; 3]) -> [u8; 3]) -> Option<Self> {
        let file = |map: &str| map_file(root, id, map);
        Some(Self {
            colour: images.add(texture(&file("diff"), Kind::Colour, grade)?),
            normal: images.add(texture(&file("nor_gl"), Kind::Normal, |p| p)?),
            arm: images.add(texture(&file("arm"), Kind::Data, |p| p)?),
        })
    }

    /// Loads Poly Haven texture set `id` as a grain: its light and dark
    /// without its colour, so that whatever is painted under it (a
    /// material's colour, a mesh's vertex colours) shows through with the
    /// surface's texture.
    pub fn load_grain(root: &Path, id: &str, images: &mut Assets<Image>) -> Option<Self> {
        let file = |map: &str| map_file(root, id, map);
        Some(Self {
            colour: images.add(texture(&file("diff"), Kind::Grain, |p| p)?),
            normal: images.add(texture(&file("nor_gl"), Kind::Normal, |p| p)?),
            arm: images.add(texture(&file("arm"), Kind::Data, |p| p)?),
        })
    }

    /// Its colour map (or grain).
    pub fn colour(&self) -> Handle<Image> {
        self.colour.clone()
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

/// A grain is stored at this mean, in linear light: what it covers is
/// painted `1 / GRAIN` times brighter to come out as meant.
pub const GRAIN: f32 = 0.5;

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    /// sRGB colour: mipmaps are averaged in linear light.
    Colour,
    /// sRGB colour reduced to its brightness, around a mean of [`GRAIN`].
    Grain,
    /// Unit vectors: mipmaps are renormalised.
    Normal,
    /// Linear data.
    Data,
}

/// A JPEG as a repeating texture with its mipmaps; `grade` adjusts each
/// pixel (sRGB bytes) first.
fn texture(path: &Path, kind: Kind, grade: impl Fn([u8; 3]) -> [u8; 3]) -> Option<Image> {
    let bytes = std::fs::read(path).ok()?;
    let mut decoder = JpegDecoder::new(&bytes);
    let rgb = decoder.decode().inspect_err(|error| warn!("{}: {error}", path.display())).ok()?;
    let (width, height) = decoder.dimensions()?;
    let coloured = matches!(kind, Kind::Colour | Kind::Grain);
    let mut pixels: Vec<[f32; 3]> = rgb
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| grade(*p).map(|c| if coloured { to_linear(c) } else { f32::from(c) / 255.0 }))
        .collect();
    if kind == Kind::Grain {
        let brightness = |p: &[f32; 3]| 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2];
        let mean = pixels.iter().map(brightness).sum::<f32>() / pixels.len().max(1) as f32;
        for pixel in &mut pixels {
            // Half of its contrast is kept: a texture, not a pattern.
            *pixel = [(GRAIN * (0.5 + 0.5 * brightness(pixel) / mean.max(1e-4))).min(1.0); 3];
        }
    }
    Some(tiled(pixels, width, height, kind))
}

/// A repeating texture with its mipmaps, from `width` by `height` pixels in
/// linear values.
pub fn tiled(mut level: Vec<[f32; 3]>, width: usize, height: usize, kind: Kind) -> Image {
    let coloured = matches!(kind, Kind::Colour | Kind::Grain);
    let (mut w, mut h) = (width, height);
    let mut data = Vec::with_capacity(width * height * 4 * 4 / 3);
    let mut levels = 0;
    loop {
        for pixel in &level {
            let pixel = pixel.map(|c| if coloured { to_srgb(c) } else { (c * 255.0 + 0.5) as u8 });
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

    let format = if coloured { TextureFormat::Rgba8UnormSrgb } else { TextureFormat::Rgba8Unorm };
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
    image
}

fn to_linear(c: u8) -> f32 {
    let c = f32::from(c) / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn to_srgb(c: f32) -> u8 {
    let c = if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn levels(image: &Image) -> Vec<&[u8]> {
        let size = image.texture_descriptor.size;
        let (mut w, mut h, mut at) = (size.width as usize, size.height as usize, 0);
        let data = image.data.as_ref().unwrap();
        let mut levels = Vec::new();
        for _ in 0..image.texture_descriptor.mip_level_count {
            levels.push(&data[at..at + w * h * 4]);
            at += w * h * 4;
            (w, h) = ((w / 2).max(1), (h / 2).max(1));
        }
        assert_eq!(at, data.len(), "the levels fill the data exactly");
        levels
    }

    #[test]
    fn a_texture_has_every_level_down_to_one_pixel() {
        let image = tiled(vec![[0.5; 3]; 8 * 2], 8, 2, Kind::Data);
        assert_eq!(image.texture_descriptor.mip_level_count, 4, "8×2, 4×1, 2×1, 1×1");
        assert_eq!(levels(&image).last().unwrap().len(), 4);
    }

    #[test]
    fn colours_are_averaged_in_linear_light() {
        // Black and white: their mean is half the light, which sRGB stores near 188, not 128.
        let image = tiled(vec![[0.0; 3], [1.0; 3], [1.0; 3], [0.0; 3]], 2, 2, Kind::Colour);
        let mean = levels(&image)[1][0];
        assert!((186..=189).contains(&mean), "{mean}");
        assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba8UnormSrgb);
    }

    #[test]
    fn normals_stay_unit_vectors_down_the_levels() {
        // Two normals leaning opposite ways average to straight up, not to a short vector.
        let lean = |x: f32| [x * 0.5 + 0.5, 0.5, (1.0 - x * x).sqrt() * 0.5 + 0.5];
        let image = tiled(vec![lean(0.8), lean(-0.8), lean(0.8), lean(-0.8)], 2, 2, Kind::Normal);
        let n = levels(&image)[1];
        let v = Vec3::new(n[0] as f32, n[1] as f32, n[2] as f32) / 255.0 * 2.0 - 1.0;
        assert!((v.length() - 1.0).abs() < 0.02, "length {}", v.length());
        assert!(v.z > 0.99, "straight up: {v}");
    }

    #[test]
    fn srgb_bytes_survive_the_trip_through_linear_light() {
        for byte in 0..=255u8 {
            assert_eq!(to_srgb(to_linear(byte)), byte);
        }
    }

    #[test]
    fn the_best_resolution_there_is_is_chosen() {
        let root = std::env::temp_dir().join(format!("osage-content-test-{}", std::process::id()));
        let set = root.join("polyhaven").join("road");
        std::fs::create_dir_all(&set).unwrap();
        std::fs::write(set.join("road_diff_1k.jpg"), b"").unwrap();
        assert!(map_file(&root, "road", "diff").ends_with("road_diff_1k.jpg"));
        std::fs::write(set.join("road_diff_2k.jpg"), b"").unwrap();
        assert!(map_file(&root, "road", "diff").ends_with("road_diff_2k.jpg"));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
