// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Sound from real objects: faces made of triangles, one surface however
//! many faces it is cut into, and contacts as the physics solver
//! (`osage-dynamics`) reports them — where, how hard and when.

use osage_dynamics::{Capsule, Settings, Simulation, StaticMesh};
use osage_sound::{
    ears::Listening,
    engine::{BLOCK, Engine, Options, Quality, RATE, Struck, strength},
    render::render,
    scene::Scene,
    space::vec3,
};

fn rms(samples: &[f32]) -> f64 {
    (samples.iter().map(|&s| f64::from(s) * f64::from(s)).sum::<f64>() / samples.len().max(1) as f64).sqrt()
}

fn part(samples: &[f32], from: f64, to: f64) -> &[f32] {
    &samples[(from * RATE as f64) as usize..(to * RATE as f64) as usize]
}

/// A scene: ears at the origin looking along −Z, a noise 8 m ahead behind
/// a wall 6 m ahead (8 m wide, 4 m high, standing on the ground), a hard
/// ground; the wall cut into `cut` × `cut` cells, each two triangles when
/// `triangles`.
fn walled(cut: usize, triangles: bool) -> Scene {
    let mut faces = Vec::new();
    for i in 0..cut {
        for j in 0..cut {
            let x = |k: usize| -4.0 + 8.0 * k as f64 / cut as f64;
            let y = |k: usize| -1.6 + 4.0 * k as f64 / cut as f64;
            let (a, b, c, d) = ([x(i), y(j)], [x(i + 1), y(j)], [x(i + 1), y(j + 1)], [x(i), y(j + 1)]);
            let corner = |p: [f64; 2]| format!("[{}, {}, -6]", p[0], p[1]);
            let quads = if triangles { vec![[a, b, c, c], [a, c, d, d]] } else { vec![[a, b, c, d]] };
            for (n, q) in quads.iter().enumerate() {
                faces.push(format!(
                    r#"{{"id": "wall-{i}-{j}-{n}", "quadM": [{}], "acoustic": "brick"}}"#,
                    q.iter().map(|&p| corner(p)).collect::<Vec<_>>().join(", ")
                ));
            }
        }
    }
    faces.push(r#"{"id": "ground", "quadM": [[-40, -1.6, -40], [-40, -1.6, 40], [40, -1.6, 40], [40, -1.6, -40]], "acoustic": "brick"}"#.to_owned());
    let text = format!(
        r#"{{
      "asset": {{"version": "2.0", "extras": {{
        "osageObject": {{"version": 1, "id": "walled", "collisionMaterials": {{}},
          "acousticMaterials": {{"brick": {{"absorption": [0.02, 0.03, 0.04], "transmission": [0, 0, 0], "scattering": 0.3}}}}}},
        "osageSound": {{"version": 0, "contactSounds": {{}}}}}}}},
      "scene": 0,
      "scenes": [{{"nodes": [0], "extras": {{"osageSound": {{"seconds": 2, "seed": 3,
        "listener": {{"node": ["world", "ears"]}}, "reverb": {{"secondsT60": [0.8, 0.7, 0.5]}},
        "air": {{"celsius": 20, "humidityPercent": 50}}}}}}}}],
      "nodes": [
        {{"children": [1, 2], "extras": {{"osage": {{"id": "world"}}, "osageSound": {{"surfaces": [{}]}}}}}},
        {{"extras": {{"osage": {{"id": "ears"}}}}}},
        {{"translation": [0.4, 0, -8], "extras": {{"osage": {{"id": "source"}}, "osageSound": {{
          "emitters": [{{"id": "noise", "voice": "noise"}}]}}}}}}
      ]
    }}"#,
        faces.join(", ")
    );
    Scene::from_json(&text).unwrap()
}

#[test]
fn a_wall_cut_into_triangles_sounds_as_the_whole_wall() {
    let options = Options { quality: Quality::HIGH, listening: Listening::Speakers, ..Options::default() };
    let whole = render(&walled(1, false), &options, 2.0);
    for (cut, triangles) in [(1, true), (3, false), (4, true)] {
        let cut_up = render(&walled(cut, triangles), &options, 2.0);
        for (one, other) in [(&whole.left, &cut_up.left), (&whole.right, &cut_up.right)] {
            let (a, b) = (rms(part(one, 0.5, 2.0)), rms(part(other, 0.5, 2.0)));
            let db = 20.0 * (b / a).log10();
            eprintln!("{cut}×{cut}, triangles {triangles}: {db:+.2} dB");
            assert!(db.abs() < 0.3, "{cut}×{cut}, triangles {triangles}: {db:+.2} dB");
        }
    }
    // And the wall hides: without it the noise is louder by far.
    let open = {
        let mut scene = walled(1, false);
        scene.surfaces.retain(|s| s.id == "ground");
        render(&scene, &options, 2.0)
    };
    let db = 20.0 * (rms(part(&open.left, 0.5, 2.0)) / rms(part(&whole.left, 0.5, 2.0))).log10();
    eprintln!("the wall takes {db:.1} dB");
    assert!(db > 6.0, "the wall takes {db:.1} dB");
}

/// A scene with a stone ground, ears 1.6 m above it at the origin.
const GROUND: &str = r#"{
  "asset": {"version": "2.0", "extras": {
    "osageObject": {"version": 1, "id": "yard",
      "collisionMaterials": {"stone": {"friction": 0.7, "restitution": 0}, "boot": {"friction": 0.9, "restitution": 0}},
      "acousticMaterials": {}},
    "osageSound": {"version": 0, "contactSounds": {
      "stone": {"voice": "impact", "params": {"tone": 900, "ring": 0.05}, "hardness": 0.9},
      "boot": {"voice": "impact", "hardness": 0.4}}}}},
  "scene": 0,
  "scenes": [{"nodes": [0], "extras": {"osageSound": {"seconds": 2, "seed": 4,
    "listener": {"node": ["yard", "ears"]}, "reverb": {"secondsT60": [0.5, 0.4, 0.3]},
    "air": {"celsius": 20, "humidityPercent": 50}}}}],
  "nodes": [
    {"children": [1], "extras": {"osage": {"id": "yard"}, "osageSound": {"surfaces": [
      {"id": "paving", "quadM": [[-50, 0, -50], [-50, 0, 50], [50, 0, 50], [50, 0, -50]], "collision": "stone"}]}}},
    {"translation": [0, 1.6, 0], "extras": {"osage": {"id": "ears"}}}
  ]
}"#;

#[test]
fn a_contact_the_solver_reports_is_heard_where_and_when_it_is_made() {
    let scene = Scene::from_json(GROUND).unwrap();
    // The physics is given the scene's own collision surfaces, as meshes in the world.
    let meshes: Vec<StaticMesh> = (0..scene.surfaces.len())
        .filter(|&i| scene.surfaces[i].collision.is_some())
        .map(|i| StaticMesh {
            material: None,
            id: scene.surfaces[i].id.clone(),
            vertices: scene.corners(i, 0.0).map(|c| [c.x as f32, c.y as f32, c.z as f32]).to_vec(),
            triangles: vec![[0, 1, 2], [0, 2, 3]],
        })
        .collect();
    let mass = 70.0;
    let settings = Settings {
        tick_hz: 120,
        gravity: 9.81,
        terminal_speed: 50.0,
        capsule: Capsule { radius: 0.2, half_segment: 0.3, mass, friction: 0.7 },
    };
    // A body 20 m to the listener's right, its lowest point 1.5 m up, let fall.
    let mut sim =
        Simulation::new(osage_dynamics::Engine::Rapier, &meshes, &[], settings, [20.0, 2.0, 0.0], [0.0; 3]).unwrap();
    let mut records = Vec::new();
    let mut told = Vec::new();
    for tick in 0..120 {
        sim.step_with_impulses(&mut records).unwrap();
        for record in &records {
            let (Some(pair), Some(point), Some(impulse)) = (record.pair(), record.point, record.normal_impulse_ns())
            else {
                continue;
            };
            // What resting on the ground pushes each tick is no blow.
            let hard = strength(f64::from(impulse), f64::from(mass));
            if hard < 0.2 {
                continue;
            }
            let (node, material) = scene.collision_surface(&["yard"], &meshes[pair.surface].id).unwrap();
            told.push(Struck {
                at: tick as f64 / f64::from(settings.tick_hz) + f64::from(record.start_offset_s),
                point: vec3(f64::from(point[0]), f64::from(point[1]), f64::from(point[2])),
                node,
                material,
                by: "boot".to_owned(),
                by_node: None,
                strength: hard,
            });
        }
    }
    assert!(!told.is_empty(), "the solver reported no blow");
    let first = told[0].clone();
    // It fell 1.5 m: about 0.55 s.
    assert!((first.at - (2.0 * 1.5 / 9.81f64).sqrt()).abs() < 0.03, "struck at {} s", first.at);
    assert!((first.point.x - 20.0).abs() < 0.3 && first.point.y.abs() < 0.05, "struck at {:?}", first.point);

    let mut engine = Engine::new(scene, Options { listening: Listening::Speakers, ..Options::default() });
    for struck in told {
        engine.strike_told(struck);
    }
    let (mut left, mut right) = (Vec::new(), Vec::new());
    let (mut l, mut r) = ([0.0f32; BLOCK], [0.0f32; BLOCK]);
    for _ in 0..(1.5 * RATE as f64 / BLOCK as f64) as usize {
        engine.render(&mut l, &mut r);
        left.extend_from_slice(&l);
        right.extend_from_slice(&r);
    }
    assert!(engine.tally.contacts >= 1);
    let peak = right.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(peak > 1e-3, "nothing heard: {peak}");
    let heard = right.iter().position(|s| s.abs() > 0.05 * peak).unwrap() as f64 / RATE as f64;
    let expected = first.at + first.point.distance(vec3(0.0, 1.6, 0.0)) / 343.0;
    assert!((heard - expected).abs() < 0.003, "heard at {heard} s for {expected} s");
    // On the right, where it fell; nothing before.
    let after = |s: &[f32]| rms(part(s, expected, expected + 0.2));
    assert!(after(&right) > 5.0 * after(&left));
    assert_eq!(rms(part(&right, 0.0, first.at)), 0.0);
}
