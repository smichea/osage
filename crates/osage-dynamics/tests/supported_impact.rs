// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

use glam::{Quat, Vec3};
use osage_dynamics::{
    AnimatedBody, AnimationHandoff, BodyTransform, Capsule, DynamicBox, Engine, Part, Pose, RigStart, Settings,
    Simulation, StaticMesh,
};
use osage_objects::CollisionMaterial;

#[derive(Clone, Copy)]
struct Witness {
    gap: f32,
    normal_speed: f32,
}

fn surface_witness(link: &Part, limb: Pose, plate: Pose, half: Vec3) -> Witness {
    let n = Quat::from_array(plate.rotation_xyzw) * Vec3::Y;
    let axis = Quat::from_array(limb.rotation_xyzw) * Vec3::Y;
    let end = -axis.dot(n).signum() * link.half_segment * axis;
    let foot_arm = end - link.radius * n;
    let foot = Vec3::from(limb.center) + foot_arm;
    let plate_arm = foot - Vec3::from(plate.center);
    let local = Quat::from_array(plate.rotation_xyzw).inverse() * plate_arm;
    assert!(local.x.abs() < half.x && local.z.abs() < half.z, "the link left the top face");
    let gap = local.y - half.y;
    let foot_v = Vec3::from(limb.linear_velocity) + Vec3::from(limb.angular_velocity).cross(foot_arm);
    let plate_v = Vec3::from(plate.linear_velocity) + Vec3::from(plate.angular_velocity).cross(plate_arm - gap * n);
    Witness { gap, normal_speed: (foot_v - plate_v).dot(n) }
}

fn released_rig(phase: usize, restitution: f32, spin: Option<f32>) -> (Simulation, [Part; 2], DynamicBox) {
    let smooth = CollisionMaterial { friction: 0., restitution: 0. };
    let floor = StaticMesh {
        id: "floor".into(),
        material: Some(smooth),
        vertices: vec![[-5., 0., -5.], [-5., 0., 5.], [5., 0., 5.], [5., 0., -5.]],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
    };
    let plate = DynamicBox {
        center_m: [0., 0.25, 0.],
        half_extents_m: [1.1, 0.25, 0.9],
        rotation_xyzw: Quat::IDENTITY.to_array(),
        velocity_m_s: [0.3, 0., 0.],
        mass_kg: 850.,
        friction: 0.,
    };
    let pivot = [0., 0.28, -0.04];
    let mut rig = [
        Part::between("upper", None, [pivot, [0., 0.86, -0.08]], 0.095, 9., [[0.; 2]; 3]),
        Part::between("lower", Some(0), [pivot, [0., 0.05, 0.28]], 0.06, 1.25, [[-65., 65.], [0.; 2], [0.; 2]]),
    ];
    rig[1].material = Some(CollisionMaterial { restitution, ..smooth });
    let settings = Settings {
        tick_hz: 60,
        gravity: 9.81,
        terminal_speed: 55.,
        capsule: Capsule { radius: 0.12, half_segment: 0.1, mass: 1., friction: 0. },
    };
    // Sample roughly one native interval of travel with eight release heights, keeping the geometry fixed.
    let start = [0.14, 1.25 + 0.004 * phase as f32, 0.18];
    let animation = spin.map(|rate| {
        let dt = 1. / 120.;
        let previous = Quat::from_rotation_x(-rate * dt);
        let center = (rig[0].center * rig[0].mass + rig[1].center * rig[1].mass) / (rig[0].mass + rig[1].mass);
        AnimationHandoff {
            sample_interval_s: dt,
            parts: rig
                .iter()
                .map(|p| AnimatedBody {
                    name: p.name.clone(),
                    current: BodyTransform { center_m: p.center.into(), rotation_xyzw: p.rotation.into() },
                    previous: BodyTransform {
                        center_m: (center + previous * (p.center - center)).into(),
                        rotation_xyzw: (previous * p.rotation).into(),
                    },
                })
                .collect(),
        }
    });
    let motion = RigStart { origin_m: start, velocity_m_s: [0.42, -6., 0.12], animation: animation.as_ref() };
    let sim = Simulation::from_rig(Engine::Rapier, &[floor], &[plate], settings, &rig, motion).unwrap();
    assert_eq!(sim.substeps_per_tick(), 4);
    (sim, rig, plate)
}

fn impact(phase: usize, restitution: f32) -> (Witness, Vec<Witness>) {
    let (mut sim, rig, plate) = released_rig(phase, restitution, None);
    let observe = |p, b| surface_witness(&rig[1], p, b, plate.half_extents_m.into());
    let mut previous = observe(sim.part_poses[1], sim.box_poses[0]);
    let mut incoming = None;
    let mut afterwards = Vec::new();
    for _ in 0..12 {
        sim.step_with_substeps(|state| {
            if afterwards.len() >= 7 {
                return;
            }
            let b = state.boxes[0];
            assert!((b.center[1] - plate.center_m[1]).abs() < 0.002, "the moving plate lost its floor support");
            let now = observe(state.parts[1], b);
            for c in state.contacts {
                assert_eq!((c.part, c.surface), (1, 1), "another body touched during the measurement");
                assert!(c.normal[1] > 0.99, "expected an impact on the top face");
                incoming.get_or_insert(previous);
            }
            if incoming.is_some() {
                afterwards.push(now);
            }
            previous = now;
        })
        .unwrap();
        if afterwards.len() == 7 {
            break;
        }
    }
    assert_eq!(afterwards.len(), 7, "missing contact or its 25 ms continuation");
    (incoming.unwrap(), afterwards)
}

#[test]
fn inelastic_leading_link_loads_the_supported_plate_at_first_crossing() {
    for phase in 0..8 {
        let (before, after) = impact(phase, 0.);
        let travel_per_step = -before.normal_speed / 240.;
        assert!(before.normal_speed < -6., "the link was not approaching");
        // The contact interval straddles the face to within 10% of one step of incoming travel.
        assert!(
            before.gap > -0.1 * travel_per_step && before.gap < 1.1 * travel_per_step,
            "phase {phase}: {} m before contact",
            before.gap
        );
        assert!(
            after[0].normal_speed.abs() < 0.1 * -before.normal_speed,
            "phase {phase}: {} m/s after contact",
            after[0].normal_speed
        );
        assert!(
            after.iter().all(|p| p.gap >= -0.1 * travel_per_step),
            "phase {phase}: unresolved overlap after impact"
        );
    }
}

#[test]
fn the_same_oblique_contact_preserves_authored_restitution() {
    for phase in 0..8 {
        let (before, after) = impact(phase, 0.8);
        assert!(before.normal_speed < -6.);
        assert!(
            after[0].normal_speed > 0.6 * -before.normal_speed,
            "phase {phase}: {} m/s after an elastic impact",
            after[0].normal_speed
        );
        assert!(after.iter().any(|s| s.gap > 0.03), "the rebounding link did not clear the top face");
    }
}

#[test]
fn a_second_fall_after_both_links_clear_the_plate_reports_the_new_crossing() {
    let (mut sim, rig, plate) = released_rig(0, 0., Some(-3.));
    let observe = |parts: &[Pose], box_pose| {
        std::array::from_fn::<_, 2, _>(|i| surface_witness(&rig[i], parts[i], box_pose, plate.half_extents_m.into()))
    };
    let mut previous = observe(&sim.part_poses, sim.box_poses[0]);
    let mut first_part = None;
    let mut peak_clearance = 0_f32;
    let mut second = None;
    for _ in 0..16 {
        sim.step_with_substeps(|state| {
            if second.is_some() {
                return;
            }
            let now = observe(state.parts, state.boxes[0]);
            if let Some(contact) = state.contacts.first() {
                assert_eq!(contact.surface, 1);
                if peak_clearance > 0.05 {
                    second = Some((contact.part, previous[contact.part], now[contact.part]));
                }
                first_part.get_or_insert(contact.part);
                peak_clearance = 0.;
            } else if first_part.is_some() {
                peak_clearance = peak_clearance.max(now[0].gap.min(now[1].gap));
            }
            previous = now;
        })
        .unwrap();
        if second.is_some() {
            break;
        }
    }
    assert_eq!(first_part, Some(1), "expected the lower link to strike first");
    let (part, before, after) = second.expect("both links must leave the plate and fall a second time");
    assert_eq!(part, 0, "the rig turns in flight and lands on the upper link");
    assert!(before.normal_speed < -3.);
    let gap_budget = -0.1 * before.normal_speed / 240.;
    assert!(
        before.gap > -gap_budget && after.gap > -gap_budget,
        "late second contact: {} to {} m",
        before.gap,
        after.gap
    );
}
