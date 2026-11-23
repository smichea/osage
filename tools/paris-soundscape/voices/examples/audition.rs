// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Renders one voice alone to a mono WAV file, to listen to it and to look
//! at it without the scene (as `osage-kart-sound`'s `lap`):
//!
//!     cargo run -p osage-paris-voices --example audition -- car car.wav --seconds 8 --speed 30
//!     cargo run -p osage-paris-voices --example audition -- --list
//!
//! `--speed` and `--to` are km/h (the speed moves from one to the other over
//! the file), `--load` and `--detail` 0…1, `--hits` blows a second,
//! `--set name=value` a parameter of the voice. Prints the level, the peak and the level in each
//! octave.

use osage_paris_voices::{Drive, Hit, KINDS, Params, RATE, dsp::Biquad, make};

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "--list") {
        for kind in KINDS {
            println!("{:<12} {}", kind.name, kind.about);
            for (name, default) in kind.params {
                println!("{:<12}   {name} = {default}", "");
            }
        }
        return Ok(());
    }
    let value = |flag: &str, default: f32| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let (name, path) = (args[0].as_str(), args.get(1).map_or("audition.wav", String::as_str));
    let (seconds, seed) = (value("--seconds", 8.0), value("--seed", 6.0) as u32);
    let from = value("--speed", 30.0) / 3.6;
    let to = value("--to", from * 3.6) / 3.6;
    let (load, hits, detail) = (value("--load", 0.5), value("--hits", 0.0), value("--detail", 1.0));
    let mut params = Params::default();
    for (i, arg) in args.iter().enumerate() {
        if arg == "--set"
            && let Some((key, number)) = args.get(i + 1).and_then(|pair| pair.split_once('='))
        {
            params.set(key, number.parse().expect("--set name=number"));
        }
    }
    let Some(mut voice) = make(name, seed, &params) else {
        eprintln!("no voice named {name}: try --list");
        std::process::exit(2);
    };

    let mut samples = vec![0.0f32; (seconds * RATE as f32) as usize];
    let (blocks, mut since_hit) = (samples.len().div_ceil(128), 0.0f32);
    for (i, block) in samples.chunks_mut(128).enumerate() {
        let t = i as f32 / blocks as f32;
        let speed = from + (to - from) * t;
        since_hit += hits * 128.0 / RATE as f32;
        let hit = (since_hit >= 1.0).then(|| {
            since_hit -= 1.0;
            Hit { at: 0, strength: 1.0 }
        });
        let accel = (to - from) / seconds;
        voice.render(&Drive { speed, accel, load, hit, detail }, block);
    }

    let rms = |xs: &[f32]| (xs.iter().map(|s| s * s).sum::<f32>() / xs.len().max(1) as f32).sqrt();
    let decibels = |x: f32| 20.0 * x.max(1e-9).log10();
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    println!(
        "{name}: {seconds} s, level {:.1} dB (RMS {:.4}), peak {:.3}",
        decibels(rms(&samples)),
        rms(&samples),
        peak
    );
    print!("octaves, dB:");
    for centre in [31.5f32, 63.0, 125.0, 250.0, 500.0, 1_000.0, 2_000.0, 4_000.0, 8_000.0, 16_000.0] {
        let mut band = Biquad::default();
        band.band_pass(centre, 1.414);
        let through: Vec<f32> = samples.iter().map(|s| band.pass(*s)).collect();
        print!(" {}:{:.0}", centre, decibels(rms(&through)));
    }
    println!();
    write_wav(path, &samples)?;
    println!("{path}");
    Ok(())
}

fn write_wav(path: &str, samples: &[f32]) -> std::io::Result<()> {
    let data = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32_767.0).round() as i16).to_le_bytes());
    }
    std::fs::write(path, out)
}
