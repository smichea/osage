// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use anyhow::{Context, Result, ensure};
use bevy::prelude::{Mat4, Quat, Resource, Vec2, Vec3};
use gltf::{Gltf, buffer::Source, mesh::Mode};
use osage_dynamics::StaticMesh;
use std::collections::HashSet;

pub const DEFAULT: &[u8] = include_bytes!("../fixtures/bir-hakeim.glb");

#[derive(Clone, Copy, Default, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum SurfaceAlpha {
    #[default]
    Opaque,
    Mask,
    Blend,
}

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SurfaceStyle {
    pub alpha_mode: SurfaceAlpha,
    pub alpha_cutoff: f32,
    pub double_sided: bool,
}

impl Default for SurfaceStyle {
    fn default() -> Self {
        Self { alpha_mode: SurfaceAlpha::Opaque, alpha_cutoff: 0.5, double_sided: false }
    }
}

impl SurfaceStyle {
    fn read(material: gltf::Material) -> Self {
        use gltf::material::AlphaMode;
        Self {
            alpha_mode: match material.alpha_mode() {
                AlphaMode::Opaque => SurfaceAlpha::Opaque,
                AlphaMode::Mask => SurfaceAlpha::Mask,
                AlphaMode::Blend => SurfaceAlpha::Blend,
            },
            alpha_cutoff: material.alpha_cutoff().unwrap_or(0.5),
            double_sided: material.double_sided(),
        }
    }
}

#[derive(Clone)]
pub struct Primitive {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Option<Vec<[f32; 4]>>,
    pub colors_1: Option<Vec<[f32; 4]>>,
    pub source_colors: Option<Vec<[f32; 4]>>,
    pub uv: Option<Vec<[f32; 2]>>,
    pub indices: Vec<u32>,
    pub color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub facade_glass: bool,
    pub style: SurfaceStyle,
    pub texture: Option<crate::texture::Texture>,
    pub(crate) embedded_texture: Option<crate::compressed_texture::Binding>,
}

pub struct Instance {
    pub mesh: usize,
    pub transform: bevy::prelude::Mat4,
}

#[derive(Debug)]
pub struct Support {
    pub id: String,
    pub low: Vec2,
    pub high: Vec2,
    pub y: f32,
}

#[derive(Resource)]
pub struct Sector {
    pub meshes: Vec<Vec<Primitive>>,
    pub instances: Vec<Instance>,
    pub supports: Vec<Support>,
    pub colliders: Vec<StaticMesh>,
    pub triangles: usize,
    pub object: Option<crate::object::ObjectContext>,
    pub bundled_bridge: bool,
}

pub struct ImageBinding {
    pub mesh: usize,
    pub part: usize,
    pub file: String,
    pub alternative: Option<String>,
    pub sampler: crate::texture::Sampler,
}
pub(crate) fn read_glb(bytes: &[u8]) -> Result<Gltf> {
    match Gltf::from_slice(bytes) {
        Ok(gltf) => Ok(gltf),
        Err(gltf::Error::Deserialize(error)) if error.to_string().starts_with("missing field `nodes`") => {
            // glTF 2 permits a scene without roots. gltf-json 1.4 requires the field when reading it.
            let glb = gltf::binary::Glb::from_slice(bytes)?;
            let mut root: serde_json::Value = serde_json::from_slice(&glb.json)?;
            if let Some(scenes) = root.get_mut("scenes").and_then(|s| s.as_array_mut()) {
                for scene in scenes.iter_mut().filter_map(|s| s.as_object_mut()) {
                    scene.entry("nodes").or_insert_with(|| serde_json::json!([]));
                }
            }
            let document = gltf::Document::from_json(serde_json::from_value(root)?)?;
            Ok(Gltf { document, blob: glb.bin.map(|b| b.into_owned()) })
        }
        Err(other) => Err(other.into()),
    }
}

pub(crate) fn read_meshes(
    doc: &Gltf,
    generate_normals: bool,
    mut bindings: Option<&mut Vec<ImageBinding>>,
    deferred_textures: bool,
) -> Result<Vec<Vec<Primitive>>> {
    ensure!(
        doc.buffers().len() == 1 && matches!(doc.buffers().next().unwrap().source(), Source::Bin),
        "expected one embedded GLB buffer"
    );
    let buffer = doc.blob.as_deref().context("missing GLB binary chunk")?;
    let declared = doc.buffers().next().unwrap().length();
    ensure!(declared <= buffer.len() && buffer.len() - declared <= 3, "GLB payload length mismatch");
    for view in doc.views() {
        ensure!(
            view.offset().checked_add(view.length()).is_some_and(|end| end <= declared),
            "buffer view outside payload"
        );
    }
    for a in doc.accessors() {
        let v = a.view().context("expected dense accessors")?;
        let stride = v.stride().unwrap_or(a.size());
        let end = a
            .count()
            .checked_sub(1)
            .and_then(|n| n.checked_mul(stride))
            .and_then(|len| len.checked_add(a.offset()))
            .and_then(|len| len.checked_add(a.size()));
        ensure!(
            a.sparse().is_none() && stride >= a.size() && end.is_some_and(|end| end <= v.length()),
            "invalid dense accessor"
        );
    }
    let mut compressed_inputs = crate::compressed_texture::Inputs::default();
    let mut texture_pixels = std::collections::BTreeMap::new();
    let mut meshes = Vec::new();
    for mesh in doc.meshes() {
        let mut parts = Vec::new();
        for part in mesh.primitives() {
            ensure!(part.mode() == Mode::Triangles, "expected triangles");
            let read = part.reader(|b| (b.index() == 0).then_some(buffer));
            let positions: Vec<_> = read.read_positions().context("missing POSITION")?.collect();

            let indices: Vec<_> = read.read_indices().context("missing indices")?.into_u32().collect();
            ensure!(
                !indices.is_empty()
                    && indices.len().is_multiple_of(3)
                    && indices.iter().all(|i| (*i as usize) < positions.len()),
                "invalid triangles"
            );
            let normals: Vec<_> = match read.read_normals() {
                Some(normals) => normals.collect(),
                None if generate_normals => {
                    let mut sums = vec![Vec3::ZERO; positions.len()];
                    for tri in indices.as_chunks::<3>().0 {
                        let [a, b, c] = tri.map(|i| Vec3::from(positions[i as usize]));
                        let normal = (b - a).cross(c - a);
                        for &i in tri {
                            sums[i as usize] += normal;
                        }
                    }
                    sums.into_iter().map(|n| n.normalize_or_zero().to_array()).collect()
                }
                None => anyhow::bail!("missing NORMAL"),
            };
            ensure!(positions.len() == normals.len() && !positions.is_empty(), "invalid vertex arrays");
            ensure!(positions.iter().chain(&normals).flatten().all(|v| v.is_finite()), "non-finite vertex");
            let pbr = part.material().pbr_metallic_roughness();
            let mut embedded_texture = None;
            let texture = if let Some(info) = pbr.base_color_texture() {
                ensure!(info.tex_coord() == 0, "base colour image requires TEXCOORD_0");
                let t = info.texture();
                if deferred_textures {
                    embedded_texture =
                        Some(crate::compressed_texture::Binding::read(doc, &info, buffer, &mut compressed_inputs)?);
                    None
                } else if let gltf::image::Source::Uri { uri, .. } = t.source().source()
                    && !uri.starts_with("data:")
                    && let Some(pending) = bindings.as_deref_mut()
                {
                    pending.push(ImageBinding {
                        mesh: mesh.index(),
                        part: part.index(),
                        file: uri.to_owned(),
                        alternative: t
                            .extension_value("KHR_texture_basisu")
                            .map(|ext| -> Result<String> {
                                let i = ext["source"]
                                    .as_u64()
                                    .and_then(|n| usize::try_from(n).ok())
                                    .context("invalid optional BasisU binding")?;
                                match doc.images().nth(i).context("missing optional BasisU image")?.source() {
                                    gltf::image::Source::Uri { uri, mime_type: Some("image/ktx2") } => {
                                        Ok(uri.to_owned())
                                    }
                                    _ => anyhow::bail!("invalid district BasisU image URI"),
                                }
                            })
                            .transpose()?,
                        sampler: crate::texture::Sampler::from_gltf(t.sampler()),
                    });
                    None
                } else {
                    Some(crate::texture::Texture::embedded(info, buffer, &mut texture_pixels)?)
                }
            } else {
                None
            };
            let style = SurfaceStyle::read(part.material());
            ensure!(style.alpha_cutoff.is_finite() && style.alpha_cutoff >= 0., "invalid alpha cutoff");
            let facade_glass =
                mesh.name().is_some_and(|s| s.ends_with("-facade")) && part.material().name() == Some("glass");
            let colors = read.read_colors(0).map(|v| v.into_rgba_f32().collect::<Vec<_>>());
            let colors_1 = read.read_colors(1).map(|v| v.into_rgba_f32().collect::<Vec<_>>());
            let source_colors = part
                .get(&gltf::Semantic::Extras("OSAGE_SOURCE_COLOR_0".into()))
                .map(|a| {
                    use gltf::{
                        accessor::{DataType, Dimensions, Iter},
                        mesh::util::ReadColors,
                    };
                    ensure!(a.dimensions() == Dimensions::Vec4, "archival vertex colour must have four components");
                    let bytes = |b: gltf::Buffer| (b.index() == 0).then_some(buffer);
                    let colors = match (a.data_type(), a.normalized()) {
                        (DataType::U8, true) => Iter::new(a, bytes).map(ReadColors::RgbaU8),
                        (DataType::U16, true) => Iter::new(a, bytes).map(ReadColors::RgbaU16),
                        (DataType::F32, false) => Iter::new(a, bytes).map(ReadColors::RgbaF32),
                        _ => None,
                    }
                    .context("invalid archival vertex colour encoding")?;
                    Ok::<_, anyhow::Error>(colors.into_rgba_f32().collect::<Vec<_>>())
                })
                .transpose()?;
            let uv = read.read_tex_coords(0).map(|v| v.into_f32().collect::<Vec<_>>());
            ensure!(
                colors
                    .iter()
                    .chain(&colors_1)
                    .chain(&source_colors)
                    .all(|a| a.len() == positions.len() && a.iter().flatten().all(|p| p.is_finite()))
                    && uv
                        .as_ref()
                        .is_none_or(|a| a.len() == positions.len() && a.iter().flatten().all(|p| p.is_finite())),
                "invalid vertex attributes"
            );
            ensure!(pbr.base_color_texture().is_none() || uv.is_some(), "textured primitive has no TEXCOORD_0");
            parts.push(Primitive {
                embedded_texture,
                texture,
                positions,
                normals,
                colors,
                colors_1,
                source_colors,
                uv,
                indices,
                color: pbr.base_color_factor(),
                metallic: pbr.metallic_factor(),
                roughness: pbr.roughness_factor(),
                facade_glass,
                style,
            });
        }
        meshes.push(parts);
    }
    Ok(meshes)
}

impl Sector {
    /// The district fixture uses a flat glTF scene with one binary buffer.
    /// Both the mesh sent to the GPU and the landing footprints come from its POSITION data.
    pub fn read(bytes: &[u8]) -> Result<Self> {
        Self::read_levels(bytes, Default::default())
    }

    pub fn read_levels(bytes: &[u8], levels: osage_objects::GeometryLevels) -> Result<Self> {
        Self::decode(bytes, levels, true, false, None, false)
    }

    pub fn read_adaptive(bytes: &[u8], levels: osage_objects::GeometryLevels) -> Result<Self> {
        Self::decode(bytes, levels, true, true, None, false)
    }

    pub fn read_for_view(bytes: &[u8], levels: osage_objects::GeometryLevels, adaptive: bool) -> Result<Self> {
        Self::decode(bytes, levels, true, adaptive, None, true)
    }

    pub fn read_visual_page(bytes: &[u8]) -> Result<Self> {
        Self::decode(bytes, Default::default(), false, false, None, false)
    }

    pub fn read_actor_page(bytes: &[u8]) -> Result<(Self, Vec<usize>)> {
        let doc = read_glb(bytes)?;
        let (profile, source_nodes) = osage_objects::ObjectProfile::read_with_source_indices(&doc.document)?
            .context("actor GLB needs an Osage object profile")?;
        let meshes = read_meshes(&doc, true, None, false)?;
        let sector = crate::object::sector(meshes, profile, Default::default(), false)?;
        Ok((sector, source_nodes))
    }

    pub fn read_tile(bytes: &[u8], bindings: &mut Vec<ImageBinding>) -> Result<Self> {
        Self::decode(bytes, Default::default(), false, false, Some(bindings), false)
    }

    fn decode(
        bytes: &[u8],
        levels: osage_objects::GeometryLevels,
        require_collision: bool,
        adaptive: bool,
        bindings: Option<&mut Vec<ImageBinding>>,
        deferred_textures: bool,
    ) -> Result<Self> {
        let doc = read_glb(bytes)?;
        let profile = osage_objects::ObjectProfile::read(&doc.document)?;
        ensure!(bindings.is_none() || profile.is_none(), "tile must use flat geometry");
        ensure!(profile.is_some() || levels == Default::default(), "explicit geometry levels need an object profile");
        let meshes = read_meshes(&doc, profile.is_some(), bindings, deferred_textures)?;
        if let Some(profile) = profile {
            return crate::object::sector(meshes, profile, levels, adaptive);
        }
        let mut instances = Vec::new();
        let mut supports = Vec::new();
        let mut colliders = Vec::new();
        let mut ids = HashSet::new();
        let mut triangles = 0;
        for node in doc.default_scene().context("missing default scene")?.nodes() {
            ensure!(node.children().len() == 0, "expected a flat scene");
            let mesh = node.mesh().context("expected a mesh instance")?.index();
            let transform = match node.transform() {
                gltf::scene::Transform::Matrix { matrix } => Mat4::from_cols_array_2d(&matrix),
                gltf::scene::Transform::Decomposed { translation, rotation, scale } => {
                    Mat4::from_scale_rotation_translation(scale.into(), Quat::from_array(rotation), translation.into())
                }
            };
            ensure!(
                transform.is_finite()
                    && transform.row(3) == bevy::math::Vec4::W
                    && transform.as_dmat4().determinant() != 0.,
                "invalid instance transform"
            );
            let name = node.name().unwrap_or_default();
            let support = name.strip_prefix("support/");
            if let Some(id) = support.or_else(|| name.strip_prefix("collider/")) {
                ensure!(
                    !id.is_empty() && !id.contains([',', '\n', '\r']) && ids.insert(id.to_string()),
                    "duplicate or empty support ID {id}"
                );
                if support.is_some() {
                    supports.push(top(id, &meshes[mesh], transform)?);
                }
                colliders.push(crate::object::world_mesh(id, &meshes[mesh], transform)?);
            }
            triangles += meshes[mesh].iter().map(|p| p.indices.len() / 3).sum::<usize>();
            instances.push(Instance { mesh, transform });
        }
        ensure!(!require_collision || !colliders.is_empty(), "scene has no collision geometry");
        // Stable tie break for coincident surfaces, independent of node order.
        supports.sort_by(|a, b| a.id.cmp(&b.id));
        colliders.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(Self { meshes, instances, supports, colliders, triangles, object: None, bundled_bridge: bytes == DEFAULT })
    }
}

/// A support node promises a horizontal, filled rectangular top. Check the two
/// triangles that actually form it before using its footprint for the sweep.
fn top(id: &str, parts: &[Primitive], transform: Mat4) -> Result<Support> {
    let vertices: Vec<_> = parts
        .iter()
        .flat_map(|p| p.indices.iter().map(|&i| transform.transform_point3(Vec3::from(p.positions[i as usize]))))
        .collect();
    ensure!(vertices.iter().all(|v| v.is_finite()), "non-finite support {id}");
    let y = vertices.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
    let faces: Vec<_> = vertices.as_chunks::<3>().0.iter().filter(|tri| tri.iter().all(|p| p.y == y)).collect();
    ensure!(faces.len() == 2, "support {id} needs two flat top triangles");
    let points: Vec<_> = faces.iter().flat_map(|tri| tri.iter().map(|p| Vec2::new(p.x, p.z))).collect();
    let low = points.iter().copied().fold(Vec2::splat(f32::INFINITY), Vec2::min);
    let high = points.iter().copied().fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max);
    let corners = [low, Vec2::new(low.x, high.y), high, Vec2::new(high.x, low.y)];
    ensure!(
        (high - low).min_element() > 0. && corners.iter().all(|p| points.contains(p)),
        "invalid rectangular top on {id}"
    );
    // The diagonally opposite vertices must be the shared edge.
    let shared: Vec<_> = points[..3].iter().filter(|p| points[3..].contains(p)).collect();
    ensure!(
        shared.len() == 2 && shared[0].x != shared[1].x && shared[0].y != shared[1].y,
        "invalid top triangles on {id}"
    );
    Ok(Support { id: id.to_owned(), low, high, y })
}
