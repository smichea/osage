// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use osage_sdk::{World, components, types as core, v0::Role};
use osage_server::world::{
    Harness, SessionKind,
    bevy::math::{Vec2, Vec3, Vec3Swizzles},
};
use osage_world_paris::{
    ParisWorld, TICK_HZ, Walkers, proto, types,
    walk::{RADIUS, Walk},
};
use std::{collections::BTreeMap, sync::Arc};

fn street(width: usize) -> (Harness, Walkers) {
    let length = width as f32 * 0.5;
    let walk = Walk::new(0.5, width, width, Vec2::splat(-length / 2.), vec![0.; width * width], Vec3::ZERO);
    server(walk)
}

fn server(walk: Walk) -> (Harness, Walkers) {
    let walkers = Walkers::default();
    let module = ParisWorld { walk: Arc::new(walk), walkers: walkers.clone(), light_port: 0, horizon: 1. };
    (Harness::new(module, TICK_HZ), walkers)
}

fn residents(client: &World) -> BTreeMap<u64, (bool, Vec3)> {
    client
        .entities()
        .filter_map(|id| {
            let walker: proto::Walker = client.get(id, types::WALKER)?;
            let at = client.get::<components::Transform>(id, core::TRANSFORM).unwrap().translation.unwrap();
            let owner = client.get::<components::Controller>(id, core::CONTROLLER);
            assert_eq!(walker.passer_by, owner.is_none());
            Some((id, (walker.passer_by, Vec3::new(at.x, at.y, at.z))))
        })
        .collect()
}

#[test]
fn group_arrives_among_moving_residents_and_late_observer_receives_the_same_street() {
    group_arrival(street(40));
}

#[test]
#[ignore = "requires the installed Bistro walk grid"]
fn group_arrives_in_the_installed_bistro() {
    let dir = std::path::PathBuf::from(std::env::var_os("OSAGE_PARIS_WALK_DIR").unwrap());
    let walk = Walk::parse(
        &std::fs::read_to_string(dir.join("walk.json")).unwrap(),
        &std::fs::read(dir.join("walk.bin")).unwrap(),
    )
    .unwrap();
    group_arrival(server(walk));
}

fn group_arrival((mut h, lighting): (Harness, Walkers)) {
    let mut first = World::new(&h.welcome());
    let (_, mut messages) = h.join(SessionKind::Human, Role::Observer, "first");
    h.run(30);
    while let Ok(m) = messages.try_recv() {
        first.apply(&m);
    }
    let initial = residents(&first);
    assert_eq!(initial.len(), 2);
    assert!(initial.values().all(|(passer, _)| *passer));

    for i in 0..32 {
        h.join(SessionKind::Human, Role::Player, &format!("arrival-{i}"));
    }
    let (_, mut snapshot) = h.join(SessionKind::Human, Role::Observer, "late");
    let mut late = World::new(&h.welcome());
    h.run(1);
    while let Ok(m) = messages.try_recv() {
        first.apply(&m);
    }
    while let Ok(m) = snapshot.try_recv() {
        late.apply(&m);
    }
    let scene = residents(&late);
    assert_eq!(scene, residents(&first));
    assert_eq!(scene.len(), 34);
    let mut closest = f32::INFINITY;
    for (i, (_, a)) in scene.values().enumerate() {
        for (_, b) in scene.values().skip(i + 1) {
            let horizontal = a.xz().distance(b.xz());
            closest = closest.min(horizontal);
            assert!(horizontal >= 2. * RADIUS - 1e-6, "overlap at {a} and {b}");
        }
    }
    println!("arrivals=32 residents=2 minimum_centres_m={closest:.6}");
    assert_eq!(lighting.0.lock().unwrap().len(), 32);

    h.run(30);
    while let Ok(m) = snapshot.try_recv() {
        late.apply(&m);
    }
    let after = residents(&late);
    for (id, (_, from)) in initial {
        assert!(after[&id].0 && after[&id].1.distance(from) > 0.1);
    }
}

#[test]
fn full_entrance_waits_for_space_and_cancels_departures() {
    let (mut h, _) = street(1);
    let mut observer = World::new(&h.welcome());
    let (_, mut feed) = h.join(SessionKind::Human, Role::Observer, "observer");
    let (first, _) = h.join(SessionKind::Human, Role::Player, "first");
    let (leaving, _) = h.join(SessionKind::Human, Role::Player, "leaving");
    h.join(SessionKind::Human, Role::Player, "remaining");
    h.run(2);
    while let Ok(m) = feed.try_recv() {
        observer.apply(&m);
    }
    assert_eq!(residents(&observer).len(), 1);
    assert!(observer.controlled_by("first").is_some());
    h.leave(leaving);
    h.leave(first);
    let (immediate, _) = h.join(SessionKind::Human, Role::Player, "immediate");
    h.leave(immediate);
    h.run(3);
    while let Ok(m) = feed.try_recv() {
        observer.apply(&m);
    }
    assert_eq!(residents(&observer).len(), 1);
    assert!(observer.controlled_by("remaining").is_some());
}
