// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;
use crate::{ParisWorld, TICK_HZ, Walkers, walk::RADIUS};
use bevy::time::TimeUpdateStrategy;
use osage_world_module_api::{IntentReceived, Joined, Left, Schema};
use prost::Message;
use std::{sync::Arc, time::Duration};

fn flat() -> Walk {
    Walk::new(0.25, 80, 80, Vec2::splat(-10.), vec![0.; 80 * 80], Vec3::ZERO)
}

fn app(grid: Walk) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1. / f64::from(TICK_HZ))));
    let module = ParisWorld { walk: Arc::new(grid), walkers: Walkers::default(), light_port: 0, horizon: 2. };
    app.insert_resource(Schema::new(&module))
        .init_resource::<EntityIds>()
        .add_message::<IntentReceived>()
        .add_message::<Joined>()
        .add_message::<Left>()
        .add_plugins(module);
    app.update();
    app
}

fn pair(app: &mut App) -> [Entity; 2] {
    let world = app.world_mut();
    let walker = world.resource::<Ids>().walker;
    let mut q = world.query_filtered::<(Entity, &Replicated, &Discrete), With<Patrol>>();
    let mut ids: Vec<_> = q
        .iter(world)
        .map(|(e, id, d)| {
            assert!(proto::Walker::decode(d.0[&walker].as_slice()).unwrap().passer_by);
            (id.0, e)
        })
        .collect();
    ids.sort_by_key(|p| p.0);
    ids.into_iter().map(|p| p.1).collect::<Vec<_>>().try_into().unwrap()
}

fn track(app: &mut App, actors: [Entity; 2], label: &str) -> (f32, [usize; 2]) {
    let grid = app.world().resource::<Grid>().0.clone();
    let state = |world: &World| {
        actors.map(|id| (world.get::<Transform>(id).unwrap().translation, world.get::<Patrol>(id).unwrap().goal))
    };
    let mut previous = state(app.world());
    let mut gap = f32::INFINITY;
    let mut reversals = [0, 0];
    let mut trace = (label == "Bistro")
        .then(|| std::env::var_os("OSAGE_ENCOUNTER_CSV"))
        .flatten()
        .map(|path| std::io::BufWriter::new(std::fs::File::create(path).unwrap()));
    use std::io::Write;
    if let Some(file) = &mut trace {
        writeln!(file, "tick,ax,az,avx,avz,bx,bz,bvx,bvz,osage").unwrap();
    }
    for tick in 0..600 {
        if let Some(file) = &mut trace {
            let [a, b] = actors.map(|id| Motion {
                at: app.world().get::<Transform>(id).unwrap().translation,
                velocity: app.world().get::<Patrol>(id).unwrap().velocity,
                undecided: false,
            });
            let (_, forecast) = clearance(a.at, a.velocity, std::iter::once(&b), 1. / TICK_HZ as f32);
            writeln!(
                file,
                "{tick},{},{},{},{},{},{},{},{},{}",
                a.at.x,
                a.at.z,
                a.velocity.x,
                a.velocity.y,
                b.at.x,
                b.at.z,
                b.velocity.x,
                b.velocity.y,
                u8::from(forecast < DIAMETER_SQ)
            )
            .unwrap();
        }
        app.update();
        let now = state(app.world());
        let before = previous[0].0.xz() - previous[1].0.xz();
        let after = now[0].0.xz() - now[1].0.xz();
        // Independent double-precision segment-to-origin oracle between ticks.
        let d = after.as_dvec2() - before.as_dvec2();
        let t =
            if d.length_squared() > 0. { (-before.as_dvec2().dot(d) / d.length_squared()).clamp(0., 1.) } else { 0. };
        gap = gap.min((before.as_dvec2() + d * t).length() as f32);
        for i in 0..2 {
            assert!(grid.holds(now[i].0.xz()));
            assert!(now[i].0.xz().distance(previous[i].0.xz()) <= WALK / TICK_HZ as f32 + 1e-5);
            reversals[i] += usize::from(now[i].1 != previous[i].1);
        }
        previous = now;
    }
    println!("route={label} min_centres_m={gap:.7} reversals={reversals:?}");
    (gap, reversals)
}

#[test]
fn server_walkers_pass_and_reach_both_ends_on_two_crossing_routes() {
    for (label, b) in [("head-on", [Vec3::X * 2.1, Vec3::NEG_X * 2.1]), ("cross", [Vec3::NEG_Z * 2.1, Vec3::Z * 2.1])] {
        let mut app = app(flat());
        let actors = pair(&mut app);
        for (id, ends) in actors.into_iter().zip([[Vec3::NEG_X * 2.1, Vec3::X * 2.1], b]) {
            app.world_mut()
                .entity_mut(id)
                .insert((Transform::from_translation(ends[0]), Patrol { ends, goal: 1, velocity: Vec2::ZERO }));
        }
        let (gap, reversals) = track(&mut app, actors, label);
        assert!(gap >= 2. * RADIUS - 1e-5 && reversals.iter().all(|&n| n >= 4), "{label} {gap} {reversals:?}");
    }
}

#[test]
fn prediction_sees_both_velocities_and_ignores_parallel_and_late_encounters() {
    let x = Vec2::X;
    let y = Vec2::Y;
    for (label, b, v, first) in [
        ("head-on", 2.1 * x, -x, Some(11)),
        ("cross", -2.1 * y, y, Some(9)),
        ("head-on-offset", 2.1 * x + 0.6 * y, -x, None),
        ("cross-half-second-late", -2.8 * y, y, None),
        ("cross-late", -4.2 * y, y, None),
        ("parallel", -2.1 * x + 0.6 * y, x, None),
    ] {
        let mut detected = None;
        let mut signal = String::new();
        let grid = flat();
        let first_actor = -2.1 * x;
        for frame in 0..106 {
            let seconds = frame as f32 / TICK_HZ as f32;
            let a = first_actor + x * WALK * seconds;
            let at = b + v * WALK * seconds;
            let other = Motion { at: Vec3::new(at.x, 0., at.y), velocity: v * WALK, undecided: false };
            let from = Vec3::new(a.x, 0., a.y);
            let dt = 1. / TICK_HZ as f32;
            let (_, forecast) = clearance(from, x * WALK, std::iter::once(&other), dt);
            let me = Motion { at: from, velocity: x * WALK, undecided: false };
            let (_, reverse) = clearance(other.at, other.velocity, std::iter::once(&me), dt);
            assert_eq!(forecast < DIAMETER_SQ, reverse < DIAMETER_SQ);
            signal.push(if forecast < DIAMETER_SQ { '1' } else { '0' });
            if forecast < DIAMETER_SQ {
                detected.get_or_insert(frame);
            }
            if first.is_none() {
                let next = choose(&grid, from, from + Vec3::X * 3., x * WALK, std::iter::once(&other), dt);
                assert!(next.distance(from + Vec3::X * WALK * dt) < 1e-6, "unexpected avoidance on {label} at {frame}");
            }
        }
        println!("prediction={label} first_tick={detected:?} flags={signal}");
        assert_eq!(detected, first, "{label}");
    }
}

#[test]
fn no_crossing_is_spawned_in_a_narrow_dead_end() {
    let grid = Walk::new(0.25, 5, 5, Vec2::ZERO, vec![0.; 25], Vec3::ZERO);
    assert!(crossing(&grid).is_none());
}

#[test]
#[ignore = "uses the installed open Paris street; set OSAGE_PARIS_WALK_DIR"]
fn two_passers_in_the_installed_street_follow_their_routes() {
    let dir = std::path::PathBuf::from(std::env::var_os("OSAGE_PARIS_WALK_DIR").unwrap());
    let grid = Walk::parse(
        &std::fs::read_to_string(dir.join("walk.json")).unwrap(),
        &std::fs::read(dir.join("walk.bin")).unwrap(),
    )
    .unwrap();
    let mut app = app(grid);
    let actors = pair(&mut app);
    for e in actors {
        println!("Bistro waypoints {:?}", app.world().get::<Patrol>(e).unwrap().ends);
    }
    let (gap, reversals) = track(&mut app, actors, "Bistro");
    assert!(gap >= 2. * RADIUS - 1e-5 && reversals.iter().all(|&n| n >= 2), "{gap} {reversals:?}");
}

#[test]
fn a_passer_walks_around_a_stationary_controlled_walker() {
    let mut app = app(flat());
    let [moving, spare] = pair(&mut app);
    app.world_mut().despawn(spare);
    let ends = [Vec3::NEG_X * 2.1, Vec3::X * 2.1];
    app.world_mut()
        .entity_mut(moving)
        .insert((Transform::from_translation(ends[0]), Patrol { ends, goal: 1, velocity: Vec2::ZERO }));
    let fixed = app.world_mut().spawn((Striding::default(), Transform::IDENTITY)).id();
    let mut nearest = f32::INFINITY;
    let mut reached = false;
    for _ in 0..180 {
        app.update();
        let at = app.world().get::<Transform>(moving).unwrap().translation;
        nearest = nearest.min(at.xz().length());
        reached |= at.distance(ends[1]) < 0.05;
        assert_eq!(app.world().get::<Transform>(fixed).unwrap().translation, Vec3::ZERO);
    }
    println!("stationary walker clearance={nearest:.7} reached={reached}");
    assert!(nearest >= 2. * RADIUS && reached);
}

#[test]
fn an_arrival_accounts_for_a_resident_and_a_player_who_just_moved() {
    use osage_world_module_api::{Owner, SessionKind, osage_proto::v0::Role};

    let join = Joined { session: 7, kind: SessionKind::Human, role: Role::Player, controller: Default::default() };
    let mut app = app(flat());
    let arrived = |app: &mut App| {
        let world = app.world_mut();
        let mut owned = world.query::<(&Owner, &Transform)>();
        owned.iter(world).find(|(owner, _)| owner.0 == 7).map(|(_, place)| place.translation).unwrap()
    };
    app.world_mut().write_message(join.clone());
    app.update();
    let preferred = arrived(&mut app);

    for moving_player in [false, true] {
        let mut app = self::app(flat());
        if moving_player {
            // Clear of the first candidate at the start of the tick, inside it at the end.
            app.world_mut().spawn((
                Transform::from_translation(preferred - Vec3::X * 0.48),
                Striding { stride: crate::walk::Stride { right: 1., run: true, ..default() }, ..default() },
            ));
        } else {
            let resident = pair(&mut app)[0];
            app.world_mut().get_mut::<Transform>(resident).unwrap().translation = preferred;
        }
        app.world_mut().write_message(join.clone());
        app.update();
        assert!(arrived(&mut app).distance(preferred) > 0.4);
    }
}
