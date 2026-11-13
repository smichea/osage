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
use osage_kart_physics::{Ground, Kart, MAX_SPEED};
use osage_render::{Look, OsageCamera, Sun, panorama::Panorama};
use osage_world_kart::Circuit;

use crate::{effects, karts, marks, scenery, track};

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

/// What a kart can be painted (D-064): the team colours, then the special
/// paints players win in cups — silver, pearl, midnight, gold.
pub const PAINTS: [(f32, f32, f32); 12] = [
    PALETTE[0],
    PALETTE[1],
    PALETTE[2],
    PALETTE[3],
    PALETTE[4],
    PALETTE[5],
    PALETTE[6],
    PALETTE[7],
    (0.76, 0.77, 0.79),
    (0.93, 0.92, 0.88),
    (0.09, 0.11, 0.24),
    (0.86, 0.66, 0.2),
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

/// The sky shown, and whether it follows the track's (D-056): in a game,
/// unless `--sky` chose one; a bench keeps its own.
#[derive(Resource, Clone, Copy)]
pub struct SkyShown {
    pub sky: Sky,
    pub follows: bool,
}

/// A track raced under another sky: its photograph is decoded on a thread
/// of its own (seconds, for a 4k panorama), then it and its exposure
/// replace the shown ones and the rendering works the light out again.
fn follow_the_sky(mut commands: Commands, course: Res<Course>, mut shown: ResMut<SkyShown>) {
    let Ok(wanted) = course.0.about.sky.parse::<Sky>() else { return };
    if !shown.follows || wanted == shown.sky {
        return;
    }
    shown.sky = wanted;
    let (tx, rx) = crossbeam_channel::bounded(1);
    let spawned = std::thread::Builder::new().name("osage-sky".to_owned()).spawn(move || {
        let _ = tx.send(panorama(wanted));
    });
    if spawned.is_ok() {
        commands.insert_resource(SkyLoading { sky: wanted, photo: rx });
    }
}

/// A sky's photograph being decoded.
#[derive(Resource)]
pub struct SkyLoading {
    sky: Sky,
    photo: crossbeam_channel::Receiver<Option<Panorama>>,
}

/// The photograph decoded, the sky changes; if the track changed again
/// meanwhile, only the latest is shown.
fn sky_loaded(mut commands: Commands, loading: Res<SkyLoading>, shown: Res<SkyShown>) {
    let Ok(photo) = loading.photo.try_recv() else { return };
    commands.remove_resource::<SkyLoading>();
    if loading.sky != shown.sky {
        return;
    }
    commands.insert_resource(look(loading.sky));
    if let Some(photo) = photo {
        commands.insert_resource(photo);
    }
}

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
    /// What the kart holds, shown on it for all to see.
    pub held: Option<osage_kart_items::Item>,
    /// The colour of the smoke it pours (0xRRGGBB), while it pours some.
    pub smoke: Option<u32>,
    /// It holds its item behind it, on the road (D-052).
    pub trailing: bool,
    /// Seconds of a pit-crew push left, when nothing touches it.
    pub guard: f32,
    /// A ghost (D-062): a run from an earlier time trial, driven again;
    /// drawn seen through, and silent.
    pub ghost: bool,
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
        let circuit = (follow_the_sky.run_if(resource_exists::<SkyShown>), track::build, scenery::build, marks::build)
            .chain()
            .run_if(resource_exists_and_changed::<Course>);
        app.configure_sets(Update, (Stage::Simulate, Stage::Draw).chain())
            .init_resource::<View>()
            .init_resource::<Glance>()
            .init_resource::<marks::Marks>()
            .add_plugins(crate::rain::RainPlugin)
            .add_systems(Startup, (setup, karts::setup, effects::setup, scenery::load_roadside))
            .add_systems(
                Update,
                (
                    circuit,
                    karts::dress,
                    karts::park,
                    karts::animate,
                    effects::emit,
                    effects::gloom,
                    marks::lay,
                    effects::update,
                    effects::prewarm,
                    scenery::bob,
                    scenery::turn,
                    track::flow,
                    choose_view,
                    follow,
                    sky_loaded.run_if(resource_exists::<SkyLoading>),
                )
                    .chain()
                    .in_set(Stage::Draw),
            );
    }
}

/// Where the camera is: behind the kart (the default), at the driver's
/// eyes, or high above and behind.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Chase,
    Cockpit,
    Bird,
}

/// Where the player looks, as a turn from ahead (radians, positive to the
/// left): held keys or the right stick.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct Glance(pub f32);

/// C (or a gamepad's Select) cycles the views; Q and E, held, look left
/// and right, X behind; a gamepad's right stick looks round.
fn choose_view(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut view: ResMut<View>,
    mut glance: ResMut<Glance>,
) {
    let pad = gamepads.iter().next();
    if keys.just_pressed(KeyCode::KeyC) || pad.is_some_and(|pad| pad.just_pressed(GamepadButton::Select)) {
        *view = match *view {
            View::Chase => View::Cockpit,
            View::Cockpit => View::Bird,
            View::Bird => View::Chase,
        };
    }
    let quarter = std::f32::consts::FRAC_PI_2;
    glance.0 = if keys.pressed(KeyCode::KeyX) {
        std::f32::consts::PI
    } else if keys.pressed(KeyCode::KeyQ) {
        quarter
    } else if keys.pressed(KeyCode::KeyE) {
        -quarter
    } else {
        0.0
    };
    if let Some(pad) = pad {
        let (x, y) =
            (pad.get(GamepadAxis::RightStickX).unwrap_or(0.0), pad.get(GamepadAxis::RightStickY).unwrap_or(0.0));
        if y < -0.6 {
            glance.0 = std::f32::consts::PI;
        } else if x.abs() > 0.2 {
            glance.0 = -x * quarter;
        }
    }
}

/// The chasing camera's state: where it looks, eased like its position;
/// the kart's heading and speed a frame ago, to feel it turn and slow; how
/// far the camera banks; what is left of a jolt; the time, for its shake.
#[derive(Component)]
struct Chase {
    /// Where the camera is, eased, before it shakes.
    place: Vec3,
    look: Vec3,
    heading: f32,
    speed: f32,
    roll: f32,
    jolt: f32,
    time: f32,
    /// The glance, eased: the head turns, it does not jump.
    glance: f32,
}

/// What the wheels run on at `x`, `z`, as how much it shakes a kart: kerbs
/// shake (0.6), grass bumps (1), asphalt hums (0.1).
pub fn roughness(course: Option<&Course>, x: f32, z: f32) -> f32 {
    course.map_or(0.1, |course| {
        let track = &course.0.track;
        match track.ground(track.locate(x, z).lateral) {
            Ground::Road => 0.1,
            Ground::Kerb => 0.6,
            Ground::Grass => 1.0,
        }
    })
}

/// A smooth wobble in -1..1: three sines whose periods never line up.
pub fn wobble(t: f32, seed: f32) -> f32 {
    ((t * 1.0 + seed).sin() * 0.5 + (t * 2.31 + seed * 1.7).sin() * 0.3 + (t * 5.17 + seed * 0.3).sin() * 0.2)
        .clamp(-1.0, 1.0)
}

fn setup(mut commands: Commands) {
    if osage_content::root().is_some() {
        commands.insert_resource(Photographed);
    }
    commands.spawn((
        Camera3d::default(),
        OsageCamera,
        Projection::Perspective(PerspectiveProjection { fov: 62f32.to_radians(), far: 6000.0, ..default() }),
        Transform::from_xyz(0.0, 120.0, -160.0).looking_at(Vec3::new(-60.0, 0.0, 0.0), Vec3::Y),
        Chase {
            place: Vec3::new(0.0, 120.0, -160.0),
            look: Vec3::new(-60.0, 0.0, 0.0),
            heading: 0.0,
            speed: 0.0,
            roll: 0.0,
            jolt: 0.0,
            time: 0.0,
            glance: 0.0,
        },
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
#[allow(clippy::too_many_arguments)]
fn follow(
    time: Res<Time>,
    course: Option<Res<Course>>,
    view: Option<Res<View>>,
    glance: Option<Res<Glance>>,
    viewpoint: Option<Res<Viewpoint>>,
    followed: Query<&Pose, With<Followed>>,
    mut camera: Query<(&mut Transform, &mut Chase, &mut Projection)>,
) {
    if let (Some(viewpoint), Ok((mut camera, mut chase, _))) = (viewpoint, camera.single_mut()) {
        *camera = Transform::from_translation(viewpoint.from).looking_at(viewpoint.to, Vec3::Y);
        // When the viewpoint goes, the chase camera eases from here.
        (chase.place, chase.look, chase.roll) = (viewpoint.from, viewpoint.to, 0.0);
        return;
    }
    let (Some(pose), Ok((mut camera, mut chase, mut projection))) = (followed.iter().next(), camera.single_mut())
    else {
        return;
    };
    let (kart, seconds) = (&pose.kart, time.delta_secs());
    let forward = Vec3::new(kart.heading.sin(), 0.0, kart.heading.cos());
    // The kart's left: it faces +Z with its left at +X.
    let left = Vec3::new(kart.heading.cos(), 0.0, -kart.heading.sin());
    // Over a crest and in the air, the camera rises and falls with the kart.
    let at = Vec3::new(kart.x, kart.height, kart.z);
    let step = seconds.max(1e-4);
    chase.time += step;
    // How fast it turns (positive to the left) and how hard it slows: a
    // stop harder than brakes give is a knock.
    let turning = (osage_kart_physics::wrap(kart.heading - chase.heading) / step).clamp(-3.0, 3.0);
    let slowing = (chase.speed - kart.speed) / step;
    (chase.heading, chase.speed) = (kart.heading, kart.speed);
    if slowing > 30.0 {
        chase.jolt = (chase.jolt + slowing / 80.0).min(1.0);
    }
    chase.jolt *= (-step * 5.0).exp();
    let surface = roughness(course.as_deref(), kart.x, kart.z);
    let pace = (kart.speed.abs() / MAX_SPEED).clamp(0.0, 1.3);
    let view = view.map_or(View::Chase, |view| *view);
    let wanted_glance = glance.map_or(0.0, |glance| glance.0);
    chase.glance += (wanted_glance - chase.glance) * (1.0 - (-seconds * 9.0).exp());
    // Where the player looks: ahead, turned by the glance.
    let (sin, cos) = chase.glance.sin_cos();
    let looking = forward * cos + left * sin;
    let amplitude =
        (0.003 + 0.016 * surface * pace + 0.06 * chase.jolt) * if view == View::Cockpit { 0.5 } else { 1.0 };
    let shake = Vec3::new(wobble(chase.time * 23.0, 1.0), wobble(chase.time * 31.0, 4.0), 0.0) * amplitude;
    let (place, look, ease) = match view {
        // A camera crew's car would follow a real kart this close, a
        // little late, and looks into the bend the kart turns into.
        View::Chase => (
            at - looking * 3.4 + Vec3::Y * 1.35,
            at + looking * 2.6 + Vec3::Y * 0.55 + left * turning * 0.9 * cos.max(0.0),
            7.0,
        ),
        // The driver's eyes, inside the helmet (whose inside is not drawn),
        // fixed to the kart.
        View::Cockpit => {
            let eyes = at + Vec3::Y * 0.87 - forward * 0.17;
            (eyes, eyes + looking * 10.0 - Vec3::Y * 0.9 + left * turning * 1.5 * cos.max(0.0), 40.0)
        }
        // High above and behind, the race laid out ahead.
        View::Bird => (at - looking * 11.0 + Vec3::Y * 8.5, at + looking * 4.0, 5.0),
    };
    chase.place = if view == View::Cockpit {
        // Fixed to the kart, without lag: a camera that eased after the
        // eyes fell behind the helmet at speed (v / ease metres).
        place
    } else {
        chase.place.lerp(place, 1.0 - (-seconds * ease).exp())
    };
    camera.translation = chase.place;
    chase.look = chase.look.lerp(look, 1.0 - (-seconds * ease.max(10.0)).exp());
    camera.look_at(chase.look, Vec3::Y);
    let offset = camera.rotation * shake;
    camera.translation += offset;
    // It banks a little into the bend, as a following car leans.
    let bank = (turning * 0.03 + kart.slide * 0.004).clamp(-0.045, 0.045);
    chase.roll += (bank - chase.roll) * (1.0 - (-seconds * 4.0).exp());
    camera.rotate_local_z(chase.roll);
    if let Projection::Perspective(lens) = &mut *projection {
        let pace = (kart.speed / MAX_SPEED).clamp(0.0, 1.4);
        let base = match view {
            View::Chase => 56.0,
            View::Cockpit => 72.0,
            View::Bird => 50.0,
        };
        let wanted = (base + 8.0 * pace + if kart.boost > 0.0 { 4.0 } else { 0.0 }).to_radians();
        lens.fov += (wanted - lens.fov) * (seconds * 4.0).min(1.0);
    }
}
