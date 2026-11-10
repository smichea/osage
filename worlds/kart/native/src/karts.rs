// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The karts: a chunky toy kart with its driver, sculpted from rounded
//! shapes ([`crate::sculpt`]), and the motion that makes it feel alive —
//! wheels that spin and steer, a body that leans in bends, lifts its nose
//! when it accelerates and hops into a drift, a driver who looks into the
//! turn. An original design.
//!
//! Each kart is six meshes: the body with its driver, the driver's head, and
//! four wheels. Their colours are in their vertices, so that all karts share
//! two materials.
//!
//! A kart faces +Z, stands on the ground at its origin, and has its left
//! at +X.

use std::{
    collections::HashMap,
    f32::consts::{FRAC_PI_2, PI, TAU},
};

use bevy::prelude::*;
use osage_kart_physics::{MAX_SPEED, wrap};

use crate::{
    scene::{PALETTE, Pose},
    sculpt::{Batch, Paint, Shape},
};

/// Radius of the front and of the rear wheels, metres.
const WHEELS: [f32; 2] = [0.3, 0.36];
/// Where the wheels are: front left, front right, rear left, rear right.
const AXLES: [Vec3; 4] = [
    Vec3::new(0.7, WHEELS[0], 0.78),
    Vec3::new(-0.7, WHEELS[0], 0.78),
    Vec3::new(0.76, WHEELS[1], -0.62),
    Vec3::new(-0.76, WHEELS[1], -0.62),
];
/// Where the exhaust pipes end.
pub const EXHAUSTS: [Vec3; 2] = [Vec3::new(0.24, 0.66, -1.3), Vec3::new(-0.24, 0.66, -1.3)];
/// Where the rear tyres touch the road.
pub const REAR_CONTACTS: [Vec3; 2] = [Vec3::new(0.76, 0.05, -0.75), Vec3::new(-0.76, 0.05, -0.75)];
/// Where the driver's neck is: the head turns around it.
const NECK: Vec3 = Vec3::new(0.0, 1.08, -0.16);
/// A hop into a drift lasts this long, seconds.
const HOP: f32 = 0.28;

/// The meshes of one colour of kart.
struct Model {
    body: Handle<Mesh>,
    head: Handle<Mesh>,
    wheels: [Handle<Mesh>; 2],
}

/// What the karts share: two materials, and a model per colour, made the
/// first time a kart of that colour comes.
#[derive(Resource)]
pub struct Kit {
    gloss: Handle<StandardMaterial>,
    models: HashMap<usize, Model>,
}

/// The moving parts of a kart and where its motion stands.
#[derive(Component)]
pub struct Rig {
    body: Entity,
    head: Entity,
    /// Each wheel's pivot (the front ones turn to steer) and its tyre (which spins).
    wheels: [(Entity, Entity); 4],
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
}

pub fn setup(mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>) {
    let gloss = materials.add(StandardMaterial { perceptual_roughness: 0.28, reflectance: 0.6, ..default() });
    commands.insert_resource(Kit { gloss, models: HashMap::new() });
}

fn ball(detail: u32) -> Shape {
    Shape::new(Sphere::new(1.0).mesh().ico(detail).expect("a small icosphere"))
}

/// A tube along a path, its seam on the side of `beside`.
fn tube(path: &[Vec3], radius: f32) -> Shape {
    Shape::sweep(path, radius, 8, Vec3::X)
}

fn at(x: f32, y: f32, z: f32) -> Transform {
    Transform::from_xyz(x, y, z)
}

fn sized(transform: Transform, x: f32, y: f32, z: f32) -> Transform {
    transform.with_scale(Vec3::new(x, y, z))
}

/// The body of a kart of colour `(r, g, b)`, with its driver but not the head.
fn body(colour: (f32, f32, f32), suit: (f32, f32, f32)) -> Batch {
    let (r, g, b) = colour;
    let paint = Paint::blend((r * 0.55, g * 0.55, b * 0.6), (r, g, b));
    let white = Paint::blend((0.72, 0.74, 0.78), (0.98, 0.98, 0.98));
    let dark = Paint::blend((0.05, 0.05, 0.06), (0.16, 0.16, 0.18));
    let chrome = Paint::blend((0.5, 0.52, 0.56), (0.95, 0.96, 1.0));
    let suit = Paint::blend((suit.0 * 0.6, suit.1 * 0.6, suit.2 * 0.6), suit);
    let mut kart = Batch::default();
    let round = ball(3);

    // The tub: a rounded shell, wider at the back, with a nose that dips.
    let shell = Shape::lathe(
        &[(0.0, -1.0), (0.55, -0.92), (0.86, -0.6), (0.98, -0.1), (0.96, 0.35), (0.8, 0.72), (0.45, 0.96), (0.0, 1.0)],
        20,
    );
    // The lathe's axis lies along the kart: its foot at the back, its top at the nose.
    let along = Quat::from_rotation_x(FRAC_PI_2);
    kart.add(&shell, sized(at(0.0, 0.42, -0.05).with_rotation(along), 0.6, 1.1, 0.3), paint);
    kart.add(&round, sized(at(0.0, 0.36, 0.86), 0.42, 0.2, 0.42), paint);
    // A floor pan under it, and side pods with white stripes.
    kart.add(&Shape::block(Vec3::new(-0.62, 0.16, -1.05), Vec3::new(0.62, 0.24, 1.05)), Transform::IDENTITY, dark);
    for side in [-1.0, 1.0] {
        kart.add(&round, sized(at(side * 0.6, 0.36, -0.08), 0.2, 0.17, 0.62), white);
        kart.add(&round, sized(at(side * 0.66, 0.4, -0.08), 0.12, 0.1, 0.5), paint);
    }
    // Bumpers: a curved bar in front, a plate behind.
    let front: Vec<Vec3> = (0..=10)
        .map(|k| {
            let a = (k as f32 / 10.0 - 0.5) * 2.4;
            Vec3::new(0.62 * a.sin(), 0.26, 1.02 + 0.28 * a.cos())
        })
        .collect();
    kart.add(&tube(&front, 0.08), Transform::IDENTITY, white);
    kart.add(&round, sized(at(0.0, 0.3, -1.14), 0.6, 0.12, 0.12), white);
    // The engine behind the seat, with two pipes that curve up and out.
    kart.add(&Shape::block(Vec3::new(-0.34, 0.3, -1.08), Vec3::new(0.34, 0.68, -0.7)), Transform::IDENTITY, dark);
    for side in [-1.0f32, 1.0] {
        let pipe: Vec<Vec3> = (0..=6)
            .map(|k| {
                let t = k as f32 / 6.0;
                Vec3::new(side * (0.16 + 0.08 * t), 0.5 + 0.16 * t * t, -0.86 - 0.44 * t)
            })
            .collect();
        kart.add(&tube(&pipe, 0.075), Transform::IDENTITY, chrome);
    }
    // The wing on two struts.
    kart.add(&round, sized(at(0.0, 1.08, -1.0), 0.66, 0.05, 0.2), paint);
    for side in [-0.36, 0.36] {
        kart.add(
            &Shape::block(Vec3::new(side - 0.03, 0.6, -1.05), Vec3::new(side + 0.03, 1.06, -0.95)),
            Transform::IDENTITY,
            dark,
        );
    }
    // The seat, the steering column and wheel.
    kart.add(&round, sized(at(0.0, 0.66, -0.48), 0.32, 0.36, 0.1), dark);
    kart.add(&tube(&[Vec3::new(0.0, 0.42, 0.62), Vec3::new(0.0, 0.84, 0.32)], 0.035), Transform::IDENTITY, chrome);
    let ring = Shape::new(
        Torus { minor_radius: 0.035, major_radius: 0.17 }.mesh().minor_resolution(6).major_resolution(16).build(),
    );
    kart.add(&ring, at(0.0, 0.86, 0.3).with_rotation(Quat::from_rotation_x(1.05)), dark);

    // The driver: a round body, arms to the wheel, white gloves, legs forward.
    kart.add(&round, sized(at(0.0, 0.78, -0.26), 0.27, 0.3, 0.24), suit);
    kart.add(&round, sized(at(0.0, 0.95, -0.24), 0.2, 0.08, 0.19), white);
    for side in [-1.0f32, 1.0] {
        let shoulder = Vec3::new(side * 0.23, 0.95, -0.22);
        let elbow = Vec3::new(side * 0.3, 0.8, 0.02);
        let hand = Vec3::new(side * 0.15, 0.9, 0.27);
        kart.add(&tube(&[shoulder, elbow, hand], 0.07), Transform::IDENTITY, suit);
        kart.add(&round, sized(Transform::from_translation(hand), 0.09, 0.09, 0.09), white);
        let (hip, knee) = (Vec3::new(side * 0.14, 0.55, -0.2), Vec3::new(side * 0.17, 0.62, 0.25));
        kart.add(&tube(&[hip, knee, Vec3::new(side * 0.15, 0.42, 0.5)], 0.085), Transform::IDENTITY, suit);
        kart.add(&round, sized(at(side * 0.15, 0.42, 0.56), 0.09, 0.08, 0.13), dark);
    }
    // A white disc on the nose for a number.
    let disc = Shape::new(Circle::new(1.0).mesh().resolution(16).build());
    kart.add(&disc, sized(at(0.0, 0.47, 0.98).with_rotation(Quat::from_rotation_x(-0.9)), 0.17, 0.17, 1.0), white);
    kart
}

/// The driver's head, around the neck: a big round face, a cap of the
/// kart's colour with its peak, eyes that look ahead.
/// What a driver wears on the head: each colour of kart has its own, so
/// that every driver can be told apart at a glance.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Headwear {
    Cap,
    Helmet,
    Beanie,
    Bun,
}

const HEADWEAR: [Headwear; 4] = [Headwear::Cap, Headwear::Helmet, Headwear::Beanie, Headwear::Bun];
/// Hair colours, for the drivers whose hair shows.
const HAIR: [(f32, f32, f32); 4] = [(0.32, 0.18, 0.08), (0.95, 0.78, 0.36), (0.1, 0.08, 0.07), (0.72, 0.3, 0.12)];

fn head(colour: (f32, f32, f32), skin: (f32, f32, f32), wear: Headwear, hair: (f32, f32, f32)) -> Batch {
    let (r, g, b) = colour;
    let round = ball(3);
    let mut head = Batch::default();
    let skin = Paint::blend((skin.0 * 0.8, skin.1 * 0.75, skin.2 * 0.75), skin);
    head.add(&round, sized(at(0.0, 0.27, 0.0), 0.29, 0.27, 0.27), skin);
    let dome = Shape::lathe(&[(1.0, 0.0), (0.97, 0.3), (0.8, 0.66), (0.48, 0.9), (0.0, 1.0)], 16);
    let kart = Paint::blend((r * 0.65, g * 0.65, b * 0.7), (r, g, b));
    let white = Paint::plain(0.98, 0.98, 0.98);
    let hair = Paint::blend((hair.0 * 0.6, hair.1 * 0.6, hair.2 * 0.6), hair);
    let tilt = |y: f32, z: f32, angle: f32| at(0.0, y, z).with_rotation(Quat::from_rotation_x(angle));
    match wear {
        // A cap of the kart's colour, its peak in front, a button on top.
        Headwear::Cap => {
            head.add(&dome, sized(tilt(0.33, -0.01, -0.15), 0.305, 0.25, 0.3), kart);
            head.add(&round, sized(tilt(0.36, 0.28, -0.12), 0.22, 0.035, 0.14), kart);
            head.add(&round, sized(at(0.0, 0.58, 0.04), 0.07, 0.04, 0.07), white);
        }
        // An open-face helmet down to the ears, a white stripe over the top.
        Headwear::Helmet => {
            head.add(&dome, sized(tilt(0.16, -0.03, -0.25), 0.34, 0.42, 0.34), kart);
            let stripe: Vec<Vec3> =
                // From the back of the neck to the top of the forehead, clear of the eyes.
                (0..=12).map(|k| Vec3::from((Vec2::from_angle(k as f32 / 12.0 * (PI - 0.75)) * 0.355, 0.0))).collect();
            let over = Quat::from_rotation_y(FRAC_PI_2);
            head.add(&tube(&stripe, 0.04), at(0.0, 0.2, -0.03).with_rotation(over), white);
            for side in [-1.0, 1.0] {
                head.add(&round, sized(at(side * 0.31, 0.25, 0.0), 0.05, 0.09, 0.09), white);
            }
        }
        // A knitted beanie with a rolled brim and a pompom; a fringe shows.
        Headwear::Beanie => {
            head.add(&dome, sized(tilt(0.36, -0.03, -0.18), 0.3, 0.32, 0.29), kart);
            let brim = Shape::new(
                Torus { minor_radius: 0.045, major_radius: 0.27 }
                    .mesh()
                    .minor_resolution(8)
                    .major_resolution(20)
                    .build(),
            );
            head.add(&brim, tilt(0.38, 0.0, -0.18), white);
            head.add(&round, sized(at(0.0, 0.72, -0.1), 0.09, 0.09, 0.09), white);
            head.add(&round, sized(tilt(0.37, 0.2, -0.5), 0.2, 0.05, 0.07), hair);
        }
        // Hair, gathered in a bun with a ribbon of the kart's colour.
        Headwear::Bun => {
            head.add(&dome, sized(tilt(0.28, -0.05, -0.3), 0.305, 0.28, 0.3), hair);
            head.add(&round, sized(at(0.0, 0.52, -0.2), 0.14, 0.14, 0.14), hair);
            for side in [-1.0, 1.0] {
                let loop_ = at(side * 0.1, 0.62, -0.24).with_rotation(Quat::from_rotation_z(side * 0.5));
                head.add(&round, sized(loop_, 0.09, 0.05, 0.04), kart);
            }
        }
    }
    // Eyes: white, with dark pupils looking ahead; a small round nose.
    for side in [-1.0, 1.0] {
        head.add(&round, sized(at(side * 0.1, 0.26, 0.23), 0.07, 0.09, 0.05), Paint::plain(1.0, 1.0, 1.0));
        head.add(&round, sized(at(side * 0.1, 0.25, 0.27), 0.035, 0.05, 0.02), Paint::plain(0.05, 0.05, 0.08));
    }
    head.add(&round, sized(at(0.0, 0.18, 0.27), 0.05, 0.045, 0.04), skin);
    head
}

/// A wheel of radius `radius`: a fat tyre with tread blocks, a hub of the
/// kart's colour with a white rim. Its axle is X.
fn wheel(radius: f32, colour: (f32, f32, f32)) -> (Batch, Batch) {
    let (mut rubber, mut hub) = (Batch::default(), Batch::default());
    let tyre = Shape::new(
        Torus { minor_radius: radius * 0.42, major_radius: radius * 0.58 }
            .mesh()
            .minor_resolution(10)
            .major_resolution(24)
            .build(),
    );
    let axle = Quat::from_rotation_z(FRAC_PI_2);
    rubber.add(
        &tyre,
        Transform::from_rotation(axle).with_scale(Vec3::new(1.0, 1.35, 1.0)),
        Paint::blend((0.03, 0.03, 0.035), (0.09, 0.09, 0.1)),
    );
    for k in 0..14 {
        let around = Quat::from_rotation_x(k as f32 / 14.0 * TAU);
        let block = Shape::block(
            Vec3::new(-radius * 0.5, radius * 0.94, -0.035),
            Vec3::new(radius * 0.5, radius * 1.01, 0.035),
        );
        rubber.add(&block, Transform::from_rotation(around), Paint::plain(0.07, 0.07, 0.08));
    }
    let disc = Shape::lathe(&[(0.0, 0.0), (0.5, 0.0), (0.56, 0.08), (0.4, 0.2), (0.0, 0.24)], 14);
    let (r, g, b) = colour;
    for side in [-1.0f32, 1.0] {
        let face = Transform::from_rotation(Quat::from_rotation_z(-side * FRAC_PI_2))
            .with_translation(Vec3::X * side * radius * 0.4);
        let face = face.with_scale(Vec3::splat(radius));
        hub.add(&disc, face, Paint::blend((0.85, 0.85, 0.86), (r, g, b)));
        // A bar across the hub, so that the spin shows.
        let bar = Shape::block(Vec3::new(-0.42, 0.18, -0.07), Vec3::new(0.42, 0.27, 0.07));
        hub.add(&bar, face, Paint::plain(0.95, 0.95, 0.95));
    }
    (rubber, hub)
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
        let model = kit.models.entry(colour).or_insert_with(|| {
            let paint = PALETTE[colour];
            // The suit is a paler shade of the kart; the face, one of four skins.
            let suit = (paint.0 * 0.4 + 0.55, paint.1 * 0.4 + 0.55, paint.2 * 0.4 + 0.55);
            let skins = [(1.0, 0.82, 0.68), (0.88, 0.66, 0.48), (0.66, 0.46, 0.32), (0.46, 0.32, 0.23)];
            // Tyre and hub are one mesh: the rubber is painted dark.
            let wheels = WHEELS.map(|radius| {
                let (mut tyre, hub) = wheel(radius, paint);
                tyre.append(hub);
                meshes.add(tyre.merged())
            });
            Model {
                body: meshes.add(body(paint, suit).merged()),
                head: meshes
                    .add(head(paint, skins[colour % skins.len()], HEADWEAR[colour % 4], HAIR[colour / 2 % 4]).merged()),
                wheels,
            }
        });
        let (body_mesh, head_mesh, wheel_meshes) = (model.body.clone(), model.head.clone(), model.wheels.clone());
        let gloss = kit.gloss.clone();

        let body = commands.spawn((Transform::default(), Visibility::default(), ChildOf(kart))).id();
        commands.spawn((Mesh3d(body_mesh), MeshMaterial3d(gloss.clone()), Transform::default(), ChildOf(body)));
        let head = commands
            .spawn((Mesh3d(head_mesh), MeshMaterial3d(gloss.clone()), Transform::from_translation(NECK), ChildOf(body)))
            .id();
        let wheels = [0, 1, 2, 3].map(|i| {
            let pivot =
                commands.spawn((Transform::from_translation(AXLES[i]), Visibility::default(), ChildOf(body))).id();
            let mesh = wheel_meshes[i / 2].clone();
            // Wheels on the right are the left ones turned round.
            let facing = if AXLES[i].x < 0.0 { Quat::from_rotation_y(PI) } else { Quat::IDENTITY };
            let tyre = commands.spawn((Transform::from_rotation(facing), Visibility::default(), ChildOf(pivot))).id();
            commands.spawn((Mesh3d(mesh), MeshMaterial3d(gloss.clone()), Transform::default(), ChildOf(tyre)));
            (pivot, tyre)
        });

        commands.entity(kart).insert((
            Transform::default(),
            Visibility::default(),
            Rig {
                body,
                head,
                wheels,
                heading: pose.kart.heading,
                speed: pose.kart.speed,
                steer: 0.0,
                spin: 0.0,
                slip: 0.0,
                lean: 0.0,
                pitch: 0.0,
                hop: 0.0,
                drifting: false,
            },
        ));
    }
}

/// Places every kart and moves its parts from how its state changes.
pub fn animate(time: Res<Time>, mut karts: Query<(Entity, &Pose, &mut Rig)>, mut transforms: Query<&mut Transform>) {
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
        rig.lean += ((turning * 0.04 * pace).clamp(-0.11, 0.11) - rig.lean) * ease(7.0);
        rig.pitch += ((-pushing * 0.0035).clamp(-0.05, 0.07) - rig.pitch) * ease(6.0);
        if kart.drift != 0.0 && !rig.drifting {
            rig.hop = HOP;
        }
        rig.drifting = kart.drift != 0.0;
        rig.hop = (rig.hop - seconds).max(0.0);
        let hop = 0.22 * (PI * rig.hop / HOP).sin();

        if let Ok(mut place) = transforms.get_mut(entity) {
            *place = Transform::from_xyz(kart.x, hop, kart.z).with_rotation(Quat::from_rotation_y(kart.heading));
        }
        if let Ok(mut body) = transforms.get_mut(rig.body) {
            body.rotation =
                Quat::from_rotation_y(rig.slip) * Quat::from_rotation_z(rig.lean) * Quat::from_rotation_x(rig.pitch);
        }
        // The driver looks into the turn and leans against the bend.
        if let Ok(mut head) = transforms.get_mut(rig.head) {
            head.rotation =
                Quat::from_rotation_y(rig.steer * 0.45 - rig.slip * 0.6) * Quat::from_rotation_z(-rig.lean * 1.5);
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
