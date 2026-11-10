// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What stands around the circuit: trees on the hills, clouds, hot-air
//! balloons, a grandstand and a gantry at the start line, tyre stacks and
//! chevron boards at the bends, mountains on the horizon. All procedural,
//! and placed the same way on every machine.

use std::f32::consts::{PI, TAU};

use bevy::{
    asset::RenderAssetUsages,
    light::NotShadowCaster,
    mesh::Indices,
    prelude::*,
    render::render_resource::{Extent3d, PrimitiveTopology, TextureDimension, TextureFormat},
};

use crate::{
    scene::{Course, PALETTE, Part},
    track::{self, Land},
};

/// Something that floats: it bobs gently around its height.
#[derive(Component)]
pub struct Floating {
    height: f32,
    phase: f32,
}

/// The lamps of the start gantry, lit by whoever knows the race.
#[derive(Component)]
pub struct StartLamp;

/// A small repeatable generator: the scenery is the same everywhere.
struct Dice(u64);

impl Dice {
    fn roll(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 40) & 0xff_ffff) as f32 / 16_777_216.0
    }

    fn between(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.roll()
    }

    fn pick<T: Clone>(&mut self, choices: &[T]) -> T {
        choices[(self.roll() * choices.len() as f32) as usize % choices.len()].clone()
    }
}

pub fn bob(time: Res<Time>, mut floating: Query<(&Floating, &mut Transform)>) {
    for (floating, mut place) in &mut floating {
        place.translation.y = floating.height + 1.6 * (time.elapsed_secs() * 0.45 + floating.phase).sin();
    }
}

#[allow(clippy::too_many_lines)]
pub fn build(
    mut commands: Commands,
    course: Res<Course>,
    land: Res<Land>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let track = &course.0.track;
    let frames = track::frames(track);
    let (half, verge) = (track.half_width, track.verge);
    let mut dice = Dice(7);
    let mut matte = |r: f32, g: f32, b: f32, perceptual_roughness: f32| {
        // Pure white is the clouds': they also give off light.
        let emissive = if (r, g, b) == (1.0, 1.0, 1.0) { LinearRgba::rgb(0.62, 0.66, 0.74) } else { LinearRgba::BLACK };
        materials.add(StandardMaterial {
            base_color: Color::srgb(r, g, b),
            perceptual_roughness,
            emissive,
            emissive_exposure_weight: 0.0,
            ..default()
        })
    };
    let centre = frames.iter().map(|f| f.at).sum::<Vec2>() / frames.len() as f32;
    let (start, heading) = track.at(course.0.start);
    let start = Vec2::from(start);

    // Trees: three kinds, thick near the track, sparse on the hills.
    let bark = matte(0.33, 0.2, 0.1, 0.9);
    let pines = [matte(0.07, 0.3, 0.14, 0.85), matte(0.09, 0.36, 0.16, 0.85), matte(0.05, 0.25, 0.16, 0.85)];
    let leaves = [matte(0.3, 0.58, 0.14, 0.85), matte(0.42, 0.66, 0.16, 0.85), matte(0.2, 0.5, 0.18, 0.85)];
    let trunk = meshes.add(Cylinder::new(0.28, 1.0).mesh().resolution(7));
    let cone = meshes.add(Cone { radius: 1.0, height: 1.0 }.mesh().resolution(10));
    let ball = meshes.add(Sphere::new(1.0).mesh().ico(1).expect("a small icosphere"));
    let puff_mesh = meshes.add(Sphere::new(1.0).mesh().ico(2).expect("a small icosphere"));
    let mut planted = 0;
    for _ in 0..2600 {
        let at = centre + Vec2::new(dice.between(-400.0, 400.0), dice.between(-400.0, 400.0));
        let away = land.distance_to_track(at);
        let (kind, turn, size) = (dice.roll(), dice.between(0.0, TAU), dice.between(0.8, 1.5));
        let keep = dice.roll() < if away < 70.0 { 0.55 } else { 0.16 };
        if away < half + verge + 7.0 || at.distance(start) < 55.0 || !keep || planted >= 420 {
            continue;
        }
        planted += 1;
        let place = Transform::from_xyz(at.x, land.height(at) - 0.1, at.y)
            .with_rotation(Quat::from_rotation_y(turn))
            .with_scale(Vec3::splat(size));
        let (pine, leaf) = (dice.pick(&pines), dice.pick(&leaves));
        let piece = |mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>, at: Vec3, scale: Vec3| {
            (Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), Transform::from_translation(at).with_scale(scale))
        };
        commands.spawn((Part, place, Visibility::default())).with_children(|tree| {
            if kind < 0.45 {
                tree.spawn(piece(&trunk, &bark, Vec3::Y * 0.7, Vec3::new(1.0, 1.4, 1.0)));
                for (radius, height, y) in [(1.8, 2.4, 2.2), (1.4, 2.1, 3.5), (0.95, 1.9, 4.7)] {
                    tree.spawn(piece(&cone, &pine, Vec3::Y * y, Vec3::new(radius, height, radius)));
                }
            } else if kind < 0.85 {
                tree.spawn(piece(&trunk, &bark, Vec3::Y * 1.1, Vec3::new(1.2, 2.2, 1.2)));
                for (radius, at) in [
                    (1.7, Vec3::new(0.0, 3.4, 0.0)),
                    (1.25, Vec3::new(1.0, 2.9, 0.3)),
                    (1.15, Vec3::new(-0.8, 3.1, -0.6)),
                ] {
                    tree.spawn(piece(&ball, &leaf, at, Vec3::splat(radius)));
                }
            } else {
                tree.spawn(piece(&trunk, &bark, Vec3::Y * 1.0, Vec3::new(0.8, 2.0, 0.8)));
                tree.spawn(piece(&ball, &leaf, Vec3::Y * 4.2, Vec3::new(1.0, 2.6, 1.0)));
            }
        });
    }

    // Clouds: puffs of flattened spheres, high and far. They glow a little
    // by themselves, as real ones do from the light inside them: seen from
    // below they stay white.
    let cloud = matte(1.0, 1.0, 1.0, 1.0);
    for _ in 0..22 {
        let (angle, radius) = (dice.between(0.0, TAU), dice.between(260.0, 1300.0));
        let at = centre + Vec2::from_angle(angle) * radius;
        let size = dice.between(18.0, 46.0);
        let place = Transform::from_xyz(at.x, dice.between(130.0, 260.0), at.y);
        commands.spawn((Part, place, Visibility::default())).with_children(|puffs| {
            for _ in 0..6 {
                let offset =
                    Vec3::new(dice.between(-1.3, 1.3), dice.between(-0.15, 0.25), dice.between(-0.7, 0.7)) * size;
                let scale = Vec3::new(1.0, 0.55, 0.8) * size * dice.between(0.55, 1.0);
                let puff = Transform::from_translation(offset).with_scale(scale);
                puffs.spawn((Mesh3d(puff_mesh.clone()), MeshMaterial3d(cloud.clone()), puff, NotShadowCaster));
            }
        });
    }

    // Hot-air balloons over the circuit.
    let basket = meshes.add(Cuboid::new(1.6, 1.3, 1.6));
    let wicker = matte(0.45, 0.3, 0.14, 0.9);
    for (i, &(r, g, b)) in PALETTE.iter().enumerate().take(6) {
        let angle = i as f32 / 6.0 * TAU + dice.between(0.0, 0.6);
        let at = centre + Vec2::from_angle(angle) * dice.between(90.0, 260.0);
        let height = dice.between(38.0, 85.0);
        let envelope = matte(r, g, b, 0.6);
        commands
            .spawn((
                Part,
                Floating { height, phase: i as f32 * 1.3 },
                Transform::from_xyz(at.x, height, at.y),
                Visibility::default(),
            ))
            .with_children(|balloon| {
                let shape = Transform::from_scale(Vec3::new(7.0, 8.4, 7.0));
                balloon.spawn((Mesh3d(puff_mesh.clone()), MeshMaterial3d(envelope), shape));
                let below = Transform::from_xyz(0.0, -10.5, 0.0);
                balloon.spawn((Mesh3d(basket.clone()), MeshMaterial3d(wicker.clone()), below));
            });
    }

    // At the start line: a gantry over the road, and a grandstand with its crowd.
    let steel = matte(0.75, 0.77, 0.8, 0.4);
    let concrete = matte(0.7, 0.7, 0.68, 0.9);
    let glow = |materials: &mut Assets<StandardMaterial>, r: f32, g: f32, b: f32| {
        materials.add(StandardMaterial {
            base_color: Color::BLACK,
            emissive: LinearRgba::rgb(r * 5.0, g * 5.0, b * 5.0),
            emissive_exposure_weight: 0.0,
            ..default()
        })
    };
    drop(matte);
    let banner = materials.add(StandardMaterial {
        base_color_texture: Some(images.add(track::chequers(24, 2, [245, 245, 245], [20, 20, 22]))),
        perceptual_roughness: 0.7,
        ..default()
    });
    let lamp = glow(&mut materials, 0.15, 1.0, 0.25);
    let span = 2.0 * (half + 1.8);
    let across_road = Transform::from_xyz(start.x, 0.0, start.y).with_rotation(Quat::from_rotation_y(heading));
    commands.spawn((Part, across_road, Visibility::default())).with_children(|gantry| {
        let pillar = meshes.add(Cuboid::new(0.6, 6.4, 0.6));
        for side in [-1.0, 1.0] {
            let at = Transform::from_xyz(side * span / 2.0, 3.2, 0.0);
            gantry.spawn((Mesh3d(pillar.clone()), MeshMaterial3d(steel.clone()), at));
        }
        let beam = Transform::from_xyz(0.0, 6.6, 0.0);
        gantry.spawn((Mesh3d(meshes.add(Cuboid::new(span + 0.6, 1.2, 0.7))), MeshMaterial3d(steel.clone()), beam));
        // The banner faces the grid, which is behind the line.
        let face = Transform::from_xyz(0.0, 6.6, -0.37).with_rotation(Quat::from_rotation_y(PI));
        gantry.spawn((Mesh3d(meshes.add(Rectangle::new(span, 1.0))), MeshMaterial3d(banner.clone()), face));
        let bulb = meshes.add(Sphere::new(0.26));
        for i in -2..=2 {
            let at = Transform::from_xyz(i as f32 * 1.1, 5.7, -0.3);
            gantry.spawn((Mesh3d(bulb.clone()), MeshMaterial3d(lamp.clone()), at, StartLamp));
        }
    });

    // The grandstand, on the right of the grid. A kart facing +Z has its
    // right at -X: the tiers step away from the road towards -X.
    let crowd: Vec<_> =
        [(0.9, 0.2, 0.2), (0.2, 0.45, 0.9), (0.95, 0.8, 0.2), (0.95, 0.95, 0.95), (0.2, 0.7, 0.35), (0.9, 0.5, 0.15)]
            .into_iter()
            .map(|(r, g, b)| {
                materials.add(StandardMaterial {
                    base_color: Color::srgb(r, g, b),
                    perceptual_roughness: 0.8,
                    ..default()
                })
            })
            .collect();
    let (stand_at, stand_heading) = track.at(course.0.start - 16.0);
    let beside = Transform::from_xyz(stand_at[0], 0.0, stand_at[1]).with_rotation(Quat::from_rotation_y(stand_heading));
    let (length, tiers, depth, rise) = (44.0, 6, 1.5, 0.55);
    commands.spawn((Part, beside, Visibility::default())).with_children(|stand| {
        let tier = meshes.add(Cuboid::new(depth, rise, length));
        let seat = meshes.add(Cuboid::new(0.5, 0.75, 0.5));
        let front = -(half + verge + 3.5);
        for k in 0..tiers {
            let (x, y) = (front - k as f32 * depth, 0.3 + k as f32 * rise);
            stand.spawn((Mesh3d(tier.clone()), MeshMaterial3d(concrete.clone()), Transform::from_xyz(x, y, 0.0)));
            for seat_number in 0..34 {
                if dice.roll() < 0.18 {
                    continue; // an empty seat
                }
                let z = -length / 2.0 + 0.9 + seat_number as f32 * 1.26;
                let at = Transform::from_xyz(x, y + rise / 2.0 + 0.37 + dice.between(0.0, 0.12), z);
                stand.spawn((Mesh3d(seat.clone()), MeshMaterial3d(dice.pick(&crowd)), at));
            }
        }
        let back = front - tiers as f32 * depth;
        let roof = Transform::from_xyz((front + back) / 2.0 + 0.5, tiers as f32 * rise + 3.4, 0.0)
            .with_rotation(Quat::from_rotation_z(0.08));
        let slab = meshes.add(Cuboid::new(tiers as f32 * depth + 2.0, 0.25, length + 1.0));
        stand.spawn((Mesh3d(slab), MeshMaterial3d(steel.clone()), roof));
        let post = meshes.add(Cuboid::new(0.3, tiers as f32 * rise + 3.4, 0.3));
        for z in [-length / 2.0, 0.0, length / 2.0] {
            let at = Transform::from_xyz(back, (tiers as f32 * rise + 3.4) / 2.0, z);
            stand.spawn((Mesh3d(post.clone()), MeshMaterial3d(steel.clone()), at));
        }
    });

    // At the bends: tyre stacks along the outer barrier, chevron boards behind it.
    // Hundreds of them, seen from a kart: a few facets are enough.
    let tyre =
        meshes.add(Torus { minor_radius: 0.15, major_radius: 0.31 }.mesh().minor_resolution(6).major_resolution(12));
    let rubbers = [
        materials.add(StandardMaterial {
            base_color: Color::srgb(0.85, 0.1, 0.1),
            perceptual_roughness: 0.6,
            ..default()
        }),
        materials.add(StandardMaterial {
            base_color: Color::srgb(0.95, 0.95, 0.95),
            perceptual_roughness: 0.6,
            ..default()
        }),
    ];
    let board = meshes.add(Rectangle::new(3.0, 1.2));
    let boards = [false, true].map(|to_left| {
        materials.add(StandardMaterial {
            base_color_texture: Some(images.add(chevrons(to_left))),
            perceptual_roughness: 0.6,
            double_sided: true,
            cull_mode: None,
            ..default()
        })
    });
    let post = meshes.add(Cuboid::new(0.12, 1.6, 0.12));
    for (i, frame) in frames.iter().enumerate() {
        if frame.bend.abs() < 0.011 {
            continue;
        }
        // A bend to the left throws karts to the right, and the other way round.
        let outside = frame.bend.signum();
        let next = &frames[(i + 1) % frames.len()];
        for step in 0..3 {
            let t = step as f32 / 3.0;
            let (at, right) = (frame.at.lerp(next.at, t), frame.right.lerp(next.right, t).normalize_or_zero());
            let foot = at + right * outside * (half + verge + track::BARRIER - 0.48);
            for level in 0..3 {
                let place = Transform::from_xyz(foot.x, 0.15 + 0.29 * level as f32, foot.y);
                let colour = rubbers[(i * 3 + step + level) % 2].clone();
                commands.spawn((Part, Mesh3d(tyre.clone()), MeshMaterial3d(colour), place));
            }
        }
        if frame.bend.abs() > 0.02 && i % 2 == 0 {
            let foot = frame.at + frame.right * outside * (half + verge + track::BARRIER + 1.8);
            let facing = Quat::from_rotation_y(frame.forward.x.atan2(frame.forward.y) + PI);
            let face = Transform::from_xyz(foot.x, 2.1, foot.y).with_rotation(facing);
            // Seen by the drivers, the arrows point the way the road turns.
            let material = boards[usize::from(frame.bend > 0.0)].clone();
            commands.spawn((Part, Mesh3d(board.clone()), MeshMaterial3d(material), face));
            for side in [-1.2, 1.2] {
                let at = face * Transform::from_xyz(side, -1.3, 0.0);
                commands.spawn((Part, Mesh3d(post.clone()), MeshMaterial3d(steel.clone()), at));
            }
        }
    }

    // Mountains on the horizon.
    let ridge = materials.add(StandardMaterial {
        // Far away, the air tints everything blue.
        base_color: Color::srgb(0.42, 0.56, 0.72),
        emissive: LinearRgba::rgb(0.1, 0.16, 0.26),
        emissive_exposure_weight: 0.0,
        perceptual_roughness: 1.0,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    commands.spawn((Part, Mesh3d(meshes.add(mountains(centre, &mut dice))), MeshMaterial3d(ridge), NotShadowCaster));
}

/// A yellow board with black chevrons pointing left or right.
fn chevrons(to_left: bool) -> Image {
    let (width, height) = (48u32, 20u32);
    let pixels: Vec<u8> = (0..height)
        .flat_map(|y| {
            (0..width).flat_map(move |x| {
                let lean = (y as i32 - height as i32 / 2).unsigned_abs();
                // Bands whose tip, in the middle row, is on the side they point to.
                let along = if to_left { x + height - lean } else { x + lean };
                if along % 16 < 7 { [20, 20, 22, 255] } else { [250, 205, 20, 255] }
            })
        })
        .collect();
    let size = Extent3d { width, height, depth_or_array_layers: 1 };
    Image::new(size, TextureDimension::D2, pixels, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default())
}

/// A ring of ridges far around the circuit.
fn mountains(centre: Vec2, dice: &mut Dice) -> Mesh {
    let (segments, radius) = (120usize, 1900.0);
    let heights: Vec<f32> = (0..segments).map(|_| dice.between(70.0, 330.0)).collect();
    let (mut positions, mut normals) = (Vec::new(), Vec::new());
    for i in 0..segments {
        let around = |k: usize| Vec2::from_angle(k as f32 / segments as f32 * TAU);
        let (a, b) = (centre + around(i) * radius, centre + around(i + 1) * radius);
        // Each peak is the mean of its neighbours: a ridge line, not teeth.
        let peak = |k: usize| (heights[k % segments] + heights[(k + 1) % segments] + heights[(k + 2) % segments]) / 3.0;
        let inward = -around(i);
        let quad = [
            Vec3::new(a.x, -20.0, a.y),
            Vec3::new(b.x, -20.0, b.y),
            Vec3::new(b.x, peak(i + 1), b.y),
            Vec3::new(a.x, peak(i), a.y),
        ];
        for corner in [0, 1, 3, 3, 1, 2] {
            positions.push(quad[corner].to_array());
            normals.push([inward.x * 0.6, 0.8, inward.y * 0.6]);
        }
    }
    let indices = (0..positions.len() as u32).collect();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_indices(Indices::U32(indices))
}
