// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The engine played live, its nodes placed by a game: it must give what an
//! offline render of the same poses gives, block for block, and a node told
//! where it is sixty times a second must sound as one that moves by itself.

use osage_sound::{
    ears::Listening,
    engine::{BLOCK, Options, Pose, Quality, RATE},
    live::Live,
    render::{Told, render, render_told},
    scene::Scene,
    space::{Placement, Vec3, vec3},
};

/// A car with a tone, a floor that mirrors it, and a listener, each in a
/// node of its own under the world. `motion`: the car's motion, if it moves
/// by itself.
fn scene(motion: &str, seconds: f64) -> Scene {
    let text = format!(
        r#"{{
      "asset": {{"version": "2.0", "extras": {{
        "osageObject": {{"version": 1, "id": "test", "collisionMaterials": {{}},
          "acousticMaterials": {{"hard": {{"absorption": [0.0, 0.0, 0.0], "transmission": [0.0, 0.0, 0.0], "scattering": 0.3}}}}}},
        "osageSound": {{"version": 0, "contactSounds": {{}}}}}}}},
      "scene": 0,
      "scenes": [{{"nodes": [0], "extras": {{"osageSound": {{
        "seconds": {seconds}, "seed": 6,
        "listener": {{"node": ["world", "ears"]}},
        "reverb": {{"secondsT60": [0.5, 0.4, 0.3]}}, "air": {{"celsius": 20, "humidityPercent": 50}}}}}}}}],
      "nodes": [
        {{"children": [1, 2, 3], "extras": {{"osage": {{"id": "world"}}, "osageSound": {{
          "surfaces": [{{"id": "floor", "quadM": [[-200, -2, -200], [200, -2, -200], [200, -2, 200], [-200, -2, 200]],
                        "acoustic": "hard", "reflects": true, "occludes": true}}]}}}}}},
        {{"translation": [0, 0, 0], "extras": {{"osage": {{"id": "ears"}}}}}},
        {{"translation": [-40, -1, -6], "extras": {{"osage": {{"id": "car"}}, "osageSound": {{{motion}
          "emitters": [{{"id": "horn", "voice": "tone", "params": {{"hz": 700}}, "maxDistanceM": 400}}]}}}}}},
        {{"translation": [3, -1, 8], "extras": {{"osage": {{"id": "river"}}, "osageSound": {{
          "emitters": [{{"id": "water", "voice": "noise", "maxDistanceM": 400}}]}}}}}}
      ]
    }}"#
    );
    Scene::from_json(&text).unwrap_or_else(|why| panic!("{why}"))
}

/// 20 m/s along x, from 40 m to the left: level with the listener at 2 s.
const SPEED: f64 = 20.0;

/// Where the car is at `t`, as a game would tell it, sixty times a second
/// from `from` to `to`.
fn poses(car: usize, from: f64, to: f64) -> Vec<Told> {
    let count = ((to - from) * 60.0).round() as usize;
    (0..=count)
        .map(|i| {
            let at = from + i as f64 / 60.0;
            let placement = Placement { translation: vec3(-40.0 + SPEED * at, -1.0, -6.0), ..Placement::default() };
            Told { at, node: car, pose: Pose { placement, velocity: vec3(SPEED, 0.0, 0.0) } }
        })
        .collect()
}

fn options(quality: Quality) -> Options {
    Options { quality, listening: Listening::Headphones, ..Options::default() }
}

fn rms(samples: &[f32]) -> f64 {
    (samples.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / samples.len().max(1) as f64).sqrt()
}

#[test]
fn played_live_it_gives_the_samples_of_an_offline_render_block_for_block() {
    let seconds = 4.0;
    let still = scene("", seconds);
    let car = still.tree.find(&["world", "car"]).expect("the car");
    // Told from before the start, as a game that was already running.
    let told = poses(car, -3.0, seconds);
    for quality in [Quality::LOW, Quality::HIGH] {
        let offline = render_told(&still, &options(quality), seconds, &told);
        let live = Live::start(still.clone(), options(quality), &told, 4);
        let mut heard = live.feed().listen();
        let blocks = offline.left.len().div_ceil(BLOCK);
        for b in 0..blocks {
            let block = heard.next_block();
            let (l, r): (Vec<f32>, Vec<f32>) = block.chunks(2).map(|frame| (frame[0], frame[1])).unzip();
            let end = ((b + 1) * BLOCK).min(offline.left.len());
            let n = end - b * BLOCK;
            assert!(
                l[..n] == offline.left[b * BLOCK..end] && r[..n] == offline.right[b * BLOCK..end],
                "{}: block {b} differs",
                quality.name
            );
        }
        assert!(rms(&offline.left) > 1e-3, "something is heard");
    }
}

#[test]
fn a_node_told_where_it_is_sixty_times_a_second_sounds_as_one_that_moves_by_itself() {
    let seconds = 4.0;
    let moving = scene(r#""motion": {"alongM": [1, 0, 0], "atS": 0.0, "speedKmh": 72},"#, seconds);
    let still = scene("", seconds);
    let car = still.tree.find(&["world", "car"]).expect("the car");
    for quality in [Quality::MEDIUM, Quality::REFERENCE] {
        let by_itself = render(&moving, &options(quality), seconds);
        let told = render_told(&still, &options(quality), seconds, &poses(car, -3.0, seconds));
        // The same sound, to the rounding of a placement made again sixty times a second.
        for (a, b) in [(&by_itself.left, &told.left), (&by_itself.right, &told.right)] {
            let difference: Vec<f32> = a.iter().zip(b).map(|(a, b)| a - b).collect();
            let db = 20.0 * (rms(&difference) / rms(a)).log10();
            assert!(db < -60.0, "{}: {db:.1} dB of difference", quality.name);
        }
    }
}

#[test]
#[ignore = "real time: on an idle machine, alone (cargo test -p osage-sound --test live -- --ignored)"]
fn the_output_never_waits_and_a_pose_is_heard_within_the_blocks_kept_ahead_and_one() {
    let seconds = 2.0;
    let still = scene("", seconds);
    let car = still.tree.find(&["world", "car"]).expect("the car");
    let live = Live::start(still, options(Quality::MEDIUM), &[], 2);
    let mut heard = live.feed().listen();
    // An output that takes a block's worth of samples every block's time, and a game that tells the car's
    // pose every sixtieth of a second.
    let block = std::time::Duration::from_secs_f64(BLOCK as f64 / RATE as f64);
    let start = std::time::Instant::now();
    let blocks = (seconds * RATE as f64 / BLOCK as f64) as usize;
    let mut told = 0;
    for b in 0..blocks {
        while std::time::Instant::now() < start + block * b as u32 {
            std::hint::spin_loop();
        }
        let now = b as f64 * BLOCK as f64 / RATE as f64;
        if now * 60.0 >= told as f64 {
            let placement = Placement { translation: vec3(-40.0 + SPEED * now, -1.0, -6.0), ..Default::default() };
            live.drive(car, Pose { placement, velocity: Vec3::X * SPEED });
            told += 1;
        }
        for _ in 0..2 * BLOCK {
            heard.sample();
        }
    }
    let report = live.report();
    assert!(report.latency_count > 0);
    // A test machine may be busy: a few late blocks are allowed, not a habit.
    assert!(report.underruns < (RATE as u64) / 100, "{report:?}");
    // Two blocks kept ahead and the one made with the pose, and what a busy machine adds.
    assert!(report.latency_mean < 3.5 * block.as_secs_f64(), "{report:?}");
}
