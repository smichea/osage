// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The bench of the rendering programme (design/03 §3): a timedemo. Without
//! a server, the world's autopilot drives six karts round the circuit with
//! the world's physics, one step per frame, so that every run draws the same
//! frames whatever the machine; frames are drawn as fast as the machine can,
//! and their times recorded. It writes `report.json` and screenshots taken at
//! fixed frames, to compare a change before and after, and machines.
//!
//! The bench fixes the output size (`osage_render::resolution`): the scene is
//! drawn off screen for that size and only shown in the window, so that
//! neither the display's scale factor nor the window manager changes what
//! is measured.

use std::{
    path::PathBuf,
    time::{Duration, Instant, SystemTime},
};

use bevy::{
    app::AppExit,
    prelude::*,
    render::{
        renderer::RenderAdapterInfo,
        view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    },
    time::TimeUpdateStrategy,
};
use osage_kart_physics::{DT, Kart, bump, step};
use osage_render::{
    Tier,
    resolution::{Output, Picture},
};
use osage_world_kart::{Circuit, autopilot, proto::Autopilot};
use serde_json::json;

use crate::scene::{Course, Followed, Pose, Stage};

/// Karts on the bench's grid.
const KARTS: usize = 6;
/// Frames not timed at the start: pipelines compile, assets load. Their
/// worst frame is reported apart — first-use stutter is a defect too.
const WARM_UP: u32 = 120;
/// Frames not timed after a screenshot, which costs a read-back.
const AFTER_SHOT: u32 = 3;
/// Frames between the last timed frame and the exit, for the screenshots to be saved.
const WIND_DOWN: u32 = 30;

pub struct BenchPlugin {
    pub tier: Tier,
    pub frames: u32,
    pub out: PathBuf,
    pub resolution: (u32, u32),
    /// Techniques left out of this run, by name.
    pub without: Vec<String>,
    /// Whether the camera is held at a place (`--camera`) instead of following the race.
    pub held: bool,
}

impl Plugin for BenchPlugin {
    fn build(&self, app: &mut App) {
        // Screenshots at fixed frames spread over the run.
        let shots = [1, 3, 5, 7].map(|eighths| self.frames * eighths / 8).to_vec();
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(DT)))
            .insert_resource(Output(Some(self.resolution.into())))
            .insert_resource(Bench {
                tier: self.tier,
                total: self.frames,
                out: self.out.clone(),
                resolution: self.resolution,
                without: self.without.clone(),
                held: self.held,
                shots,
                frame: 0,
                last: None,
                times: Vec::new(),
                warm_up_worst: 0.0,
                skip: 0,
                saved: Vec::new(),
            })
            .add_systems(Startup, line_up)
            .add_systems(Update, simulate.in_set(Stage::Simulate))
            .add_systems(Last, measure);
    }
}

#[derive(Resource)]
struct Bench {
    tier: Tier,
    total: u32,
    out: PathBuf,
    resolution: (u32, u32),
    without: Vec<String>,
    held: bool,
    shots: Vec<u32>,
    frame: u32,
    last: Option<Instant>,
    /// Timed frames, milliseconds.
    times: Vec<f32>,
    warm_up_worst: f32,
    skip: u32,
    saved: Vec<String>,
}

/// A kart of the local race and how the autopilot drives it.
#[derive(Component)]
struct Racer {
    kart: Kart,
    autopilot: Autopilot,
    /// The line it keeps when nobody is in its way.
    line: f32,
}

/// How far ahead a kart looks for one in its way, metres.
const LOOK_AHEAD: f32 = 16.0;

fn line_up(mut commands: Commands) {
    let circuit = Circuit::new();
    for slot in 0..KARTS {
        // The followed kart starts last and is the fastest: it works its way
        // through the field, which keeps karts in view.
        let last = slot == KARTS - 1;
        let autopilot = Autopilot {
            pace: if last { 1.0 } else { 0.8 + 0.02 * slot as f32 },
            line: if slot % 2 == 0 { -0.3 } else { 0.3 },
            drift: true,
        };
        let (kart, line) = (circuit.grid_slot(slot), autopilot.line);
        let mut racer = commands.spawn((Racer { kart, autopilot, line }, Pose { kart, color: slot as u32 }));
        if last {
            racer.insert(Followed);
        }
    }
    commands.insert_resource(Course(circuit));
}

/// One physics step per frame. The world's autopilot does not know about
/// other karts: here a kart that finds another just ahead on its line moves
/// to the side with more room, and back to its own line once past.
fn simulate(course: Res<Course>, mut racers: Query<(&mut Racer, &mut Pose)>) {
    let track = &course.0.track;
    let room = track.half_width - 1.5;
    let spots: Vec<_> = racers.iter().map(|(racer, _)| track.locate(racer.kart.x, racer.kart.z)).collect();
    for (mut racer, _) in &mut racers {
        let me = track.locate(racer.kart.x, racer.kart.z);
        let aim = racer.autopilot.line * room;
        let in_the_way = spots.iter().find(|other| {
            let ahead = (other.along - me.along).rem_euclid(track.length());
            ahead > 0.5 && ahead < LOOK_AHEAD && (other.lateral - aim).abs() < 2.6
        });
        racer.autopilot.line = match in_the_way {
            Some(other) if other.lateral > 0.0 => -0.65,
            Some(_) => 0.65,
            // Past it: back to its own line, unless that would cut in front.
            None if (me.lateral - racer.line * room).abs() > 0.5 => {
                let clear = spots.iter().all(|other| {
                    let ahead = (other.along - me.along).rem_euclid(track.length());
                    !(ahead > 0.5 && ahead < LOOK_AHEAD) || (other.lateral - racer.line * room).abs() > 2.6
                });
                if clear { racer.line } else { racer.autopilot.line }
            }
            None => racer.line,
        };
    }
    for (mut racer, _) in &mut racers {
        let controls = autopilot(&racer.kart, &racer.autopilot, &course.0);
        step(&mut racer.kart, controls, &course.0.track);
    }
    let mut pairs = racers.iter_combinations_mut();
    while let Some([(mut a, _), (mut b, _)]) = pairs.fetch_next() {
        bump(&mut a.kart, &mut b.kart);
    }
    for (racer, mut pose) in &mut racers {
        pose.kart = racer.kart;
    }
}

fn measure(
    mut commands: Commands,
    mut bench: ResMut<Bench>,
    picture: Res<Picture>,
    adapter: Res<RenderAdapterInfo>,
    mut exit: MessageWriter<AppExit>,
) {
    let now = Instant::now();
    let elapsed = bench.last.replace(now).map(|last| (now - last).as_secs_f32() * 1000.0);
    bench.frame += 1;
    let frame = bench.frame;
    if frame > bench.total {
        if frame == bench.total + WIND_DOWN {
            exit.write(AppExit::Success);
        }
        return;
    }

    match elapsed {
        Some(ms) if frame <= WARM_UP => bench.warm_up_worst = bench.warm_up_worst.max(ms),
        Some(_) if bench.skip > 0 => bench.skip -= 1,
        Some(ms) => bench.times.push(ms),
        None => {}
    }
    if bench.shots.contains(&frame) {
        let name = format!("shot-{frame:05}.png");
        commands
            .spawn(Screenshot::image(picture.image.clone()))
            // Encoding the picture takes time that is not rendering's.
            .observe(|_: On<ScreenshotCaptured>, mut bench: ResMut<Bench>| bench.skip = bench.skip.max(1))
            .observe(save_to_disk(bench.out.join(&name)));
        bench.saved.push(name);
        bench.skip = AFTER_SHOT;
    }
    if frame == bench.total {
        match report(&bench, picture.size, &adapter) {
            Ok(summary) => println!("{summary}"),
            Err(error) => eprintln!("kart-native: cannot write the report: {error}"),
        }
    }
}

fn report(bench: &Bench, rendered: UVec2, adapter: &RenderAdapterInfo) -> anyhow::Result<String> {
    let mut sorted = bench.times.clone();
    sorted.sort_by(f32::total_cmp);
    anyhow::ensure!(!sorted.is_empty(), "no frame was timed: run more than {WARM_UP} frames");
    let percentile = |p: usize| sorted[(sorted.len() * p / 100).min(sorted.len() - 1)];
    let mean = sorted.iter().sum::<f32>() / sorted.len() as f32;
    let budget = bench.tier.budget_ms();
    let over = sorted.iter().filter(|ms| **ms > budget).count() as f32 * 100.0 / sorted.len() as f32;
    let round = |ms: f32| (f64::from(ms) * 100.0).round() / 100.0;

    let report = json!({
        "bench": "kart-lap/2",
        "tier": bench.tier.to_string(),
        "without": bench.without,
        "camera": if bench.held { "held" } else { "race" },
        "commit": commit(),
        "unix_time": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs()),
        "machine": {
            "gpu": adapter.name,
            "backend": format!("{:?}", adapter.backend),
            "device_type": format!("{:?}", adapter.device_type),
            "driver": format!("{} {}", adapter.driver, adapter.driver_info).trim(),
            "cpu": cpu(),
            "os": std::env::consts::OS,
        },
        "resolution": [bench.resolution.0, bench.resolution.1],
        "rendered": [rendered.x, rendered.y],
        // Whether the asset manifest's content was there: it changes the picture.
        "assets": crate::content::root().is_some(),
        "frames": { "total": bench.total, "timed": sorted.len(), "warm_up": WARM_UP },
        "frame_ms": {
            "mean": round(mean),
            "p50": round(percentile(50)),
            "p95": round(percentile(95)),
            "p99": round(percentile(99)),
            "worst": round(sorted[sorted.len() - 1]),
            "warm_up_worst": round(bench.warm_up_worst),
        },
        "fps_mean": round(1000.0 / mean),
        "budget_ms": round(budget),
        "over_budget_percent": round(over),
        "screenshots": bench.saved,
    });
    std::fs::create_dir_all(&bench.out)?;
    std::fs::write(bench.out.join("report.json"), serde_json::to_string_pretty(&report)? + "\n")?;

    let size = if rendered == UVec2::from(bench.resolution) {
        format!("{}×{}", rendered.x, rendered.y)
    } else {
        format!("{}×{} (drawn at {}×{})", bench.resolution.0, bench.resolution.1, rendered.x, rendered.y)
    };
    let tier = match bench.without.as_slice() {
        [] => bench.tier.to_string(),
        without => format!("{} without {}", bench.tier, without.join(", ")),
    };
    Ok(format!(
        "kart-lap/2 · {tier} · {size} · {gpu}\n  frame time: mean {mean:.2} ms ({fps:.0} fps) · p50 {p50:.2} · p95 {p95:.2} · p99 {p99:.2} · worst {worst:.2}\n  \
         budget {budget:.1} ms: {over:.1} % of frames over · warm-up worst {warm:.0} ms\n  written to {out}",
        gpu = adapter.name,
        fps = 1000.0 / mean,
        p50 = percentile(50),
        p95 = percentile(95),
        p99 = percentile(99),
        worst = sorted[sorted.len() - 1],
        warm = bench.warm_up_worst,
        out = bench.out.display(),
    ))
}

/// The commit this was run from, with `-dirty` if the tree has changes. A
/// build made elsewhere than where it runs (a cloud bench machine has no
/// checkout) carries it from its build: `OSAGE_COMMIT` when it was compiled.
fn commit() -> String {
    if let Some(commit) = option_env!("OSAGE_COMMIT") {
        return commit.to_owned();
    }
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git").args(args).output().ok()?;
        output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    match (git(&["rev-parse", "--short", "HEAD"]), git(&["status", "--porcelain"])) {
        (Some(commit), Some(changes)) if changes.is_empty() => commit,
        (Some(commit), _) => format!("{commit}-dirty"),
        _ => "unknown".to_owned(),
    }
}

fn cpu() -> String {
    std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|info| {
            info.lines()
                .find(|l| l.starts_with("model name"))
                .map(|l| l.split(':').nth(1).unwrap_or("").trim().to_owned())
        })
        .unwrap_or_else(|| "unknown".to_owned())
}
