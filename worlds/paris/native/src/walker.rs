// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The walker, seen from its own eyes: the keys tell its stride, the mouse
//! where it looks, and its steps are taken here at once with the world's
//! own walk grid — the server, taking the same steps, only corrects it
//! when they part.

use std::sync::Arc;

use bevy::{
    input::mouse::AccumulatedMouseMotion,
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use osage_render::OsageCamera;
use osage_world_paris::walk::{EYES, Stride, Walk};

use crate::Options;

/// The street's walk grid.
#[derive(Resource, Clone)]
pub struct Grid(pub Arc<Walk>);

/// The walker as this client sees it.
#[derive(Resource, Debug, Clone)]
pub struct Me {
    pub feet: Vec3,
    /// Radians about y, 0 facing -z.
    pub heading: f32,
    /// Radians, up positive.
    pub pitch: f32,
    pub stride: Stride,
    /// How fast it went over the last frame, m/s.
    pub speed: f32,
}

impl Me {
    pub fn eyes(&self) -> Vec3 {
        self.feet + Vec3::Y * EYES
    }
}

pub struct WalkerPlugin;

impl Plugin for WalkerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(Update, (grab, look, keys, film, step, follow).chain());
    }
}

fn setup(mut commands: Commands, grid: Res<Grid>, options: Res<Options>) {
    let me = match &options.shot {
        Some(shot) => Me {
            feet: shot.eyes - Vec3::Y * EYES,
            heading: shot.heading,
            pitch: 0.0,
            stride: Stride { heading: shot.heading, ..Stride::default() },
            speed: 0.0,
        },
        None => {
            let (feet, heading) = (options.at.map_or(grid.0.start, |eyes| eyes - Vec3::Y * EYES), options.heading);
            Me { feet, heading, pitch: 0.0, stride: Stride { heading, ..Stride::default() }, speed: 0.0 }
        }
    };
    // osage-render draws the scene into its picture, shown in the window or
    // (a shot) taken from it.
    commands.spawn((
        Camera3d::default(),
        OsageCamera,
        Projection::Perspective(PerspectiveProjection {
            fov: options.fov.to_radians(),
            near: 0.05,
            far: 3000.0,
            ..default()
        }),
        Transform::from_translation(me.eyes()),
    ));
    commands.insert_resource(me);
}

/// A click takes the mouse; Escape lets it go.
fn grab(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let Ok(mut cursor) = cursors.single_mut() else { return };
    if mouse.just_pressed(MouseButton::Left) {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
    if keys.just_pressed(KeyCode::Escape) {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
}

fn look(motion: Res<AccumulatedMouseMotion>, cursors: Query<&CursorOptions, With<PrimaryWindow>>, mut me: ResMut<Me>) {
    let grabbed = cursors.single().is_ok_and(|c| c.grab_mode != CursorGrabMode::None);
    if !grabbed {
        return;
    }
    let turn = motion.delta * 0.0025;
    me.heading -= turn.x;
    me.pitch = (me.pitch - turn.y).clamp(-1.4, 1.4);
}

/// The stride the keys ask for: physical keys, so Z Q S D on a French
/// keyboard are where W A S D are.
fn keys(keys: Res<ButtonInput<KeyCode>>, mut me: ResMut<Me>) {
    let held = |a: KeyCode, b: KeyCode| keys.pressed(a) || keys.pressed(b);
    let forward = f32::from(held(KeyCode::KeyW, KeyCode::ArrowUp)) - f32::from(held(KeyCode::KeyS, KeyCode::ArrowDown));
    let right =
        f32::from(held(KeyCode::KeyD, KeyCode::ArrowRight)) - f32::from(held(KeyCode::KeyA, KeyCode::ArrowLeft));
    let run = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let heading = me.heading;
    me.stride = Stride { forward, right, heading, run };
}

/// Filming: straight ahead, once the street is there and the light of the
/// first block has come.
fn film(
    options: Res<Options>,
    placed: Option<Res<crate::street::Placed>>,
    store: Res<crate::light::Store>,
    mut me: ResMut<Me>,
) {
    if options.film.is_none() {
        return;
    }
    let ready = placed.is_some_and(|p| p.0) && store.drawn > 0;
    me.heading = options.heading;
    me.stride = Stride { forward: if ready { 1.0 } else { 0.0 }, right: 0.0, heading: options.heading, run: false };
}

/// The walker's own steps, taken here at once.
fn step(time: Res<Time>, grid: Res<Grid>, options: Res<Options>, mut me: ResMut<Me>) {
    if options.shot.is_some() {
        return;
    }
    let dt = time.delta_secs().min(0.1);
    let from = me.feet;
    let stride = me.stride;
    me.feet = grid.0.step(from, &stride, dt);
    me.speed = if dt > 0.0 { from.distance(me.feet) / dt } else { 0.0 };
}

fn follow(me: Res<Me>, mut cameras: Query<&mut Transform, With<OsageCamera>>) {
    let (eyes, look) = (me.eyes(), Quat::from_euler(EulerRot::YXZ, me.heading, me.pitch, 0.0));
    for mut camera in &mut cameras {
        // Written only when the eyes move: a camera marked as moved starts
        // the reference path tracer's average again every frame.
        if camera.translation != eyes || camera.rotation != look {
            camera.translation = eyes;
            camera.rotation = look;
        }
    }
}
