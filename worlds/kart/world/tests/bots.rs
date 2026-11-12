// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The world's AI drivers race the real circuit: every one of them laps, and
//! the field stays on the road, contacts and mistakes included.

use osage_kart_physics::{DT, Ground, bump, step, wheels};
use osage_kart_racecraft::{Mind, Temper, drive, field};
use osage_world_kart::Circuit;

#[test]
fn eight_drivers_race_the_circuit_and_keep_to_the_road() {
    let circuit = Circuit::new();
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
