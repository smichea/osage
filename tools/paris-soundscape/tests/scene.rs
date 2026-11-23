// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The scene that comes with the program: what the brief asks of the Pont de
//! Bir-Hakeim, checked on renders short and cheap enough for every run of
//! the tests. The `verify` command checks the same on the whole 45 seconds
//! at the quality of the examples.

use osage_paris_soundscape::{
    ears::Listening,
    engine::{Options, Quality, RATE},
    render::{level, master, render},
    scene::{Reach, Scene},
};
use osage_sound_analysis::{Band, cue, envelope, measure, sha256, wav};

fn options(quality: Quality, solo: &[&str]) -> Options {
    Options { quality, solo: solo.iter().map(|text| (*text).to_owned()).collect(), ..Options::default() }
}

/// The file a render of `seconds` makes, as the command writes it.
fn file(scene: &Scene, options: &Options, seconds: f64) -> (Vec<u8>, usize) {
    let rendered = render(scene, options, seconds);
    let (gain, _) = level(&rendered, scene.loudness, -1.0);
    let mastered = master(&rendered, gain, scene.seed, 0.4, 1.5);
    (wav::pcm16(RATE, 2, &mastered.samples), mastered.held)
}

#[test]
fn every_source_is_a_node_path_and_a_place_in_that_node() {
    let scene = Scene::bundled();
    assert_eq!(scene.tree.path(scene.listener), ["bridge", "road-deck", "pavement-upstream", "walker", "head"]);
    // A bogie of the train: five nodes down, 30 cm above its own node.
    let tyres =
        scene.emitter(&["bridge", "upper-deck", "track-to-passy", "train", "car-3", "bogie-5"], "tyres").unwrap();
    assert_eq!(scene.emitter_name(tyres), "bridge / upper-deck / track-to-passy / train / car-3 / bogie-5 : tyres");
    assert_eq!(scene.emitters[tyres].offset.array(), [0.0, 0.3, 0.0]);
    // When the train's middle is level with the listener, that bogie is 5 m behind it on the upstream track,
    // 8.5 m above the road.
    let train = scene.tree.find(&["bridge", "upper-deck", "track-to-passy", "train"]).unwrap();
    let abeam = scene.tree.nodes[train].motion.as_ref().unwrap().at;
    let (head, bogie) =
        (scene.head(abeam).at, scene.tree.point(scene.emitters[tyres].node, scene.emitters[tyres].offset, abeam));
    assert!(
        (bogie.x - head.x + 5.0).abs() < 0.05 && (bogie.z + 1.5).abs() < 1e-9 && (bogie.y - 16.5).abs() < 1e-9,
        "{bogie:?}"
    );
    // The listener walks towards Grenelle with the Seine on the left: upstream is −Z, the left of who looks along +X.
    let looks = scene.head(0.0);
    assert!(looks.forward.x > 0.99 && looks.right.z > 0.99);
    assert!(scene.head(45.0).at.x - scene.head(0.0).at.x > 60.0);
    // The sources the brief names are all there, each in the tree.
    for (path, voice) in [
        (&["bridge", "river", "pier-2"][..], "water"),
        (&["bridge", "road-deck", "lane-to-passy-kerb", "car-a"][..], "car"),
        (&["bridge", "road-deck", "lane-to-grenelle-middle", "scooter"][..], "scooter"),
        (&["bridge", "upper-deck", "span-4"][..], "metro-span"),
        (&["bridge", "upper-deck", "track-to-passy", "train", "car-1"][..], "metro-motor"),
    ] {
        let node = scene.tree.find(path).unwrap_or_else(|| panic!("no node {path:?}"));
        assert!(
            scene.emitters.iter().any(|e| e.node == node && e.voice == voice && e.reach == Reach::Positional),
            "{path:?}"
        );
    }
}

#[test]
fn the_same_seed_gives_the_same_file_and_nothing_clips() {
    let scene = Scene::bundled();
    // The loudest stretch for the near lane: three cars in the first eight seconds.
    let (first, held) = file(&scene, &options(Quality::HIGH, &[]), 8.0);
    let (second, _) = file(&scene, &options(Quality::HIGH, &[]), 8.0);
    assert!(first == second, "the same seed, another file: {} and {}", sha256(&first), sha256(&second));
    let (other, _) = file(&Scene { seed: scene.seed + 1, ..scene.clone() }, &options(Quality::HIGH, &[]), 8.0);
    assert!(first != other);
    let report = measure(&wav::parse(&first).unwrap());
    assert_eq!((held, report.left.clipping.samples, report.right.clipping.samples), (0, 0, 0));
    assert!(report.true_peak_db <= -0.9, "true peak {} dBTP", report.true_peak_db);
    assert_eq!((report.rate, report.channels, report.frames), (48_000, 2, 8 * 48_000));
}

#[test]
fn turning_the_listener_half_round_moves_the_river_to_the_other_ear() {
    let scene = Scene::bundled();
    let ahead = render(&scene, &options(Quality::HIGH, &["river"]), 6.0);
    let turned = render(&scene, &Options { turn: std::f64::consts::PI, ..options(Quality::HIGH, &["river"]) }, 6.0);
    let wide = Band::new(500.0, 8_000.0);
    let (left_side, right_side) = (
        cue(&ahead.left, &ahead.right, RATE, &wide, 0.0, 6.0),
        cue(&turned.left, &turned.right, RATE, &wide, 0.0, 6.0),
    );
    assert!(left_side.ild_db < -2.0, "ahead: {:+.1} dB", left_side.ild_db);
    assert!(right_side.ild_db > 2.0, "turned: {:+.1} dB", right_side.ild_db);
    assert!((left_side.ild_db + right_side.ild_db).abs() < 0.2);
    // Sample for sample: what one ear heard, the other hears.
    assert!(ahead.left == turned.right && ahead.right == turned.left);
    // And the left ear hears it first, by a good part of the way round a head.
    let fine = cue(&ahead.left, &ahead.right, RATE, &Band::new(100.0, 1_500.0), 0.0, 6.0);
    assert!(fine.itd_us < -150.0, "the right ear leads by {} µs", fine.itd_us);
}

#[test]
fn the_train_comes_passes_and_goes_on_the_right() {
    let scene = Scene::bundled();
    let train = scene.tree.find(&["bridge", "upper-deck", "track-to-passy", "train"]).unwrap();
    let abeam = scene.tree.nodes[train].motion.as_ref().unwrap().at;
    // Alone, and as a weak device would make it: the passage must not depend on the quality.
    let heard = render(&scene, &options(Quality::LOW, &["upper-deck"]), scene.seconds);
    let both: Vec<f32> = heard.left.iter().zip(&heard.right).map(|(l, r)| (l + r) / 2.0).collect();
    let passage = envelope(&both, RATE, 0.5).smoothed(2.0).passage().unwrap();
    assert!((passage.time - abeam).abs() < 5.0, "loudest at {} s for {abeam} s", passage.time);
    assert!(passage.rise_db > 10.0 && passage.fall_db > 8.0, "{passage:?}");
    let side = cue(&heard.left, &heard.right, RATE, &Band::new(500.0, 8_000.0), abeam - 3.0, abeam + 3.0);
    assert!(side.ild_db > 1.0, "right minus left {:+.1} dB as it passes", side.ild_db);
    // From afar two emitters stand for its thirteen: at the start and at the end fewer voices are made.
    assert!(heard.tally.voices < 20 * heard.tally.blocks);
}

#[test]
fn cars_pass_on_the_right_and_a_weak_device_hears_the_same_scene_for_less() {
    let scene = Scene::bundled();
    // The first car of the near lane is level with the listener at 2.6 s.
    let cars = render(&scene, &options(Quality::HIGH, &["lane-"]), 5.0);
    let near = cue(&cars.left, &cars.right, RATE, &Band::new(500.0, 8_000.0), 2.1, 3.1);
    assert!(near.ild_db > 3.0, "right minus left {:+.1} dB", near.ild_db);
    // Every quality hears the car there; the lower ones follow fewer ways.
    let whole = |quality: Quality| render(&scene, &options(quality, &[]), 5.0);
    let (low, high) = (whole(Quality::LOW), whole(Quality::HIGH));
    // A way that is dropped is rendered one block more, fading: a few over the budget when contacts
    // (two wheels and a step in one block) take the place of as many quieter ways.
    assert!(low.tally.most_ways <= Quality::LOW.ways + 8, "{} ways at once", low.tally.most_ways);
    assert!(high.tally.most_ways > low.tally.most_ways + 8, "{} ways at once", high.tally.most_ways);
    assert!(low.tally.ways <= Quality::LOW.ways as u64 * low.tally.blocks + low.tally.blocks / 4);
    let loud = |r: &osage_paris_soundscape::render::Rendered| envelope(&r.right, RATE, 0.5).peak().unwrap();
    assert!((loud(&low).0 - loud(&high).0).abs() < 1.0, "the car peaks at {} s and {} s", loud(&low).0, loud(&high).0);
    assert!((loud(&low).1 - loud(&high).1).abs() < 4.0, "{} dB and {} dB", loud(&low).1, loud(&high).1);
}

#[test]
fn for_speakers_the_sides_are_kept_and_the_ears_are_not_delayed() {
    let scene = Scene::bundled();
    let speakers = Options { listening: Listening::Speakers, ..options(Quality::HIGH, &["river"]) };
    let heard = render(&scene, &speakers, 4.0);
    let wide = cue(&heard.left, &heard.right, RATE, &Band::new(500.0, 8_000.0), 0.0, 4.0);
    assert!(wide.ild_db < -3.0, "{:+.1} dB", wide.ild_db);
    let low = cue(&heard.left, &heard.right, RATE, &Band::new(100.0, 400.0), 0.0, 4.0);
    // Level only: the same difference low and high, where a head makes almost none low.
    assert!(low.ild_db < -3.0, "{:+.1} dB", low.ild_db);
}
