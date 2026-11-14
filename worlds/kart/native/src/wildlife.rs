// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! Life in the country round the circuit (D-104): birds and a few animals,
//! sculpted by the program ([`Sculpture`]), each moving on its own and
//! minding the race.
//!
//! - **Flocks** wheel over the hills, 20 to 45 m up, each bird on its own
//!   loop round the flock's centre, wings beating.
//! - **Birds of prey** glide in wide circles high above, a few beats now
//!   and then.
//! - **Birds on the ground** feed in the meadows near the track, pecking
//!   and hopping; a kart passing within 25 m puts them up: they burst
//!   into the air, wheel, and settle again elsewhere some seconds later.
//! - **Rabbits** nibble in the meadows, hop now and then, and bolt away
//!   in long bounds from a kart within 20 m.
//! - **Deer** graze at the edges of the woods, far from the track; they
//!   raise their heads when a kart goes by, and walk on now and then.
//!
//! Everything follows the circuit's seeded dice and the race's time: the
//! same on every run of the bench.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::{light::NotShadowCaster, prelude::*};
use osage_kart_land::Land;
use osage_procedural::{Batch, Dice, Paint, Sculpture, Shape};

use crate::scene::{Part, Pose, Stage};

pub struct WildlifePlugin;

impl Plugin for WildlifePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                build.run_if(resource_exists_and_changed::<Land>),
                (flocks, ground_birds, rabbits, deer, wings)
                    .chain()
                    .in_set(Stage::Draw)
                    .run_if(resource_exists::<Land>),
            ),
        );
    }
}

/// A bird's wing, beating about the body's long axis.
#[derive(Component)]
struct Wing {
    side: f32,
}

/// How a bird's wings beat now: rad/s, and how far (0 gliding).
#[derive(Component, Default)]
struct Beat {
    rate: f32,
    size: f32,
    phase: f32,
}

/// A bird of a flock in the air, or a bird of prey: its loop round its
/// flock's centre.
#[derive(Component)]
struct Flier {
    flock: usize,
    radius: f32,
    speed: f32,
    phase: f32,
    height: f32,
}

/// A flock's centre, wandering over the land.
#[derive(Component)]
struct Flock {
    centre: Vec2,
    reach: f32,
    speed: f32,
    phase: f32,
    height: f32,
}

/// A bird feeding on the ground, or put up by a kart.
#[derive(Component)]
struct Feeder {
    home: Vec2,
    at: Vec3,
    /// Seconds in the air left; 0 on the ground.
    flying: f32,
    phase: f32,
}

#[derive(Component)]
struct Rabbit {
    at: Vec2,
    heading: f32,
    /// A hop under way: seconds into it, and where it ends.
    hop: Option<(f32, Vec2)>,
    rest: f32,
    phase: f32,
}

#[derive(Component)]
struct Deer {
    at: Vec2,
    heading: f32,
    walking: f32,
    phase: f32,
    head: Entity,
}

/// What the animals see of the race: the karts' places.
fn karts(poses: &Query<&Pose>) -> Vec<Vec2> {
    poses.iter().filter(|pose| !pose.ghost).map(|pose| Vec2::new(pose.kart.x, pose.kart.z)).collect()
}

fn nearest(karts: &[Vec2], at: Vec2) -> Option<(f32, Vec2)> {
    karts.iter().map(|k| (k.distance(at), *k)).min_by(|a, b| a.0.total_cmp(&b.0))
}

/// One mesh from a sculpture, its groups painted.
fn sculpted(sculpture: &Sculpture, cell: f32, paints: &[Paint]) -> Mesh {
    let mut batch = Batch::default();
    for (group, shape) in sculpture.shapes(cell) {
        batch.add(&shape, Transform::IDENTITY, paints[group.min(paints.len() - 1)]);
    }
    batch.merged()
}

/// A small bird's body, along +Z, and one wing (the left, along +X).
fn bird(size: f32) -> (Mesh, Mesh) {
    let mut body = Sculpture::new(0.01 * size);
    body.ellipsoid(0, Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.035, 0.035, 0.08) * size, Quat::IDENTITY)
        .ellipsoid(0, Vec3::new(0.0, 0.015, 0.07) * size, Vec3::splat(0.026) * size, Quat::IDENTITY)
        .cone(1, Vec3::new(0.0, 0.012, 0.09) * size, Vec3::new(0.0, 0.008, 0.115) * size, 0.008 * size, 0.002 * size)
        .cone(0, Vec3::new(0.0, 0.0, -0.06) * size, Vec3::new(0.0, 0.005, -0.13) * size, 0.022 * size, 0.004 * size);
    let body = sculpted(&body, 0.008 * size, &[Paint::plain(0.16, 0.15, 0.14), Paint::plain(0.6, 0.5, 0.2)]);
    // A wing's outline seen from above (out along X, ahead along Y): a
    // rounded leading edge sweeping back to a pointed tip, the trailing
    // edge straight; thin, laid flat.
    let outline = [
        Vec2::new(0.0, 0.035),
        Vec2::new(0.06, 0.042),
        Vec2::new(0.12, 0.022),
        Vec2::new(0.175, -0.035),
        Vec2::new(0.13, -0.05),
        Vec2::new(0.0, -0.05),
    ]
    .map(|p| p * size);
    let wing = Shape::extrude_rounded(&outline, -0.003 * size, 0.003 * size, 0.006 * size, 2);
    let flat = Transform::from_rotation(Quat::from_rotation_x(FRAC_PI_2));
    let mut batch = Batch::default();
    batch.add(&wing, flat, Paint::plain(0.12, 0.11, 0.1));
    (body, batch.merged())
}

fn rabbit() -> Mesh {
    let fur = Paint::plain(0.42, 0.34, 0.25);
    let mut s = Sculpture::new(0.02);
    s.ellipsoid(0, Vec3::new(0.0, 0.11, 0.0), Vec3::new(0.08, 0.09, 0.14), Quat::IDENTITY)
        .ellipsoid(0, Vec3::new(0.0, 0.19, 0.12), Vec3::new(0.055, 0.055, 0.065), Quat::IDENTITY)
        .ellipsoid(0, Vec3::new(0.025, 0.28, 0.1), Vec3::new(0.015, 0.06, 0.008), Quat::from_rotation_x(-0.3))
        .ellipsoid(0, Vec3::new(-0.025, 0.28, 0.1), Vec3::new(0.015, 0.06, 0.008), Quat::from_rotation_x(-0.3))
        .ellipsoid(1, Vec3::new(0.0, 0.13, -0.14), Vec3::splat(0.03), Quat::IDENTITY);
    for side in [-1.0f32, 1.0] {
        // The hind legs folded under the haunches, their long feet flat on the ground.
        s.ellipsoid(0, Vec3::new(side * 0.065, 0.08, -0.07), Vec3::new(0.035, 0.06, 0.07), Quat::from_rotation_x(0.4))
            .ellipsoid(0, Vec3::new(side * 0.06, 0.015, -0.03), Vec3::new(0.022, 0.015, 0.075), Quat::IDENTITY)
            // The forelegs, thin, down from the chest to small paws.
            .cone(0, Vec3::new(side * 0.04, 0.09, 0.09), Vec3::new(side * 0.035, 0.012, 0.11), 0.017, 0.012)
            .ellipsoid(0, Vec3::new(side * 0.035, 0.01, 0.12), Vec3::new(0.014, 0.01, 0.022), Quat::IDENTITY);
    }
    sculpted(&s, 0.012, &[fur, Paint::plain(0.85, 0.83, 0.8)])
}

/// A deer's body without its head and neck, and the head with its neck,
/// pivoting at the shoulders.
fn deer_meshes() -> (Mesh, Mesh) {
    let coat = Paint::plain(0.45, 0.3, 0.18);
    let mut s = Sculpture::new(0.04);
    s.ellipsoid(0, Vec3::new(0.0, 0.95, 0.0), Vec3::new(0.17, 0.2, 0.48), Quat::IDENTITY);
    for (x, z) in [(0.1, 0.33), (-0.1, 0.33), (0.1, -0.35), (-0.1, -0.35)] {
        s.cone(0, Vec3::new(x, 0.85, z), Vec3::new(x, 0.45, z + 0.02), 0.06, 0.035).cone(
            0,
            Vec3::new(x, 0.45, z + 0.02),
            Vec3::new(x, 0.02, z),
            0.032,
            0.022,
        );
    }
    s.ellipsoid(1, Vec3::new(0.0, 0.98, -0.47), Vec3::new(0.07, 0.08, 0.05), Quat::IDENTITY);
    let body = sculpted(&s, 0.03, &[coat, Paint::plain(0.9, 0.88, 0.82)]);
    let mut h = Sculpture::new(0.03);
    h.cone(0, Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.42, 0.18), 0.09, 0.06)
        .ellipsoid(0, Vec3::new(0.0, 0.48, 0.26), Vec3::new(0.07, 0.08, 0.14), Quat::from_rotation_x(0.5))
        .ellipsoid(0, Vec3::new(0.06, 0.58, 0.18), Vec3::new(0.02, 0.06, 0.01), Quat::from_rotation_z(-0.6))
        .ellipsoid(0, Vec3::new(-0.06, 0.58, 0.18), Vec3::new(0.02, 0.06, 0.01), Quat::from_rotation_z(0.6))
        .ellipsoid(1, Vec3::new(0.0, 0.42, 0.37), Vec3::splat(0.025), Quat::IDENTITY);
    let head = sculpted(&h, 0.02, &[coat, Paint::plain(0.08, 0.07, 0.06)]);
    (body, head)
}

/// A free place in the meadows, `low` to `high` m from the track.
fn meadow(land: &Land, dice: &mut Dice, low: f32, high: f32) -> Option<Vec2> {
    for _ in 0..40 {
        let at = land.centre() + Vec2::new(dice.between(-260.0, 260.0), dice.between(-260.0, 260.0));
        let d = land.distance_to_track(at);
        if (low..high).contains(&d) && !land.wet(at) {
            return Some(at);
        }
    }
    None
}

fn build(
    mut commands: Commands,
    land: Res<Land>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut dice = Dice(31);
    let matte = materials.add(StandardMaterial { perceptual_roughness: 0.9, ..default() });
    let (small_body, small_wing) = bird(1.0);
    let (big_body, big_wing) = bird(3.2);
    let (small_body, small_wing) = (meshes.add(small_body), meshes.add(small_wing));
    let (big_body, big_wing) = (meshes.add(big_body), meshes.add(big_wing));
    let spawn_bird =
        |commands: &mut Commands, body: &Handle<Mesh>, wing: &Handle<Mesh>, extra: (Option<Flier>, Option<Feeder>)| {
            let mut entity = commands.spawn((
                Part,
                Mesh3d(body.clone()),
                MeshMaterial3d(matte.clone()),
                Transform::default(),
                NotShadowCaster,
                Beat::default(),
            ));
            if let Some(flier) = extra.0 {
                entity.insert(flier);
            }
            if let Some(feeder) = extra.1 {
                entity.insert(feeder);
            }
            entity.with_children(|bird| {
                for side in [1.0, -1.0] {
                    bird.spawn((
                        Wing { side },
                        Mesh3d(wing.clone()),
                        MeshMaterial3d(matte.clone()),
                        Transform::from_scale(Vec3::new(side, 1.0, 1.0)),
                        NotShadowCaster,
                    ));
                }
            });
        };

    // Flocks wheeling over the hills, and two birds of prey higher still.
    let mut flock = 0;
    for f in 0..6 {
        let Some(centre) = meadow(&land, &mut dice, 40.0, 220.0) else { continue };
        let prey = f >= 4;
        commands.spawn((
            Part,
            Flock {
                centre,
                reach: dice.between(30.0, 90.0),
                speed: dice.between(0.05, 0.12) * if prey { 0.5 } else { 1.0 },
                phase: dice.between(0.0, TAU),
                height: if prey { dice.between(60.0, 85.0) } else { dice.between(20.0, 45.0) },
            },
        ));
        let count = if prey { 1 } else { 6 + (dice.roll() * 9.0) as usize };
        for _ in 0..count {
            let flier = Flier {
                flock,
                radius: if prey { dice.between(25.0, 40.0) } else { dice.between(3.0, 9.0) },
                speed: if prey { dice.between(0.15, 0.25) } else { dice.between(0.6, 1.2) },
                phase: dice.between(0.0, TAU),
                height: dice.between(-2.5, 2.5),
            };
            let (body, wing) = if prey { (&big_body, &big_wing) } else { (&small_body, &small_wing) };
            spawn_bird(&mut commands, body, wing, (Some(flier), None));
        }
        flock += 1;
    }

    // Birds feeding on the ground near the track.
    for _ in 0..4 {
        let Some(home) = meadow(&land, &mut dice, 14.0, 45.0) else { continue };
        debug!(?home, "wildlife: birds feeding");
        for _ in 0..(5 + (dice.roll() * 6.0) as usize) {
            let at = home + Vec2::from_angle(dice.between(0.0, TAU)) * dice.between(0.0, 3.5);
            let feeder = Feeder {
                home: at,
                at: Vec3::new(at.x, land.height(at), at.y),
                flying: 0.0,
                phase: dice.between(0.0, TAU),
            };
            spawn_bird(&mut commands, &small_body, &small_wing, (None, Some(feeder)));
        }
    }

    // Rabbits in the meadows.
    let rabbit = meshes.add(rabbit());
    for _ in 0..10 {
        let Some(at) = meadow(&land, &mut dice, 12.0, 60.0) else { continue };
        debug!(?at, height = land.height(at), "wildlife: a rabbit");
        commands.spawn((
            Part,
            Mesh3d(rabbit.clone()),
            MeshMaterial3d(matte.clone()),
            Transform::from_xyz(at.x, land.height(at), at.y),
            Rabbit {
                at,
                heading: dice.between(0.0, TAU),
                hop: None,
                rest: dice.between(0.5, 4.0),
                phase: dice.between(0.0, TAU),
            },
        ));
    }

    info!(flocks = flock, "wildlife: birds and rabbits placed");
    // Deer at the woods' edge, far off.
    let (body, head) = deer_meshes();
    let (body, head) = (meshes.add(body), meshes.add(head));
    for _ in 0..4 {
        let Some(at) = meadow(&land, &mut dice, 70.0, 140.0) else { continue };
        let head = commands
            .spawn((Mesh3d(head.clone()), MeshMaterial3d(matte.clone()), Transform::from_xyz(0.0, 1.05, 0.38)))
            .id();
        commands
            .spawn((
                Part,
                Mesh3d(body.clone()),
                MeshMaterial3d(matte.clone()),
                Transform::from_xyz(at.x, land.height(at), at.y),
                Deer { at, heading: dice.between(0.0, TAU), walking: 0.0, phase: dice.between(0.0, TAU), head },
            ))
            .add_child(head);
    }
}

/// Faces a model built along +Z the way `forward` goes, banked by `bank`.
fn facing(forward: Vec3, bank: f32) -> Quat {
    let flat = Vec2::new(forward.x, forward.z);
    let yaw = flat.x.atan2(flat.y);
    let pitch = (-forward.y).atan2(flat.length().max(1e-4)).clamp(-0.6, 0.6);
    Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch) * Quat::from_rotation_z(bank)
}

fn flocks(
    time: Res<Time>,
    land: Res<Land>,
    flocks: Query<&Flock>,
    mut birds: Query<(&Flier, &mut Transform, &mut Beat)>,
) {
    let t = time.elapsed_secs();
    let centres: Vec<(Vec3, f32)> = flocks
        .iter()
        .map(|f| {
            let a = t * f.speed + f.phase;
            let at = f.centre + Vec2::new(a.cos(), (a * 1.3).sin()) * f.reach;
            (Vec3::new(at.x, land.height(at) + f.height, at.y), f.speed)
        })
        .collect();
    for (bird, mut transform, mut beat) in &mut birds {
        let Some(&(centre, _)) = centres.get(bird.flock) else { continue };
        let angle = t * bird.speed + bird.phase;
        let place = |a: f32| {
            centre + Vec3::new(a.cos() * bird.radius, bird.height + 1.5 * (a * 2.3).sin(), a.sin() * bird.radius * 0.8)
        };
        let (here, ahead) = (place(angle), place(angle + 0.05));
        let big = bird.radius > 20.0;
        transform.translation = here;
        transform.rotation = facing(ahead - here, if big { -0.35 } else { -0.2 });
        // Small birds beat all the time; a bird of prey glides, a few beats now and then.
        *beat = if big {
            let flapping = ((t * 0.3 + bird.phase).sin() > 0.85) as u8 as f32;
            Beat { rate: 7.0, size: 0.45 * flapping, phase: bird.phase }
        } else {
            Beat { rate: 22.0 + 6.0 * (bird.phase / TAU), size: 0.9, phase: bird.phase }
        };
    }
}

fn ground_birds(
    time: Res<Time>,
    land: Res<Land>,
    poses: Query<&Pose>,
    mut birds: Query<(&mut Feeder, &mut Transform, &mut Beat)>,
) {
    let (t, dt) = (time.elapsed_secs(), time.delta_secs().min(0.1));
    let karts = karts(&poses);
    for (mut bird, mut transform, mut beat) in &mut birds {
        let ground = |at: Vec2| land.height(at);
        let flat = Vec2::new(bird.at.x, bird.at.z);
        let scared = nearest(&karts, flat).is_some_and(|(d, _)| d < 25.0);
        if bird.flying <= 0.0 && scared {
            bird.flying = 6.0 + 4.0 * (bird.phase / TAU);
        }
        if bird.flying > 0.0 {
            // Up and round in a wide loop, then down near home again.
            bird.flying -= dt;
            let up = (bird.flying.min(2.0) / 2.0).min(1.0);
            let a = t * 0.9 + bird.phase;
            let loop_at = bird.home + Vec2::new(a.cos(), a.sin()) * 12.0;
            let target = Vec3::new(loop_at.x, ground(loop_at) + 2.0 + 10.0 * up, loop_at.y);
            let step = (target - bird.at).clamp_length_max(9.0 * dt);
            let was = bird.at;
            bird.at += step;
            transform.translation = bird.at;
            if step.length() > 1e-4 {
                transform.rotation = facing(bird.at - was, -0.15);
            }
            *beat = Beat { rate: 24.0, size: 0.9, phase: bird.phase };
            if bird.flying <= 0.0 {
                bird.flying = 0.0;
            }
            continue;
        }
        // On the ground: settle, then peck and hop about home.
        let hop = (t * 0.7 + bird.phase).sin();
        let wander = bird.home + Vec2::new((t * 0.13 + bird.phase).sin(), (t * 0.11 + bird.phase * 1.7).cos()) * 1.2;
        let target = Vec3::new(wander.x, ground(wander), wander.y);
        let was = bird.at;
        let step = (target - bird.at).clamp_length_max(3.0 * dt);
        bird.at += step;
        bird.at.y = bird.at.y.max(ground(Vec2::new(bird.at.x, bird.at.z)));
        let peck = 0.35 * ((t * 5.0 + bird.phase).sin().max(0.0)).powi(4);
        transform.translation = bird.at + Vec3::Y * (0.03 * hop.max(0.0));
        let heading = if (bird.at - was).length() > 1e-3 { bird.at - was } else { transform.forward().as_vec3() };
        transform.rotation = facing(Vec3::new(heading.x, 0.0, heading.z), 0.0) * Quat::from_rotation_x(peck);
        *beat = Beat { rate: 0.0, size: 0.0, phase: bird.phase };
    }
}

fn wings(time: Res<Time>, birds: Query<(&Beat, &Children)>, mut wings: Query<(&Wing, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (beat, children) in &birds {
        // Folded back along the body when still; beating about the long
        // axis when not.
        let angle = beat.size * (t * beat.rate + beat.phase).sin();
        for child in children.iter() {
            if let Ok((wing, mut transform)) = wings.get_mut(child) {
                transform.rotation = if beat.size == 0.0 {
                    Quat::from_rotation_y(wing.side * 1.45) * Quat::from_rotation_z(0.15)
                } else {
                    Quat::from_rotation_z(angle * wing.side)
                };
            }
        }
    }
}

fn rabbits(time: Res<Time>, land: Res<Land>, poses: Query<&Pose>, mut rabbits: Query<(&mut Rabbit, &mut Transform)>) {
    let (t, dt) = (time.elapsed_secs(), time.delta_secs().min(0.1));
    let karts = karts(&poses);
    for (mut rabbit, mut transform) in &mut rabbits {
        let threat = nearest(&karts, rabbit.at).filter(|(d, _)| *d < 20.0);
        if rabbit.hop.is_none() {
            rabbit.rest -= dt;
            if threat.is_some() || rabbit.rest <= 0.0 {
                // Away from the kart in long bounds, or a short hop about.
                let (length, heading) = match threat {
                    Some((_, kart)) => {
                        let away = (rabbit.at - kart).normalize_or(Vec2::X);
                        (1.8, away.x.atan2(away.y) + 0.4 * (t + rabbit.phase).sin())
                    }
                    None => (0.4, rabbit.heading + 0.8 * (t * 1.7 + rabbit.phase).sin()),
                };
                rabbit.heading = heading;
                let to = rabbit.at + Vec2::new(heading.sin(), heading.cos()) * length;
                if land.distance_to_track(to) > 9.0 && !land.wet(to) {
                    rabbit.hop = Some((0.0, to));
                } else {
                    rabbit.heading += PI;
                }
                rabbit.rest = if threat.is_some() { 0.05 } else { 1.0 + 3.0 * ((t + rabbit.phase).sin() * 0.5 + 0.5) };
            }
        }
        let (mut at, mut up) = (rabbit.at, 0.0);
        if let Some((into, to)) = rabbit.hop {
            let length = 0.35;
            let s = ((into + dt) / length).min(1.0);
            at = rabbit.at.lerp(to, s);
            up = 4.0 * s * (1.0 - s) * (to.distance(rabbit.at) * 0.18);
            rabbit.hop = if s >= 1.0 {
                rabbit.at = to;
                None
            } else {
                Some((into + dt, to))
            };
        }
        // Nibbling: the head bobs a little while it sits.
        let nibble = if rabbit.hop.is_none() { 0.05 * (t * 6.0 + rabbit.phase).sin().max(0.0) } else { 0.0 };
        transform.translation = Vec3::new(at.x, land.height(at) + up, at.y);
        transform.rotation = Quat::from_rotation_y(rabbit.heading) * Quat::from_rotation_x(nibble);
    }
}

fn deer(
    time: Res<Time>,
    land: Res<Land>,
    poses: Query<&Pose>,
    mut deer: Query<(&mut Deer, &mut Transform), Without<Wing>>,
    mut heads: Query<&mut Transform, (Without<Deer>, Without<Wing>)>,
) {
    let (t, dt) = (time.elapsed_secs(), time.delta_secs().min(0.1));
    let karts = karts(&poses);
    for (mut animal, mut transform) in &mut deer {
        let watching = nearest(&karts, animal.at).filter(|(d, _)| *d < 60.0);
        // Now and then a few steps on, grazing as it goes.
        let walk = ((t * 0.05 + animal.phase).sin() > 0.7) && watching.is_none();
        animal.walking = if walk { 0.6 } else { 0.0 };
        if walk {
            animal.heading += 0.08 * dt * (t * 0.3 + animal.phase).sin();
            let to = animal.at + Vec2::new(animal.heading.sin(), animal.heading.cos()) * animal.walking * dt;
            if land.distance_to_track(to) > 60.0 && !land.wet(to) {
                animal.at = to;
            } else {
                animal.heading += PI * dt;
            }
        }
        transform.translation = Vec3::new(animal.at.x, land.height(animal.at), animal.at.y);
        transform.rotation = Quat::from_rotation_y(animal.heading);
        // Head down to graze, up and turned to a kart going by.
        if let Ok(mut head) = heads.get_mut(animal.head) {
            head.rotation = match watching {
                Some((_, kart)) => {
                    let to = kart - animal.at;
                    let turn = (to.x.atan2(to.y) - animal.heading + PI).rem_euclid(TAU) - PI;
                    Quat::from_rotation_y(turn.clamp(-1.0, 1.0)) * Quat::from_rotation_x(-0.15)
                }
                None => Quat::from_rotation_x(1.9 + 0.08 * (t * 2.0 + animal.phase).sin()),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_built_along_z_faces_where_it_goes() {
        let east = facing(Vec3::X, 0.0) * Vec3::Z;
        assert!(east.distance(Vec3::X) < 1e-4, "{east}");
        let climbing = facing(Vec3::new(0.0, 1.0, 1.0), 0.0) * Vec3::Z;
        assert!(climbing.y > 0.3, "the nose up when climbing: {climbing}");
    }

    #[test]
    fn the_animals_are_whole_meshes() {
        assert!(rabbit().count_vertices() > 50);
        let (body, wing) = bird(1.0);
        assert!(body.count_vertices() > 20 && wing.count_vertices() > 4);
        let (body, head) = deer_meshes();
        assert!(body.count_vertices() > 100 && head.count_vertices() > 50);
    }
}
