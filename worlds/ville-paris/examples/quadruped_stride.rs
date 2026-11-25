// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Replay a model-defined four-foot rig over a raised strip. Output paired paw positions after Bevy propagation.
use anyhow::{Result, anyhow, ensure};
use bevy::{app::AnimationSystems, prelude::*, time::TimeUpdateStrategy};
use osage_humans::{ContactHeight, FootChain, FootGround, FootPlantPlugin, FootRig, PlantFeet, SolePoints};
use osage_objects::ObjectProfile;
use std::{
    f32::consts::TAU,
    io::{BufWriter, Write},
    time::Duration,
};

const MODEL: &[u8] = include_bytes!("fixtures/four-foot.gltf");

fn floor(p: Vec3) -> f32 {
    if (1.0..2.8).contains(&p.z) { 0.12 } else { 0. }
}

#[derive(Resource, Default)]
struct ReplayTime(f32);

#[derive(Component)]
struct Trot(Vec<FootChain>);

fn sample_gait(clock: Res<ReplayTime>, actors: Query<&Trot>, mut poses: Query<&mut Transform>) {
    for actor in &actors {
        for (index, chain) in actor.0.iter().enumerate() {
            // The model lists the two diagonal pairs. Lift the swinging foot by folding
            // the two links; the fore and hind legs bend in opposite directions.
            let phase = (clock.0 + if index < 2 { 0. } else { 0.5 }).fract();
            let swing = (phase * TAU).sin().max(0.);
            let bend = if index == 0 || index == 3 { 1. } else { -1. };
            let stride = 0.22 * (TAU * phase).cos();
            let upper = stride - bend * 0.38 * swing;
            let lower = bend * 0.76 * swing;
            let [mut h, mut k, mut a] = poses.get_many_mut([chain.upper, chain.lower, chain.ankle]).unwrap();
            h.rotation = Quat::from_rotation_x(upper);
            k.rotation = Quat::from_rotation_x(lower);
            a.rotation = Quat::from_rotation_x(-upper - lower);
        }
    }
}

fn spawn_actor(world: &mut World, profile: &ObjectProfile, corrected: bool) -> Result<Entity> {
    let actor = world.spawn(Transform::IDENTITY).id();
    let mut entities = Vec::new();
    for node in &profile.nodes {
        let parent = node.parent.map_or(actor, |i| entities[i]);
        let id = world.spawn((Transform::from_matrix(node.parent_from_local), ChildOf(parent))).id();
        entities.push(id);
    }
    let rig = profile.foot_rig.as_ref().ok_or_else(|| anyhow!("missing foot rig in fixture"))?;
    let chains: Vec<_> = rig
        .chains
        .iter()
        .map(|&ids| {
            let [upper, lower, ankle, contact] = ids.map(|i| entities[i]);
            FootChain { upper, lower, ankle, contact }
        })
        .collect();
    ensure!(chains.len() == 4, "expected four declared feet");
    if corrected {
        for (&ankle, points) in &rig.sole_points {
            let sole = SolePoints::new(points.iter().copied().map(Vec3::from_array).collect())
                .ok_or_else(|| anyhow!("invalid sole"))?;
            world.entity_mut(entities[ankle]).insert(sole);
        }
        let bound = FootRig::bind(world, actor, entities[rig.armature], chains.clone()).map_err(anyhow::Error::msg)?;
        world.entity_mut(actor).insert((bound, ContactHeight(Some(0.032)), PlantFeet));
    }
    world.entity_mut(actor).insert(Trot(chains));
    Ok(actor)
}

fn main() -> Result<()> {
    let gltf = gltf::Gltf::from_slice(MODEL)?;
    let profile = ObjectProfile::read(&gltf.document)?.ok_or_else(|| anyhow!("missing object profile"))?;
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin, FootPlantPlugin))
        .init_resource::<ReplayTime>()
        .insert_resource(FootGround::new(0.2, |p| Some(floor(p))))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1. / 60.)))
        .add_systems(PostUpdate, sample_gait.in_set(AnimationSystems));
    let control = spawn_actor(app.world_mut(), &profile, false)?;
    let planted = spawn_actor(app.world_mut(), &profile, true)?;
    let mut out = BufWriter::new(std::io::stdout().lock());
    writeln!(out, "frame,paw,root_y,root_z,raw_x,raw_y,raw_z,raw_ground,planted_x,planted_y,planted_z,planted_ground")?;
    for frame in 0..=360 {
        let secs = frame as f32 / 60.;
        app.world_mut().resource_mut::<ReplayTime>().0 = secs;
        for actor in [control, planted] {
            let mut pose = app.world_mut().get_mut::<Transform>(actor).unwrap();
            pose.translation.z = secs * 0.7;
            pose.translation.y += (floor(pose.translation) - pose.translation.y) / 6.;
        }
        app.update();
        let world = app.world();
        let root = world.get::<Transform>(control).unwrap().translation;
        let paws = |id| &world.get::<Trot>(id).unwrap().0;
        for (i, (a, b)) in paws(control).iter().zip(paws(planted)).enumerate() {
            let pos = |chain: &FootChain| world.get::<GlobalTransform>(chain.contact).unwrap().translation();
            let (raw, actual) = (pos(a), pos(b));
            writeln!(
                out,
                "{frame},{i},{},{},{},{},{},{},{},{},{},{}",
                root.y,
                root.z,
                raw.x,
                raw.y,
                raw.z,
                floor(raw),
                actual.x,
                actual.y,
                actual.z,
                floor(actual)
            )?;
        }
    }
    out.flush()?;
    Ok(())
}
