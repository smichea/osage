// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The physics must give the same bits everywhere it runs (D-031): this
//! script's results are recorded in `golden.txt`, checked here natively and
//! by the kart client against the WebAssembly build. Inputs are exactly
//! representable, so that JavaScript passes the same numbers.
//! `UPDATE_GOLDEN=1 cargo test -p osage-kart-physics` rewrites the file.

use osage_kart_physics::{Controls, Kart, Track, step};

const GOLDEN: &str = include_str!("golden.txt");

/// The script the client test replays: 1200 steps on a 200 m × 100 m loop.
fn run() -> String {
    let track = Track::new(vec![[0.0, 0.0], [0.0, 200.0], [-100.0, 200.0], [-100.0, 0.0]], 6.0, 3.0);
    let mut kart = Kart { z: 10.0, ..Kart::default() };
    let mut out = String::new();
    for i in 0..1200u32 {
        let controls = Controls {
            throttle: [1.0, 1.0, 1.0, 0.5, -1.0][(i / 100 % 5) as usize],
            steer: ((i % 9) as f32 - 4.0) / 4.0,
            drift: i / 120 % 2 == 1,
        };
        step(&mut kart, controls, &track);
        if i % 60 == 59 {
            let fields = [kart.x, kart.z, kart.heading, kart.speed, kart.slide, kart.drift, kart.charge, kart.boost];
            let bits: Vec<String> = fields.iter().map(|f| format!("{:08x}", f.to_bits())).collect();
            out.push_str(&format!("{} {}\n", i + 1, bits.join(" ")));
        }
    }
    out
}

#[test]
fn the_physics_matches_its_recorded_bits() {
    let results = run();
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden.txt"), &results).unwrap();
        return;
    }
    assert_eq!(results, GOLDEN, "the physics changed: if on purpose, rerun with UPDATE_GOLDEN=1");
}
