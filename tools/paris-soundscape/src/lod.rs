// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! `paris-soundscape lod`: what each level of detail of a recorded voice
//! keeps and saves (issue #22). The kart's engine loops, as the recorded
//! bridge has them:
//!
//! - their levels: how long, how many bytes, up to what frequency each holds;
//! - what a loop voice costs a sample at each level, with two loops
//!   crossfaded and with one;
//! - a loop going away from 10 m to 900 m at high quality, beside the same
//!   with every level of detail held at its best: in each stretch of
//!   distance, the level the air allows and how far apart the two are, by
//!   third of an octave below what the air leaves;
//! - the recorded bridge at high quality, with and without levels of detail:
//!   what the voices cost.

use std::{path::Path, sync::Arc};

use osage_sound::{
    bench::cpu_seconds,
    ears::Listening,
    engine::{Options, Quality, RATE},
    render::{Rendered, render},
    scene::Scene,
    ways::air_loss,
};
use osage_sound_analysis::{Band, band_levels};
use osage_sound_voices::{
    Drive, Voice,
    recorded::{Clip, FEW, Follows, LEVELS, Loop, holds, level_for},
};

const RECORDED: &str = include_str!("../scenes/bir-hakeim-recorded.json");

/// The engine's rule for what is worth making: the frequency at which the
/// straight way's air takes 24 dB (as `engine::top_for`), at 20 °C and 70 %.
fn top(metres: f64, celsius: f64, humidity: f64) -> f64 {
    let mut hz = 500.0f64;
    while hz < 24_000.0 && air_loss(hz, celsius, humidity) * metres < 24.0 {
        hz *= 2f64.powf(1.0 / 24.0);
    }
    hz.min(24_000.0)
}

fn thirds(x: &[f32], top: f64) -> Vec<f64> {
    let bands: Vec<Band> = (0..)
        .map(|k| 100.0 * 2f64.powf(f64::from(k) / 3.0))
        .take_while(|&hz| hz * 2f64.powf(1.0 / 6.0) <= top)
        .map(|hz| Band::new(hz / 2f64.powf(1.0 / 6.0), hz * 2f64.powf(1.0 / 6.0)))
        .collect();
    band_levels(x, RATE, &bands)
}

/// Nanoseconds a sample of a loop voice of `clips`, held at `state`, told
/// `top` and `detail`; the median of three runs of 10 s.
fn cost(clips: &[(Arc<Clip>, f32)], state: f32, top: f32, detail: f32) -> f64 {
    let mut runs: Vec<f64> = (0..3)
        .map(|_| {
            let mut voice = Loop::new(7, clips.to_vec(), Follows::Load, None);
            let mut out = [0.0f32; 128];
            let drive = Drive { load: state, top, detail, ..Drive::default() };
            let blocks = 10 * RATE as usize / 128;
            let started = cpu_seconds();
            for _ in 0..blocks {
                voice.render(&drive, &mut out);
                std::hint::black_box(&out);
            }
            (cpu_seconds() - started) * 1e9 / (blocks * 128) as f64
        })
        .collect();
    runs.sort_by(f64::total_cmp);
    runs[1]
}

pub fn run(assets: &Path) -> Result<(), String> {
    let mut bridge = Scene::from_json(RECORDED)?;
    bridge.load_clips(assets)?;
    let names = ["engine-idle", "engine-mid", "engine-high"];
    let clips: Vec<Arc<Clip>> = names
        .iter()
        .map(|name| bridge.clips[*name].clip.clone().ok_or_else(|| format!("{name}: not read")))
        .collect::<Result<_, _>>()?;

    println!("## The kart's engine loops and their levels\n");
    print!("| Loop | Seconds |");
    for k in 0..LEVELS {
        print!(" Level {k}: holds to |");
    }
    println!(" Bytes, level 0 | Bytes, all levels |");
    println!("|---|---:|{}---:|---:|", "---:|".repeat(LEVELS));
    for (name, clip) in names.iter().zip(&clips) {
        let (first, all) = clip.bytes();
        print!("| {name} | {:.2} |", clip.seconds());
        for k in 0..LEVELS {
            print!(" {:.0} Hz |", holds(k));
        }
        println!(" {first} | {all} |");
    }

    println!("\n## What a loop voice costs a sample\n");
    println!(
        "Three loops at load 0, 0.5 and 1, held at 0.515: two crossfaded, or with little detail the nearest alone"
    );
    println!("(the other is 27 dB down there).\n");
    println!("| Level | Two loops, ns | One loop, ns |");
    println!("|---|---:|---:|");
    let set: Vec<(Arc<Clip>, f32)> = clips.iter().cloned().zip([0.0, 0.5, 1.0]).collect();
    for k in 0..LEVELS {
        // Well under what level k holds: the voice plays level k.
        let told = holds(k) * 0.8;
        debug_assert_eq!(level_for(told, 1.0), k);
        println!("| {k} | {:.1} | {:.1} |", cost(&set, 0.515, told, 1.0), cost(&set, 0.515, told, FEW * 0.5));
    }

    println!("\n## A loop going away, beside the same at its best\n");
    println!(
        "The loops on a node going away at 30 m/s from 10 m, at high quality and dry, against the same with every"
    );
    println!(
        "voice at full detail. A tenth of a second at a time, each third of an octave from 100 Hz up to what the air"
    );
    println!(
        "leaves (24 dB) and within 50 dB of the loudest third. At a load of 0.5 one recording is heard and only the"
    );
    println!("levels of rate act; at 0.515 the second is 27 dB down, and with little detail it is left out.\n");
    let mut worst = 0.0f64;
    for load in [0.5, 0.515] {
        let mut going = Scene::from_json(&away(30.0, load))?;
        for (name, clip) in names.iter().zip(&clips) {
            going.set_clip(name, clip.clone());
        }
        let dry = Quality { reverb: false, ..Quality::HIGH };
        let options = |quality: Quality| Options { quality, listening: Listening::Speakers, ..Options::default() };
        let (lower, whole): (Rendered, Rendered) = (
            render(&going, &options(dry), going.seconds),
            render(&going, &options(Quality { full_detail: true, ..dry }), going.seconds),
        );
        // The same loops shipped at 12 kHz: level 2 and below, a quarter of the bytes.
        let mut shipped = going.clone();
        for (name, clip) in names.iter().zip(&clips) {
            shipped.set_clip(name, Clip::new(clip.level(2), RATE >> 2, true));
        }
        let quarter = render(&shipped, &options(dry), shipped.seconds);
        println!("Load {load}:\n");
        println!(
            "| From, m | To, m | The air leaves, Hz | Level | Loops heard | Most apart, dB | Shipped at 12 kHz, most apart, dB |"
        );
        println!("|---:|---:|---:|---:|---|---:|---:|");
        for (from, to) in [(10.0, 50.0), (50.0, 100.0), (100.0, 200.0), (200.0, 400.0), (400.0, 600.0), (600.0, 880.0)]
        {
            let (mut most, mut most_shipped) = (0.0f64, 0.0f64);
            let (t0, t1) = ((from - 10.0) / 30.0, (to - 10.0) / 30.0);
            let mut t = t0;
            while t + 0.1 <= t1 {
                let distance = 10.0 + 30.0 * (t + 0.1);
                let leaves = top(distance, going.air.celsius, going.air.humidity);
                let span =
                    |x: &[f32]| x[(t * f64::from(RATE)) as usize..((t + 0.1) * f64::from(RATE)) as usize].to_vec();
                let b = thirds(&span(&whole.left), leaves);
                // Only the thirds the ear can still tell: within 50 dB of the loudest (a recording's top
                // octave, cut by its encoding, is near nothing).
                let loudest = b.iter().copied().fold(f64::MIN, f64::max);
                let apart = |x: &[f32]| {
                    thirds(&span(x), leaves)
                        .iter()
                        .zip(&b)
                        .filter(|(_, y)| **y > loudest - 50.0)
                        .map(|(x, y)| (x - y).abs())
                        .fold(0.0f64, f64::max)
                };
                most = most.max(apart(&lower.left));
                most_shipped = most_shipped.max(apart(&quarter.left));
                t += 0.1;
            }
            worst = worst.max(most);
            let leaves = top(to, going.air.celsius, going.air.humidity);
            let heard =
                if (25.0 / to).clamp(0.15, 1.0) >= f64::from(FEW) || load == 0.5 { "as at full detail" } else { "one" };
            println!(
                "| {from:.0} | {to:.0} | {leaves:.0} | {} | {heard} | {most:.2} | {most_shipped:.2} |",
                level_for(leaves as f32, if load == 0.5 { 1.0 } else { 1.03 })
            );
        }
        println!();
    }
    println!("Most apart anywhere: {worst:.2} dB.");

    println!("\n## The recorded bridge at high quality\n");
    println!("| | ms a second of sound | Voices, ms | Voices at once |");
    println!("|---|---:|---:|---:|");
    for (name, quality) in [
        ("levels of detail", Quality::HIGH),
        ("every voice at its best", Quality { full_detail: true, ..Quality::HIGH }),
    ] {
        let mut runs: Vec<(f64, f64, f64)> = (0..3)
            .map(|_| {
                let started = cpu_seconds();
                let made = render(&bridge, &Options { quality, ..Options::default() }, 20.0);
                let seconds = cpu_seconds() - started;
                let voices = made.tally.voices as f64 / made.tally.blocks.max(1) as f64;
                (seconds * 1e3 / 20.0, made.tally.voicing * 1e3 / 20.0, voices)
            })
            .collect();
        runs.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (ms, voicing, voices) = runs[1];
        println!("| {name} | {ms:.0} | {voicing:.1} | {voices:.1} |");
    }
    Ok(())
}

/// A scene of the three loops on a node going away along −Z at `speed`
/// m/s from 10 m ahead, for 29 s, at `load`.
fn away(speed: f64, load: f64) -> String {
    format!(
        r#"{{
      "asset": {{"version": "2.0", "extras": {{
        "osageObject": {{"version": 1, "id": "away", "collisionMaterials": {{}}, "acousticMaterials": {{}}}},
        "osageSound": {{"version": 0, "contactSounds": {{}}, "clips": {{
          "engine-idle": {{"uri": "freesound/kart/engine-idle.ogg", "loop": true}},
          "engine-mid": {{"uri": "freesound/kart/engine-mid.ogg", "loop": true}},
          "engine-high": {{"uri": "freesound/kart/engine-high.ogg", "loop": true}}}}}}}}}},
      "scene": 0,
      "scenes": [{{"nodes": [0], "extras": {{"osageSound": {{
        "seconds": 29, "seed": 6, "listener": {{"node": ["world", "ears"]}},
        "reverb": {{"secondsT60": [0.5, 0.4, 0.3]}}, "air": {{"celsius": 20, "humidityPercent": 70}}}}}}}}],
      "nodes": [
        {{"children": [1, 2], "extras": {{"osage": {{"id": "world"}}}}}},
        {{"extras": {{"osage": {{"id": "ears"}}}}}},
        {{"translation": [0, 0, -10], "extras": {{"osage": {{"id": "engine"}}, "osageSound": {{
          "motion": {{"alongM": [0, 0, -1], "atS": 0, "speedKmh": {}}},
          "emitters": [{{"id": "loops", "maxDistanceM": 900, "load": {load}, "recorded": {{"follows": "load", "clips": [
            {{"clip": "engine-idle", "at": 0}}, {{"clip": "engine-mid", "at": 0.5}}, {{"clip": "engine-high", "at": 1}}]}}}}]}}}}}}
      ]
    }}"#,
        speed * 3.6,
        load = load
    )
}
