// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The traced light's grain filtered out (D-095). Solari's light, without a
//! learned denoiser (DLSS ray reconstruction is a proprietary SDK, out of
//! bounds here), leaves grain: sparkles on wet asphalt, a speckled kart.
//! The light — the picture divided by each surface's colour — is first
//! averaged with what the same surface showed over the frames before
//! (D-097: reprojected by the motion vectors, the history dropped where a
//! surface was just uncovered), then an edge-avoiding à-trous filter
//! (`denoise.wesl`) smooths it across pixels of the same surface (alike in
//! facing, plane and light), in three passes; after the opaque pass and
//! before the see-through things are drawn over it.
//!
//! Only the T4 camera is filtered, on hardware that traces rays.
//! `OSAGE_DENOISE=0` leaves the grain, to compare; `OSAGE_DENOISE=force`
//! filters the deferred picture of hardware without, to see the filter
//! keep edges where there is no grain.

use bevy::{
    asset::{embedded_asset, load_embedded_asset},
    core_pipeline::{
        core_3d::{main_opaque_pass_3d, main_transparent_pass_3d},
        prepass::{
            DeferredPrepassDoubleBuffer, DepthPrepassDoubleBuffer, PreviousViewData, PreviousViewUniformOffset,
            PreviousViewUniforms, ViewPrepassTextures,
        },
        schedule::{Core3d, Core3dSystems},
    },
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup, RenderSystems,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        render_resource::{
            BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, CachedComputePipelineId,
            ComputePassDescriptor, ComputePipelineDescriptor, Extent3d, PipelineCache, ShaderStages,
            StorageTextureAccess, Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType,
            TextureUsages, TextureView, TextureViewDescriptor,
            binding_types::{texture_2d, texture_depth_2d, texture_storage_2d, uniform_buffer},
        },
        renderer::{RenderContext, RenderDevice, ViewQuery},
        texture::{CachedTexture, TextureCache},
        view::{ViewTarget, ViewUniform, ViewUniformOffset, ViewUniforms},
    },
    solari::SolariPlugins,
};

/// A camera whose picture is filtered.
#[derive(Component, Clone, Copy, Default, ExtractComponent)]
#[extract_app(RenderApp)]
pub struct Denoised;

pub struct DenoisePlugin;

impl Plugin for DenoisePlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "denoise.wesl");
        app.add_plugins(ExtractComponentPlugin::<Denoised>::default());
    }

    fn finish(&self, app: &mut App) {
        let asked = std::env::var("OSAGE_DENOISE").unwrap_or_default();
        if asked == "0" {
            info!("traced light: not filtered (OSAGE_DENOISE=0)");
            return;
        }
        let Some(render) = app.get_sub_app_mut(RenderApp) else { return };
        let features = render.world().resource::<RenderDevice>().features();
        let traced = features.contains(SolariPlugins::required_wgpu_features());
        if !traced && asked != "force" {
            return;
        }
        // Forced on hardware without ray queries, the camera keeps last
        // frame's G-buffer itself (Solari's ReSTIR keeps it otherwise).
        if !traced {
            app.add_systems(PostUpdate, keep_last_frame);
        }
        let Some(render) = app.get_sub_app_mut(RenderApp) else { return };
        render
            .add_systems(RenderStartup, pipelines)
            .add_systems(Render, (textures, histories).in_set(RenderSystems::PrepareResources))
            .add_systems(
                Core3d,
                denoise.after(main_opaque_pass_3d).before(main_transparent_pass_3d).in_set(Core3dSystems::MainPass),
            );
    }
}

fn keep_last_frame(
    mut commands: Commands,
    cameras: Query<Entity, (With<Denoised>, Without<DeferredPrepassDoubleBuffer>)>,
) {
    for camera in &cameras {
        commands.entity(camera).insert((DeferredPrepassDoubleBuffer, DepthPrepassDoubleBuffer));
    }
}

#[derive(Resource)]
struct Pipelines {
    layout: BindGroupLayoutDescriptor,
    /// The passes, taps 1, 2 and 4 pixels apart.
    steps: [CachedComputePipelineId; 3],
    temporal: BindGroupLayoutDescriptor,
    accumulate: CachedComputePipelineId,
}

/// A view's light averaged over the frames before, and how many frames:
/// two pictures kept from frame to frame, one read while the other is
/// written, their roles swapped each frame.
#[derive(Component)]
struct History {
    pictures: [(Texture, TextureView); 2],
    /// The one read this frame.
    read: usize,
}

/// The two pictures the passes write between them.
#[derive(Component)]
struct Between(CachedTexture, CachedTexture);

fn pipelines(mut commands: Commands, cache: Res<PipelineCache>, assets: Res<AssetServer>) {
    let layout = BindGroupLayoutDescriptor::new(
        "osage_denoise_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                texture_2d(TextureSampleType::Float { filterable: false }),
                texture_storage_2d(TextureFormat::Rgba16Float, StorageTextureAccess::WriteOnly),
                texture_2d(TextureSampleType::Uint),
                texture_depth_2d(),
                uniform_buffer::<ViewUniform>(true),
            ),
        ),
    );
    let temporal = BindGroupLayoutDescriptor::new(
        "osage_denoise_temporal_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                texture_2d(TextureSampleType::Float { filterable: false }),
                texture_storage_2d(TextureFormat::Rgba16Float, StorageTextureAccess::WriteOnly),
                texture_2d(TextureSampleType::Uint),
                texture_depth_2d(),
                uniform_buffer::<ViewUniform>(true),
                texture_2d(TextureSampleType::Float { filterable: false }),
                texture_2d(TextureSampleType::Uint),
                texture_depth_2d(),
                texture_2d(TextureSampleType::Float { filterable: false }),
                texture_storage_2d(TextureFormat::Rgba16Float, StorageTextureAccess::WriteOnly),
                uniform_buffer::<PreviousViewData>(true),
            ),
        ),
    );
    let shader = load_embedded_asset!(assets.as_ref(), "denoise.wesl");
    let step = |entry: &'static str| {
        cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some(format!("osage_denoise_{entry}").into()),
            layout: vec![layout.clone()],
            shader: shader.clone(),
            entry_point: Some(entry.into()),
            ..default()
        })
    };
    let steps = [step("step1"), step("step2"), step("step4")];
    let accumulate = cache.queue_compute_pipeline(ComputePipelineDescriptor {
        label: Some("osage_denoise_accumulate".into()),
        layout: vec![temporal.clone()],
        shader: shader.clone(),
        entry_point: Some("accumulate".into()),
        ..default()
    });
    commands.insert_resource(Pipelines { layout, steps, temporal, accumulate });
}

fn textures(
    mut commands: Commands,
    device: Res<RenderDevice>,
    mut cache: ResMut<TextureCache>,
    views: Query<(Entity, &ViewTarget), With<Denoised>>,
) {
    for (entity, target) in &views {
        let size = target.main_texture().size();
        let picture = |label: &'static str, cache: &mut TextureCache| {
            cache.get(
                &device,
                TextureDescriptor {
                    label: Some(label),
                    size: Extent3d { width: size.width, height: size.height, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format: TextureFormat::Rgba16Float,
                    usage: TextureUsages::TEXTURE_BINDING | TextureUsages::STORAGE_BINDING,
                    view_formats: &[],
                },
            )
        };
        let a = picture("osage_denoise_a", &mut cache);
        let b = picture("osage_denoise_b", &mut cache);
        commands.entity(entity).insert(Between(a, b));
    }
}

/// Each view's history, made at its size (zeroed: no frames yet), remade
/// when the size changes; its two pictures swapped each frame.
fn histories(
    mut commands: Commands,
    device: Res<RenderDevice>,
    mut views: Query<(Entity, &ViewTarget, Option<&mut History>), With<Denoised>>,
) {
    for (entity, target, history) in &mut views {
        let size = target.main_texture().size();
        if let Some(mut history) = history
            && history.pictures[0].0.size() == size
        {
            history.read = 1 - history.read;
            continue;
        }
        let picture = |label: &'static str| {
            let texture = device.create_texture(&TextureDescriptor {
                label: Some(label),
                size: Extent3d { width: size.width, height: size.height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Rgba16Float,
                usage: TextureUsages::TEXTURE_BINDING | TextureUsages::STORAGE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&TextureViewDescriptor::default());
            (texture, view)
        };
        let pictures = [picture("osage_denoise_history_0"), picture("osage_denoise_history_1")];
        commands.entity(entity).insert(History { pictures, read: 0 });
    }
}

/// What the passes take of a view.
type Filtered = (
    &'static ViewTarget,
    &'static ViewPrepassTextures,
    &'static ViewUniformOffset,
    &'static Between,
    Option<&'static History>,
    Option<&'static PreviousViewUniformOffset>,
);

#[allow(clippy::too_many_arguments)]
fn denoise(
    view: ViewQuery<Filtered>,
    pipelines: Option<Res<Pipelines>>,
    cache: Res<PipelineCache>,
    uniforms: Res<ViewUniforms>,
    previous_uniforms: Res<PreviousViewUniforms>,
    device: Res<RenderDevice>,
    mut ctx: RenderContext,
) {
    let (target, prepass, offset, Between(a, b), history, previous_offset) = view.into_inner();
    let Some(pipelines) = pipelines else { return };
    let (Some(s1), Some(s2), Some(s4), Some(gbuffer), Some(depth), Some(view_binding)) = (
        cache.get_compute_pipeline(pipelines.steps[0]),
        cache.get_compute_pipeline(pipelines.steps[1]),
        cache.get_compute_pipeline(pipelines.steps[2]),
        prepass.deferred_view(),
        prepass.depth_only_view(),
        uniforms.uniforms.binding(),
    ) else {
        return;
    };
    let layout = cache.get_bind_group_layout(&pipelines.layout);
    let picture = target.main_texture_view();
    let group = |from, to| {
        device.create_bind_group(
            "osage_denoise",
            &layout,
            &BindGroupEntries::sequential((from, to, gbuffer, depth, view_binding.clone())),
        )
    };
    // With last frame's G-buffer (kept for ReSTIR), the light is first
    // averaged over the frames before; without, filtered as it is.
    let temporal = match (
        history,
        previous_offset,
        cache.get_compute_pipeline(pipelines.accumulate),
        prepass.motion_vectors_view(),
        prepass.previous_deferred_view(),
        prepass.previous_depth_only_view(),
        previous_uniforms.uniforms.binding(),
    ) {
        (
            Some(history),
            Some(previous_offset),
            Some(pipeline),
            Some(motion),
            Some(gbuffer_was),
            Some(depth_was),
            Some(previous),
        ) => {
            let (read, written) = (&history.pictures[history.read].1, &history.pictures[1 - history.read].1);
            let group = device.create_bind_group(
                "osage_denoise_temporal",
                &cache.get_bind_group_layout(&pipelines.temporal),
                &BindGroupEntries::sequential((
                    picture,
                    &a.default_view,
                    gbuffer,
                    depth,
                    view_binding.clone(),
                    motion,
                    gbuffer_was,
                    depth_was,
                    read,
                    written,
                    previous,
                )),
            );
            Some((pipeline, group, previous_offset.offset))
        }
        _ => None,
    };
    let passes = if temporal.is_some() {
        [
            (s1, group(&a.default_view, &b.default_view)),
            (s2, group(&b.default_view, &a.default_view)),
            (s4, group(&a.default_view, picture)),
        ]
    } else {
        [
            (s1, group(picture, &a.default_view)),
            (s2, group(&a.default_view, &b.default_view)),
            (s4, group(&b.default_view, picture)),
        ]
    };
    let size = target.main_texture().size();
    let groups = (size.width.div_ceil(8), size.height.div_ceil(8));
    let mut pass = ctx
        .command_encoder()
        .begin_compute_pass(&ComputePassDescriptor { label: Some("osage_denoise"), timestamp_writes: None });
    if let Some((pipeline, group, previous_offset)) = &temporal {
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[offset.offset, *previous_offset]);
        pass.dispatch_workgroups(groups.0, groups.1, 1);
    }
    for (pipeline, group) in &passes {
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[offset.offset]);
        pass.dispatch_workgroups(groups.0, groups.1, 1);
    }
}
