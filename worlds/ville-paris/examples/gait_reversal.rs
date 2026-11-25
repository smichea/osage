// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Replay walk/run requests from stdin against two real Bevy players on the Khronos Fox.
#[path = "../../support/animated_app.rs"]
mod animated_app;

use anyhow::{Result, anyhow, ensure};
use bevy::{
    animation::{AnimatedBy, AnimationTargetId},
    gltf::{Gltf, GltfNode},
    prelude::*,
    time::TimeUpdateStrategy,
};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Deserialize)]
struct Trial {
    label: String,
    dt: f32,
    fade_s: f32,
    phase: f32,
    run: Vec<bool>,
}

struct Clips {
    nodes: [AnimationNodeIndex; 2],
    lengths: [f32; 2],
    graph: Handle<AnimationGraph>,
    root: Handle<GltfNode>,
}

struct Fox {
    player: Entity,
    paws: Vec<Entity>,
}

impl Fox {
    fn spawn(world: &mut World, clips: &Clips) -> Result<Self> {
        let player = world
            .spawn((
                Transform::from_scale(Vec3::splat(0.01)),
                AnimationPlayer::default(),
                AnimationTransitions::new(),
                AnimationGraphHandle(clips.graph.clone()),
            ))
            .id();
        let mut paws = Vec::new();
        let mut stack = vec![(player, clips.root.clone(), Vec::new())];
        while let Some((parent, handle, mut path)) = stack.pop() {
            let n = world.resource::<Assets<GltfNode>>().get(&handle).ok_or_else(|| anyhow!("missing node"))?;
            let children = n.children.clone();
            let transform = n.transform;
            let paw = matches!(
                n.name.as_str(),
                "b_LeftHand_011" | "b_RightHand_08" | "b_LeftFoot02_018" | "b_RightFoot02_022"
            );
            path.push(Name::new(n.name.clone()));
            let id = world
                .spawn((transform, ChildOf(parent), AnimationTargetId::from_names(path.iter()), AnimatedBy(player)))
                .id();
            if paw {
                paws.push(id);
            }
            stack.extend(children.into_iter().map(|c| (id, c, path.clone())));
        }
        ensure!(paws.len() == 4, "missing paw joints");
        Ok(Self { player, paws })
    }

    fn play(&self, world: &mut World, clips: &Clips, gait: usize, fade: Duration, phase: f32, preserve: bool) {
        let mut q = world.query::<(&mut AnimationPlayer, &mut AnimationTransitions)>();
        let (mut player, mut transitions) = q.get_mut(world, self.player).unwrap();
        let node = clips.nodes[gait];
        let start = phase * clips.lengths[gait];
        if preserve {
            osage_humans::crossfade_loop(&mut player, &mut transitions, node, fade, start);
        } else {
            transitions.play(&mut player, node, fade).repeat().seek_to(start);
        }
    }

    fn positions(&self, world: &World) -> Vec<Vec3> {
        self.paws.iter().map(|&e| world.get::<GlobalTransform>(e).unwrap().translation()).collect()
    }
}

#[derive(Serialize)]
struct Tick {
    frame: usize,
    run_weight: f32,
    clock_before_s: Option<f32>,
    clock_after_command_s: Option<f32>,
    zero_time_jump_m: [f32; 2],
}

fn tick(app: &mut App, dt: Duration) {
    app.insert_resource(TimeUpdateStrategy::ManualDuration(dt));
    app.update();
}

fn replay(app: &mut App, clips: &Clips, trial: &Trial) -> Result<Vec<Tick>> {
    ensure!(
        trial.dt.is_finite()
            && trial.dt > 0.
            && trial.dt <= 0.25
            && trial.fade_s.is_finite()
            && trial.fade_s > 0.
            && (0.0..1.0).contains(&trial.phase),
        "invalid timing"
    );
    let dt = Duration::from_secs_f32(trial.dt);
    let fade = Duration::from_secs_f32(trial.fade_s);
    let actors = [Fox::spawn(app.world_mut(), clips)?, Fox::spawn(app.world_mut(), clips)?];
    for actor in &actors {
        actor.play(app.world_mut(), clips, 0, Duration::ZERO, trial.phase, false);
    }
    // Let the source clock move away from its configured entry time before the first command.
    tick(app, Duration::ZERO);
    for _ in 0..10 {
        tick(app, dt);
    }
    let mut running = false;
    let mut result = Vec::new();
    for (i, &run) in trial.run.iter().enumerate() {
        let node = clips.nodes[run as usize];
        let player =
            |world: &World, e: Entity, index| world.get::<AnimationPlayer>(e).unwrap().animation(index).copied();
        let mut before = None;
        let mut after = None;
        let mut jumps = [0.; 2];
        if run != running {
            let from = actors.each_ref().map(|a| a.positions(app.world()));
            before = player(app.world(), actors[1].player, node).map(|a| a.seek_time());
            for (i, a) in actors.iter().enumerate() {
                a.play(app.world_mut(), clips, run as usize, fade, trial.phase, i == 1);
            }
            // Hold time still to isolate any instantaneous discontinuity in the evaluated pose.
            tick(app, Duration::ZERO);
            after = player(app.world(), actors[1].player, node).map(|a| a.seek_time());
            for (i, a) in actors.iter().enumerate() {
                jumps[i] = from[i].iter().zip(a.positions(app.world())).map(|(p, q)| p.distance(q)).fold(0., f32::max);
            }
            running = run;
        }
        tick(app, dt);
        let weights = clips.nodes.map(|node| player(app.world(), actors[1].player, node).map_or(0., |a| a.weight()));
        let original_weights =
            clips.nodes.map(|node| player(app.world(), actors[0].player, node).map_or(0., |a| a.weight()));
        ensure!(weights == original_weights, "clock policy changed Bevy's blend weights");
        ensure!((weights[0] + weights[1] - 1.).abs() < 1e-5);
        result.push(Tick {
            frame: i + 1,
            run_weight: weights[1],
            clock_before_s: before,
            clock_after_command_s: after,
            zero_time_jump_m: jumps,
        });
    }
    for actor in actors {
        app.world_mut().despawn(actor.player);
    }
    Ok(result)
}

fn main() -> Result<()> {
    let trials: Vec<Trial> = serde_json::from_reader(std::io::stdin().lock())?;
    let mut app =
        animated_app::animated_app((), &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"))?;
    let handle: Handle<Gltf> = app.world().resource::<AssetServer>().load("khronos/Fox.glb");
    let deadline = Instant::now() + Duration::from_secs(60);
    while !app.world().resource::<AssetServer>().is_loaded_with_dependencies(&handle) {
        ensure!(Instant::now() < deadline, "fox asset did not load");
        tick(&mut app, Duration::ZERO);
        std::thread::sleep(Duration::from_millis(2));
    }
    let gltf = app.world().resource::<Assets<Gltf>>().get(&handle).unwrap();
    let root = gltf.named_nodes["root"].clone();
    let handles = ["Walk", "Run"].map(|name| gltf.named_animations[name].clone());
    let lengths =
        handles.each_ref().map(|h| app.world().resource::<Assets<AnimationClip>>().get(h).unwrap().duration());
    let (graph, indices) = AnimationGraph::from_clips(handles);
    let graph = app.world_mut().resource_mut::<Assets<AnimationGraph>>().add(graph);
    let clips = Clips { nodes: [indices[0], indices[1]], lengths, graph, root };
    let mut output = std::collections::BTreeMap::new();
    for trial in &trials {
        ensure!(output.insert(&trial.label, replay(&mut app, &clips, trial)?).is_none(), "duplicate trial name");
    }
    serde_json::to_writer(std::io::stdout().lock(), &output)?;
    Ok(())
}
