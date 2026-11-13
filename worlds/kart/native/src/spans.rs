// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Bridges and tunnels (D-077, D-080), built from the track's spans and the
//! land around them ([`osage_kart_land`]: a gorge under a bridge, a ridge on
//! each side of a tunnel).
//!
//! Everything is swept along the track's frames from a cross-section, with
//! texture coordinates in metres (photographed concrete and rock from the
//! content, at their real size) and a colour per vertex for the weather's
//! marks: concrete darker where water runs off and settles — the foot of a
//! face, the underside of a deck, streaks down a fascia — and a tunnel's
//! lining grimed by the spray of the road and sooted at its crown.
//!
//! - A **bridge** is a deck under the road and its verges; on each side one
//!   face of concrete from the deck's underside to a parapet a metre over
//!   the road, under a cornice; every eighteen metres or so a crosshead
//!   under the deck on two round columns sunk into the gorge's floor.
//! - A **tunnel** is a vault over the road from one barrier's face to the
//!   other, nine and a half metres at its crown: a concrete lining inside,
//!   faced with pale panels up to 3.4 m, lit by lamps high on the walls,
//!   one side then the other, each a light bright enough that the road shows
//!   under the vault in daylight's exposure (about 30 000 lux under it, a
//!   third of the sun's: a tunnel's entrance zone); rock outside where the
//!   ridge meets it; and a concrete portal at each end.

use std::f32::consts::PI;

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use osage_content::{GRAIN, Surface};
use osage_kart_land::{BARRIER, Frame, Land, VAULT};
use osage_kart_tracks::SpanKind;
use osage_world_kart::Circuit;

use crate::scene::Part;

/// Steps across a vault, from one side to the other.
const ARCH: usize = 24;
/// A tunnel's walls are faced with pale panels up to this height, m.
const PANELS: f32 = 3.4;
/// How thick a vault's shell is, m.
const SHELL: f32 = 1.6;
/// Piers stand about this far apart, m.
const PIER_EVERY: f32 = 18.0;
/// How thick a bridge's deck is, m.
const DECK: f32 = 1.3;
/// A parapet's thickness, and its height over the road under the cornice, m.
const PARAPET: (f32, f32) = (0.35, 0.85);
/// The cornice: how far it stands out past the face, how tall it is, m.
const CORNICE: (f32, f32) = (0.08, 0.2);
/// A pier's column: radius, m; sides round it.
const COLUMN: f32 = 0.65;
const SIDES: usize = 16;
/// The real size of the photographed sets, m (Poly Haven's dimensions).
const CONCRETE_TILE: f32 = 2.7;
const ROCK_TILE: f32 = 4.0;

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

/// A cheap, smooth noise in 0–1 along a line: the weather's streaks.
fn streaks(x: f32) -> f32 {
    let wave = (x * 0.73).sin() * 0.5 + (x * 1.91 + 1.3).sin() * 0.3 + (x * 4.37 + 0.4).sin() * 0.2;
    0.5 + 0.5 * wave
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// A mesh being built: positions, normals, texture coordinates in metres,
/// and a colour per vertex (grey: the weather's marks).
#[derive(Default)]
struct Sheet {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colours: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl Sheet {
    fn vertex(&mut self, at: Vec3, normal: Vec3, uv: Vec2, shade: f32) -> u32 {
        self.positions.push(at.to_array());
        self.normals.push(normal.to_array());
        self.uvs.push(uv.to_array());
        let s = shade.clamp(0.0, 1.0);
        self.colours.push([s, s, s, 1.0]);
        self.positions.len() as u32 - 1
    }

    /// A quad from four vertices in order round it, facing `normal`.
    fn quad(&mut self, [a, b, c, d]: [u32; 4], normal: Vec3) {
        let p = |i: u32| Vec3::from(self.positions[i as usize]);
        let facing = (p(b) - p(a)).cross(p(c) - p(a)).dot(normal);
        if facing >= 0.0 {
            self.indices.extend([a, b, c, a, c, d]);
        } else {
            self.indices.extend([a, c, b, a, d, c]);
        }
    }

    /// One straight piece of a cross-section, from `a` to `b` (metres right
    /// of the centre line, height above the road), swept along `frames`,
    /// facing `outward` in the same terms; `u` where its texture starts
    /// across; `shade` the weather's mark at a height and a distance along.
    fn sweep(
        &mut self,
        frames: &[&Frame],
        (a, b): (Vec2, Vec2),
        outward: Vec2,
        u: f32,
        shade: &impl Fn(f32, f32) -> f32,
    ) {
        let mut along = 0.0;
        let mut previous: Option<(u32, u32, Vec3)> = None;
        for (k, frame) in frames.iter().enumerate() {
            if k > 0 {
                along += frames[k - 1].at.distance(frame.at);
            }
            let right = Vec3::new(frame.right.x, 0.0, frame.right.y);
            let base = Vec3::new(frame.at.x, frame.height, frame.at.y);
            let normal = (right * outward.x + Vec3::Y * outward.y).normalize_or(Vec3::Y);
            let place = |p: Vec2| base + right * p.x + Vec3::Y * p.y;
            let across = a.distance(b);
            let i = self.vertex(place(a), normal, Vec2::new(u, along), shade(a.y, along));
            let j = self.vertex(place(b), normal, Vec2::new(u + across, along), shade(b.y, along));
            if let Some((pi, pj, pn)) = previous {
                self.quad([pi, pj, j, i], (pn + normal).normalize_or(normal));
            }
            previous = Some((i, j, normal));
        }
    }

    /// A closed outline (corners in order round it), swept: each side faces
    /// away from the outline's middle. `open` leaves out the sides that face
    /// that way (a deck's top, under the road). The texture runs round it.
    fn outline(&mut self, frames: &[&Frame], corners: &[Vec2], open: Option<Vec2>, shade: &impl Fn(f32, f32) -> f32) {
        let middle = corners.iter().copied().sum::<Vec2>() / corners.len() as f32;
        let mut u = 0.0;
        for (i, &a) in corners.iter().enumerate() {
            let b = corners[(i + 1) % corners.len()];
            let side = b - a;
            let mut outward = Vec2::new(side.y, -side.x).normalize_or(Vec2::Y);
            if outward.dot((a + b) / 2.0 - middle) < 0.0 {
                outward = -outward;
            }
            if open.is_none_or(|open| outward.dot(open) < 0.9) {
                self.sweep(frames, (a, b), outward, u, shade);
            }
            u += side.length();
        }
    }

    /// A box at `centre`, its axes `x`, `y`, `z` (unit) and half sizes.
    fn cuboid(&mut self, centre: Vec3, axes: [Vec3; 3], half: Vec3, shade: &impl Fn(Vec3) -> f32) {
        let sizes = [half.x, half.y, half.z];
        for (n, &axis) in axes.iter().enumerate() {
            for sign in [-1.0f32, 1.0] {
                let normal = axis * sign;
                let (s, t) = (axes[(n + 1) % 3], axes[(n + 2) % 3]);
                let (hs, ht) = (sizes[(n + 1) % 3], sizes[(n + 2) % 3]);
                let face = centre + normal * sizes[n];
                let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(i, j)| {
                    let at = face + s * (i * hs) + t * (j * ht);
                    self.vertex(at, normal, Vec2::new((i + 1.0) * hs, (j + 1.0) * ht), shade(at))
                });
                self.quad(corners, normal);
            }
        }
    }

    /// A round column from `foot` up to `top`, `radius` wide.
    fn column(&mut self, foot: Vec3, top: f32, radius: f32, shade: &impl Fn(Vec3) -> f32) {
        let mut ring: Vec<(u32, u32)> = Vec::new();
        for k in 0..=SIDES {
            let angle = 2.0 * PI * k as f32 / SIDES as f32;
            let normal = Vec3::new(angle.cos(), 0.0, angle.sin());
            let (low, high) = (foot + normal * radius, Vec3::new(foot.x, top, foot.z) + normal * radius);
            let u = angle * radius;
            let i = self.vertex(low, normal, Vec2::new(u, 0.0), shade(low));
            let j = self.vertex(high, normal, Vec2::new(u, top - foot.y), shade(high));
            ring.push((i, j));
        }
        for pair in ring.windows(2) {
            let ((a, b), (c, d)) = (pair[0], pair[1]);
            let middle = (Vec3::from(self.normals[a as usize]) + Vec3::from(self.normals[c as usize])) / 2.0;
            self.quad([a, c, d, b], middle);
        }
    }

    fn mesh(self) -> Mesh {
        let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colours)
            .with_inserted_indices(Indices::U32(self.indices));
        // Tangents for the normal maps; without them, flat.
        let plain = mesh.clone();
        mesh.with_generated_tangents().unwrap_or(plain)
    }
}

/// A vault between frames: each frame's arch (an ellipse, `across` half
/// wide and `up` high), joined; facing in for a lining, out for a shell.
/// The texture runs round the arch and along; the lining is grimed low by
/// the road's spray and sooted at the crown. `keep` picks the steps of the
/// arch (by the height of their top) this sheet takes.
fn vault(sheet: &mut Sheet, frames: &[&Frame], across: f32, up: f32, inward: bool, keep: impl Fn(f32) -> bool) {
    let mut along = 0.0;
    let mut rings: Vec<Vec<u32>> = Vec::new();
    for (f, frame) in frames.iter().enumerate() {
        if f > 0 {
            along += frames[f - 1].at.distance(frame.at);
        }
        let right = Vec3::new(frame.right.x, 0.0, frame.right.y);
        let base = Vec3::new(frame.at.x, frame.height, frame.at.y);
        let (mut arc, mut last) = (0.0, None::<Vec3>);
        let mut ring = Vec::new();
        for k in 0..=ARCH {
            let angle = PI * k as f32 / ARCH as f32;
            let (c, s) = (angle.cos(), angle.sin());
            let at = base + right * (across * c) + Vec3::Y * (up * s);
            if let Some(last) = last {
                arc += last.distance(at);
            }
            last = Some(at);
            let outward = (right * (c / across) + Vec3::Y * (s / up)).normalize_or(Vec3::Y);
            let height = up * s;
            let shade = if inward {
                let spray = 1.0 - 0.42 * (1.0 - smooth(height / 2.2)) * (0.8 + 0.2 * streaks(along * 0.9));
                let soot = 1.0 - 0.25 * smooth((height - 0.7 * up) / (0.3 * up));
                spray * soot
            } else {
                0.85 + 0.15 * streaks(along * 0.31 + arc * 0.17)
            };
            ring.push(sheet.vertex(at, if inward { -outward } else { outward }, Vec2::new(arc, along), shade));
        }
        rings.push(ring);
    }
    for pair in rings.windows(2) {
        for k in 0..ARCH {
            let top = |k: usize| up * (PI * k as f32 / ARCH as f32).sin();
            if !keep(top(k).max(top(k + 1))) {
                continue;
            }
            let (a, b, c, d) = (pair[0][k], pair[0][k + 1], pair[1][k + 1], pair[1][k]);
            let normal = Vec3::from(sheet.normals[a as usize]);
            sheet.quad([a, b, c, d], normal);
        }
    }
}

/// A tunnel's portal at `frame`: a concrete headwall round the vault's
/// mouth, square-shouldered, a metre deep, facing out of the tunnel
/// (`outward`: +1 ahead of the frame, -1 behind).
fn portal(sheet: &mut Sheet, frame: &Frame, across: f32, up: f32, outward: f32) {
    let right = Vec3::new(frame.right.x, 0.0, frame.right.y);
    let forward = Vec3::new(frame.forward.x, 0.0, frame.forward.y) * outward;
    let base = Vec3::new(frame.at.x, frame.height, frame.at.y);
    let (outer_across, outer_up) = (across + SHELL + 1.4, up + SHELL + 1.6);
    let mut rows: Vec<(u32, u32)> = Vec::new();
    for k in 0..=ARCH {
        let angle = PI * k as f32 / ARCH as f32;
        let (c, s) = (angle.cos(), angle.sin());
        let inner = base + right * (across * c) + Vec3::Y * (up * s);
        // The outside of the headwall: square.
        let (ex, ey) = (c.signum() * outer_across * c.abs().powf(0.3), outer_up * s.powf(0.3));
        let outer = base + right * ex + Vec3::Y * ey;
        let uv = |p: Vec3| Vec2::new((p - base).dot(right), p.y - base.y);
        let shade =
            |p: Vec3| 0.78 + 0.22 * smooth((p.y - base.y) / 3.0) * (0.85 + 0.15 * streaks((p - base).dot(right)));
        let i = sheet.vertex(inner + forward * 0.5, forward, uv(inner), shade(inner));
        let j = sheet.vertex(outer + forward * 0.5, forward, uv(outer), shade(outer));
        rows.push((i, j));
    }
    for pair in rows.windows(2) {
        let ((a, b), (c, d)) = (pair[0], pair[1]);
        sheet.quad([a, b, d, c], forward);
    }
    // Its coping along the top, standing out a little.
    let top = base + Vec3::Y * (outer_up + 0.15) + forward * 0.55;
    sheet.cuboid(top, [right, Vec3::Y, forward], Vec3::new(outer_across + 0.2, 0.18, 0.6), &|_| 0.8);
}

/// The circuit's bridges and tunnels, as parts of it (rebuilt with it).
pub fn build(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    circuit: &Circuit,
    frames: &[Frame],
    land: &Land,
) {
    if circuit.spans.is_empty() {
        return;
    }
    let track = &circuit.track;
    let root = osage_content::root();
    let photographed =
        |id: &str, images: &mut Assets<Image>| root.as_deref().and_then(|root| Surface::load(root, id, images, |p| p));
    let canyon = circuit.about.scenery == "canyon";
    // Photographed concrete and rock at their real size; plain colours
    // without the content. The rock of a canyon is red sandstone.
    let concrete = match photographed("concrete_wall_008", images) {
        Some(set) => StandardMaterial { ..set.material(CONCRETE_TILE) },
        None => StandardMaterial { base_color: Color::srgb(0.58, 0.57, 0.54), perceptual_roughness: 0.88, ..default() },
    };
    let rock_tint = if canyon { Color::srgb(1.0, 0.6, 0.42) } else { Color::srgb(0.92, 0.9, 0.86) };
    let rock = match photographed("rock_face_03", images) {
        Some(set) => StandardMaterial { base_color: rock_tint, ..set.material(ROCK_TILE) },
        None => StandardMaterial {
            base_color: if canyon { Color::srgb(0.55, 0.31, 0.19) } else { Color::srgb(0.43, 0.41, 0.38) },
            perceptual_roughness: 0.95,
            ..default()
        },
    };
    let lining = concrete.clone();
    // The panels: pale painted concrete that sends the lamps' light back
    // onto the road; matt, as the sky's light the scene reflects is not a
    // tunnel's.
    // Its photograph is of green paint: only its grain is taken, under a
    // pale cream (a grain darkens by `GRAIN` on average, so brighter).
    let grain = root.as_deref().and_then(|root| Surface::load_grain(root, "painted_concrete", images));
    let panels = match grain {
        Some(set) => StandardMaterial {
            base_color: Color::linear_rgba(0.72 / GRAIN, 0.69 / GRAIN, 0.6 / GRAIN, 1.0),
            perceptual_roughness: 0.85,
            reflectance: 0.25,
            ..set.material(2.0)
        },
        None => StandardMaterial { base_color: Color::srgb(0.85, 0.84, 0.8), perceptual_roughness: 0.85, ..default() },
    };
    let panels = materials.add(panels);
    let (concrete, lining, rock) = (materials.add(concrete), materials.add(lining), materials.add(rock));
    let lamp = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.92, 0.75),
        emissive: LinearRgba::rgb(9.0, 7.8, 5.4),
        ..default()
    });
    let housing = materials.add(StandardMaterial {
        base_color: Color::srgb(0.2, 0.21, 0.22),
        perceptual_roughness: 0.5,
        metallic: 0.6,
        ..default()
    });
    let face = track.half_width + track.verge + BARRIER;
    let (mut built, mut tunnels_rock, mut fittings, mut lamps) =
        (Sheet::default(), Sheet::default(), Sheet::default(), Sheet::default());
    let (mut lined, mut panelled) = (Sheet::default(), Sheet::default());
    let mut lights = Vec::new();
    for &(kind, from, length) in &circuit.spans {
        let picked = stretch(frames, track.length(), from, length);
        if picked.len() < 2 {
            continue;
        }
        let point =
            |f: &Frame, lateral: f32| Vec3::new(f.at.x + f.right.x * lateral, f.height, f.at.y + f.right.y * lateral);
        match kind {
            SpanKind::Bridge => {
                let edge = face + 0.6;
                // Water runs off the deck: the fascia streaked, darker low;
                // the underside darker still.
                let weather = |height: f32, along: f32| {
                    let low = 1.0 - 0.3 * (1.0 - smooth((height + DECK) / (DECK + 1.0)));
                    let streak = 0.82 + 0.18 * streaks(along * 1.3);
                    if height <= -DECK + 0.01 { 0.62 * streak } else { low * streak }
                };
                // The deck, its top under the road.
                let deck =
                    [Vec2::new(-edge, -0.02), Vec2::new(edge, -0.02), Vec2::new(edge, -DECK), Vec2::new(-edge, -DECK)];
                built.outline(&picked, &deck, Some(Vec2::Y), &weather);
                for side in [-1.0f32, 1.0] {
                    let x = |lateral: f32| side * lateral;
                    // The parapet, flush with the fascia, from the deck's top.
                    let (thick, high) = PARAPET;
                    let parapet = [
                        Vec2::new(x(edge - thick), -0.02),
                        Vec2::new(x(edge), -0.02),
                        Vec2::new(x(edge), high),
                        Vec2::new(x(edge - thick), high),
                    ];
                    built.outline(&picked, &parapet, Some(Vec2::new(0.0, -1.0)), &weather);
                    // The cornice over it, standing out a little both sides.
                    let (out, tall) = CORNICE;
                    let cornice = [
                        Vec2::new(x(edge - thick - 0.1), high),
                        Vec2::new(x(edge + out), high),
                        Vec2::new(x(edge + out), high + tall),
                        Vec2::new(x(edge - thick - 0.1), high + tall),
                    ];
                    built.outline(&picked, &cornice, None, &|_, along| 0.9 + 0.1 * streaks(along * 2.1));
                }
                // Piers: a crosshead under the deck on two columns down to
                // the gorge, sunk well in (the land's grid is coarser than a
                // gorge's slope, and may lie lower than here).
                let mut since_pier = PIER_EVERY / 2.0;
                for pair in picked.windows(2) {
                    let (a, b) = (pair[0], pair[1]);
                    since_pier += a.at.distance(b.at);
                    if since_pier < PIER_EVERY {
                        continue;
                    }
                    since_pier = 0.0;
                    let right = Vec3::new(a.right.x, 0.0, a.right.y);
                    let forward = Vec3::new(a.forward.x, 0.0, a.forward.y);
                    let under = point(a, 0.0) - Vec3::Y * (DECK + 0.6);
                    let low = |at: Vec3| 0.7 + 0.3 * smooth((at.y - under.y + 30.0) / 30.0);
                    built.cuboid(under, [right, Vec3::Y, forward], Vec3::new(edge - 0.9, 0.6, 0.9), &low);
                    for side in [-1.0f32, 1.0] {
                        let foot = point(a, side * (edge - 2.4));
                        let ground = land.height(Vec2::new(foot.x, foot.z));
                        let top = under.y - 0.6;
                        if top <= ground {
                            continue;
                        }
                        let splash =
                            |at: Vec3| 0.68 + 0.32 * smooth((at.y - ground) / 3.0) * (0.9 + 0.1 * streaks(at.y));
                        built.column(Vec3::new(foot.x, ground - 6.0, foot.z), top, COLUMN, &splash);
                    }
                }
            }
            SpanKind::Tunnel => {
                let (across, up) = (face + 0.5, VAULT);
                vault(&mut lined, &picked, across, up, true, |top| top > PANELS + 0.01);
                vault(&mut panelled, &picked, across, up, true, |top| top <= PANELS + 0.01);
                vault(&mut tunnels_rock, &picked, across + SHELL, up + SHELL, false, |_| true);
                portal(&mut built, picked[0], across, up, -1.0);
                portal(&mut built, picked[picked.len() - 1], across, up, 1.0);
                // Lamps high on the walls, every eight metres or so, one
                // side then the other, each lighting the road and the
                // panels, in a housing.
                let (mut since, mut left) = (0.0, false);
                for pair in picked.windows(2) {
                    since += pair[0].at.distance(pair[1].at);
                    if since < 8.0 {
                        continue;
                    }
                    since = 0.0;
                    left = !left;
                    // On the arch, two thirds of the way up, facing in.
                    let angle = if left { PI * 0.72 } else { PI * 0.28 };
                    let (c, sn) = (angle.cos(), angle.sin());
                    let right = Vec3::new(pair[0].right.x, 0.0, pair[0].right.y);
                    let wall = point(pair[0], across * c) + Vec3::Y * (up * sn);
                    let inward = -(right * (c / across) + Vec3::Y * (sn / up)).normalize_or(Vec3::Y);
                    let along = (point(pair[1], 0.0) - point(pair[0], 0.0)).normalize_or(Vec3::Z);
                    let side = along.cross(inward).normalize_or(right);
                    fittings.cuboid(wall + inward * 0.1, [side, inward, along], Vec3::new(0.3, 0.1, 0.9), &|_| 1.0);
                    lamps.cuboid(wall + inward * 0.21, [side, inward, along], Vec3::new(0.22, 0.02, 0.8), &|_| 1.0);
                    lights.push(wall + inward * 0.6);
                }
            }
        }
    }
    let mut spawn = |sheet: Sheet, material: &Handle<StandardMaterial>| {
        if !sheet.indices.is_empty() {
            commands.spawn((Part, Mesh3d(meshes.add(sheet.mesh())), MeshMaterial3d(material.clone())));
        }
    };
    spawn(built, &concrete);
    spawn(lined, &lining);
    spawn(panelled, &panels);
    spawn(tunnels_rock, &rock);
    spawn(fittings, &housing);
    spawn(lamps, &lamp);
    for at in lights {
        commands.spawn((
            Part,
            PointLight {
                color: Color::srgb(1.0, 0.9, 0.72),
                // A tunnel lit for drivers coming out of daylight: about
                // 30 000 lux on the road below.
                intensity: 3.0e7,
                range: 26.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_translation(at),
        ));
    }
}
