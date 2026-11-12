// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The physics must give the same bits everywhere it runs (D-031): this
//! script's results are recorded in `golden.txt`, checked here natively and
//! by the kart client against the WebAssembly build. Inputs are exactly
//! representable, so that JavaScript passes the same numbers.
//! `UPDATE_GOLDEN=1 cargo test -p osage-kart-physics` rewrites the files.
//! Contacts between karts run on the server only: their script is checked
//! natively, against `golden-contacts.txt`.

use osage_kart_physics::{Controls, Crest, Kart, Track, bump, step};

const GOLDEN: &str = include_str!("golden.txt");
const CONTACTS: &str = include_str!("golden-contacts.txt");

fn bits(kart: &Kart) -> String {
    let fields = [
        kart.x,
        kart.z,
        kart.heading,
        kart.speed,
        kart.slide,
        kart.drift,
        kart.charge,
        kart.boost,
        kart.spin,
        kart.height,
        kart.climb,
        kart.trick,
        kart.stall,
        kart.held,
    ];
    fields.iter().map(|f| format!("{:08x}", f.to_bits())).collect::<Vec<_>>().join(" ")
}

fn check(results: &str, recorded: &str, file: &str) {
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(format!("{}/tests/{file}", env!("CARGO_MANIFEST_DIR")), results).unwrap();
        return;
    }
    assert_eq!(results, recorded, "the physics changed: if on purpose, rerun with UPDATE_GOLDEN=1");
}

/// The script the client test replays: 1200 steps on a 200 m × 100 m loop
/// with two crests on its first side, from a stalled start.
fn run() -> String {
    let track = Track::new(vec![[0.0, 0.0], [0.0, 200.0], [-100.0, 200.0], [-100.0, 0.0]], 6.0, 3.0).with_crests(vec![
        Crest { along: 20.0, rise: 6.0, fall: 4.0, height: 1.0 },
        Crest { along: 34.0, rise: 4.0, fall: 3.0, height: 0.5 },
    ]);
    let mut kart = Kart { z: 10.0, stall: 0.5, ..Kart::default() };
    let mut out = String::new();
    for i in 0..1200u32 {
        let controls = Controls {
            throttle: [1.0, 1.0, 1.0, 0.5, -1.0][(i / 100 % 5) as usize],
            steer: ((i % 9) as f32 - 4.0) / 4.0,
            drift: i / 120 % 2 == 1,
        };
        step(&mut kart, controls, &track);
        // Every second, and every step off level ground (on a crest, in the air).
        if i % 60 == 59 || kart.height > 0.0 {
            out.push_str(&format!("{} {}\n", i + 1, bits(&kart)));
        }
    }
    out
}

/// Four karts on the same loop, driven into one another: rear ends, side
/// by side, a corner knock into a spin, against the barriers.
fn contacts() -> String {
    let track = Track::new(vec![[0.0, 0.0], [0.0, 200.0], [-100.0, 200.0], [-100.0, 0.0]], 6.0, 3.0);
    let mut karts = [
        Kart { z: 10.0, ..Kart::default() },
        Kart { x: -0.75, z: 13.0, ..Kart::default() },
        Kart { x: 1.5, z: 9.0, speed: 6.0, ..Kart::default() },
        Kart { x: -2.0, z: 16.0, heading: 0.5, ..Kart::default() },
    ];
    let mut out = String::new();
    for i in 0..900u32 {
        for (k, kart) in karts.iter_mut().enumerate() {
            let controls = Controls {
                throttle: [1.0, 0.5, 1.0, 1.0, -0.5][((i / 90) as usize + k) % 5],
                steer: (((i + 3 * k as u32) % 7) as f32 - 3.0) / 6.0 + [0.25, -0.25, 0.5, -0.5][k],
                drift: (i / 150 + k as u32).is_multiple_of(3),
            };
            step(kart, controls, &track);
        }
        for a in 0..karts.len() {
            for b in a + 1..karts.len() {
                let (left, right) = karts.split_at_mut(b);
                bump(&mut left[a], &mut right[0]);
            }
        }
        if i % 30 == 29 {
            for kart in &karts {
                out.push_str(&format!("{} {}\n", i + 1, bits(kart)));
            }
        }
    }
    out
}

#[test]
fn the_physics_matches_its_recorded_bits() {
    check(&run(), GOLDEN, "golden.txt");
}

#[test]
fn contacts_match_their_recorded_bits() {
    check(&contacts(), CONTACTS, "golden-contacts.txt");
}
