// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The second experiment of issue #7: the facade's model and its hologram
//! side by side, filmed by a camera turning round them, off screen.
//!
//!     hologram-view OUT [FRAMES]
//!
//! The model stands at the origin, the hologram (drawn from the views in
//! `holograms/facade-256`, baked with `HOLOGRAM_KEY=1` at 256 px) 32 m to
//! its left, both under the same sun. The camera turns from 60° to one
//! side of them to 60° to the other, at 15 m up, 70 m away, one picture a
//! frame (default 90 frames), in OUT, numbered; frame times are printed.

use std::{path::PathBuf, time::Duration};

use bevy::{
    app::ScheduleRunnerPlugin,
    asset::RenderAssetUsages,
    camera::RenderTarget,
    prelude::*,
    render::{
        RenderPlugin,
        render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
        view::screenshot::{Screenshot, save_to_disk},
    },
    window::ExitCondition,
    winit::WinitPlugin,
};
use osage_hologram::{Grid, Hologram, HologramPlugin};

const FACADE: &str = "polyhaven/modular_urban_apartments_facade/modular_urban_apartments_facade_1k.gltf";
/// The part of the facade its views frame (as `hologram-bake` was given).
const CENTRE: Vec3 = Vec3::new(13.75, 6.75, -2.9);
const RADIUS: f32 = 16.2;
/// Where the hologram stands from the model.
const APART: f32 = -62.0;
const SIZE: (u32, u32) = (1280, 720);

#[derive(Resource)]
struct Film {
    out: PathBuf,
    frames: u32,
    at: u32,
    settle: u32,
    target: Handle<Image>,
    camera: Option<Entity>,
    started: Option<std::time::Instant>,
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(anyhow::Context::with_context(args.next(), || "hologram-view OUT [FRAMES]")?);
    let frames: u32 = args.next().map_or(Ok(90), |f| f.parse())?;
    std::fs::create_dir_all(&out)?;
    let root = anyhow::Context::with_context(osage_content::root(), || "the asset manifest content is not here")?;
    let plugins = DefaultPlugins
        .build()
        .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
        .set(RenderPlugin { synchronous_pipeline_compilation: true, ..default() })
        .set(AssetPlugin { file_path: root.to_string_lossy().into_owned(), ..default() })
        .disable::<WinitPlugin>();
    App::new()
        .add_plugins(plugins)
        .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::ZERO))
        .add_plugins((osage_props::PropsPlugin, HologramPlugin))
        .insert_resource(Film { out, frames, at: 0, settle: 0, target: Handle::default(), camera: None, started: None })
        .add_systems(Startup, setup)
        .add_systems(Update, film)
        .run();
    Ok(())
}

fn setup(
    mut commands: Commands,
    server: Res<AssetServer>,
    mut film: ResMut<Film>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn(osage_props::Scatter {
        model: server.load(FACADE),
        places: vec![Transform::IDENTITY],
        using: osage_props::Use::Whole,
        within: None,
        shadows: true,
        lods: false,
        tint: None,
        opaque: false,
        lit_as_ground: false,
    });
    commands.spawn((
        Hologram {
            folder: "holograms/facade-256".to_owned(),
            grid: Grid::facade(),
            centre: CENTRE,
            radius: RADIUS,
            stand: (Vec3::new(CENTRE.x, CENTRE.y, 0.0), Vec2::new(13.25, 8.75)),
            flat: std::env::var_os("HOLOGRAM_FLAT").is_some(),
            lit: std::env::var_os("HOLOGRAM_LIT").is_some(),
            fade_in: None,
            aspect: 1.0,
            masked: false,
            stand_depth: 0.0,
            window: None,
        },
        Transform::from_xyz(APART, 0.0, 0.0),
    ));
    // The same sun as the views were baked under.
    commands.spawn((
        DirectionalLight { illuminance: 10_000.0, shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(-0.6, 1.0, 0.8).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(400.0, 400.0))),
        MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgb(0.32, 0.34, 0.3), ..default() })),
        Transform::from_xyz(0.0, -2.0, 0.0),
    ));
    let mut target = Image::new_fill(
        Extent3d { width: SIZE.0, height: SIZE.1, depth_or_array_layers: 1 },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    target.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC;
    film.target = images.add(target);
}

fn film(
    mut commands: Commands,
    mut film: ResMut<Film>,
    waiting: Res<osage_props::Waiting>,
    drawn: Query<&Children, With<Hologram>>,
    mut exit: MessageWriter<AppExit>,
) {
    // Both there before the first picture.
    if waiting.0 > 0 || drawn.iter().next().is_none() {
        return;
    }
    let middle = Vec3::new((APART + 27.0) / 2.0, 6.0, 0.0);
    let t = film.at as f32 / (film.frames.max(2) - 1) as f32;
    let angle = (-60.0 + 120.0 * t).to_radians();
    let place = match std::env::var("HOLOGRAM_CLOSE") {
        // Close, on the hologram, from between the grid's views:
        // HOLOGRAM_CLOSE=1 the hologram, 2 the model, from the same place.
        Ok(which) => {
            let x = if which == "2" { 0.0 } else { APART };
            let target = Vec3::new(x + 20.0, 8.0, 0.0);
            let (a, e) = (35f32.to_radians(), 22f32.to_radians());
            Transform::from_translation(target + Vec3::new(a.sin() * e.cos(), e.sin(), a.cos() * e.cos()) * 14.0)
                .looking_at(target, Vec3::Y)
        }
        Err(_) => Transform::from_translation(middle + Vec3::new(angle.sin() * 70.0, 9.0, angle.cos() * 70.0))
            .looking_at(middle, Vec3::Y),
    };
    match film.camera {
        None => {
            let camera = commands
                .spawn((
                    Camera3d::default(),
                    Camera { clear_color: ClearColorConfig::Custom(Color::srgb(0.55, 0.68, 0.85)), ..default() },
                    RenderTarget::Image(film.target.clone().into()),
                    place,
                ))
                .id();
            film.camera = Some(camera);
            film.started = Some(std::time::Instant::now());
            return;
        }
        Some(camera) => {
            commands.entity(camera).insert(place);
        }
    }
    // A few frames for the pipelines, then one picture a frame.
    if film.settle < 10 {
        film.settle += 1;
        return;
    }
    if film.at >= film.frames {
        // Frames for the last pictures to be written.
        film.settle += 1;
        if film.settle < 40 {
            return;
        }
        if let Some(started) = film.started {
            info!(frames = film.frames, seconds = started.elapsed().as_secs_f32(), "hologram-view: done");
        }
        exit.write(AppExit::Success);
        return;
    }
    let file = film.out.join(format!("{:04}.png", film.at));
    commands.spawn(Screenshot::image(film.target.clone())).observe(save_to_disk(file));
    film.at += 1;
}
