// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The interface on the bench (`--ui-preview`, D-078): the race's HUD
//! drawn over the bench's race in four states set by hand — racing,
//! the lights counting down, results between races with records, a cup and
//! the driver's pick, and a battle — and the whole picture, interface
//! included, shot at the bench's four frames. Without a window, the
//! interface's camera draws into a picture of the output's size, behind
//! which the scene's picture is laid; that picture is what is shot. Over
//! the countdown, the crash-report question; over the battle, the settings.

use bevy::{camera::RenderTarget, prelude::*, render::render_resource::TextureFormat};
use osage_kart_items::Item;
use osage_render::resolution::Output;

use crate::{
    hud::{Best, Entry, HudPlugin, Lights, RaceView, Records},
    settings::{Menu, Panel},
    theme::{Arrows, Faces},
};

/// The picture the interface is drawn into, with the scene behind it.
#[derive(Resource)]
pub struct Composite(pub Handle<Image>);

pub struct PreviewPlugin {
    /// Frames of the bench: a quarter each state.
    pub frames: u32,
}

#[derive(Resource)]
struct Frames(u32);

impl Plugin for PreviewPlugin {
    fn build(&self, app: &mut App) {
        // The frame rate off: the bench's pace is its own.
        app.add_plugins((HudPlugin, crate::settings::SettingsPlugin { fps: Some(None) }))
            .init_resource::<crate::play::CallWanted>()
            .insert_resource(Frames(self.frames))
            .add_systems(PostStartup, aim)
            .add_systems(Update, (stage.in_set(crate::scene::Stage::Simulate), menus));
    }
}

/// The interface's camera draws into a picture of the output's size.
fn aim(
    mut commands: Commands,
    output: Res<Output>,
    mut images: ResMut<Assets<Image>>,
    cameras: Query<Entity, With<IsDefaultUiCamera>>,
) {
    let size = output.0.unwrap_or(UVec2::new(1280, 720));
    let image = images.add(Image::new_target_texture(
        size.x,
        size.y,
        TextureFormat::Rgba8Unorm,
        Some(TextureFormat::Rgba8UnormSrgb),
    ));
    for camera in &cameras {
        commands.entity(camera).insert(RenderTarget::Image(image.clone().into()));
    }
    commands.insert_resource(Composite(image));
}

fn entry(id: u64, name: &str, color: u32, position: u32, lap: u32, at: Vec2) -> Entry {
    Entry {
        id,
        name: name.into(),
        bot: id != 2,
        ghost: false,
        lives: 0,
        out: false,
        color,
        position,
        lap,
        finish_ms: 0,
        at,
    }
}

fn best(name: &str, ms: u32, fresh: bool, mine: bool) -> Best {
    Best { name: name.into(), ms, fresh, mine }
}

/// The quarter of the bench run.
fn quarter(time: &Time, frames: &Frames) -> u32 {
    let (seconds, total) = (time.elapsed_secs(), frames.0 as f32 / 60.0);
    ((seconds / total.max(1.0)) * 4.0).floor() as u32
}

/// The report's question over the countdown; the settings over the battle.
#[allow(clippy::too_many_arguments)]
fn menus(
    mut commands: Commands,
    time: Res<Time>,
    frames: Res<Frames>,
    faces: Res<Faces>,
    arrows: Res<Arrows>,
    mut asking: Local<Option<Entity>>,
    mut menu: ResMut<Menu>,
    mut panels: Query<&mut Node, With<Panel>>,
) {
    let quarter = quarter(&time, &frames);
    match (quarter == 1, *asking) {
        (true, None) => {
            let what = "Last time, the game did not finish starting.".to_owned();
            *asking = Some(crate::report::prompt(&mut commands, &faces, &arrows, what));
        }
        (false, Some(entity)) => {
            commands.entity(entity).despawn();
            *asking = None;
        }
        _ => {}
    }
    let open = quarter >= 3;
    if menu.0 != open {
        menu.0 = open;
        for mut node in &mut panels {
            node.display = if open { Display::Flex } else { Display::None };
        }
    }
}

/// Which state the interface shows, by the share of the bench run.
fn stage(time: Res<Time>, frames: Res<Frames>, course: Option<Res<crate::scene::Course>>, mut view: ResMut<RaceView>) {
    let quarter = quarter(&time, &frames);
    let at = |along: f32| {
        course.as_ref().map_or(Vec2::ZERO, |c| {
            let ([x, z], _) = c.0.track.at(along);
            Vec2::new(x, z)
        })
    };
    let field = vec![
        entry(1, "Comet", 3, 1, 2, at(320.0)),
        entry(2, "agent-1.alice", 1, 2, 2, at(300.0)),
        entry(3, "Pixel", 1, 3, 2, at(280.0)),
        entry(4, "Bolt", 2, 4, 2, at(250.0)),
        entry(5, "Nova", 0, 5, 1, at(120.0)),
        entry(6, "Echo", 5, 6, 1, at(90.0)),
    ];
    let mut wanted = RaceView {
        status: "world osage-kart | you are agent-1.alice | via WebTransport".into(),
        laps: 3,
        karts: field,
        me: Some(1),
        ..default()
    };
    match quarter {
        0 => {
            wanted.stage = Lights::Racing;
            wanted.clock_ms = 72_430;
            wanted.speed = 21.3;
            wanted.charge = 0.9;
            wanted.boosting = true;
            wanted.held = Some(Item::Stack);
            wanted.count = 2;
        }
        1 => {
            wanted.stage = Lights::Count(2);
            wanted.clock_ms = 0;
            for kart in &mut wanted.karts {
                kart.lap = 0;
            }
        }
        2 => {
            wanted.stage = Lights::Results(6);
            for (i, kart) in wanted.karts.iter_mut().enumerate() {
                kart.finish_ms = 96_000 + i as u64 * 1_730;
                kart.lap = 4;
            }
            wanted.records = Records {
                track: "paddock".into(),
                race_laps: 3,
                laps: vec![best("agent-1.alice", 30_120, true, true), best("marco", 30_480, false, false)],
                races: vec![best("adrien", 94_200, false, false), best("agent-1.alice", 96_000, true, true)],
                trials: Vec::new(),
            };
            wanted.cup = "GRAND TOUR - race 1 of 4: The Paddock\n> 1 agent-1.alice     10 pts  1\n  2 Comet              8 pts  2".into();
            wanted.event = "Next: Grand Tour (cup)".into();
            wanted.pick = "Driver: Juno (medium) – even-tempered, any kart suits\nKart: Sprint – leaps out of bends\nPaint: Gold\n38 pts · 2 cups · 1 won – Midnight at 90 pts".into();
        }
        _ => {
            wanted.stage = Lights::Racing;
            wanted.battle = true;
            wanted.clock_ms = 84_000;
            wanted.speed = 17.0;
            for (i, kart) in wanted.karts.iter_mut().enumerate() {
                kart.lives = [3, 2, 2, 1, 0, 1][i];
                kart.out = kart.lives == 0;
            }
            wanted.held = Some(Item::Drone);
        }
    }
    *view = wanted;
}
