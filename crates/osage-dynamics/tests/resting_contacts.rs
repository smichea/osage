// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

use osage_dynamics::{Capsule, Part, RigStart, Settings, streamed::TerrainSimulation};
use rapier3d::prelude::{ColliderBuilder, Vector};

fn floor_at(height: f32) -> ColliderBuilder {
    let vertices = [[-4., height, -4.], [-4., height, 4.], [4., height, 4.], [4., height, -4.]];
    ColliderBuilder::trimesh(vertices.map(Vector::from_array).to_vec(), vec![[0, 1, 2], [0, 2, 3]]).unwrap()
}

#[test]
fn a_resting_rig_wakes_when_its_support_slot_is_reused_elsewhere() {
    let settings = Settings {
        tick_hz: 60,
        gravity: 9.81,
        terminal_speed: 55.,
        capsule: Capsule { radius: 0.1, half_segment: 0.12, mass: 3., friction: 0.55 },
    };
    let parts = [
        Part::between("left", None, [[-0.48, 0., 0.], [0.; 3]], 0.1, 3., [[0.; 2]; 3]),
        Part::between("right", Some(0), [[0.; 3], [0.48, 0., 0.]], 0.1, 3., [[0.; 2]; 3]),
    ];
    let start = RigStart { origin_m: [0.25, 2.65, 0.45], velocity_m_s: [0.; 3], animation: None };
    let mut world = TerrainSimulation::new(settings, &parts, start).unwrap();
    let balcony = world.insert(floor_at(1.5), Some(61), "balcony", [0, 0]);
    let road = world.insert(floor_at(0.), Some(44), "road", [0, 0]);
    for _ in 0..180 {
        world.step(None).unwrap();
    }
    let still = world.state().part_poses.clone();
    let mut impulses = Vec::new();
    for tick in 0..24 {
        world.step((tick % 2 == 0).then_some(&mut impulses)).unwrap();
        let state = world.state();
        assert_eq!(state.part_poses, still);
        assert!(impulses.is_empty());
        assert!(state.contacts.iter().all(|c| c.surface == balcony.into_raw_parts().0 as usize));
        for substep in 1..=state.substeps_per_tick() {
            assert!(state.contacts.iter().any(|c| c.substep == substep));
        }
    }

    world.remove(balcony);
    let distant = world.insert(floor_at(1.5).translation(Vector::new(32., 0., 0.)), Some(72), "remote", [4, 0]);
    assert_eq!(distant.into_raw_parts().0, balcony.into_raw_parts().0);
    assert_ne!(distant, balcony);
    world.step(Some(&mut impulses)).unwrap();
    assert!(world.state().contacts.is_empty() && impulses.is_empty());
    for (old, falling) in still.iter().zip(&world.state().part_poses) {
        assert!(falling.center[1] < old.center[1] && falling.linear_velocity[1] < 0.);
    }

    let road_slot = road.into_raw_parts().0 as usize;
    let mut loaded = [false; 2];
    for _ in 0..120 {
        world.step(Some(&mut impulses)).unwrap();
        for c in &world.state().contacts {
            assert_eq!(c.surface, road_slot);
            assert!(matches!(c.surface_triangle, Some(0 | 1)));
        }
        for impulse in &impulses {
            let pair = impulse.pair().unwrap();
            assert_eq!(pair.surface, road_slot);
            loaded[pair.part] |= impulse.normal_impulse_ns().unwrap() > 0.;
        }
    }
    assert_eq!(loaded, [true; 2]);
    assert!(world.state().part_poses.iter().all(|p| (p.center[1] - 0.1).abs() < 0.01 && p.linear_velocity == [0.; 3]));
}
