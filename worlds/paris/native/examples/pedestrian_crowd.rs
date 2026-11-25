// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Replay 24 animated walkers before and after an about-face; no renderer or network required.
#[path = "../../../support/animated_app.rs"]
mod animated_app;
#[path = "../src/foot_detail.rs"]
mod foot_detail;

use anyhow::{Result, ensure};
use bevy::{prelude::*, time::TimeUpdateStrategy};
use osage_humans::{Body, FootGround, Human, HumansPlugin, Move, Outfit, PlannedHumanSteps, Skeleton, Tone};
use osage_objects::DetailView;
use std::{
    io::{BufWriter, Write},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

// Sum the CPU time of the Bevy worker threads. The wall timer also includes
// time spent waiting for other processes on a shared machine.
#[cfg(target_os = "linux")]
fn cpu_us() -> Option<u64> {
    let mut t = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: clock_gettime writes one valid timespec to the supplied address.
    let result = unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut t) };
    (result == 0).then(|| t.tv_sec as u64 * 1_000_000 + t.tv_nsec as u64 / 1000)
}

#[cfg(not(target_os = "linux"))]
fn cpu_us() -> Option<u64> {
    None
}

fn pavement(z: f32) -> f32 {
    if (2.15..4.65).contains(&z.rem_euclid(8.)) { 0.15 } else { 0. }
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let mode = args.first().map(String::as_str).unwrap_or("view");
    ensure!(
        matches!(mode, "view" | "full" | "clip") && (args.len() <= 1 || args[1..] == ["--count"]),
        "expected full|view|clip [--count]"
    );
    let count = args.len() == 2;
    let probes = Arc::new(AtomicUsize::new(0));
    let tally = probes.clone();
    let mut app = animated_app::animated_app(
        HumansPlugin,
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../assets"),
    )?;
    app.insert_resource(FootGround::new(0.4, move |p| {
        if count {
            tally.fetch_add(1, Ordering::Relaxed);
        }
        Some(pavement(p.z))
    }));
    let mut actors = Vec::new();
    for z in [10., 180., -24., -160.] {
        for col in 0..6 {
            let gait = if col % 3 == 0 { Move::JOG } else { Move::WALK };
            let feet = Vec3::new((col as f32 - 2.5) * 1.2, pavement(z), z);
            let id = app
                .world_mut()
                .spawn((
                    Human {
                        body: if col % 2 == 0 { Body::Man } else { Body::Woman },
                        tone: Tone::Light,
                        hair: None,
                        hair_colour: Color::BLACK,
                        outfit: Outfit { top: Color::WHITE, bottom: Color::BLACK, shoes: Color::BLACK },
                        act: gait,
                        pace: gait.ground_speed_m_s().map_or(1., |v| if gait == Move::JOG { 4. / v } else { 1.4 / v }),
                        phase: col as f32 / 6.,
                    },
                    Transform::from_translation(feet),
                ))
                .id();
            actors.push((id, z, if gait == Move::JOG { 4. } else { 1.4 }));
        }
    }
    let start = Instant::now();
    while actors.iter().any(|(id, ..)| app.world().get::<Skeleton>(*id).is_none()) {
        ensure!(
            start.elapsed() < Duration::from_secs(90),
            "models missing or slow to load (install the manifest's Quaternius assets)"
        );
        app.update();
        std::thread::sleep(Duration::from_millis(2));
    }
    app.update();
    let step = Duration::from_secs_f64(1. / 60.);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(step));
    let mut out = BufWriter::new(std::io::stdout().lock());
    writeln!(
        out,
        "frame,actor,planned,queries,frame_us,cpu_us,clock,root_y,root_z,left_x,left_y,left_z,right_x,right_y,right_z"
    )?;
    for frame in 0..240 {
        let dir = if frame < 120 { Vec3::Z } else { Vec3::NEG_Z };
        let view = DetailView::new(Vec3::new(0., 1.65, 0.).as_dvec3(), dir.as_dvec3(), 60_f64.to_radians(), 1080, 0.05);
        probes.store(0, Ordering::Relaxed);
        let cpu_start = cpu_us();
        let start = Instant::now();
        for &(actor, initial_z, speed) in &actors {
            let world = app.world_mut();
            let mut root = world.get_mut::<Transform>(actor).unwrap();
            root.translation.z = initial_z + speed * (frame + 1) as f32 / 60.;
            root.translation.y += (pavement(root.translation.z) - root.translation.y) / 6.;
            let feet = root.translation;
            let planned = world.get::<PlannedHumanSteps>(actor).is_some();
            match mode {
                "view" => foot_detail::update(&mut world.commands(), actor, view, feet, planned),
                "full" if !planned => {
                    world.commands().entity(actor).insert(PlannedHumanSteps::default());
                }
                _ => {}
            }
        }
        app.world_mut().flush();
        app.update();
        let us = start.elapsed().as_micros();
        let cpu = cpu_start.zip(cpu_us()).map_or_else(String::new, |(a, b)| (b - a).to_string());
        let calls = probes.load(Ordering::Relaxed);
        for (i, &(id, ..)) in actors.iter().enumerate() {
            let world = app.world();
            let body = world.get::<Skeleton>(id).unwrap();
            let foot = |name: &str| world.get::<GlobalTransform>(body.bones[name]).unwrap().translation();
            let player = world.get::<AnimationPlayer>(body.armature).unwrap();
            let (_, clip) = player.playing_animations().next().unwrap();
            let root = world.get::<Transform>(id).unwrap().translation;
            let (l, r) = (foot("ball_l"), foot("ball_r"));
            writeln!(
                out,
                "{frame},{i},{},{calls},{us},{cpu},{},{},{},{},{},{},{},{},{}",
                world.get::<PlannedHumanSteps>(id).is_some(),
                clip.seek_time(),
                root.y,
                root.z,
                l.x,
                l.y,
                l.z,
                r.x,
                r.y,
                r.z
            )?;
        }
    }
    Ok(())
}
