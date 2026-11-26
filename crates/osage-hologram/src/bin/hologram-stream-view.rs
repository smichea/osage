// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A streamed hologram filmed by a camera turning round it, off screen
//! (D-134): its views come from a hologram server as the camera needs them.
//!
//!     hologram-stream-view SERVER PATH OUT [FRAMES] [DISTANCE]
//!
//! SERVER is a `hologram-serve` (`host:port`), PATH the hologram's folder
//! there. The camera turns round the hologram's photographed sphere, from
//! 40° to one side to 40° to the other, 6° up, DISTANCE m from its centre
//! (2.5 radii unless given), one picture a frame (default 120) in OUT,
//! numbered; at the end it prints what came over the network.

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
use osage_hologram::{
    HologramPlugin,
    stream::{Stream, StreamPlugin, Streamed},
};

const SIZE: (u32, u32) = (1280, 720);

#[derive(Resource)]
struct Film {
    out: PathBuf,
    frames: u32,
    at: u32,
    settle: u32,
    distance: Option<f32>,
    target: Handle<Image>,
    camera: Option<Entity>,
    started: Option<std::time::Instant>,
}

fn main() -> anyhow::Result<()> {
    let usage = "hologram-stream-view SERVER PATH OUT [FRAMES] [DISTANCE]";
    let mut args = std::env::args().skip(1);
    let server = anyhow::Context::with_context(args.next(), || usage)?;
    let path = anyhow::Context::with_context(args.next(), || usage)?;
    let out = PathBuf::from(anyhow::Context::with_context(args.next(), || usage)?);
    let frames: u32 = args.next().map_or(Ok(120), |f| f.parse())?;
    let distance: Option<f32> = args.next().map(|d| d.parse()).transpose()?;
    std::fs::create_dir_all(&out)?;
    let plugins = DefaultPlugins
        .build()
        .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
        .set(RenderPlugin { synchronous_pipeline_compilation: true, ..default() })
        .disable::<WinitPlugin>();
    App::new()
        .add_plugins(plugins)
        .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::ZERO))
        .add_plugins((HologramPlugin, StreamPlugin))
        .insert_resource(Film {
            out,
            frames,
            at: 0,
            settle: 0,
            distance,
            target: Handle::default(),
            camera: None,
            started: None,
        })
        .add_systems(Startup, move |mut commands: Commands, images: ResMut<Assets<Image>>, film: ResMut<Film>| {
            setup(&mut commands, images, film);
            commands.spawn((
                Streamed { server: server.clone(), path: path.clone(), window: (5, 3), stand: None },
                Transform::IDENTITY,
            ));
        })
        .add_systems(Update, film)
        .run();
    Ok(())
}

fn setup(commands: &mut Commands, mut images: ResMut<Assets<Image>>, mut film: ResMut<Film>) {
    commands.spawn((
        DirectionalLight { illuminance: 10_000.0, shadow_maps_enabled: false, ..default() },
        Transform::from_xyz(-0.6, 1.0, 0.8).looking_at(Vec3::ZERO, Vec3::Y),
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
    streams: Query<(&Stream, Option<&Children>)>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some((stream, children)) = streams.iter().next() else { return };
    let centre = Vec3::from(stream.packed.centre);
    let distance = film.distance.unwrap_or(stream.packed.radius * 2.5);
    let t = film.at as f32 / (film.frames.max(2) - 1) as f32;
    let (angle, up) = ((-40.0 + 80.0 * t).to_radians(), 6f32.to_radians());
    let place = Transform::from_translation(
        centre + Vec3::new(angle.sin() * up.cos(), up.sin(), angle.cos() * up.cos()) * distance,
    )
    .looking_at(centre, Vec3::Y);
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
    // Drawn once its first views have come.
    if children.is_none_or(|c| c.is_empty()) || film.settle < 10 {
        if children.is_some_and(|c| !c.is_empty()) {
            film.settle += 1;
        }
        return;
    }
    if film.at >= film.frames {
        film.settle += 1;
        if film.settle < 40 {
            return;
        }
        let seconds = film.started.map_or(0.0, |s| s.elapsed().as_secs_f32());
        let all: u64 = stream.packed.bytes.values().sum();
        println!(
            "hologram-stream-view: {} frames in {seconds:.1} s; {} views came, {:.1} MiB of {:.1} MiB",
            film.frames,
            stream.views,
            stream.bytes as f64 / 1048576.0,
            all as f64 / 1048576.0
        );
        exit.write(AppExit::Success);
        return;
    }
    let file = film.out.join(format!("{:04}.png", film.at));
    commands.spawn(Screenshot::image(film.target.clone())).observe(save_to_disk(file));
    film.at += 1;
}
