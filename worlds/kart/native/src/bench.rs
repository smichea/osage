// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The bench of the rendering programme (design/03 §3): a timedemo. Without
//! a server, the world's AI drivers race the karts round the circuit with
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
        Render, RenderApp, RenderSystems,
        render_resource::PollType,
        renderer::{RenderAdapterInfo, RenderDevice, RenderQueue},
        view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    },
    time::TimeUpdateStrategy,
};
use osage_kart_physics::{DT, Kart, bump, step};
use osage_kart_racecraft::{Mind, Temper, drive, field};
use osage_render::{
    Tier,
    resolution::{Output, Picture},
};
use osage_world_kart::Circuit;
use serde_json::json;

use crate::scene::{Course, Followed, Part, Pose, Stage};

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
    /// The time of day (`--sky`): it changes the picture.
    pub sky: String,
    /// No window: nothing paces the frames but the bench itself.
    pub headless: bool,
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
                sky: self.sky.clone(),
                shots,
                frame: 0,
                last: None,
                times: Vec::new(),
                slowest: Vec::new(),
                warm_up_worst: 0.0,
                skip: 0,
                saved: Vec::new(),
                settled: 0,
            })
            .add_systems(Startup, line_up)
            .add_systems(Update, simulate.in_set(Stage::Simulate).run_if(started))
            .add_systems(Last, (settle, measure.run_if(started)).chain());
        // A window paces frames: drawing waits for the screen, and the screen
        // for the GPU, with a frame in flight. Without one, the processor would
        // queue frames far ahead of the GPU and then stall until all are drawn:
        // frames of a few milliseconds, then one of seconds. The render thread
        // keeps one frame in flight instead: once it has handed frame N to the
        // GPU, it waits for frame N - 1 to be done.
        if self.headless
            && let Some(render) = app.get_sub_app_mut(RenderApp)
        {
            render.init_resource::<InFlight>().add_systems(Render, keep_one_in_flight.in_set(RenderSystems::Cleanup));
        }
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
    sky: String,
    shots: Vec<u32>,
    frame: u32,
    last: Option<Instant>,
    /// Timed frames, milliseconds.
    times: Vec<f32>,
    /// The three slowest timed frames and their numbers: where a stall is.
    slowest: Vec<(u32, f32)>,
    warm_up_worst: f32,
    skip: u32,
    saved: Vec<String>,
    /// Frames in a row with the circuit built and nothing left to load.
    settled: u32,
}

/// The race starts once the circuit is built and every model it places has
/// loaded: loading takes its own time on each machine, the race must not.
const SETTLED: u32 = 3;

fn settle(mut bench: ResMut<Bench>, waiting: Res<osage_props::Waiting>, parts: Query<(), With<Part>>) {
    if bench.settled < SETTLED {
        bench.settled = if !parts.is_empty() && waiting.0 == 0 { bench.settled + 1 } else { 0 };
    }
}

fn started(bench: Res<Bench>) -> bool {
    bench.settled >= SETTLED
}

/// Whether the frame handed to the GPU last is done.
#[derive(Resource, Default)]
struct InFlight(Option<std::sync::Arc<std::sync::atomic::AtomicBool>>);

fn keep_one_in_flight(device: Res<RenderDevice>, queue: Res<RenderQueue>, mut in_flight: ResMut<InFlight>) {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let done = Arc::new(AtomicBool::new(false));
    let signal = done.clone();
    queue.on_submitted_work_done(move || signal.store(true, Ordering::Release));
    if let Some(previous) = in_flight.0.replace(done) {
        // Callbacks run when the device is polled.
        while !previous.load(Ordering::Acquire) {
            let _ = device.poll(PollType::Poll);
            std::thread::sleep(Duration::from_micros(100));
        }
    }
}

/// A kart of the local race and the driver racing it.
#[derive(Component)]
struct Racer {
    kart: Kart,
    temper: Temper,
    mind: Mind,
}

fn line_up(mut commands: Commands) {
    let circuit = Circuit::new();
    for slot in 0..KARTS {
        // The followed kart starts last and is the fastest: it works its way
        // through the field, which keeps karts in view. The others have
        // temperaments drawn from their place, slower towards the front.
        let last = slot == KARTS - 1;
        let temper = if last {
            Temper { pace: 1.0, consistency: 1.0, ..Temper::HOTHEAD }
        } else {
            Temper { pace: 0.8 + 0.02 * slot as f32, ..Temper::of(slot as u32) }
        };
        let kart = circuit.grid_slot(slot);
        let racer = Racer { kart, temper, mind: Mind::new(slot as u32 + 1) };
        let mut racer = commands.spawn((racer, Pose { kart, color: slot as u32, held: None, smoke: None }));
        if last {
            racer.insert(Followed);
        }
    }
    commands.insert_resource(Course(circuit));
}

/// One physics step per frame: each driver races the field
/// ([osage-kart-racecraft](osage_kart_racecraft)), then the karts collide.
fn simulate(course: Res<Course>, mut racers: Query<(&mut Racer, &mut Pose)>) {
    let track = &course.0.track;
    let karts: Vec<Kart> = racers.iter().map(|(racer, _)| racer.kart).collect();
    let field = field(track, &karts);
    for (me, (mut racer, _)) in racers.iter_mut().enumerate() {
        let Racer { kart, temper, mind } = &mut *racer;
        let controls = drive(kart, me, &field, temper, mind, track);
        step(kart, controls, track);
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
        Some(ms) => {
            bench.times.push(ms);
            bench.slowest.push((frame, ms));
            bench.slowest.sort_by(|a, b| b.1.total_cmp(&a.1));
            bench.slowest.truncate(3);
        }
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
        "bench": "kart-lap/3",
        "tier": bench.tier.to_string(),
        "without": bench.without,
        "camera": if bench.held { "held" } else { "race" },
        "sky": bench.sky,
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
        "assets": osage_content::root().is_some(),
        "frames": { "total": bench.total, "timed": sorted.len(), "warm_up": WARM_UP },
        "frame_ms": {
            "mean": round(mean),
            "p50": round(percentile(50)),
            "p95": round(percentile(95)),
            "p99": round(percentile(99)),
            "worst": round(sorted[sorted.len() - 1]),
            "warm_up_worst": round(bench.warm_up_worst),
        },
        "slowest": bench.slowest.iter().map(|&(frame, ms)| json!({ "frame": frame, "ms": round(ms) })).collect::<Vec<_>>(),
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
        "kart-lap/3 · {tier} · {size} · {gpu}\n  frame time: mean {mean:.2} ms ({fps:.0} fps) · p50 {p50:.2} · p95 {p95:.2} · p99 {p99:.2} · worst {worst:.2} (frame {worst_at})\n  \
         budget {budget:.1} ms: {over:.1} % of frames over · warm-up worst {warm:.0} ms\n  written to {out}",
        gpu = adapter.name,
        fps = 1000.0 / mean,
        p50 = percentile(50),
        p95 = percentile(95),
        p99 = percentile(99),
        worst = sorted[sorted.len() - 1],
        worst_at = bench.slowest.first().map_or(0, |&(frame, _)| frame),
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
