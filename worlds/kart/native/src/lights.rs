// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A race at night (D-085): floodlights along the track, and lights on
//! the karts.
//!
//! - **Floodlights:** a mast every 40 m round the circuit, on one side then
//!   the other, 13 m tall beyond the barrier, its lamp a spot light aimed
//!   at the road ahead of it — about 500 lux where it lands, the lighting
//!   of a club circuit's night races; none in a tunnel (its own lamps light
//!   it) nor on a bridge.
//! - **On the karts:** a lamp under the nose, lighting the road ahead, and
//!   a red glow at the back.
//!
//! Built while the sky shown is the night's ([`SkyShown`]): a night race,
//! or `--sky night` on the bench; rebuilt with the circuit.

use bevy::prelude::*;
use osage_kart_land::BARRIER;
use osage_kart_tracks::SpanKind;

use crate::scene::{Course, Pose, Sky, SkyShown, Stage};

/// Masts stand this far apart round the circuit, m.
const MAST_EVERY: f32 = 40.0;
/// A mast's height, m.
const MAST: f32 = 13.0;
/// A floodlight's light, lumens: about 500 lux on the road 20 m away.
const FLOOD: f32 = 2.4e6;
/// A kart's lamp, lumens.
const HEADLAMP: f32 = 1.2e5;

pub struct LightsPlugin;

impl Plugin for LightsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (floodlights, headlamps).in_set(Stage::Draw));
    }
}

/// What the floodlights are part of: removed together, by this module
/// (not the circuit's parts, which a rebuild removes after them).
#[derive(Component)]
struct Floodlit;

/// A kart's lamps, as children of its model.
#[derive(Component)]
struct Headlamp;

/// Whether `along` lies on a span of `kind`.
fn on_span(course: &Course, along: f32, kind: SpanKind) -> bool {
    let length = course.0.track.length();
    course.0.spans.iter().any(|&(k, from, span)| k == kind && (along - from).rem_euclid(length) <= span + 4.0)
}

/// Where each mast stands and where its lamp aims, round the circuit.
pub fn masts(course: &Course) -> Vec<(Vec3, Vec3)> {
    let track = &course.0.track;
    let length = track.length();
    let off = track.half_width + track.verge + BARRIER + 2.6;
    let count = (length / MAST_EVERY).floor().max(3.0) as usize;
    let mut masts = Vec::new();
    for i in 0..count {
        let along = i as f32 * length / count as f32;
        if on_span(course, along, SpanKind::Tunnel) || on_span(course, along, SpanKind::Bridge) {
            continue;
        }
        let ([x, z], heading) = track.at(along);
        let across = Vec3::new(heading.cos(), 0.0, -heading.sin());
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        let road = track.lift(along).0;
        let foot = Vec3::new(x, road, z) + across * (side * off);
        // Aimed at the middle of the road, a little ahead.
        let ahead = (along + 14.0).rem_euclid(length);
        let ([ax, az], _) = track.at(ahead);
        let aim = Vec3::new(ax, track.lift(ahead).0, az);
        masts.push((foot, aim));
    }
    masts
}

fn floodlights(
    mut commands: Commands,
    course: Option<Res<Course>>,
    shown: Option<Res<SkyShown>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    built: Query<Entity, With<Floodlit>>,
    mut lit: Local<Option<(f32, bool)>>,
) {
    let (Some(course), Some(shown)) = (course, shown) else { return };
    let night = shown.sky == Sky::Night;
    let wanted = (course.0.track.length(), night);
    if *lit == Some(wanted) && !course.is_changed() {
        return;
    }
    *lit = Some(wanted);
    for entity in &built {
        commands.entity(entity).despawn();
    }
    if !night {
        return;
    }
    let steel = materials.add(StandardMaterial {
        base_color: Color::srgb(0.42, 0.44, 0.46),
        metallic: 0.8,
        perceptual_roughness: 0.45,
        ..default()
    });
    let lamp = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.97, 0.9),
        emissive: LinearRgba::rgb(60.0, 57.0, 50.0),
        ..default()
    });
    let pole = meshes.add(Cylinder::new(0.16, MAST));
    let head = meshes.add(Cuboid::new(1.8, 0.9, 0.35));
    let face = meshes.add(Cuboid::new(1.6, 0.7, 0.05));
    let masts = masts(&course);
    for (foot, aim) in &masts {
        let top = *foot + Vec3::Y * MAST;
        commands.spawn((
            Floodlit,
            Mesh3d(pole.clone()),
            MeshMaterial3d(steel.clone()),
            Transform::from_translation(*foot + Vec3::Y * (MAST / 2.0)),
        ));
        let facing = Transform::from_translation(top).looking_at(*aim, Vec3::Y);
        commands.spawn((Floodlit, Mesh3d(head.clone()), MeshMaterial3d(steel.clone()), facing));
        commands.spawn((
            Floodlit,
            Mesh3d(face.clone()),
            MeshMaterial3d(lamp.clone()),
            Transform::from_translation(top + facing.forward() * 0.2).looking_at(*aim, Vec3::Y),
        ));
        commands.spawn((
            Floodlit,
            SpotLight {
                color: Color::srgb(1.0, 0.96, 0.88),
                intensity: FLOOD,
                range: 70.0,
                outer_angle: 0.95,
                inner_angle: 0.55,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_translation(top + facing.forward() * 0.4).looking_at(*aim, Vec3::Y),
        ));
    }
    info!(masts = masts.len(), "night: floodlights along the track");
}

/// A lamp under each kart's nose and a red glow at its back, at night.
fn headlamps(
    mut commands: Commands,
    shown: Option<Res<SkyShown>>,
    karts: Query<(Entity, &Pose, Option<&Children>)>,
    lamps: Query<Entity, With<Headlamp>>,
) {
    let night = shown.is_some_and(|shown| shown.sky == Sky::Night);
    if !night {
        for lamp in &lamps {
            commands.entity(lamp).despawn();
        }
        return;
    }
    for (kart, pose, children) in &karts {
        if pose.ghost || children.is_some_and(|children| children.iter().any(|c| lamps.contains(c))) {
            continue;
        }
        commands.entity(kart).with_children(|kart| {
            kart.spawn((
                Headlamp,
                SpotLight {
                    color: Color::srgb(1.0, 0.97, 0.92),
                    intensity: HEADLAMP,
                    range: 35.0,
                    outer_angle: 0.6,
                    inner_angle: 0.35,
                    shadow_maps_enabled: false,
                    ..default()
                },
                // The kart faces +Z; a light shines along its own -Z.
                Transform::from_xyz(0.0, 0.45, 1.05).looking_to(Vec3::new(0.0, -0.1, 1.0), Vec3::Y),
            ));
            kart.spawn((
                Headlamp,
                PointLight {
                    color: Color::srgb(1.0, 0.12, 0.08),
                    intensity: 250.0,
                    range: 2.5,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::from_xyz(0.0, 0.4, -1.05),
            ));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masts_stand_round_the_circuit_beside_the_road_never_in_a_tunnel() {
        let def = osage_kart_tracks::by_id("mountain").unwrap();
        let course = Course(osage_world_kart::Circuit::of(&def));
        let track = &course.0.track;
        let masts = masts(&course);
        assert!(masts.len() >= 15, "{}", masts.len());
        let face = track.half_width + track.verge + BARRIER;
        for (foot, aim) in &masts {
            let at = track.locate(foot.x, foot.z);
            assert!(at.lateral.abs() > face, "beyond the barrier: {at:?}");
            assert!(!on_span(&course, at.along, SpanKind::Tunnel), "not in a tunnel");
            let road = track.locate(aim.x, aim.z);
            assert!(road.lateral.abs() < track.half_width, "aimed at the road");
            assert!(foot.distance(*aim) < 30.0);
        }
    }
}
