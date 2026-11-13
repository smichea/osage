// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Bridges and tunnels (D-077), built from the track's spans and the land
//! around them ([`osage_kart_land`]: a gorge under a bridge, a ridge on
//! each side of a tunnel).
//!
//! - A **bridge** is a concrete deck under the road and its verges, its
//!   edge a parapet on each side, on piers down to the gorge's floor every
//!   eighteen metres or so.
//! - A **tunnel** is a vault over the road from one barrier's face to the
//!   other, nine and a half metres at its crown: concrete inside, lit by a
//!   row of lamps (a light every sixteen metres, so that the road shows
//!   under the vault in daylight's exposure), rock outside where the ridge
//!   meets it, and a concrete portal at each end.

use std::f32::consts::PI;

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use osage_kart_land::{BARRIER, Frame, Land, VAULT};
use osage_kart_tracks::SpanKind;
use osage_world_kart::Circuit;

use crate::scene::Part;

/// Steps across a vault, from one side to the other.
const ARCH: usize = 14;
/// How thick a vault's shell is, m.
const SHELL: f32 = 1.6;
/// Piers stand about this far apart, m.
const PIER_EVERY: f32 = 18.0;

/// The frames of a span, `from` metres along, `length` long, and the one after.
fn stretch(frames: &[Frame], length_of_track: f32, from: f32, length: f32) -> Vec<&Frame> {
    let inside = |along: f32| (along - from).rem_euclid(length_of_track) <= length;
    let mut picked: Vec<&Frame> = Vec::new();
    for (i, frame) in frames.iter().enumerate() {
        if inside(frame.along) {
            if picked.is_empty() && i > 0 {
                picked.push(&frames[i - 1]);
            }
            picked.push(frame);
        }
    }
    // Ordered from the span's start, the wrap round the loop included.
    picked.sort_by(|a, b| {
        let into = |f: &Frame| (f.along - from + 1.0).rem_euclid(length_of_track);
        into(a).total_cmp(&into(b))
    });
    picked
}

/// A box from `a` to `b` (its middle line), `width` across and `height`
/// tall, its top `top` below the line: as a transform of a unit cube.
fn slab(a: Vec3, b: Vec3, width: f32, height: f32, below: f32) -> Transform {
    let middle = (a + b) / 2.0 - Vec3::Y * (below + height / 2.0);
    let along = b - a;
    Transform::from_translation(middle).looking_to(along.normalize_or(Vec3::Z), Vec3::Y).with_scale(Vec3::new(
        width,
        height,
        along.length() + 0.05,
    ))
}

/// A ring section of a vault between frames, as a mesh: each frame's arch
/// (an ellipse, `across` half wide and `up` high), joined; inward faces
/// for the inside, outward for the outside.
fn vault(frames: &[&Frame], across: f32, up: f32, inward: bool) -> Mesh {
    let (mut positions, mut normals, mut indices) = (Vec::new(), Vec::new(), Vec::new());
    for frame in frames {
        let right = Vec3::new(frame.right.x, 0.0, frame.right.y);
        let base = Vec3::new(frame.at.x, frame.height, frame.at.y);
        for k in 0..=ARCH {
            let angle = PI * k as f32 / ARCH as f32;
            let (c, s) = (angle.cos(), angle.sin());
            positions.push((base + right * (across * c) + Vec3::Y * (up * s)).to_array());
            let outward = (right * (c / across) + Vec3::Y * (s / up)).normalize_or(Vec3::Y);
            normals.push((if inward { -outward } else { outward }).to_array());
        }
    }
    let ring = ARCH as u32 + 1;
    for f in 0..frames.len().saturating_sub(1) as u32 {
        for k in 0..ARCH as u32 {
            let (a, b) = (f * ring + k, f * ring + k + 1);
            let (c, d) = (a + ring, b + ring);
            if inward {
                indices.extend([a, c, b, b, c, d]);
            } else {
                indices.extend([a, b, c, b, d, c]);
            }
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_indices(Indices::U32(indices))
}

/// A tunnel's portal at `frame`: a frame of concrete round the vault's mouth.
fn portal(frame: &Frame, across: f32, up: f32) -> Mesh {
    let right = Vec3::new(frame.right.x, 0.0, frame.right.y);
    let forward = Vec3::new(frame.forward.x, 0.0, frame.forward.y);
    let base = Vec3::new(frame.at.x, frame.height, frame.at.y);
    let (outer_across, outer_up) = (across + SHELL + 1.2, up + SHELL + 1.4);
    let (mut positions, mut normals, mut indices) = (Vec::new(), Vec::new(), Vec::new());
    for k in 0..=ARCH {
        let angle = PI * k as f32 / ARCH as f32;
        let (c, s) = (angle.cos(), angle.sin());
        positions.push((base + right * (across * c) + Vec3::Y * (up * s)).to_array());
        // The outside of the frame, square-shouldered.
        let edge = Vec3::new(c.signum() * outer_across * c.abs().powf(0.35), (outer_up * s.powf(0.35)).max(0.0), 0.0);
        positions.push((base + right * edge.x + Vec3::Y * edge.y).to_array());
        for _ in 0..2 {
            normals.push((-forward).to_array());
        }
    }
    for k in 0..ARCH as u32 {
        let (a, b, c, d) = (2 * k, 2 * k + 1, 2 * k + 2, 2 * k + 3);
        indices.extend([a, b, c, c, b, d, a, c, b, c, d, b]);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_indices(Indices::U32(indices))
}

/// The circuit's bridges and tunnels, as parts of it (rebuilt with it).
pub fn build(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    circuit: &Circuit,
    frames: &[Frame],
    land: &Land,
) {
    if circuit.spans.is_empty() {
        return;
    }
    let track = &circuit.track;
    let concrete = materials.add(StandardMaterial {
        base_color: Color::srgb(0.58, 0.57, 0.54),
        perceptual_roughness: 0.88,
        ..default()
    });
    let lining = materials.add(StandardMaterial {
        base_color: Color::srgb(0.42, 0.41, 0.4),
        perceptual_roughness: 0.75,
        cull_mode: None,
        ..default()
    });
    let rock = materials.add(StandardMaterial {
        base_color: if circuit.about.scenery == "canyon" {
            Color::srgb(0.55, 0.31, 0.19)
        } else {
            Color::srgb(0.43, 0.41, 0.38)
        },
        perceptual_roughness: 0.95,
        ..default()
    });
    let lamp = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.92, 0.75),
        emissive: LinearRgba::rgb(6.0, 5.2, 3.6),
        ..default()
    });
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let pillar = meshes.add(Cylinder::new(1.0, 1.0));
    let face = track.half_width + track.verge + BARRIER;
    let mut lights = Vec::new();
    let mut spawn = |mesh: Handle<Mesh>, material: &Handle<StandardMaterial>, place: Transform| {
        commands.spawn((Part, Mesh3d(mesh), MeshMaterial3d(material.clone()), place));
    };
    for &(kind, from, length) in &circuit.spans {
        let picked = stretch(frames, track.length(), from, length);
        if picked.len() < 2 {
            continue;
        }
        let point =
            |f: &Frame, lateral: f32| Vec3::new(f.at.x + f.right.x * lateral, f.height, f.at.y + f.right.y * lateral);
        match kind {
            SpanKind::Bridge => {
                let mut since_pier = PIER_EVERY / 2.0;
                for pair in picked.windows(2) {
                    let (a, b) = (pair[0], pair[1]);
                    let (pa, pb) = (point(a, 0.0), point(b, 0.0));
                    // The deck under the road, verges and barriers.
                    spawn(cube.clone(), &concrete, slab(pa, pb, 2.0 * (face + 0.6), 1.1, 0.03));
                    // A parapet on each side, its top a metre over the road.
                    for side in [-1.0f32, 1.0] {
                        let (sa, sb) = (point(a, side * (face + 0.35)), point(b, side * (face + 0.35)));
                        spawn(cube.clone(), &concrete, slab(sa + Vec3::Y * 1.05, sb + Vec3::Y * 1.05, 0.4, 1.1, 0.0));
                    }
                    // Piers down to the floor of the gorge.
                    since_pier += pa.distance(pb);
                    if since_pier >= PIER_EVERY {
                        since_pier = 0.0;
                        for side in [-0.55f32, 0.55] {
                            let foot = point(a, side * face);
                            let ground = land.height(Vec2::new(foot.x, foot.z));
                            let top = foot.y - 1.1;
                            let tall = (top - ground + 2.0).max(0.5);
                            spawn(
                                pillar.clone(),
                                &concrete,
                                Transform::from_xyz(foot.x, top - tall / 2.0, foot.z)
                                    .with_scale(Vec3::new(1.1, tall, 1.1)),
                            );
                        }
                    }
                }
            }
            SpanKind::Tunnel => {
                let (across, up) = (face + 0.5, VAULT);
                spawn(meshes.add(vault(&picked, across, up, true)), &lining, Transform::default());
                spawn(meshes.add(vault(&picked, across + SHELL, up + SHELL, false)), &rock, Transform::default());
                for end in [picked[0], picked[picked.len() - 1]] {
                    spawn(meshes.add(portal(end, across, up)), &concrete, Transform::default());
                }
                // Lamps along the crown, every eight metres or so; every
                // other one lights the road.
                let (mut since, mut count) = (0.0, 0);
                for pair in picked.windows(2) {
                    since += pair[0].at.distance(pair[1].at);
                    if since >= 8.0 {
                        since = 0.0;
                        count += 1;
                        let at = point(pair[0], 0.0) + Vec3::Y * (up - 0.25);
                        let along = (point(pair[1], 0.0) - point(pair[0], 0.0)).normalize_or(Vec3::Z);
                        spawn(
                            cube.clone(),
                            &lamp,
                            Transform::from_translation(at)
                                .looking_to(along, Vec3::Y)
                                .with_scale(Vec3::new(0.5, 0.12, 1.6)),
                        );
                        if count % 2 == 0 {
                            lights.push(at - Vec3::Y * 0.4);
                        }
                    }
                }
            }
        }
    }
    for at in lights {
        commands.spawn((
            Part,
            PointLight {
                color: Color::srgb(1.0, 0.9, 0.72),
                // A tunnel lit for drivers coming out of daylight.
                intensity: 4.0e6,
                range: 22.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_translation(at),
        ));
    }
}
