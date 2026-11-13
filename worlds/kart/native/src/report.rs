// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The game's crash reports (D-073, [`osage_report`]): kept in
//! `osage/kart-reports/`, the run's log among them. If the last run
//! panicked, never finished starting, or its renderer reported errors, the
//! game asks the player once whether to send such reports — cleaned of
//! their name and addresses — to the developers' relay, which files them
//! as GitHub issues and has DeepSeek read them; their answer is kept
//! (`consent`: `always` or `never`; deleting the file asks again).

use std::{path::PathBuf, sync::OnceLock, time::Duration};

use bevy::prelude::*;
use osage_report::{Report, Watch};

use crate::{
    hud::{Lights, RaceView},
    theme::{self, Arrows, Faces, Key},
};

/// Where the developers' relay takes reports (D-073).
pub const RELAY: &str = "https://osage-crash-relay-841931972409.europe-west1.run.app/report";
/// A start that has not finished after this long is a start that hung.
const PATIENCE: Duration = Duration::from_secs(120);

static WATCH: OnceLock<Watch> = OnceLock::new();
/// The run ends on purpose but went wrong (the picture of one colour,
/// D-075): its marker stays, for the next start to report.
static KEEP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Leaves this run to be reported at the next start, however it ends.
pub fn keep_for_next() {
    KEEP.store(true, std::sync::atomic::Ordering::Relaxed);
}

fn folder() -> Option<PathBuf> {
    Some(crate::settings::folder()?.join("kart-reports"))
}

/// Starts watching this run (a game, not the bench); the last run's report.
pub fn begin() -> Option<Report> {
    let (watch, report) = Watch::start(&folder()?, PATIENCE);
    let _ = WATCH.set(watch);
    report
}

/// The log's file, for Bevy's `LogPlugin`.
pub fn layer(_: &mut App) -> Option<bevy::log::BoxedLayer> {
    osage_report::file_layer()
}

/// A clean exit.
pub fn finish() {
    if KEEP.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    if let Some(watch) = WATCH.get() {
        watch.finish();
    }
}

/// What the player said about sending reports.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Consent {
    Ask,
    Always,
    Never,
}

fn consent() -> Consent {
    match folder().and_then(|f| std::fs::read_to_string(f.join("consent")).ok()).as_deref().map(str::trim) {
        Some("always") => Consent::Always,
        Some("never") => Consent::Never,
        _ => Consent::Ask,
    }
}

fn keep(consent: Consent) {
    let word = match consent {
        Consent::Always => "always",
        Consent::Never => "never",
        Consent::Ask => return,
    };
    if let Some(folder) = folder() {
        let _ = std::fs::write(folder.join("consent"), format!("{word}\n"));
    }
}

/// The last run's report, waiting for the player's word.
#[derive(Resource)]
pub struct Pending(pub Option<Report>);

#[derive(Component)]
struct Asking;

pub struct ReportPlugin;

impl Plugin for ReportPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<theme::ThemePlugin>() {
            app.add_plugins(theme::ThemePlugin);
        }
        app.add_systems(Startup, ask).add_systems(Update, (answer, started));
    }
}

fn kind(report: &Report) -> &'static str {
    match report.kind {
        osage_report::Kind::Panic => "the game stopped unexpectedly",
        osage_report::Kind::Start => "the game did not finish starting",
        osage_report::Kind::Render => "the graphics reported errors",
    }
}

fn ask(
    mut commands: Commands,
    mut pending: ResMut<Pending>,
    tier: Res<osage_render::Tier>,
    faces: Res<Faces>,
    arrows: Res<Arrows>,
) {
    let Some(report) = pending.0.as_ref() else { return };
    match consent() {
        Consent::Never => pending.0 = None,
        Consent::Always => {
            send(pending.0.take(), *tier);
        }
        Consent::Ask => {
            prompt(&mut commands, &faces, &arrows, format!("Last time, {}.", kind(report)));
        }
    }
}

/// The question, on screen: what went wrong, what a report carries, the
/// answers' keys. Its entity.
pub fn prompt(commands: &mut Commands, faces: &Faces, arrows: &Arrows, what: String) -> Entity {
    let (node, _, edge) = theme::panel(Node {
        position_type: PositionType::Absolute,
        top: percent(18),
        left: percent(50),
        margin: UiRect::left(px(-300)),
        width: px(600),
        flex_direction: FlexDirection::Column,
        row_gap: px(10),
        padding: UiRect::all(px(22)),
        ..default()
    });
    commands
        .spawn((Asking, node, BackgroundColor(theme::DIALOG), edge, GlobalZIndex(20)))
        .with_children(|panel| {
            panel.spawn(Node { column_gap: px(10), align_items: AlignItems::Center, ..default() }).with_children(
                |title| {
                    title.spawn((Node { width: px(4), height: px(22), ..default() }, BackgroundColor(theme::ACCENT)));
                    title.spawn(theme::heavy("SEND A REPORT?", 22.0, theme::TEXT));
                },
            );
            panel.spawn(theme::heavy(what, 16.0, theme::ACCENT));
            panel.spawn(theme::light(
                "A report carries the error and the end of the game's log, without your name, \
                 your folders or any address. It is published as an issue on GitHub \
                 (smichea/osage) and read by DeepSeek, to help the developers fix it.",
                14.0,
                theme::TEXT,
                faces,
            ));
            panel
                .spawn((
                    Node {
                        column_gap: px(18),
                        flex_wrap: FlexWrap::Wrap,
                        row_gap: px(6),
                        padding: UiRect::top(px(12)),
                        margin: UiRect::top(px(4)),
                        border: UiRect::top(px(1)),
                        ..default()
                    },
                    BorderColor::all(theme::EDGE),
                ))
                .with_children(|bar| {
                    theme::hint(bar, &[Key::Word("Y")], "SEND", arrows);
                    theme::hint(bar, &[Key::Word("A")], "ALWAYS SEND", arrows);
                    theme::hint(bar, &[Key::Word("N")], "NOT NOW", arrows);
                    theme::hint(bar, &[Key::Word("X")], "NEVER", arrows);
                });
        })
        .id()
}

fn answer(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut pending: ResMut<Pending>,
    tier: Res<osage_render::Tier>,
    asking: Query<Entity, With<Asking>>,
) {
    if pending.0.is_none() || asking.is_empty() {
        return;
    }
    let choice = if keys.just_pressed(KeyCode::KeyY) {
        Some((true, Consent::Ask))
    } else if keys.just_pressed(KeyCode::KeyA) {
        Some((true, Consent::Always))
    } else if keys.just_pressed(KeyCode::KeyN) {
        Some((false, Consent::Ask))
    } else if keys.just_pressed(KeyCode::KeyX) {
        Some((false, Consent::Never))
    } else {
        None
    };
    let Some((sending, kept)) = choice else { return };
    keep(kept);
    let report = pending.0.take();
    if sending {
        send(report, *tier);
    }
    for entity in &asking {
        commands.entity(entity).despawn();
    }
}

/// Sends a report on a thread of its own: the game goes on.
fn send(report: Option<Report>, tier: osage_render::Tier) {
    let Some(report) = report else { return };
    let version = format!("{} ({})", env!("CARGO_PKG_VERSION"), release());
    let _ = std::thread::Builder::new().name("osage-report".into()).spawn(move || {
        match report.send(RELAY, "osage-kart", &version, &tier.to_string()) {
            Ok(issue) => info!("report: filed as {issue}"),
            Err(error) => warn!("report: not sent: {error}"),
        }
    });
}

/// The release the folder was updated to (`release.txt`, written by `kart-update`), if any.
fn release() -> String {
    let file = std::env::current_exe().ok().and_then(|exe| exe.parent().map(|d| d.join("release.txt")));
    let text = file.and_then(|f| std::fs::read_to_string(f).ok()).unwrap_or_default();
    text.lines()
        .filter_map(|line| line.split_once('='))
        .find(|(key, _)| matches!(key.trim(), "tag" | "release"))
        .map_or_else(|| "built here".to_owned(), |(_, value)| value.trim().to_owned())
}

/// Up: the race is shown (a frame or a hundred after it is known).
fn started(view: Res<RaceView>, mut frames: Local<u32>) {
    if view.stage != Lights::Off {
        *frames += 1;
        if *frames == 100
            && let Some(watch) = WATCH.get()
        {
            watch.started();
        }
    }
}
