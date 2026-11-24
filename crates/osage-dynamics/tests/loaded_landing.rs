// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

use glam::{Quat, Vec3};
use osage_dynamics::{Capsule, Engine, Lift, Part, Settings, Simulation};
use osage_objects::{CollisionMaterial, SwingEnvelope};
use serde::Deserialize;

// Bind frames and mass properties of Osage's authored mechanical quadruped.
#[derive(Deserialize)]
struct Limb {
    name: String,
    parent: Option<usize>,
    center_m: [f32; 3],
    rotation_xyzw: [f32; 4],
    anchor_m: [f32; 3],
    joint_frame_xyzw: [f32; 4],
    radius_m: f32,
    half_segment_m: f32,
    mass_kg: f32,
    inertia_kg_m2: [f32; 3],
}

fn quadruped() -> Vec<Part> {
    serde_json::from_str::<Vec<Limb>>(include_str!("fixtures/wide-quadruped.json"))
        .unwrap()
        .into_iter()
        .map(|b| Part {
            name: b.name,
            parent: b.parent,
            center: b.center_m.into(),
            rotation: Quat::from_array(b.rotation_xyzw),
            radius: b.radius_m,
            half_segment: b.half_segment_m,
            mass: b.mass_kg,
            inertia: Some(b.inertia_kg_m2.into()),
            material: Some(CollisionMaterial { friction: 0.5, restitution: 0. }),
            joint_rotation: Quat::from_array(b.joint_frame_xyzw),
            anchor: b.anchor_m.into(),
            swing_envelope: if b.parent.is_some() { SwingEnvelope::AngularEllipse } else { SwingEnvelope::Independent },
            limits_deg: if b.parent.is_some() { [[-30., 30.]; 3] } else { [[0.; 2]; 3] },
        })
        .collect()
}

#[test]
fn both_angular_stops_hold_during_a_frictionless_landing() -> anyhow::Result<()> {
    let parts = quadruped();
    let settings = Settings {
        tick_hz: 60,
        gravity: 9.81,
        terminal_speed: 55.,
        capsule: Capsule { radius: 0.28, half_segment: 0.60, mass: 75., friction: 0.5 },
    };
    let deck =
        Lift { center_m: [32., 80., 8.], half_extents_m: [1., 0.125, 1.], travel_m: 0.5, period_s: 4., friction: 0. };
    for hz in [480, 960] {
        let mut sim = Simulation::from_parts(Engine::Rapier, &[], &[], settings, [32., 80.8, 8.], [0.; 3], &parts)?;
        sim.add_lift(deck)?;
        sim.plan_lift_approach(&parts, 2., hz)?;
        sim.sample_lift_response(&parts, 0.001)?;
        let mut peak_error_deg = 0_f32;
        let mut penetration_m = 0_f32;
        let mut first_touch = None;
        for tick in 1..=36 {
            sim.step()?;
            if sim.contacts.iter().any(|c| c.separation <= 0.) {
                first_touch.get_or_insert(tick);
            }
            if tick < 22 {
                continue;
            }
            let box_center = Vec3::from(sim.box_poses[0].center);
            let delta: Vec<_> = sim
                .part_poses
                .iter()
                .zip(&parts)
                .map(|(pose, p)| Quat::from_array(pose.rotation_xyzw) * p.rotation.conjugate())
                .collect();
            for (i, p) in parts.iter().enumerate() {
                let pose = sim.part_poses[i];
                let arm = Quat::from_array(pose.rotation_xyzw) * Vec3::Y * p.half_segment;
                let center = Vec3::from(pose.center) - box_center;
                // All tested spines project within the finite deck throughout this window.
                assert!(
                    center.x.abs() + arm.x.abs() < deck.half_extents_m[0]
                        && center.z.abs() + arm.z.abs() < deck.half_extents_m[2]
                );
                penetration_m = penetration_m.max(deck.half_extents_m[1] - center.y + arm.y.abs() + p.radius);
                if let Some(parent) = p.parent {
                    let q = (p.joint_rotation.conjugate() * delta[parent].conjugate() * delta[i] * p.joint_rotation)
                        .normalize();
                    let twist = 2. * q.x.abs().atan2(q.w.abs());
                    let tilt = 2. * q.y.hypot(q.z).atan2(q.w.hypot(q.x));
                    peak_error_deg = peak_error_deg.max(twist.max(tilt).to_degrees() - 30.);
                }
            }
        }
        assert_eq!(first_touch, Some(22));
        assert!(
            penetration_m < 0.005 && peak_error_deg < 3.1,
            "at {hz} Hz: {penetration_m} m, {peak_error_deg} degrees"
        );
    }
    Ok(())
}
