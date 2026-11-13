// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `kart-native`: the native client of the kart world, and the testbed of the
//! rendering programme (design/03).
//!
//! - `kart-native [ws://host:port] [--tier T1|T2|T3|T4]` joins a kart world
//!   (default `ws://127.0.0.1:7878`, tier T2). Escape opens the settings.
//!   `--fps N|off` sets the frame rate wanted for this game (settings). Drive with the arrows or WASD,
//!   drift with Space or Shift; a gamepad works too. F12 saves a screenshot.
//!   `--autodrive` lets the world's autopilot hold the wheel;
//!   `--screenshot-after SECONDS[,SECONDS…]` saves screenshots by itself.
//! - `kart-native --bench [--tier T…] [--frames N] [--resolution WxH] [--out DIR]`
//!   runs the bench without a server and writes `report.json` and screenshots
//!   to `bench/out/<tier>-<time>/`.
//! - `--without TECHNIQUE` (sky, sky-light, shadows, ambient-occlusion, post,
//!   upscaling; repeatable) leaves a rendering technique out, to measure its
//!   cost or to find the cause of a defect.
//! - `--track ID|FILE` (with `--bench`) races the bench on a built-in track
//!   (paddock, coast, forest, harbour) or a track file (D-056), under its
//!   sky unless `--sky` says otherwise.
//! - `--headless` (with `--bench`) opens no window and needs no display: for
//!   machines without a screen, such as cloud GPUs (D-042).
//! - `--camera X,Y,Z,TX,TY,TZ` holds the camera at a place, looking at
//!   another: pictures of a place rather than of the race. A bench run with
//!   it is not the sanctioned bench, and its report says so.

mod bench;
mod effects;
mod graphics;
mod hud;
mod karts;
mod landmarks;
mod lights;
mod marks;
#[cfg(feature = "sound")]
mod music;
mod paddock;
mod photo;
mod play;
mod predict;
mod preview;
mod rain;
mod report;
mod scene;
mod scenery;
mod settings;
mod show;
#[cfg(feature = "sound")]
mod sound;
mod spans;
mod theme;
mod track;
#[cfg(feature = "sound")]
mod voice;

use std::{
    path::PathBuf,
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result, bail};
// Not Bevy's prelude here: its error helpers clash with anyhow's.
#[cfg(feature = "sound")]
use bevy::audio::AudioPlugin;
use bevy::{
    DefaultPlugins,
    app::{App, PluginGroup, ScheduleRunnerPlugin},
    asset::AssetPlugin,
    render::RenderPlugin,
    window::{ExitCondition, PresentMode, Window, WindowPlugin, WindowResolution},
    winit::WinitPlugin,
};
use osage_render::{OsageRenderPlugin, Tier};
use osage_sdk::v0;

fn main() -> Result<()> {
    // The tier: the command line's, else the settings' (a game), else T2.
    let (mut url, mut tier, mut bench) = ("ws://127.0.0.1:7878".to_owned(), None, false);
    let (mut frames, mut resolution, mut out, mut without) = (2400u32, None, None, Vec::new());
    let (mut autodrive, mut screenshot_after, mut viewpoint, mut headless) = (false, Vec::new(), None, false);
    let (mut sky, mut track) = (None, None);
    let mut fps = None;
    // A lens focused this far, f/2.8, as the photo mode has it (D-066).
    let mut focus = None;
    // Rain on the bench, from the start (D-068).
    let mut raining = false;
    // The interface over the bench's race, in states set by hand (D-078).
    let mut ui_preview = false;
    // Players at this computer: 2 splits the screen (D-086).
    let mut players = 1u8;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().with_context(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--bench" => bench = true,
            "--autodrive" => autodrive = true,
            "--headless" => headless = true,
            "--screenshot-after" => {
                screenshot_after = value()?
                    .split(',')
                    .map(|s| s.trim().parse::<f32>())
                    .collect::<Result<_, _>>()
                    .context("--screenshot-after takes seconds, comma-separated")?
            }
            "--tier" => tier = Some(value()?.parse::<Tier>().map_err(anyhow::Error::msg)?),
            "--sky" => sky = Some(value()?.parse::<scene::Sky>().map_err(anyhow::Error::msg)?),
            "--track" => {
                let wanted = value()?;
                let def = match osage_kart_tracks::by_id(&wanted).or_else(|| osage_kart_tracks::arena(&wanted)) {
                    Some(def) => def,
                    None => {
                        let text = std::fs::read_to_string(&wanted).with_context(|| format!("no track {wanted}"))?;
                        osage_kart_tracks::TrackDef::parse(&text).with_context(|| format!("track {wanted}"))?
                    }
                };
                track = Some(def);
            }
            "--rain" => raining = true,
            "--ui-preview" => ui_preview = true,
            "--players" => {
                players = value()?.parse().ok().filter(|n| (1..=2).contains(n)).context("--players takes 1 or 2")?
            }
            "--focus" => focus = Some(value()?.parse::<f32>().context("--focus takes metres")?),
            "--fps" => fps = Some(settings::parse_fps(&value()?).map_err(anyhow::Error::msg)?),
            "--frames" => frames = value()?.parse().context("--frames")?,
            "--out" => out = Some(PathBuf::from(value()?)),
            "--without" => without.push(value()?.parse().map_err(anyhow::Error::msg)?),
            "--camera" => {
                let numbers: Vec<f32> = value()?
                    .split(',')
                    .map(str::parse)
                    .collect::<Result<_, _>>()
                    .context("--camera takes six numbers")?;
                let [x, y, z, tx, ty, tz] = numbers[..] else { bail!("--camera takes X,Y,Z,TX,TY,TZ") };
                viewpoint = Some(scene::Viewpoint { from: [x, y, z].into(), to: [tx, ty, tz].into() });
            }
            "--resolution" => {
                let value = value()?;
                let (width, height) = value.split_once(['x', 'X']).context("--resolution takes WIDTHxHEIGHT")?;
                resolution = Some((width.parse().context("width")?, height.parse().context("height")?));
            }
            flag if flag.starts_with("--") => bail!("unknown argument {flag}"),
            _ => url = arg,
        }
    }

    // Crash reports (D-073): a game watches its run, and finds what went
    // wrong in the last one; the bench does not.
    let last_report = if bench || headless { None } else { report::begin() };
    // The tier: the command line's, else (a game) chosen for the machine
    // (D-075), else T2 (the bench's default).
    let chosen = tier.is_none() && !bench;
    let tier = match tier {
        Some(tier) => tier,
        None if bench => Tier::T2,
        None => {
            let (tier, adapter) = graphics::choose(last_report.is_some());
            eprintln!("graphics: {adapter}: {tier}");
            tier
        }
    };
    // Playing without a window is for checking a game end to end: it takes
    // pictures (`--screenshot-after`), drawn off screen.
    if headless && !bench && screenshot_after.is_empty() {
        bail!("--headless goes with --bench, or with --screenshot-after when playing");
    }
    let mut window = Window { title: format!("osage kart · {tier}"), ..Default::default() };
    let (width, height) = resolution.unwrap_or(tier.bench_resolution());
    if bench {
        // The bench draws off screen at a fixed size, as fast as the machine
        // can; the window only shows it.
        window.resolution = WindowResolution::new(1280, 720);
        window.present_mode = PresentMode::AutoNoVsync;
    }
    let mut app = App::new();
    // The bench compiles its shaders before drawing, so that every frame is
    // complete and the same everywhere; playing, they compile in the background.
    let render = RenderPlugin { synchronous_pipeline_compilation: bench, ..Default::default() };
    // On Windows, Direct3D 12 unless WGPU_BACKEND says otherwise: Intel's
    // Vulkan driver for its older integrated GPUs (HD 630) crashes the game
    // on start, where Direct3D 12 runs it.
    #[cfg(windows)]
    let render = if std::env::var_os("WGPU_BACKEND").is_none() {
        use bevy::render::settings::{Backends, RenderCreation, WgpuSettings};
        let settings = WgpuSettings { backends: Some(Backends::DX12), ..Default::default() };
        RenderPlugin { render_creation: RenderCreation::Automatic(Box::new(settings)), ..render }
    } else {
        render
    };
    // Models are loaded from the asset manifest's content, wherever it is.
    let log = bevy::log::LogPlugin { custom_layer: report::layer, ..Default::default() };
    let assets = AssetPlugin {
        file_path: osage_content::root()
            .map_or_else(|| "assets".to_owned(), |root| root.to_string_lossy().into_owned()),
        ..Default::default()
    };
    if headless {
        // No window and no windowing system: frames are drawn into the
        // off-screen picture only, as fast as the machine can.
        let windowless =
            WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..Default::default() };
        let plugins = DefaultPlugins.set(windowless).set(render).set(assets).set(log).disable::<WinitPlugin>();
        #[cfg(feature = "sound")]
        let plugins = plugins.disable::<AudioPlugin>();
        app.add_plugins(plugins).add_plugins(ScheduleRunnerPlugin::run_loop(Duration::ZERO));
    } else {
        let plugins = DefaultPlugins
            .set(WindowPlugin { primary_window: Some(window), ..Default::default() })
            .set(render)
            .set(assets)
            .set(log);
        // The bench measures pictures: no sound (and built without it, it
        // needs no sound library on the machine).
        #[cfg(feature = "sound")]
        if bench {
            app.add_plugins(plugins.disable::<AudioPlugin>());
        } else {
            app.add_plugins(plugins).add_plugins(sound::SoundPlugin);
        }
        #[cfg(not(feature = "sound"))]
        app.add_plugins(plugins);
    }
    let track = track.unwrap_or_else(|| osage_kart_tracks::by_id("paddock").expect("the paddock is built in"));
    // A bench is raced under its track's sky, unless asked otherwise.
    // In a game, the sky follows the track's unless `--sky` chose one.
    let follows = !bench && sky.is_none();
    // A bench in the rain is raced under an overcast sky (D-069).
    let sky = sky.unwrap_or_else(|| match (bench, raining) {
        (true, true) => scene::Sky::Overcast,
        (true, false) => track.sky.parse().unwrap_or(scene::Sky::Noon),
        (false, _) => scene::Sky::Noon,
    });
    app.insert_resource(scene::look(sky)).insert_resource(scene::SkyShown { sky, follows });
    // Before the rendering plugin, which draws the sky from it when it is there.
    if let Some(panorama) = scene::panorama(sky) {
        app.insert_resource(panorama);
    }
    app.add_plugins(OsageRenderPlugin { tier, without: without.clone() })
        .add_plugins(osage_props::PropsPlugin)
        .add_plugins(scene::ScenePlugin);
    if let Some(viewpoint) = viewpoint {
        app.insert_resource(viewpoint);
    }
    if raining {
        app.insert_resource(rain::Rain::already());
    }
    if let Some(focus) = focus {
        app.add_systems(bevy::app::Update, photo::focus(focus));
    }
    if bench {
        let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let out = out.unwrap_or_else(|| PathBuf::from(format!("bench/out/{tier}-{now}")));
        std::fs::create_dir_all(&out).with_context(|| format!("creating {}", out.display()))?;
        let without = without.iter().map(|t| t.name().to_owned()).collect();
        let held = viewpoint.is_some();
        let sky = format!("{sky:?}").to_lowercase();
        if ui_preview {
            app.add_plugins(preview::PreviewPlugin { frames });
        }
        app.add_plugins(bench::BenchPlugin {
            tier,
            frames,
            out,
            resolution: (width, height),
            without,
            held,
            sky,
            headless,
            track,
        });
    } else {
        let tier = match tier {
            Tier::T1 => v0::Tier::T1Minimal,
            Tier::T2 => v0::Tier::T2Standard,
            Tier::T3 => v0::Tier::T3High,
            Tier::T4 => v0::Tier::T4Ultra,
        };
        if headless {
            // Off screen: the picture at a fixed size, and the pictures taken of it.
            app.insert_resource(osage_render::resolution::Output(Some(bevy::math::UVec2::new(width, height))))
                .insert_resource(play::OffScreen);
        }
        app.add_plugins(play::PlayPlugin { url, tier, autodrive, screenshot_after, players })
            .add_plugins(settings::SettingsPlugin { fps })
            .insert_resource(report::Pending(last_report))
            .add_plugins((report::ReportPlugin, graphics::GraphicsPlugin));
        if chosen {
            app.insert_resource(graphics::Chosen);
        }
        // Music and voice need the sound a window's game has.
        #[cfg(feature = "sound")]
        if !headless {
            app.add_plugins((music::MusicPlugin, voice::VoicePlugin));
        }
    }
    app.run();
    report::finish();
    Ok(())
}
