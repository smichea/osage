// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::source::{Source, Ticket};
use super::tiles::{Index, Visual};
use crate::scene::{Instance, Primitive, Sector};
use anyhow::{Context, Result, ensure};
use bevy::prelude::{Mat4, Quat, Vec3};
use flate2::read::ZlibDecoder;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    sync::Arc,
};

pub fn unpack(bytes: &[u8], tag: &[u8; 4]) -> Result<Vec<u8>> {
    ensure!(bytes.get(..4) == Some(tag), "invalid packed visual signature");
    let size = u32::from_le_bytes(bytes.get(4..8).context("missing packed length")?.try_into()?) as usize;
    let mut plain = Vec::new();
    ZlibDecoder::new(&bytes[8..]).take(size as u64 + 1).read_to_end(&mut plain)?;
    ensure!(plain.len() == size, "packed visual length mismatch");
    Ok(plain)
}

// The worker owns the glTF decoder and drops its linear GLB buffer before delivery.
// Joined tickets share immutable mesh arrays. Only the simulation can install references.
pub(super) struct DecodedModel {
    meshes: Vec<Arc<Vec<Primitive>>>,
    instances: Vec<Instance>,
    triangles: usize,
    has_other_channels: bool,
}

impl DecodedModel {
    pub(super) fn read(bytes: &[u8]) -> Result<Self> {
        let plain = unpack(bytes, b"OMD1")?;
        let scene = Sector::read_visual_page(&plain)?;
        Ok(Self {
            meshes: scene.meshes.into_iter().map(Arc::new).collect(),
            instances: scene.instances,
            triangles: scene.triangles,
            has_other_channels: !scene.colliders.is_empty() || !scene.supports.is_empty() || scene.object.is_some(),
        })
    }
}

#[derive(Default, Deserialize)]
pub struct Affine {
    pub matrix: Option<[f32; 16]>,
    translation: Option<[f32; 3]>,
    rotation: Option<[f32; 4]>,
    scale: Option<[f32; 3]>,
}
impl Affine {
    pub(super) fn matrix(&self) -> Result<Mat4> {
        let mat = match self.matrix {
            Some(m) => Mat4::from_cols_array(&m),
            None => Mat4::from_scale_rotation_translation(
                self.scale.map_or(Vec3::ONE, Vec3::from_array),
                self.rotation.map_or(Quat::IDENTITY, Quat::from_array),
                self.translation.map_or(Vec3::ZERO, Vec3::from_array),
            ),
        };
        ensure!(mat.is_finite(), "invalid model placement");
        Ok(mat)
    }
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Record {
    Local(usize),
    Shared((usize, usize, Affine)),
}
pub enum Draw {
    Hidden,
    Local(usize),
    Shared(usize, Mat4),
}
pub struct Page {
    pub image_bindings: Vec<(usize, crate::scene::ImageBinding)>,
    pub(super) local: Option<Visual>,
    pub owners: Vec<usize>,
    pub draws: Vec<Draw>,
    pub revision: u64,
    pub fallback: BTreeSet<usize>,
    pub triangles: usize,
    pub local_buffer_bytes: usize,
}
#[derive(Default, Serialize)]
pub struct ImageIo {
    pub resident_reuses: usize,
    pub worker_prepared: usize,
    pub simulation_decode_us: u64,
    pub reads: usize,
    pub read_bytes: usize,
    pub rejected: usize,
    pub evictions: usize,
    pub peak_images: usize,
    pub resident_bytes: usize,
    pub peak_bytes: usize,
}

#[derive(Default)]
pub struct Library {
    images: BTreeMap<usize, Arc<crate::texture::Pixels>>,
    image_users: BTreeMap<String, (Arc<crate::texture::Pixels>, usize)>,
    pub image_io: ImageIo,
    pub(super) models: Vec<Option<Arc<Vec<Primitive>>>>,
    counts: Vec<usize>,
    textured_slots: Vec<BTreeSet<usize>>,
    pub encoded_bytes: usize,
    pub decoded_bytes: usize,
    pub io: ModelIo,
}
#[derive(Default, Serialize)]
pub struct ModelIo {
    pub worker_prepared: usize,
    pub simulation_decode_us: u64,
    pub simulation_publish_us: u64,
    pub reads: usize,
    pub read_bytes: usize,
    pub rejected: usize,
    pub evictions: usize,
    pub resident_models: usize,
    pub peak_models: usize,
    pub resident_buffer_bytes: usize,
    pub peak_buffer_bytes: usize,
}
pub fn sector_buffer_bytes(s: &Sector) -> usize {
    s.meshes.iter().map(|m| buffer_bytes(m)).sum()
}
fn buffer_bytes(mesh: &[Primitive]) -> usize {
    mesh.iter()
        .map(|p| {
            12 * (p.positions.len() + p.normals.len())
                + 4 * p.indices.len()
                + p.colors.as_ref().map_or(0, |v| 16 * v.len())
                + p.colors_1.as_ref().map_or(0, |v| 16 * v.len())
                + p.source_colors.as_ref().map_or(0, |v| 16 * v.len())
                + p.uv.as_ref().map_or(0, |v| 8 * v.len())
        })
        .sum()
}
impl ModelIo {
    fn install(&mut self, models: usize, bytes: usize) {
        self.resident_models += models;
        self.resident_buffer_bytes += bytes;
        self.peak_models = self.peak_models.max(self.resident_models);
        self.peak_buffer_bytes = self.peak_buffer_bytes.max(self.resident_buffer_bytes);
    }
}
impl Library {
    pub fn load(index: &mut Index, dir: &Source) -> Result<Self> {
        let mut library = Self::default();
        if let Some(file) = &index.models {
            let embedded = index.inline_models.take();
            let external = embedded.is_none();
            let bytes = match embedded {
                Some(bytes) => bytes,
                None => file.content.read(dir)?,
            };
            let raw = unpack(&bytes, b"OML1")?;
            let scene = Sector::read_visual_page(&raw)?;
            ensure!(
                scene.meshes.len() == file.source_meshes.len()
                    && scene.instances.iter().enumerate().all(|(i, n)| n.mesh == i && n.transform == Mat4::IDENTITY)
                    && scene.instances.len() == scene.meshes.len()
                    && scene.colliders.is_empty(),
                "invalid model library"
            );
            library.counts = scene.meshes.iter().map(|m| m.iter().map(|p| p.indices.len() / 3).sum()).collect();
            library.io.install(scene.meshes.len(), scene.meshes.iter().map(|m| buffer_bytes(m)).sum());
            if external {
                library.io.reads = 1;
                library.io.read_bytes = file.content.bytes;
            }
            library.models = scene.meshes.into_iter().map(|mesh| Some(Arc::new(mesh))).collect();
            library.encoded_bytes = file.content.bytes;
            library.decoded_bytes = raw.len();
        }
        if index.demand() {
            library.counts = index.model_variants.iter().map(|v| index.model_resources[v.resource].triangles).collect();
            library.models = (0..index.model_resources.len()).map(|_| None).collect();
            library.textured_slots = vec![BTreeSet::new(); index.model_resources.len()];
            for variant in &index.model_variants {
                for (slot, &material) in variant.materials.iter().enumerate() {
                    if index.material_palette[material].pbr.texture.is_some() {
                        library.textured_slots[variant.resource].insert(slot);
                    }
                }
            }
        }
        Ok(library)
    }
    pub fn fetch(&mut self, i: usize, index: &Index, dir: &Source, ticket: Option<&Ticket>) -> Result<()> {
        let image_id = i.checked_sub(index.model_resources.len());
        let (reads, byte_count) = if image_id.is_some() {
            (&mut self.image_io.reads, &mut self.image_io.read_bytes)
        } else {
            (&mut self.io.reads, &mut self.io.read_bytes)
        };
        *reads += 1;
        let file = index.resource(i);
        let raw;
        let bytes = if let Some(t) = ticket {
            *byte_count += t.encoded_len()?;
            if image_id.is_none()
                && let Some(model) = t.model()?
            {
                self.install_model(i, index, model)?;
                self.io.worker_prepared += 1;
                return Ok(());
            }
            t.bytes()?
        } else {
            raw = dir.raw(file)?;
            *byte_count += raw.len();
            file.check(&raw)?;
            &raw
        };
        if let Some(id) = image_id {
            let image = match ticket.map(Ticket::image).transpose()?.flatten() {
                Some(pixels) => {
                    self.image_io.worker_prepared += 1;
                    pixels
                }
                None => {
                    let start = std::time::Instant::now();
                    let pixels = crate::texture::Pixels::read_resource(
                        bytes,
                        &index.texture_resources[id].mime,
                        index.bc7_limit,
                    )?;
                    self.image_io.simulation_decode_us += start.elapsed().as_micros() as u64;
                    pixels
                }
            };
            return self.install_image(id, &file.sha256, image);
        }
        let start = std::time::Instant::now();
        let decoded = DecodedModel::read(bytes);
        self.io.simulation_decode_us += start.elapsed().as_micros() as u64;
        self.install_model(i, index, &decoded?)
    }

    fn install_image(&mut self, id: usize, hash: &str, pixels: Arc<crate::texture::Pixels>) -> Result<()> {
        ensure!(!self.images.contains_key(&id), "image already resident");
        let (image, users) = self.image_users.entry(hash.to_owned()).or_insert_with(|| {
            self.image_io.resident_bytes += pixels.byte_len();
            self.image_io.peak_bytes = self.image_io.peak_bytes.max(self.image_io.resident_bytes);
            (pixels, 0)
        });
        *users += 1;
        self.images.insert(id, image.clone());
        self.image_io.peak_images = self.image_io.peak_images.max(self.image_users.len());
        Ok(())
    }

    /// A core-only material may already hold the fallback for this failed optional image.
    pub fn reuse_fallback(&mut self, slot: usize, index: &Index) -> Result<bool> {
        let id = slot - index.model_resources.len();
        let hash = &index.texture_resources[id].content.sha256;
        let Some((pixels, _)) = self.image_users.get(hash) else { return Ok(false) };
        self.install_image(id, hash, pixels.clone())?;
        self.image_io.resident_reuses += 1;
        Ok(true)
    }

    fn install_model(&mut self, i: usize, index: &Index, decoded: &DecodedModel) -> Result<()> {
        let start = std::time::Instant::now();
        let r = &index.model_resources[i];
        ensure!(
            decoded.meshes.len() == 1
                && matches!(decoded.instances.as_slice(), [instance] if instance.mesh == 0 && instance.transform == Mat4::IDENTITY)
                && decoded.triangles == r.triangles
                && !decoded.has_other_channels
                && self.models[i].is_none(),
            "invalid shared model asset"
        );
        let mesh = &decoded.meshes[0];
        ensure!(mesh.len() == r.primitives, "incompatible material slots");
        ensure!(
            mesh.iter().all(|p| p.texture.is_none() && p.embedded_texture.is_none()),
            "shared geometry contains an undeclared texture"
        );
        ensure!(self.textured_slots[i].iter().all(|&slot| mesh[slot].uv.is_some()), "shared texture needs TEXCOORD_0");
        self.io.install(1, buffer_bytes(mesh));
        self.models[i] = Some(mesh.clone());
        self.io.simulation_publish_us += start.elapsed().as_micros() as u64;
        Ok(())
    }

    pub fn material(&self, index: &Index, i: usize) -> Result<crate::view::MaterialFactors> {
        let m = &index.material_palette[i];
        let texture = match &m.pbr.texture {
            None => None,
            Some(info) => {
                let t = &index.textures[info.index];
                let pixels = self.images.get(&t.source).context("material image is not resident")?.clone();
                Some(crate::texture::Texture {
                    pixels,
                    sampler: t.sampler.map(|n| index.samplers[n]).unwrap_or_default(),
                })
            }
        };
        Ok(m.factors(texture))
    }

    /// Optional full-buffer oracle, run after the last measured simulation tick.
    pub fn fingerprints(&self, index: &Index, pages: &BTreeMap<usize, Page>) -> serde_json::Value {
        use serde_json::json;
        use sha2::{Digest, Sha256};

        fn digest(words: impl Iterator<Item = [u8; 4]>) -> String {
            let mut hash = Sha256::new();
            for word in words {
                hash.update(word);
            }
            crate::bench::hex(&hash.finalize())
        }
        fn channel<const N: usize>(values: &[[f32; N]]) -> String {
            digest(values.iter().flatten().map(|n| n.to_le_bytes()))
        }

        let mut placements: BTreeMap<usize, BTreeSet<(usize, usize)>> = BTreeMap::new();
        for page in pages.values() {
            for (draw, &owner) in page.draws.iter().zip(&page.owners) {
                if let Draw::Shared(variant, _) = draw
                    && let Some(v) = index.model_variants.get(*variant)
                {
                    placements.entry(v.resource).or_default().insert((owner, *variant));
                }
            }
        }
        let mut result = Vec::new();
        for (id, resource) in index.model_resources.iter().enumerate() {
            let Some(mesh) = self.models[id].as_ref() else { continue };
            let parts: Vec<_> = mesh
                .iter()
                .map(|p| {
                    json!({
                        "POSITION":channel(&p.positions), "NORMAL":channel(&p.normals),
                        "TEXCOORD_0":p.uv.as_deref().map(channel),
                        "COLOR_0":p.colors.as_deref().map(channel), "COLOR_1":p.colors_1.as_deref().map(channel),
                        "_OSAGE_SOURCE_COLOR_0":p.source_colors.as_deref().map(channel),
                        "indices":digest(p.indices.iter().map(|i| i.to_le_bytes())),
                        "vertices":p.positions.len(), "triangles":p.indices.len()/3,
                    })
                })
                .collect();
            result.push(json!({"resource_sha256":resource.content.sha256,"parts":parts,
                "owner_variants":placements.get(&id).into_iter().flatten().collect::<Vec<_>>()}));
        }
        json!(result)
    }

    pub fn remove(&mut self, i: usize, index: &Index) -> Result<()> {
        if let Some(id) = i.checked_sub(index.model_resources.len()) {
            let pixels = self.images.remove(&id).context("evicting absent image")?;
            let hash = &index.texture_resources[id].content.sha256;
            let (_, users) = self.image_users.get_mut(hash).context("missing resident image")?;
            *users -= 1;
            if *users == 0 {
                self.image_users.remove(hash);
                self.image_io.resident_bytes -= pixels.byte_len();
            }
            self.image_io.evictions += 1;
            return Ok(());
        }
        let model = self.models[i].take().context("evicting absent model")?;
        self.io.resident_models -= 1;
        self.io.resident_buffer_bytes -= buffer_bytes(&model);
        self.io.evictions += 1;
        Ok(())
    }
    pub fn len(&self) -> usize {
        self.models.len()
    }
}

impl Page {
    pub fn needs_parent(&self, owner: Option<&usize>) -> bool {
        owner.is_some_and(|id| self.fallback.contains(id))
    }
    pub fn read(bytes: &[u8], idx: &Index, lib: &Library) -> Result<Self> {
        if !idx.shared() {
            let v = Visual::read(bytes, idx)?;
            return Ok(Self {
                revision: 0,
                fallback: BTreeSet::new(),
                image_bindings: vec![],
                owners: v.owners.clone(),
                draws: (0..v.scene.instances.len()).map(Draw::Local).collect(),
                triangles: v.scene.triangles,
                local_buffer_bytes: sector_buffer_bytes(&v.scene),
                local: Some(v),
            });
        }
        let plain = unpack(bytes, b"OVP1")?;
        let size = u32::from_le_bytes(plain.get(..4).context("missing placement length")?.try_into()?) as usize;
        let cut = 4usize.checked_add(size).context("placement length overflow")?;
        let records: Vec<Record> = serde_json::from_slice(plain.get(4..cut).context("truncated placements")?)?;
        let mut names = Vec::new();
        let local = if plain.len() > cut && idx.demand() {
            let raw = &plain[cut..];
            Some(Visual::with_scene(raw, idx, Sector::read_tile(raw, &mut names)?)?)
        } else if plain.len() > cut {
            Some(Visual::read(&plain[cut..], idx)?)
        } else {
            None
        };
        let image_bindings = names
            .into_iter()
            .map(|binding| Ok((idx.image_slot(&binding.file, binding.alternative.as_deref())?, binding)))
            .collect::<Result<Vec<_>>>()?;
        let mut next = 0;
        let mut owners = Vec::with_capacity(records.len());
        let mut draws = Vec::with_capacity(records.len());
        let mut triangles = local.as_ref().map_or(0, |v| v.scene.triangles);
        for record in records {
            match record {
                Record::Local(i) => {
                    ensure!(i == next, "local placement sequence mismatch");
                    owners.push(*local.as_ref().and_then(|v| v.owners.get(i)).context("missing local placement")?);
                    draws.push(Draw::Local(i));
                    next += 1;
                }
                Record::Shared((i, owner, pose)) => {
                    ensure!(owner < idx.source_parents.len(), "shared model has unknown owner");
                    triangles += *lib.counts.get(i).context("unknown shared model")?;
                    owners.push(owner);
                    draws.push(Draw::Shared(i, pose.matrix()?));
                }
            }
        }
        ensure!(next == local.as_ref().map_or(0, |v| v.owners.len()), "unreferenced local geometry");
        Ok(Self {
            revision: 0,
            fallback: BTreeSet::new(),
            image_bindings,
            local_buffer_bytes: local.as_ref().map_or(0, |v| sector_buffer_bytes(&v.scene)),
            local,
            owners,
            draws,
            triangles,
        })
    }
    pub fn bind_images(&mut self, library: &Library) -> Result<()> {
        for (id, binding) in self.image_bindings.drain(..) {
            let pixels = library.images.get(&id).context("tile image has not arrived")?.clone();
            let scene = &mut self.local.as_mut().context("missing local mesh")?.scene;
            scene.meshes[binding.mesh][binding.part].texture =
                Some(crate::texture::Texture { pixels, sampler: binding.sampler });
        }
        Ok(())
    }

    pub fn publish(
        self,
        dest: &mut Sector,
        lib: &mut Library,
        published: &mut BTreeMap<usize, usize>,
        idx: &Index,
        palette: &mut crate::view::InstanceMaterials,
    ) -> Result<()> {
        let offset = dest.meshes.len();
        let local_instances = if let Some(page) = self.local {
            dest.meshes.extend(page.scene.meshes);
            page.scene.instances
        } else {
            vec![]
        };
        for draw in self.draws {
            let instance = match draw {
                Draw::Hidden => continue,
                Draw::Local(i) => {
                    let p = &local_instances[i];
                    Instance { mesh: offset + p.mesh, transform: p.transform }
                }
                Draw::Shared(i, transform) => {
                    let i = if let Some(v) = idx.model_variants.get(i) {
                        palette.assignments.insert(dest.instances.len(), i);
                        v.resource
                    } else {
                        i
                    };
                    let mesh = if let Some(&m) = published.get(&i) {
                        m
                    } else {
                        let m = dest.meshes.len();
                        let parts = lib.models[i].take().context("unavailable shared draw model")?;
                        dest.meshes.push(Arc::unwrap_or_clone(parts));
                        published.insert(i, m);
                        m
                    };
                    Instance { mesh, transform }
                }
            };
            dest.instances.push(instance);
        }
        Ok(())
    }
}
