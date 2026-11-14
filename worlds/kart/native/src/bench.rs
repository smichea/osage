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
    /// The track it races on (`--track`, D-056).
    pub track: osage_kart_tracks::TrackDef,
    /// A film (`--film FROM,TO,EVERY`): a picture of every `EVERY`th frame
    /// from `FROM` to `TO`, in place of the four screenshots. The bench
    /// steps time by a fixed amount, so the pictures are those of a smooth
    /// film whatever the machine's speed; its timings are not a bench's.
    pub film: Option<(u32, u32, u32)>,
}

impl Plugin for BenchPlugin {
    fn build(&self, app: &mut App) {
        // Screenshots at fixed frames spread over the run.
        let shots = match self.film {
            Some((from, to, every)) => (from..=to).step_by(every.max(1) as usize).collect(),
            None => [1, 3, 5, 7].map(|eighths| self.frames * eighths / 8).to_vec(),
        };
        app.add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin)
            .insert_resource(crate::crowd::CrowdSeed(0))
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(DT)))
            .insert_resource(Output(Some(self.resolution.into())))
            .insert_resource(Bench {
                tier: self.tier,
                total: self.frames,
                out: self.out.clone(),
                resolution: self.resolution,
                without: self.without.clone(),
                held: self.held,
                sky: self.sky.clone(),
                track: self.track.clone(),
                shots,
                frame: 0,
                last: None,
                times: Vec::new(),
                slowest: Vec::new(),
                gpu: 0.0,
                began: None,
                census: [0; 4],
                warm_up_worst: 0.0,
                skip: 0,
                saved: Vec::new(),
                passes: Default::default(),
                race: 0,
                settled: 0,
            })
            .add_systems(Startup, line_up)
            .add_systems(First, main_begins)
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
        // The render world's own processor time, from extraction to its
        // cleanup (before waiting for the GPU), for the slowest frames.
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .init_resource::<RenderBegan>()
                .add_systems(bevy::render::ExtractSchedule, render_begins)
                .add_systems(Render, render_ends.in_set(RenderSystems::Render).before(RenderSystems::Cleanup));
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
    track: osage_kart_tracks::TrackDef,
    shots: Vec<u32>,
    frame: u32,
    last: Option<Instant>,
    /// Timed frames, milliseconds.
    times: Vec<f32>,
    /// The three slowest timed frames and their numbers: where a stall is;
    /// with the GPU's time in that frame's passes, to tell a GPU's stall
    /// from the processor's (D-123).
    slowest: Vec<(u32, f32, f32, f32, f32)>,
    /// The GPU's time in all passes, as last reported, ms.
    gpu: f32,
    /// When this frame's main world began.
    began: Option<Instant>,
    /// Entities, meshes, and meshes with a visibility range, at the end of the warm-up.
    census: [usize; 4],
    warm_up_worst: f32,
    skip: u32,
    saved: Vec<String>,
    /// What each pass of the renderer cost the GPU, ms: (sum, frames,
    /// worst), by the pass's name (D-121).
    passes: std::collections::BTreeMap<String, (f64, u32, f64)>,
    /// The race so far, as a number (D-115): every kart's place and heading
    /// at the last step, in grid order. The same on every run, or the runs
    /// did not time the same frames.
    race: u64,
    /// Frames in a row with the circuit built and nothing left to load.
    settled: u32,
}

/// The race starts once the circuit is built and every model it places has
/// loaded: loading takes its own time on each machine, the race must not.
const SETTLED: u32 = 3;

/// The render world's processor time in its last frame, ms, as f32 bits:
/// from extraction to its cleanup (D-123).
static RENDER_CPU: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

#[derive(Resource, Default)]
struct RenderBegan(Option<Instant>);

fn render_begins(mut began: ResMut<RenderBegan>) {
    began.0 = Some(Instant::now());
}

fn render_ends(began: Res<RenderBegan>) {
    if let Some(began) = began.0 {
        let ms = began.elapsed().as_secs_f32() * 1000.0;
        RENDER_CPU.store(ms.to_bits(), std::sync::atomic::Ordering::Relaxed);
    }
}

fn main_begins(mut bench: ResMut<Bench>) {
    bench.began = Some(Instant::now());
}

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
    /// Its place on the grid: what orders the karts, whatever order the
    /// engine keeps their entities in.
    slot: usize,
    kart: Kart,
    temper: Temper,
    mind: Mind,
}

fn line_up(mut commands: Commands, bench: Res<Bench>) {
    let circuit = Circuit::of(&bench.track);
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
        let racer = Racer { slot, kart, temper, mind: Mind::new(slot as u32 + 1) };
        let mut racer = commands.spawn((
            racer,
            Pose {
                kart,
                color: slot as u32,
                held: None,
                smoke: None,
                trailing: false,
                guard: 0.0,
                ghost: false,
                cheer: false,
            },
        ));
        if last {
            racer.insert(Followed);
        }
    }
    commands.insert_resource(Course(circuit));
}

/// One physics step per frame: each driver races the field
/// ([osage-kart-racecraft](osage_kart_racecraft)), then the karts collide.
fn simulate(course: Res<Course>, mut bench: ResMut<Bench>, mut racers: Query<(&mut Racer, &mut Pose)>) {
    let track = &course.0.track;
    // In grid order, whatever order the engine keeps the entities in: the
    // order karts are stepped and knocked in is part of the race (D-115).
    let mut grid: Vec<_> = racers.iter_mut().collect();
    grid.sort_unstable_by_key(|(racer, _)| racer.slot);
    let karts: Vec<Kart> = grid.iter().map(|(racer, _)| racer.kart).collect();
    let field = field(track, &karts);
    for (me, (racer, _)) in grid.iter_mut().enumerate() {
        let Racer { kart, temper, mind, .. } = &mut **racer;
        let controls = drive(kart, me, &field, temper, mind, track);
        step(kart, controls, track);
    }
    for a in 0..grid.len() {
        let (before, after) = grid.split_at_mut(a + 1);
        for (b, _) in after {
            bump(&mut before[a].0.kart, &mut b.kart);
        }
    }
    for (racer, pose) in &mut grid {
        pose.kart = racer.kart;
    }
    drop(grid);
    let mut places: Vec<(usize, [u32; 3])> = racers
        .iter()
        .map(|(racer, _)| (racer.slot, [racer.kart.x.to_bits(), racer.kart.z.to_bits(), racer.kart.heading.to_bits()]))
        .collect();
    places.sort_unstable();
    // FNV-1a over the karts' bits.
    bench.race = places
        .iter()
        .flat_map(|(_, bits)| bits.iter().flat_map(|b| b.to_le_bytes()))
        .fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3));
}

#[allow(clippy::too_many_arguments)]
fn measure(
    mut commands: Commands,
    mut bench: ResMut<Bench>,
    picture: Res<Picture>,
    composite: Option<Res<crate::preview::Composite>>,
    adapter: Res<RenderAdapterInfo>,
    diagnostics: Option<Res<bevy::diagnostic::DiagnosticsStore>>,
    entities: Query<(Has<Mesh3d>, Has<bevy::camera::visibility::VisibilityRange>, Option<&osage_props::Level>)>,
    mut exit: MessageWriter<AppExit>,
) {
    // What the engine walks every frame, counted once the race is under way (D-123).
    if bench.frame == WARM_UP {
        let (mut all, mut meshes, mut ranged, mut levelled) = (0, 0, 0, 0);
        for (mesh, range, level) in &entities {
            all += 1;
            meshes += usize::from(mesh);
            ranged += usize::from(mesh && range);
            levelled += usize::from(level.is_some());
        }
        bench.census = [all, meshes, ranged, levelled];
    }
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

    // Where the frame's time goes: the GPU's time in each pass, as the
    // renderer's own spans give it (a frame or two late, which a mean does
    // not mind).
    if frame > WARM_UP
        && let Some(diagnostics) = &diagnostics
    {
        for diagnostic in diagnostics.iter() {
            let path = diagnostic.path().as_str();
            let (Some(name), Some(ms)) =
                (path.strip_prefix("render/").and_then(|p| p.strip_suffix("/elapsed_gpu")), diagnostic.value())
            else {
                continue;
            };
            let pass = bench.passes.entry(name.to_owned()).or_insert((0.0, 0, 0.0));
            *pass = (pass.0 + ms, pass.1 + 1, pass.2.max(ms));
        }
        // The innermost spans only: a span inside another ("fxaa/fxaa") is
        // part of it, and some passes are only their spans ("solari_lighting/…").
        let spans: Vec<(&str, f64)> = diagnostics
            .iter()
            .filter_map(|d| {
                let name = d.path().as_str().strip_prefix("render/")?.strip_suffix("/elapsed_gpu")?;
                Some((name, d.value()?))
            })
            .collect();
        bench.gpu = spans
            .iter()
            .filter(|(name, _)| {
                !spans.iter().any(|(other, _)| other.len() > name.len() && other.starts_with(&format!("{name}/")))
            })
            .map(|(_, ms)| ms)
            .sum::<f64>() as f32;
    }
    match elapsed {
        Some(ms) if frame <= WARM_UP => bench.warm_up_worst = bench.warm_up_worst.max(ms),
        Some(_) if bench.skip > 0 => bench.skip -= 1,
        Some(ms) => {
            bench.times.push(ms);
            let gpu = bench.gpu;
            let main = bench.began.map_or(0.0, |began| (now - began).as_secs_f32() * 1000.0);
            let render = f32::from_bits(RENDER_CPU.load(std::sync::atomic::Ordering::Relaxed));
            bench.slowest.push((frame, ms, gpu, main, render));
            bench.slowest.sort_by(|a, b| b.1.total_cmp(&a.1));
            bench.slowest.truncate(3);
        }
        None => {}
    }
    if bench.shots.contains(&frame) {
        let name = format!("shot-{frame:05}.png");
        commands
            // With the interface over it, when it is previewed (D-078).
            .spawn(Screenshot::image(composite.map_or_else(|| picture.image.clone(), |c| c.0.clone())))
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
        "track": bench.track.id,
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
        "entities": { "all": bench.census[0], "meshes": bench.census[1], "meshes_with_range": bench.census[2], "placed_levels": bench.census[3] },
        // The GPU's time in each pass, ms: mean and worst, the costliest first (D-121).
        "passes_ms": passes(bench),
        // The race as a number: the same on every run that raced the same frames (D-115).
        "race": format!("{:016x}", bench.race),
        "frame_ms": {
            "mean": round(mean),
            "p50": round(percentile(50)),
            "p95": round(percentile(95)),
            "p99": round(percentile(99)),
            "worst": round(sorted[sorted.len() - 1]),
            "warm_up_worst": round(bench.warm_up_worst),
        },
        "slowest": bench
            .slowest
            .iter()
            .map(|&(frame, ms, gpu, main, render)| {
                json!({ "frame": frame, "ms": round(ms), "gpu_ms": round(gpu), "main_ms": round(main), "render_cpu_ms": round(render) })
            })
            .collect::<Vec<_>>(),
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
        worst_at = bench.slowest.first().map_or(0, |&(frame, ..)| frame),
        warm = bench.warm_up_worst,
        out = bench.out.display(),
    ))
}

/// The commit this was run from, with `-dirty` if the tree has changes. A
/// build made elsewhere than where it runs (a cloud bench machine has no
/// checkout) carries it from its build: `OSAGE_COMMIT` when it was compiled.
/// The passes' GPU times for the report: mean and worst, the costliest first.
fn passes(bench: &Bench) -> serde_json::Value {
    let mut passes: Vec<(&String, f64, f64)> = bench
        .passes
        .iter()
        .map(|(name, (sum, frames, worst))| (name, sum / f64::from((*frames).max(1)), *worst))
        .collect();
    passes.sort_by(|a, b| b.1.total_cmp(&a.1));
    let round = |ms: f64| (ms * 100.0).round() / 100.0;
    passes
        .into_iter()
        .map(|(name, mean, worst)| serde_json::json!({ "pass": name, "mean": round(mean), "worst": round(worst) }))
        .collect()
}

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
