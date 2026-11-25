// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Optional embedded Basis Universal textures. The renderer chooses an enabled format
//! before a worker decodes either image. The PNG/JPEG and KTX2 are never both uploaded.
use crate::texture::{Pixels, Sampler, Texture};
use anyhow::{Context, Result, ensure};
use base64::Engine as _;
use basis_universal::{DecodeFlags, LowLevelUastcTranscoder, SliceParametersUastc, TranscoderBlockFormat};
use bevy::prelude::StandardMaterial;
use bevy::{
    asset::{AssetId, RenderAssetUsages},
    prelude::{
        App, AppExit, Assets, Handle, Image, IntoScheduleConfigs, Local, MessageWriter, Plugin, PreUpdate, Res, ResMut,
        Resource, error, info,
    },
    render::{
        Render, RenderApp, RenderPlugin, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_resource::{Extent3d, TextureDimension, TextureFormat, TextureFormatFeatureFlags, TextureUsages},
        renderer::{RenderAdapter, RenderAdapterInfo, RenderDevice},
        settings::{WgpuFeatures, WgpuSettings},
        texture::GpuImage,
    },
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
use sha2::{Digest, Sha256};
use std::{borrow::Cow, collections::BTreeMap, io::Read, sync::Arc, thread, time::Instant};

const EXTENSION: &str = "KHR_texture_basisu";

#[derive(Clone)]
struct Encoded {
    mime: String,
    bytes: Arc<[u8]>,
    digest: [u8; 32],
}

#[derive(Default)]
pub(crate) struct Inputs(BTreeMap<usize, Arc<Encoded>>);

impl Inputs {
    fn image(&mut self, image: gltf::Image, buffer: &[u8]) -> Result<Arc<Encoded>> {
        if let Some(data) = self.0.get(&image.index()) {
            return Ok(data.clone());
        }
        let (mime, bytes): (_, Arc<[u8]>) = match image.source() {
            gltf::image::Source::View { view, mime_type } => {
                ensure!(view.stride().is_none(), "unexpected image stride");
                (mime_type, buffer[view.offset()..view.offset() + view.length()].into())
            }
            gltf::image::Source::Uri { uri, mime_type } => {
                let (prefix, data) =
                    uri.strip_prefix("data:").and_then(|v| v.split_once(',')).context("expected embedded image")?;
                let mime = prefix.strip_suffix(";base64").context("expected base64 image")?;
                ensure!(mime_type.is_none_or(|m| m == mime), "conflicting image type");
                (mime, base64::engine::general_purpose::STANDARD.decode(data)?.into())
            }
        };
        let item = Arc::new(Encoded { mime: mime.to_owned(), digest: Sha256::digest(&bytes).into(), bytes });
        self.0.insert(image.index(), item.clone());
        Ok(item)
    }
}

#[derive(Clone)]
pub(crate) struct Binding {
    fallback: Arc<Encoded>,
    alternative: Option<Arc<Encoded>>,
    alternative_error: Option<String>,
    sampler: Sampler,
}

impl Binding {
    pub(crate) fn read(
        doc: &gltf::Gltf,
        info: &gltf::texture::Info,
        buffer: &[u8],
        cache: &mut Inputs,
    ) -> Result<Self> {
        let texture = info.texture();
        let fallback = cache.image(texture.source(), buffer)?;
        ensure!(
            matches!(fallback.mime.as_str(), "image/png" | "image/jpeg"),
            "expected a PNG or JPEG base color fallback"
        );
        let alternative = texture
            .extension_value(EXTENSION)
            .map(|extension| {
                let index = extension
                    .get("source")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|n| usize::try_from(n).ok())
                    .context("invalid optional BasisU image source")?;
                let image = doc.images().nth(index).context("optional BasisU image missing")?;
                let image = cache.image(image, buffer)?;
                ensure!(image.mime == "image/ktx2", "optional BasisU source is not KTX2");
                Ok::<_, anyhow::Error>(image)
            })
            .transpose();
        let (alternative, alternative_error) = match alternative {
            Ok(image) => (image, None),
            Err(error) => (None, Some(error.to_string())),
        };
        Ok(Self { fallback, alternative, alternative_error, sampler: Sampler::from_gltf(texture.sampler()) })
    }

    fn prepare(self, cutoffs: Vec<Option<u8>>, bc7: bool, max_dimension: u32, trace: bool) -> Result<Vec<Prepared>> {
        let started = Instant::now();
        let mut reason = self.alternative_error;
        if bc7 && let Some(src) = &self.alternative {
            match bc7_image(&src.bytes, max_dimension, self.sampler) {
                Ok(image) => return Ok(vec![Prepared::new(image, src, cutoffs, started, trace, None)]),
                Err(error) => reason = Some(error.to_string()),
            }
        }
        let src = &self.fallback;
        let texture = Texture { pixels: Pixels::read(&src.bytes, &src.mime)?, sampler: self.sampler };
        Ok(cutoffs
            .into_iter()
            .map(|cutoff| {
                Prepared::new(texture.upload_image(cutoff), src, vec![cutoff], started, trace, reason.as_deref())
            })
            .collect())
    }
}

/// This profile transcodes straight sRGB RGB/RGBA UASTC from a 2D glTF base-color slot.
/// Limit each decompressed level to its declared block grid before calling the native codec.
pub(crate) fn bc7_image(bytes: &[u8], max_dimension: u32, sampler: Sampler) -> Result<Image> {
    let reader = ktx2::Reader::new(bytes).context("invalid KTX2 container")?;
    let h = reader.header();
    let (width, height) = (h.pixel_width, h.pixel_height);
    ensure!(
        width > 0
            && height > 0
            && width <= max_dimension
            && height <= max_dimension
            && width.is_multiple_of(4)
            && height.is_multiple_of(4),
        "unsupported KTX2 dimensions"
    );
    ensure!(
        h.format.is_none()
            && h.type_size == 1
            && h.face_count == 1
            && h.layer_count == 0
            && h.pixel_depth == 0
            && matches!(h.supercompression_scheme, None | Some(ktx2::SupercompressionScheme::Zstandard)),
        "expected 2D UASTC KTX2"
    );
    for (key, bytes) in reader.key_value_data() {
        let value = bytes.strip_suffix(&[0]).unwrap_or(bytes);
        ensure!(key != "KTXorientation" || value == b"rd", "unsupported KTX2 orientation");
        ensure!(key != "KTXswizzle" || value == b"rgba", "unsupported KTX2 swizzle");
    }
    let dfd = reader.basic_dfd().context("missing KTX2 data format descriptor")?;
    ensure!(
        reader.color_model() == Some(ktx2::ColorModel::UASTC)
            && reader.color_primaries() == Some(ktx2::ColorPrimaries::BT709)
            && reader.transfer_function() == Some(ktx2::TransferFunction::SRGB)
            && reader.is_alpha_premultiplied() == Some(false)
            && dfd.texel_block_dimensions.map(|n| n.get()) == [4, 4, 1, 1],
        "expected straight sRGB UASTC"
    );
    let [sample] = dfd.sample_information.as_slice() else { anyhow::bail!("expected RGB/RGBA UASTC") };
    ensure!(matches!(sample.channel_type, 0 | 3), "unsupported UASTC channel layout");
    let max_mips = 32 - width.max(height).leading_zeros();
    ensure!(h.level_count > 0 && h.level_count <= max_mips, "invalid KTX2 mip count");
    let level_count = if sampler.uses_mips() { h.level_count } else { 1 };
    let codec = LowLevelUastcTranscoder::new();
    let mut payload = Vec::new();
    for (mip, source) in reader.levels().take(level_count as usize).enumerate() {
        let (w, h) = ((width >> mip).max(1), (height >> mip).max(1));
        let (x, y) = (w.div_ceil(4), h.div_ceil(4));
        let expected = u64::from(x) * u64::from(y) * 16;
        ensure!(source.uncompressed_byte_length == expected, "invalid UASTC mip length at level {mip}");
        let input = if reader.header().supercompression_scheme.is_some() {
            let decoder = ruzstd::decoding::StreamingDecoder::new(source.data).context("invalid Zstandard mip")?;
            let mut decoded = Vec::new();
            decoder.take(expected + 1).read_to_end(&mut decoded)?;
            Cow::Owned(decoded)
        } else {
            Cow::Borrowed(source.data)
        };
        ensure!(input.len() as u64 == expected, "invalid uncompressed UASTC data at level {mip}");
        let params = SliceParametersUastc {
            original_width: w,
            original_height: h,
            num_blocks_x: x,
            num_blocks_y: y,
            has_alpha: sample.channel_type == 3,
        };
        let data = codec
            .transcode_slice(&input, params, DecodeFlags::HIGH_QUALITY, TranscoderBlockFormat::BC7)
            .map_err(|error| anyhow::anyhow!("UASTC mip {mip} to BC7: {error:?}"))?;
        ensure!(data.len() as u64 == expected, "unexpected BC7 mip length at level {mip}");
        payload.extend_from_slice(&data);
    }
    let mut image = Image::new_uninit(
        Extent3d { width, height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        TextureFormat::Bc7RgbaUnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.mip_level_count = level_count;
    image.sampler = sampler.descriptor();
    image.data = Some(payload);
    Ok(image)
}

const LABEL: &str = "Ville-Paris embedded base color";

struct Prepared {
    image: Image,
    cutoffs: Vec<Option<u8>>,
    diagnostics: Option<serde_json::Value>,
}

impl Prepared {
    fn new(
        mut image: Image,
        input: &Encoded,
        cutoffs: Vec<Option<u8>>,
        started: Instant,
        trace: bool,
        reason: Option<&str>,
    ) -> Self {
        if let Some(error) = reason {
            bevy::log::warn!("using embedded core image: {error}");
        }
        let diagnostics = trace.then(|| {
            image.texture_descriptor.label = Some(LABEL);
            let elapsed_us = started.elapsed().as_micros();
            serde_json::json!({
                "mime":input.mime, "sourceSha256":crate::bench::hex(&input.digest),
                "format":format!("{:?}", image.texture_descriptor.format),
                "preparedBytes":image.data.as_ref().map_or(0,Vec::len),
                "dataSha256":image.data.as_ref().map(|d|crate::bench::sha(d)),
                "worker":thread::current().name().unwrap_or("unnamed"),
                "prepareUs":elapsed_us, "fallbackReason":reason,
            })
        });
        Self { image, cutoffs, diagnostics }
    }
}

type SourceKey = ([u8; 32], Option<[u8; 32]>, Sampler);
type MaterialsByCutoff = BTreeMap<Option<u8>, Vec<Handle<StandardMaterial>>>;

struct Request {
    binding: Binding,
    placeholder: Handle<Image>,
    materials: MaterialsByCutoff,
}

struct Job {
    task: Task<Result<Vec<Prepared>>>,
    placeholder: Handle<Image>,
    materials: MaterialsByCutoff,
}

#[derive(Resource, Default)]
pub(crate) struct Queue {
    requests: BTreeMap<SourceKey, Request>,
    jobs: Vec<Job>,
    bc7: bool,
    max_dimension: u32,
    trace: bool,
}

impl Queue {
    pub(crate) fn configure(&mut self, device: &RenderDevice, adapter: &RenderAdapter) {
        let limit = bc7_limit(device, adapter);
        self.bc7 = limit.is_some();
        self.max_dimension = limit.unwrap_or(0);
        self.trace = std::env::var_os("OSAGE_TEXTURE_TRACE").is_some_and(|v| v == "1");
    }

    pub(crate) fn register(
        &mut self,
        binding: &Binding,
        cutoff: Option<u8>,
        material: &Handle<StandardMaterial>,
        images: &Assets<Image>,
    ) -> Handle<Image> {
        let key = (
            binding.fallback.digest,
            binding.alternative.as_ref().filter(|_| self.bc7).map(|a| a.digest),
            binding.sampler,
        );
        let request = self.requests.entry(key).or_insert_with(|| Request {
            binding: binding.clone(),
            placeholder: images.reserve_handle(),
            materials: BTreeMap::new(),
        });
        let cutoff = cutoff.filter(|_| binding.sampler.uses_mips());
        request.materials.entry(cutoff).or_default().push(material.clone());
        request.placeholder.clone()
    }

    pub(crate) fn start(&mut self) {
        let (bc7, dim, trace) = (self.bc7, self.max_dimension, self.trace);
        for request in std::mem::take(&mut self.requests).into_values() {
            let cutoffs = request.materials.keys().copied().collect();
            let task =
                AsyncComputeTaskPool::get().spawn(async move { request.binding.prepare(cutoffs, bc7, dim, trace) });
            self.jobs.push(Job { task, placeholder: request.placeholder, materials: request.materials });
        }
    }

    pub(crate) fn pending(&self) -> bool {
        !self.jobs.is_empty()
    }
}

#[derive(Clone, Default, Resource, ExtractResource)]
#[extract_app(RenderApp)]
struct Published(Vec<AssetId<Image>>);

fn publish(
    mut queue: ResMut<Queue>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut published: ResMut<Published>,
    mut exit: MessageWriter<AppExit>,
) {
    let bc7 = queue.bc7;
    queue.jobs.retain_mut(|job| {
        let Some(result) = block_on(poll_once(&mut job.task)) else { return true };
        let result = result.and_then(|prepared| {
            for (index, output) in prepared.into_iter().enumerate() {
                let handle = if index == 0 { job.placeholder.clone() } else { images.reserve_handle() };
                for cutoff in &output.cutoffs {
                    for material in &job.materials[cutoff] {
                        if let Some(mut value) = materials.get_mut(material) {
                            value.base_color_texture = Some(handle.clone());
                        }
                    }
                }
                if let Some(mut row) = output.diagnostics {
                    row["adapterBc7"] = bc7.into();
                    row["materialSlots"] =
                        output.cutoffs.iter().map(|cut| job.materials[cut].len()).sum::<usize>().into();
                    info!("embedded texture worker {row}");
                    published.0.push(handle.id());
                }
                images.insert(handle.id(), output.image)?;
            }
            Ok(())
        });
        if let Err(error) = result {
            error!("embedded texture: {error:#}");
            exit.write(AppExit::error());
        }
        false
    });
}

fn observe_gpu(
    published: Res<Published>,
    textures: Res<RenderAssets<GpuImage>>,
    device: Res<RenderDevice>,
    adapter: Res<RenderAdapterInfo>,
    mut done: Local<usize>,
) {
    for id in published.0.iter().skip(*done) {
        let Some(image) = textures.get(*id) else { break };
        let tex = &image.texture;
        let (w, h) = (tex.width(), tex.height());
        let format = tex.format();
        let bytes = payload_bytes(format, w, h, tex.mip_level_count());
        let alloc = device.wgpu_device().generate_allocator_report();
        let named_allocations = alloc
            .as_ref()
            .map(|report| report.allocations.iter().filter(|a| a.name == LABEL).map(|a| a.size).collect::<Vec<_>>());
        info!(
            "embedded texture resident {}",
            serde_json::json!({
                "gpu":adapter.name,"format":format!("{format:?}"),"width":w,"height":h,"levels":tex.mip_level_count(),
                "payloadBytes":bytes,"namedAllocations":named_allocations,"deviceAllocatedBytes":alloc.map(|a|a.total_allocated_bytes)
            })
        );
        *done += 1;
    }
}

pub(crate) fn bc7_limit(device: &RenderDevice, adapter: &RenderAdapter) -> Option<u32> {
    let caps = adapter.get_texture_format_features(TextureFormat::Bc7RgbaUnormSrgb);
    (device.features().contains(WgpuFeatures::TEXTURE_COMPRESSION_BC)
        && caps.allowed_usages.contains(TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST)
        && caps.flags.contains(TextureFormatFeatureFlags::FILTERABLE))
    .then(|| device.limits().max_texture_dimension_2d)
}

pub(crate) fn payload_bytes(format: TextureFormat, width: u32, height: u32, levels: u32) -> u64 {
    let (bw, bh) = format.block_dimensions();
    (0..levels)
        .map(|m| {
            u64::from((width >> m).max(1).div_ceil(bw))
                * u64::from((height >> m).max(1).div_ceil(bh))
                * u64::from(format.block_copy_size(None).expect("color format"))
        })
        .sum()
}

/// Mask only the optional BC capability when reproducing the portable fallback on a BC adapter.
pub(crate) fn renderer(offscreen: bool) -> RenderPlugin {
    let mut settings = WgpuSettings::default();
    if std::env::var_os("OSAGE_DISABLE_BC").is_some_and(|value| value == "1") {
        settings.disabled_features =
            Some(WgpuFeatures::TEXTURE_COMPRESSION_BC | WgpuFeatures::TEXTURE_COMPRESSION_BC_SLICED_3D);
    }
    RenderPlugin { render_creation: settings.into(), synchronous_pipeline_compilation: offscreen, ..Default::default() }
}

pub(crate) struct CompressionPlugin;

impl Plugin for CompressionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Queue>()
            .init_resource::<Published>()
            .add_plugins(ExtractResourcePlugin::<Published>::default())
            .add_systems(PreUpdate, publish);
        app.sub_app_mut(RenderApp).add_systems(Render, observe_gpu.in_set(RenderSystems::Cleanup));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The converter's original 4 × 4 panel and its two authored lower mips. The
    // independently transcoded block digest is from Khronos KTX-Software 4.4.2.
    const PANEL: &str = concat!(
        "q0tUWCAyMLsNChoKAAAAAAEAAAAEAAAABAAAAAAAAAAAAAAAAQAAAAMAAAACAAAAmAAAACwAAADEAAAA",
        "cAAAAAAAAAAAAAAAAAAAAAAAAABmAQAAAAAAABkAAAAAAAAAEAAAAAAAAABNAQAAAAAAABkAAAAAAAAA",
        "EAAAAAAAAAA0AQAAAAAAABkAAAAAAAAAEAAAAAAAAAAsAAAAAAAAAAIAKACmAQIAAwMAABAAAAAAAAAA",
        "AAB/AwAAAAAAAAAA/////ywAAABLVFh3cml0ZXIAa3R4IGNyZWF0ZSB2NC40LjIgLyBsaWJrdHggdjQu",
        "NC4yADoAAABLVFh3cml0ZXJTY1BhcmFtcwAtLXVhc3RjLXF1YWxpdHkgMiAtLXRocmVhZHMgMSAtLXpz",
        "dGQgMTAAAAAotS/9IBCBAAAXHgUBKOYEAAAAAAAAAAAAKLUv/SAQgQAAFwEdHDgQnQMAAAAAAAAAACi1",
        "L/0gEIEAAFCCLCJUrQoFMDNEd7i4/M8=",
    );
    const FALLBACK: &str = concat!(
        "iVBORw0KGgoAAAANSUhEUgAAAAQAAAAECAYAAACp8Z5+AAAAO0lEQVR4nBXKQQ0AQAgDwYpAxIlABEpQ",
        "ghLkYIhbHpsmk2paO22afuTSFFBGQB0kkDzysQcBBBA8wvcDG4Qdyj0Z9RUAAAAASUVORK5CYII=",
    );

    fn encoded(mime: &str, bytes: Vec<u8>) -> Arc<Encoded> {
        Arc::new(Encoded { mime: mime.to_owned(), digest: Sha256::digest(&bytes).into(), bytes: bytes.into() })
    }

    #[test]
    fn authored_mips_match_the_independent_bc7_transcoder() {
        let input = base64::engine::general_purpose::STANDARD.decode(PANEL).unwrap();
        let image = bc7_image(&input, 4096, Sampler::default()).unwrap();
        assert_eq!(image.texture_descriptor.mip_level_count, 3);
        assert_eq!(image.texture_descriptor.format, TextureFormat::Bc7RgbaUnormSrgb);
        assert_eq!(
            crate::bench::sha(image.data.as_ref().unwrap()),
            "118c0e3646661be0617c0b12fa14ff4f871f407d309d9e57988b335e90cd1f7c"
        );
        let sampler = serde_json::from_str(r#"{"minFilter":9729}"#).unwrap();
        let single = bc7_image(&input, 4096, sampler).unwrap();
        assert_eq!(single.texture_descriptor.mip_level_count, 1);
        assert_eq!(single.data.unwrap(), image.data.unwrap()[..16]);
    }

    #[test]
    fn share_authored_levels_across_masks_and_preserve_core_mips_on_fallback() {
        let ktx = base64::engine::general_purpose::STANDARD.decode(PANEL).unwrap();
        let png = base64::engine::general_purpose::STANDARD.decode(FALLBACK).unwrap();
        let binding = Binding {
            fallback: encoded("image/png", png),
            alternative: Some(encoded("image/ktx2", ktx.clone())),
            alternative_error: None,
            sampler: Sampler::default(),
        };
        let cuts = vec![Some(102), Some(153)];
        let authored = binding.clone().prepare(cuts.clone(), true, 4096, false).unwrap();
        assert_eq!(authored.len(), 1);
        assert_eq!(authored[0].cutoffs, cuts);
        let core = binding.clone().prepare(cuts.clone(), false, 4096, false).unwrap();
        assert_eq!(core.len(), 2);
        let mut short = ktx;
        let off = u64::from_le_bytes(short[80..88].try_into().unwrap()) as usize;
        short[off..off + 4].fill(0);
        let invalid = Binding { alternative: Some(encoded("image/ktx2", short)), ..binding };
        let fallback = invalid.prepare(cuts, true, 4096, true).unwrap();
        for (i, (control, actual)) in core.iter().zip(fallback).enumerate() {
            assert_eq!(control.cutoffs, [Some([102, 153][i])]);
            assert_eq!(actual.image.texture_descriptor.format, TextureFormat::Rgba8UnormSrgb);
            assert_eq!(actual.image.data, control.image.data);
            assert!(actual.diagnostics.unwrap()["fallbackReason"].as_str().is_some());
        }
    }
}
