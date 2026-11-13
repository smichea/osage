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

use bevy::prelude::*;
use osage_render::pace::{FrameTarget, LADDER, Pace};
use osage_world_kart::proto;

use crate::{
    hud::{Lights, RaceView},
    play::CallWanted,
    theme::{self, Arrows, Faces, Key},
};

/// The rates offered; `None` is off.
const CHOICES: [Option<u32>; 8] = [None, Some(30), Some(45), Some(60), Some(75), Some(90), Some(120), Some(144)];
/// Without settings kept: 60 frames a second.
/// The frame rate wanted unless the player chose: 30, the whole picture
/// kept more often (the maintainer's choice, D-074).
const DEFAULT: Option<u32> = Some(30);

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
}

impl Default for Settings {
    fn default() -> Self {
        Self { fps: DEFAULT, sound: true, music: true, from: MusicFrom::Auto, directory: None, voice: VoiceMode::Off }
    }
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

const ROWS: usize = 7;

/// The settings' panel, shown while they are open.
#[derive(Component)]
pub struct Panel;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        let kept = load().unwrap_or_default();
        let settings = Settings { fps: self.fps.unwrap_or(kept.fps), ..kept };
        if !app.is_plugin_added::<theme::ThemePlugin>() {
            app.add_plugins(theme::ThemePlugin);
        }
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
        "# osage kart settings\nfps = {value}\nsound = {}\nmusic = {}\nmusic-from = {}\nradio-directory = {}\nvoice = {}\n",
        on(settings.sound),
        on(settings.music),
        settings.from.name(),
        settings.directory.map_or("off", |kind| KINDS[kind % KINDS.len()]),
        settings.voice.name(),
    );
    let written = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&path, text));
    if let Err(error) = written {
        warn!("settings not kept in {}: {error}", path.display());
    }
}

/// Each setting's name, and the key that changes it at any time.
const NAMES: [(&str, Option<&str>); ROWS] = [
    ("Frame rate", None),
    ("Sound", Some("N")),
    ("Music", Some("M")),
    ("Music from", None),
    ("Radio directory", None),
    ("Voice", None),
    ("Mute", Some("SPACE")),
];

/// A setting's row: its name, and its value.
#[derive(Component)]
struct Line(usize);

#[derive(Component)]
struct Value(usize);

/// What the setting chosen does, under the rows.
#[derive(Component)]
struct Detail;

/// The hint that Enter ends the race, shown when it may.
#[derive(Component)]
struct EndHint;

fn setup(mut commands: Commands, faces: Res<Faces>, arrows: Res<Arrows>) {
    let (node, _, edge) = theme::panel(Node {
        display: Display::None,
        position_type: PositionType::Absolute,
        top: percent(22),
        left: percent(50),
        margin: UiRect::left(px(-240)),
        width: px(480),
        flex_direction: FlexDirection::Column,
        row_gap: px(3),
        padding: UiRect::all(px(18)),
        ..default()
    });
    commands.spawn((Panel, node, BackgroundColor(theme::DIALOG), edge, GlobalZIndex(10))).with_children(|panel| {
        panel
            .spawn(Node {
                column_gap: px(10),
                align_items: AlignItems::Center,
                margin: UiRect::bottom(px(10)),
                ..default()
            })
            .with_children(|title| {
                title.spawn((Node { width: px(4), height: px(22), ..default() }, BackgroundColor(theme::ACCENT)));
                title.spawn(theme::heavy("SETTINGS", 22.0, theme::TEXT));
            });
        for (i, (name, key)) in NAMES.iter().enumerate() {
            panel
                .spawn((
                    Line(i),
                    Node {
                        justify_content: JustifyContent::SpaceBetween,
                        align_items: AlignItems::Center,
                        padding: UiRect::axes(px(10), px(6)),
                        border_radius: BorderRadius::all(px(6)),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                ))
                .with_children(|line| {
                    line.spawn(Node { column_gap: px(8), align_items: AlignItems::Center, ..default() }).with_children(
                        |left| {
                            left.spawn(theme::light(*name, 16.0, theme::TEXT, &faces));
                            if let Some(key) = key {
                                left.spawn(theme::heavy(*key, 10.0, theme::MUTED));
                            }
                        },
                    );
                    line.spawn((Value(i), theme::heavy("", 16.0, theme::TEXT)));
                });
        }
        panel.spawn((
            Detail,
            theme::light("", 13.0, theme::MUTED, &faces),
            // Two lines' room, whichever setting is chosen.
            Node { margin: UiRect::new(px(10), px(10), px(8), px(4)), height: px(40), ..default() },
        ));
        panel
            .spawn((
                Node {
                    column_gap: px(16),
                    flex_wrap: FlexWrap::Wrap,
                    row_gap: px(6),
                    padding: UiRect::top(px(12)),
                    border: UiRect::top(px(1)),
                    ..default()
                },
                BorderColor::all(theme::EDGE),
            ))
            .with_children(|bar| {
                theme::hint(bar, &[Key::Arrow(0), Key::Arrow(2)], "CHOOSE", &arrows);
                theme::hint(bar, &[Key::Arrow(3), Key::Arrow(1)], "CHANGE", &arrows);
                let end = theme::hint(bar, &[Key::Word("ENTER")], "END RACE", &arrows);
                bar.commands().entity(end).insert(EndHint);
                theme::hint(bar, &[Key::Word("ESC")], "CLOSE", &arrows);
            });
    });
    commands.spawn((
        Meter,
        theme::light("", 12.0, theme::MUTED, &faces),
        // Bottom centre: the corners are the HUD's.
        Node {
            position_type: PositionType::Absolute,
            bottom: px(6),
            left: percent(50),
            width: px(80),
            margin: UiRect::left(px(-40)),
            justify_content: JustifyContent::Center,
            ..default()
        },
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

/// The frame rate, small, in a corner, always.
#[derive(Component)]
struct Meter;

fn meter(pace: Res<Pace>, mut texts: Query<&mut Text, With<Meter>>) {
    let wanted = format!("{:.0} FPS", pace.fps);
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

/// The words with a capital first letter.
fn capital(words: &str) -> String {
    let mut chars = words.chars();
    chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn show(
    menu: Res<Menu>,
    row: Res<Row>,
    settings: Res<Settings>,
    pace: Res<Pace>,
    view: Res<RaceView>,
    voices: Res<Voices>,
    mut lines: Query<(&Line, &mut BackgroundColor)>,
    mut values: Query<(&Value, &mut Text, &mut TextColor), Without<Detail>>,
    mut detail: Query<&mut Text, (With<Detail>, Without<Value>)>,
    mut end: Query<&mut Node, With<EndHint>>,
) {
    if !menu.0 {
        return;
    }
    let rung = pace.rung();
    let picture = match pace.aim {
        None => "picture as this computer started".to_owned(),
        Some(aim) => {
            let level = LADDER.len() - pace.rung;
            let mut parts =
                vec![format!("picture level {level} of {}", LADDER.len()), format!("{:.0} % size", rung.scale * 100.0)];
            if !rung.effects {
                parts.push("effects off".to_owned());
            }
            if !rung.shadows {
                parts.push("no shadows".to_owned());
            }
            if settings.fps.is_some_and(|fps| aim < fps as f32 - 0.5) {
                parts.push(format!("the screen holds {aim:.0}"));
            }
            parts.join(" · ")
        }
    };
    let on = |on: bool| if on { "On" } else { "Off" }.to_owned();
    let value = |r: usize| match r {
        0 => settings.fps.map_or_else(|| "Off".to_owned(), |n| format!("{n} fps")),
        1 => on(settings.sound),
        2 => on(settings.music),
        3 => capital(settings.from.shown()),
        4 => capital(settings.directory.map_or("Off", |kind| KINDS[kind % KINDS.len()])),
        5 => capital(settings.voice.shown()),
        _ => voices.others.get(voices.picked).map_or_else(
            || "Nobody else talking".to_owned(),
            |(name, muted, _)| format!("{name}: {}", if *muted { "muted" } else { "heard" }),
        ),
    };
    for (line, mut back) in &mut lines {
        let wanted = if line.0 == row.0 { Color::srgba(1.0, 0.72, 0.2, 0.2) } else { Color::NONE };
        if back.0 != wanted {
            back.0 = wanted;
        }
    }
    for (at, mut text, mut colour) in &mut values {
        let chosen = at.0 == row.0;
        // The one chosen shows that the arrows change it.
        let wanted = if chosen { format!("‹  {}  ›", value(at.0)) } else { value(at.0) };
        if text.0 != wanted {
            text.0 = wanted;
        }
        let tint = if chosen { theme::ACCENT } else { theme::TEXT };
        if colour.0 != tint {
            colour.0 = tint;
        }
    }
    let about = match row.0 {
        0 => format!(
            "Picture lowered until frames come this often, raised when there is room.\nNow {:.0} fps · {picture}",
            pace.fps
        ),
        1 => "N turns the sound on or off at any time in the game.".to_owned(),
        2 => "M turns the music on or off at any time; B skips to the next piece.".to_owned(),
        3 => "Auto: the host's music when they share some, else the game's own.".to_owned(),
        4 => "Off: radio-browser.info is not asked. On, it learns the kind of music searched.".to_owned(),
        5 => "Off closes the microphone. Push to talk: hold V.".to_owned(),
        _ => "Left and right pick another driver; Space mutes or unmutes them.".to_owned(),
    };
    for mut text in &mut detail {
        if text.0 != about {
            text.0.clone_from(&about);
        }
    }
    let shown = if may_end(&view) { Display::Flex } else { Display::None };
    for mut node in &mut end {
        if node.display != shown {
            node.display = shown;
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
                voice: VoiceMode::Off
            }
        );
        assert_eq!(parse("voice = push-to-talk").map(|s| s.voice), Some(VoiceMode::PushToTalk));
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
