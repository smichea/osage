// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the kart world looks like, natively. Whoever computes the karts (the
//! network replica when playing, a local simulation in the bench) writes each
//! kart's [`Pose`] in [`Stage::Simulate`]; the modules here draw the rest in
//! [`Stage::Draw`]: the circuit ([`crate::track`]), what stands around it
//! ([`crate::scenery`]), the karts ([`crate::karts`]), their sparks and
//! smoke ([`crate::effects`]), and the camera that chases one of them.
//!
//! All of it is procedural, plus the surfaces of the asset manifest when
//! they have been fetched; the rendering programme (design/03, R6 to R9)
//! replaces it piece by piece with richer content.

use bevy::{core_pipeline::tonemapping::Tonemapping, prelude::*};
use osage_kart_physics::{Kart, MAX_SPEED};
use osage_render::{Look, OsageCamera, Sun};
use osage_world_kart::Circuit;

use crate::{effects, karts, scenery, track};

/// Kart colours, by `kart.racer/1` colour index: the browser client's palette.
pub const PALETTE: [(f32, f32, f32); 8] = [
    (0.898, 0.325, 0.294),
    (0.325, 0.608, 0.961),
    (0.341, 0.671, 0.353),
    (0.776, 0.565, 0.149),
    (0.690, 0.514, 0.941),
    (0.224, 0.773, 0.812),
    (0.878, 0.510, 0.239),
    (0.969, 0.471, 0.729),
];

/// The kart world's look: vivid and sunny, like a toy circuit.
pub fn look() -> Look {
    Look { tonemapping: Tonemapping::TonyMcMapface, saturation: 1.2, exposure: 0.9 }
}

pub struct ScenePlugin;

/// The two halves of a frame: something computes the karts, then they are drawn.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum Stage {
    Simulate,
    Draw,
}

/// The circuit to draw.
#[derive(Resource)]
pub struct Course(pub Circuit);

/// A kart to draw, as it is now.
#[derive(Component)]
pub struct Pose {
    pub kart: Kart,
    pub color: u32,
}

/// The kart the camera follows.
#[derive(Component)]
pub struct Followed;

/// A piece of the circuit or of its scenery: rebuilt when the circuit changes.
#[derive(Component)]
pub struct Part;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        let circuit = (track::build, scenery::build).chain().run_if(resource_exists_and_changed::<Course>);
        app.configure_sets(Update, (Stage::Simulate, Stage::Draw).chain())
            .add_systems(Startup, (setup, karts::setup, effects::setup))
            .add_systems(
                Update,
                (circuit, karts::dress, karts::animate, effects::emit, effects::update, scenery::bob, follow)
                    .chain()
                    .in_set(Stage::Draw),
            );
    }
}

/// Where the camera looks, eased like its position.
#[derive(Component)]
struct Chase(Vec3);

fn setup(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        OsageCamera,
        Projection::Perspective(PerspectiveProjection { fov: 62f32.to_radians(), far: 6000.0, ..default() }),
        Transform::from_xyz(0.0, 120.0, -160.0).looking_at(Vec3::new(-60.0, 0.0, 0.0), Vec3::Y),
        Chase(Vec3::new(-60.0, 0.0, 0.0)),
    ));
    // A sun of mid-morning, behind the grid's right shoulder.
    commands.spawn((
        DirectionalLight::default(),
        Sun,
        Transform::from_xyz(55.0, 60.0, 35.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

/// Chase camera: behind the followed kart, a little above; it looks ahead of
/// the kart and widens its view with speed, more when the kart boosts.
fn follow(
    time: Res<Time>,
    followed: Query<&Pose, With<Followed>>,
    mut camera: Query<(&mut Transform, &mut Chase, &mut Projection)>,
) {
    let (Some(pose), Ok((mut camera, mut chase, mut projection))) = (followed.iter().next(), camera.single_mut())
    else {
        return;
    };
    let (kart, seconds) = (&pose.kart, time.delta_secs());
    let forward = Vec3::new(kart.heading.sin(), 0.0, kart.heading.cos());
    let at = Vec3::new(kart.x, 0.0, kart.z);
    let behind = at - forward * 5.6 + Vec3::Y * 2.55;
    camera.translation = camera.translation.lerp(behind, 1.0 - (-seconds * 7.0).exp());
    chase.0 = chase.0.lerp(at + forward * 3.5 + Vec3::Y * 1.1, 1.0 - (-seconds * 10.0).exp());
    camera.look_at(chase.0, Vec3::Y);
    if let Projection::Perspective(lens) = &mut *projection {
        let pace = (kart.speed / MAX_SPEED).clamp(0.0, 1.4);
        let wanted = (62.0 + 9.0 * pace + if kart.boost > 0.0 { 5.0 } else { 0.0 }).to_radians();
        lens.fov += (wanted - lens.fov) * (seconds * 4.0).min(1.0);
    }
}
