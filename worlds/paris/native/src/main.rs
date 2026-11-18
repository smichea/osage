// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `paris`: a walk on foot in a street of Paris, Amazon Lumberyard's Bistro
//! (D-136).
//!
//!     paris [ws://HOST:7878] [--tier T1..T4] [--street DIR] [--offline] [--fov DEGREES]
//!           [--shot FILE --at X,Y,Z --heading DEGREES]
//!
//! The walker goes where the keys say (W A S D or Z Q S D, the arrows;
//! Shift runs), looks where the mouse does once the window is clicked
//! (Escape lets the mouse go). Around it, the 9 cubes of 3 m of its block
//! are drawn from the street's model; beyond them, the light coming into
//! the block, as the world's server of light sends it from where the
//! walker is and how fast it goes. `--offline` walks without a server,
//! taking the light from the street's own folder; `--shot` draws one
//! picture from a place (`--at`, the eyes) once the light has come, saves
//! it and leaves. DIR is what `tools/bistro/export.py` and the tiles' bake
//! made (default: `orca/bistro/out` under the asset manifest's content).

mod hud;
mod light;
mod net;
mod street;
mod walker;

use std::path::PathBuf;

use bevy::{asset::AssetPlugin, prelude::*, window::ExitCondition, winit::WinitPlugin};
use osage_render::{OsageRenderPlugin, Technique, Tier};
use osage_world_paris::walk::Walk;

/// Where the street's files are, and how the client runs.
#[derive(Resource, Clone)]
pub struct Options {
    pub street: PathBuf,
    pub server: Option<String>,
    pub shot: Option<Shot>,
    /// The camera's field, top to bottom, degrees.
    pub fov: f32,
}

/// One picture from a place, then leave.
#[derive(Clone, Debug)]
pub struct Shot {
    pub file: PathBuf,
    pub eyes: Vec3,
    pub heading: f32,
}

/// The renderer's radiance (Cycles: 1 the white of a sunny day's picture)
/// as luminance, cd/m², under the sunny day's exposure the renderer of
/// osage keeps (EV 15): what a world lit as Cycles lit it multiplies by.
pub const NITS: f32 = 1.2 * 32768.0;

/// What is missing, as an error (anyhow's `context`, named apart from Bevy's).
fn need<T>(value: Option<T>, what: &str) -> anyhow::Result<T> {
    value.ok_or_else(|| anyhow::anyhow!("{what}"))
}

fn read<T, E: std::fmt::Display>(value: Result<T, E>, what: &str) -> anyhow::Result<T> {
    value.map_err(|e| anyhow::anyhow!("{what}: {e}"))
}

fn main() -> anyhow::Result<()> {
    let mut server = Some("ws://127.0.0.1:7878".to_owned());
    let mut tier = Tier::T2;
    let mut street: Option<PathBuf> = None;
    let (mut shot_file, mut at, mut heading) = (None, None, 0.0f32);
    let mut fov = 70.0f32;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--tier" => tier = need(args.next(), "--tier T1..T4")?.parse().map_err(anyhow::Error::msg)?,
            "--street" => street = Some(need(args.next(), "--street needs a folder")?.into()),
            "--offline" => server = None,
            "--shot" => shot_file = Some(PathBuf::from(need(args.next(), "--shot needs a file")?)),
            "--at" => {
                let text = need(args.next(), "--at X,Y,Z")?;
                let n: Vec<f32> = text.split(',').map(str::parse).collect::<Result<_, _>>()?;
                anyhow::ensure!(n.len() == 3, "--at X,Y,Z");
                at = Some(Vec3::new(n[0], n[1], n[2]));
            }
            "--heading" => heading = need(args.next(), "--heading DEGREES")?.parse::<f32>()?.to_radians(),
            "--fov" => fov = need(args.next(), "--fov DEGREES")?.parse::<f32>()?,
            url if url.starts_with("ws://") || url.starts_with("wss://") => server = Some(url.to_owned()),
            other => anyhow::bail!("unknown argument {other}"),
        }
    }
    let root = need(osage_content::root(), "the asset manifest's content is not here (OSAGE_ASSETS)")?;
    let street = street.unwrap_or_else(|| root.join("orca/bistro/out"));
    let walk = Walk::parse(
        &read(std::fs::read_to_string(street.join("walk.json")), "walk.json (tools/bistro/export.py)")?,
        &read(std::fs::read(street.join("walk.bin")), "walk.bin")?,
    )
    .map_err(anyhow::Error::msg)?;
    let shot = match shot_file {
        Some(file) => Some(Shot { file, eyes: need(at, "--shot needs --at X,Y,Z")?, heading }),
        None => None,
    };
    let online = server.is_some();
    let options = Options { street, server, shot: shot.clone(), fov };

    let assets = AssetPlugin { file_path: root.to_string_lossy().into_owned(), ..default() };
    let mut app = App::new();
    if shot.is_some() {
        // No window: the picture is drawn off screen.
        let windowless = WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() };
        app.add_plugins(DefaultPlugins.set(windowless).set(assets).disable::<WinitPlugin>())
            .add_plugins(bevy::app::ScheduleRunnerPlugin::run_loop(std::time::Duration::ZERO))
            // osage-render's picture at a fixed size, the shot taken from it.
            .insert_resource(osage_render::resolution::Output(Some(UVec2::new(1280, 720))));
    } else {
        let window = Window { title: "osage — Paris".to_owned(), ..default() };
        app.add_plugins(DefaultPlugins.set(WindowPlugin { primary_window: Some(window), ..default() }).set(assets));
    }
    // The sky is the light coming into the block, and it lights the block
    // too: osage's own sky and sky light are left out.
    app.add_plugins(OsageRenderPlugin { tier, without: vec![Technique::Sky, Technique::SkyLight] })
        .insert_resource(options)
        .insert_resource(walker::Grid(std::sync::Arc::new(walk)))
        .add_plugins((street::StreetPlugin, light::LightPlugin, walker::WalkerPlugin, hud::HudPlugin));
    if osage_humans::installed(&root) {
        app.add_plugins(osage_humans::HumansPlugin);
    }
    if online {
        app.add_plugins(net::NetPlugin);
    }
    app.run();
    Ok(())
}
