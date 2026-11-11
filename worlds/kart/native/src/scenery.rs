// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What stands around the circuit: the landmarks ([`crate::landmarks`]),
//! then what grows between them — trees in groves, bushes, flowers, rocks —
//! and what is in the sky: clouds, hot-air balloons, mountains on the
//! horizon. All procedural, and placed the same way on every machine.
//!
//! With the asset manifest's models ([`Roadside`]), the circuit is dressed
//! as a real one instead (`osage-kart-roadside`): tyre walls, grass, wild
//! plants, rocks and pines near the track; the toys — clouds, balloons,
//! arches, the fair, the windmill — give way to the photographed sky, and
//! procedural trees only stand far away, in the colours of real ones.
//!
//! Nearly all of it is static: it is sculpted into a few meshes per patch of
//! land ([`crate::sculpt`]) instead of thousands of objects. Only what moves
//! (balloons, sails, the big wheel, a boat) and what has a texture of its
//! own is an object.

use std::f32::consts::TAU;

use bevy::{
    asset::RenderAssetUsages, light::NotShadowCaster, mesh::Indices, prelude::*,
    render::render_resource::PrimitiveTopology,
};

use osage_kart_roadside::{Detail, Models};
use osage_procedural::{Batch, Paint, Shape};
use osage_render::Tier;

use crate::{
    landmarks,
    scene::{Course, PALETTE, Part},
    track::{self, Land, Site},
};

/// The models of the roadside, when the asset manifest's content is there
/// and the tier can afford them.
#[derive(Resource)]
pub struct Roadside {
    pub models: Models,
    pub detail: Detail,
}

/// Loads the roadside's models from T3. Below, the integrated GPUs of the reference cannot afford them (T2 took
/// 54 ms against its 16.7 with the large ones, 2026-11-11): the procedural
/// scenery stands in.
pub fn load_roadside(mut commands: Commands, tier: Res<Tier>, server: Res<AssetServer>) {
    if osage_content::root().is_none() || *tier < Tier::T3 {
        return;
    }
    let detail = Detail::Full;
    commands.insert_resource(Roadside { models: Models::load(&server), detail });
}

/// Something that floats: it bobs gently around its height.
#[derive(Component)]
pub struct Floating {
    pub height: f32,
    pub phase: f32,
    /// How far it rises and sinks, metres.
    pub swing: f32,
}

/// Something that turns on itself, around its own Z axis.
#[derive(Component)]
pub struct Turning {
    /// Radians per second.
    pub rate: f32,
}

pub use osage_procedural::Dice;

/// The static scenery being gathered, and where nothing may grow.
pub struct Set {
    /// Rough things: leaves, wood, cloth, plaster.
    pub matte: Batch,
    /// Smooth things: paint, rubber, steel.
    pub satin: Batch,
    /// Small bright things that cast no shadow: flowers.
    pub bright: Batch,
    pub dice: Dice,
    /// Places kept clear of trees.
    pub clearings: Vec<Site>,
}

impl Set {
    pub fn clear(&mut self, centre: Vec2, radius: f32) {
        self.clearings.push(Site { centre, radius });
    }

    fn is_clear(&self, at: Vec2) -> bool {
        self.clearings.iter().any(|site| site.centre.distance(at) < site.radius)
    }
}

/// What a builder needs to add an object of its own to the scene.
pub struct Props<'a, 'w, 's> {
    pub commands: &'a mut Commands<'w, 's>,
    pub meshes: &'a mut Assets<Mesh>,
    pub materials: &'a mut Assets<StandardMaterial>,
    pub images: &'a mut Assets<Image>,
    /// The material of sculpted things: their colours are in their vertices.
    pub satin: Handle<StandardMaterial>,
}

impl Props<'_, '_, '_> {
    /// A sculpted object that moves: one mesh, placed, with what makes it move.
    pub fn moving(&mut self, sculpted: Batch, place: Transform, motion: impl Bundle) -> Entity {
        let mesh = self.meshes.add(sculpted.merged());
        self.commands.spawn((Part, Mesh3d(mesh), MeshMaterial3d(self.satin.clone()), place, motion)).id()
    }
}

pub fn bob(time: Res<Time>, mut floating: Query<(&Floating, &mut Transform)>) {
    for (floating, mut place) in &mut floating {
        place.translation.y = floating.height + floating.swing * (time.elapsed_secs() * 0.45 + floating.phase).sin();
    }
}

pub fn turn(time: Res<Time>, mut turning: Query<(&Turning, &mut Transform)>) {
    for (turning, mut place) in &mut turning {
        place.rotate_local_z(turning.rate * time.delta_secs());
    }
}

/// A point of the land as a place in space.
pub fn on_land(land: &Land, at: Vec2) -> Vec3 {
    Vec3::new(at.x, land.height(at), at.y)
}

#[allow(clippy::too_many_arguments)]
pub fn build(
    mut commands: Commands,
    course: Res<Course>,
    land: Res<Land>,
    roadside: Option<Res<Roadside>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let track = &course.0.track;
    let frames = track::frames(track);
    let centre = frames.iter().map(|f| f.at).sum::<Vec2>() / frames.len() as f32;
    let matte = materials.add(StandardMaterial { perceptual_roughness: 0.85, ..default() });
    let satin = materials.add(StandardMaterial { perceptual_roughness: 0.38, ..default() });
    let mut set = Set {
        matte: Batch::default(),
        satin: Batch::default(),
        bright: Batch::default(),
        dice: Dice(7),
        clearings: Vec::new(),
    };
    let mut props = Props {
        commands: &mut commands,
        meshes: &mut meshes,
        materials: &mut materials,
        images: &mut images,
        satin: satin.clone(),
    };

    let photographed = osage_content::root().is_some();
    landmarks::start(&mut set, &mut props, &course.0, &land, photographed);
    landmarks::lake(&mut set, &mut props, &land);
    // With the manifest's content, the circuit is photographed: no toys.
    if photographed {
        let look = if let Some(roadside) = &roadside {
            let scatters =
                osage_kart_roadside::scatters(&land, &frames, track, &set.clearings, &roadside.models, roadside.detail);
            for scatter in scatters {
                commands.spawn((Part, scatter));
            }
            // Modelled woods stand far too: nothing procedural is needed.
            Growth::None
        } else {
            Growth::Real
        };
        plant(&mut set, &land, track.half_width + track.verge, centre, look);
        spawn(&mut commands, &mut meshes, set, matte, satin);
        return;
    }
    landmarks::bends(&mut set, &mut props, track, &frames);
    landmarks::arches(&mut set, &course.0, &frames);
    landmarks::knoll(&mut set, &mut props, &course.0, &land);
    landmarks::fair(&mut set, &mut props, &course.0, &land);
    plant(&mut set, &land, track.half_width + track.verge, centre, Growth::Toy);
    balloons(&mut set, &mut props, centre);

    // Clouds glow a little by themselves, as real ones do from the light
    // inside them: seen from below they stay white.
    let cloud = props.materials.add(StandardMaterial {
        perceptual_roughness: 1.0,
        emissive: LinearRgba::rgb(0.62, 0.66, 0.74),
        emissive_exposure_weight: 0.0,
        ..default()
    });
    for mesh in clouds(&mut set.dice, centre).meshes() {
        commands.spawn((Part, Mesh3d(meshes.add(mesh)), MeshMaterial3d(cloud.clone()), NotShadowCaster));
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
    let mountains = mountains(centre, &mut set.dice);
    commands.spawn((Part, Mesh3d(meshes.add(mountains)), MeshMaterial3d(ridge), NotShadowCaster));

    spawn(&mut commands, &mut meshes, set, matte, satin);
}

/// The static scenery gathered, as a few meshes.
fn spawn(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    set: Set,
    matte: Handle<StandardMaterial>,
    satin: Handle<StandardMaterial>,
) {
    for mesh in set.matte.meshes() {
        commands.spawn((Part, Mesh3d(meshes.add(mesh)), MeshMaterial3d(matte.clone())));
    }
    for mesh in set.satin.meshes() {
        commands.spawn((Part, Mesh3d(meshes.add(mesh)), MeshMaterial3d(satin.clone())));
    }
    for mesh in set.bright.meshes() {
        commands.spawn((Part, Mesh3d(meshes.add(mesh)), MeshMaterial3d(matte.clone()), NotShadowCaster));
    }
}

/// What grows, and how it looks.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Growth {
    /// Everything, in a toy's bright colours.
    Toy,
    /// Trees and bushes in the muted greens of real ones, no flowers or rocks.
    Real,
    /// Only trees, only far from the track: the near roadside has models.
    Far,
    /// Nothing: modelled trees stand everywhere.
    None,
}

/// Trees in groves, bushes, flowers in the meadows, a few rocks.
fn plant(set: &mut Set, land: &Land, roadside: f32, centre: Vec2, growth: Growth) {
    if growth == Growth::None {
        return;
    }
    let far = growth == Growth::Far;
    let trunk = Shape::lathe(&[(0.34, 0.0), (0.25, 0.55), (0.2, 1.0)], 6);
    let cone = Shape::lathe(&[(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)], 9);
    let ball = Shape::new(Sphere::new(1.0).mesh().ico(1).expect("a small icosphere"));
    let spire = Shape::lathe(&[(0.0, 0.0), (0.62, 0.14), (0.86, 0.42), (0.66, 0.78), (0.0, 1.0)], 8);
    let pebble = Shape::new(Sphere::new(1.0).mesh().ico(0).expect("a small icosphere"));
    let petals = Shape::lathe(&[(0.0, 0.05), (1.0, 0.0), (0.0, -0.02)], 6);

    let bark = Paint::shaded(0.42, 0.27, 0.15, 0.35);
    let (pines, leaves) = if growth != Growth::Toy {
        // Foliage seen from afar: dark, olive, a little blue.
        (
            [(0.07, 0.16, 0.09), (0.09, 0.19, 0.1), (0.06, 0.14, 0.1)],
            [(0.16, 0.24, 0.09), (0.2, 0.27, 0.1), (0.13, 0.21, 0.09)],
        )
    } else {
        (
            [(0.1, 0.42, 0.2), (0.13, 0.48, 0.22), (0.08, 0.36, 0.22)],
            [(0.36, 0.68, 0.18), (0.48, 0.76, 0.2), (0.25, 0.6, 0.22)],
        )
    };
    // Now and then a tree in blossom, or one that has turned.
    let rare = [(0.98, 0.7, 0.8), (1.0, 0.86, 0.9), (0.96, 0.62, 0.2), (0.97, 0.8, 0.28)];

    // Whether something may grow here: not on the road's side, not in a
    // clearing, not in the lake.
    let free = |set: &Set, at: Vec2, margin: f32| {
        land.distance_to_track(at) > roadside + margin && !set.is_clear(at) && !land.wet(at)
    };

    let mut planted = 0;
    for _ in 0..9000 {
        let at = centre + Vec2::new(set.dice.between(-420.0, 420.0), set.dice.between(-420.0, 420.0));
        let (kind, turn, size) = (set.dice.roll(), set.dice.between(0.0, TAU), set.dice.between(0.8, 1.55));
        let (chance, tint) = (set.dice.roll(), set.dice.roll());
        // Groves and glades, thicker near the circuit where they are seen.
        let grove = track::noise(at / 55.0 + 3.0);
        let near = land.distance_to_track(at) < 90.0;
        let thick = (grove - 0.42).max(0.0) * if near { 1.5 } else { 0.5 };
        if chance > thick || planted >= 760 || !free(set, at, 7.0) || (far && land.distance_to_track(at) < 150.0) {
            continue;
        }
        planted += 1;
        let foot = on_land(land, at) - Vec3::Y * 0.1;
        let stand =
            Transform::from_translation(foot).with_rotation(Quat::from_rotation_y(turn)).with_scale(Vec3::splat(size));
        let mut piece = |shape: &Shape, at: Vec3, scale: Vec3, paint: Paint| {
            set.matte.add_within(foot, shape, stand * Transform::from_translation(at).with_scale(scale), paint);
        };
        let crown = |(r, g, b): (f32, f32, f32)| Paint::blend((r * 0.6, g * 0.62, b * 0.7), (r, g, b));
        if kind < 0.42 {
            let green = set.dice.pick(&pines);
            piece(&trunk, Vec3::ZERO, Vec3::new(1.0, 1.5, 1.0), bark);
            for (radius, height, y) in [(1.85, 2.5, 1.0), (1.45, 2.2, 2.4), (1.0, 2.0, 3.7)] {
                piece(&cone, Vec3::Y * y, Vec3::new(radius, height, radius), crown(green));
            }
        } else if kind < 0.86 {
            let green =
                if tint < 0.09 && growth == Growth::Toy { set.dice.pick(&rare) } else { set.dice.pick(&leaves) };
            piece(&trunk, Vec3::ZERO, Vec3::new(1.25, 2.3, 1.25), bark);
            for (radius, at) in
                [(1.75, Vec3::new(0.0, 3.4, 0.0)), (1.3, Vec3::new(1.05, 2.9, 0.3)), (1.2, Vec3::new(-0.85, 3.1, -0.6))]
            {
                piece(&ball, at, Vec3::new(radius, radius * 0.9, radius), crown(green));
            }
        } else {
            let green = set.dice.pick(&leaves);
            piece(&trunk, Vec3::ZERO, Vec3::new(0.8, 1.4, 0.8), bark);
            piece(&spire, Vec3::Y * 1.0, Vec3::new(1.3, 6.2, 1.3), crown(green));
        }
    }

    if far {
        return;
    }
    // Bushes, mostly along the outside of the barriers.
    for _ in 0..1500 {
        let at = centre + Vec2::new(set.dice.between(-260.0, 260.0), set.dice.between(-260.0, 260.0));
        let (size, chance) = (set.dice.between(0.6, 1.2), set.dice.roll());
        let green = set.dice.pick(&leaves);
        let away = land.distance_to_track(at) - roadside;
        if away > 40.0 || chance > 0.3 || !free(set, at, 5.5) {
            continue;
        }
        let foot = on_land(land, at);
        let paint = Paint::blend((green.0 * 0.55, green.1 * 0.6, green.2 * 0.65), green);
        for (offset, radius) in [(Vec3::ZERO, 1.0), (Vec3::new(0.8, -0.1, 0.3), 0.7), (Vec3::new(-0.5, -0.1, 0.6), 0.6)]
        {
            let place = Transform::from_translation(foot + (offset + Vec3::Y * 0.45) * size)
                .with_scale(Vec3::new(radius, radius * 0.75, radius) * size);
            set.matte.add_within(foot, &ball, place, paint);
        }
    }

    if growth == Growth::Real {
        return;
    }
    // Flowers: drifts of one colour each, in the meadows near the circuit.
    let colours = [(1.0, 1.0, 1.0), (1.0, 0.86, 0.2), (1.0, 0.5, 0.7), (0.95, 0.25, 0.2), (0.62, 0.45, 0.95)];
    for _ in 0..420 {
        let middle = centre + Vec2::new(set.dice.between(-230.0, 230.0), set.dice.between(-230.0, 230.0));
        let colour = set.dice.pick(&colours);
        let (spread, count) = (set.dice.between(2.0, 6.0), 6 + (set.dice.roll() * 14.0) as usize);
        let away = land.distance_to_track(middle) - roadside;
        if away > 70.0 || !free(set, middle, 6.5 + spread) {
            continue;
        }
        for _ in 0..count {
            let at = middle + Vec2::from_angle(set.dice.between(0.0, TAU)) * spread * set.dice.roll().sqrt();
            let lean =
                Quat::from_rotation_x(set.dice.between(-0.3, 0.3)) * Quat::from_rotation_z(set.dice.between(-0.3, 0.3));
            let place = Transform::from_translation(on_land(land, at) + Vec3::Y * 0.22)
                .with_rotation(lean)
                .with_scale(Vec3::splat(set.dice.between(0.16, 0.3)));
            set.bright.add(&petals, place, Paint::plain(colour.0, colour.1, colour.2));
        }
    }

    // A few rocks.
    for _ in 0..600 {
        let at = centre + Vec2::new(set.dice.between(-300.0, 300.0), set.dice.between(-300.0, 300.0));
        let (size, chance, turn) = (set.dice.between(0.5, 1.7), set.dice.roll(), set.dice.between(0.0, TAU));
        if chance > 0.12 || !free(set, at, 6.0) {
            continue;
        }
        let place = Transform::from_translation(on_land(land, at) + Vec3::Y * 0.2 * size)
            .with_rotation(Quat::from_rotation_y(turn))
            .with_scale(Vec3::new(1.0, 0.62, 0.8) * size);
        set.matte.add(&pebble, place, Paint::blend((0.42, 0.42, 0.44), (0.72, 0.71, 0.68)));
    }
}

/// Clouds: puffs of flattened spheres, high and far.
fn clouds(dice: &mut Dice, centre: Vec2) -> Batch {
    let puff = Shape::new(Sphere::new(1.0).mesh().ico(2).expect("a small icosphere"));
    let mut clouds = Batch::default();
    for _ in 0..24 {
        let (angle, radius) = (dice.between(0.0, TAU), dice.between(260.0, 1300.0));
        let at = centre + Vec2::from_angle(angle) * radius;
        let size = dice.between(18.0, 46.0);
        let middle = Vec3::new(at.x, dice.between(130.0, 260.0), at.y);
        for _ in 0..6 {
            let offset = Vec3::new(dice.between(-1.3, 1.3), dice.between(-0.15, 0.25), dice.between(-0.7, 0.7)) * size;
            let scale = Vec3::new(1.0, 0.55, 0.8) * size * dice.between(0.55, 1.0);
            let place = Transform::from_translation(middle + offset).with_scale(scale);
            // Their undersides are in their own shade.
            clouds.add_within(middle, &puff, place, Paint::blend((0.8, 0.84, 0.93), (1.0, 1.0, 1.0)));
        }
    }
    clouds
}

/// Hot-air balloons over the circuit, each in two colours.
fn balloons(set: &mut Set, props: &mut Props, centre: Vec2) {
    // The envelope: round above, drawn in towards the basket.
    let envelope = [(0.0, 8.6), (2.6, 8.2), (5.0, 6.6), (6.2, 4.2), (6.0, 1.6), (4.6, -1.2), (2.6, -4.0), (1.3, -5.6)];
    let basket = Shape::block(Vec3::new(-0.8, -8.6, -0.8), Vec3::new(0.8, -7.3, 0.8));
    let rope = |x: f32, z: f32| {
        Shape::sweep(
            &[Vec3::new(x * 0.75, -7.3, z * 0.75), Vec3::new(x * 1.25, -5.5, z * 1.25)],
            0.04,
            4,
            Vec3::X + Vec3::Z,
        )
    };
    for i in 0..6 {
        let angle = i as f32 / 6.0 * TAU + set.dice.between(0.0, 0.6);
        let at = centre + Vec2::from_angle(angle) * set.dice.between(90.0, 260.0);
        let height = set.dice.between(38.0, 85.0);
        let ((r, g, b), white) = (PALETTE[i], i % 2 == 0);
        let other = if white { (0.97, 0.96, 0.92) } else { PALETTE[(i + 3) % 8] };
        let mut balloon = Batch::default();
        for gore in 0..12 {
            // Profiles run from the foot up: the envelope's is listed from its crown.
            let profile: Vec<(f32, f32)> = envelope.iter().rev().copied().collect();
            let slice = Shape::gore(&profile, 2, gore as f32 / 12.0 * TAU, TAU / 12.0);
            let (r, g, b) = if gore % 2 == 0 { (r, g, b) } else { other };
            balloon.add(&slice, Transform::IDENTITY, Paint::blend((r * 0.7, g * 0.7, b * 0.72), (r, g, b)));
        }
        balloon.add(&basket, Transform::IDENTITY, Paint::shaded(0.55, 0.38, 0.2, 0.3));
        for (x, z) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            balloon.add(&rope(x, z), Transform::IDENTITY, Paint::plain(0.3, 0.25, 0.2));
        }
        let place = Transform::from_xyz(at.x, height, at.y);
        props.moving(balloon, place, Floating { height, phase: i as f32 * 1.3, swing: 1.6 });
    }
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
