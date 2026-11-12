// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The music heard in the game (osage-music), from where the settings say:
//! the host's shared playlist when they share one, else the game's own
//! pieces (auto); the game's own; the player's own folder
//! (`osage/kart-music/`); the host's; or radio stations — the player's
//! list (`osage/kart-radios.txt`), and the radio directory's stations for
//! a kind of music when the player turned it on. B goes to the next piece
//! or station, Shift+B back. What plays is shown at the bottom of the
//! screen for a few seconds.

use std::{io, path::PathBuf, time::Duration};

use bevy::{audio::PlaybackSettings, prelude::*, text::FontSize};
use crossbeam_channel::Receiver;
use osage_music::{
    Player, Program, Status,
    audio::MusicSource,
    playlist,
    radio::{self, Station},
    share::Remote,
};

use crate::{
    play::Host,
    settings::{Audio, MusicFrom, folder},
    sound::Music,
};

/// How loud the music is under the karts: pieces, levelled; radio, louder
/// as stations master it, turned down more.
const PIECES: f32 = 0.5;
const STATIONS: f32 = 0.32;
/// How long "auto" waits to hear from the world whether its host shares music.
const WAIT_FOR_HOST: Duration = Duration::from_secs(5);
/// How long what plays is shown.
const SHOWN: Duration = Duration::from_secs(6);
/// Stations asked of the directory.
const ASKED: u32 = 40;

pub struct MusicPlugin;

impl Plugin for MusicPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(osage_music::audio::MusicPlugin).add_systems(Startup, start).add_systems(
            Update,
            (heard, choose, skip, show).chain().run_if(resource_exists::<Audio>).run_if(resource_exists::<Host>),
        );
    }
}

/// What the program was chosen from: chosen again when it changes.
#[derive(Clone, PartialEq)]
struct Chosen {
    audio: Audio,
    host: Host,
    found: usize,
    station: usize,
}

#[derive(Resource)]
struct Jukebox {
    player: Player,
    chosen: Option<Chosen>,
    program: Program,
    station: usize,
    /// The directory being asked, for a kind of music.
    asking: Option<(&'static str, Receiver<io::Result<Vec<Station>>>)>,
    /// Its last answer.
    found: Option<(&'static str, Vec<Station>)>,
    /// Something to tell the player, once.
    notice: Option<String>,
    /// The player's changes already shown, and what is shown until when.
    shown: u64,
    showing: Option<(String, Duration)>,
}

#[derive(Component)]
struct Line;

fn start(mut commands: Commands, mut sources: ResMut<Assets<MusicSource>>) {
    let player = Player::new();
    let source = sources.add(MusicSource(player.feed()));
    commands.spawn((Music, AudioPlayer(source), PlaybackSettings::LOOP));
    commands.insert_resource(Jukebox {
        player,
        chosen: None,
        program: Program::Silence,
        station: 0,
        asking: None,
        found: None,
        notice: None,
        shown: 0,
        showing: None,
    });
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            bottom: px(14),
            width: percent(100),
            justify_content: JustifyContent::Center,
            ..default()
        })
        .with_child((
            Line,
            Text::default(),
            TextFont { font_size: FontSize::Px(17.0), ..default() },
            TextColor(Color::srgba(1.0, 1.0, 1.0, 0.85)),
            TextShadow::default(),
        ));
}

/// The directory's answer, when it comes.
fn heard(mut jukebox: ResMut<Jukebox>) {
    let Some((kind, answer)) = jukebox.asking.as_ref().and_then(|(kind, rx)| Some((*kind, rx.try_recv().ok()?))) else {
        return;
    };
    jukebox.asking = None;
    match answer {
        Ok(stations) => jukebox.found = Some((kind, stations)),
        Err(e) => {
            jukebox.notice = Some(format!("the radio directory did not answer: {e}"));
            jukebox.found = Some((kind, Vec::new()));
        }
    }
}

fn choose(time: Res<Time>, audio: Res<Audio>, host: Res<Host>, mut jukebox: ResMut<Jukebox>) {
    if audio.from == MusicFrom::Auto && !host.known && time.elapsed() < WAIT_FOR_HOST {
        return;
    }
    let found = jukebox.found.as_ref().filter(|(kind, _)| Some(*kind) == audio.directory).map_or(0, |f| f.1.len());
    let chosen = Chosen { audio: *audio, host: host.clone(), found, station: jukebox.station };
    if jukebox.chosen.as_ref() == Some(&chosen) {
        return;
    }
    let from = match audio.from {
        MusicFrom::Auto if host.music != 0 => MusicFrom::Host,
        MusicFrom::Auto => MusicFrom::Game,
        other => other,
    };
    let (program, notice) = if !audio.music {
        (Program::Silence, None)
    } else {
        match from {
            MusicFrom::Game | MusicFrom::Auto => game(),
            MusicFrom::Mine => mine(),
            MusicFrom::Host if host.music == 0 => {
                (Program::Silence, Some("the host shares no music (kart-server --music FOLDER)".to_owned()))
            }
            MusicFrom::Host => (Program::Shared(Remote::new(&host.host, host.music)), None),
            MusicFrom::Radio => stations(&mut jukebox, audio.directory),
        }
    };
    let changed_source = jukebox.chosen.as_ref().is_none_or(|c| c.audio != chosen.audio || c.host != chosen.host);
    jukebox.chosen = Some(chosen);
    if let Some(notice) = notice
        && changed_source
    {
        jukebox.notice = Some(notice);
    }
    if program != jukebox.program {
        info!(
            "music: from {}",
            match &program {
                Program::Silence => "nowhere".to_owned(),
                Program::Playlist { tracks, .. } => format!("a playlist of {}", tracks.len()),
                Program::Radio(station) => format!("the station {}", station.name),
                Program::Shared(remote) => format!("the host's share at {}", remote.0),
            }
        );
        let gain = if matches!(program, Program::Radio(_)) { STATIONS } else { PIECES };
        jukebox.player.set_gain(gain);
        jukebox.player.play(program.clone());
        jukebox.program = program;
    }
}

/// The game's own pieces: their files in the content.
const PLAYLIST: &str = include_str!("kart.playlist");

fn game() -> (Program, Option<String>) {
    let folder = osage_content::root().map(|root| root.join("opengameart/music"));
    let tracks = folder.map(|folder| playlist::parse(PLAYLIST, &folder).0);
    let tracks = tracks.map(|tracks| {
        tracks
            .into_iter()
            .filter(|t| matches!(&t.origin, osage_music::Origin::File(p) if p.exists()))
            .collect::<Vec<_>>()
    });
    match tracks {
        Some(tracks) if !tracks.is_empty() => {
            let first = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
                as usize
                % tracks.len();
            (Program::Playlist { tracks, first }, None)
        }
        _ => (Program::Silence, Some("the game's music comes with its content (osage-kart-content.zip)".to_owned())),
    }
}

/// The player's own folder, made if it is not there.
fn mine() -> (Program, Option<String>) {
    let Some(dir) = folder().map(|f| f.join("kart-music")) else {
        return (Program::Silence, Some("no settings folder for your music".to_owned()));
    };
    let _ = std::fs::create_dir_all(&dir);
    match playlist::folder(&dir) {
        Ok(tracks) if !tracks.is_empty() => (Program::Playlist { tracks, first: 0 }, None),
        _ => (Program::Silence, Some(format!("put music files (ogg, mp3, flac, wav, m4a) in {}", dir.display()))),
    }
}

/// The player's list, made with an example if it is not there.
fn radios() -> (Vec<Station>, Option<PathBuf>) {
    let Some(path) = folder().map(|f| f.join("kart-radios.txt")) else { return (Vec::new(), None) };
    if !path.exists() {
        let example = format!(
            "{}\n# Your radio stations, one a line: station NAME | http(s)://STREAM\n# station My station | https://example.org/stream.mp3\n",
            radio::HEADER
        );
        let _ = path.parent().map(std::fs::create_dir_all);
        let _ = std::fs::write(&path, example);
    }
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    (radio::parse(&text).0, Some(path))
}

fn stations(jukebox: &mut Jukebox, directory: Option<&'static str>) -> (Program, Option<String>) {
    let (mut stations, list) = radios();
    if let Some(kind) = directory {
        match &jukebox.found {
            Some((found, more)) if *found == kind => stations.extend(more.iter().cloned()),
            _ if jukebox.asking.as_ref().is_some_and(|(asked, _)| *asked == kind) => {}
            _ => {
                let (tx, rx) = crossbeam_channel::bounded(1);
                let search = radio::Search { tag: kind.to_owned(), limit: ASKED, ..default() };
                let asked = std::thread::Builder::new().name("osage-radio-directory".into()).spawn(move || {
                    let _ = tx.send(radio::search(&search));
                });
                if asked.is_ok() {
                    jukebox.asking = Some((kind, rx));
                }
            }
        }
    }
    if stations.is_empty() {
        let waiting = jukebox.asking.is_some();
        let list = list.map_or_else(String::new, |p| format!(" to {}", p.display()));
        let notice = if waiting {
            "asking the radio directory...".to_owned()
        } else {
            format!("no station: add some{list}, or choose a kind in the settings' radio directory")
        };
        return (Program::Silence, Some(notice));
    }
    (Program::Radio(stations[jukebox.station % stations.len()].clone()), None)
}

/// B: the next piece or station; Shift+B: the one before.
fn skip(keys: Res<ButtonInput<KeyCode>>, mut jukebox: ResMut<Jukebox>) {
    if !keys.just_pressed(KeyCode::KeyB) {
        return;
    }
    let back = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let by = if back { -1 } else { 1 };
    match jukebox.program {
        Program::Playlist { .. } => jukebox.player.skip(by),
        Program::Radio(_) => jukebox.station = jukebox.station.wrapping_add_signed(by),
        Program::Shared(_) => jukebox.notice = Some("the host's playlist plays on: everyone hears the same".to_owned()),
        Program::Silence => {}
    }
}

fn show(time: Res<Time>, mut jukebox: ResMut<Jukebox>, mut lines: Query<&mut Text, With<Line>>) {
    let now = time.elapsed();
    let status: Status = jukebox.player.status();
    if status.changes != jukebox.shown {
        jukebox.shown = status.changes;
        let line = match (&status.playing, &status.problem) {
            (Some(playing), _) => {
                let mut line = format!("Music: {}", playing.title);
                if !playing.artist.is_empty() {
                    line.push_str(&format!(" - {}", playing.artist));
                }
                if !playing.from.is_empty() {
                    line.push_str(&format!("  |  {}", playing.from));
                }
                Some(line)
            }
            (None, Some(problem)) => Some(format!("Music: {problem}")),
            (None, None) => None,
        };
        if let Some(line) = line {
            info!("music: {line}");
            jukebox.showing = Some((line, now + SHOWN));
        }
    }
    if let Some(notice) = jukebox.notice.take() {
        info!("music: {notice}");
        jukebox.showing = Some((format!("Music: {notice}"), now + SHOWN));
    }
    let content = match &jukebox.showing {
        Some((line, until)) if now < *until => line.clone(),
        _ => String::new(),
    };
    for mut text in &mut lines {
        if text.0 != content {
            text.0 = content.clone();
        }
    }
}
