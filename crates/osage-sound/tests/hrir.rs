// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Measured head responses through virtual loudspeakers (D-242), with a set
//! whose answer is known: each response an impulse, louder in the nearer
//! ear, and the time between the ears that of a sphere. (The measured set
//! itself is an asset, which a test may not have: `tools/make-hrir.py
//! --check` hears it against its own measurements.)

use std::sync::Arc;

use osage_sound::{
    ears::Listening,
    engine::{Options, Quality, RATE},
    hrir::{Hrir, rings, sofa_direction},
    render::render,
    scene::Scene,
    space::Vec3,
};
use osage_sound_analysis::{
    bands::Band,
    cues::{cue, gcc_phat_between},
};

/// The time between the ears the set gives: 600 µs from the side.
const SIDE: f64 = 600e-6;

fn set() -> Hrir {
    let speakers = rings()
        .into_iter()
        .map(|d| {
            let ear = |across: f64| {
                let mut h = vec![0.0f32; 16];
                h[3] = (1.0 + 0.5 * across) as f32;
                h
            };
            (d, [ear(-d.x), ear(d.x)])
        })
        .collect();
    let steps = [72, 37];
    let itd = (0..steps[1])
        .flat_map(|j| {
            (0..steps[0]).map(move |i| (sofa_direction(i as f64 * 5.0, -90.0 + j as f64 * 5.0).x * SIDE) as f32)
        })
        .collect();
    Hrir::new(RATE, speakers, itd, steps).unwrap()
}

/// A noise 3 m away in `direction` (head frame: x right, y up, z ahead).
fn alone(direction: Vec3) -> Scene {
    let at = direction * 3.0;
    Scene::from_json(&format!(
        r#"{{"asset": {{"version": "2.0", "extras": {{
          "osageObject": {{"version": 1, "id": "alone", "collisionMaterials": {{}}, "acousticMaterials": {{}}}},
          "osageSound": {{"version": 0, "contactSounds": {{}}}}}}}},
        "scene": 0,
        "scenes": [{{"nodes": [0], "extras": {{"osageSound": {{"seconds": 1, "seed": 2,
          "listener": {{"node": ["world", "ears"]}}, "reverb": {{"secondsT60": [0.3, 0.3, 0.3]}},
          "air": {{"celsius": 20, "humidityPercent": 50}}}}}}}}],
        "nodes": [
          {{"children": [1, 2], "extras": {{"osage": {{"id": "world"}}}}}},
          {{"extras": {{"osage": {{"id": "ears"}}}}}},
          {{"translation": [{}, {}, {}], "extras": {{"osage": {{"id": "source"}}, "osageSound": {{
            "emitters": [{{"id": "noise", "voice": "noise", "reflections": false}}]}}}}}}
        ]}}"#,
        at.x, at.y, -at.z
    ))
    .unwrap()
}

#[test]
fn a_way_is_heard_with_the_set_s_time_and_level_between_the_ears() {
    let measured = Options {
        quality: Quality::REFERENCE,
        listening: Listening::Headphones,
        measured: Some(Arc::new(set())),
        ..Options::default()
    };
    for (direction, itd, ild) in [
        // From the right: the right ear 600 µs sooner and 9.5 dB louder (1.5 against 0.5).
        (Vec3::X, SIDE, 20.0 * 3f64.log10()),
        (-Vec3::X, -SIDE, -20.0 * 3f64.log10()),
        // Ahead, and between loudspeakers up and to the right.
        (Vec3::Z, 0.0, 0.0),
        (sofa_direction(-40.0, 20.0), sofa_direction(-40.0, 20.0).x * SIDE, f64::NAN),
    ] {
        let heard = render(&alone(direction), &measured, 1.0);
        let (l, r) = (&heard.left[RATE as usize / 5..], &heard.right[RATE as usize / 5..]);
        let found = gcc_phat_between(l, r, RATE, 0.0, 0.8);
        assert!((found.itd_us * 1e-6 - itd).abs() < 25e-6, "{direction:?}: {} µs for {}", found.itd_us, itd * 1e6);
        // In the octave of 1 kHz, where reading a line between its samples takes nothing.
        let db = cue(l, r, RATE, &Band::octave(1_000.0), 0.0, 0.8).ild_db;
        if ild.is_nan() {
            // The three loudspeakers' levels, mixed: louder on the right, less than from the side.
            assert!(db > 1.0 && db < 20.0 * 3f64.log10(), "{direction:?}: {db} dB");
        } else {
            assert!((db - ild).abs() < 0.3, "{direction:?}: {db} dB for {ild}");
        }
    }
}

#[test]
fn without_the_set_or_for_loudspeakers_or_below_high_nothing_changes() {
    let scene = alone(sofa_direction(70.0, 10.0));
    for (quality, listening) in [
        (Quality::REFERENCE, Listening::Speakers),
        (Quality::MEDIUM, Listening::Headphones),
        (Quality::LOW, Listening::Headphones),
    ] {
        let plain = Options { quality, listening, ..Options::default() };
        let measured = Options { measured: Some(Arc::new(set())), ..plain.clone() };
        let (a, b) = (render(&scene, &plain, 0.5), render(&scene, &measured, 0.5));
        assert_eq!((a.left, a.right), (b.left, b.right), "{} {listening:?}", quality.name);
    }
}

#[test]
fn through_measured_ears_the_samples_are_the_same_whatever_the_number_of_threads() {
    // Twenty noises round the head, so that their virtual loudspeakers' feeds are mixed in three groups: 1, 2 and
    // 3 threads give the same samples to the bit.
    let emitters: Vec<String> = (0..20)
        .map(|k| {
            let turn = f64::from(k) * std::f64::consts::TAU / 20.0;
            format!(
                r#"{{"translation": [{}, {}, {}], "extras": {{"osage": {{"id": "noise-{k}"}}, "osageSound": {{
                     "emitters": [{{"id": "noise", "voice": "noise"}}]}}}}}}"#,
                5.0 * turn.sin(),
                0.5 * (turn * 3.0).sin(),
                -5.0 * turn.cos()
            )
        })
        .collect();
    let children: Vec<String> = (1..=21).map(|i| i.to_string()).collect();
    let scene = Scene::from_json(&format!(
        r#"{{
        "asset": {{"version": "2.0", "extras": {{
          "osageObject": {{"version": 1, "id": "ring", "collisionMaterials": {{}}, "acousticMaterials": {{}}}},
          "osageSound": {{"version": 0, "contactSounds": {{}}}}}}}},
        "scene": 0,
        "scenes": [{{"nodes": [0], "extras": {{"osageSound": {{"seconds": 1, "seed": 2,
          "listener": {{"node": ["world", "ears"]}}, "reverb": {{"secondsT60": [0.3, 0.3, 0.3]}},
          "air": {{"celsius": 20, "humidityPercent": 50}}}}}}}}],
        "nodes": [
          {{"children": [{}], "extras": {{"osage": {{"id": "world"}}}}}},
          {{"extras": {{"osage": {{"id": "ears"}}}}}},
          {}
        ]}}"#,
        children.join(", "),
        emitters.join(",\n")
    ))
    .unwrap();
    let heard = |threads: usize| {
        let options = Options {
            quality: Quality::REFERENCE,
            listening: Listening::Headphones,
            measured: Some(Arc::new(set())),
            threads,
            ..Options::default()
        };
        render(&scene, &options, 0.5)
    };
    let one = heard(1);
    assert!(one.peak() > 0.0);
    for threads in [2, 3] {
        let other = heard(threads);
        assert!(one.left == other.left && one.right == other.right, "{threads} threads");
    }
}
