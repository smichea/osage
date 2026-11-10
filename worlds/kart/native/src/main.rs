// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `kart-native`: the native client of the kart world, and the testbed of the
//! rendering programme (design/03).
//!
//! - `kart-native [ws://host:port] [--tier T1|T2|T3|T4]` joins a kart world
//!   (default `ws://127.0.0.1:7878`, tier T2). Drive with the arrows or WASD,
//!   drift with Space or Shift; a gamepad works too. F12 saves a screenshot.
//!   `--autodrive` lets the world's autopilot hold the wheel;
//!   `--screenshot-after SECONDS` saves a screenshot by itself.
//! - `kart-native --bench [--tier T…] [--frames N] [--resolution WxH] [--out DIR]`
//!   runs the bench without a server and writes `report.json` and screenshots
//!   to `bench/out/<tier>-<time>/`.
//! - `--without TECHNIQUE` (sky, shadows, ambient-occlusion, post; repeatable)
//!   leaves a rendering technique out, to measure its cost or to find the
//!   cause of a defect.

mod bench;
mod content;
mod effects;
mod karts;
mod play;
mod predict;
mod scene;
mod scenery;
mod track;

use std::{path::PathBuf, time::SystemTime};

use anyhow::{Context, Result, bail};
// Not Bevy's prelude here: its error helpers clash with anyhow's.
use bevy::{
    DefaultPlugins,
    app::{App, PluginGroup},
    render::RenderPlugin,
    window::{PresentMode, Window, WindowPlugin, WindowResolution},
};
use osage_render::{OsageRenderPlugin, Tier};
use osage_sdk::v0;

fn main() -> Result<()> {
    let (mut url, mut tier, mut bench) = ("ws://127.0.0.1:7878".to_owned(), Tier::T2, false);
    let (mut frames, mut resolution, mut out, mut without) = (2400u32, None, None, Vec::new());
    let (mut autodrive, mut screenshot_after) = (false, None);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().with_context(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--bench" => bench = true,
            "--autodrive" => autodrive = true,
            "--screenshot-after" => screenshot_after = Some(value()?.parse().context("--screenshot-after")?),
            "--tier" => tier = value()?.parse().map_err(anyhow::Error::msg)?,
            "--frames" => frames = value()?.parse().context("--frames")?,
            "--out" => out = Some(PathBuf::from(value()?)),
            "--without" => without.push(value()?.parse().map_err(anyhow::Error::msg)?),
            "--resolution" => {
                let value = value()?;
                let (width, height) = value.split_once(['x', 'X']).context("--resolution takes WIDTHxHEIGHT")?;
                resolution = Some((width.parse().context("width")?, height.parse().context("height")?));
            }
            flag if flag.starts_with("--") => bail!("unknown argument {flag}"),
            _ => url = arg,
        }
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
    app.add_plugins(
        DefaultPlugins.set(WindowPlugin { primary_window: Some(window), ..Default::default() }).set(render),
    )
    .insert_resource(scene::look())
    .add_plugins(OsageRenderPlugin { tier, without: without.clone() })
    .add_plugins(scene::ScenePlugin);
    if bench {
        let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let out = out.unwrap_or_else(|| PathBuf::from(format!("bench/out/{tier}-{now}")));
        std::fs::create_dir_all(&out).with_context(|| format!("creating {}", out.display()))?;
        let without = without.iter().map(|t| t.name().to_owned()).collect();
        app.add_plugins(bench::BenchPlugin { tier, frames, out, resolution: (width, height), without });
    } else {
        let tier = match tier {
            Tier::T1 => v0::Tier::T1Minimal,
            Tier::T2 => v0::Tier::T2Standard,
            Tier::T3 => v0::Tier::T3High,
            Tier::T4 => v0::Tier::T4Ultra,
        };
        app.add_plugins(play::PlayPlugin { url, tier, autodrive, screenshot_after });
    }
    app.run();
    Ok(())
}
