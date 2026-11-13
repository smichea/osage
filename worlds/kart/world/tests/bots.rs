// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The world's AI drivers race the real circuit: every one of them laps, and
//! the field stays on the road, contacts and mistakes included.

use osage_kart_physics::{DT, Ground, bump, step, wheels};
use osage_kart_racecraft::{Mind, Temper, drive, field};
use osage_world_kart::Circuit;

#[test]
fn eight_drivers_race_the_circuit_and_keep_to_the_road() {
    race_and_keep_to_the_road(&Circuit::new());
}

/// G3.3: the field laps every track that comes with the game and keeps to
/// its road (D-056).
#[test]
fn eight_drivers_race_every_track_and_keep_to_the_road() {
    for def in osage_kart_tracks::built_in().iter().filter(|def| def.id != "paddock") {
        eprintln!("{}:", def.id);
        race_and_keep_to_the_road(&Circuit::of(def));
    }
}

/// D-068: in the rain, on every track, the field still laps and keeps
/// to the road, if a little more often on the grass.
#[test]
fn eight_drivers_race_in_the_rain_and_keep_to_the_road() {
    for def in osage_kart_tracks::built_in() {
        eprintln!("{} in the rain:", def.id);
        let mut circuit = Circuit::of(&def);
        circuit.track.wet = 1.0;
        race_and_keep_to_the_road(&circuit);
    }
}

fn race_and_keep_to_the_road(circuit: &Circuit) {
    let track = &circuit.track;
    let tempers: Vec<Temper> = [Temper::VETERAN, Temper::ROOKIE, Temper::HOTHEAD, Temper::STEADY, Temper::CAUTIOUS]
        .into_iter()
        .chain((5..8).map(Temper::of))
        .collect();
    let mut karts: Vec<_> = (0..8).map(|slot| circuit.grid_slot(slot)).collect();
    let mut minds: Vec<_> = (0..8).map(|i| Mind::new(i + 1)).collect();
    let mut travelled = [0.0f32; 8];
    let (mut grass, mut steps) = (0usize, 0usize);
    for _ in 0..(90.0 / DT) as usize {
        let field = field(track, &karts);
        let controls: Vec<_> = (0..8).map(|i| drive(&karts[i], i, &field, &tempers[i], &mut minds[i], track)).collect();
        for (kart, controls) in karts.iter_mut().zip(controls) {
            step(kart, controls, track);
        }
        for a in 0..8 {
            for b in a + 1..8 {
                let (left, right) = karts.split_at_mut(b);
                bump(&mut left[a], &mut right[0]);
            }
        }
        for (i, kart) in karts.iter().enumerate() {
            let along = track.locate(kart.x, kart.z).along;
            let moved = (along - field[i].spot.along).rem_euclid(track.length());
            travelled[i] += if moved > track.length() / 2.0 { moved - track.length() } else { moved };
            grass += wheels(kart, track).contains(&Ground::Grass) as usize;
            steps += 1;
        }
    }
    let laps: Vec<f32> = travelled.iter().map(|t| t / track.length()).collect();
    assert!(laps.iter().all(|&l| l > 1.5), "every driver laps: {laps:?}");
    let share = grass as f32 / steps as f32;
    assert!(share < 0.05, "the field keeps to the road: {share}");
    let mistakes: u32 = minds.iter().map(Mind::mistakes).sum();
    eprintln!("laps {laps:?}, grass {share}, mistakes {mistakes}");
}

/// D-055: every driver in every kart, alone on the circuit with a steady
/// driver who never errs, laps within a few per cent of the standard
/// kart, and keeps to the road: a build is a flavour, not a ladder.
#[test]
fn every_driver_in_every_kart_laps_within_a_band() {
    use osage_kart_roster::{BODIES, DRIVERS, build};
    let circuit = Circuit::new();
    let track = &circuit.track;
    let temper = Temper { consistency: 1.0, ..Temper::STEADY };
    // Seconds for two laps from the grid: the first from a standing start.
    let two_laps = |kart: osage_kart_physics::Kart| -> (f32, usize) {
        let mut kart = kart;
        let mut mind = Mind::new(1);
        let (mut travelled, mut grass) = (0.0f32, 0usize);
        let mut last = track.locate(kart.x, kart.z).along;
        for i in 0..(150.0 / DT) as usize {
            let field = field(track, &[kart]);
            let controls = drive(&kart, 0, &field, &temper, &mut mind, track);
            step(&mut kart, controls, track);
            let along = track.locate(kart.x, kart.z).along;
            let moved = (along - last).rem_euclid(track.length());
            travelled += if moved > track.length() / 2.0 { moved - track.length() } else { moved };
            last = along;
            grass += wheels(&kart, track).contains(&Ground::Grass) as usize;
            if travelled >= 2.0 * track.length() {
                return (i as f32 * DT, grass);
            }
        }
        panic!("two laps not done: {travelled}");
    };
    let (standard, _) = two_laps(circuit.grid_slot(0));
    let mut times = Vec::new();
    for d in &DRIVERS {
        for b in &BODIES {
            let (time, grass) = two_laps(osage_kart_physics::Kart { build: build(d, b), ..circuit.grid_slot(0) });
            assert!(grass < 30, "{} in a {} ran wide: {grass} steps on the grass", d.name, b.name);
            times.push((time, d.name, b.name));
        }
    }
    times.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (fastest, slowest) = (times[0], times[times.len() - 1]);
    eprintln!("standard {standard:.2} s; fastest {fastest:?}; slowest {slowest:?}");
    for (time, driver, body) in &times {
        eprintln!("  {driver:>6} {body:<8} {time:.2} s ({:+.1} %)", 100.0 * (time / standard - 1.0));
    }
    for (time, driver, body) in &times {
        assert!((time / standard - 1.0).abs() < 0.02, "{driver} in a {body}: {time:.2} s against {standard:.2}");
    }
    assert!(slowest.0 - fastest.0 > 0.05, "the builds differ at all");
}
