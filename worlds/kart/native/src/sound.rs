// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The karts' sound: each kart a voice synthesised from its state
//! ([osage-kart-sound](osage_kart_sound)) — the engine following its revs,
//! tyres squealing as it slides, kerbs and grass under its wheels, wind, a
//! knock when it is hit — placed where the kart is and heard from the
//! camera. Not in the bench: it measures pictures.

use std::{
    num::NonZero,
    sync::{Arc, Mutex},
    time::Duration,
};

use bevy::{
    audio::{AddAudioSource, ChannelCount, Decodable, PlaybackSettings, Sample, SampleRate, Source, SpatialListener},
    prelude::*,
};
use osage_kart_physics::MAX_SPEED;
use osage_kart_sound::{Drive, RATE, Voice, revs};
use osage_render::OsageCamera;

use crate::scene::{Course, Pose, roughness};

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<Synth>().add_systems(Update, (listen, voice, drive).chain());
    }
}

/// What a voice follows, written by the game, read by the audio thread.
#[derive(Default)]
struct Shared {
    drive: Drive,
    knocks: Vec<f32>,
}

/// A kart's voice as an audio source.
#[derive(Asset, TypePath)]
pub struct Synth {
    shared: Arc<Mutex<Shared>>,
    seed: u32,
}

impl Decodable for Synth {
    type Decoder = Stream;

    fn decoder(&self) -> Stream {
        Stream { voice: Voice::new(self.seed), shared: self.shared.clone(), count: 0 }
    }
}

/// The samples of a voice, endless.
pub struct Stream {
    voice: Voice,
    shared: Arc<Mutex<Shared>>,
    count: u32,
}

impl Iterator for Stream {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        // Every 5 ms, what the game wants now; never wait for it.
        if self.count.is_multiple_of(240)
            && let Ok(mut shared) = self.shared.try_lock()
        {
            self.voice.set(shared.drive);
            for strength in shared.knocks.drain(..) {
                self.voice.knock(strength);
            }
        }
        self.count = self.count.wrapping_add(1);
        Some(self.voice.sample())
    }
}

impl Source for Stream {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        NonZero::<u16>::MIN
    }

    fn sample_rate(&self) -> SampleRate {
        NonZero::new(RATE).unwrap()
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

/// A kart's voice and what it last heard of the kart.
#[derive(Component)]
struct Heard {
    shared: Arc<Mutex<Shared>>,
    speed: f32,
    spin: f32,
    throttle: f32,
}

/// The camera listens.
fn listen(mut commands: Commands, cameras: Query<Entity, (With<OsageCamera>, Without<SpatialListener>)>) {
    for camera in &cameras {
        commands.entity(camera).insert(SpatialListener::new(0.25));
    }
}

/// Every kart gets its voice.
fn voice(mut commands: Commands, mut synths: ResMut<Assets<Synth>>, karts: Query<(Entity, &Pose), Without<Heard>>) {
    for (entity, pose) in &karts {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let synth = synths.add(Synth { shared: shared.clone(), seed: pose.color + 1 });
        commands.entity(entity).insert((
            Heard { shared, speed: pose.kart.speed, spin: pose.kart.spin, throttle: 0.0 },
            AudioPlayer(synth),
            PlaybackSettings::LOOP.with_spatial(true),
        ));
    }
}

/// What each voice follows, from its kart's state.
fn drive(time: Res<Time>, course: Option<Res<Course>>, mut karts: Query<(&Pose, &mut Heard)>) {
    let seconds = time.delta_secs().max(1e-4);
    for (pose, mut heard) in &mut karts {
        let kart = &pose.kart;
        // The throttle is not sent for other karts: it shows in how they
        // gain speed, or hold it near the top.
        let gaining = (kart.speed - heard.speed) / seconds;
        let wanted = if gaining > 1.0 || (kart.speed > 0.85 * MAX_SPEED && gaining > -2.0) { 1.0 } else { 0.0 };
        heard.throttle += (wanted - heard.throttle) * (seconds * 8.0).min(1.0);
        let knock = (kart.spin.abs() - heard.spin.abs()).max(0.0) / 4.0
            + ((heard.speed - kart.speed) / seconds - 40.0).max(0.0) / 80.0;
        (heard.speed, heard.spin) = (kart.speed, kart.spin);
        let slip =
            (kart.slide.abs() / 4.0 + kart.spin.abs() / 3.0 + if kart.drift != 0.0 { 0.45 } else { 0.0 }).min(1.0);
        let ground = match roughness(course.as_deref(), kart.x, kart.z) {
            r if r < 0.3 => 0.0,
            r if r < 0.8 => 0.6,
            _ => 1.0,
        };
        let top = if kart.boost > 0.0 { MAX_SPEED * 1.27 } else { MAX_SPEED };
        let drive = Drive {
            revs: revs(kart.speed, top, heard.throttle),
            throttle: heard.throttle,
            slip,
            ground,
            speed: kart.speed.abs(),
        };
        if let Ok(mut shared) = heard.shared.lock() {
            shared.drive = drive;
            if knock > 0.15 {
                shared.knocks.push(knock.min(1.0));
            }
        }
    }
}
