// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::{
    replay::{Body, Clock, Grid, Replay, Search},
    scene::Sector,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fmt::Write, fs, hint::black_box, path::Path, time::Instant};

pub fn sha(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

pub fn hex(digest: &[u8]) -> String {
    let mut hex = String::with_capacity(2 * digest.len());
    for byte in digest {
        write!(hex, "{byte:02x}").unwrap();
    }
    hex
}

const CADENCES: [u32; 3] = [30, 60, 144];
const SAMPLES: usize = 61;
const BATCH: usize = 8;

struct Trace {
    csv: String,
    outcomes: Value,
}

fn trace(scene: &Sector, r: &Replay, search: Search, hz: u32) -> Result<Trace> {
    let mut csv =
        String::from("route,tick,foot_x,foot_y,foot_z,vx,vy,vz,eye_x,eye_y,eye_z,aim_x,aim_y,aim_z,support\n");
    let mut outcomes = Vec::new();
    for route in &r.routes {
        let mut body = Body::new(route);
        let mut clock = Clock::default();
        let (mut tick, mut frame) = (0, 0u64);
        let mut record = |tick, body: &Body| {
            let (eye, aim) = route.camera_at(tick);
            write!(csv, "{},{}", route.id, tick).unwrap();
            for v in [body.foot, body.velocity, eye, aim] {
                write!(csv, ",{},{},{}", v.x, v.y, v.z).unwrap();
            }
            let name = body.landed.map_or("", |(_, i)| scene.supports[i].id.as_str());
            writeln!(csv, ",{name}").unwrap();
        };
        record(0, &body);
        while tick < route.ticks {
            // Generate display timestamps on the rational frame grid, including the remainder.
            let ns = |f: u64| u128::from(f) * 1_000_000_000 / u128::from(hz);
            let due = clock.advance(ns(frame + 1) - ns(frame), r.tick_hz);
            frame += 1;
            for _ in 0..due.min(route.ticks - tick) {
                tick += 1;
                body.step(tick, r, &scene.supports, search);
                record(tick, &body);
            }
        }
        let (hit_tick, support) = body.landed.context("route ended in the air")?;
        let id = &scene.supports[support].id;
        ensure!(*id == route.expected_support, "{} reached {id}, expected {}", route.id, route.expected_support);
        outcomes.push(
            json!({"id":route.id,"ticks":tick,"contact_tick":hit_tick,"support":id,"foot_m":body.foot.to_array()}),
        );
    }
    Ok(Trace { csv, outcomes: json!(outcomes) })
}

fn verified_trace(scene: &Sector, r: &Replay, grid: &Grid) -> Result<Trace> {
    let control = trace(scene, r, Search::Full, r.tick_hz)?;
    for hz in CADENCES {
        let other = trace(scene, r, Search::Grid(grid), hz)?;
        ensure!(other.csv == control.csv && other.outcomes == control.outcomes, "divergent replay at {hz} Hz");
    }
    Ok(control)
}

/// Only stepping the drops is inside this timer. Record the queries so a result
/// can be explained on machines with different clocks or different load.
fn suite(scene: &Sector, r: &Replay, search: Search) -> usize {
    let mut checks = 0;
    for route in &r.routes {
        let mut body = Body::new(route);
        for tick in 1..=route.ticks {
            checks += body.step(tick, r, &scene.supports, search);
        }
        black_box(body);
    }
    checks
}

fn time_batch(scene: &Sector, r: &Replay, search: Search) -> u64 {
    let start = Instant::now();
    for _ in 0..BATCH {
        black_box(suite(black_box(scene), black_box(r), black_box(search)));
    }
    (start.elapsed().as_nanos() / BATCH as u128) as u64
}

pub fn distribution(mut ns: Vec<u64>) -> Value {
    ns.sort_unstable();
    let at = |p: usize| ns[(p * ns.len()).div_ceil(100) - 1];
    json!({"p50":at(50),"p95":at(95),"p99":at(99),"samples":ns.len()})
}

pub fn run(dir: &Path, scene: &Sector, r: &Replay, inputs: &Value) -> Result<()> {
    let start = Instant::now();
    let grid = Grid::new(&scene.supports);
    let build_ns = start.elapsed().as_nanos() as u64;
    let control = verified_trace(scene, r, &grid)?;
    let modes = [Search::Full, Search::Grid(&grid)];
    let mut samples = [Vec::new(), Vec::new()];
    for i in 0..SAMPLES + 6 {
        for j in [i % 2, 1 - i % 2] {
            let ns = time_batch(scene, r, modes[j]);
            if i >= 6 {
                samples[j].push(ns);
            }
        }
    }
    let [full, spatial] = samples;
    let cpu = fs::read_to_string("/proc/cpuinfo").ok().and_then(|s| {
        s.lines().find_map(|l| l.strip_prefix("model name")?.split_once(':').map(|(_, v)| v.trim().to_owned()))
    });
    let code_sha =
        sha(concat!(include_str!("scene.rs"), include_str!("replay.rs"), include_str!("bench.rs")).as_bytes());
    let receipt = json!({
        "format":"ville-paris/drop-cpu/1","inputs":inputs,"kernel_sha256":code_sha,
        "machine":{"cpu":cpu,"os":std::env::consts::OS,"arch":std::env::consts::ARCH,
                   "debug_assertions":cfg!(debug_assertions)},
        "scene":{"instances":scene.instances.len(),"triangles":scene.triangles,"supports":scene.supports.len()},
        "clock_hz":r.tick_hz,"display_hz_checked":CADENCES,"trace_rows":control.csv.lines().count()-1,
        "trace_sha256":sha(control.csv.as_bytes()),"routes":control.outcomes,
        "metric":"nanoseconds per complete suite of routes, CPU stepping only","suites_per_sample":BATCH,
        "grid_build_ns":build_ns,
        "full":{"ns":distribution(full),"candidate_checks":suite(scene,r,modes[0])},
        "grid":{"ns":distribution(spatial),"candidate_checks":suite(scene,r,modes[1])}
    });
    fs::create_dir_all(dir)?;
    fs::write(dir.join("trace.csv"), control.csv)?;
    let rendered = serde_json::to_string_pretty(&receipt)?;
    fs::write(dir.join("receipt.json"), &rendered)?;
    println!("{rendered}");
    Ok(())
}

pub fn verify(dir: &Path, scene: &Sector, r: &Replay, inputs: &Value) -> Result<()> {
    let receipt: Value = serde_json::from_slice(&fs::read(dir.join("receipt.json"))?)?;
    let csv = fs::read(dir.join("trace.csv"))?;
    let expected = verified_trace(scene, r, &Grid::new(&scene.supports))?;
    check_trace(&receipt, inputs, &csv, &expected)?;
    println!("Verified {} states and {} landings", receipt["trace_rows"], r.routes.len());
    Ok(())
}

fn check_trace(receipt: &Value, inputs: &Value, csv: &[u8], expected: &Trace) -> Result<()> {
    ensure!(
        receipt["format"] == "ville-paris/drop-cpu/1" && receipt["inputs"] == *inputs,
        "scene or replay fingerprint mismatch"
    );
    ensure!(receipt["trace_sha256"] == sha(csv), "trace digest mismatch");
    // Both sides pass through the JSON reader. The exact f32 coordinates also live in the CSV.
    let routes: Value = serde_json::from_slice(&serde_json::to_vec(&expected.outcomes)?)?;
    ensure!(
        csv == expected.csv.as_bytes()
            && receipt["routes"] == routes
            && receipt["trace_rows"] == expected.csv.lines().count() - 1,
        "trace or contacts differ from replay"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::Vec3;

    #[test]
    fn verifier_rejects_wrong_inputs_and_a_missing_tick_even_with_a_new_digest() {
        let expected = Trace { csv: "tick\n0\n1\n2\n".into(), outcomes: json!([]) };
        let inputs = json!({"scene_sha256":"s","replay_sha256":"r"});
        let mut receipt = json!({"format":"ville-paris/drop-cpu/1","inputs":inputs,"routes":[],
            "trace_rows":3,"trace_sha256":sha(expected.csv.as_bytes())});
        assert!(check_trace(&receipt, &inputs, expected.csv.as_bytes(), &expected).is_ok());
        assert!(check_trace(&receipt, &json!({}), expected.csv.as_bytes(), &expected).is_err());
        let missing = b"tick\n0\n2\n";
        receipt["trace_sha256"] = json!(sha(missing));
        receipt["trace_rows"] = json!(2);
        assert!(check_trace(&receipt, &inputs, missing, &expected).is_err());
    }

    #[test]
    fn identical_at_three_cadences_and_on_both_decks() {
        let scene = Sector::read(crate::scene::DEFAULT).unwrap();
        let replay = Replay::read(crate::replay::DEFAULT, scene.supports.iter().map(|s| s.id.as_str())).unwrap();
        let t = verified_trace(&scene, &replay, &Grid::new(&scene.supports)).unwrap();
        let p = |i: usize| Vec3::from_array(serde_json::from_value(t.outcomes[i]["foot_m"].clone()).unwrap());
        assert!((p(0) - p(1)).with_y(0.).length() < 0.02);
        assert_eq!((p(0).y, p(1).y), (16., 8.));
    }
}
