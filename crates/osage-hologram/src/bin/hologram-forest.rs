// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A forest as a hologram (D-133, issue #7): the plot `hologram-bake`
//! photographed (120 trees of Poly Haven's most detailed, about 500
//! million faces), drawn as its trees or as its hologram, filmed by a
//! camera driving past it 60 m away at 2 m — as from a circuit — one frame
//! on the GPU at a time; the frame times printed.
//!
//!     hologram-forest trees|hologram [OUT]

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

const TREES: [&str; 3] = [
    "polyhaven/island_tree_03/island_tree_03_1k.gltf",
    "polyhaven/tree_small_02/tree_small_02_1k.gltf",
    "polyhaven/island_tree_01/island_tree_01_1k.gltf",
];
/// The plot, as baked: width, depth, trees, seed.
const PLOT: (f32, f32, usize, u64) = (60.0, 30.0, 120, 7);
/// The views' framing, as `hologram-bake` measured it, and their grid.
const CENTRE: Vec3 = Vec3::new(0.79, 3.24, -15.40);
const RADIUS: f32 = 36.26;
const ASPECT: f32 = 0.25;
const WARM: u32 = 30;
const FRAMES: u32 = 300;
const SIZE: (u32, u32) = (1280, 720);

#[derive(Resource)]
struct Drive {
    hologram: bool,
    out: Option<PathBuf>,
    frame: u32,
    times: Vec<f32>,
    last: Option<std::time::Instant>,
    target: Handle<Image>,
    camera: Option<Entity>,
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let hologram = match args.next().as_deref() {
        Some("trees") => false,
        Some("hologram") => true,
        _ => anyhow::bail!("hologram-forest trees|hologram [OUT]"),
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
            hologram,
            out,
            frame: 0,
            times: Vec::new(),
            last: None,
            target: Handle::default(),
            camera: None,
        })
        .add_systems(Startup, setup)
        .add_systems(Update, drive);
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
    let (width, depth, count, seed) = PLOT;
    if drive.hologram {
        commands.spawn(Hologram {
            folder: "holograms/forest-1024".to_owned(),
            grid: Grid::new((-40.0, 40.0, 4.0), (0.0, 12.0, 3.0)),
            centre: CENTRE,
            radius: RADIUS,
            // A box over the plot: its front at the plot's front edge.
            stand: (Vec3::new(CENTRE.x, 4.0, 2.5), Vec2::new(width / 2.0 + 2.5, 4.5)),
            flat: false,
            lit: true,
            fade_in: None,
            aspect: ASPECT,
            masked: true,
            stand_depth: depth + 5.0,
            window: std::env::var("HOLOGRAM_WINDOW")
                .ok()
                .and_then(|w| w.split_once('x').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)))),
        });
    } else {
        for (tree, places) in TREES.iter().zip(osage_hologram::forest(width, depth, count, seed, TREES.len())) {
            commands.spawn(osage_props::Scatter {
                model: server.load(*tree),
                places,
                using: osage_props::Use::Whole,
                within: None,
                shadows: true,
                lods: false,
                tint: None,
                opaque: true,
                lit_as_ground: false,
            });
        }
    }
    commands.spawn((
        // No shadow maps for either: the hologram casts none yet, and the
        // trees' hundreds of millions of faces redrawn per cascade lost the GPU.
        DirectionalLight { illuminance: 10_000.0, shadow_maps_enabled: false, ..default() },
        Transform::from_xyz(-0.6, 1.0, 0.8).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(400.0, 400.0))),
        MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgb(0.3, 0.36, 0.24), ..default() })),
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

/// Where the camera is at a frame: 60 m in front of the plot, driving
/// along it from 45 m to one side to 45 m to the other, looking at it.
fn camera_at(frame: u32) -> Transform {
    let t = frame as f32 / (WARM + FRAMES) as f32;
    let x = -45.0 + 90.0 * t;
    Transform::from_xyz(x, 2.0, 60.0).looking_at(Vec3::new(x * 0.4, 3.0, -10.0), Vec3::Y)
}

fn drive(
    mut commands: Commands,
    mut drive: ResMut<Drive>,
    waiting: Res<osage_props::Waiting>,
    holograms: Query<&Children, With<Hologram>>,
    mut exit: MessageWriter<AppExit>,
) {
    if waiting.0 > 0 || drive.hologram && holograms.is_empty() {
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
        drive.times.push((now - last).as_secs_f32() * 1000.0);
    }
    drive.last = Some(now);
    // HOLOGRAM_FILM: a picture every other frame, for a film of the drive
    // (its frame times are then not a measurement).
    let film = std::env::var_os("HOLOGRAM_FILM").is_some() && drive.frame > WARM && drive.frame.is_multiple_of(2);
    if let Some(out) = drive.out.clone()
        && ([WARM + 10, WARM + 150, WARM + 290].contains(&drive.frame) || film)
    {
        let name = if drive.hologram { "hologram" } else { "trees" };
        commands
            .spawn(Screenshot::image(drive.target.clone()))
            .observe(save_to_disk(out.join(format!("{name}-{:04}.png", drive.frame))));
    }
    drive.frame += 1;
    if drive.frame > WARM + FRAMES + 30 {
        let mut times = drive.times.clone();
        times.sort_by(f32::total_cmp);
        let mean = times.iter().sum::<f32>() / times.len().max(1) as f32;
        let p95 = times.get(times.len() * 95 / 100).copied().unwrap_or(0.0);
        println!(
            "{}: {} frames, mean {mean:.2} ms, p95 {p95:.2} ms",
            if drive.hologram { "hologram" } else { "trees" },
            times.len()
        );
        exit.write(AppExit::Success);
    }
}
