// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The players talking to one another (osage-voice, D-060): when the world
//! announces a voice relay, the player joins its conference under their
//! driver's name and hears the others; their microphone opens only if
//! they chose push to talk (V held) or an open microphone in the settings,
//! and in push to talk only what is said while V is held is sent. Who is
//! talking is shown at the bottom of the screen; each voice can be muted
//! in the settings.

use std::time::Duration;

use bevy::{audio::PlaybackSettings, prelude::*, text::FontSize};
use osage_voice::{Voice, audio::VoiceSource, mic::Microphone};

use crate::{
    play::Host,
    settings::{Audio, VoiceMode, Voices},
    sound::Speech,
};

/// An open microphone sends what is louder than this (RMS)…
const GATE: f32 = 0.02;
/// …and for this long after, so words are not cut between syllables.
const HANGOVER: Duration = Duration::from_millis(400);

pub struct VoicePlugin;

impl Plugin for VoicePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(osage_voice::audio::VoicePlugin)
            .init_resource::<Conference>()
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (join, talk, others, show).chain().run_if(resource_exists::<Audio>).run_if(resource_exists::<Host>),
            );
    }
}

#[derive(Resource, Default)]
struct Conference {
    voice: Option<Voice>,
    /// Where and as whom it was joined: (host, port, name).
    at: Option<(String, u16, String)>,
    sink: Option<Entity>,
    mic: Option<Microphone>,
    /// The microphone could not open for this mode: not tried again until it changes.
    failed: Option<VoiceMode>,
    /// Until when an open microphone keeps sending.
    open_until: Duration,
    sending: bool,
    notice: Option<(String, Duration)>,
}

#[derive(Component)]
struct Line;

fn setup(mut commands: Commands) {
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            bottom: px(40),
            width: percent(100),
            justify_content: JustifyContent::Center,
            ..default()
        })
        .with_child((
            Line,
            Text::default(),
            TextFont { font_size: FontSize::Px(17.0), ..default() },
            TextColor(Color::srgb(0.75, 1.0, 0.8)),
            TextShadow::default(),
        ));
}

/// Joins the conference the world announces, as the player's driver.
fn join(
    mut commands: Commands,
    time: Res<Time>,
    host: Res<Host>,
    mut sources: ResMut<Assets<VoiceSource>>,
    mut conference: ResMut<Conference>,
) {
    let wanted = (host.voice != 0 && !host.me.is_empty()).then(|| (host.host.clone(), host.voice, host.me.clone()));
    if wanted == conference.at {
        return;
    }
    conference.at = wanted.clone();
    conference.voice = None;
    if let Some(sink) = conference.sink.take() {
        commands.entity(sink).despawn();
    }
    let Some((machine, port, name)) = wanted else { return };
    match Voice::join((machine.as_str(), port), &name) {
        Ok(voice) => {
            let source = sources.add(VoiceSource::new(voice.feed()));
            conference.sink = Some(commands.spawn((Speech, AudioPlayer(source), PlaybackSettings::LOOP)).id());
            conference.voice = Some(voice);
        }
        Err(e) => conference.notice = Some((format!("Voice: no relay at {machine}:{port}: {e}"), time.elapsed())),
    }
}

/// The microphone, open only when the player chose to be heard, and what
/// it hears sent while they talk.
fn talk(time: Res<Time>, keys: Res<ButtonInput<KeyCode>>, audio: Res<Audio>, mut conference: ResMut<Conference>) {
    let now = time.elapsed();
    let wanted = audio.voice != VoiceMode::Off && conference.voice.is_some();
    if conference.failed.is_some_and(|mode| mode != audio.voice) {
        conference.failed = None;
    }
    if !wanted {
        conference.mic = None;
        conference.sending = false;
        return;
    }
    if conference.mic.is_none() && conference.failed.is_none() {
        match Microphone::open() {
            Ok(mic) => {
                info!("voice: microphone {}", mic.name);
                conference.notice = Some((format!("Voice: microphone {}", mic.name), now));
                conference.mic = Some(mic);
            }
            Err(e) => {
                conference.notice = Some((format!("Voice: no microphone ({e})"), now));
                conference.failed = Some(audio.voice);
            }
        }
    }
    let held = keys.pressed(KeyCode::KeyV);
    let (Some(mic), Some(voice)) = (&conference.mic, &conference.voice) else { return };
    let mut sending = conference.sending;
    let mut open_until = conference.open_until;
    for spoken in mic.frames.try_iter() {
        sending = match audio.voice {
            VoiceMode::PushToTalk => held,
            VoiceMode::Open => {
                if spoken.level > GATE {
                    open_until = now + HANGOVER;
                }
                now < open_until
            }
            VoiceMode::Off => false,
        };
        if sending {
            voice.say(&spoken.samples);
        }
    }
    if audio.voice == VoiceMode::PushToTalk {
        sending = held;
    }
    (conference.sending, conference.open_until) = (sending, open_until);
}

/// The others, for the settings, and the mutings asked there.
fn others(host: Res<Host>, conference: Res<Conference>, mut voices: ResMut<Voices>) {
    let Some(voice) = &conference.voice else {
        if !voices.others.is_empty() {
            voices.others.clear();
        }
        return;
    };
    if let Some(name) = voices.toggle.take() {
        let muted = voices.others.iter().any(|(n, muted, _)| *n == name && *muted);
        voice.mute(&name, !muted);
    }
    let others: Vec<_> =
        voice.heard().into_iter().filter(|h| h.name != host.me).map(|h| (h.name, h.muted, h.talking)).collect();
    if voices.others != others {
        voices.picked = voices.picked.min(others.len().saturating_sub(1));
        voices.others = others;
    }
}

fn show(time: Res<Time>, conference: Res<Conference>, voices: Res<Voices>, mut lines: Query<&mut Text, With<Line>>) {
    let talking: Vec<&str> =
        voices.others.iter().filter(|(_, muted, talking)| *talking && !muted).map(|(n, ..)| n.as_str()).collect();
    let mut parts = Vec::new();
    if conference.sending {
        parts.push("You are heard".to_owned());
    }
    if !talking.is_empty() {
        parts.push(format!("Talking: {}", talking.join(", ")));
    }
    if parts.is_empty()
        && let Some((notice, at)) = &conference.notice
        && time.elapsed() < *at + Duration::from_secs(6)
    {
        parts.push(notice.clone());
    }
    let content = parts.join("   |   ");
    for mut text in &mut lines {
        if text.0 != content {
            text.0 = content.clone();
        }
    }
}
