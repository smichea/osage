// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The karts: a racing kart and its driver to scale (`osage-kart-model`),
//! and the motion that makes it feel alive — wheels that spin and steer, a
//! body that leans a little in bends, lifts its nose when it accelerates and
//! hops into a drift, a driver who looks into the turn.
//!
//! Each kart is a body, a helmet and four wheels, each a few meshes, one per
//! finish; colours are in the vertices, so that all karts share one material
//! per finish.
//!
//! A kart faces +Z, stands on the ground at its origin, and has its left
//! at +X.

use std::{
    collections::HashMap,
    f32::consts::{FRAC_PI_2, PI, TAU},
};

use bevy::{
    camera::visibility::NoFrustumCulling,
    mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes},
    prelude::*,
};
use osage_kart_model::{
    AXLES, DRIVER_BONES, Finish, Livery, NECK, WHEELS, driver_cheering, driver_rest, neck, steering_column,
};
pub use osage_kart_model::{EXHAUSTS, REAR_CONTACTS};
use osage_kart_physics::{MAX_SPEED, wrap};

use crate::scene::{Course, PAINTS, Pose, roughness, wobble};

/// A hop into a drift lasts this long, seconds.
const HOP: f32 = 0.28;
/// The fastest a trick rolls the kart, rad/s: two turns a second. A jump
/// too short for a whole turn at that lands the kart tilted (D-096).
const TRICK_SPIN: f32 = 2.0 * TAU;
/// The shortest flight a trick rolls the kart in, s.
const MIN_FLIGHT: f32 = 0.2;

/// The rockets of a rocket item (paddock objects), strapped either side
/// behind the seat, pointing back: where each sits, and its nozzle.
const ROCKETS: [Vec3; 2] = [Vec3::new(0.32, 0.5, -0.62), Vec3::new(-0.32, 0.5, -0.62)];
const ROCKET_LENGTH: f32 = 0.78;
const ROCKET_RADIUS: f32 = 0.085;
pub const ROCKET_NOZZLES: [Vec3; 2] = [
    Vec3::new(0.32, 0.5, -0.62 - ROCKET_LENGTH / 2.0 - 0.05),
    Vec3::new(-0.32, 0.5, -0.62 - ROCKET_LENGTH / 2.0 - 0.05),
];
/// Rockets slide out and back in this long, seconds.
const ROCKET_SHOW: f32 = 0.25;
/// Where the item a kart holds rides, for all to see: on the back of the
/// kart, behind the seat.
const CARRIED: Vec3 = Vec3::new(0.0, 0.62, -0.8);
/// Where a kart's item is held behind it: on the road behind its tail.
const TRAILED: Vec3 = Vec3::new(0.0, 0.05, -1.5);
/// Seconds a bumper's wave takes to spread.
const WAVE: f32 = 0.4;

/// A model made of parts: a mesh, its material, where it sits.
type Parts = Vec<(Handle<Mesh>, Handle<StandardMaterial>, Transform)>;

/// The meshes of a part, each with its finish.
type Meshes = Vec<(Finish, Handle<Mesh>)>;

/// The meshes of one colour of kart.
struct Model {
    body: Meshes,
    /// The driver below the neck, skinned to the bones of [`Bones`].
    driver: Meshes,
    steering: Meshes,
    helmet: Meshes,
    wheels: [Meshes; 2],
}

/// A kart's moving parts that steering moves: the steering wheel and the
/// driver's bones, which the arms follow.
pub struct Bones {
    steering: Entity,
    joints: [Entity; DRIVER_BONES],
}

/// What the karts share: a material per finish, and a model per colour,
/// made the first time a kart of that colour comes.
#[derive(Resource)]
pub struct Kit {
    finishes: HashMap<Finish, Handle<StandardMaterial>>,
    models: HashMap<usize, Model>,
    /// The driver's bones at rest, inverted: every driver's skin.
    bindposes: Handle<SkinnedMeshInverseBindposes>,
    /// A rocket's parts: its tube, its nose, its fins, its nozzle.
    rocket: Parts,
    /// The model of each item a kart can hold, in the order of `Item::ALL`.
    carried: Vec<Parts>,
    /// A pit-crew push's glow around the kart, and a bumper's shock wave.
    bubble: (Handle<Mesh>, Handle<StandardMaterial>),
    wave: (Handle<Mesh>, Handle<StandardMaterial>),
    /// What a ghost is made of (D-062): pale, seen through.
    ghost: Handle<StandardMaterial>,
}

/// The moving parts of a kart and where its motion stands.
#[derive(Component)]
pub struct Rig {
    body: Entity,
    head: Entity,
    bones: Bones,
    /// Each wheel's pivot (the front ones turn to steer) and its tyre (which spins).
    wheels: [(Entity, Entity); 4],
    /// The rockets, shown while the kart has rocket left.
    rockets: Entity,
    /// The model of each item it can hold, one shown at a time.
    carried: Vec<Entity>,
    shown: f32,
    heading: f32,
    speed: f32,
    steer: f32,
    spin: f32,
    slip: f32,
    lean: f32,
    pitch: f32,
    /// Seconds left of the hop into a drift.
    hop: f32,
    drifting: bool,
    /// The driver's trunk, bent about the waist: thrown out of a bend (roll,
    /// about Z) and forward under braking (pitch, about X); and how fast it
    /// moves. A mass on a spring: it lags, swings a little past, settles.
    torso: Vec2,
    sway: Vec2,
    /// How far the driver's arm is raised, cheering a place on the podium
    /// (D-087): eased from 0 to 1.
    cheer: f32,
    /// Seconds driven, and this kart's own offset, for its rumble.
    time: f32,
    seed: f32,
    /// The nose up a crest and through the air, radians (down is positive).
    attitude: f32,
    /// The body's roll across a slope it stands on sideways, rad.
    bank: f32,
    /// The body rolling over a trick and bouncing on landing.
    tumble: Tumble,
    /// Whether the kart was doing a trick last frame.
    tricking: bool,
    /// How high it was over the road last frame, m, and how fast it climbed.
    was: (f32, f32),
    /// The glow of a pit-crew push, and a bumper's wave with its age (s;
    /// none running when negative), and what the kart held last frame.
    bubble: Entity,
    wave: Entity,
    waved: f32,
    held: Option<osage_kart_items::Item>,
}

pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut skins: ResMut<Assets<SkinnedMeshInverseBindposes>>,
) {
    let stickers = images.add(osage_kart_model::decals::atlas());
    let finishes = Finish::ALL
        .into_iter()
        .map(|finish| {
            let mut material = finish.material();
            if finish == Finish::Decal {
                material.base_color_texture = Some(stickers.clone());
            }
            (finish, materials.add(material))
        })
        .collect();
    let rest: Vec<Mat4> = driver_rest().iter().map(|bone| bone.to_matrix().inverse()).collect();
    // A firework-sized rocket: a red tube with a white band, a nose, three
    // fins, a dark nozzle. Its axis is +Z; its nose points forward.
    let paint = |r: f32, g: f32, b: f32, rough: f32, metal: f32| StandardMaterial {
        base_color: Color::srgb(r, g, b),
        perceptual_roughness: rough,
        metallic: metal,
        ..default()
    };
    let (red, white, steel) = (
        materials.add(paint(0.75, 0.06, 0.04, 0.35, 0.0)),
        materials.add(paint(0.9, 0.9, 0.88, 0.4, 0.0)),
        materials.add(paint(0.2, 0.2, 0.22, 0.3, 0.9)),
    );
    let along = Quat::from_rotation_x(PI / 2.0);
    let (r, l) = (ROCKET_RADIUS, ROCKET_LENGTH);
    let mut rocket = vec![
        (meshes.add(Cylinder::new(r, l * 0.7)), red.clone(), Transform::from_rotation(along)),
        (
            meshes.add(Cylinder::new(r * 1.02, l * 0.1)),
            white.clone(),
            Transform::from_xyz(0.0, 0.0, l * 0.2).with_rotation(along),
        ),
        (
            meshes.add(Cone { radius: r, height: l * 0.3 }),
            red.clone(),
            Transform::from_xyz(0.0, 0.0, l * 0.5).with_rotation(along),
        ),
        (
            meshes.add(Cylinder::new(r * 0.75, l * 0.12)),
            steel.clone(),
            Transform::from_xyz(0.0, 0.0, -l * 0.41).with_rotation(along),
        ),
    ];
    let fin = meshes.add(Cuboid::new(0.012, r * 2.2, l * 0.22));
    for k in 0..3 {
        let around = Quat::from_rotation_z(k as f32 * TAU / 3.0);
        rocket.push((
            fin.clone(),
            white.clone(),
            Transform::from_translation(around * Vec3::Y * r * 1.3 + Vec3::Z * -l * 0.27).with_rotation(around),
        ));
    }
    // What a kart holds, small, on its back.
    let mut paint = |r: f32, g: f32, b: f32, rough: f32, metal: f32| {
        materials.add(StandardMaterial {
            base_color: Color::srgb(r, g, b),
            perceptual_roughness: rough,
            metallic: metal,
            ..default()
        })
    };
    let red = paint(0.85, 0.08, 0.06, 0.5, 0.0);
    let (blue, silver, rubber, orange, black, yellow, green) = (
        paint(0.05, 0.3, 0.85, 0.3, 0.3),
        paint(0.8, 0.8, 0.82, 0.25, 1.0),
        paint(0.035, 0.035, 0.04, 0.85, 0.0),
        paint(1.0, 0.36, 0.05, 0.55, 0.0),
        paint(0.03, 0.03, 0.035, 0.3, 0.2),
        paint(0.95, 0.75, 0.1, 0.4, 0.0),
        paint(0.25, 0.6, 0.3, 0.4, 0.2),
    );
    let at = |x: f32, y: f32, z: f32| Transform::from_xyz(x, y, z);
    let carried: Vec<Parts> = vec![
        // Nitro: a blue bottle, a silver valve.
        vec![
            (meshes.add(Capsule3d::new(0.07, 0.22)), blue, at(0.0, 0.18, 0.0)),
            (meshes.add(Cylinder::new(0.03, 0.08)), silver.clone(), at(0.0, 0.4, 0.0)),
        ],
        // Tyre: a spare wheel lying on the back, tilted to be seen; standing,
        // it hid the driver from the chase camera.
        vec![(
            meshes.add(Torus { minor_radius: 0.055, major_radius: 0.13 }),
            rubber.clone(),
            at(0.0, 0.08, -0.05).with_rotation(Quat::from_rotation_x(0.6)),
        )],
        // Cone.
        vec![(meshes.add(Cone { radius: 0.12, height: 0.32 }), orange, at(0.0, 0.16, 0.0))],
        // Oil: a black can, a yellow band.
        vec![
            (meshes.add(Cylinder::new(0.09, 0.22)), black.clone(), at(0.0, 0.11, 0.0)),
            (meshes.add(Cylinder::new(0.092, 0.05)), yellow.clone(), at(0.0, 0.12, 0.0)),
        ],
        // Rocket: lying along the kart, nose forward.
        rocket
            .iter()
            .map(|(mesh, material, place)| (mesh.clone(), material.clone(), at(0.0, 0.12, 0.0) * *place))
            .collect(),
        // Smoke: a green canister with a nozzle.
        vec![
            (meshes.add(Cylinder::new(0.075, 0.26)), green, at(0.0, 0.13, 0.0)),
            (meshes.add(Cylinder::new(0.025, 0.07)), silver.clone(), at(0.0, 0.3, 0.0)),
        ],
        // Stack: three spare wheels, one on another.
        (0..3)
            .map(|level| {
                (
                    meshes.add(Torus { minor_radius: 0.05, major_radius: 0.12 }),
                    rubber.clone(),
                    at(0.0, 0.05 + 0.1 * level as f32, -0.05),
                )
            })
            .collect(),
        // Drone: a small one, folded on the back.
        vec![(
            meshes.add(crate::paddock::drone_mesh()),
            black.clone(),
            at(0.0, 0.08, 0.0).with_scale(Vec3::splat(0.55)),
        )],
        // Bumper: a red air bag rolled up, with a yellow tab.
        vec![
            (
                meshes.add(Capsule3d::new(0.09, 0.3)),
                red.clone(),
                at(0.0, 0.1, 0.0).with_rotation(Quat::from_rotation_z(FRAC_PI_2)),
            ),
            (meshes.add(Cuboid::new(0.08, 0.05, 0.05)), yellow.clone(), at(0.0, 0.2, 0.0)),
        ],
        // Pit-crew push: the crew's lollipop board, yellow on a pole.
        vec![
            (meshes.add(Cylinder::new(0.015, 0.36)), silver, at(0.0, 0.18, 0.0)),
            (
                meshes.add(Cylinder::new(0.11, 0.02)),
                yellow,
                at(0.0, 0.38, 0.0).with_rotation(Quat::from_rotation_x(FRAC_PI_2)),
            ),
        ],
    ];
    // A pit-crew push: a golden glow about the kart, seen through.
    let bubble = (
        meshes.add(Sphere::new(1.0).mesh().uv(24, 12)),
        materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.82, 0.3, 0.18),
            emissive: LinearRgba::rgb(1.2, 0.8, 0.2),
            alpha_mode: AlphaMode::Add,
            unlit: true,
            ..default()
        }),
    );
    // A bumper's shock wave: a ring of air that spreads and fades.
    let wave = (
        meshes.add(Torus { minor_radius: 0.06, major_radius: 1.0 }),
        materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.95, 0.8, 0.5),
            emissive: LinearRgba::rgb(0.8, 0.7, 0.5),
            alpha_mode: AlphaMode::Add,
            unlit: true,
            ..default()
        }),
    );
    let ghost = materials.add(StandardMaterial {
        base_color: Color::srgba(0.7, 0.88, 1.0, 0.3),
        emissive: LinearRgba::rgb(0.12, 0.2, 0.3),
        perceptual_roughness: 0.6,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    commands.insert_resource(Kit {
        ghost,
        finishes,
        models: HashMap::new(),
        bindposes: skins.add(rest),
        rocket,
        carried,
        bubble,
        wave,
    });
}

/// A team's colours from the palette: the paint, white or black stripes
/// against it, a frame in silver or in the paint's darker shade.
fn livery(colour: usize) -> Livery {
    let (r, g, b) = PAINTS[colour];
    // Racing paint is deeper than the browser client's palette: more saturated, a little darker.
    let base = Vec3::new(r, g, b);
    let grey = base.dot(Vec3::new(0.2126, 0.7152, 0.0722));
    let main = (Vec3::splat(grey).lerp(base, 1.35) * 0.88).clamp(Vec3::ZERO, Vec3::ONE);
    let light = 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.6;
    let second = if light { Vec3::splat(0.12) } else { Vec3::splat(0.92) };
    let frame = if colour.is_multiple_of(2) { Vec3::new(0.8, 0.81, 0.83) } else { main * 0.7 };
    // Each colour has its number, as a team keeps it season after season.
    let number = [7, 12, 23, 31, 44, 58, 66, 89][colour % 8];
    // Suits: anthracite, white, or a deep shade of the team's colour.
    let suit = match colour % 3 {
        0 => Vec3::new(0.13, 0.13, 0.15),
        1 => Vec3::new(0.88, 0.88, 0.86),
        _ => main * 0.45,
    };
    Livery { main, second, frame, suit, number }
}

/// The material of a finish on a kart: a ghost's is the same all over.
fn finish_of(kit: &Kit, finish: &Finish, ghost: bool) -> Handle<StandardMaterial> {
    if ghost { kit.ghost.clone() } else { kit.finishes[finish].clone() }
}

/// Children with the meshes of `part`, each in its finish's material; a
/// ghost's cast no shadow.
fn spawn_part(commands: &mut Commands, kit: &Kit, part: &Meshes, parent: Entity, ghost: bool) {
    for (finish, mesh) in part {
        let mut spawned = commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(finish_of(kit, finish, ghost)),
            Transform::default(),
            ChildOf(parent),
        ));
        if ghost {
            spawned.insert(bevy::light::NotShadowCaster);
        }
    }
}

/// A kart standing still where the scenery puts it, out of the race: in the
/// pit lane, its driver waiting in it.
#[derive(Component)]
pub struct Parked {
    pub colour: usize,
}

/// The model of a colour of kart, made the first time it is needed.
fn model_of(kit: &mut Kit, meshes: &mut Assets<Mesh>, colour: usize) {
    kit.models.entry(colour).or_insert_with(|| {
        let built = osage_kart_model::build(livery(colour));
        let mut add = |part: Vec<(Finish, Mesh)>| -> Meshes {
            part.into_iter().map(|(finish, mesh)| (finish, meshes.add(mesh))).collect()
        };
        let [front, rear] = built.wheels;
        Model {
            body: add(built.body),
            driver: add(built.driver),
            steering: add(built.steering),
            helmet: add(built.helmet),
            wheels: [add(front), add(rear)],
        }
    });
}

/// A kart's parts under `kart`: its body, its head, each wheel's pivot and
/// tyre, its bones, its rockets and the items it can carry.
#[allow(clippy::type_complexity)]
fn assemble(
    commands: &mut Commands,
    kit: &Kit,
    colour: usize,
    kart: Entity,
    ghost: bool,
) -> (Entity, Entity, [(Entity, Entity); 4], Bones, Entity, Vec<Entity>) {
    let model = &kit.models[&colour];
    let body = commands.spawn((Transform::default(), Visibility::default(), ChildOf(kart))).id();
    spawn_part(commands, kit, &model.body, body, ghost);
    // The steering wheel turns about its column; the driver's bones follow it.
    let steering = commands.spawn((steering_column(), Visibility::default(), ChildOf(body))).id();
    spawn_part(commands, kit, &model.steering, steering, ghost);
    let joints = driver_rest().map(|rest| commands.spawn((rest, ChildOf(body))).id());
    for (finish, mesh) in &model.driver {
        let mut spawned = commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(finish_of(kit, finish, ghost)),
            SkinnedMesh { inverse_bindposes: kit.bindposes.clone(), joints: joints.to_vec() },
            // Its bounds move with its bones: never culled by stale ones.
            NoFrustumCulling,
            Transform::default(),
            ChildOf(body),
        ));
        if ghost {
            spawned.insert(bevy::light::NotShadowCaster);
        }
    }
    let head = commands.spawn((Transform::from_translation(NECK), Visibility::default(), ChildOf(body))).id();
    spawn_part(commands, kit, &model.helmet, head, ghost);
    let wheels = [0, 1, 2, 3].map(|i| {
        let pivot = commands.spawn((Transform::from_translation(AXLES[i]), Visibility::default(), ChildOf(body))).id();
        // Wheels on the right are the left ones turned round.
        let facing = if AXLES[i].x < 0.0 { Quat::from_rotation_y(PI) } else { Quat::IDENTITY };
        let tyre = commands.spawn((Transform::from_rotation(facing), Visibility::default(), ChildOf(pivot))).id();
        spawn_part(commands, kit, &model.wheels[i / 2], tyre, ghost);
        (pivot, tyre)
    });
    let rockets = commands.spawn((Transform::from_scale(Vec3::ZERO), Visibility::Hidden, ChildOf(body))).id();
    for at in ROCKETS {
        let rocket = commands.spawn((Transform::from_translation(at), Visibility::default(), ChildOf(rockets))).id();
        for (mesh, material, place) in &kit.rocket {
            commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), *place, ChildOf(rocket)));
        }
    }
    let carried = kit
        .carried
        .iter()
        .map(|parts| {
            let item = commands.spawn((Transform::from_translation(CARRIED), Visibility::Hidden, ChildOf(body))).id();
            for (mesh, material, place) in parts {
                commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), *place, ChildOf(item)));
            }
            item
        })
        .collect();
    (body, head, wheels, Bones { steering, joints }, rockets, carried)
}

/// Gives every parked kart its model.
pub fn park(
    mut commands: Commands,
    mut kit: ResMut<Kit>,
    mut meshes: ResMut<Assets<Mesh>>,
    parked: Query<(Entity, &Parked), Added<Parked>>,
) {
    for (kart, parked) in &parked {
        let colour = parked.colour % PAINTS.len();
        model_of(&mut kit, &mut meshes, colour);
        assemble(&mut commands, &kit, colour, kart, false);
        commands.entity(kart).insert(Visibility::default());
    }
}

/// Gives every new kart its model.
pub fn dress(
    mut commands: Commands,
    mut kit: ResMut<Kit>,
    mut meshes: ResMut<Assets<Mesh>>,
    karts: Query<(Entity, &Pose), Added<Pose>>,
) {
    for (kart, pose) in &karts {
        let colour = pose.color as usize % PAINTS.len();
        model_of(&mut kit, &mut meshes, colour);
        let (body, head, wheels, bones, rockets, carried) = assemble(&mut commands, &kit, colour, kart, pose.ghost);
        let glow = |(mesh, material): &(Handle<Mesh>, Handle<StandardMaterial>)| {
            (
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                bevy::light::NotShadowCaster,
                Visibility::Hidden,
                ChildOf(kart),
            )
        };
        let bubble = commands.spawn((glow(&kit.bubble), Transform::from_xyz(0.0, 0.55, 0.0))).id();
        let wave = commands.spawn((glow(&kit.wave), Transform::from_xyz(0.0, 0.35, 0.0))).id();

        commands.entity(kart).insert((
            Transform::default(),
            Visibility::default(),
            Rig {
                body,
                head,
                bones,
                wheels,
                rockets,
                carried,
                shown: 0.0,
                heading: pose.kart.heading,
                speed: pose.kart.speed,
                steer: 0.0,
                spin: 0.0,
                slip: 0.0,
                lean: 0.0,
                pitch: 0.0,
                hop: 0.0,
                drifting: false,
                torso: Vec2::ZERO,
                sway: Vec2::ZERO,
                cheer: 0.0,
                time: 0.0,
                seed: (kart.index_u32() % 97) as f32 * 0.61,
                attitude: 0.0,
                bank: 0.0,
                tumble: Tumble::default(),
                tricking: false,
                was: (0.0, 0.0),
                bubble,
                wave,
                waved: -1.0,
                held: None,
            },
        ));
    }
}

/// The slope a kart lies on, as rises per metre along its nose and across
/// it towards its left (+X): on the road, the road's; in the air, its path's.
/// Also how high the kart is above the road, m.
fn lie(course: Option<&Course>, kart: &osage_kart_physics::Kart) -> (f32, f32, f32) {
    let (sin, cos) = kart.heading.sin_cos();
    let (nose, left) = (Vec2::new(sin, cos), Vec2::new(cos, -sin));
    let (grade, above) = match course {
        Some(course) => {
            let track = &course.0.track;
            let along = track.locate(kart.x, kart.z).along;
            let (road, slope) = track.lift(along);
            let above = (kart.height - road).max(0.0);
            if above <= 0.02 {
                let (_, heading) = track.at(along);
                (Vec2::new(heading.sin(), heading.cos()) * slope, above)
            } else {
                (flown(kart), above)
            }
        }
        None => (flown(kart), kart.height),
    };
    (grade.dot(nose), grade.dot(left), above)
}

/// A flying kart's path, as a rise per metre in the direction it moves.
fn flown(kart: &osage_kart_physics::Kart) -> Vec2 {
    let (vx, vz) = kart.velocity();
    let v = Vec2::new(vx, vz);
    v * (kart.climb / v.length_squared().max(16.0))
}

/// The kart's centre of mass over the road, m: the body rolls about it.
const MASS_HEIGHT: f32 = 0.35;
/// The kart and driver's moment of inertia about their long axis, per
/// unit of mass, m²: about 160 kg, 1.4 m wide and 1 m tall, a slab.
const ROLL_INERTIA: f32 = 0.25;
/// How hard a body lying on the road is rocked back towards its wheels,
/// rad/s² per rad: more than its weight holds it on its side with — the
/// physics drives on, and a kart drives on its wheels.
const RIGHTING: f32 = 40.0;
/// How much of its speed into the road a tyre gives back on a bounce.
const BOUNCE: f32 = 0.3;

/// The kart's outline across, (half width, height) m: the tyres' outer
/// edges, the driver's shoulders, the helmet's top.
const OUTLINE: [(f32, f32); 4] = [(0.7, 0.0), (0.7, 0.28), (0.26, 0.66), (0.15, 0.97)];

/// The body as a rigid slab rolling about the kart's long axis over the
/// road, apart from the kart's own path (which the physics keeps): it
/// turns freely through the air, and its outline, meeting the road, is
/// pushed out and bounced — a tyre landing first rights it or tips it,
/// as the speed and angle make it (D-096).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Tumble {
    /// Roll, rad (0 upright).
    roll: f32,
    /// Rate of roll, rad/s.
    turning: f32,
    /// How far the body is lifted over where the kart is, m, and how fast.
    lift: f32,
    rising: f32,
}

impl Tumble {
    /// Where a point of the outline is, relative to the kart, and how fast
    /// it moves up.
    fn point(&self, (x, y): (f32, f32)) -> (f32, f32, f32) {
        let (sin, cos) = self.roll.sin_cos();
        let (rx, ry) = (x * cos - (y - MASS_HEIGHT) * sin, x * sin + (y - MASS_HEIGHT) * cos);
        (rx, MASS_HEIGHT + self.lift + ry, self.rising + self.turning * rx)
    }

    /// One step of `dt` s over a road `above` m under the kart.
    fn step(&mut self, dt: f32, above: f32) {
        self.rising -= osage_kart_physics::GRAVITY * dt;
        self.lift += self.rising * dt;
        self.roll += self.turning * dt;
        let mut touching = false;
        for (x, y) in OUTLINE.iter().flat_map(|&(x, y)| [(x, y), (-x, y)]) {
            let (rx, height, up) = self.point((x, y));
            let depth = -above - height;
            if depth <= 0.0 {
                continue;
            }
            touching = true;
            self.lift += depth;
            if up < 0.0 {
                // An impulse at the point, along the road's normal.
                let push = -(1.0 + BOUNCE) * up / (1.0 + rx * rx / ROLL_INERTIA);
                self.rising += push;
                self.turning += push * rx / ROLL_INERTIA;
            }
        }
        // Resting on the road the body cannot hang below the kart, and
        // tyres scrubbing the road slow a roll; a kart on its side or its
        // back is rocked over onto its wheels.
        if self.lift < 0.0 && touching {
            self.lift = 0.0;
            self.rising = self.rising.max(0.0);
        }
        if touching {
            self.turning *= (-3.0 * dt).exp();
            self.turning -= RIGHTING * wrap(self.roll) * dt;
        }
        if self.lift <= 0.0 && above <= 0.02 {
            self.lift = 0.0;
            self.rising = self.rising.max(0.0);
        }
        self.roll = wrap(self.roll);
    }

    /// Settled upright on its wheels.
    fn settled(&self) -> bool {
        self.roll.abs() < 1e-3 && self.turning.abs() < 1e-2 && self.lift.abs() < 1e-3 && self.rising.abs() < 1e-2
    }
}

/// Places every kart and moves its parts from how its state changes.
pub fn animate(
    time: Res<Time>,
    course: Option<Res<Course>>,
    mut karts: Query<(Entity, &Pose, &mut Rig)>,
    mut transforms: Query<&mut Transform>,
    mut visibilities: Query<&mut Visibility>,
) {
    let seconds = time.delta_secs().max(1e-4);
    let ease = |rate: f32| (seconds * rate).min(1.0);
    for (entity, pose, mut rig) in &mut karts {
        let kart = &pose.kart;
        let turning = (wrap(kart.heading - rig.heading) / seconds).clamp(-3.5, 3.5);
        let pushing = ((kart.speed - rig.speed) / seconds).clamp(-40.0, 40.0);
        (rig.heading, rig.speed) = (kart.heading, kart.speed);
        let pace = (kart.speed / MAX_SPEED).clamp(-1.0, 1.3);

        rig.steer += ((turning / 2.3).clamp(-1.0, 1.0) - rig.steer) * ease(12.0);
        rig.spin = (rig.spin + kart.speed * seconds / WHEELS[1]) % TAU;
        // A drifting kart is drawn turned into the slide; it leans out of a
        // bend and lifts its nose when it accelerates.
        rig.slip += (-kart.drift * 0.4 - rig.slip) * ease(9.0);
        rig.lean += ((turning * 0.02 * pace).clamp(-0.05, 0.05) - rig.lean) * ease(7.0);
        rig.pitch += ((-pushing * 0.0015).clamp(-0.02, 0.03) - rig.pitch) * ease(6.0);
        if kart.drift != 0.0 && !rig.drifting {
            rig.hop = HOP;
        }
        rig.drifting = kart.drift != 0.0;
        rig.hop = (rig.hop - seconds).max(0.0);
        let hop = 0.06 * (PI * rig.hop / HOP).sin();
        // Kerbs and grass rattle a kart that has no suspension, the faster
        // the harder; the driver's trunk takes the knocks.
        rig.time += seconds;
        let shaking = roughness(course.as_deref(), kart.x, kart.z) * (kart.speed.abs() / MAX_SPEED).min(1.0);
        let rattle = Vec2::new(wobble(rig.time * 41.0, rig.seed), wobble(rig.time * 29.0, rig.seed + 2.0)) * shaking;

        // The trunk: thrown out of the bend by the turn (speed × rate of
        // turn: up to 2 g in a kart), forward when braking, back when
        // accelerating; its spring a second and a bit, under-damped.
        let wanted = Vec2::new((kart.speed * turning * 0.009).clamp(-0.22, 0.22), (-pushing * 0.01).clamp(-0.1, 0.16));
        let (stiff, damped, step) = (55.0, 2.0 * 0.55 * 55f32.sqrt(), seconds.min(0.05));
        let pull = stiff * (wanted - rig.torso) - damped * rig.sway + rattle * 18.0;
        rig.sway += pull * step;
        let sway = rig.sway;
        rig.torso += sway * step;
        let lean = Quat::from_rotation_x(rig.torso.y) * Quat::from_rotation_z(rig.torso.x);

        // On the road the kart lies on it, whichever way it points: the
        // road's rise, taken along the kart's nose, pitches it, and taken
        // across, rolls it (spun round on a crest, it stood on one wheel
        // and sank the other). Through the air the nose follows the path
        // (D-050); a trick is a full roll of the body while it flies.
        let (rise, across, above) = lie(course.as_deref(), kart);
        rig.attitude += (-rise.atan().clamp(-0.5, 0.5) - rig.attitude) * ease(14.0);
        rig.bank += (across.atan().clamp(-0.5, 0.5) - rig.bank) * ease(14.0);
        // A trick throws the body into a roll timed to land upright, if
        // the jump is long enough; landing, the body keeps falling as the
        // kart fell, onto the road the physics has put the kart back on.
        let tricking = kart.trick > 0.0;
        if tricking && !rig.tricking {
            let g = osage_kart_physics::GRAVITY;
            let flight = (kart.climb + (kart.climb * kart.climb + 2.0 * g * above).sqrt()) / g;
            let side = if (rig.seed as i32) % 2 == 0 { 1.0 } else { -1.0 };
            // A trick a hop off the road (the physics counts a kart a few
            // centimetres up as flying) is no roll: it toppled karts on the
            // grid.
            if flight >= MIN_FLIGHT {
                rig.tumble.turning = side * (TAU / flight).min(TRICK_SPIN);
            }
        }
        rig.tricking = tricking;
        let (was_above, was_climbing) = rig.was;
        if was_above > 0.02 && above <= 0.02 {
            rig.tumble.lift += was_above - above;
            rig.tumble.rising += was_climbing.min(0.0);
        }
        rig.was = (above, kart.climb);
        if !rig.tumble.settled() {
            for _ in 0..8 {
                rig.tumble.step(seconds.min(0.05) / 8.0, above);
            }
        }
        let tumble = rig.tumble;
        if let Ok(mut place) = transforms.get_mut(entity) {
            *place = Transform::from_xyz(kart.x, kart.height + hop + tumble.lift, kart.z).with_rotation(
                Quat::from_rotation_y(kart.heading)
                    * Quat::from_rotation_x(rig.attitude)
                    * Quat::from_rotation_z(rig.bank),
            ) * Transform::from_xyz(0.0, MASS_HEIGHT, 0.0)
                * Transform::from_rotation(Quat::from_rotation_z(tumble.roll))
                * Transform::from_xyz(0.0, -MASS_HEIGHT, 0.0);
        }
        // What it holds, on its back — or, held behind it, on the road
        // behind its tail (D-052).
        for (item, model) in osage_kart_items::Item::ALL.into_iter().zip(&rig.carried) {
            if let Ok(mut visibility) = visibilities.get_mut(*model) {
                visibility.set_if_neq(if pose.held == Some(item) { Visibility::Inherited } else { Visibility::Hidden });
            }
            if pose.held == Some(item)
                && let Ok(mut place) = transforms.get_mut(*model)
            {
                place.translation = if pose.trailing { TRAILED } else { CARRIED };
            }
        }
        // A pit-crew push: a golden glow that breathes, fading at its end.
        if let Ok(mut visibility) = visibilities.get_mut(rig.bubble) {
            visibility.set_if_neq(if pose.guard > 0.0 { Visibility::Inherited } else { Visibility::Hidden });
        }
        if pose.guard > 0.0
            && let Ok(mut place) = transforms.get_mut(rig.bubble)
        {
            let breath = 1.0 + 0.05 * (rig.time * 9.0).sin();
            let size = breath * (pose.guard / 0.3).min(1.0);
            place.scale = Vec3::new(1.05, 0.75, 1.5) * size;
        }
        // A bumper used: a wave of air spreads from the kart and fades.
        if rig.held == Some(osage_kart_items::Item::Bumper) && pose.held != rig.held {
            rig.waved = 0.0;
        }
        rig.held = pose.held;
        if rig.waved >= 0.0 {
            rig.waved += seconds;
        }
        let spread = rig.waved / WAVE;
        if let Ok(mut visibility) = visibilities.get_mut(rig.wave) {
            visibility.set_if_neq(if (0.0..1.0).contains(&spread) {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
        if (0.0..1.0).contains(&spread)
            && let Ok(mut place) = transforms.get_mut(rig.wave)
        {
            let reach = 0.6 + (osage_kart_items::BUMPER_REACH - 0.6) * (1.0 - (1.0 - spread).powi(2));
            place.scale = Vec3::new(reach, 1.0 + 2.0 * (1.0 - spread), reach);
        } else if spread >= 1.0 {
            rig.waved = -1.0;
        }
        // The rockets slide out while the kart has rocket left, shaking as they burn.
        let wanted = if kart.rocket > 0.0 { 1.0 } else { 0.0 };
        rig.shown = (rig.shown + (wanted - rig.shown).signum() * seconds / ROCKET_SHOW).clamp(0.0, 1.0);
        if let Ok(mut visibility) = visibilities.get_mut(rig.rockets) {
            visibility.set_if_neq(if rig.shown > 0.0 { Visibility::Inherited } else { Visibility::Hidden });
        }
        if let Ok(mut rockets) = transforms.get_mut(rig.rockets) {
            let burn = wobble(rig.time * 53.0, rig.seed) * 0.006 * wanted;
            rockets.scale = Vec3::splat(rig.shown);
            rockets.translation = Vec3::new(burn, -0.25 * (1.0 - rig.shown) + burn, 0.0);
        }
        if let Ok(mut body) = transforms.get_mut(rig.body) {
            body.translation.y = rattle.y * 0.006;
            body.rotation = Quat::from_rotation_y(rig.slip)
                * Quat::from_rotation_z(rig.lean + rattle.x * 0.01)
                * Quat::from_rotation_x(rig.pitch);
        }
        // The head goes where the trunk takes it; the neck keeps the eyes
        // nearer level than the shoulders. The driver looks into the turn
        // and leans against the bend.
        // Spun round, the driver shakes their head (D-087).
        let dizzy = (kart.spin.abs() / 4.0).min(1.0);
        let shake = (rig.time * 16.0 + rig.seed).sin() * 0.35 * dizzy;
        if let Ok(mut head) = transforms.get_mut(rig.head) {
            head.translation = neck(lean);
            head.rotation = Quat::IDENTITY.slerp(lean, 0.45)
                * Quat::from_rotation_y(rig.steer * 0.45 - rig.slip * 0.6 + shake)
                * Quat::from_rotation_z(-rig.lean * 1.5);
        }
        // On the podium, the left arm up, waving.
        let wanted = if pose.cheer { 1.0 } else { 0.0 };
        rig.cheer += (wanted - rig.cheer) * (1.0 - (-seconds * 3.0).exp());
        // The steering wheel turns twice as far as the front wheels, the
        // hands on it, the arms following.
        let turn = rig.steer * 1.1;
        if let Ok(mut wheel) = transforms.get_mut(rig.bones.steering) {
            *wheel = steering_column() * Transform::from_rotation(Quat::from_rotation_y(turn));
        }
        for (joint, bone) in rig.bones.joints.iter().zip(driver_cheering(turn, lean, [rig.cheer, 0.0], rig.time)) {
            if let Ok(mut place) = transforms.get_mut(*joint) {
                *place = bone;
            }
        }
        for (i, (pivot, tyre)) in rig.wheels.into_iter().enumerate() {
            if let (true, Ok(mut pivot)) = (i < 2, transforms.get_mut(pivot)) {
                pivot.rotation = Quat::from_rotation_y(rig.steer * 0.55);
            }
            if let Ok(mut tyre) = transforms.get_mut(tyre) {
                let turns = rig.spin * WHEELS[1] / WHEELS[i / 2];
                // A wheel on the right is turned round: it spins the other way about its own X.
                let facing = if AXLES[i].x < 0.0 { Quat::from_rotation_y(PI) } else { Quat::IDENTITY };
                let spin = if AXLES[i].x < 0.0 { -turns } else { turns };
                tyre.rotation = facing * Quat::from_rotation_x(spin);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The lowest point of the body's outline over the road, m.
    fn lowest(tumble: &Tumble, above: f32) -> f32 {
        OUTLINE
            .iter()
            .flat_map(|&(x, y)| [(x, y), (-x, y)])
            .map(|p| tumble.point(p).1 + above)
            .fold(f32::INFINITY, f32::min)
    }

    /// The body falling from `above` m onto the road, rolled by `roll`.
    fn land(roll: f32, turning: f32, falling: f32) -> Vec<Tumble> {
        let mut tumble = Tumble { roll, turning, lift: 0.4, rising: -falling };
        (0..240 * 8)
            .map(|_| {
                tumble.step(1.0 / 240.0, 0.0);
                tumble
            })
            .collect()
    }

    #[test]
    fn a_kart_landing_tilted_bounces_back_onto_its_wheels_never_into_the_road() {
        for (roll, turning) in [(0.0, 0.0), (0.6, 4.0), (-1.2, -6.0), (1.6, 2.0), (2.6, 9.0)] {
            let path = land(roll, turning, 6.0);
            let sunk = path.iter().map(|t| lowest(t, 0.0)).fold(f32::INFINITY, f32::min);
            assert!(sunk > -0.03, "landing at {roll} rad: the outline sank {sunk} m into the road");
            let end = path.last().unwrap();
            assert!(end.roll.abs() < 0.05 && end.lift.abs() < 0.02, "landing at {roll} rad: ended at {end:?}");
        }
    }

    #[test]
    fn a_kart_landing_tilted_tips_over_its_wheel_before_it_settles() {
        let path = land(0.6, 0.0, 6.0);
        let bounced = path.iter().map(|t| t.lift).fold(0.0f32, f32::max);
        assert!(bounced > 0.0, "the tyre's impulse throws the body up");
        assert!(path.iter().any(|t| t.roll < 0.6 && t.turning < 0.0), "the impulse turns it back towards upright");
    }
}
