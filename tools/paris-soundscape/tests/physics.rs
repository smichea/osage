// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! What the engine must get right whatever the scene: each test places a
//! known sound in a small tree and checks what reaches the ears against the
//! physics it stands for.

use osage_paris_soundscape::{
    ears::Listening,
    engine::{Options, Quality, RATE},
    render::{Rendered, render},
    scene::Scene,
};

/// A scene from its nodes (the first is the root; the listener is the node
/// "ears" under it), with two materials to mirror with and two things to
/// touch.
fn scene(nodes: &str, seconds: f64) -> Scene {
    let text = format!(
        r#"{{
      "asset": {{"version": "2.0", "extras": {{
        "osageObject": {{"version": 1, "id": "test",
          "collisionMaterials": {{"ground": {{"friction": 0.8, "restitution": 0.0}}, "plate": {{"friction": 0.5, "restitution": 0.0}},
                                 "wheel": {{"friction": 1.0, "restitution": 0.0}}}},
          "acousticMaterials": {{
            "hard": {{"absorption": [0.0, 0.0, 0.0], "transmission": [0.0, 0.0, 0.0], "scattering": 0.0}},
            "felt": {{"absorption": [0.19, 0.51, 0.75], "transmission": [0.0, 0.0, 0.0], "scattering": 0.0}}}}}},
        "osageSound": {{"version": 0, "contactSounds": {{
          "ground": {{"hardness": 0.5}},
          "plate": {{"params": {{"tone": 400, "ring": 0.3, "shape": 1, "scuff": 0.0}}, "hardness": 1.0}},
          "wheel": {{"hardness": 1.0}}}}}}}}}},
      "scene": 0,
      "scenes": [{{"nodes": [0], "extras": {{"osageSound": {{
        "seconds": {seconds}, "seed": 6,
        "listener": {{"node": ["world", "ears"]}},
        "reverb": {{"secondsT60": [0.5, 0.4, 0.3]}},
        "air": {{"celsius": 20, "humidityPercent": 50}}}}}}}}],
      "nodes": [{nodes}]
    }}"#
    );
    Scene::from_json(&text).unwrap_or_else(|why| panic!("{why}"))
}

/// The engine at its best, without the space's own answer: only the ways.
const DRY: Quality = Quality { reverb: false, ..Quality::REFERENCE };

fn options(listening: Listening) -> Options {
    Options { quality: DRY, listening, ..Options::default() }
}

fn rms(samples: &[f32]) -> f64 {
    (samples.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / samples.len().max(1) as f64).sqrt()
}

/// The part of a render between two instants, s.
fn part(samples: &[f32], from: f64, to: f64) -> &[f32] {
    &samples[(from * RATE as f64) as usize..(to * RATE as f64) as usize]
}

/// The pitch of a tone, Hz, from how often it rises through zero.
fn pitch(samples: &[f32]) -> f64 {
    let rises: Vec<f64> = samples
        .windows(2)
        .enumerate()
        .filter(|(_, w)| w[0] < 0.0 && w[1] >= 0.0)
        // Where between the two samples it crosses.
        .map(|(i, w)| i as f64 + f64::from(-w[0]) / f64::from(w[1] - w[0]))
        .collect();
    (rises.len() - 1) as f64 / (rises[rises.len() - 1] - rises[0]) * RATE as f64
}

/// A tone or a noise at `at` in a node of its own under the world.
fn emitter(id: &str, voice: &str, hz: f64, at: [f64; 3]) -> String {
    let params = if voice == "tone" { format!(r#""params": {{"hz": {hz}}}, "#) } else { String::new() };
    format!(
        r#"{{"translation": [{}, {}, {}], "extras": {{"osage": {{"id": "{id}"}}, "osageSound": {{
             "emitters": [{{"id": "sound", "voice": "{voice}", {params}"maxDistanceM": 400}}]}}}}}}"#,
        at[0], at[1], at[2]
    )
}

const EARS: &str = r#"{"extras": {"osage": {"id": "ears"}}}"#;

fn world(children: usize, sound: &str) -> String {
    let list: Vec<String> = (1..=children).map(|i| i.to_string()).collect();
    format!(r#"{{"children": [{}], "extras": {{"osage": {{"id": "world"}}{sound}}}}}"#, list.join(", "))
}

#[test]
fn twice_as_far_is_half_as_loud() {
    // A low tone, which the air leaves alone; straight ahead, so each speaker has 1/√2 of it.
    let at = |metres: f64| {
        let nodes = [world(2, ""), EARS.to_owned(), emitter("tone", "tone", 250.0, [0.0, 0.0, -metres])].join(",");
        render(&scene(&nodes, 1.0), &options(Listening::Speakers), 1.0)
    };
    let (near, far) = (at(10.0), at(20.0));
    let ratio = rms(&near.left) / rms(&far.left);
    assert!((ratio - 2.0).abs() < 0.01, "{ratio}");
    // The voice makes an RMS of 0.1 one metre away.
    let expected = 0.1 / 10.0 * std::f64::consts::FRAC_1_SQRT_2;
    assert!((rms(&near.left) / expected - 1.0).abs() < 0.01, "{} for {expected}", rms(&near.left));
    assert_eq!(near.left, near.right);
}

#[test]
fn the_air_takes_the_top_first() {
    // 200 m away, a high tone has lost what ISO 9613-1 says and a low one almost nothing.
    let level = |hz: f64, metres: f64| {
        let nodes = [world(2, ""), EARS.to_owned(), emitter("tone", "tone", hz, [0.0, 0.0, -metres])].join(",");
        rms(&render(&scene(&nodes, 1.0), &options(Listening::Speakers), 1.0).left)
    };
    let loss = |hz: f64| 20.0 * (level(hz, 10.0) / level(hz, 200.0) / 20.0).log10();
    assert!(loss(250.0) < 0.4, "{}", loss(250.0));
    // 190 m of air at 20 °C and 50 %: about 0.9 dB at 1 kHz, 5.5 dB at 4 kHz, 20 dB at 8 kHz. The two poles
    // that stand for the air follow it to 4 kHz and understate it above.
    assert!((0.2..1.6).contains(&loss(1_000.0)), "{}", loss(1_000.0));
    assert!((4.3..6.8).contains(&loss(4_000.0)), "{}", loss(4_000.0));
    assert!(loss(8_000.0) > 10.0, "{}", loss(8_000.0));
}

#[test]
fn what_comes_nearer_sounds_higher_and_what_goes_away_lower() {
    // 72 km/h along a line 5 m in front: level with the listener at 2 s.
    let mover = r#"{"translation": [0, 0, -5], "extras": {"osage": {"id": "car"}, "osageSound": {
        "motion": {"alongM": [1, 0, 0], "atS": 2.0, "speedKmh": 72},
        "emitters": [{"id": "horn", "voice": "tone", "params": {"hz": 1000}}]}}}"#;
    let nodes = [world(2, ""), EARS.to_owned(), mover.to_owned()].join(",");
    let heard = render(&scene(&nodes, 4.0), &options(Listening::Speakers), 4.0);
    let mid = |left: &[f32], right: &[f32]| left.iter().zip(right).map(|(l, r)| l + r).collect::<Vec<f32>>();
    let both = mid(&heard.left, &heard.right);
    // Half a second in, the sound heard left the car at about 0.39 s: 32 m before, 5 m aside.
    let coming = pitch(part(&both, 0.45, 0.55));
    let going = pitch(part(&both, 3.45, 3.55));
    // 1000 / (1 − v cos / c), with the angle the car was at when the sound left it.
    let shift = |along: f64| 1_000.0 / (1.0 + 20.0 * along / (along * along + 25.0).sqrt() / 343.0);
    assert!((coming - shift(-32.2)).abs() < 1.5, "{coming} for {}", shift(-32.2));
    assert!((going - shift(27.3)).abs() < 1.5, "{going} for {}", shift(27.3));
    assert!(coming > 1_055.0 && going < 950.0);
    // It comes from the left and leaves to the right.
    assert!(rms(part(&heard.left, 0.4, 0.8)) > 2.0 * rms(part(&heard.right, 0.4, 0.8)));
    assert!(rms(part(&heard.right, 3.2, 3.6)) > 2.0 * rms(part(&heard.left, 3.2, 3.6)));
}

#[test]
fn a_floor_adds_its_reflection_with_its_own_delay() {
    // An emitter 1 m and ears 2 m above a hard floor, 4 m apart: the mirrored way is 0.877 m longer.
    let heard = |hz: f64, floor: &str| {
        let sound = if floor.is_empty() {
            String::new()
        } else {
            format!(
                r#", "osageSound": {{"surfaces": [{{"id": "floor", "quadM": [[-30, 0, -30], [-30, 0, 30], [30, 0, 30], [30, 0, -30]], "acoustic": "{floor}"}}]}}"#
            )
        };
        let ears = r#"{"translation": [0, 2, 0], "extras": {"osage": {"id": "ears"}}}"#;
        let nodes = [world(2, &sound), ears.to_owned(), emitter("tone", "tone", hz, [0.0, 1.0, -4.0])].join(",");
        rms(&render(&scene(&nodes, 1.0), &options(Listening::Speakers), 1.0).left)
    };
    let (straight, mirrored) = (17f64.sqrt(), 5.0);
    for hz in [300.0, 500.0, 700.0] {
        // Two tones of the same pitch, the second weaker by its longer way and late by it.
        let turn = std::f64::consts::TAU * hz * (mirrored - straight) / 343.0;
        let share = straight / mirrored;
        let expected = (1.0 + share * share + 2.0 * share * turn.cos()).sqrt();
        let found = heard(hz, "hard") / heard(hz, "");
        assert!((found / expected - 1.0).abs() < 0.02, "{hz} Hz: {found} for {expected}");
    }
    // A floor that takes most of the top sends back less of it.
    let (hard, felt, none) = (heard(4_000.0, "hard"), heard(4_000.0, "felt"), heard(4_000.0, ""));
    assert!((felt - none).abs() < 0.75 * (hard - none).abs(), "{hard} {felt} {none}");
}

#[test]
fn turning_the_head_half_round_swaps_the_ears() {
    // A noise to the left and a little ahead, above ear level.
    let nodes = [world(2, ""), EARS.to_owned(), emitter("hiss", "noise", 0.0, [-6.0, 1.5, -2.0])].join(",");
    let scene = scene(&nodes, 1.0);
    for listening in [Listening::Headphones, Listening::Speakers] {
        let ahead = render(&scene, &options(listening), 1.0);
        let behind = render(&scene, &Options { turn: std::f64::consts::PI, ..options(listening) }, 1.0);
        // On the left: the left ear hears more of it.
        assert!(rms(&ahead.left) > 1.5 * rms(&ahead.right), "{} {}", rms(&ahead.left), rms(&ahead.right));
        // Turned half round it is on the right, sample for sample what the other ear heard.
        assert!(ahead.left == behind.right && ahead.right == behind.left);
    }
    // In each ear: for headphones the far ear hears it later, by the way round the head.
    let heard = render(&scene, &options(Listening::Headphones), 1.0);
    let lag = (0..40i64)
        .max_by(|a, b| {
            let at = |lag: i64| -> f64 {
                (2_000..40_000)
                    .map(|i| f64::from(heard.left[i]) * f64::from(heard.right[(i as i64 + lag) as usize]))
                    .sum()
            };
            at(*a).total_cmp(&at(*b))
        })
        .unwrap();
    // 0.6 ms or so: 25 to 31 samples.
    assert!((22..=32).contains(&lag), "the right ear is {lag} samples late");
}

#[test]
fn a_source_follows_the_node_it_is_placed_in() {
    // An emitter at [3, 0, −4] of a cart; the cart moved, the sound moves with it.
    let scene_with = |cart_x: f64| {
        let cart = format!(
            r#"{{"translation": [{cart_x}, 0, 0], "extras": {{"osage": {{"id": "cart"}}, "osageSound": {{
                 "emitters": [{{"id": "bell", "voice": "tone", "positionM": [3, 0, -4], "params": {{"hz": 250}}}}]}}}}}}"#
        );
        scene(&[world(2, ""), EARS.to_owned(), cart].join(","), 1.0)
    };
    let here = scene_with(0.0);
    assert_eq!(here.emitter_name(0), "world / cart : bell");
    let heard = render(&here, &options(Listening::Speakers), 1.0);
    // 5 m away, 3/5 of the way to the right: the pan's angle is (0.6 + 1) π/4.
    let angle = 1.6 * std::f64::consts::FRAC_PI_4;
    assert!((rms(&heard.right) / rms(&heard.left) - angle.tan()).abs() < 0.01);
    assert!((rms(&heard.right) - 0.1 / 5.0 * angle.sin()).abs() < 2e-4);
    // The cart six metres to the left: the same sound, on the other side.
    let moved = render(&scene_with(-6.0), &options(Listening::Speakers), 1.0);
    assert!(
        (rms(&moved.left) / rms(&heard.right) - 1.0).abs() < 1e-3
            && (rms(&moved.right) / rms(&heard.left) - 1.0).abs() < 1e-3
    );
}

#[test]
fn a_contact_is_heard_where_it_is_and_late_by_its_distance() {
    // A wheel rolling along X at 36 km/h comes onto a plate 20 m to the right at 1 s.
    let ground = r#", "osageSound": {"surfaces": [
        {"id": "ground", "quadM": [[-50, 0, -50], [-50, 0, 50], [50, 0, 50], [50, 0, -50]], "collision": "ground"},
        {"id": "plate", "quadM": [[20, 0.01, -3], [20, 0.01, 3], [21, 0.01, 3], [21, 0.01, -3]], "collision": "plate", "strikes": true}]}"#;
    let ears = r#"{"translation": [0, 1.6, 0], "extras": {"osage": {"id": "ears"}}}"#;
    let wheel = r#"{"translation": [10, 0, 0], "extras": {"osage": {"id": "wheel"}, "osageSound": {
        "motion": {"alongM": [1, 0, 0], "atS": 0, "speedKmh": 36},
        "touches": [{"kind": "wheels", "material": "wheel", "pointsM": [[0, 0, 0]], "strength": 1.0}]}}}"#;
    let nodes = [world(2, ground), ears.to_owned(), wheel.to_owned()].join(",");
    let heard = render(&scene(&nodes, 2.5), &options(Listening::Speakers), 2.5);
    assert_eq!(heard.tally.contacts, 1);
    let peak = heard.right.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(peak > 1e-3, "nothing heard: {peak}");
    let first = heard.right.iter().position(|s| s.abs() > 0.05 * peak).unwrap() as f64 / RATE as f64;
    // 20 m and a little: 58 ms after the wheel came onto the plate.
    let expected = 1.0 + (20.0f64 * 20.0 + 1.6 * 1.6).sqrt() / 343.0;
    assert!((first - expected).abs() < 0.004, "heard at {first} s for {expected} s");
    // On the right, where the plate is; and it rings, then dies.
    assert!(rms(part(&heard.right, 1.06, 1.3)) > 5.0 * rms(part(&heard.left, 1.06, 1.3)));
    assert!(rms(part(&heard.right, 1.1, 1.2)) > 0.01 * f64::from(peak));
    assert_eq!(rms(part(&heard.right, 0.0, 1.0)), 0.0);
}

/// Four noises a metre apart round a point `metres` ahead; with `cut`, their
/// group stands for them beyond 50 m with one noise 6 dB louder.
fn group(metres: f64, cut: bool) -> Rendered {
    let stands = if cut {
        r#""proxyBeyondM": 50, "emitters": [{"id": "all", "voice": "noise", "gainDb": 6.02, "maxDistanceM": 400}]"#
    } else {
        ""
    };
    let mut nodes = vec![
        world(2, ""),
        EARS.to_owned(),
        format!(
            r#"{{"children": [3, 4, 5, 6], "translation": [0, 0, {}], "extras": {{"osage": {{"id": "group"}}, "osageSound": {{{stands}}}}}}}"#,
            -metres
        ),
    ];
    for (i, x) in [-1.5, -0.5, 0.5, 1.5].into_iter().enumerate() {
        nodes.push(emitter(&format!("one-{i}"), "noise", 0.0, [x, 0.0, 0.0]));
    }
    render(&scene(&nodes.join(","), 2.0), &options(Listening::Speakers), 2.0)
}

#[test]
fn from_afar_a_node_s_own_emitter_stands_for_its_branch() {
    // Near: the four are made and heard. Far: one voice, and the level they would have made.
    let (near, far) = (group(10.0, true), group(100.0, true));
    assert_eq!(near.tally.voices, 4 * near.tally.blocks);
    assert_eq!(far.tally.voices, far.tally.blocks);
    let four = group(100.0, false);
    assert_eq!(four.tally.voices, 4 * four.tally.blocks);
    let ratio = rms(&far.left) / rms(&four.left);
    assert!((ratio - 1.0).abs() < 0.1, "one for four: {ratio}");
    // Between the two, both sound, and the level holds.
    let between = group(46.0, true);
    assert_eq!(between.tally.voices, 5 * between.tally.blocks);
    let ratio = rms(&between.left) / rms(&group(46.0, false).left);
    assert!((ratio - 1.0).abs() < 0.15, "crossing: {ratio}");
}

#[test]
fn a_budget_of_ways_keeps_the_loudest() {
    // Six noises from 5 to 30 m; a device that affords two ways hears the two nearest.
    let all: Vec<String> =
        (0..6).map(|i| emitter(&format!("n{i}"), "noise", 0.0, [0.0, 0.0, -5.0 * (i + 1) as f64])).collect();
    let nodes = |emitters: &[String]| {
        [vec![world(emitters.len() + 1, ""), EARS.to_owned()], emitters.to_vec()].concat().join(",")
    };
    let two_ways =
        Options { quality: Quality { ways: 2, ..DRY }, listening: Listening::Speakers, ..Options::default() };
    let kept = render(&scene(&nodes(&all), 1.0), &two_ways, 1.0);
    assert_eq!(kept.tally.most_ways, 2);
    assert_eq!(kept.tally.voices, 2 * kept.tally.blocks);
    let nearest = render(&scene(&nodes(&all[..2]), 1.0), &options(Listening::Speakers), 1.0);
    assert!((rms(&kept.left) / rms(&nearest.left) - 1.0).abs() < 1e-6);
    let everything = render(&scene(&nodes(&all), 1.0), &options(Listening::Speakers), 1.0);
    assert_eq!(everything.tally.most_ways, 6);
    assert!(rms(&everything.left) > rms(&kept.left));
}

#[test]
fn looking_at_the_scene_less_often_keeps_pitch_and_level() {
    // A horn passing at 72 km/h, 5 m in front, level with the listener at 2 s: its ways found every block,
    // and every fourth block.
    let mover = r#"{"translation": [0, 0, -5], "extras": {"osage": {"id": "car"}, "osageSound": {
        "motion": {"alongM": [1, 0, 0], "atS": 2.0, "speedKmh": 72},
        "emitters": [{"id": "horn", "voice": "tone", "params": {"hz": 1000}}]}}}"#;
    let scene = scene(&[world(2, ""), EARS.to_owned(), mover.to_owned()].join(","), 4.0);
    let heard = |every: u64| {
        let quality = Quality { every, ..DRY };
        let made = render(&scene, &Options { quality, listening: Listening::Speakers, ..Options::default() }, 4.0);
        made.left.iter().zip(&made.right).map(|(l, r)| l + r).collect::<Vec<f32>>()
    };
    let (each, fourth) = (heard(1), heard(4));
    for (from, to) in [(0.45, 0.55), (1.2, 1.3), (2.7, 2.8), (3.45, 3.55)] {
        let (a, b) = (pitch(part(&each, from, to)), pitch(part(&fourth, from, to)));
        assert!((a - b).abs() < 0.5, "{from} s: {a} Hz and {b} Hz");
    }
    // Its level too, a quarter of a second at a time; three blocks late is 8 ms.
    for quarter in 0..16 {
        let (from, to) = (f64::from(quarter) * 0.25, f64::from(quarter + 1) * 0.25);
        let ratio = rms(part(&fourth, from, to)) / rms(part(&each, from, to));
        assert!((ratio - 1.0).abs() < 0.06, "{from} s: {ratio}");
    }
    // And nothing steps: no change from a sample to the next beyond what the loudest tone makes.
    let steepest = fourth.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
    let peak = fourth.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(steepest < peak * 0.16, "{steepest} for a peak of {peak}");
}

#[test]
fn a_voice_that_wakes_swells_in() {
    // A noise heard from 20 m at most, coming from 30 m at 5 m/s: it wakes at 2 s, in the middle of its sound.
    let comer = r#"{"translation": [0, 0, -30], "extras": {"osage": {"id": "comer"}, "osageSound": {
        "motion": {"alongM": [0, 0, 1], "atS": 0.0, "speedKmh": 18},
        "emitters": [{"id": "hiss", "voice": "noise", "maxDistanceM": 20}]}}}"#;
    let scene = scene(&[world(2, ""), EARS.to_owned(), comer.to_owned()].join(","), 3.0);
    let heard = render(&scene, &options(Listening::Speakers), 3.0);
    let first = heard.left.iter().position(|s| *s != 0.0).expect("it is heard");
    // It is within 20 m at 2 s; the sound that left it there arrives 58 ms later, which is when it wakes;
    // and what it then makes takes 58 ms more to arrive.
    let at = first as f64 / RATE as f64;
    assert!((at - 2.116).abs() < 0.01, "first heard at {at} s");
    // Until then its voice was not made.
    let asleep = heard.tally.blocks - heard.tally.voices;
    assert!((765..=780).contains(&asleep), "asleep for {asleep} blocks");
    // Its first two milliseconds are a tenth of what it is once awake.
    let (start, awake) = (rms(&heard.left[first..first + 96]), rms(&heard.left[first + 2_000..first + 4_000]));
    assert!(start < 0.15 * awake, "{start} then {awake}");
    assert!(rms(&heard.left[first + 960..first + 1_400]) > 0.8 * awake);
}

#[test]
fn the_same_scene_twice_is_the_same_samples() {
    let nodes = [
        world(3, ""),
        EARS.to_owned(),
        emitter("a", "noise", 0.0, [4.0, 0.0, -3.0]),
        emitter("b", "water", 0.0, [-7.0, -1.0, 2.0]),
    ]
    .join(",");
    let scene = scene(&nodes, 1.0);
    let options = Options { quality: Quality::HIGH, ..Options::default() };
    let (first, second) = (render(&scene, &options, 1.0), render(&scene, &options, 1.0));
    assert!(first.left == second.left && first.right == second.right);
    let other = render(&Scene { seed: 7, ..scene.clone() }, &options, 1.0);
    assert!(other.left != first.left);
}
