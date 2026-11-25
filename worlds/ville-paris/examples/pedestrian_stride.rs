// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Replay three cadence and stance settings on a flat +Z trajectory. Pass --woman for the second rig.
//! Output world-space forefoot positions from Bevy after animation and transform propagation.

#[path = "../../support/animated_app.rs"]
mod animated_app;
#[path = "pedestrian_stride/sole.rs"]
mod sole;
#[path = "support/strip.rs"]
mod strip;

use std::{
    io::{BufWriter, Write},
    path::Path,
    time::{Duration, Instant},
};

use anyhow::{Result, anyhow, ensure};
use bevy::{prelude::*, time::TimeUpdateStrategy};
use osage_humans::{Body, Human, HumansPlugin, Move, Outfit, Skeleton, Tone};

// Road and raised pavement from the same collision triangles used to measure shoe clearance.
fn pavement(point: Vec3) -> Option<f32> {
    let triangles = [(-2., 40., 0.), (2.15, 4.65, 0.15)].into_iter().flat_map(|(start, end, y)| {
        let a = [-2., y, start];
        let b = [2., y, start];
        let c = [2., y, end];
        let d = [-2., y, end];
        [[a, c, b], [a, d, c]]
    });
    let x = point.x;
    let z = point.z;
    osage_objects::first_hit([x, 1., z], [x, -1., z], triangles).map(|distance| 1. - distance as f32)
}

fn main() -> Result<()> {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    ensure!(osage_humans::installed(&assets), "install the Quaternius models listed in assets/manifest.toml");
    let mut app = animated_app::animated_app(HumansPlugin, &assets)?;
    let mut body = Body::Man;
    let mut curb = false;
    let mut soles = false;
    let mut skinned_support = false;
    let mut planned = false;
    let mut switch = false;
    let mut quick_return = false;
    let mut phase = 0.13_f32;
    let mut args = std::env::args().skip(1);
    while let Some(option) = args.next() {
        match option.as_str() {
            "--planned" => planned = true,
            "--switch" => switch = true,
            "--quick-return" => {
                switch = true;
                quick_return = true;
            }
            "--woman" => body = Body::Woman,
            "--curb" => curb = true,
            "--soles" => soles = true,
            "--mesh-support" => {
                soles = true;
                skinned_support = true;
            }
            "--phase" => {
                phase = args.next().ok_or_else(|| anyhow!("missing stride phase"))?.parse()?;
                ensure!((0.0..1.0).contains(&phase), "stride phase must be in [0, 1)");
            }
            _ => anyhow::bail!(
                "usage: pedestrian_stride [--woman] [--curb] [--soles] [--mesh-support] [--planned] [--switch | --quick-return] [--phase FRACTION]"
            ),
        }
    }
    if curb {
        app.insert_resource(
            osage_humans::FootGround::new(0.2, pavement)
                .with_surface_lift(|face, range| strip::strip_lift(face, [2.15, 4.65, 0.15], range)),
        );
    }
    let mut people = Vec::new();
    for (gait, speed) in [(Move::WALK, 1.4_f32), (Move::JOG, 4.0)] {
        for mode in ["unit", "matched", "planted"].into_iter().chain(planned.then_some("planned")) {
            let id = app
                .world_mut()
                .spawn(Human {
                    body,
                    tone: Tone::Light,
                    hair: None,
                    hair_colour: Color::BLACK,
                    outfit: Outfit { top: Color::WHITE, bottom: Color::BLACK, shoes: Color::BLACK },
                    act: gait,
                    pace: 1.0,
                    phase,
                })
                .id();
            if mode == "planned" {
                app.world_mut().entity_mut(id).insert(osage_humans::PlannedHumanSteps::default());
            } else if mode == "planted" {
                app.world_mut().entity_mut(id).insert(osage_humans::PlantFeet);
            }
            people.push((id, gait, speed, mode));
        }
    }
    let started = Instant::now();
    while people.iter().any(|(id, ..)| app.world().get::<Skeleton>(*id).is_none()) {
        ensure!(started.elapsed() < Duration::from_secs(60), "could not build the pedestrian skeletons in 60 seconds");
        app.update();
        std::thread::sleep(Duration::from_millis(2));
    }
    app.update();
    let step = Duration::from_secs_f64(1.0 / 60.0);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(step));
    let mut shoes = std::collections::HashMap::new();
    if soles {
        let world = app.world();
        for &(id, ..) in &people {
            let bones = &world.get::<Skeleton>(id).unwrap().bones;
            shoes.insert(
                id,
                [sole::Sole::bind(world, id, bones["foot_l"])?, sole::Sole::bind(world, id, bones["foot_r"])?],
            );
        }
    }
    if skinned_support {
        for &(id, _, _, mode) in &people {
            if mode != "planted" {
                continue;
            }
            for side in ["l", "r"] {
                let world = app.world();
                let bones = &world.get::<Skeleton>(id).unwrap().bones;
                let [upper, lower, ankle, contact] =
                    ["thigh", "calf", "foot", "ball"].map(|n| bones[&format!("{n}_{side}")]);
                let chain = osage_humans::FootChain { upper, lower, ankle, contact };
                let skin = osage_humans::SoleSkin::from_actor(world, id, chain).map_err(anyhow::Error::msg)?;
                app.world_mut().entity_mut(ankle).insert(skin);
            }
        }
    }
    let gait_nodes = [Move::WALK, Move::JOG].map(|gait| {
        let actor = people.iter().find(|(_, act, _, _)| *act == gait).unwrap().0;
        let armature = app.world().get::<Skeleton>(actor).unwrap().armature;
        *app.world().get::<AnimationPlayer>(armature).unwrap().playing_animations().next().unwrap().0
    });
    let alternate_ticks = if quick_return { 3 } else { 60 };
    let mut out = BufWriter::new(std::io::stdout().lock());
    write!(out, "frame,gait,mode,root_z,phase_seconds,left_y,left_z,right_y,right_z,left_x,right_x")?;
    if curb {
        write!(out, ",root_y,left_ground,right_ground,pelvis_y")?;
    }
    if soles {
        write!(out, ",left_sole_clearance,right_sole_clearance")?;
    }
    if planned {
        write!(out, ",active_move,playing_clips,planned_phase")?;
    }
    if quick_return {
        write!(out, ",walk_time_s,walk_weight,jog_time_s,jog_weight,pace")?;
    }
    writeln!(out)?;
    for frame in 1..=480 {
        for &(id, gait, speed, mode) in &people {
            // Six seconds moving, one holding the current pose, one resuming it.
            let moving = !(361..=420).contains(&frame);
            let dt = step.as_secs_f32();
            let mut entity = app.world_mut().entity_mut(id);
            let mut transform = entity.get_mut::<Transform>().ok_or_else(|| anyhow!("actor transform"))?;
            let before = transform.translation.z;
            let other = if gait == Move::WALK { (Move::JOG, 4.0) } else { (Move::WALK, 1.4) };
            let active_move = if switch && !moving {
                Move::IDLE
            } else if switch && (121..=120 + 2 * alternate_ticks).contains(&frame) {
                other.0
            } else {
                gait
            };
            if moving {
                let travel_frames = if frame > 420 { frame - 60 } else { frame };
                let ticks: i32 = travel_frames / 2;
                let alternate = if switch { (ticks - 60).clamp(0, alternate_ticks) } else { 0 };
                let packet_z = (speed * (ticks - alternate) as f32 + other.1 * alternate as f32) / 30.0;
                transform.translation.z += (packet_z - before) * (10.0 * dt);
            }
            if curb {
                let ground = pavement(transform.translation).ok_or_else(|| anyhow!("root outside the road"))?;
                transform.translation.y += (ground - transform.translation.y) * (10.0 * dt);
            }
            let displayed_speed = (transform.translation.z - before) / dt;
            let pace = if active_move == Move::IDLE {
                1.0
            } else if !moving {
                0.0
            } else if mode != "unit" {
                displayed_speed / active_move.ground_speed_m_s().unwrap()
            } else {
                1.0
            };
            let mut human = entity.get_mut::<Human>().unwrap();
            human.act = active_move;
            human.pace = pace;
        }
        app.update();
        let world = app.world();
        for &(id, gait, _, mode) in &people {
            let skeleton = world.get::<Skeleton>(id).unwrap();
            let toe = |name| -> Result<Vec3> {
                let bone = skeleton.bones.get(name).ok_or_else(|| anyhow!("missing forefoot bone"))?;
                Ok(world
                    .get::<GlobalTransform>(*bone)
                    .ok_or_else(|| anyhow!("missing forefoot transform"))?
                    .translation())
            };
            let left = toe("ball_l")?;
            let right = toe("ball_r")?;
            let player =
                world.get::<AnimationPlayer>(skeleton.armature).ok_or_else(|| anyhow!("missing animation player"))?;
            let (_, clip) = player.playing_animations().next().ok_or_else(|| anyhow!("no active gait"))?;
            let z = world.get::<Transform>(id).unwrap().translation.z;
            write!(
                out,
                "{frame},{},{mode},{z},{},{},{},{},{},{},{}",
                gait.0,
                clip.seek_time(),
                left.y,
                left.z,
                right.y,
                right.z,
                left.x,
                right.x
            )?;
            if curb {
                let y = world.get::<Transform>(id).unwrap().translation.y;
                write!(out, ",{y},{},{},{}", pavement(left).unwrap(), pavement(right).unwrap(), toe("pelvis")?.y)?;
            }
            if let Some(shoes) = shoes.get(&id) {
                for shoe in shoes {
                    let clearance = shoe.clearance(world, |p| if curb { pavement(p) } else { Some(0.) })?;
                    write!(out, ",{clearance}")?;
                }
            }
            if planned {
                let active = world.get::<Human>(id).unwrap().act;
                let phase = world.get::<osage_humans::FootCyclePhase>(id);
                let count = player.playing_animations().count();
                write!(out, ",{},{},", active.0, count)?;
                if let Some(phase) = phase {
                    write!(out, "{}", phase.0)?;
                }
                ensure!(
                    mode != "planned" || phase.is_some() == (count == 1 && active.ground_speed_m_s().is_some()),
                    "plan and clip clocks disagree at frame {frame}"
                );
            }
            if quick_return {
                for node in gait_nodes {
                    if let Some(clip) = player.animation(node) {
                        write!(out, ",{},{}", clip.seek_time(), clip.weight())?;
                    } else {
                        write!(out, ",,0")?;
                    }
                }
                write!(out, ",{}", world.get::<Human>(id).unwrap().pace)?;
            }
            writeln!(out)?;
        }
    }
    // Removing the opt-in restores the authored pose on the next frame.
    for &(id, _, _, mode) in &people {
        if mode == "planned" {
            app.world_mut().entity_mut(id).remove::<(osage_humans::PlannedHumanSteps, osage_humans::PlantFeet)>();
        } else if mode == "planted" {
            app.world_mut().entity_mut(id).remove::<osage_humans::PlantFeet>();
        }
    }
    app.update();
    for gait in [Move::WALK, Move::JOG] {
        let toes = |mode: &str| {
            let &(id, ..) = people.iter().find(|(_, movement, _, m)| *movement == gait && *m == mode).unwrap();
            let world = app.world();
            let skeleton = world.get::<Skeleton>(id).unwrap();
            ["ball_l", "ball_r"].map(|n| world.get::<GlobalTransform>(skeleton.bones[n]).unwrap().translation())
        };
        for mode in ["planted"].into_iter().chain(planned.then_some("planned")) {
            for (before, after) in toes("matched").into_iter().zip(toes(mode)) {
                ensure!(before.distance(after) < 1e-5, "forefoot did not return to the authored pose");
            }
        }
    }
    out.flush()?;
    Ok(())
}
