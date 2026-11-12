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
    AXLES, DRIVER_BONES, Finish, Livery, NECK, WHEELS, driver_pose, driver_rest, neck, steering_column,
};
pub use osage_kart_model::{EXHAUSTS, REAR_CONTACTS};
use osage_kart_physics::{MAX_SPEED, wrap};

use crate::scene::{Course, PALETTE, Pose, roughness, wobble};

/// A hop into a drift lasts this long, seconds.
const HOP: f32 = 0.28;
/// Seconds a trick's roll takes.
const TRICK_ROLL: f32 = 0.45;

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
    /// Seconds driven, and this kart's own offset, for its rumble.
    time: f32,
    seed: f32,
    /// The nose up a crest and through the air, radians (down is positive).
    attitude: f32,
    /// How far through its roll a trick is, 0 to 1.
    trick: f32,
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
    commands.insert_resource(Kit {
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
    let (r, g, b) = PALETTE[colour];
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

/// Children with the meshes of `part`, each in its finish's material.
fn spawn_part(commands: &mut Commands, kit: &Kit, part: &Meshes, parent: Entity) {
    for (finish, mesh) in part {
        commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(kit.finishes[finish].clone()),
            Transform::default(),
            ChildOf(parent),
        ));
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
) -> (Entity, Entity, [(Entity, Entity); 4], Bones, Entity, Vec<Entity>) {
    let model = &kit.models[&colour];
    let body = commands.spawn((Transform::default(), Visibility::default(), ChildOf(kart))).id();
    spawn_part(commands, kit, &model.body, body);
    // The steering wheel turns about its column; the driver's bones follow it.
    let steering = commands.spawn((steering_column(), Visibility::default(), ChildOf(body))).id();
    spawn_part(commands, kit, &model.steering, steering);
    let joints = driver_rest().map(|rest| commands.spawn((rest, ChildOf(body))).id());
    for (finish, mesh) in &model.driver {
        commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(kit.finishes[finish].clone()),
            SkinnedMesh { inverse_bindposes: kit.bindposes.clone(), joints: joints.to_vec() },
            // Its bounds move with its bones: never culled by stale ones.
            NoFrustumCulling,
            Transform::default(),
            ChildOf(body),
        ));
    }
    let head = commands.spawn((Transform::from_translation(NECK), Visibility::default(), ChildOf(body))).id();
    spawn_part(commands, kit, &model.helmet, head);
    let wheels = [0, 1, 2, 3].map(|i| {
        let pivot = commands.spawn((Transform::from_translation(AXLES[i]), Visibility::default(), ChildOf(body))).id();
        // Wheels on the right are the left ones turned round.
        let facing = if AXLES[i].x < 0.0 { Quat::from_rotation_y(PI) } else { Quat::IDENTITY };
        let tyre = commands.spawn((Transform::from_rotation(facing), Visibility::default(), ChildOf(pivot))).id();
        spawn_part(commands, kit, &model.wheels[i / 2], tyre);
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
        let colour = parked.colour % PALETTE.len();
        model_of(&mut kit, &mut meshes, colour);
        assemble(&mut commands, &kit, colour, kart);
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
        let colour = pose.color as usize % PALETTE.len();
        model_of(&mut kit, &mut meshes, colour);
        let (body, head, wheels, bones, rockets, carried) = assemble(&mut commands, &kit, colour, kart);
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
                time: 0.0,
                seed: (kart.index_u32() % 97) as f32 * 0.61,
                attitude: 0.0,
                trick: 0.0,
                bubble,
                wave,
                waved: -1.0,
                held: None,
            },
        ));
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

        // Up a crest and through the air, the nose follows the path (D-050);
        // a trick is a full roll of the body while it flies.
        let path = -(kart.climb / kart.speed.abs().max(4.0)).atan().clamp(-0.5, 0.5);
        rig.attitude += (path - rig.attitude) * ease(14.0);
        if kart.trick > 0.0 {
            rig.trick = (rig.trick + seconds / TRICK_ROLL).min(1.0);
        } else {
            rig.trick = 0.0;
        }
        let eased = rig.trick * rig.trick * (3.0 - 2.0 * rig.trick);
        let roll = TAU * eased * if (rig.seed as i32) % 2 == 0 { 1.0 } else { -1.0 };
        if let Ok(mut place) = transforms.get_mut(entity) {
            *place = Transform::from_xyz(kart.x, kart.height + hop, kart.z)
                .with_rotation(Quat::from_rotation_y(kart.heading) * Quat::from_rotation_x(rig.attitude))
                * Transform::from_xyz(0.0, 0.5, 0.0)
                * Transform::from_rotation(Quat::from_rotation_z(roll))
                * Transform::from_xyz(0.0, -0.5, 0.0);
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
        if let Ok(mut head) = transforms.get_mut(rig.head) {
            head.translation = neck(lean);
            head.rotation = Quat::IDENTITY.slerp(lean, 0.45)
                * Quat::from_rotation_y(rig.steer * 0.45 - rig.slip * 0.6)
                * Quat::from_rotation_z(-rig.lean * 1.5);
        }
        // The steering wheel turns twice as far as the front wheels, the
        // hands on it, the arms following.
        let turn = rig.steer * 1.1;
        if let Ok(mut wheel) = transforms.get_mut(rig.bones.steering) {
            *wheel = steering_column() * Transform::from_rotation(Quat::from_rotation_y(turn));
        }
        for (joint, bone) in rig.bones.joints.iter().zip(driver_pose(turn, lean)) {
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
