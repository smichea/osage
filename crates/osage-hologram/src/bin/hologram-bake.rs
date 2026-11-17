// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Photographs a model of the asset manifest from every direction of a
//! [`Grid`], one picture per view, for the holographic textures experiment
//! (issue #7):
//!
//!     hologram-bake OUT [MODEL] [SIZE] [X,Y,Z,R]
//!
//! MODEL is a glTF file under the manifest's content (default: Poly
//! Haven's urban apartments facade), SIZE the pictures' side in pixels
//! (default 512), X,Y,Z,R the part of it framed — a sphere's centre and
//! radius, m (default: the whole model). Each view is orthographic — no perspective, the object
//! framed the same in every picture, so that a view differs from the next
//! only by the turn of the viewing angle — under one fixed sun, on a grey
//! ground colour. Without a window: the pictures are drawn off screen.

use std::{path::PathBuf, time::Duration};

use bevy::{
    app::ScheduleRunnerPlugin,
    asset::RenderAssetUsages,
    camera::{RenderTarget, ScalingMode, primitives::Aabb},
    prelude::*,
    render::{
        RenderPlugin,
        render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
        view::screenshot::{Screenshot, save_to_disk},
    },
    window::ExitCondition,
    winit::WinitPlugin,
};
use osage_hologram::Grid;

const FACADE: &str = "polyhaven/modular_urban_apartments_facade/modular_urban_apartments_facade_1k.gltf";
/// Frames drawn at a view before it is photographed, and after the last
/// before the program ends (for the pictures to be written).
const SETTLE: u32 = 4;
const WIND_DOWN: u32 = 60;

#[derive(Resource)]
struct Bake {
    out: PathBuf,
    size: u32,
    grid: Grid,
    path: Vec<(usize, usize)>,
    /// The view being photographed, and the frames it has been drawn.
    at: usize,
    frames: u32,
    /// The part framed: its centre and radius; the whole object's, once it
    /// has loaded, unless given.
    bounds: Option<(Vec3, f32)>,
    /// Whether the model has loaded and stands in place.
    loaded: bool,
    target: Handle<Image>,
    camera: Option<Entity>,
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(anyhow::Context::with_context(args.next(), || "hologram-bake OUT [MODEL] [SIZE]")?);
    let model = args.next().filter(|m| !m.is_empty()).unwrap_or_else(|| FACADE.to_owned());
    let size: u32 =
        anyhow::Context::with_context(args.next().map_or(Ok(512), |s| s.parse::<u32>()), || "SIZE in pixels")?;
    let framed = match args.next() {
        Some(region) => {
            let n: Vec<f32> = region.split(',').map(str::parse).collect::<Result<_, _>>()?;
            let [x, y, z, r] = n[..] else { anyhow::bail!("X,Y,Z,R: a centre and a radius") };
            Some((Vec3::new(x, y, z), r))
        }
        None => None,
    };
    std::fs::create_dir_all(&out)?;
    let root = anyhow::Context::with_context(
        osage_content::root(),
        || "the asset manifest content is not here (OSAGE_ASSETS)",
    )?;
    // The grid, from the environment for experiments: HOLOGRAM_GRID =
    // "AZ_FROM,AZ_TO,AZ_STEP,EL_FROM,EL_TO,EL_STEP" (default: a facade's).
    let grid = match std::env::var("HOLOGRAM_GRID") {
        Ok(text) => {
            let n: Vec<f32> = text.split(',').map(str::parse).collect::<Result<_, _>>()?;
            let [a0, a1, a2, e0, e1, e2] = n[..] else { anyhow::bail!("HOLOGRAM_GRID takes six numbers") };
            Grid::new((a0, a1, a2), (e0, e1, e2))
        }
        Err(_) => Grid::facade(),
    };
    let path = grid.path();
    let plugins = DefaultPlugins
        .build()
        .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
        .set(RenderPlugin { synchronous_pipeline_compilation: true, ..default() })
        .set(AssetPlugin { file_path: root.to_string_lossy().into_owned(), ..default() })
        .disable::<WinitPlugin>();
    App::new()
        .add_plugins(plugins)
        .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::ZERO))
        .add_plugins(osage_props::PropsPlugin)
        .insert_resource(Bake {
            out,
            size,
            grid,
            path,
            at: 0,
            frames: 0,
            bounds: framed,
            loaded: false,
            target: Handle::default(),
            camera: None,
        })
        .insert_resource(ModelPath(model))
        .add_systems(Startup, setup)
        .add_systems(Update, (measure, photograph).chain())
        .run();
    Ok(())
}

#[derive(Resource)]
struct ModelPath(String);

fn setup(
    mut commands: Commands,
    server: Res<AssetServer>,
    model: Res<ModelPath>,
    mut bake: ResMut<Bake>,
    mut images: ResMut<Assets<Image>>,
) {
    commands.spawn(osage_props::Scatter {
        model: server.load(model.0.clone()),
        places: vec![Transform::IDENTITY],
        using: osage_props::Use::Whole,
        within: None,
        shadows: true,
        lods: false,
        tint: None,
        opaque: false,
        lit_as_ground: false,
    });
    // One sun, high and to the left of the front, the same for every view:
    // the light is part of what is photographed.
    commands.spawn((
        DirectionalLight { illuminance: 10_000.0, shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(-0.6, 1.0, 0.8).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    let size = Extent3d { width: bake.size, height: bake.size, depth_or_array_layers: 1 };
    let mut target = Image::new_fill(
        size,
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    target.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC;
    bake.target = images.add(target);
}

/// Once the model is in place: its centre and the radius of the sphere
/// round it, from its meshes' boxes.
fn measure(mut bake: ResMut<Bake>, waiting: Res<osage_props::Waiting>, meshes: Query<(&Aabb, &GlobalTransform)>) {
    if bake.loaded || waiting.0 > 0 || meshes.is_empty() {
        return;
    }
    bake.loaded = true;
    if bake.bounds.is_some() {
        return;
    }
    let (mut low, mut high) = (Vec3::MAX, Vec3::MIN);
    for (aabb, at) in &meshes {
        for corner in 0..8 {
            let c = Vec3::from(aabb.center);
            let h = Vec3::from(aabb.half_extents);
            let sign = Vec3::new(
                if corner & 1 == 0 { -1.0 } else { 1.0 },
                if corner & 2 == 0 { -1.0 } else { 1.0 },
                if corner & 4 == 0 { -1.0 } else { 1.0 },
            );
            let p = at.transform_point(c + h * sign);
            (low, high) = (low.min(p), high.max(p));
        }
    }
    let centre = (low + high) / 2.0;
    let radius = (high - low).length() / 2.0;
    info!(?low, ?high, radius, "hologram-bake: the model's bounds");
    bake.bounds = Some((centre, radius));
}

/// Each view in turn: the camera put there, a few frames drawn, a picture.
fn photograph(mut commands: Commands, mut bake: ResMut<Bake>, mut exit: MessageWriter<AppExit>) {
    let Some((centre, radius)) = bake.bounds.filter(|_| bake.loaded) else { return };
    if bake.at >= bake.path.len() {
        bake.frames += 1;
        if bake.frames > WIND_DOWN {
            info!(views = bake.path.len(), out = ?bake.out, "hologram-bake: done");
            exit.write(AppExit::Success);
        }
        return;
    }
    let (row, column) = bake.path[bake.at];
    let [x, y, z] = Grid::direction(bake.grid.azimuths[column], bake.grid.elevations[row]);
    let place = Transform::from_translation(centre + Vec3::new(x, y, z) * radius * 3.0).looking_at(centre, Vec3::Y);
    match bake.camera {
        None => {
            let projection = OrthographicProjection {
                scaling_mode: ScalingMode::Fixed { width: radius * 2.0, height: radius * 2.0 },
                ..OrthographicProjection::default_3d()
            };
            let camera = commands
                .spawn((
                    Camera3d::default(),
                    Camera { clear_color: ClearColorConfig::Custom(Color::srgb(0.5, 0.5, 0.5)), ..default() },
                    RenderTarget::Image(bake.target.clone().into()),
                    Projection::Orthographic(projection),
                    place,
                ))
                .id();
            bake.camera = Some(camera);
        }
        Some(camera) if bake.frames == 0 => {
            commands.entity(camera).insert(place);
        }
        Some(_) => {}
    }
    bake.frames += 1;
    if bake.frames == SETTLE {
        let file = bake.out.join(Grid::name(row, column));
        commands.spawn(Screenshot::image(bake.target.clone())).observe(save_to_disk(file));
        bake.at += 1;
        bake.frames = 0;
    }
}
