// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Renders twelve seconds of a kart to a WAV file, to listen to the
//! synthesis without the game: idling, full throttle up the straight, a bend
//! with the tyres squealing and a run over the kerb, a knock, lifting off.
//!
//!     cargo run -p osage-kart-sound --example lap -- lap.wav

use osage_kart_sound::{Drive, RATE, Voice, revs};

fn main() -> std::io::Result<()> {
    let path = std::env::args().nth(1).unwrap_or_else(|| "lap.wav".to_owned());
    let mut voice = Voice::new(1);
    let mut samples = Vec::new();
    let (mut speed, step) = (0.0f32, 1.0 / 100.0);
    for i in 0..1200 {
        let t = i as f32 * step;
        let throttle = if t < 1.0 || (7.0..9.0).contains(&t) { 0.0 } else { 1.0 };
        let target = if t < 1.0 {
            0.0
        } else if t < 9.0 && throttle > 0.0 {
            22.0
        } else {
            12.0
        };
        speed += (target - speed).clamp(-14.0 * step, 12.0 * step);
        let slip = if (5.0..6.5).contains(&t) { 0.7 } else { 0.0 };
        let ground = if (6.0..7.0).contains(&t) { 0.6 } else { 0.0 };
        if i == 750 {
            voice.knock(0.8);
        }
        voice.set(Drive { revs: revs(speed, 22.0, throttle), throttle, slip, ground, speed });
        for _ in 0..(RATE as f32 * step) as usize {
            samples.push(voice.sample());
        }
    }
    write_wav(&path, &samples)
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
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32_000.0) as i16).to_le_bytes());
    }
    std::fs::write(path, out)
}
