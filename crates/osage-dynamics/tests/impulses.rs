// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

use glam::Vec3;
use osage_dynamics::{
    Capsule, ContactBodies, ContactImpulse, DynamicBox, Engine, ImpulseAttribution, ImpulseMeasurement, Part, Settings,
    Simulation, StaticMesh,
};
use std::collections::BTreeSet;

fn engines() -> Vec<Engine> {
    #[cfg(target_arch = "wasm32")]
    {
        vec![Engine::Rapier]
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        vec![Engine::Jolt, Engine::Rapier]
    }
}

fn settings() -> Settings {
    Settings {
        tick_hz: 60,
        gravity: 0.,
        terminal_speed: 80.,
        capsule: Capsule { radius: 0.15, half_segment: 0.2, mass: 75., friction: 0.5 },
    }
}

fn floor(left: f32, right: f32) -> StaticMesh {
    StaticMesh {
        id: format!("{left}..{right}"),
        material: None,
        vertices: vec![[left, 0., -2.], [left, 0., 2.], [right, 0., 2.], [right, 0., -2.]],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
    }
}

fn parts() -> [Part; 3] {
    let locks = [[0.; 2]; 3];
    [
        Part::between("root", None, [[-0.4, 1., 0.], [0.4, 1., 0.]], 0.06, 15., locks),
        Part::between("left", Some(0), [[-0.4, 1., 0.], [-0.4, 0., 0.]], 0.10, 6., locks),
        Part::between("right", Some(0), [[0.4, 1., 0.], [0.4, 0., 0.]], 0.10, 6., locks),
    ]
}

fn vertical(record: &ContactImpulse) -> f32 {
    match record.measurement {
        ImpulseMeasurement::SolverNormal(j) => j * record.normal.unwrap()[1],
        ImpulseMeasurement::AppliedVector(j) | ImpulseMeasurement::MomentumBalance(j) => j[1],
    }
}

#[test]
fn removing_each_support_identifies_the_reacting_limb_and_preserves_momentum() {
    let parts = parts();
    for engine in engines() {
        for (left, right) in [(true, false), (false, true), (true, true), (false, false)] {
            let mut meshes = Vec::new();
            if left {
                meshes.push(floor(-2., -0.06));
            }
            if right {
                meshes.push(floor(0.06, 2.));
            }
            let mut sim =
                Simulation::from_parts(engine, &meshes, &[], settings(), [0., 0.035, 0.], [0., -2., 0.], &parts)
                    .unwrap();
            let mut records = Vec::new();
            let mut seen = BTreeSet::new();
            let mut ambiguous = false;
            let moment = |sim: &Simulation| {
                sim.part_poses.iter().zip(&parts).map(|(p, b)| p.linear_velocity[1] * b.mass).sum::<f32>()
            };
            let start = moment(&sim);
            let mut total = 0.;
            for _ in 0..4 {
                sim.step_with_impulses(&mut records).unwrap();
                total += records.iter().map(vertical).sum::<f32>();
                for r in &records {
                    assert!(r.end_offset_s >= r.start_offset_s && r.end_offset_s <= 1. / 60. + 1e-7);
                    if vertical(r) <= 0.01 {
                        continue;
                    }
                    match &r.attribution {
                        ImpulseAttribution::Pair(pair) => {
                            seen.insert(*pair);
                            assert!(r.normal_impulse_ns().unwrap() > 0.);
                        }
                        ImpulseAttribution::Unresolved(candidates) => {
                            assert_eq!(engine, Engine::Jolt);
                            assert!(r.normal_impulse_ns().is_none() && r.pair().is_none());
                            ambiguous |= candidates.len() > 1;
                            seen.extend(candidates);
                        }
                    }
                }
            }
            let expected: BTreeSet<_> = [
                left.then_some(ContactBodies { part: 1, surface: 0 }),
                right.then_some(ContactBodies { part: 2, surface: usize::from(left) }),
            ]
            .into_iter()
            .flatten()
            .collect();
            assert_eq!(seen, expected, "{engine} {left}/{right}");
            assert_eq!(ambiguous, engine == Engine::Jolt && left && right, "{engine}");
            assert!(
                (total - (moment(&sim) - start)).abs() < 0.002,
                "{engine} total {total}, momentum {}",
                moment(&sim) - start
            );
        }
    }
}

#[test]
fn detached_force_observation_preserves_states_and_sees_the_free_pair_response() {
    let cfg = settings();
    let moving = DynamicBox {
        center_m: [0., 0., 0.],
        half_extents_m: [1.05, 0.4, 0.82],
        rotation_xyzw: [0., 0., 0., 1.],
        velocity_m_s: [2., 0., 0.],
        mass_kg: 1250.,
        friction: 0.5,
    };
    for engine in engines() {
        let new = || Simulation::new(engine, &[], &[moving], cfg, [0., 0.89, 0.], [2., -5., 0.]).unwrap();
        let (mut observer, mut plain) = (new(), new());
        let mut reported = 0.;
        let mut saw_applied = false;
        let mut samples = Vec::new();
        for tick in 0..20 {
            plain.step().unwrap();
            if tick >= 10 && tick % 2 == 0 {
                observer.step().unwrap();
            } else {
                observer.step_with_impulses(&mut samples).unwrap();
                for r in &samples {
                    if vertical(r) > 0.01 {
                        assert_eq!(r.pair(), Some(ContactBodies { part: 0, surface: 0 }), "{engine} {r:?}");
                    }
                    if matches!(r.measurement, ImpulseMeasurement::AppliedVector(_)) {
                        saw_applied = true;
                        assert!(r.point.is_some());
                        assert_eq!(r.start_offset_s, r.end_offset_s);
                    }
                    reported += vertical(r);
                }
            }
            assert_eq!(observer.part_poses, plain.part_poses, "{engine} tick {tick}");
            assert_eq!(observer.box_poses, plain.box_poses, "{engine} tick {tick}");
            assert_eq!(observer.contacts, plain.contacts, "{engine} tick {tick}");
        }
        let box_impulse = -Vec3::from_array(observer.box_poses[0].linear_velocity).y * moving.mass_kg;
        assert!(
            box_impulse > 200. && (box_impulse - reported).abs() < 0.01,
            "{engine} recoil {box_impulse}, observed {reported}"
        );
        assert_eq!(saw_applied, engine == Engine::Rapier);
    }
}

#[test]
fn an_initially_touching_pair_reports_zero_time_only_for_inward_motion() {
    // Binary-exact lengths put the lower cap exactly on the top face.
    let cfg = Settings { capsule: Capsule { half_segment: 0.5, radius: 0.25, ..settings().capsule }, ..settings() };
    let cuboid = DynamicBox {
        center_m: [0., 8., 0.],
        half_extents_m: [1., 0.25, 1.],
        rotation_xyzw: [0., 0., 0., 1.],
        velocity_m_s: [2., 0., 0.],
        mass_kg: 1250.,
        friction: 0.5,
    };
    for engine in engines() {
        for (height, velocity, at_contact) in [
            (9., [2., -8., 0.], true),
            (9., [4., 0., 0.], false),
            (9., [2., 4., 0.], false),
            (8.99, [2., -8., 0.], false),
        ] {
            let mut sim = Simulation::new(engine, &[], &[cuboid], cfg, [0., height, 0.], velocity).unwrap();
            let mut records = Vec::new();
            sim.step_with_impulses(&mut records).unwrap();
            assert_eq!(sim.swept_impact_offset_s, at_contact.then_some(0.), "{engine} {height} {velocity:?}");
            let recoil = -cuboid.mass_kg * sim.box_poses[0].linear_velocity[1];
            if at_contact {
                let ideal = 8. / (1. / cfg.capsule.mass + 1. / cuboid.mass_kg);
                assert!((recoil - ideal).abs() < 0.02, "{engine} {recoil} vs {ideal}");
                if engine == Engine::Rapier {
                    let j = records.iter().find(|r| matches!(r.measurement, ImpulseMeasurement::AppliedVector(_)));
                    assert_eq!(j.map(|r| (r.start_offset_s, r.end_offset_s)), Some((0., 0.)));
                }
            } else if velocity[1] >= 0. {
                assert!(recoil.abs() < 0.01, "{engine} {velocity:?} {recoil}");
            }
        }
    }
}

#[test]
fn an_isolated_impulse_at_the_tick_boundary_keeps_its_contact_witness() {
    let cfg = Settings {
        tick_hz: 60,
        gravity: 9.81,
        terminal_speed: 55.,
        capsule: Capsule { radius: 0.28, half_segment: 0.6, mass: 75., friction: 0.5 },
    };
    let cuboid = DynamicBox {
        center_m: [7., 9., 10.],
        half_extents_m: [1.05, 0.4, 0.82],
        rotation_xyzw: [0., 0., 0., 1.],
        velocity_m_s: [1., 0., 0.],
        mass_kg: 1250.,
        friction: 0.5,
    };
    let start = |x| {
        Simulation::new(
            Engine::Rapier,
            &[],
            &[cuboid],
            cfg,
            [x, 11. + (cfg.capsule.half_segment + cfg.capsule.radius), 10.],
            [0.5, -4., 0.],
        )
        .unwrap()
    };
    let (mut watched, mut plain, mut miss) = (start(7.), start(7.), start(12.));
    let mut samples = Vec::new();
    for tick in 1..=25 {
        let previous = [watched.pose.linear_velocity[1], watched.box_poses[0].linear_velocity[1]];
        plain.step().unwrap();
        miss.step().unwrap();
        watched.step_with_impulses(&mut samples).unwrap();
        assert_eq!(watched.contacts, plain.contacts);
        assert_eq!(watched.part_poses, plain.part_poses);
        assert_eq!(watched.box_poses, plain.box_poses);
        assert!(miss.contacts.is_empty() && miss.swept_impact_offset_s.is_none());
        if tick == 24 {
            let [contact] = watched.contacts.as_slice() else { panic!("expected one applied witness") };
            let geometric = watched.swept_impact_offset_s.unwrap();
            let applied = contact.applied_offset_s.unwrap();
            assert!(geometric <= applied && applied - geometric < 0.000003);
            assert_eq!(applied, 1. / 60.);
            assert!(contact.separation.abs() < 0.00003);
            let impulse =
                samples.iter().find(|r| matches!(r.measurement, ImpulseMeasurement::AppliedVector(_))).unwrap();
            assert_eq!(impulse.pair(), Some(ContactBodies { part: contact.part, surface: contact.surface }));
            assert_eq!((impulse.point, impulse.normal), (Some(contact.point), Some(contact.normal)));
            assert_eq!((impulse.start_offset_s, impulse.end_offset_s), (applied, applied));
            let gravity_speed = cfg.gravity / cfg.tick_hz as f32;
            let actor_n_s = (watched.pose.linear_velocity[1] - previous[0] + gravity_speed) * cfg.capsule.mass;
            let box_n_s = (watched.box_poses[0].linear_velocity[1] - previous[1] + gravity_speed) * cuboid.mass_kg;
            assert!(actor_n_s > 250. && (vertical(impulse) - actor_n_s).abs() < 0.01);
            assert!((actor_n_s + box_n_s).abs() < 0.01);
        } else {
            assert!(watched.contacts.iter().all(|c| c.applied_offset_s.is_none()));
            if tick < 24 {
                assert!(watched.contacts.is_empty());
            }
        }
    }
}

#[test]
fn one_sphere_between_two_surfaces_retains_both_partners() {
    let cfg = Settings { gravity: 9.81, capsule: Capsule { half_segment: 0., ..settings().capsule }, ..settings() };
    for engine in engines() {
        let mut meshes = [floor(-2., 0.), floor(0., 2.)];
        for mesh in &mut meshes {
            for point in &mut mesh.vertices {
                point[1] = point[0].abs() * 0.4;
            }
        }
        let mut sim = Simulation::new(engine, &meshes, &[], cfg, [0., 0.35, 0.], [0., -2., 0.]).unwrap();
        let mut out = Vec::new();
        let mut surfaces = BTreeSet::new();
        let mut shared = false;
        for _ in 0..14 {
            sim.step_with_impulses(&mut out).unwrap();
            for row in &out {
                if vertical(row) < 0.01 {
                    continue;
                }
                match &row.attribution {
                    ImpulseAttribution::Pair(p) => {
                        assert_eq!(p.part, 0);
                        surfaces.insert(p.surface);
                    }
                    ImpulseAttribution::Unresolved(candidates) => {
                        if candidates.len() == 2 {
                            assert_eq!(candidates.iter().map(|c| c.part).collect::<Vec<_>>(), [0, 0]);
                            assert!(row.point.is_none() && row.normal.is_none());
                            shared = true;
                        }
                        surfaces.extend(candidates.iter().map(|c| c.surface));
                    }
                }
            }
        }
        assert_eq!(surfaces, BTreeSet::from([0, 1]), "{engine}");
        assert_eq!(shared, engine == Engine::Jolt);
    }
}

#[test]
fn an_observer_can_join_an_existing_manifold_without_waking_the_body() {
    let cfg = Settings { gravity: 9.81, ..settings() };
    for engine in engines() {
        let new = || Simulation::new(engine, &[floor(-2., 2.)], &[], cfg, [0., 0.35, 0.], [0.; 3]).unwrap();
        let mut control = new();
        let mut observer = new();
        let mut out = Vec::new();
        for tick in 0..160 {
            control.step().unwrap();
            if tick % 2 == 0 {
                observer.step().unwrap();
            } else {
                observer.step_with_impulses(&mut out).unwrap();
                if (3..20).contains(&tick) {
                    assert!(out.iter().map(vertical).sum::<f32>() > 5., "{engine} lost support at {tick}");
                }
                if tick > 130 {
                    assert!(
                        out.iter().all(|r| r.pair().is_none() || r.normal_impulse_ns().unwrap_or(0.) < 0.001),
                        "{engine} stale impulse at {tick}: {out:?}"
                    );
                }
            }
            assert_eq!(observer.part_poses, control.part_poses, "{engine} tick {tick}");
            assert_eq!(observer.contacts, control.contacts, "{engine} tick {tick}");
        }
    }
}
