// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Replay sampled local joint transforms in metres through the normal foot-plant system.
//! Node slots are parent first. Each foot's extra contact node can represent an authored pad.
#[path = "support/strip.rs"]
mod strip;
use anyhow::{Result, ensure};
use bevy::{prelude::*, time::TimeUpdateStrategy};
use osage_humans::{
    ContactHeight, FootChain, FootCycle, FootCyclePhase, FootGround, FootPlantPlugin, FootRig, PlantFeet, SolePoints,
    SoleSkin,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Deserialize)]
struct Trial {
    parents: Vec<Option<usize>>,
    armature: usize,
    chains: Vec<[usize; 4]>,
    #[serde(default)]
    soles: Vec<Vec<Vec3>>,
    #[serde(default)]
    skin_soles: Vec<Vec<[Vec4; 4]>>,
    #[serde(default)]
    faces: Vec<Vec<[usize; 3]>>,
    #[serde(default)]
    cycle: Option<Cycle>,
    band: f32,
    dt: f32,
    ground: Option<[f32; 3]>, // start Z, end Z, height
    frames: Vec<Frame>,
}

#[derive(Deserialize)]
struct Cycle {
    samples: Vec<Vec<Vec3>>,
    travel: Vec2,
    start: f64,
    duration: f64,
}

#[derive(Deserialize)]
struct Frame {
    root: Vec3,
    poses: Vec<(Vec3, Quat)>,
}

#[derive(Resource)]
struct Poses(Vec<Transform>);
#[derive(Component)]
struct Slot(usize);

#[derive(Serialize)]
struct Correction {
    armature: Vec3,
    limbs: Vec<[Quat; 3]>,
}

fn main() -> Result<()> {
    let input: Trial = serde_json::from_reader(std::io::stdin().lock())?;
    ensure!(input.armature < input.parents.len() && input.parents[input.armature].is_none(), "invalid armature");
    ensure!(input.dt.is_finite() && input.dt > 0., "invalid frame interval");
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin, FootPlantPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(input.dt)))
        .add_systems(
            PostUpdate,
            (|poses: Res<Poses>, mut joints: Query<(&Slot, &mut Transform)>| {
                for (slot, mut joint) in &mut joints {
                    *joint = poses.0[slot.0];
                }
            })
            .in_set(bevy::app::AnimationSystems),
        );
    let ground = input.ground;
    let sampler = FootGround::new(0.25, move |p| {
        Some(ground.filter(|&[start, end, _]| (start..end).contains(&p.z)).map_or(0., |g| g[2]))
    });
    let sampler = if let Some(strip) = ground {
        sampler.with_surface_lift(move |sole, range| strip::strip_lift(sole, strip, range))
    } else {
        sampler
    };
    app.insert_resource(sampler);
    let world = app.world_mut();
    let actor = world.spawn(Transform::IDENTITY).id();
    let mut nodes = Vec::new();
    for (i, parent) in input.parents.iter().enumerate() {
        ensure!(parent.is_none_or(|n| n < i), "ancestry");
        let parent = parent.map_or(actor, |n| nodes[n]);
        nodes.push(world.spawn((Slot(i), Transform::IDENTITY, ChildOf(parent))).id());
    }
    ensure!(input.chains.iter().flatten().all(|&i| i < nodes.len()), "limb outside skeleton");
    let chains: Vec<_> = input
        .chains
        .iter()
        .map(|c| {
            let [upper, lower, ankle, contact] = c.map(|i| nodes[i]);
            FootChain { upper, lower, ankle, contact }
        })
        .collect();
    if !input.soles.is_empty() {
        ensure!(input.soles.len() == chains.len(), "sole count");
        for (chain, points) in chains.iter().zip(input.soles) {
            let sole = SolePoints::new(points).ok_or_else(|| anyhow::anyhow!("invalid sole points"))?;
            world.entity_mut(chain.ankle).insert(sole);
        }
    }
    if !input.skin_soles.is_empty() {
        ensure!(input.skin_soles.len() == chains.len(), "skin sole count");
        ensure!(input.faces.is_empty() || input.faces.len() == chains.len(), "face count");
        let mut faces = input.faces.into_iter();
        for (chain, points) in chains.iter().zip(input.skin_soles) {
            let skin = SoleSkin::from_triangles(points, faces.next().unwrap_or_default())
                .ok_or_else(|| anyhow::anyhow!("invalid skin weights"))?;
            world.entity_mut(chain.ankle).insert(skin);
        }
    }
    if let Some(cycle) = &input.cycle {
        ensure!(
            cycle.samples.len() == chains.len()
                && cycle.duration.is_finite()
                && cycle.duration > 0.
                && cycle.start.is_finite(),
            "invalid foot cycle"
        );
        for (chain, samples) in chains.iter().zip(&cycle.samples) {
            let path = FootCycle::from_samples(samples, cycle.travel, input.band)
                .ok_or_else(|| anyhow::anyhow!("invalid stance cycle"))?;
            world.entity_mut(chain.ankle).insert(path);
        }
    }
    let armature = nodes[input.armature];
    let rig = FootRig::bind(world, actor, armature, chains.clone()).map_err(anyhow::Error::msg)?;
    world.entity_mut(actor).insert((rig, PlantFeet, ContactHeight(Some(input.band))));
    let mut results = Vec::new();
    for (i, frame) in input.frames.into_iter().enumerate() {
        if let Some(cycle) = &input.cycle {
            let phase = cycle.start + i as f64 * input.dt as f64 / cycle.duration;
            app.world_mut().entity_mut(actor).insert(FootCyclePhase(phase.fract() as f32));
        }
        ensure!(frame.poses.len() == nodes.len(), "pose size");
        app.insert_resource(Poses(
            frame
                .poses
                .into_iter()
                .map(|(translation, rotation)| Transform { translation, rotation, ..default() })
                .collect(),
        ));
        app.world_mut().get_mut::<Transform>(actor).unwrap().translation = frame.root;
        app.update();
        let pose = |e| app.world().get::<Transform>(e).unwrap();
        let limbs = chains.iter().map(|c| [c.upper, c.lower, c.ankle].map(|id| pose(id).rotation)).collect();
        results.push(Correction { armature: pose(armature).translation, limbs });
    }
    serde_json::to_writer(std::io::stdout().lock(), &results)?;
    Ok(())
}
