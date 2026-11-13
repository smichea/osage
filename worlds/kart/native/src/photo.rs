// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The photo mode (design/04 G4.7, D-066). O (a pad's right stick
//! pressed) freezes the picture — every kart where it was drawn — hides
//! the interface after a few seconds of help, and frees the camera: W, A,
//! S, D move it, E and Q raise and lower it, the arrow keys turn it (Shift
//! for more), R and F zoom in and out, Z and X bring the focus nearer and
//! further, 1 and 2 open and close the lens — a full-frame camera's, so a
//! long lens wide open blurs what is not in focus; F12 saves the picture.
//! O again goes back to the game.
//!
//! The world goes on meanwhile: the picture is frozen here only, and the
//! player's kart is left to coast — a photograph in the middle of a race
//! costs the race, as it would.

use bevy::{post_process::dof::DepthOfField, prelude::*};
use osage_kart_physics::Kart;
use osage_render::OsageCamera;

use crate::{
    scene::{Pose, Stage, Viewpoint},
    settings::Menu,
};

/// Seconds the help is shown before the interface goes.
const HELP: f32 = 4.0;
/// The camera's speed, m/s, and how fast it turns, rad/s.
const MOVE: f32 = 6.0;
const TURN: f32 = 0.9;
/// Lens openings, f-stops, from the widest.
const STOPS: [f32; 7] = [1.4, 2.0, 2.8, 4.0, 5.6, 8.0, 16.0];
/// A full-frame sensor's height, m.
const SENSOR: f32 = 0.024;
/// The field of view, degrees: from a long lens to a wide one.
const FOV: (f32, f32) = (8.0, 75.0);

pub struct PhotoPlugin;

impl Plugin for PhotoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Photo>().add_systems(Update, photograph.in_set(Stage::Simulate).after(crate::show::Shown));
    }
}

/// The photo mode, when it is on.
#[derive(Resource, Default)]
pub struct Photo {
    pub on: Option<Shoot>,
}

pub struct Shoot {
    /// Every kart as it was when the picture was frozen.
    frozen: Vec<(Entity, Kart)>,
    eye: Vec3,
    yaw: f32,
    pitch: f32,
    focus: f32,
    stop: usize,
    /// Vertical field of view, degrees.
    fov: f32,
    /// Seconds since the mode began: the help shows for a while.
    since: f32,
}

impl Photo {
    pub fn is_on(&self) -> bool {
        self.on.is_some()
    }
}

/// The focal length, mm, of a full-frame lens with this field of view.
fn focal_length(fov: f32) -> f32 {
    SENSOR * 1000.0 / (2.0 * (fov.to_radians() / 2.0).tan())
}

/// The help line while it is shown; empty after.
fn help(shoot: &Shoot) -> String {
    if shoot.since > HELP {
        return String::new();
    }
    format!(
        "PHOTO  WASD move  E/Q up/down  arrows turn  R/F zoom {:.0} mm  Z/X focus {:.1} m  1/2 lens f/{}  F12 picture  O back",
        focal_length(shoot.fov),
        shoot.focus,
        STOPS[shoot.stop]
    )
}

#[allow(clippy::too_many_arguments)]
fn photograph(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    menu: Res<Menu>,
    mut photo: ResMut<Photo>,
    mut view: ResMut<crate::hud::RaceView>,
    mut karts: Query<(Entity, &mut Pose)>,
    mut camera: Query<(Entity, &Transform, &mut Projection), crate::scene::FirstCamera>,
    mut roots: Query<&mut Visibility, (With<Node>, Without<ChildOf>)>,
) {
    let toggled = !menu.0
        && (keys.just_pressed(KeyCode::KeyO) || gamepads.iter().any(|pad| pad.just_pressed(GamepadButton::RightThumb)));
    let Ok((lens, place, mut projection)) = camera.single_mut() else { return };
    if toggled {
        if photo.on.take().is_some() {
            commands.entity(lens).remove::<DepthOfField>();
            commands.remove_resource::<Viewpoint>();
            for mut shown in &mut roots {
                shown.set_if_neq(Visibility::Inherited);
            }
            view.photo.clear();
            return;
        }
        let (yaw, pitch, _) = place.rotation.to_euler(EulerRot::YXZ);
        photo.on = Some(Shoot {
            frozen: karts.iter().map(|(entity, pose)| (entity, pose.kart)).collect(),
            eye: place.translation,
            yaw,
            pitch,
            focus: 8.0,
            stop: 2,
            fov: 40.0,
            since: 0.0,
        });
    }
    let Some(shoot) = &mut photo.on else { return };
    let seconds = time.delta_secs();
    shoot.since += seconds;
    for (entity, mut pose) in &mut karts {
        if let Some((_, kart)) = shoot.frozen.iter().find(|(e, _)| *e == entity) {
            pose.kart = *kart;
        }
    }
    let held = |code: KeyCode| if keys.pressed(code) { 1.0 } else { 0.0 };
    let fast = if keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) { 4.0 } else { 1.0 };
    shoot.yaw += (held(KeyCode::ArrowLeft) - held(KeyCode::ArrowRight)) * TURN * fast * seconds;
    shoot.pitch =
        (shoot.pitch + (held(KeyCode::ArrowUp) - held(KeyCode::ArrowDown)) * TURN * fast * seconds).clamp(-1.45, 1.45);
    let facing = Quat::from_euler(EulerRot::YXZ, shoot.yaw, shoot.pitch, 0.0);
    let ahead = facing * Vec3::NEG_Z;
    let flat = Vec3::new(ahead.x, 0.0, ahead.z).normalize_or_zero();
    let right = Vec3::new(-flat.z, 0.0, flat.x);
    let moving = flat * (held(KeyCode::KeyW) - held(KeyCode::KeyS))
        + right * (held(KeyCode::KeyD) - held(KeyCode::KeyA))
        + Vec3::Y * (held(KeyCode::KeyE) - held(KeyCode::KeyQ));
    shoot.eye += moving * MOVE * fast * seconds;
    shoot.eye.y = shoot.eye.y.max(0.25);
    shoot.focus = (shoot.focus * (1.0 + (held(KeyCode::KeyX) - held(KeyCode::KeyZ)) * seconds)).clamp(0.5, 500.0);
    shoot.fov = (shoot.fov * (1.0 + (held(KeyCode::KeyF) - held(KeyCode::KeyR)) * seconds)).clamp(FOV.0, FOV.1);
    if let Projection::Perspective(lens) = &mut *projection {
        lens.fov = shoot.fov.to_radians();
    }
    if keys.just_pressed(KeyCode::Digit1) {
        shoot.stop = shoot.stop.saturating_sub(1);
    }
    if keys.just_pressed(KeyCode::Digit2) {
        shoot.stop = (shoot.stop + 1).min(STOPS.len() - 1);
    }
    commands.insert_resource(Viewpoint { from: shoot.eye, to: shoot.eye + ahead });
    commands.entity(lens).insert(DepthOfField {
        focal_distance: shoot.focus,
        aperture_f_stops: STOPS[shoot.stop],
        sensor_height: SENSOR,
        max_depth: 600.0,
        ..default()
    });
    view.photo = help(shoot);
    let hidden = view.photo.is_empty();
    for mut shown in &mut roots {
        shown.set_if_neq(if hidden { Visibility::Hidden } else { Visibility::Inherited });
    }
}

/// The camera before it has a lens.
type Unfocused = (With<OsageCamera>, Without<DepthOfField>);

/// A system that keeps the camera's lens focused `metres` away, wide open:
/// `--focus`, for pictures of the photo mode's look from the bench.
pub fn focus(metres: f32) -> impl FnMut(Commands, Query<Entity, Unfocused>) {
    move |mut commands, lenses| {
        for lens in &lenses {
            commands.entity(lens).insert(DepthOfField {
                focal_distance: metres,
                aperture_f_stops: STOPS[0],
                sensor_height: SENSOR,
                max_depth: 600.0,
                ..default()
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_help_says_the_lens_and_then_goes() {
        let shoot = |since| Shoot {
            frozen: Vec::new(),
            eye: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            focus: 12.0,
            stop: 2,
            fov: 26.99,
            since,
        };
        let shown = help(&shoot(1.0));
        assert!(shown.contains("focus 12.0 m") && shown.contains("f/2.8") && shown.contains("zoom 50 mm"), "{shown}");
        assert!(help(&shoot(HELP + 0.1)).is_empty());
    }
}
