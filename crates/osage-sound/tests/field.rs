// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Far sound as a field: what is near is heard as it was, what is far keeps
//! its side and its level through the field, and what the field costs a
//! listener does not depend on how many emitters made it.

use osage_sound::{
    bench::sphere,
    ears::Listening,
    engine::{Options, Quality},
    field::Far,
    render::render,
    scene::Scene,
    space::Vec3,
};

/// Two tones on either side of a listener, each in a node of its own.
fn scene(left: f64, right: f64) -> Scene {
    let text = format!(
        r#"{{
      "asset": {{"version": "2.0", "extras": {{
        "osageObject": {{"version": 1, "id": "test", "collisionMaterials": {{}}, "acousticMaterials": {{}}}},
        "osageSound": {{"version": 0, "contactSounds": {{}}}}}}}},
      "scene": 0,
      "scenes": [{{"nodes": [0], "extras": {{"osageSound": {{"seconds": 2, "seed": 6,
        "listener": {{"node": ["world", "ears"]}}, "reverb": {{"secondsT60": [0.5, 0.4, 0.3]}},
        "air": {{"celsius": 20, "humidityPercent": 50}}}}}}}}],
      "nodes": [
        {{"children": [1, 2, 3], "extras": {{"osage": {{"id": "world"}}}}}},
        {{"extras": {{"osage": {{"id": "ears"}}}}}},
        {{"translation": [{}, 0, -2], "extras": {{"osage": {{"id": "west"}}, "osageSound": {{
          "emitters": [{{"id": "tone", "voice": "tone", "params": {{"hz": 400}}, "maxDistanceM": 600}}]}}}}}},
        {{"translation": [{}, 0, -3], "extras": {{"osage": {{"id": "east"}}, "osageSound": {{
          "emitters": [{{"id": "tone", "voice": "noise", "maxDistanceM": 600}}]}}}}}}
      ]
    }}"#,
        -left, right
    );
    Scene::from_json(&text).unwrap_or_else(|why| panic!("{why}"))
}

fn options(far: Option<Far>) -> Options {
    Options { quality: Quality::MEDIUM, listening: Listening::Headphones, far, ..Options::default() }
}

fn far(beyond: f64, order: usize) -> Option<Far> {
    Some(Far { centre: Vec3::ZERO, beyond, order })
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

#[test]
fn with_nothing_far_a_field_changes_not_a_sample() {
    let near = scene(10.0, 20.0);
    let whole = render(&near, &options(None), 2.0);
    for order in [1, 2] {
        let split = render(&near, &options(far(100.0, order)), 2.0);
        assert!(split.left == whole.left && split.right == whole.right, "order {order}");
    }
}

#[test]
fn what_is_far_keeps_its_side_and_its_level_through_the_field() {
    // A noise 300 m to the right, alone: heard by its ways, and through a field.
    let lone = scene(1.0e5, 300.0);
    let options_solo = |far| Options { solo: vec!["east".to_owned()], ..options(far) };
    let ways = render(&lone, &options_solo(None), 2.0);
    for order in [1, 2] {
        let field = render(&lone, &options_solo(far(100.0, order)), 2.0);
        let side = |r: &osage_sound::render::Rendered| 20.0 * (rms(&r.right) / rms(&r.left)).log10();
        assert!(
            side(&ways) > 3.0 && side(&field) > 1.5,
            "order {order}: {:.1} dB by ways, {:.1} by a field",
            side(&ways),
            side(&field)
        );
        let level = |r: &osage_sound::render::Rendered| 10.0 * (rms(&r.left).powi(2) + rms(&r.right).powi(2)).log10();
        let off = level(&field) - level(&ways);
        assert!(off.abs() < 2.0, "order {order}: {off:+.2} dB");
    }
}

#[test]
fn a_listener_pays_the_same_to_hear_a_field_of_few_emitters_or_many() {
    let options = |far| Options { quality: Quality::LOW, ..options(far) };
    let hearing = |n: usize| {
        let scene = sphere(n, 150.0);
        let field = far(75.0, 1);
        osage_sound::bench::field_cost(&scene, &options(field), 0.5).2
    };
    let (few, many) = (hearing(8), hearing(256));
    // Processor time on a shared machine is noisy: the same within a factor, not thirty-two times.
    assert!(many < 3.0 * few.max(1.0), "{few:.1} ms a second for 8 emitters, {many:.1} for 256");
}
