// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the walker reads on screen — where it is, how much light has come,
//! the street's credit — and the shot: one picture from a place once the
//! light is there.

use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};
use osage_world_paris::tiles::Block;

use osage_render::resolution::Picture;

use crate::{Options, light::Store, street::Placed, walker::Me};

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(Update, (read, shoot, film));
    }
}

#[derive(Component)]
struct Readout;

fn setup(mut commands: Commands, options: Res<Options>) {
    if options.shot.is_some() || options.film.is_some() {
        return;
    }
    let text = |size: f32| TextFont { font_size: bevy::text::FontSize::Px(size), ..default() };
    commands.spawn((
        Text::new(""),
        text(15.0),
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.9)),
        Node { position_type: PositionType::Absolute, top: Val::Px(12.0), left: Val::Px(14.0), ..default() },
        Readout,
    ));
    commands.spawn((
        Text::new(
            "Walk: W A S D (Z Q S D) or the arrows, Shift to run; click to look with the mouse, Escape to let it go.\n\
             Bistro: Amazon Lumberyard, Open Research Content Archive (ORCA), NVIDIA, 2017, CC BY 4.0. \
             Sky: San Giuseppe Bridge, Poly Haven, CC0.",
        ),
        text(12.0),
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.75)),
        Node { position_type: PositionType::Absolute, bottom: Val::Px(10.0), left: Val::Px(14.0), ..default() },
    ));
}

fn read(me: Res<Me>, store: Res<Store>, placed: Option<Res<Placed>>, mut readouts: Query<&mut Text, With<Readout>>) {
    let Ok(mut text) = readouts.single_mut() else { return };
    let block = Block::of(me.eyes());
    let street = if placed.is_some_and(|p| p.0) { "" } else { " — the street is loading" };
    **text = format!(
        "Paris, Bistro{street}\n{:.1}, {:.1}, {:.1} m · {:.1} m/s · block {} {} {}\nlight: {} of 30 tiles of this block · {} tiles came, {:.1} MB",
        me.feet.x,
        me.feet.y,
        me.feet.z,
        me.speed,
        block.cube.x,
        block.cube.y,
        block.cube.z,
        store.drawn,
        store.received,
        store.bytes as f64 / 1e6,
    );
}

/// A shot: once the street is placed and the block's light has come and
/// settled, one picture, then leave.
#[allow(clippy::too_many_arguments)]
fn shoot(
    mut commands: Commands,
    options: Res<Options>,
    picture: Option<Res<Picture>>,
    placed: Option<Res<Placed>>,
    store: Res<Store>,
    mut frames: Local<u32>,
    mut all: Local<u32>,
    mut taken: Local<Option<u32>>,
    mut timed: Local<(f32, u32)>,
    mut settled: Local<f32>,
    time: Res<Time>,
    traced: Res<osage_render::TracedLight>,
    mut exit: MessageWriter<AppExit>,
) {
    let (Some(shot), Some(picture)) = (&options.shot, picture) else { return };
    *frames += 1;
    *all += 1;
    if let Some(at) = *taken {
        // Gone once the picture is on disk: on a fast GPU, 30 frames came
        // before it was read back, and it was lost.
        if (*frames > at + 30 && shot.file.exists()) || *frames > at + 3000 {
            exit.write(AppExit::Success);
        }
        return;
    }
    let street = placed.is_some_and(|p| p.0);
    // The block's light has come and lights it, its probes' tiles all sent
    // (D-141) — or, never forever, a picture of what there is after a while.
    let come = store.drawn > 0 && store.received >= store.drawn && store.lit == store.block && !store.halved;
    let ready = street && (come || *all > 1500);
    // Some frames and seconds after, for the pipelines and the textures
    // to reach the GPU (on a fast one, 60 frames are a quarter of a second)
    // and, traced, for the light to gather frame after frame.
    const SETTLE: u32 = 60;
    let seconds = if traced.0 { 6.0 } else { 2.0 };
    if !ready {
        *frames = 0;
        *settled = 0.0;
        return;
    }
    *settled += time.delta_secs();
    if *frames < SETTLE || *settled < seconds {
        // The last half of the settling frames, timed.
        if *frames > SETTLE / 2 {
            timed.0 += time.delta_secs();
            timed.1 += 1;
        }
        return;
    }
    commands.spawn(Screenshot::image(picture.image.clone())).observe(save_to_disk(shot.file.clone()));
    let ms = 1000.0 * timed.0 / timed.1.max(1) as f32;
    info!(file = %shot.file.display(), tiles = store.drawn, frame_ms = ms, "paris: shot");
    *taken = Some(*frames);
}

/// Filming: every third picture saved while the walker walks; done after
/// the film's seconds of walking.
#[allow(clippy::too_many_arguments)]
fn film(
    mut commands: Commands,
    options: Res<Options>,
    picture: Option<Res<Picture>>,
    me: Res<Me>,
    store: Res<Store>,
    time: Res<Time>,
    mut walked: Local<f32>,
    mut frame: Local<u32>,
    mut times: Local<Vec<f32>>,
    mut exit: MessageWriter<AppExit>,
) {
    let (Some(film), Some(picture)) = (&options.film, picture) else { return };
    if me.stride.forward == 0.0 {
        return;
    }
    *walked += time.delta_secs();
    *frame += 1;
    times.push(1000.0 * time.delta_secs());
    if time.delta_secs() > 0.1 {
        debug!(ms = 1000.0 * time.delta_secs(), "paris: a long frame");
    }
    if *walked > film.seconds {
        // How smooth the walk was: the frames' times in order.
        times.sort_by(f32::total_cmp);
        let at = |q: f32| times[((times.len() - 1) as f32 * q) as usize];
        info!(
            frames = *frame,
            frame_ms = 1000.0 * *walked / *frame as f32,
            median_ms = at(0.5),
            p95_ms = at(0.95),
            max_ms = at(1.0),
            over_50_ms = times.iter().filter(|&&t| t > 50.0).count(),
            tiles = store.received,
            megabytes = store.bytes as f64 / 1e6,
            "paris: filmed"
        );
        exit.write(AppExit::Success);
        return;
    }
    if (*frame).is_multiple_of(3) {
        let _ = std::fs::create_dir_all(&film.folder);
        let file = film.folder.join(format!("{:05}.png", *frame / 3));
        debug!(picture = *frame / 3, feet = ?me.feet, light = ?store.block, "paris: filming");
        commands.spawn(Screenshot::image(picture.image.clone())).observe(save_to_disk(file));
    }
}
