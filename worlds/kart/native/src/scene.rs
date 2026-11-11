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
use osage_render::{Look, OsageCamera, Sun, panorama::Panorama};
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

/// The colour of the meadows, which the light from below takes: the albedo
/// of mown grass, about a fifth of the light.
pub const GRASS: Color = Color::srgb(0.30, 0.40, 0.17);

/// A time of day: a Poly Haven panorama of the sky, with an unclipped sun,
/// and how far it is turned, radians. At noon the sun comes from the side
/// of the start straight, ahead and to the right, and shapes the karts with
/// light and shade (behind the camera, it flattened them; straight ahead,
/// it washed out the sky). In the evening, 7° up, it comes from behind:
/// ahead, the ground would only be lit by the sky.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sky {
    /// Midday, a few clouds: the bench's.
    Noon,
    /// Late afternoon, the sun low and warm, scattered clouds.
    Evening,
}

impl Sky {
    /// The exposure offset, stops: a camera opens up as the sun sinks.
    /// Midday is a third of a stop over the sunny-day exposure (ACES's
    /// deeper shadows); with the sun at 7°, the ground gets a sixth of the
    /// sun it gets at noon.
    fn exposure(self) -> f32 {
        match self {
            Sky::Noon => 0.33,
            Sky::Evening => 2.0,
        }
    }

    fn panorama(self) -> (&'static str, f32) {
        match self {
            Sky::Noon => ("kloofendal_48d_partly_cloudy_puresky", 5.6),
            // Its sun behind the start straight's camera: everything in view
            // catches the warm light, shadows stretch ahead.
            Sky::Evening => ("table_mountain_2_puresky", 2.73),
        }
    }
}

impl std::str::FromStr for Sky {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "noon" => Ok(Sky::Noon),
            "evening" => Ok(Sky::Evening),
            other => Err(format!("unknown sky {other}: noon or evening")),
        }
    }
}

/// The kart world's look under `sky`: a sunny day as a camera sees it,
/// through a lens, exposed for the light there is.
pub fn look(sky: Sky) -> Look {
    // ACES has the contrast of a photograph printed for a game, where AgX
    // greyed the asphalt and the sky; a third of a stop brighter makes up for
    // its deeper shadows. A lens darkens the corners a little and splits
    // colours a little at the edges.
    Look {
        tonemapping: Tonemapping::AcesFitted,
        saturation: 1.0,
        exposure: sky.exposure(),
        ground: GROUND,
        vignette: 0.3,
        fringe: 0.004,
    }
}

/// What lies under the karts, on average, for the light that comes back from
/// below: asphalt as much as grass.
const GROUND: Color = Color::srgb(0.3, 0.32, 0.25);

/// The photograph of the sky, when the asset manifest's content is there.
pub fn panorama(sky: Sky) -> Option<Panorama> {
    let (id, turn) = sky.panorama();
    let photo = osage_content::panorama::load(&osage_content::root()?, id)?;
    Some(Panorama { width: photo.width, height: photo.height, pixels: photo.pixels, turn })
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

/// A place the camera is held at, looking at another, instead of following a kart.
#[derive(Resource, Clone, Copy)]
pub struct Viewpoint {
    pub from: Vec3,
    pub to: Vec3,
}

/// The circuit is photographed: the asset manifest's content is there.
#[derive(Resource)]
pub struct Photographed;

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
            .add_systems(Startup, (setup, karts::setup, effects::setup, scenery::load_roadside))
            .add_systems(
                Update,
                (
                    circuit,
                    karts::dress,
                    karts::park,
                    karts::animate,
                    effects::emit,
                    effects::update,
                    effects::prewarm,
                    scenery::bob,
                    scenery::turn,
                    track::flow,
                    follow,
                )
                    .chain()
                    .in_set(Stage::Draw),
            );
    }
}

/// Where the camera looks, eased like its position.
#[derive(Component)]
struct Chase(Vec3);

fn setup(mut commands: Commands) {
    if osage_content::root().is_some() {
        commands.insert_resource(Photographed);
    }
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
    viewpoint: Option<Res<Viewpoint>>,
    followed: Query<&Pose, With<Followed>>,
    mut camera: Query<(&mut Transform, &mut Chase, &mut Projection)>,
) {
    if let (Some(viewpoint), Ok((mut camera, ..))) = (viewpoint, camera.single_mut()) {
        *camera = Transform::from_translation(viewpoint.from).looking_at(viewpoint.to, Vec3::Y);
        return;
    }
    let (Some(pose), Ok((mut camera, mut chase, mut projection))) = (followed.iter().next(), camera.single_mut())
    else {
        return;
    };
    let (kart, seconds) = (&pose.kart, time.delta_secs());
    let forward = Vec3::new(kart.heading.sin(), 0.0, kart.heading.cos());
    let at = Vec3::new(kart.x, 0.0, kart.z);
    // A camera crew's car would follow a real kart this close.
    let behind = at - forward * 3.4 + Vec3::Y * 1.35;
    camera.translation = camera.translation.lerp(behind, 1.0 - (-seconds * 7.0).exp());
    chase.0 = chase.0.lerp(at + forward * 2.6 + Vec3::Y * 0.55, 1.0 - (-seconds * 10.0).exp());
    camera.look_at(chase.0, Vec3::Y);
    if let Projection::Perspective(lens) = &mut *projection {
        let pace = (kart.speed / MAX_SPEED).clamp(0.0, 1.4);
        let wanted = (56.0 + 8.0 * pace + if kart.boost > 0.0 { 4.0 } else { 0.0 }).to_radians();
        lens.fov += (wanted - lens.fov) * (seconds * 4.0).min(1.0);
    }
}
