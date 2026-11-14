// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The traced light's grain filtered out (D-095). Solari's light, without a
//! learned denoiser (DLSS ray reconstruction is a proprietary SDK, out of
//! bounds here), leaves grain: sparkles on wet asphalt, a speckled kart.
//! An edge-avoiding à-trous filter (`denoise.wesl`) smooths the light —
//! the picture divided by each surface's colour — across pixels of the
//! same surface (alike in facing, plane and light), in three passes after
//! the opaque pass and before the see-through things are drawn over it;
//! temporal anti-aliasing then steadies it over frames.
//!
//! Only the T4 camera is filtered, on hardware that traces rays.
//! `OSAGE_DENOISE=0` leaves the grain, to compare; `OSAGE_DENOISE=force`
//! filters the deferred picture of hardware without, to see the filter
//! keep edges where there is no grain.

use bevy::{
    asset::{embedded_asset, load_embedded_asset},
    core_pipeline::{
        core_3d::{main_opaque_pass_3d, main_transparent_pass_3d},
        prepass::ViewPrepassTextures,
        schedule::{Core3d, Core3dSystems},
    },
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup, RenderSystems,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        render_resource::{
            BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, CachedComputePipelineId,
            ComputePassDescriptor, ComputePipelineDescriptor, Extent3d, PipelineCache, ShaderStages,
            StorageTextureAccess, TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType, TextureUsages,
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
        if !features.contains(SolariPlugins::required_wgpu_features()) && asked != "force" {
            return;
        }
        render
            .add_systems(RenderStartup, pipelines)
            .add_systems(Render, textures.in_set(RenderSystems::PrepareResources))
            .add_systems(
                Core3d,
                denoise.after(main_opaque_pass_3d).before(main_transparent_pass_3d).in_set(Core3dSystems::MainPass),
            );
    }
}

#[derive(Resource)]
struct Pipelines {
    layout: BindGroupLayoutDescriptor,
    /// The passes, taps 1, 2 and 4 pixels apart.
    steps: [CachedComputePipelineId; 3],
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
    commands.insert_resource(Pipelines { layout, steps });
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

fn denoise(
    view: ViewQuery<(&ViewTarget, &ViewPrepassTextures, &ViewUniformOffset, &Between)>,
    pipelines: Option<Res<Pipelines>>,
    cache: Res<PipelineCache>,
    uniforms: Res<ViewUniforms>,
    device: Res<RenderDevice>,
    mut ctx: RenderContext,
) {
    let (target, prepass, offset, Between(a, b)) = view.into_inner();
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
    let passes = [
        (s1, group(picture, &a.default_view)),
        (s2, group(&a.default_view, &b.default_view)),
        (s4, group(&b.default_view, picture)),
    ];
    let size = target.main_texture().size();
    let mut pass = ctx
        .command_encoder()
        .begin_compute_pass(&ComputePassDescriptor { label: Some("osage_denoise"), timestamp_writes: None });
    for (pipeline, group) in &passes {
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[offset.offset]);
        pass.dispatch_workgroups(size.width.div_ceil(8), size.height.div_ceil(8), 1);
    }
}
