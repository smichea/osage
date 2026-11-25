// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::{FootCycle, FootCyclePhase, FootRig, PlantFeet, SoleSkin};
use crate::{Body, Human, Library, Move, Skeleton};
use bevy::{
    animation::{AnimationEntityMut, AnimationTargetId},
    gltf::{Gltf, GltfNode},
    prelude::*,
};
use std::collections::HashMap;

/// Plan the installed human model's walk and jog using the active Bevy clip clock.
#[derive(Component, Default)]
#[require(PlantFeet)]
pub struct PlannedHumanSteps {
    bound: Option<(Entity, Move)>,
}

struct Gait {
    feet: [FootCycle; 2],
    duration: f32,
    node: AnimationNodeIndex,
}

#[derive(Resource, Default)]
struct Gaits(HashMap<(Body, Move), Option<Gait>>);

pub(super) fn install(app: &mut App) {
    app.init_resource::<Gaits>().init_resource::<Soles>().add_systems(Update, prepare.after(crate::build)).add_systems(
        PostUpdate,
        (detach, bind_soles, drive).chain().after(bevy::app::AnimationSystems).before(super::plant),
    );
}

fn prepare(
    people: Query<(&Human, &Skeleton), With<PlannedHumanSteps>>,
    library: Res<Library>,
    gltfs: Res<Assets<Gltf>>,
    nodes: Res<Assets<GltfNode>>,
    clips: Res<Assets<AnimationClip>>,
    mut gaits: ResMut<Gaits>,
) {
    let Some(ready) = &library.ready else { return };
    for (human, skeleton) in &people {
        let gait = human.act;
        let Some(band) = super::human_contact_height(gait) else { continue };
        let key = (skeleton.body, gait);
        if gaits.0.contains_key(&key) {
            continue;
        }
        let Some(body) = gltfs.get(&library.bodies[skeleton.body as usize]) else { continue };
        let Some(clip) =
            gltfs.get(&library.moves).and_then(|g| g.named_animations.get(gait.0)).and_then(|c| clips.get(c))
        else {
            continue;
        };
        let Some(&node) = ready.nodes.get(gait.0) else { continue };
        let travel = Vec2::Y * (clip.duration() * gait.ground_speed_m_s().unwrap());
        let feet = ["ball_l", "ball_r"].map(|foot| {
            let mut path = Vec::new();
            if !find_path(&nodes, body.named_nodes.get("Armature")?, foot, &mut path) {
                return None;
            }
            FootCycle::from_samples(&sample_contact(&path, clip, node)?, travel, band)
        });
        let track = match feet {
            [Some(left), Some(right)] => Some(Gait { feet: [left, right], duration: clip.duration(), node }),
            _ => None,
        };
        gaits.0.insert(key, track);
    }
}

fn find_path<'a>(
    nodes: &'a Assets<GltfNode>,
    handle: &Handle<GltfNode>,
    target: &str,
    path: &mut Vec<&'a GltfNode>,
) -> bool {
    let Some(node) = nodes.get(handle) else { return false };
    path.push(node);
    if node.name == target || node.children.iter().any(|child| find_path(nodes, child, target, path)) {
        return true;
    }
    path.pop();
    false
}

// Use Bevy's own curve evaluators, including interpolation and static channels,
// in an isolated Transform. Root-to-contact products span the model's actual hierarchy.
fn sample_contact(path: &[&GltfNode], clip: &AnimationClip, index: AnimationNodeIndex) -> Option<Vec<Vec3>> {
    const SAMPLES: usize = 180;
    if !clip.duration().is_finite() || clip.duration() <= 0. {
        return None;
    }
    let mut poses = [GlobalTransform::IDENTITY; SAMPLES];
    let mut scratch = World::new();
    let bone = scratch.spawn(Transform::IDENTITY).id();
    let mut query = scratch.query::<AnimationEntityMut>();
    let mut names = Vec::new();
    for node in path {
        names.push(Name::new(node.name.clone()));
        let id = AnimationTargetId::from_names(names.iter());
        let mut curves: Vec<_> = clip
            .curves_for_target(id)
            .into_iter()
            .flatten()
            .map(|curve| (&curve.0, curve.0.create_evaluator()))
            .collect();
        for (i, pose) in poses.iter_mut().enumerate() {
            let t = i as f32 * clip.duration() / SAMPLES as f32;
            *scratch.get_mut::<Transform>(bone)? = node.transform;
            for (curve, evaluator) in &mut curves {
                curve.apply(evaluator.as_mut(), t, 1., index).ok()?;
                evaluator.commit(query.get_mut(&mut scratch, bone).ok()?).ok()?;
            }
            *pose = pose.mul_transform(*scratch.get::<Transform>(bone)?);
        }
    }
    Some(poses.map(|p| p.translation()).to_vec())
}

fn clear(commands: &mut Commands, actor: Entity, rig: &FootRig) {
    commands.entity(actor).remove::<FootCyclePhase>();
    for chain in &rig.chains {
        commands.entity(chain.ankle).try_remove::<FootCycle>();
    }
}

#[derive(Resource, Default)]
struct Soles(HashMap<Body, [SoleSkin; 2]>);

#[derive(Component)]
struct ManagedSole;

type NewWalker = Or<(Added<PlannedHumanSteps>, Added<FootRig>)>;

fn bind_soles(
    mut commands: Commands,
    rigs: Query<(Entity, &Skeleton, &FootRig), (With<PlannedHumanSteps>, NewWalker)>,
) {
    for (actor, skeleton, rig) in &rigs {
        let Ok(chains): Result<[_; 2], _> = rig.chains.as_slice().try_into() else { continue };
        let body = skeleton.body;
        commands.queue(move |world: &mut World| {
            // Human outfits change colours; a built body's clothed shoe geometry is shared.
            world.resource_scope(|world, mut cache: Mut<Soles>| {
                let soles = match cache.0.entry(body) {
                    std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        let [Ok(left), Ok(right)] = chains.map(|chain| SoleSkin::from_actor(world, actor, chain))
                        else {
                            return;
                        };
                        entry.insert([left, right])
                    }
                };
                for (chain, skin) in chains.iter().zip(soles.iter()) {
                    if let Ok(mut ankle) = world.get_entity_mut(chain.ankle)
                        && !ankle.contains::<SoleSkin>()
                    {
                        ankle.insert((skin.clone(), ManagedSole));
                    }
                }
            });
        });
    }
}

fn detach(
    mut commands: Commands,
    mut removed: RemovedComponents<PlannedHumanSteps>,
    rigs: Query<&FootRig, Without<PlannedHumanSteps>>,
    managed: Query<(), With<ManagedSole>>,
) {
    for actor in removed.read() {
        if let Ok(rig) = rigs.get(actor) {
            clear(&mut commands, actor, rig);
            for chain in &rig.chains {
                if managed.contains(chain.ankle) {
                    commands.entity(chain.ankle).try_remove::<(ManagedSole, SoleSkin)>();
                }
            }
        }
    }
}

type Walker<'a> =
    (Entity, &'a Human, &'a Skeleton, &'a FootRig, &'a mut PlannedHumanSteps, Option<&'a mut FootCyclePhase>);

fn drive(mut commands: Commands, mut people: Query<Walker>, gaits: Res<Gaits>, players: Query<&AnimationPlayer>) {
    for (actor, human, skeleton, rig, mut plan, phase) in &mut people {
        let active = gaits.0.get(&(skeleton.body, human.act)).and_then(Option::as_ref).and_then(|gait| {
            let player = players.get(skeleton.armature).ok()?;
            let mut playing = player.playing_animations();
            let (&node, clip) = playing.next()?;
            (node == gait.node && playing.next().is_none()).then_some((gait, clip.seek_time() / gait.duration))
        });
        let Some((gait, time)) = active else {
            if plan.bound.take().is_some() {
                clear(&mut commands, actor, rig);
            }
            continue;
        };
        let binding = (skeleton.armature, human.act);
        if plan.bound != Some(binding) {
            for (chain, cycle) in rig.chains.iter().zip(&gait.feet) {
                commands.entity(chain.ankle).insert(cycle.clone());
            }
            plan.bound = Some(binding);
        }
        if let Some(mut phase) = phase {
            phase.0 = time;
        } else {
            commands.entity(actor).insert(FootCyclePhase(time));
        }
    }
}
