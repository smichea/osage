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

pub mod decals;

use bevy::prelude::*;
use osage_procedural::{Batch, Paint, Sculpture, Shape};

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
pub const NECK: Vec3 = Vec3::new(0.0, 0.705, -0.27);

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
    /// Sponsors' stickers: glossy, their marks from [`decals::atlas`],
    /// which the client gives this finish's material.
    Decal,
}

impl Finish {
    pub const ALL: [Finish; 9] = [
        Finish::Varnish,
        Finish::Satin,
        Finish::Rubber,
        Finish::Steel,
        Finish::Alloy,
        Finish::Chrome,
        Finish::Cloth,
        Finish::Visor,
        Finish::Decal,
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
            Finish::Decal => StandardMaterial {
                perceptual_roughness: 0.25,
                reflectance: 0.6,
                alpha_mode: AlphaMode::Mask(0.5),
                ..base
            },
        }
    }
}

/// A team's colours, in sRGB: the bodywork and helmet, a second colour for
/// stripes, the frame's paint, and the race suit's own colour — rarely the
/// kart's: a suit in the kart's colour from head to foot reads as a toy.
#[derive(Clone, Copy, Debug)]
pub struct Livery {
    pub main: Vec3,
    pub second: Vec3,
    pub frame: Vec3,
    pub suit: Vec3,
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
/// plane, standing `proud` in front of it (+Z). Also for a world's signs.
pub fn numerals(number: u32, height: f32, proud: f32) -> Vec<Shape> {
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
    let mut part = kart.part();
    part.push((Finish::Decal, stickers(livery).mesh()));
    part
}

/// Sponsors' stickers on the bodywork, a set of marks for each racing
/// number: along both side pods, on the nose, beside the front panel's
/// number, along the rear bumper.
fn stickers(livery: Livery) -> decals::Decals {
    let mut decals = decals::Decals::default();
    let mut k = livery.number as usize * 5;
    let mut next = || {
        k += 7;
        k
    };
    for side in [-1.0f32, 1.0] {
        // The pod's flat outer face, a hair proud of it, read from outside;
        // ahead of the number plate.
        let along = Vec3::Z * -side;
        let on_face = |z: f32| Vec3::new(side * 0.624, 0.125, z);
        decals.add(next(), on_face(0.02), along, Vec3::Y, 0.15);
        decals.add(next(), on_face(0.2), along, Vec3::Y, 0.11);
        // The nose's flat top either side of its crown, read from ahead.
        decals.add(next(), Vec3::new(side * 0.33, 0.157, 0.86), Vec3::X, Vec3::NEG_Z, 0.2);
    }
    // Beside the front panel's number, facing ahead.
    let panel = at(0.0, 0.25, 0.43).with_rotation(Quat::from_rotation_x(-0.42));
    for side in [-1.0f32, 1.0] {
        let centre = panel.transform_point(Vec3::new(side * 0.145, -0.08, 0.0135));
        decals.add(next(), centre, panel.rotation * Vec3::X, panel.rotation * Vec3::Y, 0.1);
    }
    // A sponsor's mark on the white patch of the driver's back, which the
    // chasing camera sees: the trunk leans back, so the back faces a
    // little down.
    let lean = Quat::from_rotation_x(-0.38);
    let back = Vec3::new(0.0, 0.5, -0.33) + lean * Vec3::NEG_Z * 0.114 + lean * Vec3::NEG_Y * 0.04;
    decals.add(next(), back, -Vec3::X, lean * Vec3::Y, 0.15);
    // Along the rear bumper, facing back.
    decals.add(next(), Vec3::new(-0.26, 0.17, -0.962), -Vec3::X, Vec3::Y, 0.16);
    decals.add(next(), Vec3::new(0.26, 0.17, -0.962), -Vec3::X, Vec3::Y, 0.16);
    decals
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
/// The groups of the moulded bodywork: what colour each part takes.
mod moulding {
    pub const PAINT: usize = 0;
    pub const BLACK: usize = 1;
    pub const STRIPE: usize = 2;
}

/// The moulded plastic bodywork, the same for every kart, sculpted once:
/// rounded boxes and ellipsoids blended as a mould gives them ([`Sculpture`]).
/// The nose with its black lip and the dome over the pedals; the side pods,
/// a skirt below, rounded ahead, a stripe along the top; the rear bumper, a
/// bar and two wings behind the wheels, black, the team's colour on top.
fn moulded() -> &'static [(usize, Shape)] {
    static MOULDED: std::sync::OnceLock<Vec<(usize, Shape)>> = std::sync::OnceLock::new();
    MOULDED.get_or_init(|| {
        use moulding::*;
        let flat = Quat::IDENTITY;
        let mut parts = Vec::new();
        let mut nose = Sculpture::new(0.05);
        nose.rounded(PAINT, Vec3::new(0.0, 0.1, 0.84), Vec3::new(0.46, 0.055, 0.15), 0.04, flat)
            .ellipsoid(PAINT, Vec3::new(0.0, 0.15, 0.78), Vec3::new(0.3, 0.065, 0.15), flat)
            .ellipsoid(PAINT, Vec3::new(0.0, 0.16, 0.62), Vec3::new(0.24, 0.07, 0.15), flat)
            .rounded(BLACK, Vec3::new(0.0, 0.065, 0.975), Vec3::new(0.42, 0.025, 0.035), 0.02, flat);
        parts.extend(nose.shapes(0.012));
        for side in [-1.0f32, 1.0] {
            let mut pod = Sculpture::new(0.04);
            pod.rounded(PAINT, Vec3::new(side * 0.5, 0.125, 0.0), Vec3::new(0.12, 0.07, 0.31), 0.045, flat)
                .rounded(PAINT, Vec3::new(side * 0.51, 0.065, 0.0), Vec3::new(0.13, 0.025, 0.33), 0.02, flat)
                .ellipsoid(PAINT, Vec3::new(side * 0.48, 0.12, 0.3), Vec3::new(0.1, 0.06, 0.08), flat)
                .paint_box(STRIPE, Vec3::new(side * 0.5, 0.195, 0.0), Vec3::new(0.07, 0.012, 0.29), flat);
            parts.extend(pod.shapes(0.01));
        }
        let mut bumper = Sculpture::new(0.05);
        bumper.rounded(BLACK, Vec3::new(0.0, 0.17, -0.86), Vec3::new(0.4, 0.075, 0.1), 0.04, flat);
        for side in [-1.0f32, 1.0] {
            bumper.rounded(BLACK, Vec3::new(side * 0.53, 0.19, -0.84), Vec3::new(0.16, 0.09, 0.13), 0.05, flat);
        }
        bumper.paint_box(PAINT, Vec3::new(0.0, 0.245, -0.86), Vec3::new(0.36, 0.014, 0.11), flat);
        parts.extend(bumper.shapes(0.012));
        parts
    })
}

/// The plastic bodywork: moulded nose, side pods and rear bumper in the
/// livery's colours, the front panel with its number disc, the number
/// plates; and the bucket seat.
fn bodywork(kart: &mut Sorted, livery: Livery) {
    for (group, shape) in moulded() {
        let (finish, colour) = match *group {
            moulding::PAINT => (Finish::Varnish, livery.main),
            moulding::STRIPE => (Finish::Varnish, livery.second),
            _ => (Finish::Satin, BLACK),
        };
        kart.add(finish, shape, Transform::IDENTITY, colour);
    }
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
    for side in [-1.0f32, 1.0] {
        // A number plate at the back of each pod's flat outer face.
        let plate_at = at(side * 0.622, 0.125, -0.19).with_rotation(Quat::from_rotation_y(side * FRAC_PI_2));
        kart.add(
            Finish::Varnish,
            &Shape::block(Vec3::new(-0.09, -0.045, 0.0), Vec3::new(0.09, 0.045, 0.003)),
            plate_at,
            WHITE,
        );
        for digit in numerals(livery.number, 0.065, 0.003) {
            kart.add(Finish::Varnish, &digit, plate_at * at(0.0, 0.0, 0.003), BLACK);
        }
    }
    // The rear bumper's number plate, on its flat rear face, as the rules want it.
    let plate_at = at(0.0, 0.17, -0.962).with_rotation(Quat::from_rotation_y(PI));
    kart.add(Finish::Varnish, &Shape::block(Vec3::new(-0.1, -0.04, 0.0), Vec3::new(0.1, 0.04, 0.003)), plate_at, WHITE);
    for digit in numerals(livery.number, 0.06, 0.003) {
        kart.add(Finish::Varnish, &digit, plate_at * at(0.0, 0.0, 0.003), BLACK);
    }
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

/// The groups of the driver's sculpture: what each part of the surface is.
mod worn {
    pub const SUIT: usize = 0;
    pub const PANELS: usize = 1;
    pub const BAND: usize = 2;
    pub const GLOVES: usize = 3;
    pub const BOOTS: usize = 4;
    pub const RIBS: usize = 5;
    pub const PATCH: usize = 6;
}

/// The driver's body, sitting low with legs ahead, hands on the wheel:
/// limbs and trunk blended into one surface ([`Sculpture`]), cloth folded
/// at the elbows and knees, and the suit's panels painted on. The same for
/// every driver, so sculpted once; each livery only paints it.
fn sculpted_driver() -> &'static [(usize, Shape)] {
    static DRIVER: std::sync::OnceLock<Vec<(usize, Shape)>> = std::sync::OnceLock::new();
    DRIVER.get_or_init(|| {
        use worn::*;
        let lean = Quat::from_rotation_x(-0.38);
        let mut body = Sculpture::new(0.045);
        // The trunk, leaning back into the seat: pelvis, belly, chest.
        body.ellipsoid(SUIT, Vec3::new(0.0, 0.17, -0.2), Vec3::new(0.15, 0.1, 0.13), Quat::IDENTITY)
            .ellipsoid(SUIT, Vec3::new(0.0, 0.33, -0.28), Vec3::new(0.145, 0.13, 0.1), lean)
            .ellipsoid(SUIT, Vec3::new(0.0, 0.5, -0.33), Vec3::new(0.17, 0.15, 0.11), lean)
            // The rib protector, hard and black over the suit at the waist.
            .ellipsoid(RIBS, Vec3::new(0.0, 0.33, -0.29), Vec3::new(0.162, 0.085, 0.115), lean)
            // The neck.
            .cone(SUIT, Vec3::new(0.0, 0.6, -0.32), NECK, 0.055, 0.05);
        for side in [-1.0f32, 1.0] {
            let shoulder = Vec3::new(side * 0.19, 0.6, -0.27);
            let elbow = Vec3::new(side * 0.25, 0.47, -0.08);
            let wrist = Vec3::new(side * 0.135, 0.5, 0.1);
            let knee = Vec3::new(side * 0.13, 0.27, 0.12);
            let ankle = Vec3::new(side * 0.11, 0.14, 0.48);
            body.cone(SUIT, Vec3::new(0.0, 0.63, -0.31), shoulder, 0.06, 0.068)
                .cone(SUIT, shoulder, elbow, 0.056, 0.046)
                .folded(0.0025)
                .cone(SUIT, elbow, wrist, 0.045, 0.034)
                .folded(0.0025)
                .cone(GLOVES, wrist + Vec3::new(0.0, 0.0, -0.01), Vec3::new(side * 0.122, 0.51, 0.135), 0.037, 0.035)
                .ellipsoid(GLOVES, Vec3::new(side * 0.12, 0.51, 0.15), Vec3::new(0.034, 0.042, 0.05), Quat::IDENTITY)
                .cone(SUIT, Vec3::new(side * 0.1, 0.15, -0.16), knee, 0.082, 0.058)
                .folded(0.003)
                .cone(SUIT, knee, ankle, 0.056, 0.04)
                .folded(0.0025)
                .cone(BOOTS, ankle + Vec3::new(0.0, -0.01, 0.0), Vec3::new(side * 0.11, 0.12, 0.6), 0.045, 0.038);
            // Painted on: panels down the sides and along the upper arms,
            // over the knees, in the team's colour.
            body.paint_box(PANELS, Vec3::new(side * 0.17, 0.42, -0.3), Vec3::new(0.045, 0.22, 0.16), lean)
                .paint_cone(PANELS, shoulder, elbow, 0.075, 0.062)
                .paint_cone(PANELS, knee, knee, 0.07, 0.07);
        }
        // A band of the second colour across the chest and shoulders, the
        // collar in the team's colour, and a white patch on the back, where
        // the chasing camera sees it, for a sponsor's sticker ([`stickers`]).
        body.paint_box(BAND, Vec3::new(0.0, 0.53, -0.31), Vec3::new(0.3, 0.035, 0.2), lean)
            .paint_cone(PANELS, Vec3::new(0.0, 0.66, -0.3), Vec3::new(0.0, 0.7, -0.29), 0.075, 0.07)
            .paint_box(PATCH, Vec3::new(0.0, 0.46, -0.44), Vec3::new(0.09, 0.06, 0.08), lean);
        body.shapes(0.012)
    })
}

/// The driver, sculpted, in the livery's colours.
fn driver(kart: &mut Sorted, livery: Livery) {
    for (group, shape) in sculpted_driver() {
        let (finish, colour) = match *group {
            worn::SUIT => (Finish::Cloth, livery.suit),
            worn::PANELS => (Finish::Cloth, livery.main),
            worn::BAND => (Finish::Cloth, livery.second),
            worn::GLOVES | worn::BOOTS => (Finish::Rubber, BLACK),
            worn::RIBS => (Finish::Satin, BLACK),
            worn::PATCH => (Finish::Cloth, WHITE),
            _ => (Finish::Cloth, BLACK),
        };
        kart.add(finish, shape, Transform::IDENTITY, colour);
    }
    // The foam neck brace karting drivers wear, tucked under the helmet's
    // rim: seen from behind, a thin dark band, not a ring.
    let brace = Shape::new(
        Torus { minor_radius: 0.026, major_radius: 0.088 }.mesh().minor_resolution(10).major_resolution(24).build(),
    );
    kart.add(
        Finish::Satin,
        &brace,
        at(0.0, NECK.y - 0.032, -0.275).with_scale(Vec3::new(1.12, 0.75, 1.05)),
        Vec3::splat(0.16),
    );
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
    // Its lower half in the second colour, as helmet painters split them,
    // with a black rubber trim round the opening.
    let lower = Shape::gore(&profile[..4], 28, 0.0, TAU);
    part.add(Finish::Varnish, &lower, shell * Transform::from_scale(Vec3::splat(1.006)), livery.second);
    let trim = Shape::new(
        Torus { minor_radius: 0.012, major_radius: 0.105 }.mesh().minor_resolution(8).major_resolution(28).build(),
    );
    part.add(Finish::Rubber, &trim, shell * at(0.0, 0.012, 0.0), BLACK);
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
        Livery {
            main: Vec3::new(0.9, 0.2, 0.15),
            second: WHITE,
            frame: Vec3::new(0.8, 0.8, 0.82),
            suit: Vec3::splat(0.15),
            number: 47,
        }
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
