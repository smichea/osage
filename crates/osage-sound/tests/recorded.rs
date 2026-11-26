// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Voices played from recordings in a scene (issue #22): heard as any voice
//! is, the same twice, standing back for their synthesised voice when their
//! recordings were not read; and from afar, made from a lower level of
//! detail with nothing heard of it below what the air leaves.

use osage_sound::{
    clips,
    ears::Listening,
    engine::{Options, Quality, RATE},
    render::{Rendered, render},
    scene::Scene,
};
use osage_sound_analysis::{Band, band_levels};
use osage_sound_voices::recorded::Clip;

/// A scene of `nodes` (the world first, the ears second), with a loop
/// `hiss` and a shot `knock` it may play, and a plate a wheel can knock on.
fn scene(nodes: &str, seconds: f64, contact: &str) -> Scene {
    let text = format!(
        r#"{{
      "asset": {{"version": "2.0", "extras": {{
        "osageObject": {{"version": 1, "id": "test",
          "collisionMaterials": {{"ground": {{"friction": 0.8, "restitution": 0.0}}, "plate": {{"friction": 0.5, "restitution": 0.0}},
                                 "wheel": {{"friction": 1.0, "restitution": 0.0}}}},
          "acousticMaterials": {{"hard": {{"absorption": [0.0, 0.0, 0.0], "transmission": [0.0, 0.0, 0.0], "scattering": 0.0}}}}}},
        "osageSound": {{"version": 0,
          "clips": {{"hiss": {{"uri": "test/hiss.wav", "loop": true}}, "tone": {{"uri": "test/tone.ogg", "loop": true}},
                    "knock": {{"uri": "test/knock.wav"}}}},
          "contactSounds": {{"ground": {{"hardness": 0.5}}, "plate": {contact}, "wheel": {{"hardness": 1.0}}}}}}}}}},
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

const PLATE: &str = r#"{"params": {"tone": 400, "ring": 0.3, "shape": 1, "scuff": 0.0}, "hardness": 1.0}"#;

/// A seamless loop of white noise, a second long.
fn hiss() -> Clip {
    let mut state = 12_345u32;
    let samples: Vec<f32> = (0..RATE as usize)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (f64::from(state) / f64::from(u32::MAX) * 2.0 - 1.0) as f32 * 0.3
        })
        .collect();
    Clip::new(&samples, RATE, true)
}

/// A seamless 1 kHz tone, half a second.
fn tone() -> Clip {
    let samples: Vec<f32> =
        (0..24_000).map(|i| (0.3 * (std::f64::consts::TAU * 1_000.0 * f64::from(i) / 48_000.0).sin()) as f32).collect();
    Clip::new(&samples, RATE, true)
}

fn loaded(mut scene: Scene) -> Scene {
    assert!(scene.set_clip("hiss", hiss()) && scene.set_clip("tone", tone()));
    let knock: Vec<f32> = (0..4_800).map(|i| 0.5 * (-(i as f32) / 600.0).exp() * (i as f32 * 0.2).sin()).collect();
    assert!(scene.set_clip("knock", Clip::new(&knock, RATE, false)));
    scene
}

const WORLD: &str = r#"{"children": [1, 2], "extras": {"osage": {"id": "world"}}}"#;
const EARS: &str = r#"{"extras": {"osage": {"id": "ears"}}}"#;

/// An emitter `metres` ahead, of `sound` (its "voice" and "recorded").
fn ahead(metres: f64, sound: &str, moving: &str) -> String {
    format!(
        r#"{{"translation": [0, 0, {}], "extras": {{"osage": {{"id": "thing"}}, "osageSound": {{{moving}
             "emitters": [{{"id": "sound", {sound}, "maxDistanceM": 900}}]}}}}}}"#,
        -metres
    )
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

fn options(quality: Quality) -> Options {
    Options { quality, listening: Listening::Speakers, ..Options::default() }
}

fn heard(scene: &Scene, quality: Quality) -> Rendered {
    render(scene, &options(quality), scene.seconds)
}

#[test]
fn a_recorded_tone_is_heard_as_the_same_tone_synthesised() {
    // A 1 kHz loop, brought to the voices' level, against the tone voice at 1 kHz: as loud, the same pitch,
    // and the same samples twice.
    let recorded = loaded(scene(
        &[WORLD, EARS, &ahead(10.0, r#""recorded": {"clips": [{"clip": "tone", "at": 0}]}"#, "")].join(","),
        1.0,
        PLATE,
    ));
    let synthesised =
        scene(&[WORLD, EARS, &ahead(10.0, r#""voice": "tone", "params": {"hz": 1000}"#, "")].join(","), 1.0, PLATE);
    let (one, other) = (heard(&recorded, Quality::REFERENCE), heard(&synthesised, Quality::REFERENCE));
    let (a, b) = (rms(&one.left[24_000..]), rms(&other.left[24_000..]));
    assert!((a / b - 1.0).abs() < 0.01, "{a} against {b}");
    let rises = |x: &[f32]| x.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
    assert!(rises(&one.left[24_000..]).abs_diff(rises(&other.left[24_000..])) <= 1);
    assert!(heard(&recorded, Quality::REFERENCE).left == one.left);
}

#[test]
fn recordings_not_read_leave_their_synthesised_voice_or_silence() {
    let with_layer = r#""voice": "tone", "params": {"hz": 500}, "recorded": {"clips": [{"clip": "hiss", "at": 0}], "layerGainDb": -6}"#;
    let unread = scene(&[WORLD, EARS, &ahead(10.0, with_layer, "")].join(","), 0.5, PLATE);
    let alone =
        scene(&[WORLD, EARS, &ahead(10.0, r#""voice": "tone", "params": {"hz": 500}"#, "")].join(","), 0.5, PLATE);
    assert!(heard(&unread, Quality::HIGH).left == heard(&alone, Quality::HIGH).left);
    let bare = scene(
        &[WORLD, EARS, &ahead(10.0, r#""recorded": {"clips": [{"clip": "hiss", "at": 0}]}"#, "")].join(","),
        0.5,
        PLATE,
    );
    assert_eq!(heard(&bare, Quality::HIGH).peak(), 0.0);
    // Read, the recording is heard with the tone under it, 6 dB down: their powers add.
    let read = heard(&loaded(unread), Quality::HIGH);
    let only = heard(&loaded(bare), Quality::HIGH);
    let power = |r: &Rendered| rms(&r.left[12_000..]).powi(2);
    let expected = power(&only) + 0.25 * power(&heard(&alone, Quality::HIGH));
    assert!((power(&read) / expected - 1.0).abs() < 0.05, "{} for {expected}", power(&read));
}

#[test]
fn a_recorded_knock_is_heard_where_and_when_the_wheel_strikes() {
    // A wheel at 36 km/h comes onto a plate 20 m to the right at 1 s: the plate plays its recording.
    let world = r#"{"children": [1, 2], "extras": {"osage": {"id": "world"}, "osageSound": {"surfaces": [
        {"id": "ground", "quadM": [[-50, 0, -50], [-50, 0, 50], [50, 0, 50], [50, 0, -50]], "collision": "ground"},
        {"id": "plate", "quadM": [[20, 0.01, -3], [20, 0.01, 3], [21, 0.01, 3], [21, 0.01, -3]], "collision": "plate", "strikes": true}]}}}"#;
    let ears = r#"{"translation": [0, 1.6, 0], "extras": {"osage": {"id": "ears"}}}"#;
    let wheel = r#"{"translation": [10, 0, 0], "extras": {"osage": {"id": "wheel"}, "osageSound": {
        "motion": {"alongM": [1, 0, 0], "atS": 0, "speedKmh": 36},
        "touches": [{"kind": "wheels", "material": "wheel", "pointsM": [[0, 0, 0]], "strength": 1.0}]}}}"#;
    let recorded = r#"{"hardness": 1.0, "recorded": {"clips": ["knock"]}}"#;
    let played = loaded(scene(&[world, ears, wheel].join(","), 1.5, recorded));
    let made = heard(&played, Quality::REFERENCE);
    assert_eq!(made.tally.contacts, 1);
    let peak = made.right.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let first = made.right.iter().position(|s| s.abs() > 0.05 * peak).unwrap() as f64 / f64::from(RATE);
    let expected = 1.0 + (20.0f64 * 20.0 + 1.6 * 1.6).sqrt() / 343.0;
    assert!(peak > 1e-3 && (first - expected).abs() < 0.004, "heard at {first} s for {expected} s, peak {peak}");
    assert!(rms(&made.right[..48_000]) == 0.0);
    assert!(heard(&played, Quality::REFERENCE).right == made.right);
}

/// Third-octave levels, dB, from 100 Hz to `top`, of `x` between two instants.
fn thirds(x: &[f32], from: f64, to: f64, top: f64) -> Vec<f64> {
    let bands: Vec<Band> = (0..)
        .map(|k| 100.0 * 2f64.powf(f64::from(k) / 3.0))
        .take_while(|&hz| hz * 2f64.powf(1.0 / 6.0) <= top)
        .map(|hz| Band::new(hz / 2f64.powf(1.0 / 6.0), hz * 2f64.powf(1.0 / 6.0)))
        .collect();
    let rate = f64::from(RATE);
    band_levels(&x[(from * rate) as usize..(to * rate) as usize], RATE, &bands)
}

#[test]
fn from_afar_a_lower_level_is_heard_as_the_whole_recording_below_what_the_air_leaves() {
    // A noise loop 300 m ahead: at high quality the voice is told what the air leaves, and plays a level of
    // half the rate; at reference, the whole. Below 4 kHz, every third of an octave within half a dB.
    let far = loaded(scene(
        &[WORLD, EARS, &ahead(300.0, r#""recorded": {"clips": [{"clip": "hiss", "at": 0}]}"#, "")].join(","),
        3.0,
        PLATE,
    ));
    let dry = Quality { reverb: false, ..Quality::HIGH };
    let (lower, whole) = (heard(&far, dry), heard(&far, Quality { full_detail: true, ..dry }));
    assert!(lower.left != whole.left, "the level changed nothing");
    let (a, b) = (thirds(&lower.left, 1.5, 3.0, 4_000.0), thirds(&whole.left, 1.5, 3.0, 4_000.0));
    for (k, (x, y)) in a.iter().zip(&b).enumerate() {
        assert!((x - y).abs() < 0.5, "third {k}: {x:.2} dB against {y:.2} dB");
    }
}

#[test]
fn a_noise_passing_changes_levels_with_no_step_in_any_band() {
    // A noise loop at 108 km/h from 450 m ahead to 450 m behind, 15 s: its level of detail falls and rises
    // as it comes and goes. A tenth of a second at a time, below what the air leaves at its farthest, it is
    // within a dB of the whole recording heard alike: no step where the level changes.
    let moving = r#""motion": {"alongM": [0, 0, 1], "atS": 0, "speedKmh": 108},"#;
    let pass = loaded(scene(
        &[WORLD, EARS, &ahead(450.0, r#""recorded": {"clips": [{"clip": "hiss", "at": 0}]}"#, moving)].join(","),
        15.0,
        PLATE,
    ));
    let dry = Quality { reverb: false, ..Quality::HIGH };
    let (lower, whole) = (heard(&pass, dry), heard(&pass, Quality { full_detail: true, ..dry }));
    let mut worst = 0.0f64;
    for k in 15..148 {
        let (from, to) = (f64::from(k) * 0.1, f64::from(k + 1) * 0.1);
        let (a, b) = (thirds(&lower.left, from, to, 3_000.0), thirds(&whole.left, from, to, 3_000.0));
        for (x, y) in a.iter().zip(&b) {
            worst = worst.max((x - y).abs());
        }
    }
    assert!(worst < 1.0, "{worst:.2} dB apart at worst");
}

#[test]
fn a_recording_is_read_from_ogg_and_wav() {
    // A WAV file of a tone, read as a loop.
    let samples: Vec<i16> = (0..4_800).map(|i| ((i as f32 * 0.1).sin() * 8_000.0) as i16).collect();
    let bytes = osage_sound_analysis::wav::pcm16(48_000, 1, &samples);
    let clip = clips::from_wav(&bytes, true).unwrap();
    assert_eq!(clip.level(0).len(), 4_800);
    assert!(clips::from_ogg(&bytes, true).is_err());
    // The kart's recordings, when the assets are here.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/freesound/kart/engine-idle.ogg");
    match std::fs::read(&root) {
        Ok(ogg) => {
            let clip = clips::from_ogg(&ogg, true).unwrap();
            assert!(clip.seconds() > 1.0, "{} s", clip.seconds());
        }
        Err(_) => eprintln!("{}: not fetched, the Ogg file not read", root.display()),
    }
}
