// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The controlled scenes (`scenes/controlled-*.json`, issue #19): each renders whole, and answers its one
//! question as the geometry says it should. `paris-soundscape against` reads them beside another engine's
//! captures of the same geometries.

use osage_sound::{
    engine::{Options, Quality, RATE},
    render::{Rendered, render},
    scene::Scene,
};
use osage_sound_analysis::{Band, band_levels, cue, db, tone_shift, tone_track};

const SOUND: f64 = 343.0;

fn scene(name: &str) -> Scene {
    let path = format!("{}/scenes/controlled-{name}.json", env!("CARGO_MANIFEST_DIR"));
    Scene::from_json(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

/// The whole scene, raw, at medium quality: what every run of the tests can afford.
fn heard(scene: &Scene) -> Rendered {
    let rendered = render(scene, &Options { quality: Quality::MEDIUM, ..Options::default() }, scene.seconds);
    assert_eq!(rendered.left.len(), (scene.seconds * f64::from(RATE)).round() as usize);
    assert!(rendered.left.iter().chain(&rendered.right).all(|v| v.is_finite()));
    assert!(rendered.peak() > 0.0, "the scene is heard");
    rendered
}

fn span(x: &[f32], from: f64, to: f64) -> &[f32] {
    &x[(from * f64::from(RATE)) as usize..(to * f64::from(RATE)) as usize]
}

fn power(x: &[f32]) -> f64 {
    x.iter().map(|&v| f64::from(v) * f64::from(v)).sum::<f64>() / x.len() as f64
}

#[test]
fn a_noise_going_round_is_on_the_right_at_5_s_and_on_the_left_at_15_s() {
    let r = heard(&scene("circle"));
    let band = Band::new(2_000.0, 8_000.0);
    let right = cue(&r.left, &r.right, RATE, &band, 4.5, 5.5);
    let left = cue(&r.left, &r.right, RATE, &band, 14.5, 15.5);
    let ahead = cue(&r.left, &r.right, RATE, &band, 19.5, 20.0);
    assert!(right.ild_db > 10.0 && right.itd_us > 500.0, "{right:?}");
    assert!(left.ild_db < -10.0 && left.itd_us < -500.0, "{left:?}");
    assert!(ahead.ild_db.abs() < 3.0, "{ahead:?}");
}

#[test]
fn a_tone_passing_at_72_kmh_comes_back_under_1_khz_when_its_nearest_sound_arrives() {
    let r = heard(&scene("passby"));
    let mono: Vec<f32> = r.left.iter().zip(&r.right).map(|(l, r)| 0.5 * (l + r)).collect();
    let track = tone_track(&mono, RATE, &Band::new(800.0, 1_250.0), 0.01);
    let shift = tone_shift(&track, 5.0 + 5.0 / SOUND, 1.0).unwrap();
    // (c − v)/(c + v) far from the passage, at 20 m/s.
    let far = (SOUND - 20.0) / (SOUND + 20.0);
    assert!(shift.before_hz > 1_040.0 && shift.after_hz < 960.0, "{shift:?}");
    assert!((shift.ratio / far - 1.0).abs() < 0.02, "{shift:?} against {far}");
}

#[test]
fn a_tone_300_m_ahead_is_steady_and_on_neither_side() {
    let r = heard(&scene("far"));
    let (l, rr) = (db(power(span(&r.left, 3.0, 5.0))), db(power(span(&r.right, 3.0, 5.0))));
    assert!((l - rr).abs() < 1.0, "{l} {rr}");
    let before = db(power(span(&r.left, 2.0, 3.0)));
    assert!((before - l).abs() < 1.0, "steady: {before} then {l}");
}

#[test]
fn a_knock_arrives_when_sound_from_its_plate_would() {
    let r = heard(&scene("clicks"));
    let onset = |from: f64, side: &[f32]| {
        let first = (from * f64::from(RATE)) as usize;
        let peak = side[first..].iter().fold(0f32, |m, v| m.max(v.abs()));
        (first..side.len()).find(|&i| side[i].abs() >= peak * 0.01).unwrap() as f64 / f64::from(RATE)
    };
    let near = onset(0.5, &r.right);
    let far = onset(2.5, &r.left);
    assert!((near - (1.0 + 34.3 / SOUND)).abs() < 0.005, "{near}");
    assert!((far - (3.0 + 137.2 / SOUND)).abs() < 0.005, "{far}");
}

#[test]
fn a_screen_halfway_takes_more_from_the_top_than_from_the_bottom() {
    let (open, screen) = (heard(&scene("open")), heard(&scene("screen")));
    let octaves = [Band::octave(250.0), Band::octave(4_000.0)];
    let level = |r: &Rendered| band_levels(span(&r.left, 0.5, 5.5), RATE, &octaves);
    let (a, b) = (level(&open), level(&screen));
    let (low, high) = (b[0] - a[0], b[1] - a[1]);
    assert!(low < -6.0 && high < low, "{low} at 250 Hz, {high} at 4 kHz");
}

#[test]
fn a_hard_floor_digs_its_first_dip_where_its_delay_says() {
    let floor = heard(&scene("floor"));
    let mut json: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(format!("{}/scenes/controlled-floor.json", env!("CARGO_MANIFEST_DIR"))).unwrap(),
    )
    .unwrap();
    *json.pointer_mut("/nodes/0/extras/osageSound").unwrap() = serde_json::json!({});
    let bare = heard(&Scene::from_json(&json.to_string()).unwrap());
    let bands: Vec<Band> = (0..60).map(|k| Band::new(100.0 + 4.0 * f64::from(k), 104.0 + 4.0 * f64::from(k))).collect();
    let (a, b) =
        (band_levels(span(&floor.left, 0.5, 5.5), RATE, &bands), band_levels(span(&bare.left, 0.5, 5.5), RATE, &bands));
    let (k, dip) = a.iter().zip(&b).map(|(x, y)| x - y).enumerate().min_by(|p, q| p.1.total_cmp(&q.1)).unwrap();
    let extra = ((9f64 + 16.0).sqrt() - 17f64.sqrt()) / SOUND;
    let at = 102.0 + 4.0 * k as f64;
    assert!((at - 0.5 / extra).abs() < 12.0 && dip < -8.0, "{dip} dB at {at} Hz, against {}", 0.5 / extra);
}
