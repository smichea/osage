// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Woods: trees far off drawn as crossed cards, each card a picture of the
//! real model seen from its side. A modelled tree costs hundreds of
//! thousands of triangles near and thousands far; seen from more than a
//! hundred metres it covers a few dozen pixels, and a picture of it does as
//! well. At load, each variant of the model is drawn on the processor from
//! [`VIEWS`] directions around the vertical — its own textures, its own
//! leaves, darker inside the crown — into an atlas; each place then gets
//! [`VIEWS`] cards crossing on its trunk, each showing the view from its
//! side, all merged into one mesh: a forest of thousands of trees in a few
//! draws.
//!
//! For trees that are always far from the camera (woods off the circuit):
//! the cards never turn into models, so up close they show what they are.

use bevy::{
    asset::RenderAssetUsages,
    gltf::{Gltf, GltfMaterial, GltfMesh, GltfNode},
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    pbr::gltf::standard_material_from_gltf_material,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use crate::{Use, pieces};

/// Views of each tree, evenly spread around the vertical; one card each.
pub const VIEWS: usize = 3;
/// Side of one view in the atlas, pixels.
const SIDE: usize = 256;
/// Views are drawn this many times finer, then averaged: soft edges.
const FINER: usize = 2;

/// A model standing at many places, always far from the camera, drawn as
/// crossed cards.
#[derive(Component, Clone)]
pub struct Woods {
    pub model: Handle<Gltf>,
    pub using: Use,
    pub places: Vec<Transform>,
}

/// Woods whose cards are made.
#[derive(Component)]
pub(crate) struct Planted;

/// A triangle of the model, in the model's space, with what colours it.
struct Triangle {
    corners: [Vec3; 3],
    uvs: [Vec2; 3],
    material: usize,
}

/// What colours a triangle: a factor, and a texture if the processor can read it.
struct Colouring {
    factor: Vec4,
    texture: Option<(Vec<u8>, usize, usize)>,
    cutoff: f32,
}

impl Colouring {
    /// The colour at `uv`, linear, with its alpha.
    fn at(&self, uv: Vec2) -> Vec4 {
        let Some((data, width, height)) = &self.texture else { return self.factor };
        let x = ((uv.x.rem_euclid(1.0) * *width as f32) as usize).min(width - 1);
        let y = ((uv.y.rem_euclid(1.0) * *height as f32) as usize).min(height - 1);
        let i = (y * width + x) * 4;
        let texel = Color::srgba_u8(data[i], data[i + 1], data[i + 2], data[i + 3]).to_linear();
        Vec4::new(texel.red, texel.green, texel.blue, texel.alpha) * self.factor
    }
}

/// One view of a model: its picture (linear colour and coverage, `SIDE`²,
/// rows from the top), and the card that shows it: its width and the
/// offset of its centre along the view's right, its foot and height.
#[derive(Clone, Debug)]
pub struct View {
    pub pixels: Vec<Vec4>,
    pub right: f32,
    pub foot: f32,
    pub size: f32,
}

/// The direction a view looks from, and its right, for view `k`.
pub fn axes(k: usize) -> (Vec3, Vec3) {
    let angle = k as f32 * std::f32::consts::PI / VIEWS as f32;
    let from = Vec3::new(angle.sin(), 0.0, angle.cos());
    let right = Vec3::new(angle.cos(), 0.0, -angle.sin());
    (from, right)
}

/// Draws view `k` of the triangles: an orthographic picture, square,
/// fitted to the model's height or width, its foot at the bottom; deeper
/// pixels darker, as the inside of a crown is.
fn draw(triangles: &[Triangle], colourings: &[Colouring], k: usize) -> View {
    let (from, right) = axes(k);
    let points = triangles.iter().flat_map(|t| t.corners);
    let (low, high) = points.fold((Vec3::MAX, Vec3::MIN), |(low, high), p| {
        let q = Vec3::new(p.dot(right), p.y, p.dot(from));
        (low.min(q), high.max(q))
    });
    let size = (high.x - low.x).max(high.y - low.y).max(1e-3);
    let centre = (low.x + high.x) / 2.0;
    let fine = SIDE * FINER;
    let scale = fine as f32 / size;
    let mut colour = vec![Vec4::ZERO; fine * fine];
    let mut depth = vec![f32::MIN; fine * fine];
    for triangle in triangles {
        let colouring = &colourings[triangle.material];
        // Screen positions: x right, y down from the top; depth towards the viewer.
        let screen = triangle.corners.map(|p| {
            Vec3::new((p.dot(right) - centre) * scale + fine as f32 / 2.0, (low.y + size - p.y) * scale, p.dot(from))
        });
        let (a, b, c) = (screen[0], screen[1], screen[2]);
        let area = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
        if area.abs() < 1e-9 {
            continue;
        }
        let x0 = a.x.min(b.x).min(c.x).floor().max(0.0) as usize;
        let x1 = (a.x.max(b.x).max(c.x).ceil() as usize).min(fine);
        let y0 = a.y.min(b.y).min(c.y).floor().max(0.0) as usize;
        let y1 = (a.y.max(b.y).max(c.y).ceil() as usize).min(fine);
        for y in y0..y1 {
            for x in x0..x1 {
                let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let w0 = ((b.x - p.x) * (c.y - p.y) - (b.y - p.y) * (c.x - p.x)) / area;
                let w1 = ((c.x - p.x) * (a.y - p.y) - (c.y - p.y) * (a.x - p.x)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = w0 * a.z + w1 * b.z + w2 * c.z;
                let slot = y * fine + x;
                if z <= depth[slot] {
                    continue;
                }
                let uv = triangle.uvs[0] * w0 + triangle.uvs[1] * w1 + triangle.uvs[2] * w2;
                let texel = colouring.at(uv);
                if texel.w < colouring.cutoff {
                    continue;
                }
                depth[slot] = z;
                colour[slot] = texel.with_w(1.0);
            }
        }
    }
    // Deeper inside the crown, darker: light reaches it less.
    let (near, far) = (high.z, low.z);
    for (pixel, z) in colour.iter_mut().zip(&depth) {
        if pixel.w > 0.0 {
            let inside = ((near - z) / (near - far).max(1e-3)).clamp(0.0, 1.0);
            *pixel = (pixel.truncate() * (1.0 - 0.45 * inside)).extend(1.0);
        }
    }
    // Averaged down: colour weighted by coverage, coverage as alpha.
    let mut pixels = vec![Vec4::ZERO; SIDE * SIDE];
    for y in 0..SIDE {
        for x in 0..SIDE {
            let mut sum = Vec4::ZERO;
            for dy in 0..FINER {
                for dx in 0..FINER {
                    let p = colour[(y * FINER + dy) * fine + x * FINER + dx];
                    sum += (p.truncate() * p.w).extend(p.w);
                }
            }
            let coverage = sum.w / (FINER * FINER) as f32;
            pixels[y * SIDE + x] = if sum.w > 0.0 { (sum.truncate() / sum.w).extend(coverage) } else { Vec4::ZERO };
        }
    }
    View { pixels, right: centre, foot: low.y, size }
}

/// The share of pixels whose alpha passes `cutoff`.
fn coverage(pixels: &[Vec4], cutoff: f32) -> f32 {
    pixels.iter().filter(|p| p.w >= cutoff).count() as f32 / pixels.len().max(1) as f32
}

/// The atlas of views (columns) of variants (rows), mipmapped so that a
/// card keeps its coverage as it shrinks (each level's alpha scaled until
/// as many pixels pass the cut as at the top), transparent pixels filled
/// with the mean colour so that filtering does not darken the edges.
fn atlas(views: &[Vec<View>], cutoff: f32) -> Image {
    let (columns, rows) = (VIEWS, views.len());
    let (width, height) = (columns * SIDE, rows * SIDE);
    let mut level = vec![Vec4::ZERO; width * height];
    for (row, variant) in views.iter().enumerate() {
        for (column, view) in variant.iter().enumerate() {
            for y in 0..SIDE {
                for x in 0..SIDE {
                    level[(row * SIDE + y) * width + column * SIDE + x] = view.pixels[y * SIDE + x];
                }
            }
        }
    }
    let filled: Vec<&Vec4> = level.iter().filter(|p| p.w > 0.0).collect();
    let mean = filled.iter().map(|p| p.truncate()).sum::<Vec3>() / filled.len().max(1) as f32;
    for pixel in &mut level {
        if pixel.w == 0.0 {
            *pixel = mean.extend(0.0);
        }
    }
    let target = coverage(&level, cutoff);
    let (mut w, mut h, mut levels) = (width, height, 0);
    let mut data = Vec::new();
    loop {
        for p in &level {
            let c = Color::linear_rgba(p.x, p.y, p.z, p.w.clamp(0.0, 1.0)).to_srgba().to_u8_array();
            data.extend_from_slice(&c);
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
                    sum += level[(2 * y + dy).min(h - 1) * w + (2 * x + dx).min(w - 1)];
                }
                next.push(sum / 4.0);
            }
        }
        // Keep the coverage: scale alpha so that as many pass the cut.
        let (mut lo, mut hi) = (1.0f32, 4.0f32);
        for _ in 0..16 {
            let mid = (lo + hi) / 2.0;
            let scaled: Vec<Vec4> = next.iter().map(|p| p.with_w(p.w * mid)).collect();
            if coverage(&scaled, cutoff) < target { lo = mid } else { hi = mid }
        }
        for p in &mut next {
            p.w = (p.w * hi).min(1.0);
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
        anisotropy_clamp: 4,
        ..default()
    });
    image
}

/// The cards of every place, each place showing the views of variant
/// `variant_of(place)`: per view, a card through the trunk facing the
/// view's direction, both sides, its normals leaning up and out so that it
/// is lit like a crown rather than a sheet.
fn cards(places: &[Transform], views: &[Vec<View>]) -> Mesh {
    let (mut positions, mut normals, mut uvs, mut indices) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let rows = views.len() as f32;
    for (i, place) in places.iter().enumerate() {
        let row = i % views.len();
        for (k, view) in views[row].iter().enumerate() {
            let (from, right) = axes(k);
            let half = view.size / 2.0;
            let corner =
                |side: f32, up: f32| right * (view.right + side * half) + Vec3::Y * (view.foot + up * view.size);
            let (u0, u1) = (k as f32 / VIEWS as f32, (k + 1) as f32 / VIEWS as f32);
            let (v0, v1) = (row as f32 / rows, (row + 1) as f32 / rows);
            for facing in [1.0f32, -1.0] {
                let first = positions.len() as u32;
                for (side, up, u, v) in
                    [(-1.0, 0.0, u0, v1), (1.0, 0.0, u1, v1), (1.0, 1.0, u1, v0), (-1.0, 1.0, u0, v0)]
                {
                    let local = corner(side, up);
                    let out = (right * side * 0.6 + from * facing * 0.5 + Vec3::Y * (0.4 + 0.5 * up)).normalize();
                    positions.push(place.transform_point(local).to_array());
                    normals.push((place.rotation * out).to_array());
                    // Seen from behind, the picture is mirrored, as it would be.
                    uvs.push([u, v]);
                }
                // Counter-clockwise seen from the side it faces.
                indices.extend(if facing > 0.0 {
                    [first, first + 1, first + 2, first, first + 2, first + 3]
                } else {
                    [first, first + 2, first + 1, first, first + 3, first + 2]
                });
            }
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices))
}

/// The triangles of a mesh, placed, with their texture coordinates.
fn triangles_of(mesh: &Mesh, place: Transform, material: usize, out: &mut Vec<Triangle>) {
    let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { return };
    let uvs: Vec<Vec2> = match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
        Some(VertexAttributeValues::Float32x2(uvs)) => uvs.iter().map(|uv| Vec2::from(*uv)).collect(),
        _ => vec![Vec2::ZERO; positions.len()],
    };
    let indices: Vec<usize> = match mesh.indices() {
        Some(indices) => indices.iter().collect(),
        None => (0..positions.len()).collect(),
    };
    for t in indices.as_chunks::<3>().0 {
        out.push(Triangle {
            corners: t.map(|i| place.transform_point(Vec3::from(positions[i]))),
            uvs: t.map(|i| uvs[i]),
            material,
        });
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn plant(
    mut commands: Commands,
    woods: Query<(Entity, &Woods), Without<Planted>>,
    server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    nodes: Res<Assets<GltfNode>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    gltf_materials: Res<Assets<GltfMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut waiting: ResMut<crate::Waiting>,
) {
    for (entity, wood) in &woods {
        if server.load_state(&wood.model).is_failed() {
            warn!("a model did not load: {:?}", server.get_path(&wood.model));
            commands.entity(entity).insert(Planted);
            continue;
        }
        if !server.is_loaded_with_dependencies(&wood.model) {
            waiting.0 += 1;
            continue;
        }
        let Some(variants) = gltfs.get(&wood.model).and_then(|gltf| pieces(gltf, &nodes, &gltf_meshes, wood.using))
        else {
            continue;
        };
        let mut colourings = Vec::new();
        let mut cutoff: f32 = 0.5;
        let views: Vec<Vec<View>> = variants
            .iter()
            .map(|variant| {
                let mut triangles = Vec::new();
                for piece in variant {
                    let source =
                        piece.material.as_ref().and_then(|m| gltf_materials.get(m)).cloned().unwrap_or_default();
                    let standard = standard_material_from_gltf_material(&source);
                    let texture = standard.base_color_texture.as_ref().and_then(|h| images.get(h)).and_then(|image| {
                        let readable = matches!(
                            image.texture_descriptor.format,
                            TextureFormat::Rgba8UnormSrgb | TextureFormat::Rgba8Unorm
                        );
                        let (w, h) = (image.width() as usize, image.height() as usize);
                        let data = image.data.as_ref().filter(|d| readable && d.len() >= w * h * 4)?;
                        Some((data[..w * h * 4].to_vec(), w, h))
                    });
                    let cut = match standard.alpha_mode {
                        AlphaMode::Mask(cut) => cut,
                        _ => 0.5,
                    };
                    cutoff = cutoff.min(cut);
                    colourings.push(Colouring {
                        factor: Vec4::from(standard.base_color.to_linear().to_f32_array()),
                        texture,
                        cutoff: cut,
                    });
                    if let Some(mesh) = meshes.get(&piece.mesh) {
                        triangles_of(mesh, piece.place, colourings.len() - 1, &mut triangles);
                    }
                }
                (0..VIEWS).map(|k| draw(&triangles, &colourings, k)).collect()
            })
            .collect();
        let atlas = images.add(atlas(&views, 0.5));
        let material = materials.add(StandardMaterial {
            base_color_texture: Some(atlas),
            alpha_mode: AlphaMode::Mask(0.5),
            perceptual_roughness: 0.95,
            reflectance: 0.2,
            ..default()
        });
        debug!("woods of {:?}: {} trees as cards", server.get_path(&wood.model), wood.places.len());
        let mesh = meshes.add(cards(&wood.places, &views));
        commands.entity(entity).insert((
            Planted,
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::IDENTITY,
            Visibility::Inherited,
            NotShadowCaster,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cone of `count` small square leaves around the vertical, green.
    fn tree(count: usize) -> Vec<Triangle> {
        let mut triangles = Vec::new();
        for n in 0..count {
            let t = n as f32 / count as f32;
            let (angle, height) = (t * 97.0, 1.0 + 4.0 * t);
            let radius = 1.5 * (1.0 - t);
            let at = Vec3::new(angle.cos() * radius, height, angle.sin() * radius);
            let (a, b, c, d) = (at, at + Vec3::X * 0.3, at + Vec3::new(0.3, 0.3, 0.0), at + Vec3::Y * 0.3);
            triangles.push(Triangle { corners: [a, b, c], uvs: [Vec2::ZERO; 3], material: 0 });
            triangles.push(Triangle { corners: [a, c, d], uvs: [Vec2::ZERO; 3], material: 0 });
        }
        // The trunk.
        triangles.push(Triangle {
            corners: [Vec3::new(-0.1, 0.0, 0.0), Vec3::new(0.1, 0.0, 0.0), Vec3::new(0.0, 1.2, 0.0)],
            uvs: [Vec2::ZERO; 3],
            material: 0,
        });
        triangles
    }

    fn green() -> Vec<Colouring> {
        vec![Colouring { factor: Vec4::new(0.1, 0.3, 0.05, 1.0), texture: None, cutoff: 0.5 }]
    }

    #[test]
    fn a_view_is_fitted_to_the_tree_and_covers_it() {
        let view = draw(&tree(400), &green(), 0);
        assert!((view.foot - 0.0).abs() < 1e-3, "foot at the ground: {}", view.foot);
        assert!((view.size - 5.3).abs() < 0.05, "fitted to its height: {}", view.size);
        let covered = coverage(&view.pixels, 0.5);
        assert!((0.08..0.6).contains(&covered), "a crown, not a square nor nothing: {covered}");
        // The bottom row holds the trunk only, where the axis falls: the
        // picture is centred on the tree's box, not on its trunk.
        let axis = (SIDE as f32 / 2.0 - view.right * SIDE as f32 / view.size) as usize;
        let bottom: Vec<usize> = (0..SIDE).filter(|x| view.pixels[(SIDE - 1) * SIDE + x].w > 0.5).collect();
        assert!(!bottom.is_empty() && bottom.iter().all(|x| x.abs_diff(axis) < 8), "{bottom:?} around {axis}");
    }

    #[test]
    fn the_atlas_keeps_its_coverage_as_it_shrinks() {
        let views: Vec<Vec<View>> = vec![(0..VIEWS).map(|k| draw(&tree(400), &green(), k)).collect()];
        let image = atlas(&views, 0.5);
        let data = image.data.as_ref().unwrap();
        let (w, h) = (VIEWS * SIDE, SIDE);
        let top = data[..w * h * 4].chunks(4).filter(|p| p[3] >= 128).count() as f32 / (w * h) as f32;
        // The level of a quarter of the size.
        let offset = (w * h + (w / 2) * (h / 2)) * 4;
        let quarter = &data[offset..offset + (w / 4) * (h / 4) * 4];
        let small = quarter.chunks(4).filter(|p| p[3] >= 128).count() as f32 / ((w / 4) * (h / 4)) as f32;
        assert!((small - top).abs() < 0.04, "coverage {top} at the top, {small} at a quarter");
        assert!(image.texture_descriptor.mip_level_count > 8);
    }

    #[test]
    fn cards_cross_on_the_trunk_at_each_place() {
        let views: Vec<Vec<View>> = vec![(0..VIEWS).map(|k| draw(&tree(100), &green(), k)).collect()];
        let places =
            [Transform::from_xyz(10.0, 2.0, -5.0), Transform::from_xyz(-3.0, 0.0, 7.0).with_scale(Vec3::splat(2.0))];
        let mesh = cards(&places, &views);
        let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { panic!() };
        assert_eq!(p.len(), places.len() * VIEWS * 2 * 4, "two sides of a card per view");
        assert_eq!(mesh.indices().unwrap().len(), places.len() * VIEWS * 2 * 6);
        // Every card of the first place stands on its foot and reaches the tree's top.
        let first = &p[..VIEWS * 8];
        let (low, high) = first.iter().fold((f32::MAX, f32::MIN), |(l, h), v| (l.min(v[1]), h.max(v[1])));
        assert!((low - 2.0).abs() < 1e-3 && (high - (2.0 + views[0][0].size)).abs() < 1e-3);
        // The second, twice as big.
        let second = &p[VIEWS * 8..];
        let high = second.iter().map(|v| v[1]).fold(f32::MIN, f32::max);
        assert!((high - 2.0 * views[0][0].size).abs() < 1e-3);
    }
}
