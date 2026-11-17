// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The fourth experiment of issue #7: where a hologram pays. A street of 40
//! facades, 20 on each side, 30 m apart, and a camera driving down it at
//! 2 m, 25 m/s, off screen at 1280 × 720; the frame times printed.
//!
//!     hologram-street models|holograms|mixed [OUT]
//!
//! `models`: every facade its model (118 000 triangles each); `holograms`:
//! every facade its hologram (one rectangle); `mixed`: the model within
//! 60 m of the camera, the hologram beyond, the one fading into the other
//! over 54–60 m (the model by its visibility range, kept prepared; the
//! hologram by its transparency). With OUT, three pictures of the
//! drive are written there.

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
const CENTRE: Vec3 = Vec3::new(13.75, 6.75, -2.9);
const RADIUS: f32 = 16.2;
/// Facades a side, how far apart, how far each side stands from the road's
/// middle.
const EACH_SIDE: usize = 20;
const SPACING: f32 = 30.0;
const SIDE: f32 = 18.0;
/// Within this, a facade is its model (`mixed`).
const NEAR: f32 = 60.0;
/// The drive: speed, frames measured after a few to warm up.
const SPEED: f32 = 25.0;
const WARM: u32 = 30;
const FRAMES: u32 = 400;
const SIZE: (u32, u32) = (1280, 720);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Models,
    Holograms,
    Mixed,
}

#[derive(Resource)]
struct Drive {
    kind: Kind,
    out: Option<PathBuf>,
    frame: u32,
    times: Vec<f32>,
    last: Option<std::time::Instant>,
    target: Handle<Image>,
    camera: Option<Entity>,
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let kind = match args.next().as_deref() {
        Some("models") => Kind::Models,
        Some("holograms") => Kind::Holograms,
        Some("mixed") => Kind::Mixed,
        _ => anyhow::bail!("hologram-street models|holograms|mixed [OUT]"),
    };
    let out = args.next().map(PathBuf::from);
    if let Some(out) = &out {
        std::fs::create_dir_all(out)?;
    }
    let root = anyhow::Context::with_context(osage_content::root(), || "the asset manifest content is not here")?;
    let plugins = DefaultPlugins
        .build()
        .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
        .set(RenderPlugin { synchronous_pipeline_compilation: true, ..default() })
        .set(AssetPlugin { file_path: root.to_string_lossy().into_owned(), ..default() })
        .disable::<WinitPlugin>();
    let mut app = App::new();
    app.add_plugins(plugins)
        .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::ZERO))
        .add_plugins((osage_props::PropsPlugin, HologramPlugin))
        .insert_resource(Drive {
            kind,
            out,
            frame: 0,
            times: Vec::new(),
            last: None,
            target: Handle::default(),
            camera: None,
        })
        .add_systems(Startup, setup)
        .add_systems(Update, drive);
    // One frame on the GPU at a time, as the kart's bench: without a window
    // to pace it, the processor would queue frames faster than the GPU draws
    // them and then wait for many at once — regular stalls that are the
    // queue's, not a frame's.
    if let Some(render) = app.get_sub_app_mut(bevy::render::RenderApp) {
        render
            .init_resource::<InFlight>()
            .add_systems(bevy::render::Render, keep_one_in_flight.in_set(bevy::render::RenderSystems::Cleanup));
    }
    app.run();
    Ok(())
}

fn setup(
    mut commands: Commands,
    server: Res<AssetServer>,
    mut drive: ResMut<Drive>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let model = server.load(FACADE);
    for side in [-1.0f32, 1.0] {
        for n in 0..EACH_SIDE {
            // The wall's middle at x along the road; its front to the road.
            let x = n as f32 * SPACING;
            let turn = if side < 0.0 { Quat::IDENTITY } else { Quat::from_rotation_y(std::f32::consts::PI) };
            let place = Transform::from_translation(Vec3::new(x, 0.0, side * SIDE))
                * Transform::from_rotation(turn)
                * Transform::from_xyz(-CENTRE.x, 0.0, 0.0);
            let wants_model = drive.kind != Kind::Holograms;
            let wants_hologram = drive.kind != Kind::Models;
            wants_model.then(|| {
                commands
                    .spawn(osage_props::Scatter {
                        model: model.clone(),
                        places: vec![place],
                        using: osage_props::Use::Whole,
                        // Mixed: the model seen up to NEAR, fading out over
                        // its last tenth, as the hologram fades in.
                        within: (drive.kind == Kind::Mixed).then_some(NEAR),
                        shadows: true,
                        lods: false,
                        tint: None,
                        opaque: false,
                        lit_as_ground: false,
                    })
                    .id()
            });
            wants_hologram.then(|| {
                commands
                    .spawn((
                        Hologram {
                            folder: "holograms/facade-256".to_owned(),
                            grid: Grid::facade(),
                            centre: CENTRE,
                            radius: RADIUS,
                            stand: (Vec3::new(CENTRE.x, CENTRE.y, 0.0), Vec2::new(13.25, 8.75)),
                            flat: false,
                            lit: std::env::var_os("HOLOGRAM_LIT").is_some(),
                            // Mixed: it fades in as its model fades out.
                            fade_in: (drive.kind == Kind::Mixed).then_some((NEAR * 0.9, NEAR)),
                            aspect: 1.0,
                            masked: false,
                            stand_depth: 0.0,
                            window: None,
                        },
                        place,
                    ))
                    .id()
            });
        }
    }
    commands.spawn((
        // HOLOGRAM_NO_SHADOWS: without shadow maps, to tell their cost apart.
        DirectionalLight {
            illuminance: 10_000.0,
            shadow_maps_enabled: std::env::var_os("HOLOGRAM_NO_SHADOWS").is_none(),
            ..default()
        },
        Transform::from_xyz(-0.6, 1.0, 0.8).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(1600.0, 200.0))),
        MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgb(0.3, 0.31, 0.3), ..default() })),
        Transform::from_xyz(300.0, -2.0, 0.0),
    ));
    let mut target = Image::new_fill(
        Extent3d { width: SIZE.0, height: SIZE.1, depth_or_array_layers: 1 },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    target.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC;
    drive.target = images.add(target);
}

/// Whether the frame handed to the GPU last is done.
#[derive(Resource, Default)]
struct InFlight(Option<std::sync::Arc<std::sync::atomic::AtomicBool>>);

fn keep_one_in_flight(
    device: Res<bevy::render::renderer::RenderDevice>,
    queue: Res<bevy::render::renderer::RenderQueue>,
    mut in_flight: ResMut<InFlight>,
) {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let done = Arc::new(AtomicBool::new(false));
    let signal = done.clone();
    queue.on_submitted_work_done(move || signal.store(true, Ordering::Release));
    if let Some(previous) = in_flight.0.replace(done) {
        while !previous.load(Ordering::Acquire) {
            let _ = device.poll(bevy::render::render_resource::PollType::Poll);
            std::thread::sleep(Duration::from_micros(100));
        }
    }
}

/// Where the camera is at a frame of the drive.
fn camera_at(frame: u32) -> Transform {
    let x = -40.0 + frame as f32 * SPEED / 60.0;
    Transform::from_xyz(x, 2.0, 0.0).looking_at(Vec3::new(x + 50.0, 3.0, 0.0), Vec3::Y)
}

fn drive(
    mut commands: Commands,
    mut drive: ResMut<Drive>,
    waiting: Res<osage_props::Waiting>,
    holograms: Query<&Children, With<Hologram>>,
    mut exit: MessageWriter<AppExit>,
) {
    let holograms_wanted = drive.kind != Kind::Models;
    if waiting.0 > 0 || holograms_wanted && holograms.iter().count() < 2 * EACH_SIDE {
        return;
    }
    let place = camera_at(drive.frame);
    match drive.camera {
        None => {
            let camera = commands
                .spawn((
                    Camera3d::default(),
                    Camera { clear_color: ClearColorConfig::Custom(Color::srgb(0.55, 0.68, 0.85)), ..default() },
                    RenderTarget::Image(drive.target.clone().into()),
                    place,
                ))
                .id();
            drive.camera = Some(camera);
            return;
        }
        Some(camera) => {
            commands.entity(camera).insert(place);
        }
    }
    let now = std::time::Instant::now();
    if let Some(last) = drive.last
        && drive.frame > WARM
    {
        let ms = (now - last).as_secs_f32() * 1000.0;
        drive.times.push(ms);
    }
    drive.last = Some(now);
    if let Some(out) = drive.out.clone()
        && [WARM + 60, WARM + 200, WARM + 340].contains(&drive.frame)
    {
        commands
            .spawn(Screenshot::image(drive.target.clone()))
            .observe(save_to_disk(out.join(format!("{:?}-{:04}.png", drive.kind, drive.frame))));
    }
    drive.frame += 1;
    if drive.frame > WARM + FRAMES + 30 {
        let mut times = drive.times.clone();
        times.sort_by(f32::total_cmp);
        let mean = times.iter().sum::<f32>() / times.len().max(1) as f32;
        let p95 = times.get(times.len() * 95 / 100).copied().unwrap_or(0.0);
        println!("{:?}: {} frames, mean {mean:.2} ms, p95 {p95:.2} ms", drive.kind, times.len());
        let slow: Vec<String> = drive
            .times
            .iter()
            .enumerate()
            .filter(|(_, ms)| **ms > 100.0)
            .map(|(i, ms)| format!("{}:{ms:.0}", i as u32 + WARM + 1))
            .collect();
        println!("  frames over 100 ms (frame:ms): {}", slow.join(" "));
        exit.write(AppExit::Success);
    }
}
