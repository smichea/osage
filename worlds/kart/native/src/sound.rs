// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The karts' sound: each kart a voice made from its state
//! ([osage-kart-sound](osage_kart_sound)) — the engine following its revs,
//! tyres squealing as it slides, kerbs and grass under its wheels, wind, a
//! knock when it is hit, a bumper's air bag and a pit-crew push from the
//! kart that uses them — placed where the kart is and heard from the
//! camera; each drone's rotors where it flies; and the crowd in the
//! grandstand. A voice plays recordings where
//! the content has them (`freesound/kart/`, made by `tools/make-sounds.py`
//! from the asset manifest's originals) and synthesises the rest. Not in
//! the bench: it measures pictures.

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
use osage_kart_sound::{Buzz, Cue, Drive, IDLE_RPM, RATE, Recordings, Voice, revs};
use osage_render::OsageCamera;

use crate::scene::{Course, Pose, roughness};

/// The engine loops and the engine speed each sounds at, turns a minute:
/// the idle as it is, the others by the pitch measured in their passage.
const ENGINE_LOOPS: [(&str, f32); 3] = [("engine-idle", IDLE_RPM), ("engine-mid", 7_000.0), ("engine-high", 9_900.0)];
/// How loud the crowd is.
const CROWD: f32 = 0.9;

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<Synth>()
            .add_audio_source::<Clip>()
            .add_audio_source::<Rotors>()
            .add_systems(Startup, load)
            .add_systems(Update, (listen, voice, drive, crowd, rotors).chain());
    }
}

/// What a voice follows, written by the game, read by the audio thread.
#[derive(Default)]
struct Shared {
    drive: Drive,
    knocks: Vec<f32>,
    cues: Vec<Cue>,
}

/// The recordings found, shared by every voice.
#[derive(Resource, Default)]
struct Tapes {
    voice: Arc<Recordings>,
    crowd: Option<Arc<[f32]>>,
}

/// Decodes the clips there are: mono Ogg Vorbis at the voices' rate.
fn load(mut commands: Commands) {
    let Some(dir) = osage_content::root().map(|root| root.join("freesound/kart")) else {
        commands.init_resource::<Tapes>();
        return;
    };
    let clip = |name: &str| decode(&dir.join(format!("{name}.ogg")));
    let engine: Vec<_> = ENGINE_LOOPS.iter().filter_map(|(name, rpm)| Some((clip(name)?, *rpm))).collect();
    let voice = Recordings {
        // All three or none: one loop stretched over every speed sounds wrong.
        engine: if engine.len() == ENGINE_LOOPS.len() { engine } else { Vec::new() },
        squeal: clip("squeal"),
        ground: clip("gravel"),
        knocks: (1..=3).filter_map(|i| clip(&format!("knock-{i}"))).collect(),
    };
    info!(recordings = ?voice, "kart sound");
    commands.insert_resource(Tapes { voice: Arc::new(voice), crowd: clip("crowd") });
}

fn decode(path: &std::path::Path) -> Option<Arc<[f32]>> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = lewton::inside_ogg::OggStreamReader::new(std::io::BufReader::new(file)).ok()?;
    if reader.ident_hdr.audio_sample_rate != RATE || reader.ident_hdr.audio_channels != 1 {
        warn!(?path, "not mono at {RATE} Hz: synthesised instead");
        return None;
    }
    let mut samples = Vec::new();
    while let Ok(Some(packet)) = reader.read_dec_packet_itl() {
        samples.extend(packet.iter().map(|&s| f32::from(s) / 32_768.0));
    }
    (!samples.is_empty()).then(|| samples.into())
}

/// A kart's voice as an audio source.
#[derive(Asset, TypePath)]
pub struct Synth {
    shared: Arc<Mutex<Shared>>,
    seed: u32,
    recordings: Arc<Recordings>,
}

impl Decodable for Synth {
    type Decoder = Stream;

    fn decoder(&self) -> Stream {
        Stream { voice: Voice::with(self.seed, self.recordings.clone()), shared: self.shared.clone(), count: 0 }
    }
}

/// A recording played in a loop, as an audio source.
#[derive(Asset, TypePath)]
pub struct Clip {
    samples: Arc<[f32]>,
    gain: f32,
}

impl Decodable for Clip {
    type Decoder = Looped;

    fn decoder(&self) -> Looped {
        Looped { samples: self.samples.clone(), gain: self.gain, at: 0 }
    }
}

/// The samples of a clip, over and over.
pub struct Looped {
    samples: Arc<[f32]>,
    gain: f32,
    at: usize,
}

impl Iterator for Looped {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        let s = *self.samples.get(self.at)?;
        self.at = (self.at + 1) % self.samples.len();
        Some(s * self.gain)
    }
}

impl Source for Looped {
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

/// The crowd in the grandstand, once the circuit is known.
fn crowd(
    mut commands: Commands,
    tapes: Res<Tapes>,
    course: Option<Res<Course>>,
    mut clips: ResMut<Assets<Clip>>,
    mut placed: Local<bool>,
) {
    let (Some(course), Some(samples), false) = (course, tapes.crowd.clone(), *placed) else { return };
    *placed = true;
    // In the middle of the stands, as the landmarks build them: beside the
    // grid, set back from the road.
    let circuit = &course.0;
    let ([x, z], heading) = circuit.track.at(circuit.start - 16.0);
    let back = -(circuit.track.half_width + circuit.track.verge + 8.0);
    let at = Transform::from_xyz(x, 0.0, z).with_rotation(Quat::from_rotation_y(heading)) * Vec3::new(back, 3.0, 0.0);
    commands.spawn((
        Transform::from_translation(at),
        AudioPlayer(clips.add(Clip { samples, gain: CROWD })),
        PlaybackSettings::LOOP.with_spatial(true),
    ));
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
            for cue in shared.cues.drain(..) {
                self.voice.cue(cue);
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
    /// What the kart held and its push, last frame: a bumper gone is a
    /// bumper used; a push started is heard.
    held: Option<osage_kart_items::Item>,
    guard: f32,
}

/// The camera listens.
fn listen(mut commands: Commands, cameras: Query<Entity, (With<OsageCamera>, Without<SpatialListener>)>) {
    for camera in &cameras {
        commands.entity(camera).insert(SpatialListener::new(0.25));
    }
}

/// Every kart gets its voice.
fn voice(
    mut commands: Commands,
    tapes: Res<Tapes>,
    mut synths: ResMut<Assets<Synth>>,
    karts: Query<(Entity, &Pose), Without<Heard>>,
) {
    for (entity, pose) in &karts {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let synth = synths.add(Synth { shared: shared.clone(), seed: pose.color + 1, recordings: tapes.voice.clone() });
        commands.entity(entity).insert((
            Heard {
                shared,
                speed: pose.kart.speed,
                spin: pose.kart.spin,
                throttle: 0.0,
                held: pose.held,
                guard: pose.guard,
            },
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
        let bumped = heard.held == Some(osage_kart_items::Item::Bumper) && pose.held != heard.held;
        let pushed = pose.guard > heard.guard + 0.5;
        (heard.held, heard.guard) = (pose.held, pose.guard);
        if let Ok(mut shared) = heard.shared.lock() {
            shared.drive = drive;
            if knock > 0.15 {
                shared.knocks.push(knock.min(1.0));
            }
            if bumped {
                shared.cues.push(Cue::Bumper);
            }
            if pushed {
                shared.cues.push(Cue::Push);
            }
        }
    }
}

/// A drone's model, which buzzes while it is shown.
#[derive(Component)]
pub struct Drone;

/// A drone's rotors as an audio source, as loud as `gain` says.
#[derive(Asset, TypePath)]
pub struct Rotors {
    gain: Arc<Mutex<f32>>,
    seed: u32,
}

impl Decodable for Rotors {
    type Decoder = Spinning;

    fn decoder(&self) -> Spinning {
        Spinning { buzz: Buzz::new(self.seed), gain: self.gain.clone(), now: 0.0, wanted: 0.0, count: 0 }
    }
}

/// A drone's buzz, sample by sample, easing to the gain wanted.
pub struct Spinning {
    buzz: Buzz,
    gain: Arc<Mutex<f32>>,
    now: f32,
    wanted: f32,
    count: u32,
}

impl Iterator for Spinning {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        if self.count.is_multiple_of(240)
            && let Ok(gain) = self.gain.try_lock()
        {
            self.wanted = *gain;
        }
        self.count = self.count.wrapping_add(1);
        self.now += (self.wanted - self.now) * 0.001;
        Some(self.buzz.sample(self.now))
    }
}

impl Source for Spinning {
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

/// How loud a drone is, and the gain its sound follows.
#[derive(Component)]
struct Loudness(Arc<Mutex<f32>>);

/// Every drone model gets its rotors, heard while it is shown.
fn rotors(
    mut commands: Commands,
    mut sources: ResMut<Assets<Rotors>>,
    new: Query<Entity, (With<Drone>, Without<Loudness>)>,
    drones: Query<(&Visibility, &Loudness)>,
) {
    for (n, entity) in new.iter().enumerate() {
        let gain = Arc::new(Mutex::new(0.0));
        let source = sources.add(Rotors { gain: gain.clone(), seed: entity.index_u32() ^ n as u32 });
        commands.entity(entity).insert((
            Loudness(gain),
            AudioPlayer(source),
            PlaybackSettings::LOOP.with_spatial(true),
        ));
    }
    for (visibility, loudness) in &drones {
        if let Ok(mut gain) = loudness.0.lock() {
            *gain = if *visibility == Visibility::Hidden { 0.0 } else { 0.8 };
        }
    }
}
