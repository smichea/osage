// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

use osage_dynamics::{Capsule, Engine, Settings, Simulation, StaticMesh};

fn settings() -> Settings {
    Settings {
        tick_hz: 60,
        gravity: 9.81,
        terminal_speed: 55.,
        capsule: Capsule { radius: 0.28, half_segment: 0.6, mass: 75., friction: 0.5 },
    }
}

fn sheet(name: &str, left_y: f32, right_y: f32, edge_z: f32) -> StaticMesh {
    StaticMesh {
        material: None,
        id: name.into(),
        vertices: vec![[-20., left_y, -20.], [-20., left_y, edge_z], [20., right_y, edge_z], [20., right_y, -20.]],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
    }
}

fn engines() -> Vec<Engine> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        vec![Engine::Rapier, Engine::Jolt]
    }
    #[cfg(target_arch = "wasm32")]
    {
        vec![Engine::Rapier]
    }
}

#[test]
fn fast_capsule_lands_on_a_thin_upper_deck_and_stays_there() {
    for engine in engines() {
        let surfaces = [sheet("road", 8., 8., 12.), sheet("metro", 16., 16., 3.8)];
        let mut sim = Simulation::new(engine, &surfaces, &[], settings(), [0., 22.88, 0.], [0., -55., 0.]).unwrap();
        let mut first = None;
        for tick in 1..=160 {
            sim.step().unwrap();
            if let Some(c) = sim.contacts.first() {
                first.get_or_insert((tick, c.surface));
                assert_eq!(c.surface, 1, "{engine} reached the wrong deck");
                assert!(c.normal[1] > 0.99, "{engine} gave a downward support normal");
            }
        }
        let (tick, surface) = first.unwrap();
        assert_eq!(surface, 1);
        assert!(tick <= 9, "{engine} first contacted the deck at {tick}");
        assert!((sim.pose.center[1] - 16.88).abs() < 0.02, "{engine}: {:?}", sim.pose);
    }
}

#[test]
fn a_sloping_triangle_gives_an_oblique_normal() {
    for engine in engines() {
        let mut sim =
            Simulation::new(engine, &[sheet("slope", 0., 10., 20.)], &[], settings(), [2., 9., 1.], [0.; 3]).unwrap();
        let contact = (0..100)
            .find_map(|_| {
                sim.step().unwrap();
                sim.contacts.first().copied()
            })
            .unwrap();
        assert!(contact.normal[0] < -0.20 && contact.normal[1] > 0.94, "{engine}: {contact:?}");
        assert!((contact.point[1] - (5. + contact.point[0] * 0.25)).abs() < 0.01, "{engine}: {contact:?}");
    }
}

#[test]
fn rapier_reports_the_input_face_on_both_sides_of_a_diagonal() {
    let mut floor = sheet("court", 0., 0., 20.);
    // Put the two cells in the opposite input order from the vertex sequence.
    floor.triangles.reverse();
    for (x, z, face) in [(-8., 8., 1), (8., -8., 0)] {
        let mut sim =
            Simulation::new(Engine::Rapier, std::slice::from_ref(&floor), &[], settings(), [x, 1.9, z], [0.; 3])
                .unwrap();
        let hit = (0..60)
            .find_map(|_| {
                sim.step().unwrap();
                sim.contacts.first().copied()
            })
            .unwrap();
        assert_eq!(hit.surface_triangle, Some(face));
    }
}

#[test]
fn a_joined_pair_bends_on_impact_and_keeps_its_anchor() {
    use glam::{Quat, Vec3};
    use osage_dynamics::Part;
    // A sloping floor gives the horizontal limb a moment about the free Z axis.
    let parts = [
        Part::between("torso", None, [[0., 0.5, 0.], [0., 1.2, 0.]], 0.12, 8., [[0.; 2]; 3]),
        Part::between("arm", Some(0), [[0., 1., 0.], [0.7, 1., 0.]], 0.07, 2., [[0.; 2], [0.; 2], [-90., 90.]]),
    ];
    for engine in engines() {
        let floor = sheet("slope", -4., 4., 20.);
        let mut sim =
            Simulation::from_parts(engine, &[floor], &[], settings(), [3., 3., 1.], [1.3, -2., 0.], &parts).unwrap();
        let mut touched = false;
        let mut turn = 0.0_f32;
        let mut gap = 0.;
        for _ in 0..240 {
            sim.step().unwrap();
            touched |= !sim.contacts.is_empty();
            let current = |i: usize| Quat::from_array(sim.part_poses[i].rotation_xyzw) * parts[i].rotation.inverse();
            let anchor = |i: usize| {
                Vec3::from_array(sim.part_poses[i].center) + current(i) * (parts[1].anchor - parts[i].center)
            };
            turn = turn.max(current(0).angle_between(current(1)));
            gap = anchor(0).distance(anchor(1));
        }
        assert!(touched && turn > 0.05, "{engine} did not articulate on impact");
        assert!(gap < 0.02, "{engine} ends with a joint gap of {gap} m");
    }
}

#[test]
fn a_free_end_swings_then_settles_when_its_parent_rests_on_a_ledge() {
    use glam::Vec3;
    use osage_dynamics::Part;
    let parts = [
        Part::between("support", None, [[0., 0.1, -0.9], [0., 0.1, 0.4]], 0.1, 30., [[0.; 2]; 3]),
        Part::between("tip", Some(0), [[0., 0.1, 0.38], [0., 0.1, 0.6]], 0.04, 0.45, [[-120., 120.], [0.; 2], [0.; 2]]),
    ];
    let floor = sheet("ledge", 0., 0., 0.);
    let mut sim = Simulation::from_parts(Engine::Rapier, &[floor], &[], settings(), [0.; 3], [0.; 3], &parts).unwrap();
    let mut first_swing = 0_f32;
    let mut late_swing = 0_f32;
    for tick in 1..=300 {
        sim.step().unwrap();
        assert!(sim.contacts.iter().all(|hit| hit.part == 0), "the swinging tip touched the ledge");
        let relative = Vec3::from(sim.part_poses[1].angular_velocity) - Vec3::from(sim.part_poses[0].angular_velocity);
        if tick <= 30 {
            first_swing = first_swing.max(relative.length());
        }
        if tick >= 180 {
            assert!(!sim.contacts.is_empty(), "the parent lost its support");
            late_swing = late_swing.max(relative.length());
        }
    }
    assert!(first_swing > 8., "the tip reached only {first_swing} rad/s on release");
    assert!(late_swing < 1., "the unloaded tip still swings at {late_swing} rad/s after 3 seconds");
}

#[test]
fn an_unloaded_branch_swings_then_rests_below_its_supported_parent() {
    use glam::{Quat, Vec3};
    use osage_dynamics::Part;
    let rig = [
        Part::between("base", None, [[0., 0.1, -0.9], [0., 0.1, 0.4]], 0.1, 30., [[0.; 2]; 3]),
        Part::between(
            "link",
            Some(0),
            [[0., 0.1, 0.38], [0., 0.1, 0.6]],
            0.04,
            0.45,
            [[-120., 120.], [0.; 2], [0.; 2]],
        ),
        Part::between("tip", Some(1), [[0., 0.1, 0.6], [0., 0.1, 0.82]], 0.04, 0.45, [[0.; 2]; 3]),
    ];
    let floor = sheet("ledge", 0., 0., 0.);
    let mut sim =
        Simulation::from_parts(Engine::Rapier, &[floor], &[], settings(), [0.25, 0., 0.], [0.; 3], &rig).unwrap();
    let mut initial_swing = 0_f32;
    let mut quiet_ticks = 0;
    let mut impacts = Vec::new();
    for tick in 1..=1800 {
        sim.step_with_impulses(&mut impacts).unwrap();
        assert!(sim.contacts.iter().all(|c| c.part == 0), "a suspended link touched the ledge");
        if tick <= 30 {
            assert!(impacts.iter().any(|i| i.normal_impulse_ns().is_some_and(|n| n > 0.)), "the base lost support");
            let spin = |i: usize| Vec3::from_array(sim.part_poses[i].angular_velocity);
            initial_swing = initial_swing.max((spin(1) - spin(0)).length());
        }
        if sim.part_poses.iter().all(|p| {
            Vec3::from_array(p.linear_velocity).length() < 0.1 && Vec3::from_array(p.angular_velocity).length() < 0.25
        }) {
            quiet_ticks += 1;
        } else {
            quiet_ticks = 0;
        }
    }
    assert!(initial_swing > 5., "initial swing under gravity reached only {initial_swing} rad/s");
    assert!(quiet_ticks >= 31, "the three bodies did not stay quiet through 30 s");
    let axis = Quat::from_array(sim.part_poses[2].rotation_xyzw) * Vec3::Y;
    assert!(axis.angle_between(Vec3::NEG_Y).to_degrees() < 5., "the unloaded mass did not hang below the pivot");
}

#[test]
fn a_load_bearing_descendant_keeps_turning_about_a_frictionless_vertical_hinge() {
    use glam::Quat;
    use osage_dynamics::{AnimatedBody, AnimationHandoff, BodyTransform, Part, RigStart};
    let smooth = osage_objects::CollisionMaterial { friction: 0., restitution: 0. };
    let mut rig = [
        Part::between("base", None, [[-0.9, 0.1, 0.], [0., 0.1, 0.]], 0.1, 30., [[0.; 2]; 3]),
        Part::between("link", Some(0), [[0., 0.1, 0.], [0.3, 0.1, 0.]], 0.035, 0.45, [[0.; 2], [-170., 170.], [0.; 2]]),
        Part::between("tip", Some(1), [[0.3, 0.1, 0.], [0.5, 0.1, 0.]], 0.1, 0.45, [[0.; 2]; 3]),
    ];
    for p in &mut rig {
        p.material = Some(smooth);
    }
    let mut floor = sheet("court", 0., 0., 20.);
    floor.material = Some(smooth);
    let dt = 1. / 120.;
    let pivot = rig[1].anchor;
    let animation = AnimationHandoff {
        sample_interval_s: dt,
        parts: rig
            .iter()
            .map(|p| {
                let previous = Quat::from_rotation_y(if p.parent.is_some() { -1.2 * dt } else { 0. });
                AnimatedBody {
                    name: p.name.clone(),
                    current: BodyTransform { center_m: p.center.into(), rotation_xyzw: p.rotation.into() },
                    previous: BodyTransform {
                        center_m: (pivot + previous * (p.center - pivot)).into(),
                        rotation_xyzw: (previous * p.rotation).into(),
                    },
                }
            })
            .collect(),
    };
    let start = RigStart { origin_m: [0.7, 0., 0.32], velocity_m_s: [0.; 3], animation: Some(&animation) };
    let mut sim = Simulation::from_rig(Engine::Rapier, &[floor], &[], settings(), &rig, start).unwrap();
    let mut impacts = Vec::new();
    for tick in 1..=90 {
        sim.step_with_impulses(&mut impacts).unwrap();
        if tick >= 30 {
            let mut loaded = [false; 3];
            for impact in &impacts {
                if let Some(pair) = impact.pair()
                    && impact.normal_impulse_ns().is_some_and(|n| n > 0.)
                {
                    loaded[pair.part] = true;
                }
            }
            assert_eq!(loaded, [true, false, true], "the tip must actually carry a floor reaction");
            let spin = |i: usize| sim.part_poses[i].angular_velocity[1];
            assert!(spin(1) - spin(0) > 0.8, "the moving branch was braked while its descendant bore load");
        }
    }
}

#[test]
fn dynamic_contact_transfers_momentum_to_the_second_body() {
    use glam::Vec3;
    use osage_dynamics::DynamicBox;
    let cfg = Settings { gravity: 0., ..settings() };
    let car = DynamicBox {
        center_m: [0.; 3],
        half_extents_m: [1., 0.6, 0.8],
        rotation_xyzw: [0., 0., 0., 1.],
        velocity_m_s: [0.; 3],
        mass_kg: 1250.,
        friction: 0.5,
    };
    let velocity = Vec3::new(6., 0., 0.);
    let initial = velocity * cfg.capsule.mass;
    for engine in engines() {
        let mut sim = Simulation::new(engine, &[], &[car], cfg, [-2., 0., 0.3], velocity.into()).unwrap();
        let mut hit = false;
        for _ in 0..90 {
            sim.step().unwrap();
            hit |= sim.contacts.iter().any(|c| c.surface == 0);
            assert!(sim.contacts.iter().all(|c| c.surface_triangle.is_none()));
        }
        let chassis = sim.box_poses[0];
        let total = Vec3::from_array(sim.pose.linear_velocity) * cfg.capsule.mass
            + Vec3::from_array(chassis.linear_velocity) * car.mass_kg;
        assert!(
            hit && chassis.linear_velocity[0] > 0.1 && chassis.angular_velocity[1].abs() > 0.01,
            "{engine} failed to move and turn the box"
        );
        assert!(initial.distance(total) / initial.length() < 0.01, "{engine} momentum {total}, expected {initial}");
    }
}

#[test]
fn the_authored_surface_restitution_changes_the_capsule_rebound() {
    for engine in engines() {
        let upward_speed = |restitution| {
            let mut floor = sheet("surface", 0., 0., 6.);
            floor.material = Some(osage_objects::CollisionMaterial { friction: 0.4, restitution });
            let mut sim = Simulation::new(engine, &[floor], &[], settings(), [0., 3., 0.], [0., -4., 0.]).unwrap();
            (0..80)
                .map(|_| {
                    sim.step().unwrap();
                    sim.pose.linear_velocity[1]
                })
                .fold(0., f32::max)
        };
        let (soft, elastic) = (upward_speed(0.), upward_speed(0.9));
        assert!(elastic > soft + 2., "{engine} rebound speeds {soft}, {elastic}");
    }
}

#[test]
fn centered_moving_pair_transmits_vertical_impulse_without_spinning() {
    use glam::Vec3;
    use osage_dynamics::DynamicBox;

    for engine in engines() {
        for tick_hz in [20, 60] {
            let cfg = Settings { tick_hz, ..settings() };
            for speed in [0., 2.8] {
                let bonnet = DynamicBox {
                    center_m: [0., 40., 0.],
                    half_extents_m: [1.05, 0.4, 0.82],
                    rotation_xyzw: [0., 0., 0., 1.],
                    velocity_m_s: [speed, 0., 0.],
                    mass_kg: 1250.,
                    friction: 0.5,
                };
                let mut simulation =
                    Simulation::new(engine, &[], &[bonnet], cfg, [0., 41.88, 0.], [speed, -8., 0.]).unwrap();
                let mut touched = false;
                for _ in 0..tick_hz / 4 {
                    simulation.step().unwrap();
                    touched |= !simulation.contacts.is_empty();
                }
                assert!(touched, "{engine} {tick_hz} Hz missed the bonnet");
                let box_pose = simulation.box_poses[0];
                let expected = Vec3::new(
                    speed,
                    -cfg.gravity * 0.25 - 8. * cfg.capsule.mass / (cfg.capsule.mass + bonnet.mass_kg),
                    0.,
                );
                for pose in [simulation.pose, box_pose] {
                    assert!(
                        Vec3::from_array(pose.linear_velocity).distance(expected) < 1.0e-3,
                        "{engine} {tick_hz} Hz recoil"
                    );
                    assert!(
                        Vec3::from_array(pose.angular_velocity).length() < 1.0e-3,
                        "{engine} {tick_hz} Hz centered torque"
                    );
                }
            }
        }
    }
}

#[test]
fn a_free_pair_uses_the_inward_sweep_and_leaves_other_contacts_to_the_solver() {
    use glam::{Quat, Vec3};
    use osage_dynamics::DynamicBox;

    let normal = Quat::from_rotation_z(0.42) * Vec3::Y;
    let gap = 0.7125;
    let speed = 8.7;
    let surface = DynamicBox {
        center_m: [1.3, 7.1, 0.7],
        half_extents_m: [0.95, 0.33, 0.77],
        rotation_xyzw: Quat::from_rotation_z(0.42).to_array(),
        velocity_m_s: [2.8, -0.1, 0.25],
        mass_kg: 567.,
        friction: 0.42,
    };
    let cfg = Settings { gravity: 0., ..settings() };
    let center = Vec3::from(surface.center_m)
        + (surface.half_extents_m[1] + cfg.capsule.radius + gap) * normal
        + cfg.capsule.half_segment * Vec3::Y;
    let velocity = Vec3::from(surface.velocity_m_s) - speed * normal;

    for engine in engines() {
        for tick_hz in [20, 60] {
            let cfg = Settings { tick_hz, ..cfg };
            for (meshes, solids, isolated) in [
                (vec![], vec![surface], true),
                (vec![], vec![surface, DynamicBox { center_m: [50.; 3], ..surface }], false),
                (vec![sheet("ground beneath the box", 6.53, 6.53, 20.)], vec![surface], false),
            ] {
                let mut sim =
                    Simulation::new(engine, &meshes, &solids, cfg, center.to_array(), velocity.to_array()).unwrap();
                let mut swept = None;
                let mut touched = false;
                for tick in 0..tick_hz / 4 {
                    sim.step().unwrap();
                    if let Some(offset) = sim.swept_impact_offset_s {
                        swept.get_or_insert(tick as f32 / tick_hz as f32 + offset);
                    }
                    touched |= sim.contacts.iter().any(|contact| contact.surface == meshes.len());
                }
                assert!(touched, "{engine} lost the moving contact");
                assert_eq!(swept.is_some(), isolated);
                if let Some(swept) = swept {
                    assert!((swept - gap / speed).abs() < 1.0e-6, "{engine} at {swept} s");
                    let capsule_impulse = cfg.capsule.mass * (Vec3::from(sim.pose.linear_velocity) - velocity);
                    let box_impulse = surface.mass_kg
                        * (Vec3::from(sim.box_poses[0].linear_velocity) - Vec3::from(surface.velocity_m_s));
                    assert!(box_impulse.dot(normal) < -400.);
                    assert!((capsule_impulse + box_impulse).length() < 0.01);
                }
            }
        }
    }
}

#[test]
fn a_capsule_skimming_a_free_box_delivers_no_impulse() {
    use osage_dynamics::DynamicBox;

    let cfg = Settings { gravity: 0., tick_hz: 20, ..settings() };
    let cuboid = DynamicBox {
        center_m: [0.; 3],
        half_extents_m: [1.05, 0.4, 0.82],
        rotation_xyzw: [0., 0., 0., 1.],
        velocity_m_s: [0.; 3],
        mass_kg: 1250.,
        friction: 0.5,
    };
    for engine in engines() {
        for gap in [0., 0.001] {
            for (center, velocity) in [
                ([-2.7, 0.4 + cfg.capsule.radius + cfg.capsule.half_segment + gap, 0.], [12., 0., 0.]),
                ([1.05 + cfg.capsule.radius + gap, 0., -2.7], [0., 0., 12.]),
            ] {
                let mut sim = Simulation::new(engine, &[], &[cuboid], cfg, center, velocity).unwrap();
                for _ in 0..10 {
                    sim.step().unwrap();
                    assert!(sim.swept_impact_offset_s.is_none());
                    assert!(sim.contacts.is_empty(), "{engine} speculative event at {gap} m");
                    assert_eq!(sim.box_poses[0].linear_velocity, [0.; 3], "{engine} false impulse");
                    assert_eq!(sim.box_poses[0].angular_velocity, [0.; 3], "{engine} false torque");
                }
            }
        }
    }
}

#[test]
fn a_sphere_uses_native_contact_and_a_bouncing_capsule_keeps_its_restitution() {
    use osage_dynamics::{DynamicBox, Part};
    let cfg = Settings { gravity: 0., ..settings() };
    let cuboid = DynamicBox {
        center_m: [0.; 3],
        half_extents_m: [1., 0.4, 0.8],
        rotation_xyzw: [0., 0., 0., 1.],
        velocity_m_s: [0.; 3],
        mass_kg: 567.,
        friction: 0.5,
    };
    for engine in engines() {
        for (half_segment, restitution) in [(0., 0.), (0.60, 0.9)] {
            let extent = half_segment + cfg.capsule.radius;
            let mut shape = Part::between(
                "test part",
                None,
                [[0., -extent, 0.], [0., extent, 0.]],
                cfg.capsule.radius,
                cfg.capsule.mass,
                [[0.; 2]; 3],
            );
            shape.material = Some(osage_objects::CollisionMaterial { friction: 0.5, restitution });
            let center = [0., cuboid.half_extents_m[1] + extent + 0.6, 0.];
            let mut sim = Simulation::from_parts(engine, &[], &[cuboid], cfg, center, [0., -8., 0.], &[shape]).unwrap();
            let mut swept = false;
            let mut touched = false;
            let mut peak = f32::NEG_INFINITY;
            for _ in 0..15 {
                sim.step().unwrap();
                swept |= sim.swept_impact_offset_s.is_some();
                touched |= !sim.contacts.is_empty();
                peak = peak.max(sim.pose.linear_velocity[1] - sim.box_poses[0].linear_velocity[1]);
            }
            assert!(touched, "{engine} missed the box");
            assert_eq!(swept, half_segment > 0.);
            if restitution > 0. {
                assert!(peak > 6., "{engine} suppressed the elastic response ({peak} m/s)");
            }
        }
    }
}

#[test]
fn a_low_frequency_actor_clock_keeps_the_same_articulated_impact() {
    use osage_dynamics::{DynamicBox, Part};
    let rig = [
        Part::between("trunk", None, [[0., 0.9, 0.], [0., 1.5, 0.]], 0.13, 12., [[0.; 2]; 3]),
        Part::between("limb", Some(0), [[0., 0.9, 0.], [0.2, 0.2, 0.]], 0.10, 3., [[0.; 2], [0.; 2], [-75., 75.]]),
    ];
    let platform = DynamicBox {
        center_m: [0., 0.4, 0.],
        half_extents_m: [1.3, 0.4, 0.9],
        rotation_xyzw: [0., 0., 0., 1.],
        velocity_m_s: [0.8, 0., 0.],
        mass_kg: 150.,
        friction: 0.5,
    };
    for engine in engines() {
        let floor = [sheet("support", 0., 0., 20.)];
        let make = |hz| {
            Simulation::from_parts(
                engine,
                &floor,
                &[platform],
                Settings { tick_hz: hz, ..settings() },
                [0., 1.6, 0.],
                [0.4, -2., 0.],
                &rig,
            )
            .unwrap()
        };
        let mut low = make(20);
        let mut high = make(60);
        assert_eq!(low.substeps_per_tick(), 12);
        assert_eq!(high.substeps_per_tick(), 4);
        let mut contacted_box = false;
        for _ in 0..40 {
            low.step().unwrap();
            let mut expected_contacts = Vec::new();
            for offset in [0, 4, 8] {
                high.step().unwrap();
                expected_contacts.extend(
                    high.contacts.iter().map(|c| osage_dynamics::Contact { substep: c.substep + offset, ..*c }),
                );
            }
            assert_eq!(low.part_poses, high.part_poses, "{engine} limb state");
            assert_eq!(low.box_poses, high.box_poses, "{engine} platform recoil");
            assert_eq!(low.contacts, expected_contacts, "{engine} contact order");
            contacted_box |= low.contacts.iter().any(|c| c.surface == 1);
        }
        assert!(contacted_box, "{engine} missed the platform");
    }
}

#[test]
fn a_loaded_foot_avoids_sustained_deep_overlap_with_a_box_supported_by_terrain() {
    use glam::{Quat, Vec3};
    use osage_dynamics::{DynamicBox, Part};
    let body = settings().capsule.part();
    let rig = [
        Part {
            name: "trunk".into(),
            center: Vec3::Y * 0.65,
            radius: 0.15,
            half_segment: 0.21,
            mass: 50.,
            ..body.clone()
        },
        Part {
            name: "foot".into(),
            parent: Some(0),
            center: Vec3::Y * 0.05,
            anchor: Vec3::Y * 0.4,
            radius: 0.05,
            half_segment: 0.,
            mass: 2.,
            ..body
        },
    ];
    let floor = [sheet("road", 8., 8., 20.)];
    for speed in [0., 2.8] {
        let platform = DynamicBox {
            center_m: [-0.45, 8.4, 6.2],
            half_extents_m: [1.05, 0.4, 0.82],
            rotation_xyzw: Quat::IDENTITY.to_array(),
            velocity_m_s: [speed, 0., 0.],
            mass_kg: 1250.,
            friction: 0.5,
        };
        let mut sim = Simulation::from_parts(
            Engine::Rapier,
            &floor,
            &[platform],
            settings(),
            [0., 10.3, 6.85],
            [0.5, -2., 0.],
            &rig,
        )
        .unwrap();
        let mut touched = false;
        let mut previously_deep = false;
        for _ in 0..60 {
            sim.step().unwrap();
            touched |= sim.contacts.iter().any(|c| c.part == 1 && c.surface == 1);
            let box_pose = sim.box_poses[0];
            let local = Quat::from_array(box_pose.rotation_xyzw).inverse()
                * (Vec3::from(sim.part_poses[1].center) - Vec3::from(box_pose.center));
            let [x, y, z] = platform.half_extents_m;
            let depth = y + rig[1].radius - local.y;
            let deep = local.x.abs() <= x && local.z.abs() <= z && local.y >= 0. && depth > 0.02;
            // There are four native substeps between the poses exposed at 60 Hz.
            assert!(!(deep && previously_deep), "box speed {speed} m/s, sustained foot overlap {depth} m");
            previously_deep = deep;
        }
        assert!(touched);
    }
}

#[test]
fn an_initially_translating_rig_hits_a_free_box_at_the_geometric_time() {
    use osage_dynamics::{DynamicBox, Part};

    let spheres = [
        Part::between("left", None, [[-0.2, 0., 0.], [-0.2, 0.2, 0.]], 0.1, 2., [[0.; 2]; 3]),
        Part::between("right", Some(0), [[0.2, 0., 0.], [0.2, 0.2, 0.]], 0.1, 2., [[0.; 2]; 3]),
    ];
    let speed = 6.0;
    let hit_time = 3.5 / 240.0;
    let platform = DynamicBox {
        center_m: [0., -0.4 - speed * hit_time, 0.],
        half_extents_m: [1., 0.4, 1.],
        rotation_xyzw: [0., 0., 0., 1.],
        velocity_m_s: [3., 0., 0.],
        mass_kg: 300.,
        friction: 0.,
    };
    for engine in engines() {
        let mut sim =
            Simulation::from_parts(engine, &[], &[platform], settings(), [0.; 3], [3., -speed, 0.], &spheres).unwrap();
        let mut observations = Vec::new();
        sim.step_with_substeps(|state| {
            assert!(state.contacts.iter().all(|c| c.substep == state.substep));
            observations.push((state.substep, state.parts.to_vec(), state.boxes.to_vec(), state.contacts.len()));
        })
        .unwrap();
        assert_eq!(observations.iter().map(|s| s.0).collect::<Vec<_>>(), [1, 2, 3, 4]);
        let (_, limbs, boxes, _) = observations.last().unwrap();
        assert_eq!(*limbs, sim.part_poses);
        assert_eq!(*boxes, sim.box_poses);
        assert_eq!(observations.iter().map(|s| s.3).sum::<usize>(), sim.contacts.len());
        let impact = sim.swept_impact_offset_s.unwrap();
        assert!((impact - hit_time).abs() < 1e-6, "{engine}: {impact} s");
        assert!(sim.contacts.iter().any(|c| c.part == 0));
        assert!(sim.contacts.iter().any(|c| c.part == 1));
        let impulse = -platform.mass_kg * (sim.box_poses[0].linear_velocity[1] + settings().gravity / 60.);
        let expected = speed / (1. / 4. + 1. / platform.mass_kg);
        assert!((impulse - expected).abs() < expected * 0.03, "{engine}: {impulse} vs {expected}");

        let floor = sheet("third collider", -0.92, -0.92, 20.);
        let mut near_floor =
            Simulation::from_parts(engine, &[floor], &[platform], settings(), [0.; 3], [3., -speed, 0.], &spheres)
                .unwrap();
        near_floor.step().unwrap();
        assert!(near_floor.swept_impact_offset_s.is_none());
        assert!(near_floor.contacts.iter().any(|c| c.surface == 1));
    }
}

#[test]
fn ragdoll_activation_preserves_rotating_joint_velocity_and_angular_limits() {
    use glam::{Quat, Vec3};
    use osage_dynamics::{AnimatedBody, AnimationHandoff, BodyTransform, Part, RigStart};
    let rig = [
        Part::between("parent", None, [[-1., 0., 0.], [0., 0., 0.]], 0.1, 2., [[0.; 2]; 3]),
        Part::between("child", Some(0), [[0., 0., 0.], [1., 0., 0.]], 0.1, 1., [[0.; 2], [0.; 2], [-70., 70.]]),
    ];
    let dt = 1. / 30.;
    let angle = 0.45;
    let speed = 3.;
    let transform = |p: &Part, angle: f32| {
        let rotation = Quat::from_rotation_z(angle);
        BodyTransform { center_m: (rotation * p.center).into(), rotation_xyzw: (rotation * p.rotation).into() }
    };
    let parent = transform(&rig[0], 0.);
    let current = transform(&rig[1], angle);
    let mut previous = transform(&rig[1], angle - speed * dt);
    // Equivalent opposite quaternion signs must give the same shortest-arc velocity.
    previous.rotation_xyzw = previous.rotation_xyzw.map(|v| -v);
    let animation = AnimationHandoff {
        sample_interval_s: dt,
        parts: vec![
            AnimatedBody { name: "child".into(), previous, current },
            AnimatedBody { name: "parent".into(), previous: parent, current: parent },
        ],
    };
    let origin = [3., 5., 0.];
    let velocity = [1.2, -0.1, 0.];
    let turn = Vec3::Z * speed;
    let expected = Vec3::from_array(velocity) + turn.cross(current.center_m.into());
    for engine in engines() {
        let mut sim = Simulation::from_rig(
            engine,
            &[],
            &[],
            Settings { gravity: 0., ..settings() },
            &rig,
            RigStart { origin_m: origin, velocity_m_s: velocity, animation: Some(&animation) },
        )
        .unwrap();
        let before = sim.part_poses[1];
        assert!(Vec3::from(before.linear_velocity).distance(expected) < 5e-6);
        assert!(Vec3::from(before.angular_velocity).distance(turn) < 5e-6);
        assert!(Vec3::from(before.center).distance(Vec3::from(origin) + Vec3::from(current.center_m)) < 1e-6);
        let momentum = Vec3::from(velocity) * 2. + expected;
        sim.step().unwrap();
        let p = Vec3::from(sim.part_poses[0].linear_velocity) * 2. + Vec3::from(sim.part_poses[1].linear_velocity);
        assert!(p.distance(momentum) < 1e-4, "{engine}: {p:?} vs {momentum:?}");
        let parent_spin = Quat::from_array(sim.part_poses[0].rotation_xyzw) * rig[0].rotation.inverse();
        let child_spin = Quat::from_array(sim.part_poses[1].rotation_xyzw) * rig[1].rotation.inverse();
        let final_angle = (parent_spin.inverse() * child_spin).to_scaled_axis().z;
        assert!(final_angle > angle && final_angle < angle + 0.1, "{engine}: {final_angle}");
    }
}

#[test]
fn removing_the_page_under_a_supported_box_releases_it_before_the_next_contact() {
    use osage_dynamics::{DynamicBox, Part, RigStart, streamed::TerrainSimulation};
    use rapier3d::prelude::{ColliderBuilder, Vector};

    let cfg = settings();
    let child = Part::between("child", Some(0), [[0., 0.7, 0.], [0., 1.1, 0.]], 0.1, 1., [[0.; 2]; 3]);
    let parts = [cfg.capsule.part(), child];
    let plate = DynamicBox {
        center_m: [0., 0.4, 0.],
        half_extents_m: [1., 0.4, 1.],
        rotation_xyzw: [0., 0., 0., 1.],
        velocity_m_s: [0.; 3],
        mass_kg: 50.,
        friction: 0.5,
    };
    let start = RigStart { origin_m: [0., 3., 0.], velocity_m_s: [0.; 3], animation: None };
    let mut world = TerrainSimulation::with_boxes(cfg, &parts, start, &[plate]).unwrap();
    let floor = world.insert(
        ColliderBuilder::cuboid(8., 0.1, 8.).translation(Vector::new(0., -0.1, 0.)),
        None,
        "ground",
        [0, 0],
    );
    for _ in 0..8 {
        world.step(None).unwrap();
    }
    let supported_y = world.state().box_poses[0].center[1];
    assert!((supported_y - plate.center_m[1]).abs() < 0.005);
    world.remove(floor);
    for _ in 0..12 {
        world.step(None).unwrap();
    }
    let unsupported = world.state().box_poses[0];
    assert!(unsupported.center[1] < supported_y - 0.15 && unsupported.linear_velocity[1] < -1.);
}
