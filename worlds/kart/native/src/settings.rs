// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The player's settings: Escape (or a gamepad's Start) opens them, the
//! arrows change them, and they are kept for the next game in
//! `osage/kart-settings.txt` under the system's settings folder (`%APPDATA%`
//! on Windows, `~/.config` elsewhere).
//!
//! A player racing alone (bots aside) can also end the race from here
//! (Enter).
//!
//! The frame rate wanted: the engine then lowers the picture until frames
//! come at that rate, and raises it when there is room (osage-render's
//! `pace`); "off" keeps the tier's picture whatever the rate. Sound and
//! music, on or off — also N and M at any time in the game. Where the
//! music comes from (`crate::music`): the host's when they share some,
//! else the game's own (auto); the game's own; the player's own folder;
//! the host's; radio stations. And whether the radio directory
//! (radio-browser.info) is asked, and for which kind of music: off unless
//! the player turns it on, as asking it tells its servers what is searched.
//! Voice (`crate::voice`): off — the microphone closed — unless the player
//! chooses push to talk (V held) or an open microphone; and each other
//! player's voice muted or heard (left and right pick them, Space mutes).
//!
//! Up and down pick a setting, left and right change it.

use std::path::PathBuf;

use bevy::{prelude::*, text::FontSize};
use osage_render::{
    Tier,
    pace::{FrameTarget, LADDER, Pace},
};
use osage_world_kart::proto;

use crate::{
    hud::{Lights, RaceView},
    play::CallWanted,
};

/// The rates offered; `None` is off.
const CHOICES: [Option<u32>; 8] = [None, Some(30), Some(45), Some(60), Some(75), Some(90), Some(120), Some(144)];
/// Without settings kept: 60 frames a second.
const DEFAULT: Option<u32> = Some(60);

pub struct SettingsPlugin {
    /// From the command line: wins over the settings kept, without changing them.
    pub fps: Option<Option<u32>>,
}

/// The settings are open: the keys are theirs, not the kart's.
#[derive(Resource, Default)]
pub struct Menu(pub bool);

#[derive(Resource, Clone, Copy, PartialEq, Debug)]
struct Settings {
    fps: Option<u32>,
    sound: bool,
    music: bool,
    from: MusicFrom,
    /// The kind of music asked of the radio directory; none: not asked.
    directory: Option<usize>,
    voice: VoiceMode,
    /// The picture's quality, a tier (T1 to T4), from the next start; none:
    /// the game's default (T2) or the command line's.
    quality: Option<Tier>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            fps: DEFAULT,
            sound: true,
            music: true,
            from: MusicFrom::Auto,
            directory: None,
            voice: VoiceMode::Off,
            quality: None,
        }
    }
}

/// The quality the player chose in the settings, for the next start: what
/// `main` starts with when the command line says no `--tier`.
pub fn kept_quality() -> Option<Tier> {
    load().and_then(|settings| settings.quality)
}

/// Whether the player's microphone is heard by the others.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum VoiceMode {
    /// The microphone closed; the others still heard.
    #[default]
    Off,
    /// Heard while V is held.
    PushToTalk,
    /// Heard whenever the player speaks louder than the room.
    Open,
}

impl VoiceMode {
    const ALL: [Self; 3] = [Self::Off, Self::PushToTalk, Self::Open];

    fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::PushToTalk => "push-to-talk",
            Self::Open => "open",
        }
    }

    fn shown(self) -> &'static str {
        match self {
            Self::Off => "microphone off",
            Self::PushToTalk => "push to talk (hold V)",
            Self::Open => "open microphone",
        }
    }
}

/// The other players in the voice conference, for the settings to show and
/// mute: filled by `crate::voice`, which takes `toggle` back.
#[derive(Resource, Default)]
pub struct Voices {
    /// (name, muted, talking), the player left out.
    pub others: Vec<(String, bool, bool)>,
    pub picked: usize,
    /// A name whose muting the player switched.
    pub toggle: Option<String>,
}

/// Where the music comes from.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum MusicFrom {
    /// The host's when they share some, else the game's own.
    #[default]
    Auto,
    /// The game's own pieces.
    Game,
    /// The player's own folder (`osage/kart-music/`).
    Mine,
    /// The host's shared playlist.
    Host,
    /// Radio stations: the player's list (`osage/kart-radios.txt`), and the
    /// directory's when it is asked.
    Radio,
}

impl MusicFrom {
    const ALL: [Self; 5] = [Self::Auto, Self::Game, Self::Mine, Self::Host, Self::Radio];

    fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Game => "game",
            Self::Mine => "mine",
            Self::Host => "host",
            Self::Radio => "radio",
        }
    }

    fn shown(self) -> &'static str {
        match self {
            Self::Auto => "host's, else the game's",
            Self::Game => "the game's",
            Self::Mine => "my folder",
            Self::Host => "the host's",
            Self::Radio => "radio",
        }
    }
}

/// The kinds of music the radio directory may be asked for.
pub const KINDS: [&str; 8] = ["electronic", "dance", "rock", "funk", "synthwave", "chillout", "jazz", "pop"];

/// Whether the game's sounds and its music are heard, and where the music
/// comes from: what the sound and music systems follow.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Audio {
    pub sound: bool,
    pub music: bool,
    pub from: MusicFrom,
    /// The kind of music asked of the radio directory; none: not asked.
    pub directory: Option<&'static str>,
    pub voice: VoiceMode,
}

impl Audio {
    fn of(settings: &Settings) -> Self {
        Self {
            sound: settings.sound,
            music: settings.music,
            from: settings.from,
            directory: settings.directory.map(|kind| KINDS[kind % KINDS.len()]),
            voice: settings.voice,
        }
    }
}

/// The setting the arrows change.
#[derive(Resource, Default)]
struct Row(usize);

const ROWS: usize = 8;

#[derive(Component)]
struct Panel;

#[derive(Component)]
struct Lines;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        let kept = load().unwrap_or_default();
        let settings = Settings { fps: self.fps.unwrap_or(kept.fps), ..kept };
        app.insert_resource(settings)
            .insert_resource(FrameTarget(settings.fps.map(|fps| fps as f32)))
            .insert_resource(Audio::of(&settings))
            .init_resource::<Menu>()
            .init_resource::<Voices>()
            .init_resource::<Row>()
            .add_systems(Startup, setup)
            .add_systems(Update, (toggle, quick, choose, show, meter).chain());
    }
}

/// The player's osage folder, under the system's settings folder.
pub fn folder() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("osage"))
}

fn file() -> Option<PathBuf> {
    Some(folder()?.join("kart-settings.txt"))
}

/// `fps = 60` or `off`, `sound = on` or `off`, `music = on` or `off`,
/// `music-from = auto|game|mine|host|radio`, `radio-directory = off` or a
/// kind of music, `voice = off|push-to-talk|open`; a line that does not
/// read is ignored, a setting not written is its default. None when
/// nothing reads.
fn parse(text: &str) -> Option<Settings> {
    let mut settings = Settings::default();
    let mut any = false;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        let value = value.trim();
        let on = match value.to_ascii_lowercase().as_str() {
            "on" => Some(true),
            "off" => Some(false),
            _ => None,
        };
        match (key.trim(), on) {
            ("fps", _) => {
                if let Some(fps) = fps(value) {
                    settings.fps = fps;
                    any = true;
                }
            }
            ("sound", Some(on)) => (settings.sound, any) = (on, true),
            ("music", Some(on)) => (settings.music, any) = (on, true),
            ("music-from", _) => {
                if let Some(from) = MusicFrom::ALL.into_iter().find(|f| f.name().eq_ignore_ascii_case(value)) {
                    (settings.from, any) = (from, true);
                }
            }
            ("voice", _) => {
                if let Some(mode) = VoiceMode::ALL.into_iter().find(|m| m.name().eq_ignore_ascii_case(value)) {
                    (settings.voice, any) = (mode, true);
                }
            }
            ("quality", _) if value.eq_ignore_ascii_case("auto") => (settings.quality, any) = (None, true),
            ("quality", _) => {
                if let Ok(tier) = value.parse::<Tier>() {
                    (settings.quality, any) = (Some(tier), true);
                }
            }
            ("radio-directory", Some(false)) => (settings.directory, any) = (None, true),
            ("radio-directory", _) => {
                if let Some(kind) = KINDS.iter().position(|k| k.eq_ignore_ascii_case(value)) {
                    (settings.directory, any) = (Some(kind), true);
                }
            }
            _ => {}
        }
    }
    any.then_some(settings)
}

/// A frame rate as written: a number, or "off".
fn fps(text: &str) -> Option<Option<u32>> {
    if text.eq_ignore_ascii_case("off") {
        return Some(None);
    }
    text.parse::<u32>().ok().filter(|n| (10..=500).contains(n)).map(Some)
}

/// A frame rate from the command line (`--fps 60`, `--fps off`).
pub fn parse_fps(text: &str) -> Result<Option<u32>, String> {
    fps(text).ok_or_else(|| format!("--fps takes a number of frames a second (10 to 500) or off, not {text}"))
}

fn load() -> Option<Settings> {
    parse(&std::fs::read_to_string(file()?).ok()?)
}

fn save(settings: Settings) {
    let Some(path) = file() else { return };
    let value = settings.fps.map_or_else(|| "off".to_owned(), |n| n.to_string());
    let on = |on: bool| if on { "on" } else { "off" };
    let text = format!(
        "# osage kart settings\nfps = {value}\nsound = {}\nmusic = {}\nmusic-from = {}\nradio-directory = {}\nvoice = {}\nquality = {}\n",
        on(settings.sound),
        on(settings.music),
        settings.from.name(),
        settings.directory.map_or("off", |kind| KINDS[kind % KINDS.len()]),
        settings.voice.name(),
        settings.quality.map_or_else(|| "auto".to_owned(), |tier| tier.to_string()),
    );
    let written = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&path, text));
    if let Err(error) = written {
        warn!("settings not kept in {}: {error}", path.display());
    }
}

fn setup(mut commands: Commands) {
    commands
        .spawn((
            Panel,
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                top: percent(30),
                left: percent(50),
                margin: UiRect::left(px(-210)),
                width: px(420),
                padding: UiRect::all(px(18)),
                border_radius: BorderRadius::all(px(12)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.06, 0.1, 0.88)),
            GlobalZIndex(10),
        ))
        .with_child((Lines, Text::default(), TextFont { font_size: FontSize::Px(18.0), ..default() }));
    commands.spawn((
        Meter,
        Text::default(),
        TextFont { font_size: FontSize::Px(13.0), ..default() },
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.7)),
        Node { position_type: PositionType::Absolute, bottom: px(6), right: px(10), ..default() },
        GlobalZIndex(9),
    ));
}

fn toggle(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut menu: ResMut<Menu>,
    mut panels: Query<&mut Node, With<Panel>>,
) {
    let pressed =
        keys.just_pressed(KeyCode::Escape) || gamepads.iter().any(|pad| pad.just_pressed(GamepadButton::Start));
    if !pressed {
        return;
    }
    menu.0 = !menu.0;
    for mut node in &mut panels {
        node.display = if menu.0 { Display::Flex } else { Display::None };
    }
}

/// N and M, at any time: sound and music on or off, kept.
fn quick(keys: Res<ButtonInput<KeyCode>>, mut settings: ResMut<Settings>, mut audio: ResMut<Audio>) {
    let (sound, music) = (keys.just_pressed(KeyCode::KeyN), keys.just_pressed(KeyCode::KeyM));
    if !sound && !music {
        return;
    }
    settings.sound ^= sound;
    settings.music ^= music;
    *audio = Audio::of(&settings);
    save(*settings);
}

/// Whether the player may end the race now: racing, alone.
fn may_end(view: &RaceView) -> bool {
    view.alone && matches!(view.stage, Lights::Go | Lights::Racing)
}

#[allow(clippy::too_many_arguments)]
fn choose(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    view: Res<RaceView>,
    mut menu: ResMut<Menu>,
    mut call: ResMut<CallWanted>,
    mut panels: Query<&mut Node, With<Panel>>,
    mut settings: ResMut<Settings>,
    mut target: ResMut<FrameTarget>,
    mut audio: ResMut<Audio>,
    mut row: ResMut<Row>,
    mut voices: ResMut<Voices>,
) {
    if !menu.0 {
        return;
    }
    if row.0 == 6 && keys.just_pressed(KeyCode::Space) {
        let picked = voices.others.get(voices.picked).map(|(name, ..)| name.clone());
        voices.toggle = picked;
        return;
    }
    let pad = |button| gamepads.iter().any(|pad| pad.just_pressed(button));
    if keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::KeyS) || pad(GamepadButton::DPadDown) {
        row.0 = (row.0 + 1) % ROWS;
    }
    if keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyW) || pad(GamepadButton::DPadUp) {
        row.0 = (row.0 + ROWS - 1) % ROWS;
    }
    if keys.just_pressed(KeyCode::Enter) && may_end(&view) {
        call.0 = Some(proto::RaceCall::Stop);
        menu.0 = false;
        for mut node in &mut panels {
            node.display = Display::None;
        }
        return;
    }
    let step = if keys.just_pressed(KeyCode::ArrowRight)
        || keys.just_pressed(KeyCode::KeyD)
        || pad(GamepadButton::DPadRight)
    {
        1
    } else if keys.just_pressed(KeyCode::ArrowLeft) || keys.just_pressed(KeyCode::KeyA) || pad(GamepadButton::DPadLeft)
    {
        -1
    } else {
        return;
    };
    match row.0 {
        0 => {
            settings.fps = next(settings.fps, step);
            target.0 = settings.fps.map(|fps| fps as f32);
        }
        1 => settings.sound = !settings.sound,
        2 => settings.music = !settings.music,
        3 => {
            let at = MusicFrom::ALL.iter().position(|f| *f == settings.from).unwrap_or(0) as i32;
            let n = MusicFrom::ALL.len() as i32;
            settings.from = MusicFrom::ALL[(at + step).rem_euclid(n) as usize];
        }
        5 => {
            let at = VoiceMode::ALL.iter().position(|m| *m == settings.voice).unwrap_or(0) as i32;
            settings.voice = VoiceMode::ALL[(at + step).rem_euclid(VoiceMode::ALL.len() as i32) as usize];
        }
        6 => {
            let n = voices.others.len().max(1) as i32;
            voices.picked = (voices.picked as i32 + step).rem_euclid(n) as usize;
            return;
        }
        7 => {
            // Auto, then T1 to T4; from the next start.
            let all = [None, Some(Tier::T1), Some(Tier::T2), Some(Tier::T3), Some(Tier::T4)];
            let at = all.iter().position(|q| *q == settings.quality).unwrap_or(0) as i32;
            settings.quality = all[(at + step).rem_euclid(all.len() as i32) as usize];
        }
        _ => {
            // Off, then each kind: the directory is asked only once one is chosen.
            let n = KINDS.len() as i32 + 1;
            let at = settings.directory.map_or(0, |kind| kind as i32 + 1);
            let next = (at + step).rem_euclid(n);
            settings.directory = (next > 0).then(|| next as usize - 1);
        }
    }
    *audio = Audio::of(&settings);
    save(*settings);
}

/// A tier as the settings name it.
fn quality_name(tier: Tier) -> &'static str {
    match tier {
        Tier::T1 => "T1 (old integrated graphics)",
        Tier::T2 => "T2 (integrated graphics)",
        Tier::T3 => "T3 (graphics card)",
        Tier::T4 => "T4 (ray tracing card)",
    }
}

/// The frame rate and the tier, small, in a corner, always.
#[derive(Component)]
struct Meter;

fn meter(pace: Res<Pace>, tier: Res<Tier>, mut texts: Query<&mut Text, With<Meter>>) {
    let wanted = format!("{:.0} fps  {}", pace.fps, *tier);
    for mut text in &mut texts {
        if text.0 != wanted {
            text.0.clone_from(&wanted);
        }
    }
}

/// The choice beside `now`; a rate from the command line that is not among
/// the choices goes to the nearest one that is.
fn next(now: Option<u32>, step: i32) -> Option<u32> {
    let at = CHOICES.iter().position(|c| *c == now).unwrap_or_else(|| {
        let n = now.unwrap_or(0);
        CHOICES.iter().rposition(|c| c.is_none_or(|c| c <= n)).unwrap_or(0)
    });
    let last = CHOICES.len() as i32 - 1;
    CHOICES[(at as i32 + step).clamp(0, last) as usize]
}

#[allow(clippy::too_many_arguments)]
fn show(
    menu: Res<Menu>,
    row: Res<Row>,
    settings: Res<Settings>,
    pace: Res<Pace>,
    view: Res<RaceView>,
    voices: Res<Voices>,
    tier: Res<Tier>,
    mut lines: Query<&mut Text, With<Lines>>,
) {
    if !menu.0 {
        return;
    }
    let wanted = settings.fps.map_or_else(|| "off".to_owned(), |n| format!("{n} fps"));
    let rung = pace.rung();
    let picture = match pace.aim {
        None => "the tier's own".to_owned(),
        Some(aim) => {
            let level = LADDER.len() - pace.rung;
            let mut parts =
                vec![format!("level {level}/{}", LADDER.len()), format!("{:.0} % size", rung.scale * 100.0)];
            if !rung.effects {
                parts.push("effects off".to_owned());
            }
            if !rung.shadows {
                parts.push("no shadows".to_owned());
            }
            if settings.fps.is_some_and(|fps| aim < fps as f32 - 0.5) {
                parts.push(format!("screen holds {aim:.0}"));
            }
            parts.join(" · ")
        }
    };
    let end = if may_end(&view) { "\nEnter  end this race (you race alone)" } else { "" };
    let mark = |r: usize| if row.0 == r { ">" } else { " " };
    let on = |on: bool| if on { "on" } else { "off" };
    let content = format!(
        "SETTINGS\n\n{} Frame rate wanted   <  {wanted}  >\n  Now {:.0} fps\n  Picture: {picture}\n\n{} Sound   <  {}  >   (N)\n{} Music   <  {}  >   (M, B next)\n{} Music from   <  {}  >\n{} Radio directory   <  {}  >\n  (off: radio-browser.info is not asked)\n{} Voice   <  {}  >\n{} Mute   <  {}  >   (Space)\n{} Quality   <  {}  >\n  now {tier}; a change takes effect at the next start\n{end}\n↑ ↓ choose    ← → change    Esc close",
        mark(0),
        pace.fps,
        mark(1),
        on(settings.sound),
        mark(2),
        on(settings.music),
        mark(3),
        settings.from.shown(),
        mark(4),
        settings.directory.map_or("off", |kind| KINDS[kind % KINDS.len()]),
        mark(5),
        settings.voice.shown(),
        mark(6),
        voices.others.get(voices.picked).map_or_else(
            || "nobody else talking".to_owned(),
            |(name, muted, _)| format!("{name}: {}", if *muted { "muted" } else { "heard" })
        ),
        mark(7),
        settings.quality.map_or_else(|| "auto (T2)".to_owned(), |tier| quality_name(tier).to_owned()),
        tier = *tier,
    );
    for mut text in &mut lines {
        if text.0 != content {
            text.0 = content.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_read_back_what_was_written() {
        assert_eq!(parse("# osage kart settings\nfps = 45\n"), Some(Settings { fps: Some(45), ..Settings::default() }));
        assert_eq!(parse("fps = off"), Some(Settings { fps: None, ..Settings::default() }));
        assert_eq!(parse("fps = fast"), None);
        let all = parse("fps = 60\nsound = off\nmusic = on\nmusic-from = radio\nradio-directory = funk\n").unwrap();
        assert_eq!(
            all,
            Settings {
                fps: Some(60),
                sound: false,
                music: true,
                from: MusicFrom::Radio,
                directory: Some(3),
                voice: VoiceMode::Off,
                quality: None
            }
        );
        assert_eq!(parse("voice = push-to-talk").map(|s| s.voice), Some(VoiceMode::PushToTalk));
        assert_eq!(parse("quality = t4").map(|s| s.quality), Some(Some(Tier::T4)));
        assert_eq!(parse("quality = auto").map(|s| s.quality), Some(None));
        assert_eq!(
            parse("radio-directory = off\nmusic-from = nowhere").map(|s| (s.directory, s.from)),
            Some((None, MusicFrom::Auto))
        );
        assert_eq!(parse("music = OFF").map(|s| (s.sound, s.music)), Some((true, false)));
        assert_eq!(parse_fps("5"), Err("--fps takes a number of frames a second (10 to 500) or off, not 5".into()));
    }

    #[test]
    fn choices_step_and_stop_at_the_ends() {
        assert_eq!(next(Some(60), 1), Some(75));
        assert_eq!(next(Some(60), -1), Some(45));
        assert_eq!(next(None, -1), None);
        assert_eq!(next(Some(144), 1), Some(144));
        // 50 from the command line: between 45 and 60.
        assert_eq!(next(Some(50), 1), Some(60));
    }
}
