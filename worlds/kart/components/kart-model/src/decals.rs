// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sponsors' stickers: an atlas of abstract marks — a panel, a disc or a
//! ring, a chevron, bars standing for a name — in sponsors' colours, drawn
//! by the program (no real brand), and the quads that lay them on a kart.
//! A racing kart is covered in stickers; plain paint reads as a toy.

use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

/// Marks across and down the atlas; each cell twice as wide as it is high.
pub const COLUMNS: usize = 4;
pub const ROWS: usize = 8;
const CELL: (usize, usize) = (128, 64);
/// Samples across a pixel, each way: soft edges.
const SAMPLES: usize = 4;

/// Sponsors' colour pairs, sRGB.
const PAIRS: [([f32; 3], [f32; 3]); 8] = [
    ([0.85, 0.1, 0.08], [0.97, 0.97, 0.95]),
    ([0.98, 0.8, 0.05], [0.06, 0.06, 0.07]),
    ([0.08, 0.25, 0.7], [0.97, 0.97, 0.95]),
    ([0.06, 0.06, 0.07], [0.98, 0.5, 0.05]),
    ([0.97, 0.97, 0.95], [0.85, 0.1, 0.08]),
    ([0.05, 0.5, 0.25], [0.97, 0.97, 0.95]),
    ([0.06, 0.06, 0.07], [0.98, 0.8, 0.05]),
    ([0.97, 0.97, 0.95], [0.08, 0.25, 0.7]),
];

fn rounded_rect(p: Vec2, low: Vec2, high: Vec2, radius: f32) -> f32 {
    let centre = (low + high) / 2.0;
    let half = (high - low) / 2.0 - Vec2::splat(radius);
    let q = (p - centre).abs() - half;
    q.max(Vec2::ZERO).length() + q.max_element().min(0.0) - radius
}

/// The colour of mark `k` at `p` (across 0..2, down 0..1), or nothing.
fn mark(k: usize, p: Vec2) -> Option<[f32; 3]> {
    let (a, b) = PAIRS[(k * 3 + k / 8) % PAIRS.len()];
    let style = (k + k / COLUMNS) % 4;
    // Bars standing for a name, right of the emblem: lengths from the mark's number.
    let bar = |p: Vec2, rows: &[f32]| {
        rows.iter().enumerate().any(|(i, &y)| {
            let length = 0.55 + 0.4 * (((k * 7 + i * 3) % 5) as f32 / 4.0);
            rounded_rect(p, Vec2::new(0.85, y - 0.055), Vec2::new(0.85 + length, y + 0.055), 0.03) <= 0.0
        })
    };
    let emblem = Vec2::new(0.45, 0.5);
    let panel = rounded_rect(p, Vec2::new(0.06, 0.08), Vec2::new(1.94, 0.92), 0.12) <= 0.0;
    match style {
        // A panel, a disc and a name in the second colour.
        0 => {
            if (p.distance(emblem) <= 0.26 || bar(p, &[0.38, 0.62])) && panel {
                Some(b)
            } else {
                panel.then_some(a)
            }
        }
        // A ring and a name, no panel.
        1 => {
            let d = p.distance(emblem);
            ((0.17..=0.29).contains(&d) || bar(p, &[0.5])).then_some(a)
        }
        // A panel crossed by a stripe, a name.
        2 => {
            let stripe = (p.x - 0.6 - (p.y - 0.5) * 0.6).abs() <= 0.1;
            if panel && (stripe || bar(p, &[0.32, 0.5, 0.68])) { Some(b) } else { panel.then_some(a) }
        }
        // A chevron and a name on a dark panel.
        _ => {
            let q = p - emblem;
            let chevron = (q.x + q.y.abs() * 0.9).abs() <= 0.09 && q.y.abs() <= 0.3;
            if panel && (chevron || bar(p, &[0.4, 0.6])) { Some(a) } else { panel.then_some(b) }
        }
    }
}

/// The atlas of sponsors' marks, `COLUMNS` by `ROWS`, transparent around
/// the marks, mipmapped.
pub fn atlas() -> Image {
    let (width, height) = (COLUMNS * CELL.0, ROWS * CELL.1);
    let mut pixels = vec![Vec4::ZERO; width * height];
    for (i, pixel) in pixels.iter_mut().enumerate() {
        let (x, y) = (i % width, i / width);
        let k = (y / CELL.1) * COLUMNS + x / CELL.0;
        let mut sum = Vec4::ZERO;
        for sy in 0..SAMPLES {
            for sx in 0..SAMPLES {
                let p = Vec2::new(
                    ((x % CELL.0) as f32 + (sx as f32 + 0.5) / SAMPLES as f32) / CELL.1 as f32,
                    ((y % CELL.1) as f32 + (sy as f32 + 0.5) / SAMPLES as f32) / CELL.1 as f32,
                );
                if let Some(colour) = mark(k, p) {
                    let linear = Color::srgb(colour[0], colour[1], colour[2]).to_linear();
                    sum += Vec4::new(linear.red, linear.green, linear.blue, 1.0);
                }
            }
        }
        let n = (SAMPLES * SAMPLES) as f32;
        *pixel = if sum.w > 0.0 { (sum.truncate() / sum.w).extend(sum.w / n) } else { Vec4::new(0.5, 0.5, 0.5, 0.0) };
    }
    // Mipmaps: colour weighted by coverage, coverage averaged.
    let (mut w, mut h, mut levels) = (width, height, 0);
    let mut level = pixels;
    let mut data = Vec::new();
    loop {
        for p in &level {
            data.extend_from_slice(&Color::linear_rgba(p.x, p.y, p.z, p.w).to_srgba().to_u8_array());
        }
        levels += 1;
        if w == 1 && h == 1 {
            break;
        }
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = Vec::with_capacity(nw * nh);
        for y in 0..nh {
            for x in 0..nw {
                let mut sum = Vec4::ZERO;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let p = level[(2 * y + dy).min(h - 1) * w + (2 * x + dx).min(w - 1)];
                    sum += (p.truncate() * p.w).extend(p.w);
                }
                next.push(if sum.w > 0.0 { (sum.truncate() / sum.w).extend(sum.w / 4.0) } else { Vec4::ZERO });
            }
        }
        (level, w, h) = (next, nw, nh);
    }
    let size = Extent3d { width: width as u32, height: height as u32, depth_or_array_layers: 1 };
    let top = data[..width * height * 4].to_vec();
    let mut image =
        Image::new(size, TextureDimension::D2, top, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD);
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = levels;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    });
    image
}

/// Stickers being laid: quads with their place in the atlas.
#[derive(Default)]
pub struct Decals {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl Decals {
    /// Mark `k`, `width` long (its height half that), centred on `centre`,
    /// its length along `right` and its height along `up`, facing
    /// `right × up`.
    pub fn add(&mut self, k: usize, centre: Vec3, right: Vec3, up: Vec3, width: f32) {
        let (right, up) = (right.normalize() * width / 2.0, up.normalize() * width / 4.0);
        let normal = right.cross(up).normalize();
        let k = k % (COLUMNS * ROWS);
        let (u0, v0) = ((k % COLUMNS) as f32 / COLUMNS as f32, (k / COLUMNS) as f32 / ROWS as f32);
        let (u1, v1) = (u0 + 1.0 / COLUMNS as f32, v0 + 1.0 / ROWS as f32);
        let first = self.positions.len() as u32;
        for (corner, uv) in [
            (centre - right - up, [u0, v1]),
            (centre + right - up, [u1, v1]),
            (centre + right + up, [u1, v0]),
            (centre - right + up, [u0, v0]),
        ] {
            self.positions.push(corner.to_array());
            self.normals.push(normal.to_array());
            self.uvs.push(uv);
        }
        self.indices.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
    }

    pub fn mesh(self) -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
            .with_inserted_indices(Indices::U32(self.indices))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mark_is_drawn_and_leaves_room_around_it() {
        let image = atlas();
        let data = image.data.as_ref().unwrap();
        let width = COLUMNS * CELL.0;
        for k in 0..COLUMNS * ROWS {
            let (x0, y0) = ((k % COLUMNS) * CELL.0, (k / COLUMNS) * CELL.1);
            let alpha = |x: usize, y: usize| data[((y0 + y) * width + x0 + x) * 4 + 3];
            let covered =
                (0..CELL.1).flat_map(|y| (0..CELL.0).map(move |x| (x, y))).filter(|&(x, y)| alpha(x, y) > 128).count();
            let share = covered as f32 / (CELL.0 * CELL.1) as f32;
            assert!((0.05..0.97).contains(&share), "mark {k} covers {share}");
            // The corners stay clear: a sticker is never a plain rectangle.
            assert!(alpha(0, 0) < 64 && alpha(CELL.0 - 1, CELL.1 - 1) < 64, "mark {k}'s corners");
        }
        assert!(image.texture_descriptor.mip_level_count > 6);
    }

    #[test]
    fn a_decal_faces_the_way_asked() {
        let mut decals = Decals::default();
        // Read from +X, a mark's length runs towards -Z: -Z × Y is +X.
        decals.add(5, Vec3::new(0.6, 0.12, 0.0), -Vec3::Z, Vec3::Y, 0.2);
        let mesh = decals.mesh();
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(normals)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL) else {
            panic!()
        };
        assert!(normals.iter().all(|n| (Vec3::from(*n) - Vec3::X).length() < 1e-5), "faces +X");
    }
}
