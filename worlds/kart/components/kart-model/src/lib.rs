// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A racing kart and its driver, to scale: a tubular steel frame five
//! centimetres off the road, a 1.04 m wheelbase, slick tyres on alloy rims,
//! plastic bodywork in the team's colour — nose, front panel, side pods,
//! rear bumper — a bucket seat, the engine on the driver's right with its
//! finned head and chromed exhaust, and the driver in a race suit, gloves
//! and a varnished helmet with a dark visor.
//!
//! Built from simple shapes (`osage-procedural`), each in one of a few
//! [`Finish`]es — varnished paint, satin plastic, rubber, steel, alloy,
//! chrome, cloth, visor — so that a kart is a handful of meshes with
//! physically plausible materials. Colours are in the vertices.
//!
//! The kart's space: +Z ahead, +X to the left, +Y up, the origin on the
//! road under the middle of the wheelbase. Moving parts are separate: the
//! wheels (axle along X, to spin and steer) and the helmet (around the neck,
//! to turn into bends).

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::prelude::*;
use osage_procedural::{Batch, Paint, Shape};

/// Radius of the front and of the rear tyres, metres: 10 and 11 inch slicks.
pub const WHEELS: [f32; 2] = [0.135, 0.14];
/// Width of the front and of the rear tyres, metres.
const TREADS: [f32; 2] = [0.13, 0.19];
/// Where the wheels' centres are: front left, front right, rear left, rear right.
pub const AXLES: [Vec3; 4] = [
    Vec3::new(0.53, WHEELS[0], 0.52),
    Vec3::new(-0.53, WHEELS[0], 0.52),
    Vec3::new(0.6, WHEELS[1], -0.52),
    Vec3::new(-0.6, WHEELS[1], -0.52),
];
/// Where the exhaust ends: the silencer behind the rear bumper, on the right.
pub const EXHAUSTS: [Vec3; 2] = [Vec3::new(-0.44, 0.3, -1.0), Vec3::new(-0.44, 0.31, -0.99)];
/// Where the rear tyres touch the road.
pub const REAR_CONTACTS: [Vec3; 2] = [Vec3::new(0.6, 0.02, -0.52), Vec3::new(-0.6, 0.02, -0.52)];
/// Where the driver's neck is: the helmet turns around it.
pub const NECK: Vec3 = Vec3::new(0.0, 0.74, -0.27);

/// How a surface reflects light. Each is one material, shared by every kart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Finish {
    /// Glossy paint on plastic or fibreglass: the bodywork, the helmet.
    Varnish,
    /// Satin plastic: the seat, the airbox, the floor tray, the grips.
    Satin,
    /// Rubber: the tyres, the boots, the gloves.
    Rubber,
    /// Painted steel: the frame.
    Steel,
    /// Machined alloy: the rims, the engine, the axle.
    Alloy,
    /// Chrome: the exhaust.
    Chrome,
    /// Race-suit cloth.
    Cloth,
    /// The helmet's tinted visor.
    Visor,
}

impl Finish {
    pub const ALL: [Finish; 8] = [
        Finish::Varnish,
        Finish::Satin,
        Finish::Rubber,
        Finish::Steel,
        Finish::Alloy,
        Finish::Chrome,
        Finish::Cloth,
        Finish::Visor,
    ];

    /// The material of this finish; colours come from the vertices.
    pub fn material(self) -> StandardMaterial {
        let base = StandardMaterial { base_color: Color::WHITE, ..default() };
        match self {
            // Glossy paint. Bevy's clear coat paled every dome that faced the
            // sky to near white (a red helmet looked grey; 2026-11-11): a
            // smooth base with a stronger specular stands in for it.
            Finish::Varnish => StandardMaterial { perceptual_roughness: 0.25, reflectance: 0.6, ..base },
            Finish::Satin => StandardMaterial { perceptual_roughness: 0.5, reflectance: 0.4, ..base },
            Finish::Rubber => StandardMaterial { perceptual_roughness: 0.82, reflectance: 0.35, ..base },
            Finish::Steel => StandardMaterial { perceptual_roughness: 0.3, metallic: 0.6, ..base },
            Finish::Alloy => StandardMaterial { perceptual_roughness: 0.28, metallic: 1.0, ..base },
            Finish::Chrome => StandardMaterial { perceptual_roughness: 0.08, metallic: 1.0, ..base },
            Finish::Cloth => StandardMaterial { perceptual_roughness: 0.88, reflectance: 0.3, ..base },
            Finish::Visor => StandardMaterial { perceptual_roughness: 0.04, reflectance: 0.9, ..base },
        }
    }
}

/// A team's colours, in sRGB: the bodywork and helmet, a second colour for
/// stripes and the suit's panels, and the frame's paint.
#[derive(Clone, Copy, Debug)]
pub struct Livery {
    pub main: Vec3,
    pub second: Vec3,
    pub frame: Vec3,
    /// The racing number, on the front panel and on the side pods.
    pub number: u32,
}

/// The seven segments of a digit, as (centre x, centre y, horizontal), in
/// a cell one wide and two high: top, upper left, upper right, middle,
/// lower left, lower right, bottom.
const SEGMENTS: [(f32, f32, bool); 7] = [
    (0.5, 2.0, true),
    (0.0, 1.5, false),
    (1.0, 1.5, false),
    (0.5, 1.0, true),
    (0.0, 0.5, false),
    (1.0, 0.5, false),
    (0.5, 0.0, true),
];
/// Which segments each digit lights.
const DIGITS: [[bool; 7]; 10] = [
    [true, true, true, false, true, true, true],
    [false, false, true, false, false, true, false],
    [true, false, true, true, true, false, true],
    [true, false, true, true, false, true, true],
    [false, true, true, true, false, true, false],
    [true, true, false, true, false, true, true],
    [true, true, false, true, true, true, true],
    [true, false, true, false, false, true, false],
    [true, true, true, true, true, true, true],
    [true, true, true, true, false, true, true],
];

/// `number` in segments, `height` high, centred on the origin of the XY
/// plane, standing `proud` in front of it (+Z).
fn numerals(number: u32, height: f32, proud: f32) -> Vec<Shape> {
    let digits: Vec<usize> = number.to_string().bytes().map(|b| usize::from(b - b'0')).collect();
    let cell = height / 2.0;
    let (stroke, gap) = (height * 0.14, height * 0.35);
    let width = digits.len() as f32 * cell + (digits.len() as f32 - 1.0) * gap;
    let mut shapes = Vec::new();
    for (k, &digit) in digits.iter().enumerate() {
        let left = -width / 2.0 + k as f32 * (cell + gap);
        for (lit, &(x, y, across)) in DIGITS[digit].iter().zip(SEGMENTS.iter()) {
            if !lit {
                continue;
            }
            let centre = Vec2::new(left + x * cell, -height / 2.0 + y * cell);
            let half = if across {
                Vec2::new(cell / 2.0 + stroke / 2.0, stroke / 2.0)
            } else {
                Vec2::new(stroke / 2.0, cell / 2.0 + stroke / 2.0)
            };
            shapes.push(Shape::block((centre - half).extend(0.0), (centre + half).extend(proud)));
        }
    }
    shapes
}

/// One part of a kart: a mesh per finish.
pub type Part = Vec<(Finish, Mesh)>;

/// The meshes of a kart in one livery.
pub struct Kart {
    /// Frame, bodywork, engine, seat, steering and the driver below the neck.
    pub body: Part,
    /// The helmet, around [`NECK`].
    pub helmet: Part,
    /// A front and a rear wheel, around their centre, axle along X, for the
    /// left side; the right side's are turned round.
    pub wheels: [Part; 2],
}

/// Shapes gathered per finish.
#[derive(Default)]
struct Sorted(std::collections::BTreeMap<Finish, Batch>);

impl Sorted {
    fn add(&mut self, finish: Finish, shape: &Shape, place: Transform, colour: Vec3) {
        self.0.entry(finish).or_default().add(shape, place, Paint::plain(colour.x, colour.y, colour.z));
    }

    fn part(self) -> Part {
        self.0.into_iter().map(|(finish, batch)| (finish, batch.merged())).collect()
    }
}

fn at(x: f32, y: f32, z: f32) -> Transform {
    Transform::from_xyz(x, y, z)
}

/// A capsule along Y: `radius`, and `length` between the centres of its ends.
fn capsule(radius: f32, length: f32) -> Shape {
    Shape::new(Capsule3d::new(radius, length).mesh().rings(4).latitudes(10).longitudes(16).build())
}

fn tube(path: &[Vec3], radius: f32) -> Shape {
    Shape::sweep(path, radius, 10, Vec3::Y)
}

/// The same path on the other side of the kart.
fn mirrored(path: &[Vec3]) -> Vec<Vec3> {
    path.iter().map(|p| Vec3::new(-p.x, p.y, p.z)).collect()
}

// Colours in sRGB, as paint takes them.
const BLACK: Vec3 = Vec3::new(0.09, 0.09, 0.1);
const RUBBER: Vec3 = Vec3::new(0.11, 0.11, 0.115);
const ALLOY: Vec3 = Vec3::new(0.86, 0.86, 0.88);
const CHROME: Vec3 = Vec3::new(0.97, 0.97, 0.98);
const WHITE: Vec3 = Vec3::new(0.92, 0.92, 0.9);

/// A kart and its driver in `livery`.
pub fn build(livery: Livery) -> Kart {
    Kart { body: body(livery), helmet: helmet(livery), wheels: [wheel(0, livery), wheel(1, livery)] }
}

fn body(livery: Livery) -> Part {
    let mut kart = Sorted::default();
    frame(&mut kart, livery);
    bodywork(&mut kart, livery);
    engine(&mut kart);
    driver(&mut kart, livery);
    kart.part()
}

/// The frame: two rails five centimetres off the road, cross members, the
/// bumper bars, the stub axles, the rear axle and the steering column.
fn frame(kart: &mut Sorted, livery: Livery) {
    let rail = [
        Vec3::new(0.16, 0.05, 0.72),
        Vec3::new(0.2, 0.05, 0.45),
        Vec3::new(0.28, 0.05, 0.05),
        Vec3::new(0.31, 0.05, -0.35),
        Vec3::new(0.3, 0.06, -0.62),
    ];
    let tubes = [
        rail.to_vec(),
        mirrored(&rail),
        vec![Vec3::new(-0.21, 0.05, 0.42), Vec3::new(0.21, 0.05, 0.42)],
        vec![Vec3::new(-0.29, 0.05, -0.05), Vec3::new(0.29, 0.05, -0.05)],
        vec![Vec3::new(-0.31, 0.055, -0.45), Vec3::new(0.31, 0.055, -0.45)],
        // Side bumper bars, out to the wheels' line.
        vec![
            Vec3::new(0.25, 0.07, 0.3),
            Vec3::new(0.47, 0.08, 0.24),
            Vec3::new(0.49, 0.08, -0.26),
            Vec3::new(0.3, 0.07, -0.32),
        ],
        mirrored(&[
            Vec3::new(0.25, 0.07, 0.3),
            Vec3::new(0.47, 0.08, 0.24),
            Vec3::new(0.49, 0.08, -0.26),
            Vec3::new(0.3, 0.07, -0.32),
        ]),
        // Stub axles: from the frame to the front wheels.
        vec![Vec3::new(0.2, 0.06, 0.52), Vec3::new(0.46, WHEELS[0], 0.52)],
        vec![Vec3::new(-0.2, 0.06, 0.52), Vec3::new(-0.46, WHEELS[0], 0.52)],
        // The steering column, from the floor to the wheel.
        vec![Vec3::new(0.0, 0.07, 0.56), Vec3::new(0.0, 0.47, 0.2)],
    ];
    for path in &tubes {
        kart.add(Finish::Steel, &tube(path, 0.015), Transform::IDENTITY, livery.frame);
    }
    // The rear axle, a hollow alloy bar from wheel to wheel.
    kart.add(
        Finish::Alloy,
        &tube(&[Vec3::new(-0.55, WHEELS[1], -0.52), Vec3::new(0.55, WHEELS[1], -0.52)], 0.025),
        Transform::IDENTITY,
        ALLOY,
    );
    // On the rear axle: the sprocket on the left, the brake disc in the middle.
    let across_axle = Quat::from_rotation_z(FRAC_PI_2);
    let sprocket = Shape::new(Cylinder::new(0.075, 0.008).mesh().resolution(28));
    kart.add(Finish::Alloy, &sprocket, at(0.24, WHEELS[1], -0.52).with_rotation(across_axle), ALLOY * 0.8);
    let disc = Shape::new(Cylinder::new(0.09, 0.01).mesh().resolution(28));
    kart.add(Finish::Steel, &disc, at(-0.12, WHEELS[1], -0.52).with_rotation(across_axle), Vec3::new(0.55, 0.55, 0.57));
    let caliper = Shape::block(Vec3::new(-0.15, 0.17, -0.58), Vec3::new(-0.09, 0.23, -0.5));
    kart.add(Finish::Satin, &caliper, Transform::IDENTITY, Vec3::new(0.7, 0.12, 0.1));
    // The chain guard, black plastic from the engine to the sprocket.
    let guard = Shape::extrude(
        &[Vec2::new(-0.3, 0.1), Vec2::new(-0.6, 0.08), Vec2::new(-0.62, 0.22), Vec2::new(-0.3, 0.27)],
        0.2,
        0.26,
    );
    kart.add(Finish::Satin, &guard, Transform::from_rotation(Quat::from_rotation_y(-FRAC_PI_2)), BLACK);
    // The floor tray under the driver's legs.
    kart.add(
        Finish::Satin,
        &Shape::block(Vec3::new(-0.2, 0.03, 0.0), Vec3::new(0.2, 0.04, 0.68)),
        Transform::IDENTITY,
        BLACK,
    );
    // The steering wheel, facing the driver, with its grips.
    let wheel_at = at(0.0, 0.5, 0.17).with_rotation(Quat::from_rotation_x(-1.05));
    let rim = Shape::new(
        Torus { minor_radius: 0.016, major_radius: 0.14 }.mesh().minor_resolution(8).major_resolution(24).build(),
    );
    kart.add(Finish::Satin, &rim, wheel_at, BLACK);
    let spoke = Shape::block(Vec3::new(-0.13, -0.008, -0.015), Vec3::new(0.13, 0.008, 0.015));
    kart.add(Finish::Alloy, &spoke, wheel_at, ALLOY);
}

/// The plastic bodywork: nose, a dome before the front panel, the front panel
/// with its number disc, side pods, the rear bumper; and the bucket seat.
fn bodywork(kart: &mut Sorted, livery: Livery) {
    // Side profiles (ahead, up) are drawn across the kart: the outline's X
    // becomes the kart's Z, and the extrusion its width.
    let across = Transform::from_rotation(Quat::from_rotation_y(-FRAC_PI_2));
    let profile = |points: &[(f32, f32)]| points.iter().map(|&(z, y)| Vec2::new(z, y)).collect::<Vec<_>>();
    // The nose: a moulded wedge ahead of the front wheels.
    let nose = profile(&[(0.64, 0.05), (0.97, 0.05), (1.0, 0.1), (0.95, 0.15), (0.7, 0.21), (0.62, 0.17)]);
    kart.add(Finish::Varnish, &Shape::extrude_rounded(&nose, -0.49, 0.49, 0.03, 3), across, livery.main);
    // A low dome between the nose and the front panel, over the pedals.
    let dome = at(0.0, 0.16, 0.6).with_scale(Vec3::new(0.26, 0.07, 0.17));
    kart.add(Finish::Varnish, &Shape::new(Sphere::new(1.0).mesh().uv(24, 12)), dome, livery.main);
    // The front panel, before the driver's shins, leaning back.
    let panel = at(0.0, 0.25, 0.43).with_rotation(Quat::from_rotation_x(-0.42));
    kart.add(
        Finish::Varnish,
        &Shape::block(Vec3::new(-0.2, -0.13, -0.012), Vec3::new(0.2, 0.13, 0.012)),
        panel,
        livery.main,
    );
    let disc = Shape::new(Cylinder::new(0.085, 0.004).mesh().resolution(24));
    let plate = panel * at(0.0, 0.02, 0.014) * Transform::from_rotation(Quat::from_rotation_x(FRAC_PI_2));
    kart.add(Finish::Varnish, &disc, plate, WHITE);
    for digit in numerals(livery.number, 0.08, 0.003) {
        kart.add(Finish::Varnish, &digit, panel * at(0.0, 0.02, 0.016), BLACK);
    }
    // Side pods between the wheels: a moulded section along the kart, with
    // sponsors' stickers on the outside.
    for side in [-1.0f32, 1.0] {
        let section: Vec<Vec2> = [(0.36, 0.06), (0.58, 0.05), (0.64, 0.11), (0.6, 0.2), (0.42, 0.2), (0.36, 0.14)]
            .iter()
            .map(|&(x, y)| Vec2::new(side * x, y))
            .collect();
        kart.add(
            Finish::Varnish,
            &Shape::extrude_rounded(&section, -0.3, 0.3, 0.035, 3),
            Transform::IDENTITY,
            livery.main,
        );
        let face = side * 0.625;
        let sticker = |low: Vec3, high: Vec3| Shape::block(low, high);
        kart.add(
            Finish::Varnish,
            &sticker(Vec3::new(face - 0.004, 0.1, -0.2), Vec3::new(face + 0.004, 0.16, 0.04)),
            Transform::IDENTITY,
            livery.second,
        );
        kart.add(
            Finish::Varnish,
            &sticker(Vec3::new(face - 0.004, 0.11, 0.08), Vec3::new(face + 0.004, 0.15, 0.2)),
            Transform::IDENTITY,
            Vec3::new(0.95, 0.78, 0.1),
        );
        // A number plate at the back of the pod, facing out.
        let plate_at = at(side * 0.628, 0.135, -0.19).with_rotation(Quat::from_rotation_y(side * FRAC_PI_2));
        kart.add(
            Finish::Varnish,
            &Shape::block(Vec3::new(-0.09, -0.055, 0.0), Vec3::new(0.09, 0.055, 0.003)),
            plate_at,
            WHITE,
        );
        for digit in numerals(livery.number, 0.07, 0.003) {
            kart.add(Finish::Varnish, &digit, plate_at * at(0.0, 0.0, 0.003), BLACK);
        }
        // A stripe along the top.
        let top: Vec<Vec2> = [(0.44, 0.2), (0.58, 0.2), (0.575, 0.208), (0.445, 0.208)]
            .iter()
            .map(|&(x, y)| Vec2::new(side * x, y))
            .collect();
        kart.add(Finish::Varnish, &Shape::extrude(&top, -0.28, 0.28), Transform::IDENTITY, livery.second);
    }
    // The rear bumper, wide and moulded, behind the rear wheels.
    let bumper = profile(&[(-0.74, 0.1), (-0.93, 0.1), (-0.98, 0.17), (-0.94, 0.27), (-0.78, 0.28), (-0.72, 0.2)]);
    kart.add(Finish::Varnish, &Shape::extrude_rounded(&bumper, -0.68, 0.68, 0.035, 3), across, livery.main);
    // The bucket seat: a shell around the driver's back, a base under him.
    let shell = Shape::gore(&[(0.17, 0.0), (0.19, 0.12), (0.2, 0.3), (0.18, 0.42)], 16, PI + 0.35, PI - 0.7);
    let seat = at(0.0, 0.08, -0.25).with_rotation(Quat::from_rotation_x(-0.42)).with_scale(Vec3::new(1.15, 1.0, 1.0));
    kart.add(Finish::Satin, &shell, seat, BLACK);
    kart.add(
        Finish::Satin,
        &Shape::block(Vec3::new(-0.17, 0.06, -0.4), Vec3::new(0.17, 0.09, -0.05)),
        Transform::IDENTITY,
        BLACK,
    );
}

/// The engine on the driver's right: a crankcase, a finned cylinder head,
/// a carburettor, the airbox behind the seat, and a chromed exhaust that
/// curls back to a silencer behind the bumper.
fn engine(kart: &mut Sorted) {
    let x = -0.33;
    kart.add(
        Finish::Alloy,
        &Shape::block(Vec3::new(x - 0.07, 0.08, -0.38), Vec3::new(x + 0.07, 0.2, -0.18)),
        Transform::IDENTITY,
        ALLOY * 0.6,
    );
    let cylinder = Shape::new(Cylinder::new(0.055, 0.14).mesh().resolution(16));
    kart.add(Finish::Alloy, &cylinder, at(x, 0.26, -0.28), ALLOY * 0.7);
    let fin = Shape::new(Cylinder::new(0.085, 0.006).mesh().resolution(20));
    for k in 0..7 {
        kart.add(Finish::Alloy, &fin, at(x, 0.21 + 0.018 * k as f32, -0.28), ALLOY);
    }
    let cap = Shape::new(Cylinder::new(0.012, 0.05).mesh().resolution(8));
    kart.add(Finish::Rubber, &cap, at(x, 0.36, -0.28), BLACK);
    let carb = Shape::new(Cylinder::new(0.025, 0.08).mesh().resolution(12));
    kart.add(Finish::Alloy, &carb, at(x + 0.09, 0.2, -0.28).with_rotation(Quat::from_rotation_z(FRAC_PI_2)), ALLOY);
    // The airbox: a black drum across, behind the seat.
    let airbox =
        at(0.0, 0.3, -0.6).with_rotation(Quat::from_rotation_z(FRAC_PI_2)).with_scale(Vec3::new(1.0, 1.0, 0.8));
    kart.add(Finish::Satin, &capsule(0.075, 0.32), airbox, BLACK);
    // The exhaust.
    let pipe = [
        Vec3::new(x, 0.3, -0.24),
        Vec3::new(x - 0.08, 0.31, -0.32),
        Vec3::new(x - 0.11, 0.3, -0.45),
        Vec3::new(x - 0.11, 0.3, -0.62),
    ];
    kart.add(Finish::Chrome, &tube(&pipe, 0.022), Transform::IDENTITY, CHROME);
    let silencer = at(x - 0.11, 0.3, -0.8).with_rotation(Quat::from_rotation_x(FRAC_PI_2));
    kart.add(Finish::Chrome, &capsule(0.045, 0.22), silencer, CHROME);
}

/// The driver, sitting low with legs ahead, hands on the wheel.
fn driver(kart: &mut Sorted, livery: Livery) {
    let suit = livery.main * 0.8 + Vec3::splat(0.06);
    let panels = livery.second;
    // The torso, leaning back into the seat.
    let torso = at(0.0, 0.42, -0.31).with_rotation(Quat::from_rotation_x(-0.38)).with_scale(Vec3::new(1.1, 1.0, 0.72));
    kart.add(Finish::Cloth, &capsule(0.15, 0.26), torso, suit);
    // Shoulders, broader than the waist.
    for side in [-1.0f32, 1.0] {
        let shoulder = at(side * 0.19, 0.6, -0.27).with_scale(Vec3::new(1.0, 0.85, 1.0));
        kart.add(Finish::Cloth, &Shape::new(Sphere::new(0.075).mesh().uv(14, 8)), shoulder, suit);
    }
    // A rib protector over the suit, black, at the waist.
    let ribs = at(0.0, 0.33, -0.31).with_rotation(Quat::from_rotation_x(-0.38)).with_scale(Vec3::new(1.14, 0.42, 0.76));
    kart.add(Finish::Satin, &capsule(0.152, 0.2), ribs, BLACK);
    // A band of the second colour across the chest.
    let band = at(0.0, 0.5, -0.27).with_rotation(Quat::from_rotation_x(-0.38)).with_scale(Vec3::new(1.12, 0.25, 0.75));
    kart.add(Finish::Cloth, &capsule(0.152, 0.0), band, panels);
    // The collar, in the second colour, as race suits have it.
    kart.add(Finish::Cloth, &capsule(0.08, 0.0), at(0.0, 0.69, -0.28).with_scale(Vec3::new(1.0, 0.6, 1.0)), panels);
    let leaning = Quat::from_rotation_x(-0.38);
    // Side panels down the torso.
    for side in [-1.0f32, 1.0] {
        let panel = at(side * 0.155, 0.42, -0.31).with_rotation(leaning).with_scale(Vec3::new(0.18, 1.0, 0.6));
        kart.add(Finish::Cloth, &capsule(0.1, 0.24), panel, panels);
    }
    // A patch on the back, where the chasing camera sees it: white, with a
    // dark mark standing for a sponsor.
    let back = at(0.0, 0.49, -0.42).with_rotation(leaning);
    kart.add(Finish::Cloth, &Shape::block(Vec3::new(-0.09, -0.06, -0.01), Vec3::new(0.09, 0.06, 0.0)), back, WHITE);
    kart.add(
        Finish::Cloth,
        &Shape::block(Vec3::new(-0.06, -0.015, -0.014), Vec3::new(0.06, 0.015, -0.01)),
        back,
        BLACK,
    );
    // Hips.
    kart.add(
        Finish::Cloth,
        &capsule(0.13, 0.14),
        at(0.0, 0.17, -0.2).with_rotation(Quat::from_rotation_z(FRAC_PI_2)),
        suit * 0.85,
    );
    for side in [-1.0f32, 1.0] {
        // Arms: shoulder, elbow out and down, hands at ten to two.
        let arm = [
            Vec3::new(side * 0.19, 0.6, -0.26),
            Vec3::new(side * 0.25, 0.47, -0.08),
            Vec3::new(side * 0.13, 0.5, 0.12),
        ];
        kart.add(Finish::Cloth, &tube(&arm, 0.042), Transform::IDENTITY, suit);
        kart.add(Finish::Cloth, &tube(&arm[..2], 0.044), at(0.0, 0.0, 0.0).with_scale(Vec3::ONE), panels);
        let glove = at(side * 0.12, 0.51, 0.14).with_scale(Vec3::new(0.85, 1.0, 1.15));
        kart.add(Finish::Rubber, &Shape::new(Sphere::new(0.04).mesh().uv(12, 8)), glove, BLACK);
        // Legs: thigh up to the knee, shin down to the pedals.
        let thigh = [Vec3::new(side * 0.1, 0.15, -0.16), Vec3::new(side * 0.13, 0.27, 0.12)];
        let shin = [Vec3::new(side * 0.13, 0.27, 0.12), Vec3::new(side * 0.11, 0.13, 0.5)];
        kart.add(Finish::Cloth, &tube(&thigh, 0.07), Transform::IDENTITY, suit);
        // A knee panel in the second colour.
        kart.add(Finish::Cloth, &Shape::new(Sphere::new(0.062).mesh().uv(12, 8)), at(side * 0.13, 0.28, 0.12), panels);
        kart.add(Finish::Cloth, &tube(&shin, 0.052), Transform::IDENTITY, suit);
        let boot = at(side * 0.11, 0.12, 0.56).with_rotation(Quat::from_rotation_x(FRAC_PI_2 - 0.4));
        kart.add(Finish::Rubber, &capsule(0.045, 0.1), boot, BLACK);
    }
}

/// The helmet, around the neck: a varnished shell in the team's colour with
/// a stripe of the second, and a dark visor across the front.
fn helmet(livery: Livery) -> Part {
    let mut part = Sorted::default();
    let profile =
        [(0.0, 0.0), (0.1, 0.01), (0.135, 0.05), (0.15, 0.12), (0.148, 0.19), (0.13, 0.25), (0.09, 0.29), (0.0, 0.305)];
    let shell = Transform::from_scale(Vec3::new(1.0, 1.0, 1.12));
    // A shade deeper than the bodywork: a varnished dome reflects the sky and pales.
    part.add(Finish::Varnish, &Shape::lathe(&profile, 28), shell, livery.main * 0.8);
    // A stripe over the top, front to back.
    let stripe = Shape::gore(&profile[3..], 4, FRAC_PI_2 - 0.2, 0.4);
    part.add(Finish::Varnish, &stripe, shell * Transform::from_scale(Vec3::splat(1.01)), livery.second);
    part.add(
        Finish::Varnish,
        &Shape::gore(&profile[3..], 4, -FRAC_PI_2 - 0.2, 0.4),
        shell * Transform::from_scale(Vec3::splat(1.01)),
        livery.second,
    );
    // The visor: a band across the front, standing proud of the shell.
    let visor = Shape::gore(&[(0.152, 0.1), (0.157, 0.145), (0.152, 0.19)], 14, FRAC_PI_2 - 1.05, 2.1);
    part.add(Finish::Visor, &visor, shell * Transform::from_scale(Vec3::splat(1.02)), Vec3::splat(0.02));
    part.part()
}

/// A wheel for the left side: a slick tyre, an alloy rim with its bolts.
/// `axle` 0 is a front wheel, 1 a rear one.
fn wheel(axle: usize, livery: Livery) -> Part {
    let mut part = Sorted::default();
    let (radius, width) = (WHEELS[axle], TREADS[axle]);
    let rim = radius * 0.62;
    let (half, shoulder) = (width / 2.0, 0.018);
    // Profile of the tyre from its inner wall, round the tread, to the outer wall.
    let tyre = [
        (rim, -half + 0.004),
        (radius - shoulder, -half),
        (radius - 0.004, -half + 0.008),
        (radius, -half + shoulder),
        (radius, half - shoulder),
        (radius - 0.004, half - 0.008),
        (radius - shoulder, half),
        (rim, half - 0.004),
    ];
    // The axle is X: Y of the lathe turned onto X.
    let onto_x = Transform::from_rotation(Quat::from_rotation_z(-FRAC_PI_2));
    part.add(Finish::Rubber, &Shape::lathe(&tyre, 36), onto_x, RUBBER);
    let face = [
        (0.0, half - 0.012),
        (rim * 0.5, half - 0.006),
        (rim, half - 0.012),
        (rim, -half + 0.01),
        (0.0, -half + 0.012),
    ];
    // Lathed from the outer face inwards: the outside faces out.
    let face: Vec<(f32, f32)> = face.iter().rev().copied().collect();
    let rims = livery.frame.lerp(ALLOY, 0.7);
    part.add(Finish::Alloy, &Shape::lathe(&face, 28), onto_x, rims);
    let bolt = Shape::new(Cylinder::new(0.008, 0.012).mesh().resolution(8));
    for k in 0..3 {
        let angle = k as f32 / 3.0 * TAU;
        let place = Transform::from_xyz(half - 0.004, angle.sin() * 0.03, angle.cos() * 0.03)
            .with_rotation(Quat::from_rotation_z(FRAC_PI_2));
        part.add(Finish::Alloy, &bolt, place, CHROME);
    }
    part.part()
}

#[cfg(test)]
mod tests {
    use bevy::mesh::VertexAttributeValues;

    use super::*;

    fn livery() -> Livery {
        Livery { main: Vec3::new(0.9, 0.2, 0.15), second: WHITE, frame: Vec3::new(0.8, 0.8, 0.82), number: 47 }
    }

    fn bounds(part: &Part) -> (Vec3, Vec3) {
        let mut low = Vec3::MAX;
        let mut high = Vec3::MIN;
        for (_, mesh) in part {
            if let Some(VertexAttributeValues::Float32x3(points)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
                for p in points {
                    low = low.min(Vec3::from(*p));
                    high = high.max(Vec3::from(*p));
                }
            }
        }
        (low, high)
    }

    #[test]
    fn a_kart_has_the_size_of_a_real_one() {
        let kart = build(livery());
        let (low, high) = bounds(&kart.body);
        let size = high - low;
        // About 1.9 m long and at most 1.4 m wide, as the rules have it; the frame a few centimetres off the road.
        assert!((1.7..2.0).contains(&size.z), "length {}", size.z);
        assert!((1.0..1.42).contains(&size.x), "width without the wheels {}", size.x);
        assert!(low.y > 0.0 && low.y < 0.04, "the lowest point is {} m off the road", low.y);
        // With the helmet, the driver's head about a metre high.
        let (_, top) = bounds(&kart.helmet);
        assert!((0.95..1.15).contains(&(NECK.y + top.y)), "helmet top {}", NECK.y + top.y);
    }

    #[test]
    fn wheels_stand_on_the_road_and_fit_their_axles() {
        let kart = build(livery());
        for (axle, wheel) in kart.wheels.iter().enumerate() {
            let (low, high) = bounds(wheel);
            assert!((high.y - WHEELS[axle]).abs() < 0.002 && (low.y + WHEELS[axle]).abs() < 0.002, "radius");
            assert!((high.x - low.x - TREADS[axle]).abs() < 0.02, "width {}", high.x - low.x);
            assert!(wheel.iter().any(|(f, _)| *f == Finish::Rubber) && wheel.iter().any(|(f, _)| *f == Finish::Alloy));
        }
        for axle in AXLES {
            assert!((axle.y - WHEELS[if axle.z > 0.0 { 0 } else { 1 }]).abs() < 1e-6, "a wheel touches the road");
        }
    }

    #[test]
    fn numbers_light_the_segments_of_their_digits() {
        // 8 lights all seven, 1 two; 18 is both, side by side.
        assert_eq!(numerals(8, 0.1, 0.01).len(), 7);
        assert_eq!(numerals(1, 0.1, 0.01).len(), 2);
        assert_eq!(numerals(18, 0.1, 0.01).len(), 9);
        // The digits' cells are centred: 88 reaches as far left as right.
        let mesh = |shapes: Vec<Shape>| {
            let mut batch = Batch::default();
            for shape in &shapes {
                batch.add(shape, Transform::IDENTITY, Paint::plain(0.0, 0.0, 0.0));
            }
            batch.merged()
        };
        let (low, high) = bounds(&vec![(Finish::Varnish, mesh(numerals(88, 0.1, 0.01)))]);
        assert!((low.x + high.x).abs() < 1e-4 && (low.y + high.y).abs() < 0.02, "{low} {high}");
    }

    #[test]
    fn every_finish_has_its_own_material_and_the_body_uses_most() {
        let kart = build(livery());
        let used: std::collections::BTreeSet<Finish> = kart.body.iter().map(|(f, _)| *f).collect();
        assert!(used.len() >= 6, "{used:?}");
        assert!(kart.helmet.iter().any(|(f, _)| *f == Finish::Visor));
        assert!(Finish::Chrome.material().metallic > 0.9 && Finish::Chrome.material().perceptual_roughness < 0.15);
        assert!(Finish::Cloth.material().perceptual_roughness > 0.8);
        assert!(Finish::Varnish.material().perceptual_roughness < Finish::Satin.material().perceptual_roughness);
    }
}
