// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Marks on the track (design/04 V1.1, D-067): what the karts' tyres leave
//! as they are driven. Black rubber on the asphalt where a rear tyre slides
//! — a drift, a slide, a spin, a stop harder than braking alone gives —
//! and brown ruts where a kart runs over the grass; each fades over
//! [`LIFE`] seconds. They join the marks baked into the road through its
//! bends ([`crate::track`]): those are the season's, these this race's.
//!
//! The marks of all karts are one mesh, built again as they are laid and
//! as they fade, a strip of quads a tyre's width; the bench draws them as
//! a game does.

use std::collections::{HashMap, VecDeque};

use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::NoFrustumCulling,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use osage_kart_model::REAR_CONTACTS;
use osage_kart_physics::{Ground, Kart};

use crate::scene::{Course, Part, Pose};

/// Seconds a mark takes to fade away.
pub const LIFE: f32 = 25.0;
/// Pieces of mark kept at most; the oldest go first.
pub const MOST: usize = 4000;
/// A tyre's mark is laid in pieces this long, m.
const PIECE: f32 = 0.35;
/// Wider than this from the last point and the kart was moved, not driven: m.
const JUMP: f32 = 3.0;
/// How often the fading is redrawn, s.
const REDRAW: f32 = 0.25;

/// A piece of mark: from one point to the next, its width, how dark, when
/// it was laid, and whether rubber or earth.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub from: Vec3,
    pub to: Vec3,
    pub width: f32,
    pub strength: f32,
    pub laid: f32,
    pub earth: bool,
}

#[derive(Resource, Default)]
pub struct Marks {
    pieces: VecDeque<Piece>,
    /// Where each kart's rear tyres last laid a mark, while they do, and
    /// its speed a frame ago.
    tyres: HashMap<Entity, ([Option<Vec3>; 2], f32)>,
    /// The mesh they are drawn in.
    mesh: Option<Handle<Mesh>>,
    redrawn: f32,
    changed: bool,
}

/// How hard a kart's rear tyres are working the surface, 0 (rolling) to 1,
/// from its state and how fast it slows (m/s²).
pub fn scrub(kart: &Kart, slowing: f32) -> f32 {
    if kart.speed.abs() < 3.0 {
        return 0.0;
    }
    let slide = ((kart.slide.abs() - 1.0) / 3.0).clamp(0.0, 1.0);
    let drift = if kart.drift != 0.0 { 0.55 + 0.45 * slide } else { 0.0 };
    let spin = (kart.spin.abs() / 3.0).clamp(0.0, 1.0);
    let braking = ((slowing - 18.0) / 15.0).clamp(0.0, 1.0);
    slide.max(drift).max(spin).max(braking)
}

/// The quads of the marks still showing at `now`, as positions and colours
/// (the darkness in the alpha), two triangles each.
pub fn quads(pieces: &VecDeque<Piece>, now: f32) -> (Vec<[f32; 3]>, Vec<[f32; 4]>) {
    let (mut positions, mut colours) = (Vec::with_capacity(pieces.len() * 6), Vec::with_capacity(pieces.len() * 6));
    for piece in pieces {
        let left = 1.0 - (now - piece.laid) / LIFE;
        if left <= 0.0 {
            continue;
        }
        let along = (piece.to - piece.from).with_y(0.0).normalize_or_zero();
        let side = Vec3::new(-along.z, 0.0, along.x) * piece.width / 2.0;
        let corners = [piece.from - side, piece.to - side, piece.to + side, piece.from + side];
        let colour = if piece.earth { [0.16, 0.11, 0.06] } else { [0.015, 0.015, 0.015] };
        let alpha = piece.strength * left * left.sqrt();
        for corner in [0, 1, 3, 3, 1, 2] {
            positions.push(corners[corner].to_array());
            colours.push([colour[0], colour[1], colour[2], alpha]);
        }
    }
    (positions, colours)
}

fn mesh_of(positions: Vec<[f32; 3]>, colours: Vec<[f32; 4]>) -> Mesh {
    let count = positions.len() as u32;
    let normals = vec![[0.0, 1.0, 0.0]; positions.len()];
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colours)
        .with_inserted_indices(Indices::U32((0..count).collect()))
}

/// The marks' mesh and material, with each new circuit (its parts are
/// cleared with it, and so are the marks).
pub fn build(
    mut commands: Commands,
    mut marks: ResMut<Marks>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    *marks = Marks::default();
    let mesh = meshes.add(mesh_of(Vec::new(), Vec::new()));
    let material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.7,
        // Over the road and its baked marks.
        depth_bias: 40.0,
        ..default()
    });
    commands.spawn((
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material),
        Transform::default(),
        bevy::light::NotShadowCaster,
        // Its bounds change as marks are laid.
        NoFrustumCulling,
        Part,
    ));
    marks.mesh = Some(mesh);
}

/// Each kart's rear tyres lay marks while they scrub; the marks fade.
pub fn lay(
    time: Res<Time>,
    course: Option<Res<Course>>,
    mut marks: ResMut<Marks>,
    mut meshes: ResMut<Assets<Mesh>>,
    karts: Query<(Entity, &Pose, &Transform)>,
) {
    let (Some(course), Some(mesh)) = (course, marks.mesh.clone()) else { return };
    let (now, seconds) = (time.elapsed_secs(), time.delta_secs());
    if seconds <= 0.0 {
        return;
    }
    let track = &course.0.track;
    let marks = &mut *marks;
    for (entity, pose, place) in &karts {
        let kart = &pose.kart;
        let (tyres, was) = marks.tyres.entry(entity).or_insert(([None, None], kart.speed));
        let slowing = (*was - kart.speed) / seconds;
        *was = kart.speed;
        let spot = track.locate(kart.x, kart.z);
        let (road, _) = track.lift(spot.along);
        // On the ground, not flying; a ghost leaves nothing.
        let grounded = (kart.height - road).abs() < 0.08 && !pose.ghost;
        let earth = track.ground(spot.lateral) == Ground::Grass;
        let strength = if earth { (kart.speed.abs() / 12.0).clamp(0.0, 0.5) } else { 0.6 * scrub(kart, slowing) };
        for (tyre, contact) in tyres.iter_mut().zip(REAR_CONTACTS) {
            if !grounded || strength < 0.05 {
                *tyre = None;
                continue;
            }
            let at = place.transform_point(contact).with_y(kart.height + 0.02);
            match *tyre {
                Some(last) if last.distance(at) > JUMP => *tyre = Some(at),
                Some(last) if last.distance(at) >= PIECE => {
                    let width = if earth { 0.32 } else { 0.24 };
                    marks.pieces.push_back(Piece { from: last, to: at, width, strength, laid: now, earth });
                    marks.changed = true;
                    *tyre = Some(at);
                }
                Some(_) => {}
                None => *tyre = Some(at),
            }
        }
    }
    while marks.pieces.len() > MOST || marks.pieces.front().is_some_and(|p| now - p.laid > LIFE) {
        marks.pieces.pop_front();
        marks.changed = true;
    }
    // Karts gone: forget their tyres.
    marks.tyres.retain(|entity, _| karts.contains(*entity));
    if marks.changed || (now - marks.redrawn > REDRAW && !marks.pieces.is_empty()) {
        let (positions, colours) = quads(&marks.pieces, now);
        if let Some(mut mesh) = meshes.get_mut(&mesh) {
            *mesh = mesh_of(positions, colours);
        }
        marks.redrawn = now;
        marks.changed = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tyres_scrub_when_they_slide_drift_spin_or_stop_hard() {
        let rolling = Kart { speed: 15.0, ..Kart::default() };
        assert_eq!(scrub(&rolling, 5.0), 0.0, "rolling, braking gently");
        assert!(scrub(&Kart { drift: 1.0, ..rolling }, 0.0) >= 0.55, "a drift");
        assert!(scrub(&Kart { slide: 4.0, ..rolling }, 0.0) > 0.9, "a slide");
        assert!(scrub(&Kart { spin: 4.0, ..rolling }, 0.0) > 0.9, "a spin");
        assert!(scrub(&rolling, 40.0) > 0.9, "a stop harder than brakes give");
        assert_eq!(scrub(&Kart { speed: 1.0, drift: 1.0, ..Kart::default() }, 0.0), 0.0, "too slow to mark");
    }

    #[test]
    fn marks_fade_and_go() {
        let piece = |laid, earth| Piece {
            from: Vec3::ZERO,
            to: Vec3::new(0.0, 0.0, 0.4),
            width: 0.25,
            strength: 0.6,
            laid,
            earth,
        };
        let pieces: VecDeque<Piece> = [piece(0.0, false), piece(10.0, true)].into();
        let (positions, colours) = quads(&pieces, 1.0);
        assert_eq!(positions.len(), 12, "two quads");
        assert!(colours[0][3] > colours[6][3] * 0.0 && colours[0][3] < 0.6, "fading: {}", colours[0][3]);
        assert!(colours[6][0] > colours[0][0], "earth is brown, rubber black");
        let (later, _) = quads(&pieces, LIFE + 5.0);
        assert_eq!(later.len(), 6, "the first has faded away");
        // A quad lies across its piece, a tyre's width.
        let xs: Vec<f32> = positions[..6].iter().map(|p| p[0]).collect();
        assert!((xs.iter().cloned().fold(f32::MIN, f32::max) - 0.125).abs() < 1e-5, "{xs:?}");
    }
}
