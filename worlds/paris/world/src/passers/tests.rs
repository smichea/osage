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
        actors.map(|id| (world.get::<Transform>(id).unwrap().translation, world.get::<Patrol>(id).unwrap().forward))
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
                .insert((Transform::from_translation(ends[0]), Patrol::new(ends.to_vec(), ends.map(|p| p.xz()))));
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
        println!("Bistro waypoints {:?}", app.world().get::<Patrol>(e).unwrap().points);
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
        .insert((Transform::from_translation(ends[0]), Patrol::new(ends.to_vec(), ends.map(|p| p.xz()))));
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

fn replay_detour(name: &str, grid: Walk, endpoints: [Vec2; 2]) -> [f32; 2] {
    use std::io::Write;
    let start = std::time::Instant::now();
    let mut app = app(grid);
    let startup_ms = start.elapsed().as_secs_f64() * 1000.;
    let world = app.world_mut();
    let entity = world.query_filtered::<Entity, With<Patrol>>().single(world).unwrap();
    let cells = world.resource::<Grid>().0.clone();
    let points = &world.get::<Patrol>(entity).unwrap().points;
    assert!(points.len() >= 3, "expected an obstacle between the endpoints");
    let safe = [points.first().unwrap().xz(), points.last().unwrap().xz()];
    for (p, requested) in safe.into_iter().zip(endpoints) {
        assert!(p.distance(requested) < ARRIVAL && cells.holds(p));
        if cells.holds(requested) {
            assert_eq!(p, requested);
        }
    }
    let mut direct = points[0];
    for _ in 0..1800 {
        let wish = endpoints[1] - direct.xz();
        let stride = crate::walk::Stride {
            forward: (wish.length() * TICK_HZ as f32 / WALK).min(1.),
            heading: (-wish.x).atan2(-wish.y),
            ..default()
        };
        direct = cells.step(direct, &stride, 1. / TICK_HZ as f32);
    }
    let residual = direct.xz().distance(endpoints[1]);
    assert!(residual > ARRIVAL, "the direct controller already arrives");
    println!(
        "route={name} direct_residual_m={residual:.4} endpoint_offset_m={:.4} planned={points:?}",
        safe[1].distance(endpoints[1])
    );
    let mut trace = std::env::var_os("OSAGE_ROUTE_TRACE_DIR").map(|dir| {
        let path = std::path::PathBuf::from(dir).join(format!("{name}.csv"));
        std::io::BufWriter::new(std::fs::File::create(path).unwrap())
    });
    if let Some(file) = &mut trace {
        writeln!(file, "tick,x,z\n0,{},{}", safe[0].x, safe[0].y).unwrap();
    }
    let corners = points.len() - 2;
    let mut previous = safe[0];
    let mut distance = 0.;
    let mut goal = 1;
    let mut outbound = 0.;
    for tick in 1..=1800 {
        app.update();
        let at = app.world().get::<Transform>(entity).unwrap().translation;
        if let Some(file) = &mut trace {
            writeln!(file, "{tick},{},{}", at.x, at.z).unwrap();
        }
        let step = at.xz().distance(previous);
        distance += step;
        assert!(step <= WALK / TICK_HZ as f32 + 1e-5);
        for i in 0..=8 {
            assert!(cells.holds(previous.lerp(at.xz(), i as f32 / 8.)), "outside the street at tick {tick}");
        }
        assert!((at.y - cells.ground(at.xz()).unwrap()).abs() < 1e-4);
        if at.xz().distance(endpoints[goal]) < 0.05 {
            println!(
                "detour to_end={goal} tick={tick} leg_m={:.4} corners={corners} app_start_ms={startup_ms:.2}",
                distance - outbound
            );
            if goal == 0 {
                return [outbound, distance - outbound];
            }
            goal = 0;
            outbound = distance;
        }
        previous = at.xz();
    }
    panic!("did not reach {:?} from {previous:?}", endpoints[goal]);
}

#[test]
fn server_routes_a_passer_around_a_wall_from_just_the_endpoints() {
    let ends = [Vec2::new(1.25, 3.25), Vec2::new(8.75, 3.25)];
    let header = serde_json::json!({"cell": 0.5, "columns": 20, "rows": 20, "origin":[0,0],
        "start":[1.25,0,3.25], "patrols":[ends.map(|p| p.to_array())]});
    let mut heights = vec![0_f32; 400];
    for row in 0..13 {
        heights[row * 20 + 10] = f32::NAN;
    }
    let parse = |heights: &[f32]| {
        Walk::parse(&header.to_string(), &heights.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>()).unwrap()
    };
    replay_detour("wall", parse(&heights), ends);
    let mut shared = parse(&heights);
    shared.patrols = Some(vec![ends.map(|p| p.to_array()), [ends[1].to_array(), ends[0].to_array()]]);
    let mut traffic = app(shared);
    let both = pair(&mut traffic);
    let (gap, visits) = track(&mut traffic, both, "two-way detour");
    assert!(gap >= 2. * RADIUS - 1e-5 && visits.iter().all(|&n| n >= 2), "{gap} {visits:?}");

    // The closing section has ground, but it is a metre above the street.
    for row in 13..20 {
        heights[row * 20 + 10] = 1.;
    }
    let closed = parse(&heights);
    assert!(closed.route(ends[0], ends[1]).is_none());
    let mut app = app(closed);
    let world = app.world_mut();
    assert_eq!(world.query_filtered::<Entity, With<Patrol>>().iter(world).count(), 0);
}

#[test]
fn patrols_choose_the_shorter_side_of_a_wall_in_both_directions() {
    let mut heights = vec![0.; 80 * 40];
    for row in 8..32 {
        heights[row * 80 + 16..row * 80 + 20].fill(f32::NAN);
    }
    let mut grid = Walk::new(0.25, 80, 40, Vec2::ZERO, heights, Vec3::ZERO);
    // The lower passage crosses x=4 and x=5 at z <= 2 minus the body radius.
    // The distances to those half-lines plus the wall width bound every such route.
    let a = Vec2::new(3.375, 7.125);
    let b = Vec2::new(18.125, 2.125);
    let lower_bound = a.distance(Vec2::new(4., 2. - RADIUS)) + 1. + b.distance(Vec2::new(5., 2. - RADIUS));
    for mirrored in [false, true] {
        let ends = [a, b].map(|p| if mirrored { Vec2::new(p.x, 10. - p.y) } else { p });
        for [start, goal] in [ends, [ends[1], ends[0]]] {
            let route = grid.route(start, goal).unwrap();
            let distance: f32 = route
                .windows(2)
                .map(|pair| {
                    assert!(grid.clear_segment(pair[0].xz(), pair[1].xz()));
                    pair[0].xz().distance(pair[1].xz())
                })
                .sum();
            assert!(distance < lower_bound, "route {distance} m, opposite passage at least {lower_bound} m");
        }
    }
    grid.patrols = Some(vec![[a.to_array(), b.to_array()]]);
    let legs = replay_detour("two-passages", grid, [a, b]);
    for length in legs {
        // The server can turn within the arrival radius at either end.
        assert!(length + 2. * ARRIVAL < lower_bound);
    }
}

#[test]
fn the_walkers_footprint_rejects_an_opening_narrower_than_its_body() {
    let mut heights = vec![0.; 100 * 100];
    for z in 0..100 {
        if !(49..52).contains(&z) {
            heights[z * 100 + 50] = f32::NAN;
        }
    }
    let walk = Walk::new(0.1, 100, 100, Vec2::ZERO, heights, Vec3::ZERO);
    assert!(walk.height(Vec2::new(5.05, 5.05)).is_some());
    assert!(walk.route(Vec2::new(4.05, 5.05), Vec2::new(6.05, 5.05)).is_none());
}

#[test]
#[ignore = "requires the installed Bistro walk grid"]
fn server_finds_the_bistro_detour_and_walks_it_both_ways() {
    let dir = std::path::PathBuf::from(std::env::var_os("OSAGE_PARIS_WALK_DIR").unwrap());
    let mut header: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("walk.json")).unwrap()).unwrap();
    let ends = [Vec2::new(24.916752, -61.643517), Vec2::new(17.166752, -70.39352)];
    header["patrols"] = serde_json::json!([ends.map(|p| p.to_array())]);
    let grid = Walk::parse(&header.to_string(), &std::fs::read(dir.join("walk.bin")).unwrap()).unwrap();
    replay_detour("bistro", grid, ends);
}

#[test]
fn five_connected_squares_around_a_missing_room() {
    // Five 2 m squares in a U, with no floor outside the U or in its middle.
    let ends = [Vec2::new(1., 1.), Vec2::new(5., 1.)];
    let header = serde_json::json!({"cell":0.25, "columns":24, "rows":16, "origin":[0,0],
        "start":[1,0,1], "patrols":[ends.map(|p| p.to_array())]});
    let mut heights = vec![0_f32; 24 * 16];
    for z in 0..8 {
        for x in 8..16 {
            heights[z * 24 + x] = f32::NAN;
        }
    }
    let parse = |heights: &[f32]| {
        Walk::parse(&header.to_string(), &heights.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>()).unwrap()
    };
    let grid = parse(&heights);
    // Both ends of this 4 cm step fit, but its middle clips the corner at (2,2).
    let a = Vec2::new(1.845, 2.127);
    let b = Vec2::new(1.873, 2.155);
    assert!(grid.holds(a) && grid.holds(b));
    assert!(!grid.holds(a.midpoint(b)));
    let from = Vec3::new(a.x, 0., a.y);
    let applied = grid.slide(from, b - a).xz();
    assert!(applied.distance(b) > 0.01 && grid.holds(applied));
    let centres = [(1., 1.), (1., 3.), (3., 3.), (5., 3.), (5., 1.)].map(|(x, z)| Vec2::new(x, z));
    assert!(centres.iter().all(|&p| grid.holds(p)));
    assert!(!grid.holds(Vec2::new(3., 1.)));
    for p in centres.windows(2) {
        for [a, b] in [[p[0], p[1]], [p[1], p[0]]] {
            assert_eq!(grid.route(a, b).unwrap().len(), 2);
        }
    }
    assert_eq!(grid.route(centres[1], centres[3]).unwrap().len(), 2);
    assert_eq!(grid.route(centres[3], centres[1]).unwrap().len(), 2);
    replay_detour("u-street", grid, ends);
    // This height field represents the closed connection as one column without floor.
    for z in 8..16 {
        heights[z * 24 + 8] = f32::NAN;
    }
    let sealed = parse(&heights);
    assert!(sealed.route(centres[1], centres[3]).is_none());
    assert!(sealed.route(centres[3], centres[1]).is_none());
    assert!(sealed.route(ends[0], ends[1]).is_none());
    assert!(sealed.route(ends[1], ends[0]).is_none());
}
