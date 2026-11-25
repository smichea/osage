// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

mod collision;
mod directory_columns;
pub(super) mod faces;

use super::source::{Source, Ticket};
use crate::{bench::sha, scene::Sector};
use anyhow::{Context, Result, ensure};
use bevy::prelude::Vec3;
use serde::Deserialize;
use std::{borrow::Cow, collections::BTreeSet};

#[derive(Clone, Default, Deserialize)]
pub struct File {
    pub file: String,
    pub sha256: String,
    pub bytes: usize,
}

impl File {
    pub fn read(&self, dir: &Source) -> Result<Vec<u8>> {
        let data = dir.raw(self)?;
        self.check(&data)?;
        Ok(data)
    }
    pub fn received<'a>(&self, source: &Source, ticket: Option<&'a Ticket>) -> Result<Cow<'a, [u8]>> {
        if let Some(ticket) = ticket {
            return Ok(Cow::Borrowed(ticket.bytes()?));
        }
        self.read(source).map(Cow::Owned)
    }
    pub fn check(&self, data: &[u8]) -> Result<()> {
        ensure!(data.len() == self.bytes && sha(data) == self.sha256, "bad tile content {}", self.file);
        Ok(())
    }
    pub fn validate(&self, ext: &str) -> Result<()> {
        ensure!(
            self.sha256.len() == 64
                && self.sha256.bytes().all(|c| c.is_ascii_hexdigit())
                && self.file == format!("{}.{ext}", self.sha256)
                && self.bytes > 0,
            "invalid file key"
        );
        Ok(())
    }
}

#[derive(Default, Deserialize)]
pub struct Layer {
    #[serde(flatten)]
    pub content: File,
    pub bounds: [[f32; 3]; 2],
    pub triangles: usize,
    #[serde(default)]
    pub ids: Vec<String>,
    #[serde(default)]
    pub(super) source_face_runs: Vec<faces::Piece>,
    #[serde(skip)]
    pub(super) ordered_faces: bool,
    #[serde(default)]
    pub source_nodes: Vec<usize>,
    #[serde(default)]
    pub materials: Vec<osage_objects::CollisionMaterial>,
    #[serde(default)]
    pub uses: Vec<usize>,
    #[serde(default)]
    pub uses_images: Vec<usize>,
    #[serde(default)]
    pub image_choices: Vec<(usize, Option<usize>)>,
    #[serde(default)]
    pub lods: Vec<super::detail::Placement>,
    #[serde(default)]
    pub adaptive_only: bool,
}

impl Layer {
    fn resolved_file(&self, sha256: String) -> File {
        File { file: format!("{sha256}{}", self.content.file), sha256, bytes: self.content.bytes }
    }

    pub fn distance(&self, point: Vec3) -> f32 {
        let min = Vec3::from(self.bounds[0]);
        let max = Vec3::from(self.bounds[1]);
        point.distance(point.clamp(min, max))
    }
}

#[derive(Deserialize)]
pub struct Tile {
    pub x: i32,
    pub z: i32,
    pub visual: Layer,
    pub physics: Option<Layer>,
    pub parent_first: usize,
    pub parent_count: usize,
}

#[derive(Deserialize)]
struct PhysicsDirectory {
    physical_addresses: Option<File>,
    format: String,
    source_sha256: String,
    edge_m: f32,
    source_parents: Vec<Option<usize>>,
    source_instances: usize,
    source_triangles: usize,
    support: SupportFile,
    #[serde(default)]
    support_regions: Vec<SupportRegion>,
    collision_face_order: Option<u8>,
    object_profile: Option<super::channels::Profile>,
    tiles: Vec<(i32, i32, Option<Layer>)>,
}

impl From<PhysicsDirectory> for Index {
    fn from(p: PhysicsDirectory) -> Self {
        Self {
            physical_addresses: p.physical_addresses,
            format: p.format,
            source_sha256: p.source_sha256,
            edge_m: p.edge_m,
            source_parents: p.source_parents,
            source_instances: p.source_instances,
            source_triangles: p.source_triangles,
            support: Some(p.support),
            support_regions: p.support_regions,
            collision_face_order: p.collision_face_order,
            object_profile: p.object_profile,
            tiles: p
                .tiles
                .into_iter()
                .map(|(x, z, physics)| Tile {
                    x,
                    z,
                    physics,
                    visual: Default::default(),
                    parent_count: 0,
                    parent_first: 0,
                })
                .collect(),
            ..Default::default()
        }
    }
}

#[derive(Default, Deserialize)]
pub struct Index {
    #[serde(skip)]
    pub(super) inline_parent: Option<Vec<u8>>,
    #[serde(skip)]
    pub(super) inline_models: Option<Vec<u8>>,
    #[serde(skip)]
    pub(super) inline_global_support: Option<Vec<u8>>,
    #[serde(skip)]
    pub(super) inline_region: Option<(usize, Vec<u8>)>,
    #[serde(default)]
    pub(super) physical_addresses: Option<File>,
    #[serde(skip)]
    pub(super) resolved_addresses: Vec<std::sync::OnceLock<File>>,
    pub format: String,
    pub source_sha256: String,
    pub(super) collision_face_order: Option<u8>,
    pub edge_m: f32,
    pub parent: File,
    pub support: Option<SupportFile>,
    #[serde(default)]
    pub(super) support_regions: Vec<SupportRegion>,
    pub models: Option<ModelsFile>,
    #[serde(default)]
    pub model_resources: Vec<ModelFile>,
    #[serde(default)]
    pub model_variants: Vec<ModelVariant>,
    #[serde(default)]
    pub material_palette: Vec<Material>,
    #[serde(default)]
    pub texture_resources: Vec<ImageFile>,
    #[serde(default)]
    pub basisu_resources: Vec<BasisImage>,
    #[serde(skip)]
    pub(super) selected_alternatives: std::collections::BTreeMap<(String, String), usize>,
    #[serde(skip)]
    pub(crate) bc7_limit: u32,
    #[serde(skip)]
    pub(super) image_files: std::collections::BTreeMap<String, usize>,
    #[serde(default)]
    pub textures: Vec<TextureRef>,
    #[serde(default)]
    pub samplers: Vec<crate::texture::Sampler>,
    #[serde(skip)]
    pub encoded_bytes: usize,
    #[serde(default)]
    pub source_parents: Vec<Option<usize>>,
    pub tiles: Vec<Tile>,
    #[serde(skip)]
    pub(super) collision_pages: std::sync::OnceLock<super::planning::CollisionPages>,
    pub object_profile: Option<super::channels::Profile>,
    pub source_instances: usize,
    pub source_triangles: usize,
}

#[derive(Deserialize)]
pub(super) struct SupportRegion {
    #[serde(default)]
    pub complete_pages: bool,
    pub tiles: Vec<usize>,
    #[serde(flatten)]
    pub file: SupportFile,
}

#[derive(Deserialize)]
pub struct SupportFile {
    #[serde(flatten)]
    pub content: File,
    pub decoded_bytes: usize,
}
#[derive(Deserialize)]
pub struct ModelsFile {
    #[serde(flatten)]
    pub content: File,
    pub source_meshes: Vec<usize>,
}
#[derive(Deserialize)]
pub struct ModelFile {
    #[serde(flatten)]
    pub content: File,
    pub triangles: usize,
    pub primitives: usize,
}
#[derive(Deserialize)]
pub struct ModelVariant {
    pub source_mesh: usize,
    pub resource: usize,
    pub materials: Vec<usize>,
}
#[derive(Clone, Deserialize)]
pub struct ImageFile {
    #[serde(flatten)]
    pub content: File,
    #[serde(rename = "mimeType")]
    pub mime: String,
}

#[derive(Deserialize)]
pub struct BasisImage {
    #[serde(flatten)]
    pub content: File,
    pub source: usize,
}

#[derive(Deserialize)]
pub struct TextureRef {
    pub source: usize,
    pub basisu: Option<usize>,
    pub sampler: Option<usize>,
}

#[derive(Deserialize)]
pub struct TextureInfo {
    pub index: usize,
    #[serde(default, rename = "texCoord")]
    pub tex_coord: u32,
}
#[derive(Deserialize)]
pub struct Material {
    #[serde(default, rename = "pbrMetallicRoughness")]
    pub pbr: Pbr,
    #[serde(flatten)]
    pub style: crate::scene::SurfaceStyle,
}
impl Material {
    pub fn factors(&self, texture: Option<crate::texture::Texture>) -> crate::view::MaterialFactors {
        crate::view::MaterialFactors {
            color: self.pbr.color.unwrap_or([1.; 4]),
            metallic: self.pbr.metallic.unwrap_or(1.),
            roughness: self.pbr.roughness.unwrap_or(1.),
            style: self.style,
            texture,
        }
    }
}
#[derive(Default, Deserialize)]
pub struct Pbr {
    #[serde(rename = "baseColorTexture")]
    pub texture: Option<TextureInfo>,
    #[serde(rename = "baseColorFactor")]
    pub color: Option<[f32; 4]>,
    #[serde(rename = "metallicFactor")]
    pub metallic: Option<f32>,
    #[serde(rename = "roughnessFactor")]
    pub roughness: Option<f32>,
}

impl Index {
    pub(super) fn physical_pages(&self) -> &super::planning::CollisionPages {
        self.collision_pages.get_or_init(|| super::planning::CollisionPages::new(&self.tiles))
    }

    // Dense transfer slots give meshes and images one bandwidth budget and one eviction policy.
    pub fn dependencies<'a>(&self, layer: &'a Layer) -> impl Iterator<Item = usize> + use<'a> {
        let offset = self.model_resources.len();
        layer.uses.iter().copied().chain(layer.uses_images.iter().map(move |i| offset + i))
    }

    pub fn image_slot(&self, name: &str, alternate: Option<&str>) -> Result<usize> {
        let base = self.image_files.get(name).copied().context("tile image URI is absent from the district index")?;
        if self.bc7_limit != 0
            && let Some(alternate) = alternate
        {
            return self
                .selected_alternatives
                .get(&(name.to_owned(), alternate.to_owned()))
                .copied()
                .context("tile KTX2 image pair is absent from the district index");
        }
        Ok(base)
    }

    /// Keep the file indices in old directories valid, then resolve offers once against the device.
    pub fn select_basisu(&mut self, limit: Option<u32>) {
        let Some(limit) = limit else { return };
        self.bc7_limit = limit;
        let cores = self.texture_resources.len();
        for (id, offer) in self.basisu_resources.iter().enumerate() {
            let base = &self.texture_resources[offer.source].content.file;
            self.selected_alternatives.insert((base.clone(), offer.content.file.clone()), cores + id);
            self.texture_resources.push(ImageFile { content: offer.content.clone(), mime: "image/ktx2".into() });
        }
        for texture in &mut self.textures {
            if let Some(id) = texture.basisu {
                texture.source = cores + id;
            }
        }
        for tile in &mut self.tiles {
            let layer = &mut tile.visual;
            if !layer.image_choices.is_empty() {
                layer.uses_images = layer
                    .image_choices
                    .iter()
                    .map(|&(base, alt)| alt.map_or(base, |id| cores + id))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
            }
        }
    }

    /// Preserve every pending binding's slot. Retrying now requests its independent core file.
    pub fn fallback_image(&mut self, slot: usize) -> bool {
        let Some(id) = slot.checked_sub(self.model_resources.len()) else { return false };
        if self.bc7_limit == 0 || self.texture_resources[id].mime != "image/ktx2" {
            return false;
        }
        let cores = self.texture_resources.len() - self.basisu_resources.len();
        let base = self.basisu_resources[id - cores].source;
        self.texture_resources[id] = self.texture_resources[base].clone();
        true
    }

    pub fn resource(&self, slot: usize) -> &File {
        match self.model_resources.get(slot) {
            Some(model) => &model.content,
            None => &self.texture_resources[slot - self.model_resources.len()].content,
        }
    }

    pub fn load(dir: &Source, pin: Option<&super::directory::Pin>, visuals: bool) -> Result<Self> {
        use super::directory::Format;
        let (format, bytes) = match pin {
            Some(pin) => pin.read(dir)?,
            None => (Format::Full, dir.index()?),
        };
        let physics_only = !matches!(format, Format::Full | Format::VisualRegional | Format::FirstFrame);
        let mut index: Self = if physics_only {
            ensure!(!visuals, "a physics-only district pin requires --no-visuals");
            let p: PhysicsDirectory = if matches!(format, Format::PhysicsColumns | Format::RegionalAddresses) {
                directory_columns::read(&bytes, format == Format::RegionalAddresses)?
            } else {
                serde_json::from_slice(&bytes)?
            };
            ensure!(
                p.physical_addresses.is_some() == (format == Format::RegionalAddresses),
                "misplaced physical address table"
            );
            ensure!(p.format == "ville-paris/physics/1", "invalid physics directory");
            p.into()
        } else if format == Format::FirstFrame {
            directory_columns::read_first_frame(&bytes, visuals)?
        } else {
            let full: Self = if format == Format::VisualRegional {
                directory_columns::read_visual(&bytes)?
            } else {
                serde_json::from_slice(&bytes)?
            };
            ensure!(
                full.physical_addresses.is_some() == (format == Format::VisualRegional),
                "misplaced physical address table"
            );
            full
        };
        if let Some(table) = &index.physical_addresses {
            table.validate("digests")?;
            ensure!(
                !index.support_regions.is_empty()
                    && index.tiles.iter().filter(|t| t.physics.is_some()).count().checked_mul(32) == Some(table.bytes),
                "invalid physical address table length"
            );
            index.resolved_addresses.resize_with(index.tiles.len(), Default::default);
        }
        index.encoded_bytes = pin.map_or(bytes.len(), |p| p.file.bytes);
        ensure!(
            (physics_only
                || matches!(
                    index.format.as_str(),
                    "ville-paris/pages/1" | "ville-paris/pages/2" | "ville-paris/pages/3" | "ville-paris/pages/4"
                ))
                && index.edge_m.is_finite()
                && index.edge_m > 0.
                && !index.tiles.is_empty(),
            "invalid district index"
        );
        if !physics_only {
            index.parent.validate(if index.shared() { "coarse" } else { "glb" })?;
        }
        if let Some(models) = &index.models {
            ensure!(
                index.format == "ville-paris/pages/3"
                    && !models.source_meshes.is_empty()
                    && models.source_meshes.windows(2).all(|w| w[0] < w[1]),
                "invalid shared source mesh directory"
            );
            models.content.validate("models")?;
        }
        let mut hashes = BTreeSet::new();
        let mut meshes = BTreeSet::new();
        for r in &index.model_resources {
            r.content.validate("model")?;
            ensure!(
                index.demand() && hashes.insert(&r.content.sha256) && r.triangles > 0 && r.primitives > 0,
                "ambiguous model resource"
            );
        }
        let mut image_hashes = BTreeSet::new();
        for (id, image) in index.texture_resources.iter().enumerate() {
            index.image_files.insert(image.content.file.clone(), id);
            let ext = match image.mime.as_str() {
                "image/png" => "png",
                "image/jpeg" => "jpg",
                _ => anyhow::bail!("unsupported texture resource"),
            };
            image.content.validate(ext)?;
            ensure!(
                index.demand() && image_hashes.insert(&image.content.sha256),
                "duplicate or misplaced image resource"
            );
        }
        let mut pairs = BTreeSet::new();
        for offer in &index.basisu_resources {
            offer.content.validate("ktx2")?;
            ensure!(
                index.demand()
                    && offer.source < index.texture_resources.len()
                    && pairs.insert((offer.source, &offer.content.sha256)),
                "invalid optional BasisU resource"
            );
        }
        for s in &index.samplers {
            s.validate()?;
        }
        for t in &index.textures {
            ensure!(
                t.source < index.texture_resources.len()
                    && t.sampler.is_none_or(|i| i < index.samplers.len())
                    && t.basisu
                        .is_none_or(|i| index.basisu_resources.get(i).is_some_and(|offer| offer.source == t.source)),
                "invalid texture binding"
            );
        }
        for m in &index.material_palette {
            let p = &m.pbr;
            ensure!(
                p.texture.as_ref().is_none_or(|t| t.index < index.textures.len() && t.tex_coord == 0),
                "invalid base colour image"
            );
            ensure!(m.style.alpha_cutoff.is_finite() && m.style.alpha_cutoff >= 0., "invalid alpha cutoff");
            ensure!(
                p.color
                    .as_ref()
                    .into_iter()
                    .flatten()
                    .chain(p.metallic.iter())
                    .chain(p.roughness.iter())
                    .all(|x| (0.0..=1.0).contains(x)),
                "invalid PBR material factors"
            );
        }
        for v in &index.model_variants {
            ensure!(
                meshes.insert(v.source_mesh)
                    && index.model_resources.get(v.resource).is_some_and(|r| r.primitives == v.materials.len())
                    && v.materials.iter().all(|&i| i < index.material_palette.len()),
                "invalid source draw/material assignment"
            );
        }
        if index.format != "ville-paris/pages/1" {
            let s = index.support.as_ref().context("missing support catalogue; recook the district")?;
            s.content.validate(if s.content.file.ends_with(".zst") { "zst" } else { "zlib" })?;
            ensure!(s.decoded_bytes > 0 && !index.source_parents.is_empty(), "missing source tree or support faces");
            // Check every parent chain once, including unused non-mesh assembly nodes.
            let mut done = vec![false; index.source_parents.len()];
            for i in 0..done.len() {
                let mut seen = BTreeSet::new();
                let mut node = Some(i);
                while let Some(k) = node {
                    ensure!(k < done.len() && seen.insert(k), "invalid source ancestry");
                    if done[k] {
                        break;
                    }
                    node = index.source_parents[k];
                }
                for k in seen {
                    done[k] = true;
                }
            }
        }
        let mut cells = BTreeSet::new();
        let mut count = 0usize;
        for t in &index.tiles {
            ensure!(
                t.visual.uses.windows(2).all(|w| w[0] < w[1])
                    && t.visual.uses.iter().all(|&i| i < index.model_resources.len()),
                "unknown or duplicate model dependency"
            );
            ensure!(
                t.visual.uses_images.windows(2).all(|w| w[0] < w[1])
                    && t.visual.uses_images.iter().all(|&i| i < index.texture_resources.len()),
                "unknown or duplicate image dependency"
            );
            if !t.visual.image_choices.is_empty() {
                let sources: BTreeSet<_> = t.visual.image_choices.iter().map(|&(src, _)| src).collect();
                ensure!(
                    sources.into_iter().eq(t.visual.uses_images.iter().copied())
                        && t.visual.image_choices.iter().all(|&(src, alt)| alt
                            .is_none_or(|id| index.basisu_resources.get(id).is_some_and(|offer| offer.source == src))),
                    "invalid page image choices"
                );
            }
            ensure!(cells.insert((t.x, t.z)) && t.parent_first == count, "invalid tile key or coarse range");
            count = count.checked_add(t.parent_count).context("coarse range overflow")?;
            if let Some(l) = &t.physics
                && index.support.is_some()
            {
                ensure!(
                    (l.materials.is_empty()
                        || (l.materials.len() == l.ids.len() && l.materials.iter().all(|m| m.valid())))
                        && l.ids.len() == l.source_nodes.len()
                        && l.source_nodes.iter().all(|&n| n < index.source_parents.len()),
                    "a collision piece has no source-tree owner"
                );
            }
            for (layer, ext, deferred) in (!physics_only)
                .then_some((&t.visual, if index.shared() { "vpage" } else { "glb" }, false))
                .into_iter()
                .chain(t.physics.as_ref().map(|p| (p, p.collision_extension(), index.physical_addresses.is_some())))
            {
                if deferred {
                    ensure!(
                        layer.content.file == format!(".{ext}")
                            && layer.content.sha256.is_empty()
                            && layer.content.bytes > 0,
                        "invalid deferred physical descriptor"
                    );
                } else {
                    layer.content.validate(ext)?;
                }
                ensure!(
                    (layer.triangles > 0
                        || (ext == "vpage"
                            && t.physics.is_some()
                            && t.parent_count == 0
                            && index.object_profile.is_some()))
                        && layer.bounds.iter().flatten().all(|x| x.is_finite())
                        && (0..3).all(|i| layer.bounds[0][i] <= layer.bounds[1][i]),
                    "invalid page geometry metadata"
                );
            }
        }
        if let Some(profile) = &index.object_profile {
            profile.validate(&index)?;
        }
        faces::validate(&mut index)?;
        Ok(index)
    }
    pub(super) fn physical_file(&self, i: usize) -> Option<&File> {
        if self.resolved_addresses.is_empty() {
            self.tiles[i].physics.as_ref().map(|l| &l.content)
        } else {
            self.resolved_addresses[i].get()
        }
    }

    pub(super) fn resolve_all_addresses(&mut self, source: &Source) -> Result<()> {
        if let Some(table) = &self.physical_addresses {
            let bytes = table.read(source)?;
            for (layer, digest) in
                self.tiles.iter_mut().filter_map(|t| t.physics.as_mut()).zip(bytes.as_chunks::<32>().0)
            {
                layer.content = layer.resolved_file(crate::bench::hex(digest));
            }
            self.resolved_addresses = Vec::new();
        }
        Ok(())
    }

    pub(super) fn resolve_region(&self, pages: &[usize], digests: &[[u8; 32]]) -> Result<()> {
        ensure!(self.physical_addresses.is_some() && pages.len() == digests.len(), "invalid regional address count");
        for (&i, digest) in pages.iter().zip(digests) {
            let layer = self.tiles[i].physics.as_ref().context("nonphysical address row")?;
            let sha256 = crate::bench::hex(digest);
            if let Some(resident) = self.physical_file(i) {
                ensure!(resident.sha256 == sha256, "conflicting regional address");
            } else {
                self.resolved_addresses[i]
                    .set(layer.resolved_file(sha256))
                    .map_err(|_| anyhow::anyhow!("duplicate regional address"))?;
            }
        }
        Ok(())
    }

    pub fn shared(&self) -> bool {
        self.format == "ville-paris/pages/3" || self.demand()
    }
    pub fn demand(&self) -> bool {
        self.format == "ville-paris/pages/4"
    }
    pub fn path(&self, node: Option<usize>) -> Vec<usize> {
        let mut path = Vec::new();
        let mut at = node;
        while let Some(i) = at {
            path.push(i);
            at = self.source_parents.get(i).copied().flatten();
        }
        path.reverse();
        path
    }
}

#[derive(Clone, Deserialize)]
pub struct Shape {
    #[serde(skip)]
    pub(super) faces: faces::FaceOrder,
    #[serde(skip)]
    pub material: Option<osage_objects::CollisionMaterial>,
    pub id: String,
    pub source_node: Option<usize>,
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

pub struct Visual {
    pub scene: Sector,
    pub owners: Vec<usize>,
}

#[derive(Deserialize)]
struct Placement {
    extras: PlacementOwner,
    matrix: Option<[f32; 16]>,
}
#[derive(Deserialize)]
struct PlacementOwner {
    osage_source_node: usize,
}
#[derive(Deserialize)]
struct SceneRoots {
    #[serde(default)]
    nodes: Vec<usize>,
}
#[derive(Deserialize)]
struct Placements {
    #[serde(default)]
    nodes: Vec<Placement>,
    scene: usize,
    scenes: Vec<SceneRoots>,
}

impl Visual {
    pub fn load_parent(index: &mut Index, source: &Source) -> Result<Self> {
        let bytes = match index.inline_parent.take() {
            Some(body) => body,
            None => index.parent.read(source)?,
        };
        let raw = if !index.shared() {
            bytes
        } else if matches!(bytes.get(..4), Some(b"OVC2" | b"OVCU")) {
            super::coarse::unpack(&bytes)?
        } else {
            super::models::unpack(&bytes, b"OVC1")?
        };
        let parent = Self::read(&raw, index)?;
        ensure!(
            index.tiles.last().is_some_and(|t| t.parent_first + t.parent_count == parent.scene.instances.len()),
            "coarse range mismatch"
        );
        Ok(parent)
    }

    pub fn read(bytes: &[u8], index: &Index) -> Result<Self> {
        Self::with_scene(bytes, index, Sector::read_visual_page(bytes)?)
    }
    pub fn with_scene(bytes: &[u8], index: &Index, mut scene: Sector) -> Result<Self> {
        ensure!(scene.colliders.is_empty(), "visual placement contains collision geometry");
        let owners = if index.support.is_some() {
            // The cooker publishes one instanced placement per glTF node, in scene order.
            let size = u32::from_le_bytes(bytes.get(12..16).context("missing GLB JSON")?.try_into()?) as usize;
            let doc: Placements = serde_json::from_slice(bytes.get(20..20 + size).context("truncated GLB JSON")?)?;
            let roots = doc.scenes.get(doc.scene).context("missing visual scene")?;
            ensure!(roots.nodes.len() == scene.instances.len(), "unexpected placement count");
            for (&i, placement) in roots.nodes.iter().zip(&mut scene.instances) {
                if let Some(m) = doc.nodes.get(i).context("invalid visual placement")?.matrix {
                    let affine = bevy::prelude::Mat4::from_cols_array(&m);
                    ensure!(affine.is_finite(), "invalid affine placement");
                    placement.transform = affine;
                }
            }
            let owners: Vec<_> = roots
                .nodes
                .iter()
                .map(|&i| doc.nodes.get(i).map(|n| n.extras.osage_source_node).context("invalid visual placement"))
                .collect::<Result<_>>()?;
            ensure!(
                owners.len() == scene.instances.len() && owners.iter().all(|&n| n < index.source_parents.len()),
                "visual piece has no source-tree owner"
            );
            owners
        } else {
            Vec::new()
        };
        Ok(Self { scene, owners })
    }
}

#[cfg(test)]
mod texture_tests {
    use super::*;

    #[test]
    fn independent_authored_chains_can_share_a_core_image() {
        let core = File { file: "core.png".into(), ..Default::default() };
        let mut index = Index {
            texture_resources: vec![ImageFile { content: core, mime: "image/png".into() }],
            basisu_resources: ["near.ktx2", "far.ktx2"]
                .map(|file| BasisImage { content: File { file: file.into(), ..Default::default() }, source: 0 })
                .into(),
            textures: [Some(0), Some(1), None].map(|basisu| TextureRef { source: 0, basisu, sampler: None }).into(),
            image_files: [("core.png".into(), 0)].into(),
            tiles: vec![Tile {
                x: 0,
                z: 0,
                visual: Layer {
                    uses_images: vec![0],
                    image_choices: vec![(0, None), (0, Some(0)), (0, Some(1))],
                    ..Default::default()
                },
                physics: None,
                parent_first: 0,
                parent_count: 0,
            }],
            ..Default::default()
        };
        index.select_basisu(None);
        assert_eq!(index.image_slot("core.png", Some("far.ktx2")).unwrap(), 0);
        index.select_basisu(Some(4096));
        assert_eq!(index.tiles[0].visual.uses_images, [0, 1, 2]);
        assert_eq!(index.textures.iter().map(|t| t.source).collect::<Vec<_>>(), [1, 2, 0]);
        assert_eq!(index.image_slot("core.png", None).unwrap(), 0);
        assert_eq!(index.image_slot("core.png", Some("near.ktx2")).unwrap(), 1);
        assert_eq!(index.image_slot("core.png", Some("far.ktx2")).unwrap(), 2);
        assert!(index.image_slot("core.png", Some("absent.ktx2")).is_err());
        assert!(index.fallback_image(2));
        assert!(!index.fallback_image(2));
        assert_eq!(index.resource(2).file, "core.png");
        assert_eq!(index.resource(1).file, "near.ktx2");
    }
}
