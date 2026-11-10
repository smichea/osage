// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The karts: a toy kart with its driver, built from rounded shapes, and the
//! motion that makes it feel alive — wheels that spin and steer, a body that
//! leans in bends, lifts its nose when it accelerates, hops into a drift.
//!
//! A kart faces +Z, stands on the ground at its origin, and has its left
//! at +X.

use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::prelude::*;
use osage_kart_physics::{MAX_SPEED, wrap};

use crate::scene::{PALETTE, Pose};

/// Radius of the front and of the rear wheels, metres.
const WHEELS: [f32; 2] = [0.3, 0.36];
/// Where the wheels are: front left, front right, rear left, rear right.
const AXLES: [Vec3; 4] = [
    Vec3::new(0.66, WHEELS[0], 0.74),
    Vec3::new(-0.66, WHEELS[0], 0.74),
    Vec3::new(0.7, WHEELS[1], -0.62),
    Vec3::new(-0.7, WHEELS[1], -0.62),
];
/// Where the exhaust pipes end.
pub const EXHAUSTS: [Vec3; 2] = [Vec3::new(0.2, 0.72, -1.28), Vec3::new(-0.2, 0.72, -1.28)];
/// Where the rear tyres touch the road.
pub const REAR_CONTACTS: [Vec3; 2] = [Vec3::new(0.7, 0.05, -0.75), Vec3::new(-0.7, 0.05, -0.75)];
/// A hop into a drift lasts this long, seconds.
const HOP: f32 = 0.28;

/// Meshes and materials the karts share.
#[derive(Resource)]
pub struct Kit {
    ball: Handle<Mesh>,
    plate: Handle<Mesh>,
    block: Handle<Mesh>,
    tube: Handle<Mesh>,
    pill: Handle<Mesh>,
    tyres: [Handle<Mesh>; 2],
    hub: Handle<Mesh>,
    spoke: Handle<Mesh>,
    ring: Handle<Mesh>,
    paints: Vec<Handle<StandardMaterial>>,
    suits: Vec<Handle<StandardMaterial>>,
    white: Handle<StandardMaterial>,
    dark: Handle<StandardMaterial>,
    rubber: Handle<StandardMaterial>,
    steel: Handle<StandardMaterial>,
    visor: Handle<StandardMaterial>,
}

/// The moving parts of a kart and where its motion stands.
#[derive(Component)]
pub struct Rig {
    body: Entity,
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

pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut colour = |r: f32, g: f32, b: f32, perceptual_roughness: f32, metallic: f32| {
        materials.add(StandardMaterial {
            base_color: Color::srgb(r, g, b),
            perceptual_roughness,
            metallic,
            ..default()
        })
    };
    let paints = PALETTE.iter().map(|&(r, g, b)| colour(r, g, b, 0.32, 0.15)).collect();
    let suits =
        PALETTE.iter().map(|&(r, g, b)| colour(r * 0.45 + 0.5, g * 0.45 + 0.5, b * 0.45 + 0.5, 0.8, 0.0)).collect();
    let kit = Kit {
        paints,
        suits,
        white: colour(0.95, 0.95, 0.95, 0.4, 0.0),
        dark: colour(0.05, 0.055, 0.07, 0.55, 0.0),
        rubber: colour(0.025, 0.025, 0.028, 0.85, 0.0),
        steel: colour(0.78, 0.8, 0.84, 0.25, 0.9),
        visor: colour(0.02, 0.03, 0.06, 0.08, 0.0),
        ball: meshes.add(Sphere::new(1.0).mesh().ico(3).expect("a small icosphere")),
        plate: meshes.add(Cuboid::new(1.0, 0.1, 1.9)),
        block: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        tube: meshes.add(Cylinder::new(1.0, 1.0)),
        pill: meshes.add(Capsule3d::new(1.0, 1.0)),
        tyres: [0.3, 0.36].map(|radius: f32| {
            meshes.add(
                Torus { minor_radius: radius * 0.4, major_radius: radius * 0.6 }
                    .mesh()
                    .minor_resolution(12)
                    .major_resolution(24),
            )
        }),
        hub: meshes.add(Cylinder::new(1.0, 1.0)),
        spoke: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        ring: meshes.add(Torus { minor_radius: 0.03, major_radius: 0.15 }),
    };
    commands.insert_resource(kit);
}

/// A rotation that lays a shape whose long axis is Y along `direction`.
fn along(direction: Vec3) -> Quat {
    Quat::from_rotation_arc(Vec3::Y, direction.normalize_or(Vec3::Y))
}

/// Gives every new kart its model.
pub fn dress(mut commands: Commands, kit: Res<Kit>, karts: Query<(Entity, &Pose), Added<Pose>>) {
    for (kart, pose) in &karts {
        let colour = pose.color as usize % kit.paints.len();
        let (paint, suit) = (kit.paints[colour].clone(), kit.suits[colour].clone());
        let piece = |mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>, at: Vec3, turn: Quat, scale: Vec3| {
            (
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                Transform { translation: at, rotation: turn, scale },
            )
        };
        let upright = Quat::IDENTITY;
        let (sideways, lengthways) = (along(Vec3::X), along(Vec3::Z));

        let body = commands.spawn((Transform::default(), Visibility::default(), ChildOf(kart))).id();
        commands.entity(body).with_children(|body| {
            // The car: a floor, a rounded shell with its nose, pods and bumpers.
            body.spawn(piece(&kit.plate, &kit.dark, Vec3::new(0.0, 0.26, 0.0), upright, Vec3::ONE));
            body.spawn(piece(&kit.ball, &paint, Vec3::new(0.0, 0.46, 0.05), upright, Vec3::new(0.52, 0.3, 1.0)));
            body.spawn(piece(&kit.ball, &paint, Vec3::new(0.0, 0.4, 0.88), upright, Vec3::new(0.36, 0.2, 0.46)));
            for side in [-1.0, 1.0] {
                let pod = Vec3::new(side * 0.57, 0.36, -0.02);
                body.spawn(piece(&kit.pill, &kit.white, pod, lengthways, Vec3::new(0.16, 0.5, 0.16)));
            }
            for (z, length) in [(1.16, 0.4), (-1.1, 0.42)] {
                let bumper = Vec3::new(0.0, 0.3, z);
                body.spawn(piece(&kit.pill, &kit.white, bumper, sideways, Vec3::new(0.09, length, 0.09)));
            }
            // The engine, its pipes, the wing on its struts.
            body.spawn(piece(&kit.block, &kit.dark, Vec3::new(0.0, 0.62, -0.74), upright, Vec3::new(0.62, 0.34, 0.46)));
            for exhaust in EXHAUSTS {
                let pipe = exhaust + Vec3::Z * 0.26;
                body.spawn(piece(&kit.tube, &kit.steel, pipe, lengthways, Vec3::new(0.08, 0.52, 0.08)));
            }
            body.spawn(piece(&kit.block, &paint, Vec3::new(0.0, 1.12, -0.98), upright, Vec3::new(1.24, 0.07, 0.36)));
            for side in [-0.4, 0.4] {
                let strut = Vec3::new(side, 0.9, -0.98);
                body.spawn(piece(&kit.block, &kit.dark, strut, upright, Vec3::new(0.06, 0.42, 0.14)));
            }
            // The driver: seat, body, helmet and visor, arms on the wheel.
            body.spawn(piece(&kit.block, &kit.dark, Vec3::new(0.0, 0.8, -0.44), upright, Vec3::new(0.52, 0.56, 0.12)));
            body.spawn(piece(&kit.pill, &suit, Vec3::new(0.0, 0.88, -0.2), upright, Vec3::new(0.22, 0.14, 0.2)));
            body.spawn(piece(&kit.ball, &paint, Vec3::new(0.0, 1.34, -0.17), upright, Vec3::splat(0.26)));
            body.spawn(piece(&kit.ball, &kit.visor, Vec3::new(0.0, 1.35, 0.01), upright, Vec3::new(0.2, 0.11, 0.13)));
            let wheel = Vec3::new(0.0, 0.92, 0.3);
            body.spawn(piece(&kit.ring, &kit.dark, wheel, Quat::from_rotation_x(1.1), Vec3::ONE));
            for side in [-1.0, 1.0] {
                let (shoulder, hand) = (Vec3::new(side * 0.21, 1.02, -0.14), wheel + Vec3::X * side * 0.15);
                // (A pill of scale s is 3 s long.)
                let arm = Vec3::new(0.065, (hand - shoulder).length() / 3.0, 0.065);
                body.spawn(piece(&kit.pill, &suit, (shoulder + hand) / 2.0, along(hand - shoulder), arm));
            }
        });

        // Wheels: a pivot that steers, a tyre that spins, a hub with a bar
        // across it so that the spin shows.
        let wheels = [0, 1, 2, 3].map(|i| {
            let radius = WHEELS[i / 2];
            let pivot =
                commands.spawn((Transform::from_translation(AXLES[i]), Visibility::default(), ChildOf(body))).id();
            let tyre = piece(&kit.tyres[i / 2], &kit.rubber, Vec3::ZERO, Quat::from_rotation_z(FRAC_PI_2), Vec3::ONE);
            let tyre = commands.spawn((tyre, ChildOf(pivot))).id();
            commands.entity(tyre).with_children(|tyre| {
                let hub = Vec3::new(radius * 0.52, radius * 0.72, radius * 0.52);
                tyre.spawn(piece(&kit.hub, &kit.white, Vec3::ZERO, upright, hub));
                let bar = Vec3::new(radius * 0.95, radius * 0.76, radius * 0.16);
                tyre.spawn(piece(&kit.spoke, &kit.steel, Vec3::ZERO, upright, bar));
            });
            (pivot, tyre)
        });

        commands.entity(kart).insert((
            Transform::default(),
            Visibility::default(),
            Rig {
                body,
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
        let hop = 0.22 * (std::f32::consts::PI * rig.hop / HOP).sin();

        if let Ok(mut place) = transforms.get_mut(entity) {
            *place = Transform::from_xyz(kart.x, hop, kart.z).with_rotation(Quat::from_rotation_y(kart.heading));
        }
        if let Ok(mut body) = transforms.get_mut(rig.body) {
            body.rotation =
                Quat::from_rotation_y(rig.slip) * Quat::from_rotation_z(rig.lean) * Quat::from_rotation_x(rig.pitch);
        }
        for (i, (pivot, tyre)) in rig.wheels.into_iter().enumerate() {
            if let (true, Ok(mut pivot)) = (i < 2, transforms.get_mut(pivot)) {
                pivot.rotation = Quat::from_rotation_y(rig.steer * 0.55);
            }
            if let Ok(mut tyre) = transforms.get_mut(tyre) {
                let turns = rig.spin * WHEELS[1] / WHEELS[i / 2];
                tyre.rotation = Quat::from_rotation_x(turns) * Quat::from_rotation_z(FRAC_PI_2);
            }
        }
    }
}
