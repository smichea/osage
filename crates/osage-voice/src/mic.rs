// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The microphone: the system's default input, open only while a
//! [`Microphone`] lives, brought to mono at [`RATE`] and cut into frames
//! of [`FRAME`] samples, each with how loud it was.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use cpal::{
    SampleFormat, SizedSample, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use crossbeam_channel::{Receiver, Sender, bounded};

use crate::{FRAME, RATE};

/// A frame from the microphone, and its loudness (RMS, 0 to 1).
pub struct Spoken {
    pub samples: Vec<i16>,
    pub level: f32,
}

/// The microphone open; closed when dropped.
pub struct Microphone {
    pub frames: Receiver<Spoken>,
    /// The device's name, to show.
    pub name: String,
    closed: Arc<AtomicBool>,
}

impl Drop for Microphone {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Relaxed);
    }
}

impl Microphone {
    /// Opens the default input; an error says why it could not be.
    pub fn open() -> Result<Self, String> {
        let (frames, heard) = bounded(25);
        let (ready, opened) = bounded(1);
        let closed = Arc::new(AtomicBool::new(false));
        let stop = closed.clone();
        // The stream stays on the thread that made it: on some systems it
        // cannot move.
        std::thread::Builder::new()
            .name("osage-voice-mic".into())
            .spawn(move || {
                let stream = match start(frames) {
                    Ok((stream, name)) => {
                        let _ = ready.send(Ok(name));
                        stream
                    }
                    Err(e) => {
                        let _ = ready.send(Err(e));
                        return;
                    }
                };
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                drop(stream);
            })
            .map_err(|e| e.to_string())?;
        let name = opened.recv().map_err(|e| e.to_string())??;
        Ok(Self { frames: heard, name, closed })
    }
}

fn start(frames: Sender<Spoken>) -> Result<(cpal::Stream, String), String> {
    let device = cpal::default_host().default_input_device().ok_or("no microphone")?;
    let name = device.description().map(|d| d.to_string()).unwrap_or_else(|_| "microphone".to_owned());
    let supported = device.default_input_config().map_err(|e| e.to_string())?;
    let config = supported.config();
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build::<f32>(&device, &config, frames, |s| s),
        SampleFormat::I16 => build::<i16>(&device, &config, frames, |s| f32::from(s) / 32_768.0),
        SampleFormat::U16 => build::<u16>(&device, &config, frames, |s| (f32::from(s) - 32_768.0) / 32_768.0),
        SampleFormat::I32 => build::<i32>(&device, &config, frames, |s| s as f32 / 2_147_483_648.0),
        other => return Err(format!("a microphone giving {other} samples")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, name))
}

fn build<T: SizedSample + Copy + Send + 'static>(
    device: &cpal::Device,
    config: &StreamConfig,
    frames: Sender<Spoken>,
    to_f32: fn(T) -> f32,
) -> Result<cpal::Stream, String> {
    let channels = usize::from(config.channels).max(1);
    let step = f64::from(config.sample_rate) / f64::from(RATE);
    let mut cut = Cutter { step, at: 0.0, last: 0.0, frame: Vec::with_capacity(FRAME), frames };
    device
        .build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                for frame in data.chunks(channels) {
                    let mono = frame.iter().map(|s| to_f32(*s)).sum::<f32>() / frame.len() as f32;
                    cut.push(mono);
                }
            },
            |e| eprintln!("microphone: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}

/// From the device's rate to [`RATE`], in frames.
struct Cutter {
    step: f64,
    at: f64,
    last: f32,
    frame: Vec<i16>,
    frames: Sender<Spoken>,
}

impl Cutter {
    fn push(&mut self, sample: f32) {
        // Each sample of the device covers `1` of its time; one at RATE is
        // taken every `step` of them, between the last and this one.
        while self.at < 1.0 {
            let s = self.last + (sample - self.last) * self.at as f32;
            self.frame.push((s.clamp(-1.0, 1.0) * 32_767.0) as i16);
            self.at += self.step;
            if self.frame.len() == FRAME {
                let samples = std::mem::replace(&mut self.frame, Vec::with_capacity(FRAME));
                let level =
                    (samples.iter().map(|s| (f32::from(*s) / 32_768.0).powi(2)).sum::<f32>() / FRAME as f32).sqrt();
                // A full queue (nobody reading): the frame is dropped.
                let _ = self.frames.try_send(Spoken { samples, level });
            }
        }
        self.at -= 1.0;
        self.last = sample;
    }
}
