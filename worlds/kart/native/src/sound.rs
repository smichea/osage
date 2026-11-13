// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The karts' sound: each kart a voice made from its state
//! ([osage-kart-sound](osage_kart_sound)) — the engine following its revs,
//! tyres squealing as it slides, kerbs and grass under its wheels, wind, a
//! knock when it is hit, a bumper's air bag and a pit-crew push from the
//! kart that uses them — heard from the player's kart (D-070): its own
//! engine loudest from the seat and a little softer from behind or above
//! (D-078), the others by their distance, to the left
//! or the right as the camera looks, higher as they come and lower as they
//! go (Doppler); each drone's rotors where it flies and the crowd in the
//! grandstand, heard from the camera. A voice plays recordings where
//! the content has them (`freesound/kart/`, made by `tools/make-sounds.py`
//! from the asset manifest's originals) and synthesises the rest. Not in
//! the bench: it measures pictures.

use std::{
    num::NonZero,
    sync::{Arc, Mutex},
    time::Duration,
};

use bevy::{
    audio::{
        AddAudioSource, AudioSink, AudioSinkPlayback, ChannelCount, Decodable, PlaybackSettings, Sample, SampleRate,
        Source, SpatialAudioSink, SpatialListener,
    },
    prelude::*,
};
use osage_kart_physics::MAX_SPEED;
use osage_kart_sound::{Buzz, Cue, Drive, IDLE_RPM, RATE, Recordings, Voice, revs};
use osage_render::OsageCamera;

use crate::{
    scene::{Course, Pose, roughness},
    settings::Audio,
};

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
            .add_systems(
                Update,
                (listen, voice, drive, hear, crowd, rotors, hush.run_if(resource_exists::<Audio>)).chain(),
            );
    }
}

/// A sink that plays music, not the game's sounds: the music setting
/// mutes it, the sound setting does not.
#[derive(Component)]
pub struct Music;

/// A sink that plays the other players' voices: neither the sound nor
/// the music setting mutes it (each voice is muted on its own).
#[derive(Component)]
pub struct Speech;

/// Sound off: every sound of the game is muted, new ones as they start;
/// on, they are heard again (settings, N).
fn hush(
    audio: Res<Audio>,
    mut sinks: Query<&mut SpatialAudioSink, (Without<Music>, Without<Speech>)>,
    mut flat: Query<&mut AudioSink, (Without<Music>, Without<Speech>)>,
) {
    let changed = audio.is_changed();
    let set = |sink: &mut dyn AudioSinkPlayback| {
        if sink.is_muted() == audio.sound {
            if audio.sound { sink.unmute() } else { sink.mute() }
        }
    };
    for mut sink in &mut sinks {
        if changed || sink.is_added() {
            set(&mut *sink);
        }
    }
    for mut sink in &mut flat {
        if changed || sink.is_added() {
            set(&mut *sink);
        }
    }
}

/// What a voice follows, written by the game, read by the audio thread.
#[derive(Default)]
struct Shared {
    drive: Drive,
    knocks: Vec<f32>,
    cues: Vec<Cue>,
    ear: Ear,
}

/// How a voice is heard: its loudness in each ear, and how much higher it
/// sounds than it is (a Doppler shift).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ear {
    pub left: f32,
    pub right: f32,
    pub pitch: f32,
}

impl Default for Ear {
    fn default() -> Self {
        Self { left: 0.0, right: 0.0, pitch: 1.0 }
    }
}

/// How loud the player's own kart is in each view (D-078): full from the
/// seat; from behind, where the default view is, about 2.5 dB under, and
/// from above about 4 dB, so that the others' karts are heard over it.
pub fn own_loudness(view: crate::scene::View) -> f32 {
    use crate::scene::View;
    match view {
        View::Cockpit => 1.0,
        View::Chase => 0.75,
        View::Bird => 0.63,
    }
}

/// The speed of sound, m/s.
const SOUND: f32 = 343.0;
/// At this distance another kart is heard at half its loudness, m.
const HALF_AT: f32 = 9.0;
/// Further than this, not at all, m.
const FARTHEST: f32 = 220.0;

/// How a kart at `at` going at `velocity` is heard by an ear at `ear`
/// going at `moving`, whose right is `right`; `own`: the player's own kart,
/// heard as from its seat.
pub fn heard(at: Vec3, velocity: Vec3, ear: Vec3, moving: Vec3, right: Vec3, own: bool) -> Ear {
    if own {
        let centre = std::f32::consts::FRAC_1_SQRT_2;
        return Ear { left: centre, right: centre, pitch: 1.0 };
    }
    let apart = (at - ear).with_y(0.0);
    let distance = apart.length().max(0.5);
    let towards = apart / distance;
    // An inverse-distance roll-off, gentler than a square: a kart is heard
    // coming, then fades to nothing at the far side of a circuit.
    let fade = (1.0 - distance / FARTHEST).clamp(0.0, 1.0);
    let loud = HALF_AT / (HALF_AT + distance) * fade;
    // Equal-power panning by where it is across the view.
    let across = towards.dot(right.with_y(0.0).normalize_or_zero()).clamp(-1.0, 1.0);
    let angle = (across + 1.0) * std::f32::consts::FRAC_PI_4;
    // Approaching, higher; going away, lower.
    let pitch = ((SOUND + moving.dot(towards)) / (SOUND + velocity.dot(towards))).clamp(0.8, 1.25);
    Ear { left: loud * angle.cos(), right: loud * angle.sin(), pitch }
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
        Stream::new(Voice::with(self.seed, self.recordings.clone()), self.shared.clone())
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

/// The samples of a voice, endless: stereo, each ear at its loudness, the
/// voice played faster or slower by its Doppler shift.
pub struct Stream {
    voice: Voice,
    shared: Arc<Mutex<Shared>>,
    count: u32,
    /// How it is to be heard, and how it is heard now (eased to it).
    ear: Ear,
    now: Ear,
    /// The voice's last two samples and how far between them the output is.
    before: f32,
    after: f32,
    at: f32,
    /// The right ear's sample, after the left one's.
    right: Option<f32>,
}

impl Stream {
    fn new(voice: Voice, shared: Arc<Mutex<Shared>>) -> Self {
        Self {
            voice,
            shared,
            count: 0,
            ear: Ear::default(),
            now: Ear::default(),
            before: 0.0,
            after: 0.0,
            at: 0.0,
            right: None,
        }
    }
}

impl Iterator for Stream {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        if let Some(right) = self.right.take() {
            return Some(right);
        }
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
            self.ear = shared.ear;
        }
        self.count = self.count.wrapping_add(1);
        // Eased over about 20 ms: no clicks as karts move.
        let ease = 1.0 / 960.0;
        self.now.left += (self.ear.left - self.now.left) * ease;
        self.now.right += (self.ear.right - self.now.right) * ease;
        self.now.pitch += (self.ear.pitch - self.now.pitch) * ease;
        self.at += self.now.pitch;
        while self.at >= 1.0 {
            self.before = self.after;
            self.after = self.voice.sample();
            self.at -= 1.0;
        }
        let s = self.before + (self.after - self.before) * self.at;
        self.right = Some(s * self.now.right);
        Some(s * self.now.left)
    }
}

impl Source for Stream {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        NonZero::new(2).unwrap()
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
/// The first player's camera, not listening yet.
type Unheard = (With<OsageCamera>, Without<SpatialListener>, Without<crate::scene::SecondCamera>);

fn listen(mut commands: Commands, cameras: Query<Entity, Unheard>) {
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
    // A ghost makes no sound (D-062).
    for (entity, pose) in karts.iter().filter(|(_, pose)| !pose.ghost) {
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
            // Heard as `hear` places it, not by the engine's own panning.
            PlaybackSettings::LOOP,
        ));
    }
}

/// Where each kart is heard from (D-070): the ear is in the player's kart
/// — the followed one — turned as the camera looks, so that its own engine
/// is heard in every view and the others come and go around it.
fn hear(
    view: Option<Res<crate::scene::View>>,
    camera: Query<&GlobalTransform, crate::scene::FirstCamera>,
    followed: Query<&Pose, With<crate::scene::Followed>>,
    karts: Query<(Entity, &Pose, &Heard)>,
    own: Query<Entity, With<crate::scene::Followed>>,
) {
    let Ok(camera) = camera.single() else { return };
    let mine = own.iter().next();
    let velocity = |kart: &osage_kart_physics::Kart| {
        let (x, z) = kart.velocity();
        Vec3::new(x, 0.0, z)
    };
    let (ear, moving) = match followed.iter().next() {
        Some(pose) => (Vec3::new(pose.kart.x, pose.kart.height, pose.kart.z), velocity(&pose.kart)),
        None => (camera.translation(), Vec3::ZERO),
    };
    let right = camera.right().as_vec3();
    let own = own_loudness(view.map_or_else(default, |v| *v));
    for (entity, pose, heard) in &karts {
        let at = Vec3::new(pose.kart.x, pose.kart.height, pose.kart.z);
        let mut placed = self::heard(at, velocity(&pose.kart), ear, moving, right, Some(entity) == mine);
        if Some(entity) == mine {
            (placed.left, placed.right) = (placed.left * own, placed.right * own);
        }
        if let Ok(mut shared) = heard.shared.lock() {
            shared.ear = placed;
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::View;

    #[test]
    fn own_kart_softest_from_above_loudest_from_the_seat() {
        let (seat, behind, above) = (own_loudness(View::Cockpit), own_loudness(View::Chase), own_loudness(View::Bird));
        assert_eq!(seat, 1.0);
        assert!(above < behind && behind < seat);
        // Softer, not lost: no view more than 6 dB under the seat.
        assert!(20.0 * above.log10() > -6.0);
    }

    #[test]
    fn a_kart_coming_is_higher_and_one_going_lower() {
        let (ear, right) = (Vec3::ZERO, Vec3::X);
        let coming = heard(Vec3::new(0.0, 0.0, 30.0), Vec3::new(0.0, 0.0, -25.0), ear, Vec3::ZERO, right, false);
        let going = heard(Vec3::new(0.0, 0.0, 30.0), Vec3::new(0.0, 0.0, 25.0), ear, Vec3::ZERO, right, false);
        assert!(coming.pitch > 1.0 && going.pitch < 1.0);
    }
}
