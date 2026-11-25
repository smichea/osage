// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

mod bench;
mod compressed_texture;
mod impact;
mod object;
mod paging;
mod people;
mod physics;
mod replay;
mod scene;
mod texture;
mod view;

use anyhow::{Context, Result, ensure};
use impact::{Listening, Playback};
use osage_dynamics::Engine;
use people::Model;
use physics::Physics;
use replay::Replay;
use scene::Sector;
use serde_json::json;
use std::{env, fs, path::PathBuf};

const HELP: &str =
    "ville-paris [--scene FILE.glb] [--replay FILE.json] [--route ID] [--physics rapier|jolt|point] [--actor capsule|human | --actor-rig FILE.glb] [--full-detail]
           [--shot FILE.png --tick N | --bench DIR | --verify DIR | --render-bench FILE.json]
           [--visual-lod LEVEL] [--collision-lod LEVEL] [--acoustic-lod LEVEL] [--inspect-object [--ray X,Y,Z X,Y,Z]... [--detail-view EYE TARGET FOV_DEG HEIGHT_PX]...] | --inspect-textures | --predict-impact | --impact-response | --time-physics-from TICK]
           [--impact-wav FILE.wav | --mute] [--listening speakers|headphones] [--direct-only] [--paging --help] [--room-audio --help]
--time-physics-from warms up to TICK, then times the remaining rigid physics steps.
The window defaults to Rapier and an articulated human. --bench alone runs the point control.
Levels: high, medium, low, veryLow.
Window controls: 1/2/3 select a drop, R restarts, Space pauses or resumes, Escape exits.";

fn main() -> Result<()> {
    match env::args().nth(1).as_deref() {
        Some("--paging") => return paging::cli(env::args().skip(2)),
        Some("--room-audio") => return impact::room_audio::cli(env::args().skip(2)),
        _ => {}
    }
    let mut args = env::args().skip(1);
    let (mut scene, mut replay) = (None, None);
    let mut full_detail = false;
    let mut impact_wav = None;
    let mut mute = false;
    let mut listening = Listening::Speakers;
    let mut direct_only = false;
    let mut inspect = false;
    let mut predict = false;
    let mut impact_response = false;
    let mut time_physics_from = None;
    let mut textures = false;
    let mut rays = Vec::new();
    let mut detail_views = Vec::new();
    let mut fixed_visual = false;
    let mut levels = osage_objects::GeometryLevels::default();
    let mut physics = None;
    let mut model = None;
    let mut actor_rig = None;
    let (mut route, mut tick) = (None, None);
    let (mut shot, mut bench_dir, mut verify_dir, mut render_bench) = (None, None, None, None);
    while let Some(a) = args.next() {
        let mut value = || args.next().with_context(|| format!("missing value for {a}"));
        match a.as_str() {
            "--impact-wav" => impact_wav = Some(PathBuf::from(value()?)),
            "--mute" => mute = true,
            "--direct-only" => direct_only = true,
            "--listening" => {
                listening = match value()?.as_str() {
                    "speakers" => Listening::Speakers,
                    "headphones" => Listening::Headphones,
                    mode => anyhow::bail!("unknown listening mode {mode}, expected speakers or headphones"),
                }
            }
            "--predict-impact" => predict = true,
            "--impact-response" => impact_response = true,
            "--time-physics-from" => time_physics_from = Some(value()?.parse::<u32>()?),
            "--inspect-object" => inspect = true,
            "--inspect-textures" => textures = true,
            "--ray" => rays.push((point(value()?)?, point(value()?)?)),
            "--detail-view" => {
                let eye = bevy::prelude::Vec3::from(point(value()?)?).as_dvec3();
                let target = bevy::prelude::Vec3::from(point(value()?)?).as_dvec3();
                let fov = value()?.parse::<f64>()?.to_radians();
                let height = value()?.parse::<u32>()?;
                detail_views.push(
                    osage_objects::DetailView::new(eye, target - eye, fov, height, 0.1)
                        .context("invalid detail view")?,
                );
            }
            "--visual-lod" | "--collision-lod" | "--acoustic-lod" => {
                let level = serde_json::from_value(serde_json::Value::String(value()?))?;
                match a.as_str() {
                    "--visual-lod" => {
                        levels.visual = level;
                        fixed_visual = true;
                    }
                    "--collision-lod" => levels.collision = level,
                    _ => levels.acoustic = level,
                }
            }
            "--full-detail" => full_detail = true,
            "--scene" => scene = Some(fs::read(value()?)?),
            "--replay" => replay = Some(fs::read(value()?)?),
            "--route" => route = Some(value()?),
            "--actor-rig" => actor_rig = Some(fs::read(value()?)?),
            "--actor" => model = Some(value()?.parse::<Model>()?),
            "--physics" => physics = Some(value()?.parse::<Physics>()?),
            "--tick" => tick = Some(value()?.parse::<u32>()?),
            "--shot" => shot = Some(PathBuf::from(value()?)),
            "--render-bench" => render_bench = Some(PathBuf::from(value()?)),
            "--bench" => bench_dir = Some(PathBuf::from(value()?)),
            "--verify" => verify_dir = Some(PathBuf::from(value()?)),
            "--help" | "-h" => {
                println!("{HELP}");
                return Ok(());
            }
            _ => anyhow::bail!("unknown option {a}\n{HELP}"),
        }
    }
    ensure!(
        [
            shot.is_some(),
            bench_dir.is_some(),
            verify_dir.is_some(),
            render_bench.is_some(),
            inspect,
            textures,
            predict,
            impact_response,
            time_physics_from.is_some(),
            impact_wav.is_some()
        ]
        .into_iter()
        .filter(|b| *b)
        .count()
            <= 1,
        "choose one output mode"
    );
    ensure!(tick.is_none() || shot.is_some(), "--tick needs --shot");
    ensure!(route.is_none() || (bench_dir.is_none() && verify_dir.is_none()), "the CPU bench checks all routes");
    ensure!(!full_detail || (bench_dir.is_none() && verify_dir.is_none()), "--full-detail applies to pictures");
    let scene_bytes = scene.as_deref().unwrap_or(scene::DEFAULT);
    let replay_bytes = replay.as_deref().unwrap_or(replay::DEFAULT);
    let mut inputs = json!({"scene_sha256":bench::sha(scene_bytes),"replay_sha256":bench::sha(replay_bytes)});
    if let Some(bytes) = actor_rig {
        ensure!(model.is_none(), "choose either --actor or --actor-rig");
        inputs["actor_sha256"] = bench::sha(&bytes).into();
        model = Some(Model::read_rig(&bytes)?);
    }
    ensure!(
        detail_views.is_empty() || inspect && !fixed_visual,
        "--detail-view requires --inspect-object and automatic detail"
    );
    let adaptive = !fixed_visual
        && !full_detail
        && (!detail_views.is_empty()
            || !inspect
                && impact_wav.is_none()
                && !predict
                && !impact_response
                && time_physics_from.is_none()
                && bench_dir.is_none()
                && verify_dir.is_none());
    if textures {
        ensure!(
            route.is_none()
                && model.is_none()
                && physics.is_none()
                && replay.is_none()
                && rays.is_empty()
                && detail_views.is_empty()
                && !full_detail
                && !fixed_visual
                && levels == Default::default(),
            "texture inspection accepts a scene"
        );
        println!("{}", serde_json::to_string_pretty(&object::inspect_textures(scene_bytes)?)?);
        return Ok(());
    }
    let scene = if impact_wav.is_none()
        && !inspect
        && !predict
        && !impact_response
        && time_physics_from.is_none()
        && bench_dir.is_none()
        && verify_dir.is_none()
    {
        Sector::read_for_view(scene_bytes, levels, adaptive)?
    } else if adaptive {
        Sector::read_adaptive(scene_bytes, levels)?
    } else if levels == Default::default() {
        Sector::read(scene_bytes)?
    } else {
        Sector::read_levels(scene_bytes, levels)?
    };
    if levels.collision != osage_objects::VisualLod::High || levels.acoustic != osage_objects::VisualLod::High {
        inputs["geometry_levels"] = serde_json::to_value(levels)?;
    }
    ensure!(inspect || rays.is_empty(), "--ray requires --inspect-object");
    if inspect {
        ensure!(
            route.is_none() && physics.is_none() && model.is_none() && replay.is_none() && !full_detail,
            "select only a scene, detail levels and optional ray for object inspection"
        );
        println!("{}", serde_json::to_string_pretty(&object::inspect(&scene, &rays, &detail_views)?)?);
        return Ok(());
    }
    let headless = bench_dir.is_some() || verify_dir.is_some();
    let model = model.unwrap_or(if headless || predict || physics == Some(Physics::Point) {
        Model::Capsule
    } else {
        Model::Human
    });
    let replay = Replay::read(replay_bytes, scene.colliders.iter().map(|s| s.id.as_str()))?;
    let flat_only = replay.routes.iter().all(|r| {
        r.moving_surface().is_none() && scene.supports.binary_search_by(|s| s.id.cmp(&r.expected_support)).is_ok()
    });
    if predict {
        ensure!(
            physics.is_none() && !full_detail && matches!(model, Model::Capsule),
            "prediction uses an upright capsule against fixed meshes"
        );
        println!("{}", serde_json::to_string_pretty(&physics::predict(&scene, &replay, route.as_deref(), &inputs)?)?);
        return Ok(());
    }
    let physics = physics.unwrap_or(if headless && matches!(model, Model::Capsule) && flat_only {
        Physics::Point
    } else {
        Physics::Rigid(Engine::Rapier)
    });
    ensure!(
        physics != Physics::Point || matches!(model, Model::Capsule) && flat_only,
        "the point control requires a capsule marker, fixed geometry and a tagged flat target"
    );
    if let Some(warmup_ticks) = time_physics_from {
        let Physics::Rigid(engine) = physics else { anyhow::bail!("timing requires rigid physics") };
        ensure!(!full_detail, "--full-detail applies to pictures");
        let report = physics::time_steps(engine, &model, &scene, &replay, route.as_deref(), warmup_ticks, &inputs)?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    if impact_response {
        let Physics::Rigid(engine) = physics else { anyhow::bail!("impact response requires rigid physics") };
        ensure!(!full_detail, "--full-detail applies to pictures");
        let report = physics::first_contact::report(engine, &model, &scene, &replay, route.as_deref(), &inputs)?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    if let Some(dir) = bench_dir {
        return match physics {
            Physics::Point => bench::run(&dir, &scene, &replay, &inputs),
            Physics::Rigid(engine) => physics::bench(&dir, engine, &model, &scene, &replay, &inputs),
        };
    }
    if let Some(dir) = verify_dir {
        return match physics {
            Physics::Point => bench::verify(&dir, &scene, &replay, &inputs),
            Physics::Rigid(engine) => physics::verify(&dir, engine, &model, &scene, &replay, &inputs),
        };
    }
    if scene.object.is_some() && impact_wav.is_none() {
        inputs["visual_lod"] = if adaptive { json!("adaptive") } else { serde_json::to_value(levels.visual)? };
    }
    let selected = match route {
        Some(id) => replay.routes.iter().position(|r| r.id == id).with_context(|| format!("unknown route {id}"))?,
        None => 0,
    };
    let playback = Playback { listening, direct_only };
    if let Some(path) = impact_wav {
        let mut report = impact::record(&path, physics, &model, &scene, &replay, &replay.routes[selected], playback)?;
        report["inputs"] = inputs;
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    ensure!(tick.unwrap_or(0) <= replay.routes[selected].ticks, "tick outside this route");
    view::run(
        scene,
        replay,
        view::Options {
            selected,
            shot,
            tick: tick.unwrap_or(0),
            listening: (!mute).then_some(playback),
            timing: render_bench.map(|p| (p, inputs)),
            full_detail,
            physics,
            model,
        },
    )
}

fn point(s: String) -> Result<[f32; 3]> {
    let v: Vec<f32> = s.split(',').map(str::parse).collect::<Result<_, _>>()?;
    v.try_into().map_err(|_| anyhow::anyhow!("a point needs three coordinates"))
}
