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
//! One setting for now: the frame rate wanted. The engine then lowers the
//! picture until frames come at that rate, and raises it when there is
//! room (osage-render's `pace`); "off" keeps the tier's picture whatever
//! the rate.

use std::path::PathBuf;

use bevy::{prelude::*, text::FontSize};
use osage_render::pace::{FrameTarget, LADDER, Pace};
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
}

#[derive(Component)]
struct Panel;

#[derive(Component)]
struct Lines;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        let settings = Settings { fps: self.fps.unwrap_or_else(|| load().map_or(DEFAULT, |s| s.fps)) };
        app.insert_resource(settings)
            .insert_resource(FrameTarget(settings.fps.map(|fps| fps as f32)))
            .init_resource::<Menu>()
            .add_systems(Startup, setup)
            .add_systems(Update, (toggle, choose, show).chain());
    }
}

fn file() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("osage").join("kart-settings.txt"))
}

/// `fps = 60`, or `fps = off`; anything else is ignored.
fn parse(text: &str) -> Option<Settings> {
    text.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == "fps").then(|| value.trim()).and_then(fps)
    })
}

/// A frame rate as written: a number, or "off".
fn fps(text: &str) -> Option<Settings> {
    if text.eq_ignore_ascii_case("off") {
        return Some(Settings { fps: None });
    }
    text.parse::<u32>().ok().filter(|n| (10..=500).contains(n)).map(|n| Settings { fps: Some(n) })
}

/// A frame rate from the command line (`--fps 60`, `--fps off`).
pub fn parse_fps(text: &str) -> Result<Option<u32>, String> {
    fps(text)
        .map(|s| s.fps)
        .ok_or_else(|| format!("--fps takes a number of frames a second (10 to 500) or off, not {text}"))
}

fn load() -> Option<Settings> {
    parse(&std::fs::read_to_string(file()?).ok()?)
}

fn save(settings: Settings) {
    let Some(path) = file() else { return };
    let value = settings.fps.map_or_else(|| "off".to_owned(), |n| n.to_string());
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&path, format!("# osage kart settings\nfps = {value}\n")));
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
) {
    if !menu.0 {
        return;
    }
    if keys.just_pressed(KeyCode::Enter) && may_end(&view) {
        call.0 = Some(proto::RaceCall::Stop);
        menu.0 = false;
        for mut node in &mut panels {
            node.display = Display::None;
        }
        return;
    }
    let pad = |button| gamepads.iter().any(|pad| pad.just_pressed(button));
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
    settings.fps = next(settings.fps, step);
    target.0 = settings.fps.map(|fps| fps as f32);
    save(*settings);
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

fn show(
    menu: Res<Menu>,
    settings: Res<Settings>,
    pace: Res<Pace>,
    view: Res<RaceView>,
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
    let content = format!(
        "SETTINGS\n\nFrame rate wanted   <  {wanted}  >\n\nNow {:.0} fps\nPicture: {picture}\n{end}\n← → change    Esc close",
        pace.fps
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
        assert_eq!(parse("# osage kart settings\nfps = 45\n"), Some(Settings { fps: Some(45) }));
        assert_eq!(parse("fps = off"), Some(Settings { fps: None }));
        assert_eq!(parse("fps = fast"), None);
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
