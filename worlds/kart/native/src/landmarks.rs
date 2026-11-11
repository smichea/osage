// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The landmarks of the circuit: what a player remembers it by. The start
//! with its gantry, grandstand and crowd; tyre stacks and chevron boards at
//! the bends; inflatable arches over the road; a windmill on the knoll inside
//! the loop; a lake with a jetty and a sailing boat; a fair with a big wheel
//! beside the start straight. Original designs, sculpted from simple shapes.
//!
//! A kart, and every object here placed along the road, faces +Z with its
//! left at +X: `Quat::from_rotation_y(heading)` turns it to the road.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::{light::NotShadowCaster, prelude::*};
use osage_kart_physics::Track;
use osage_world_kart::Circuit;

use osage_kart_crowd::{Stance, Wear};
use osage_procedural::{Batch, Paint, Shape};

use crate::{
    scene::PALETTE,
    scenery::{Floating, Props, Set, Turning, on_land},
    track::{self, BARRIER, Frame, Land, WATER},
};

/// Heading of a direction on the ground, as kart headings go.
fn heading(direction: Vec2) -> f32 {
    direction.x.atan2(direction.y)
}

fn upright(heading: f32) -> Quat {
    Quat::from_rotation_y(heading)
}

/// Where something `radius` wide stands at `at`: on the lowest ground under
/// it, so that on a slope it sinks into the high side rather than float
/// above the low one.
fn seat(land: &Land, at: Vec2, radius: f32) -> Vec3 {
    let lowest = (0..24)
        .map(|k| at + Vec2::from_angle(k as f32 / 12.0 * TAU) * if k < 12 { radius } else { radius / 2.0 })
        .map(|point| land.height(point))
        .fold(land.height(at), f32::min);
    Vec3::new(at.x, lowest, at.y)
}

/// A rod between two points.
fn rod(from: Vec3, to: Vec3, radius: f32) -> Shape {
    let beside = if (to - from).normalize_or(Vec3::Y).y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
    Shape::sweep(&[from, to], radius, 5, beside)
}

/// A string of pennants hanging between two points, in turns of the colours.
fn bunting(batch: &mut Batch, from: Vec3, to: Vec3, colours: &[(f32, f32, f32)]) {
    let count = (from.distance(to) / 0.9) as usize;
    let sag = |t: f32| Vec3::Y * -0.8 * 4.0 * t * (1.0 - t);
    let along = (to - from).normalize_or(Vec3::X);
    let points: Vec<Vec3> =
        (0..=count).map(|k| from.lerp(to, k as f32 / count as f32) + sag(k as f32 / count as f32)).collect();
    let anchor = (from + to) / 2.0;
    batch.add_within(
        anchor,
        &Shape::sweep(&points, 0.02, 3, Vec3::Y.cross(along) + Vec3::Y * 0.01),
        Transform::IDENTITY,
        Paint::plain(0.9, 0.9, 0.9),
    );
    for (k, pair) in points.windows(2).enumerate() {
        let (a, b) = (pair[0], pair[1]);
        let tip = (a + b) / 2.0 - Vec3::Y * 0.5;
        let (r, g, b_) = colours[k % colours.len()];
        batch.add_within(anchor, &Shape::faces(&[&[a, tip, b]], true), Transform::IDENTITY, Paint::plain(r, g, b_));
    }
}

/// The start: a gantry over the road with its lamps and banner, a grandstand
/// full of people with flags above it, and the grid's clearing. `real`: as a
/// club circuit has them — a galvanised gantry, plastic seats on concrete
/// tiers under a sheet-metal roof, spectators in everyday clothes, no flags.
pub fn start(set: &mut Set, props: &mut Props, circuit: &Circuit, land: &Land, real: bool) {
    let track = &circuit.track;
    let (half, verge) = (track.half_width, track.verge);
    let (line, line_heading) = track.at(circuit.start);
    let line = Vec2::from(line);
    set.clear(line, 55.0);
    let across_road = Transform::from_xyz(line.x, 0.0, line.y).with_rotation(upright(line_heading));
    let steel = Paint::blend((0.55, 0.57, 0.6), (0.85, 0.86, 0.9));

    // The gantry: two pillars, a beam, five lamps, a chequered banner.
    let span = 2.0 * (half + 1.8);
    for side in [-1.0, 1.0] {
        let pillar = Shape::block(Vec3::new(-0.3, 0.0, -0.3), Vec3::new(0.3, 6.4, 0.3));
        set.satin.add(&pillar, across_road * Transform::from_xyz(side * span / 2.0, 0.0, 0.0), steel);
    }
    let beam = Shape::block(Vec3::new(-span / 2.0 - 0.3, 6.0, -0.35), Vec3::new(span / 2.0 + 0.3, 7.2, 0.35));
    let beam_paint = if real { Paint::shaded(0.62, 0.64, 0.66, 0.2) } else { Paint::plain(0.86, 0.12, 0.12) };
    set.satin.add_within(across_road.translation, &beam, across_road, beam_paint);
    let banner = props.materials.add(StandardMaterial {
        base_color_texture: Some(props.images.add(track::chequers(24, 2, [245, 245, 245], [20, 20, 22]))),
        perceptual_roughness: 0.7,
        ..default()
    });
    // It faces the grid, which is behind the line.
    let face = across_road * Transform::from_xyz(0.0, 6.6, -0.37).with_rotation(Quat::from_rotation_y(PI));
    props.commands.spawn((
        crate::scene::Part,
        Mesh3d(props.meshes.add(Rectangle::new(span, 1.0))),
        MeshMaterial3d(banner),
        face,
    ));
    let glow = props.materials.add(StandardMaterial {
        base_color: Color::BLACK,
        emissive: LinearRgba::rgb(0.75, 5.0, 1.25),
        emissive_exposure_weight: 0.0,
        ..default()
    });
    // The start lights: flat lamps in a black housing under the beam, each
    // under its visor, facing the grid; on a photographed circuit, not bulbs.
    let mut lamps = Batch::default();
    if real {
        let housing = Shape::block(Vec3::new(-3.0, 5.35, -0.42), Vec3::new(3.0, 6.0, -0.2));
        set.satin.add_within(across_road.translation, &housing, across_road, Paint::plain(0.04, 0.04, 0.045));
        let lens = Shape::new(Cylinder::new(0.2, 0.02).mesh().resolution(24));
        let visor = Shape::gore(&[(0.24, 0.0), (0.24, 0.14)], 12, -FRAC_PI_2 - 1.2, 2.4);
        for i in -2..=2 {
            let at = Transform::from_xyz(i as f32 * 1.1, 5.68, -0.43).with_rotation(Quat::from_rotation_x(FRAC_PI_2));
            lamps.add(&lens, at, Paint::plain(1.0, 1.0, 1.0));
            let hood = across_road * at * Transform::from_xyz(0.0, -0.01, 0.0).with_rotation(Quat::from_rotation_x(PI));
            set.satin.add_within(across_road.translation, &visor, hood, Paint::plain(0.04, 0.04, 0.045));
        }
    } else {
        let bulb = Shape::new(Sphere::new(0.26).mesh().ico(2).expect("a small icosphere"));
        for i in -2..=2 {
            lamps.add(&bulb, Transform::from_xyz(i as f32 * 1.1, 5.7, -0.3), Paint::plain(1.0, 1.0, 1.0));
        }
    }
    let lamps = props.meshes.add(lamps.merged());
    props.commands.spawn((crate::scene::Part, Mesh3d(lamps), MeshMaterial3d(glow), across_road, NotShadowCaster));

    // The grandstand, on the right of the grid: the tiers step away from the
    // road towards -X.
    let (stand_at, stand_heading) = track.at(circuit.start - 16.0);
    let stand = Transform::from_xyz(stand_at[0], 0.0, stand_at[1]).with_rotation(upright(stand_heading));
    let (length, tiers, depth, rise) = (44.0, 6, 1.5, 0.55);
    let front = -(half + verge + 3.5);
    let back = front - tiers as f32 * depth;
    let concrete = Paint::shaded(0.82, 0.82, 0.8, 0.25);
    let body = Shape::lathe(&[(0.0, 0.0), (0.22, 0.04), (0.25, 0.42), (0.15, 0.6), (0.0, 0.64)], 6);
    let head = Shape::new(Sphere::new(0.15).mesh().ico(1).expect("a small icosphere"));
    let skins = [(0.98, 0.8, 0.66), (0.86, 0.64, 0.46), (0.62, 0.42, 0.28), (0.42, 0.28, 0.2)];
    let shirts: Vec<(f32, f32, f32)> =
        PALETTE.iter().copied().chain([(0.95, 0.95, 0.95), (0.95, 0.85, 0.2), (0.2, 0.25, 0.4)]).collect();
    if real {
        grandstand(set, stand, (front, back, length, tiers, depth, rise));
        pits(set, props, circuit);
        let _ = land;
        return;
    }
    for k in 0..tiers {
        let (x, y) = (front - k as f32 * depth, 0.3 + k as f32 * rise);
        let tier = Shape::block(
            Vec3::new(x - depth / 2.0, 0.0, -length / 2.0),
            Vec3::new(x + depth / 2.0, y + rise / 2.0, length / 2.0),
        );
        set.satin.add_within(stand.translation, &tier, stand, concrete);
        for seat in 0..34 {
            if set.dice.roll() < 0.15 {
                continue; // an empty seat
            }
            let z = -length / 2.0 + 0.9 + seat as f32 * 1.26 + set.dice.between(-0.15, 0.15);
            let (shirt, skin) = (set.dice.pick(&shirts), set.dice.pick(&skins));
            let seated = stand * Transform::from_xyz(x + 0.2, y + rise / 2.0, z);
            // Some stand and cheer.
            let lift = if set.dice.roll() < 0.25 { 0.2 } else { 0.0 };
            set.matte.add_within(
                stand.translation,
                &body,
                seated * Transform::from_xyz(0.0, lift, 0.0),
                Paint::shaded(shirt.0, shirt.1, shirt.2, 0.3),
            );
            let face = seated * Transform::from_xyz(0.0, lift + 0.76, 0.0);
            set.matte.add_within(stand.translation, &head, face, Paint::plain(skin.0, skin.1, skin.2));
        }
    }
    // A striped canopy on posts, flags along its edge.
    let top = tiers as f32 * rise + 3.4;
    for z in [-length / 2.0, 0.0, length / 2.0] {
        for x in [back + 0.2, front + 0.4] {
            set.satin.add_within(
                stand.translation,
                &rod(Vec3::new(x, 0.0, z), Vec3::new(x, top, z), 0.14),
                stand,
                steel,
            );
        }
    }
    let stripes = 11;
    for k in 0..stripes {
        let (z0, z1) = (
            -length / 2.0 - 0.5 + k as f32 * (length + 1.0) / stripes as f32,
            -length / 2.0 - 0.5 + (k + 1) as f32 * (length + 1.0) / stripes as f32,
        );
        let corners = [
            Vec3::new(back - 0.5, top + 0.4, z0),
            Vec3::new(back - 0.5, top + 0.4, z1),
            Vec3::new(front + 1.4, top, z1),
            Vec3::new(front + 1.4, top, z0),
        ];
        let colour = if k % 2 == 0 { Paint::plain(0.86, 0.14, 0.14) } else { Paint::plain(0.97, 0.97, 0.95) };
        set.satin.add_within(stand.translation, &Shape::faces(&[&corners], true), stand, colour);
    }
    let mut flags = Batch::default();
    bunting(
        &mut flags,
        Vec3::new(front + 1.4, top - 0.1, -length / 2.0),
        Vec3::new(front + 1.4, top - 0.1, length / 2.0),
        &PALETTE,
    );
    for mesh in flags.meshes() {
        let mesh = props.meshes.add(mesh);
        props.commands.spawn((crate::scene::Part, Mesh3d(mesh), MeshMaterial3d(props.satin.clone()), stand));
    }
    let _ = land;
}

/// A club circuit's grandstand: concrete tiers with rows of plastic seats,
/// a handrail at the front, a sheet-metal roof on steel posts, and people
/// here and there in everyday clothes. `shape`: (front, back, length,
/// tiers, depth, rise), with the tiers stepping away towards -X.
fn grandstand(set: &mut Set, stand: Transform, shape: (f32, f32, f32, usize, f32, f32)) {
    let (front, back, length, tiers, depth, rise) = shape;
    let concrete = Paint::shaded(0.6, 0.6, 0.58, 0.3);
    let steel = Paint::shaded(0.55, 0.57, 0.6, 0.2);
    let seat_colour = Paint::plain(0.12, 0.27, 0.55);
    let seat = Shape::block(Vec3::new(-0.2, 0.0, -0.21), Vec3::new(0.2, 0.06, 0.21));
    let backrest = Shape::block(Vec3::new(-0.04, 0.0, -0.21), Vec3::new(0.04, 0.38, 0.21));
    for k in 0..tiers {
        let (x, y) = (front - k as f32 * depth, 0.3 + k as f32 * rise);
        let tier = Shape::block(
            Vec3::new(x - depth / 2.0, 0.0, -length / 2.0),
            Vec3::new(x + depth / 2.0, y + rise / 2.0, length / 2.0),
        );
        set.concrete.add_within(stand.translation, &tier, stand, concrete);
        let step = y + rise / 2.0;
        for place in 0..42 {
            let z = -length / 2.0 + 0.7 + place as f32 * 1.02;
            // Seats face the road (+X): the backrest behind, towards -X.
            let at = stand * Transform::from_xyz(x + 0.15, step + 0.42, z);
            set.satin.add_within(stand.translation, &seat, at, seat_colour);
            set.satin.add_within(stand.translation, &backrest, at * Transform::from_xyz(-0.2, 0.0, 0.0), seat_colour);
            if set.dice.roll() > 0.45 {
                continue;
            }
            // A whole figure, in one of a few poses, dressed at random.
            let (shirt, skin) = (set.dice.pick(&CLOTHES), set.dice.pick(&SKINS));
            let (trousers, hair, shoe) = (set.dice.pick(&CLOTHES), set.dice.pick(&HAIRS), set.dice.pick(&SHOES));
            let pose = set.dice.pick(&osage_kart_crowd::Pose::ALL);
            let turn = Quat::from_rotation_y(set.dice.between(-0.25, 0.25));
            let seated = at * Transform::from_xyz(-0.05, 0.06, 0.0).with_rotation(turn);
            for (wear, shape) in osage_kart_crowd::seated(pose) {
                let paint = match wear {
                    Wear::Shirt => Paint::shaded(shirt.0, shirt.1, shirt.2, 0.2),
                    Wear::Trousers => Paint::plain(trousers.0 * 0.8, trousers.1 * 0.8, trousers.2 * 0.8),
                    Wear::Skin => Paint::plain(skin.0, skin.1, skin.2),
                    Wear::Hair => Paint::plain(hair.0, hair.1, hair.2),
                    Wear::Shoes => Paint::plain(shoe.0, shoe.1, shoe.2),
                };
                set.crowd.add_within(stand.translation, shape, seated, paint);
            }
        }
    }
    // A handrail along the front.
    for z in [-length / 2.0, length / 2.0] {
        let post = rod(Vec3::new(front + 0.9, 0.0, z), Vec3::new(front + 0.9, 1.05, z), 0.03);
        set.satin.add_within(stand.translation, &post, stand, steel);
    }
    let rail = rod(Vec3::new(front + 0.9, 1.05, -length / 2.0), Vec3::new(front + 0.9, 1.05, length / 2.0), 0.03);
    set.satin.add_within(stand.translation, &rail, stand, steel);
    // A sheet-metal roof on steel posts, sloping to the back.
    let top = tiers as f32 * rise + 3.4;
    for z in [-length / 2.0, -length / 6.0, length / 6.0, length / 2.0] {
        let post = rod(Vec3::new(back + 0.2, 0.0, z), Vec3::new(back + 0.2, top + 0.4, z), 0.12);
        set.satin.add_within(stand.translation, &post, stand, steel);
    }
    let roof = [
        Vec3::new(back - 0.6, top + 0.5, -length / 2.0 - 0.6),
        Vec3::new(back - 0.6, top + 0.5, length / 2.0 + 0.6),
        Vec3::new(front + 1.6, top + 0.1, length / 2.0 + 0.6),
        Vec3::new(front + 1.6, top + 0.1, -length / 2.0 - 0.6),
    ];
    set.satin.add_within(stand.translation, &Shape::faces(&[&roof], true), stand, Paint::shaded(0.66, 0.68, 0.7, 0.15));
    // Its ribs, every metre.
    let mut z = -length / 2.0 - 0.5;
    while z < length / 2.0 + 0.6 {
        let rib = rod(Vec3::new(back - 0.6, top + 0.53, z), Vec3::new(front + 1.6, top + 0.13, z), 0.025);
        set.satin.add_within(stand.translation, &rib, stand, Paint::plain(0.6, 0.62, 0.64));
        z += 1.0;
    }
}

/// The pit building on the other side of the start straight, behind the
/// fence: a storey of light concrete with six numbered roller doors facing
/// the track under a canopy, a sign band over them, a flat roof with its
/// parapet, race control above the middle behind a band of dark glass,
/// windows at the ends, and downpipes at the corners.
fn pits(set: &mut Set, props: &mut Props, circuit: &Circuit) {
    let track = &circuit.track;
    let (at, heading) = track.at(circuit.start + 14.0);
    // Local +X is the left of the track, where the grandstand is not.
    let frame = Transform::from_xyz(at[0], 0.0, at[1]).with_rotation(upright(heading));
    let (front, depth, length, height) = (track.half_width + track.verge + BARRIER + 4.6, 8.0, 38.0, 3.6);
    let concrete = Paint::shaded(0.74, 0.73, 0.7, 0.2);
    let building = Shape::block(Vec3::new(front, 0.0, -length / 2.0), Vec3::new(front + depth, height, length / 2.0));
    set.concrete.add_within(frame.translation, &building, frame, concrete);
    let mut add = |shape: Shape, paint: Paint| set.satin.add_within(frame.translation, &shape, frame, paint);
    // The parapet around the flat roof.
    for (low, high) in [
        (
            Vec3::new(front - 0.05, height, -length / 2.0 - 0.05),
            Vec3::new(front + 0.2, height + 0.45, length / 2.0 + 0.05),
        ),
        (
            Vec3::new(front + depth - 0.2, height, -length / 2.0 - 0.05),
            Vec3::new(front + depth + 0.05, height + 0.45, length / 2.0 + 0.05),
        ),
        (Vec3::new(front, height, -length / 2.0 - 0.05), Vec3::new(front + depth, height + 0.45, -length / 2.0 + 0.2)),
        (Vec3::new(front, height, length / 2.0 - 0.2), Vec3::new(front + depth, height + 0.45, length / 2.0 + 0.05)),
    ] {
        set.concrete.add_within(frame.translation, &Shape::block(low, high), frame, Paint::plain(0.66, 0.65, 0.62));
    }
    // A sign band over the doors: white, a red stripe along its foot.
    add(
        Shape::block(Vec3::new(front - 0.08, 2.95, -length / 2.0 + 0.4), Vec3::new(front, 3.45, length / 2.0 - 0.4)),
        Paint::plain(0.92, 0.92, 0.9),
    );
    add(
        Shape::block(
            Vec3::new(front - 0.09, 2.95, -length / 2.0 + 0.4),
            Vec3::new(front - 0.01, 3.05, length / 2.0 - 0.4),
        ),
        Paint::plain(0.75, 0.1, 0.08),
    );
    // Six roller doors, ribbed sheet steel, facing the track (-X).
    let (door, gap) = (4.4, (length - 6.0 * 4.4) / 7.0);
    // Two garages open, dark inside, their shutters rolled up into a drum.
    let open = [1, 4];
    for k in 0..6 {
        let z0 = -length / 2.0 + gap + k as f32 * (door + gap);
        if open.contains(&k) {
            add(
                Shape::block(Vec3::new(front - 0.02, 0.0, z0), Vec3::new(front, 2.75, z0 + door)),
                Paint::blend((0.05, 0.05, 0.05), (0.02, 0.02, 0.02)),
            );
            add(
                Shape::block(Vec3::new(front - 0.25, 2.5, z0 - 0.05), Vec3::new(front, 2.8, z0 + door + 0.05)),
                Paint::plain(0.45, 0.47, 0.5),
            );
            continue;
        }
        add(
            Shape::block(Vec3::new(front - 0.04, 0.0, z0), Vec3::new(front, 2.75, z0 + door)),
            Paint::shaded(0.5, 0.52, 0.55, 0.15),
        );
        for rib in 0..14 {
            let y = 0.12 + rib as f32 * 0.19;
            add(
                Shape::block(
                    Vec3::new(front - 0.06, y, z0 + 0.05),
                    Vec3::new(front - 0.04, y + 0.03, z0 + door - 0.05),
                ),
                Paint::plain(0.58, 0.6, 0.63),
            );
        }
    }
    // Downpipes at the front corners.
    for z in [-length / 2.0 + 0.15, length / 2.0 - 0.15] {
        add(
            rod(Vec3::new(front - 0.12, 0.0, z), Vec3::new(front - 0.12, height + 0.3, z), 0.05),
            Paint::plain(0.4, 0.42, 0.44),
        );
    }
    // Each door's number on the sign band, facing the track.
    let facing_track = Quat::from_rotation_y(-FRAC_PI_2);
    for k in 0..6 {
        let z = -length / 2.0 + gap + k as f32 * (door + gap) + door / 2.0;
        let at = frame * Transform::from_xyz(front - 0.09, 3.22, z).with_rotation(facing_track);
        for digit in osage_kart_model::numerals(k + 1, 0.32, 0.02) {
            set.satin.add_within(frame.translation, &digit, at, Paint::plain(0.08, 0.08, 0.09));
        }
    }
    // Race control above the middle: a second storey set back from the
    // front, a band of dark glass along it between slim mullions, a roof
    // slab that overhangs it.
    let (control, upper) = (9.0, 2.8);
    let storey =
        Shape::block(Vec3::new(front + 1.0, height, -control), Vec3::new(front + depth - 0.4, height + upper, control));
    set.concrete.add_within(frame.translation, &storey, frame, concrete);
    let slab = Shape::block(
        Vec3::new(front + 0.6, height + upper, -control - 0.4),
        Vec3::new(front + depth - 0.2, height + upper + 0.22, control + 0.4),
    );
    set.concrete.add_within(frame.translation, &slab, frame, Paint::plain(0.7, 0.69, 0.66));
    let pane = Shape::block(
        Vec3::new(front + 0.96, height + 0.5, -control + 0.3),
        Vec3::new(front + 1.0, height + upper - 0.35, control - 0.3),
    );
    set.glass.add_within(frame.translation, &pane, frame, Paint::plain(0.06, 0.07, 0.08));
    let mut z = -control + 0.3;
    while z <= control - 0.25 {
        let mullion = Shape::block(
            Vec3::new(front + 0.93, height + 0.45, z - 0.04),
            Vec3::new(front + 0.97, height + upper - 0.3, z + 0.04),
        );
        set.satin.add_within(frame.translation, &mullion, frame, Paint::plain(0.22, 0.23, 0.25));
        z += 1.5;
    }
    // A canopy over the pit apron, on slim posts.
    let reach = 2.4;
    set.satin.add_within(
        frame.translation,
        &Shape::block(Vec3::new(front - reach, 3.5, -length / 2.0 + 0.4), Vec3::new(front, 3.62, length / 2.0 - 0.4)),
        frame,
        Paint::shaded(0.82, 0.82, 0.8, 0.3),
    );
    let mut z = -length / 2.0 + 0.8;
    while z <= length / 2.0 - 0.7 {
        let post = rod(Vec3::new(front - reach + 0.2, 0.0, z), Vec3::new(front - reach + 0.2, 3.5, z), 0.06);
        set.satin.add_within(frame.translation, &post, frame, Paint::plain(0.5, 0.52, 0.55));
        z += (length - 1.5) / 6.0;
    }
    // Before each open garage, a kart parked along the building, its driver
    // waiting in it, and a mechanic in the team's colour beside it.
    for (k, colour) in open.iter().zip([3usize, 6]) {
        let z = -length / 2.0 + gap + *k as f32 * (door + gap) + door / 2.0;
        let along = frame * Transform::from_xyz(front - 1.25, 0.0, z).with_rotation(Quat::from_rotation_y(PI));
        props.commands.spawn((crate::scene::Part, crate::karts::Parked { colour }, along));
        let (r, g, b) = PALETTE[colour];
        let mechanic =
            frame * Transform::from_xyz(front - 0.6, 0.0, z + 1.4).with_rotation(Quat::from_rotation_y(PI * 0.85));
        person(set, Stance::ArmsFolded, mechanic, (r, g, b), (0.12, 0.12, 0.14));
    }
    // Windows at both ends of the ground floor.
    for end in [-1.0f32, 1.0] {
        for x in [front + 2.0, front + 5.0] {
            let window = Shape::block(
                Vec3::new(x - 0.6, 1.1, end * (length / 2.0) - 0.02),
                Vec3::new(x + 0.6, 2.3, end * (length / 2.0) + 0.02),
            );
            set.glass.add_within(frame.translation, &window, frame, Paint::plain(0.06, 0.07, 0.08));
        }
    }
}

/// Advertising boards behind the walls where the circuit bends gently, from
/// 60 m past the start: three metres wide, each in two colours with an abstract mark —
/// a disc and bars standing for a name. No real brand.
pub fn hoardings(set: &mut Set, circuit: &Circuit, frames: &[Frame]) {
    let track = &circuit.track;
    let (length, offset) = (track.length(), track.half_width + track.verge + BARRIER + 1.2);
    let schemes = [
        ((0.1, 0.16, 0.38), (0.95, 0.95, 0.93)),
        ((0.95, 0.78, 0.1), (0.08, 0.08, 0.09)),
        ((0.78, 0.1, 0.08), (0.96, 0.96, 0.94)),
        ((0.94, 0.94, 0.92), (0.1, 0.35, 0.2)),
        ((0.08, 0.08, 0.09), (0.9, 0.42, 0.08)),
    ];
    let board = Shape::block(Vec3::new(-1.5, 0.0, -0.03), Vec3::new(1.5, 0.9, 0.03));
    let disc = Shape::new(Cylinder::new(0.28, 0.02).mesh().resolution(20));
    let bar = |w: f32| Shape::block(Vec3::new(0.0, -0.05, 0.0), Vec3::new(w, 0.05, 0.02));
    let leg = Shape::block(Vec3::new(-0.03, 0.0, -0.03), Vec3::new(0.03, 0.35, 0.03));
    let mut since = 0.0;
    for (i, frame) in frames.iter().enumerate() {
        let from_start = (frame.along - circuit.start + length / 2.0).rem_euclid(length) - length / 2.0;
        let next = &frames[(i + 1) % frames.len()];
        since += frame.at.distance(next.at);
        // The circuit is short and winding: boards stand wherever it bends gently.
        if from_start.abs() < 60.0 || frame.bend.abs() > 0.015 || since < 3.4 {
            continue;
        }
        since = 0.0;
        let k = (frame.along / 3.4) as usize;
        let side = if k % 7 < 4 { -1.0 } else { 1.0 };
        let at = frame.at + frame.right * side * offset;
        // Facing the track: its front (+Z) towards the centre line.
        let facing = heading(-frame.right * side);
        let place = Transform::from_xyz(at.x, 0.35, at.y).with_rotation(upright(facing));
        let (ground, mark) = schemes[(k * 3 + 1) % schemes.len()];
        let paint = |c: (f32, f32, f32)| Paint::plain(c.0, c.1, c.2);
        set.satin.add_within(place.translation, &board, place, paint(ground));
        let face = place * Transform::from_xyz(0.0, 0.45, 0.03);
        set.satin.add_within(
            place.translation,
            &disc,
            face * Transform::from_xyz(-0.95, 0.0, 0.0).with_rotation(Quat::from_rotation_x(FRAC_PI_2)),
            paint(mark),
        );
        for (y, w) in [(0.1, 1.6), (-0.12, 1.1)] {
            set.satin.add_within(place.translation, &bar(w), face * Transform::from_xyz(-0.5, y, 0.0), paint(mark));
        }
        for x in [-1.3, 1.3] {
            set.satin.add_within(
                place.translation,
                &leg,
                place * Transform::from_xyz(x, -0.35, 0.0),
                Paint::plain(0.45, 0.46, 0.48),
            );
        }
    }
}

/// Chain-link fences along the straights near the start, between the wall
/// and the spectators: galvanised posts every 2.5 m, a top rail, and wire
/// woven in diamonds, drawn as a texture with see-through holes.
pub fn fences(set: &mut Set, props: &mut Props, circuit: &Circuit, frames: &[Frame]) {
    let track = &circuit.track;
    let (length, offset, top) = (track.length(), track.half_width + track.verge + BARRIER + 1.9, 2.3);
    let near_start = |frame: &Frame| {
        let from = (frame.along - circuit.start + length / 2.0).rem_euclid(length) - length / 2.0;
        from.abs() < 110.0 && frame.bend.abs() < 0.006
    };
    let wire = props.materials.add(StandardMaterial {
        base_color_texture: Some(props.images.add(chain_link())),
        alpha_mode: AlphaMode::Mask(0.5),
        double_sided: true,
        cull_mode: None,
        metallic: 0.6,
        perceptual_roughness: 0.45,
        uv_transform: bevy::math::Affine2::from_scale(Vec2::splat(1.0 / 0.12)),
        ..default()
    });
    let steel = Paint::shaded(0.62, 0.64, 0.66, 0.15);
    for side in [-1.0f32, 1.0] {
        let (mut positions, mut uvs) = (Vec::new(), Vec::new());
        let mut since_post = f32::MAX;
        for (i, frame) in frames.iter().enumerate() {
            let next = &frames[(i + 1) % frames.len()];
            if !near_start(frame) || !near_start(next) {
                since_post = f32::MAX;
                continue;
            }
            let (a, b) = (frame.at + frame.right * side * offset, next.at + next.right * side * offset);
            let (u0, u1) = (frame.along, frame.along + a.distance(b));
            for (p, u, y) in [(a, u0, 0.05), (b, u1, 0.05), (b, u1, top), (a, u0, 0.05), (b, u1, top), (a, u0, top)] {
                positions.push([p.x, y, p.y]);
                uvs.push([u, y]);
            }
            let rail = osage_procedural::Shape::sweep(
                &[Vec3::new(a.x, top, a.y), Vec3::new(b.x, top, b.y)],
                0.022,
                8,
                Vec3::Y,
            );
            set.satin.add(&rail, Transform::IDENTITY, steel);
            since_post += a.distance(b);
            if since_post >= 2.5 {
                since_post = 0.0;
                set.satin.add(
                    &rod(Vec3::new(a.x, 0.0, a.y), Vec3::new(a.x, top + 0.05, a.y), 0.03),
                    Transform::IDENTITY,
                    steel,
                );
            }
        }
        if positions.is_empty() {
            continue;
        }
        let count = positions.len();
        let mesh = Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; count])
            .with_computed_flat_normals();
        props.commands.spawn((
            crate::scene::Part,
            Mesh3d(props.meshes.add(mesh)),
            MeshMaterial3d(wire.clone()),
            NotShadowCaster,
        ));
    }
}

/// The clothes people wear at a summer race: (shirt, trousers) pairs drawn
/// from these, skin, hair and shoes.
const CLOTHES: [(f32, f32, f32); 8] = [
    (0.12, 0.15, 0.24),
    (0.85, 0.85, 0.83),
    (0.25, 0.26, 0.28),
    (0.55, 0.12, 0.1),
    (0.62, 0.55, 0.42),
    (0.08, 0.08, 0.09),
    (0.3, 0.38, 0.52),
    (0.4, 0.42, 0.3),
];
const SKINS: [(f32, f32, f32); 4] = [(0.86, 0.68, 0.56), (0.74, 0.55, 0.42), (0.52, 0.36, 0.25), (0.34, 0.23, 0.16)];
const HAIRS: [(f32, f32, f32); 5] =
    [(0.08, 0.06, 0.05), (0.25, 0.16, 0.09), (0.55, 0.42, 0.24), (0.6, 0.6, 0.58), (0.12, 0.1, 0.09)];
const SHOES: [(f32, f32, f32); 3] = [(0.9, 0.9, 0.88), (0.1, 0.1, 0.11), (0.35, 0.25, 0.18)];

/// The turn about the vertical that makes a figure (facing +X) face `towards`.
fn facing(towards: Vec2) -> Quat {
    Quat::from_rotation_y((-towards.y).atan2(towards.x))
}

/// Someone standing at `place`, dressed in `shirt` and `trousers`, the rest
/// picked by the set's dice.
fn person(set: &mut Set, stance: Stance, place: Transform, shirt: (f32, f32, f32), trousers: (f32, f32, f32)) {
    let (skin, hair, shoe) = (set.dice.pick(&SKINS), set.dice.pick(&HAIRS), set.dice.pick(&SHOES));
    for (wear, shape) in osage_kart_crowd::standing(stance) {
        let paint = match wear {
            Wear::Shirt => Paint::shaded(shirt.0, shirt.1, shirt.2, 0.2),
            Wear::Trousers => Paint::plain(trousers.0 * 0.8, trousers.1 * 0.8, trousers.2 * 0.8),
            Wear::Skin => Paint::plain(skin.0, skin.1, skin.2),
            Wear::Hair => Paint::plain(hair.0, hair.1, hair.2),
            Wear::Shoes => Paint::plain(shoe.0, shoe.1, shoe.2),
        };
        set.standing.add_within(place.translation, shape, place, paint);
    }
}

/// People standing behind the fences along the start straight, beyond the
/// grandstand and the pits, in small groups, facing the track.
pub fn onlookers(set: &mut Set, circuit: &Circuit, land: &Land, frames: &[Frame]) {
    let track = &circuit.track;
    let (length, fence) = (track.length(), track.half_width + track.verge + BARRIER + 1.9);
    let mut since = 0.0;
    for (i, frame) in frames.iter().enumerate() {
        let next = &frames[(i + 1) % frames.len()];
        since += frame.at.distance(next.at);
        let from = (frame.along - circuit.start + length / 2.0).rem_euclid(length) - length / 2.0;
        if !(42.0..105.0).contains(&from.abs()) || frame.bend.abs() > 0.006 || since < set.dice.between(5.0, 11.0) {
            continue;
        }
        since = 0.0;
        for side in [-1.0f32, 1.0] {
            if set.dice.roll() < 0.35 {
                continue;
            }
            let towards = -frame.right * side;
            let along = (next.at - frame.at).normalize_or_zero();
            let back = set.dice.between(0.7, 1.8);
            for k in 0..(1 + (set.dice.roll() * 3.5) as usize) {
                let at = frame.at
                    + frame.right * side * (fence + back + set.dice.between(0.0, 0.5))
                    + along * (k as f32 * set.dice.between(0.6, 0.9));
                let stance = set.dice.pick(&[Stance::Watching, Stance::Watching, Stance::ArmsFolded, Stance::Waving]);
                let turn = facing(towards) * Quat::from_rotation_y(set.dice.between(-0.4, 0.4));
                let place = Transform::from_xyz(at.x, land.height(at), at.y).with_rotation(turn);
                let (shirt, trousers) = (set.dice.pick(&CLOTHES), set.dice.pick(&CLOTHES));
                person(set, stance, place, shirt, trousers);
                set.clear(at, 2.5);
            }
        }
    }
}

/// Marshal posts at the sharpest bends, behind the barrier on the outside:
/// a white post with an orange band and its number facing the track, a
/// red extinguisher, and a marshal in orange holding a yellow flag.
pub fn marshals(set: &mut Set, circuit: &Circuit, land: &Land, frames: &[Frame]) {
    let track = &circuit.track;
    let wall = track.half_width + track.verge + BARRIER;
    // The sharpest point of each bend, bends at least 60 m apart.
    let mut posts: Vec<usize> = Vec::new();
    let mut order: Vec<usize> = (0..frames.len()).filter(|&i| frames[i].bend.abs() > 0.02).collect();
    order.sort_by(|&a, &b| frames[b].bend.abs().total_cmp(&frames[a].bend.abs()));
    for i in order {
        if posts.iter().all(|&j| (frames[i].along - frames[j].along).abs() > 60.0) && posts.len() < 6 {
            posts.push(i);
        }
    }
    posts.sort_unstable();
    for (n, &i) in posts.iter().enumerate() {
        let frame = &frames[i];
        let side = frame.bend.signum();
        let at = frame.at + frame.right * side * (wall + 2.4);
        let towards = -frame.right * side;
        let ground = land.height(at);
        let post = Transform::from_xyz(at.x, ground, at.y).with_rotation(facing(towards));
        set.clear(at, 4.0);
        let white = Paint::shaded(0.9, 0.9, 0.88, 0.2);
        let block = Shape::block(Vec3::new(-0.45, 0.0, -0.45), Vec3::new(0.45, 1.15, 0.45));
        set.concrete.add_within(post.translation, &block, post, white);
        set.satin.add_within(
            post.translation,
            &Shape::block(Vec3::new(-0.46, 0.9, -0.46), Vec3::new(0.46, 1.1, 0.46)),
            post,
            Paint::plain(0.95, 0.42, 0.05),
        );
        for digit in osage_kart_model::numerals(n as u32 + 1, 0.4, 0.02) {
            let face = post * Transform::from_xyz(0.46, 0.5, 0.0).with_rotation(Quat::from_rotation_y(FRAC_PI_2));
            set.satin.add_within(post.translation, &digit, face, Paint::plain(0.08, 0.08, 0.09));
        }
        let extinguisher = Shape::new(Capsule3d::new(0.08, 0.35).mesh().rings(2).latitudes(6).longitudes(12).build());
        set.satin.add_within(
            post.translation,
            &extinguisher,
            post * Transform::from_xyz(0.2, 0.25, 0.7),
            Paint::plain(0.75, 0.06, 0.05),
        );
        // The marshal beside the post, in orange, the flag raised.
        let marshal = post * Transform::from_xyz(0.1, 0.0, -1.1);
        let orange = (0.95, 0.45, 0.08);
        person(set, Stance::HoldingFlag, marshal, orange, orange);
        let hand = Stance::FLAG_HAND;
        let top = hand + Vec3::new(0.3, 0.95, 0.0);
        set.satin.add_within(
            post.translation,
            &rod(hand - Vec3::new(0.05, 0.15, 0.0), top, 0.012),
            marshal,
            Paint::plain(0.2, 0.2, 0.2),
        );
        let cloth = Shape::block(Vec3::new(0.0, -0.42, -0.005), Vec3::new(0.55, 0.0, 0.005));
        set.matte.add_within(
            post.translation,
            &cloth,
            marshal * Transform::from_translation(top),
            Paint::plain(0.97, 0.82, 0.05),
        );
    }
}

/// Chain-link wire: diamonds of galvanised wire, the holes see-through.
fn chain_link() -> Image {
    let side = 64u32;
    let mut pixels = Vec::with_capacity((side * side * 4) as usize);
    for y in 0..side {
        for x in 0..side {
            let on = (x + y) % 32 < 3 || (x + side - y) % 32 < 3;
            pixels.extend_from_slice(&if on { [170, 172, 176, 255] } else { [0, 0, 0, 0] });
        }
    }
    let size = bevy::render::render_resource::Extent3d { width: side, height: side, depth_or_array_layers: 1 };
    let mut image = Image::new(
        size,
        bevy::render::render_resource::TextureDimension::D2,
        pixels,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::default(),
    );
    image.sampler = bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor {
        address_mode_u: bevy::image::ImageAddressMode::Repeat,
        address_mode_v: bevy::image::ImageAddressMode::Repeat,
        mag_filter: bevy::image::ImageFilterMode::Linear,
        min_filter: bevy::image::ImageFilterMode::Linear,
        ..default()
    });
    image
}

/// At the bends: tyre stacks along the outer barrier, chevron boards behind it.
pub fn bends(set: &mut Set, props: &mut Props, track: &Track, frames: &[Frame]) {
    let _ = props;
    let (half, verge) = (track.half_width, track.verge);
    let tyre = Shape::new(
        Torus { minor_radius: 0.15, major_radius: 0.31 }.mesh().minor_resolution(6).major_resolution(12).build(),
    );
    let rubbers = [Paint::shaded(0.88, 0.1, 0.1, 0.3), Paint::shaded(0.96, 0.96, 0.96, 0.3)];
    // A yellow board with black chevrons pointing right; mirrored, left.
    let board = Shape::block(Vec3::new(-1.5, -0.6, -0.04), Vec3::new(1.5, 0.6, 0.04));
    let chevron = |x: f32| {
        let (top, bottom, mid, w, t) = (0.45, -0.45, 0.0, 0.42, 0.26);
        let upper = [
            Vec3::new(x, top, 0.05),
            Vec3::new(x + w, mid, 0.05),
            Vec3::new(x + w + t, mid, 0.05),
            Vec3::new(x + t, top, 0.05),
        ];
        let lower = [
            Vec3::new(x, bottom, 0.05),
            Vec3::new(x + t, bottom, 0.05),
            Vec3::new(x + w + t, mid, 0.05),
            Vec3::new(x + w, mid, 0.05),
        ];
        Shape::faces(&[&upper, &lower], false)
    };
    let chevrons: Vec<Shape> = [-1.2, -0.4, 0.4].map(chevron).into();
    let post = Shape::block(Vec3::new(-0.06, -2.1, -0.06), Vec3::new(0.06, -0.5, 0.06));

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
            let foot = at + right * outside * (half + verge + BARRIER - 0.48);
            for level in 0..3 {
                let place = Transform::from_xyz(foot.x, 0.15 + 0.29 * level as f32, foot.y);
                set.satin.add(&tyre, place, rubbers[(i * 3 + step + level) % 2]);
            }
        }
        if frame.bend.abs() > 0.02 && i % 2 == 0 {
            let foot = frame.at + frame.right * outside * (half + verge + BARRIER + 1.8);
            // The board faces the drivers; its +X is their right.
            let face = Transform::from_xyz(foot.x, 2.1, foot.y).with_rotation(upright(heading(frame.forward) + PI));
            set.satin.add(&board, face, Paint::plain(0.98, 0.8, 0.08));
            // Seen by the drivers, the arrows point the way the road turns.
            let way =
                if frame.bend > 0.0 { Transform::from_scale(Vec3::new(-1.0, 1.0, 1.0)) } else { Transform::IDENTITY };
            for shape in &chevrons {
                set.satin.add_within(face.translation, shape, face * way, Paint::plain(0.06, 0.06, 0.07));
            }
            for side in [-1.2, 1.2] {
                set.satin.add_within(
                    face.translation,
                    &post,
                    face * Transform::from_xyz(side, 0.0, -0.1),
                    Paint::plain(0.7, 0.72, 0.75),
                );
            }
        }
    }
}

/// Inflatable arches over the road, a third and two thirds of the way round.
pub fn arches(set: &mut Set, circuit: &Circuit, frames: &[Frame]) {
    let track = &circuit.track;
    let radius = track.half_width + track.verge + BARRIER + 1.1;
    let colours = [[(0.98, 0.84, 0.12), (0.12, 0.42, 0.92)], [(0.95, 0.3, 0.55), (0.98, 0.98, 0.96)]];
    for (k, share) in [0.36, 0.7].into_iter().enumerate() {
        let along = (circuit.start + share * track.length()).rem_euclid(track.length());
        let frame =
            frames.iter().min_by(|a, b| (a.along - along).abs().total_cmp(&(b.along - along).abs())).expect("a track");
        let place = Transform::from_xyz(frame.at.x, 0.0, frame.at.y).with_rotation(upright(heading(frame.forward)));
        let segments = 10;
        for s in 0..segments {
            let points: Vec<Vec3> = (0..=4)
                .map(|p| {
                    let angle = PI * (s as f32 + p as f32 / 4.0) / segments as f32;
                    Vec3::new(radius * angle.cos(), 0.6 + radius * angle.sin(), 0.0)
                })
                .collect();
            let (r, g, b) = colours[k][s % 2];
            let tube = Shape::sweep(&points, 0.85, 10, Vec3::Z);
            set.satin.add_within(place.translation, &tube, place, Paint::blend((r * 0.8, g * 0.8, b * 0.8), (r, g, b)));
        }
        for side in [-1.0, 1.0] {
            let weight = Shape::lathe(&[(0.0, 0.0), (1.1, 0.0), (1.1, 0.6), (0.0, 0.7)], 10);
            set.satin.add_within(
                place.translation,
                &weight,
                place * Transform::from_xyz(side * radius, 0.0, 0.0),
                Paint::shaded(0.3, 0.3, 0.34, 0.3),
            );
        }
    }
}

/// A windmill on the knoll, its sails turning, facing the start.
pub fn knoll(set: &mut Set, props: &mut Props, circuit: &Circuit, land: &Land) {
    let Some(knoll) = land.knoll else { return };
    set.clear(knoll.centre, 9.0);
    let foot = seat(land, knoll.centre, 3.6) - Vec3::Y * 0.2;
    let start = Vec2::from(circuit.track.at(circuit.start).0);
    let facing = upright(heading(start - knoll.centre));
    let stand = Transform::from_translation(foot).with_rotation(facing);

    // A stone base, deep enough to meet the ground on every side of the slope.
    let base = Shape::lathe(&[(3.8, -3.0), (3.7, 0.5), (3.4, 0.75), (0.0, 0.75)], 12);
    set.matte.add(&base, stand, Paint::blend((0.42, 0.4, 0.38), (0.7, 0.68, 0.64)));
    let tower = Shape::lathe(&[(3.3, 0.0), (3.0, 4.0), (2.5, 8.5), (2.2, 10.6), (0.0, 10.6)], 12);
    set.matte.add(&tower, stand, Paint::shaded(0.97, 0.94, 0.86, 0.3));
    let cap = Shape::lathe(&[(2.6, 10.4), (2.5, 11.4), (1.6, 12.6), (0.0, 13.2)], 12);
    set.satin.add_within(foot, &cap, stand, Paint::shaded(0.8, 0.22, 0.16, 0.25));
    // A door and windows, on the side that faces the start.
    let dark = Paint::plain(0.28, 0.18, 0.12);
    set.matte.add_within(foot, &Shape::block(Vec3::new(-0.7, 0.0, 2.9), Vec3::new(0.7, 2.3, 3.25)), stand, dark);
    for (y, z) in [(4.6, 2.85), (7.6, 2.4)] {
        set.matte.add_within(
            foot,
            &Shape::block(Vec3::new(-0.45, y, z), Vec3::new(0.45, y + 1.0, z + 0.4)),
            stand,
            Paint::plain(0.35, 0.55, 0.75),
        );
    }

    // The sails turn around the axle, which points the way the mill faces.
    let mut sails = Batch::default();
    let wood = Paint::shaded(0.55, 0.36, 0.2, 0.2);
    sails.add(&Shape::new(Sphere::new(0.55).mesh().ico(2).expect("a small icosphere")), Transform::IDENTITY, wood);
    sails.add(&rod(Vec3::new(0.0, 0.0, -1.6), Vec3::new(0.0, 0.0, 0.4), 0.22), Transform::IDENTITY, wood);
    for k in 0..4 {
        let turn = Transform::from_rotation(Quat::from_rotation_z(k as f32 * FRAC_PI_2 + 0.3));
        sails.add(&rod(Vec3::ZERO, Vec3::new(0.0, 9.0, 0.15), 0.13), turn, wood);
        for slat in 0..7 {
            let y = 2.0 + slat as f32 * 1.0;
            let cloth = Shape::block(Vec3::new(0.25, y, 0.1), Vec3::new(1.9, y + 0.82, 0.16));
            sails.add(&cloth, turn, Paint::plain(0.97, 0.95, 0.88));
        }
        sails.add(&rod(Vec3::new(1.95, 1.8, 0.13), Vec3::new(1.95, 9.0, 0.13), 0.06), turn, wood);
    }
    let hub = Transform::from_translation(foot + Vec3::Y * 11.6 + facing * Vec3::Z * 2.9).with_rotation(facing);
    props.moving(sails, hub, Turning { rate: 0.55 });

    // A fence of white rails around the mill.
    for k in 0..16 {
        let (a, b) =
            (Vec2::from_angle(k as f32 / 16.0 * TAU) * 8.0, Vec2::from_angle((k + 1) as f32 / 16.0 * TAU) * 8.0);
        let (pa, pb) = (on_land(land, knoll.centre + a), on_land(land, knoll.centre + b));
        if k % 4 == 0 {
            continue; // a gap: the way in
        }
        set.matte.add(&rod(pa, pa + Vec3::Y * 1.0, 0.07), Transform::IDENTITY, Paint::plain(0.96, 0.96, 0.94));
        for h in [0.5, 0.9] {
            set.matte.add(
                &rod(pa + Vec3::Y * h, pb + Vec3::Y * h, 0.04),
                Transform::IDENTITY,
                Paint::plain(0.96, 0.96, 0.94),
            );
        }
    }
}

/// The lake: a jetty, a sailing boat, lily pads and reeds at the shore.
pub fn lake(set: &mut Set, props: &mut Props, land: &Land) {
    let Some(lake) = land.lake else { return };
    // Where the shore is, every way from the middle of the lake.
    let shore = |angle: f32| {
        let way = Vec2::from_angle(angle);
        let mut distance = 0.0;
        while land.shore(lake.centre + way * distance).is_some_and(|s| s < 1.0) && distance < 2.0 * lake.radius {
            distance += 0.25;
        }
        (way, distance)
    };

    // The jetty, from the shore towards the middle.
    let (way, reach) = shore(0.8);
    let place = Transform::from_translation(
        Vec3::new(lake.centre.x, WATER + 0.38, lake.centre.y) + Vec3::new(way.x, 0.0, way.y) * (reach + 3.0),
    )
    .with_rotation(upright(heading(-way)));
    let plank = Paint::shaded(0.62, 0.45, 0.28, 0.2);
    for k in 0..26 {
        let z = k as f32 * 0.5;
        set.matte.add_within(
            place.translation,
            &Shape::block(Vec3::new(-1.2, -0.08, z), Vec3::new(1.2, 0.0, z + 0.44)),
            place,
            plank,
        );
    }
    for z in [1.0, 5.5, 10.0, 12.6] {
        for x in [-1.1, 1.1] {
            set.matte.add_within(
                place.translation,
                &rod(Vec3::new(x, -2.5, z), Vec3::new(x, 0.35, z), 0.1),
                place,
                Paint::plain(0.4, 0.28, 0.18),
            );
        }
    }

    // A sailing boat that rocks on the water.
    let mut boat = Batch::default();
    let hull = Shape::lathe(&[(0.0, -0.35), (0.7, -0.25), (0.95, 0.1), (1.0, 0.3)], 12);
    boat.add(
        &hull,
        Transform::from_scale(Vec3::new(1.0, 1.0, 2.5)),
        Paint::blend((0.1, 0.25, 0.6), (0.95, 0.95, 0.95)),
    );
    boat.add(&Shape::block(Vec3::new(-0.85, 0.12, -2.1), Vec3::new(0.85, 0.2, 2.1)), Transform::IDENTITY, plank);
    boat.add(
        &rod(Vec3::new(0.0, 0.0, 0.5), Vec3::new(0.0, 5.4, 0.5), 0.07),
        Transform::IDENTITY,
        Paint::plain(0.5, 0.36, 0.22),
    );
    let sail = [Vec3::new(0.0, 0.9, 0.35), Vec3::new(0.0, 5.2, 0.35), Vec3::new(0.0, 0.9, -1.9)];
    boat.add(&Shape::faces(&[&sail], true), Transform::IDENTITY, Paint::blend((0.98, 0.98, 0.96), (0.95, 0.3, 0.25)));
    let jib = [Vec3::new(0.0, 0.9, 0.65), Vec3::new(0.0, 4.4, 0.6), Vec3::new(0.0, 0.9, 2.3)];
    boat.add(&Shape::faces(&[&jib], true), Transform::IDENTITY, Paint::plain(0.98, 0.98, 0.96));
    let afloat = lake.centre + Vec2::from_angle(3.6) * lake.radius * 0.3;
    let height = WATER + 0.05;
    let place = Transform::from_xyz(afloat.x, height, afloat.y).with_rotation(upright(1.1));
    props.moving(boat, place, Floating { height, phase: 0.7, swing: 0.07 });

    // Lily pads in the shallows, reeds at the edge.
    let pad = Shape::new(Circle::new(1.0).mesh().resolution(10).build());
    let reed = rod(Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0), 0.05);
    for k in 0..200 {
        let angle = set.dice.between(0.0, TAU);
        let (way, reach) = shore(angle);
        if k % 2 == 0 {
            let at = lake.centre + way * reach * set.dice.between(0.62, 0.9);
            let place = Transform::from_xyz(at.x, WATER + 0.03, at.y)
                .with_rotation(Quat::from_rotation_y(angle) * Quat::from_rotation_x(-FRAC_PI_2))
                .with_scale(Vec3::splat(set.dice.between(0.35, 0.7)));
            let flower = set.dice.roll() < 0.15;
            set.bright.add(
                &pad,
                place,
                if flower { Paint::plain(1.0, 0.72, 0.85) } else { Paint::plain(0.2, 0.55, 0.2) },
            );
        } else {
            let at = lake.centre + way * reach * set.dice.between(0.94, 1.04);
            for _ in 0..9 {
                let blade = at + Vec2::new(set.dice.between(-0.35, 0.35), set.dice.between(-0.35, 0.35));
                let lean = Quat::from_rotation_x(set.dice.between(-0.3, 0.3))
                    * Quat::from_rotation_z(set.dice.between(-0.3, 0.3));
                let place = Transform::from_translation(Vec3::new(blade.x, WATER - 0.1, blade.y))
                    .with_rotation(lean)
                    .with_scale(Vec3::new(1.0, set.dice.between(0.9, 1.6), 1.0));
                set.bright.add(&reed, place, Paint::blend((0.24, 0.42, 0.14), (0.5, 0.72, 0.26)));
            }
        }
    }
}

/// A fair beside the start straight: a big wheel that turns, striped tents,
/// flags between them.
pub fn fair(set: &mut Set, props: &mut Props, circuit: &Circuit, land: &Land) {
    let track = &circuit.track;
    let (along_road, road_heading) = track.at(circuit.start - 34.0);
    let road = Vec2::from(along_road);
    // The left of the road: away from the inside of the loop.
    let left = Vec2::new(road_heading.cos(), -road_heading.sin());
    let left = if land.inside(road + left * 30.0) { -left } else { left };
    let centre = road + left * 46.0;
    set.clear(centre, 34.0);
    let foot = seat(land, centre, 8.0);
    let facing = upright(heading(-left));

    // The big wheel: rims, spokes and cabins turn; the legs stand.
    let (radius, axle) = (13.0, 15.5);
    let steel = Paint::blend((0.75, 0.76, 0.8), (0.95, 0.96, 0.98));
    for side in [-1.6, 1.6] {
        for lean in [-1.0, 1.0] {
            let leg = rod(Vec3::new(lean * 7.5, 0.0, side * 1.6), Vec3::new(0.0, axle, side), 0.22);
            set.satin.add_within(foot, &leg, Transform::from_translation(foot).with_rotation(facing), steel);
        }
    }
    let mut wheel = Batch::default();
    let ring: Vec<Vec3> =
        (0..=40).map(|k| Vec3::from((Vec2::from_angle(k as f32 / 40.0 * TAU) * radius, 0.0))).collect();
    for z in [-1.1, 1.1] {
        wheel.add(
            &Shape::sweep(&ring, 0.16, 6, Vec3::Z),
            Transform::from_xyz(0.0, 0.0, z),
            Paint::plain(0.95, 0.95, 0.97),
        );
    }
    wheel.add(&rod(Vec3::new(0.0, 0.0, -1.8), Vec3::new(0.0, 0.0, 1.8), 0.4), Transform::IDENTITY, steel);
    let cabin = Shape::lathe(&[(0.0, -0.9), (0.9, -0.6), (1.0, 0.1), (0.6, 0.7), (0.0, 0.8)], 8);
    for k in 0..12 {
        let out = Vec2::from_angle(k as f32 / 12.0 * TAU) * radius;
        for z in [-1.1, 1.1] {
            wheel.add(&rod(Vec3::new(0.0, 0.0, z), Vec3::new(out.x, out.y, z), 0.08), Transform::IDENTITY, steel);
        }
        let (r, g, b) = PALETTE[k % PALETTE.len()];
        wheel.add(&cabin, Transform::from_xyz(out.x, out.y - 1.0, 0.0), Paint::shaded(r, g, b, 0.35));
    }
    let hub = Transform::from_translation(foot + Vec3::Y * axle).with_rotation(facing);
    props.moving(wheel, hub, Turning { rate: 0.12 });

    // Tents around the wheel, in two colours each.
    let colours = [(0.9, 0.16, 0.16), (0.14, 0.42, 0.9), (0.98, 0.72, 0.1), (0.2, 0.66, 0.32)];
    let mut tops = Vec::new();
    for (k, (x, z)) in [(-16.0, 9.0), (15.0, 11.0), (-12.0, -14.0), (17.0, -12.0), (0.0, 19.0)].into_iter().enumerate()
    {
        let at = foot + facing * Vec3::new(x, 0.0, z);
        let (size, colour) = (set.dice.between(0.85, 1.25), colours[k % colours.len()]);
        let at = seat(land, Vec2::new(at.x, at.z), 3.2 * size);
        // Its walls go a little into the ground, for a slope.
        let profile = [(3.2 * size, -1.2), (3.2 * size, 2.2 * size), (0.0, 5.0 * size)];
        for gore in 0..12 {
            let slice = Shape::gore(&profile, 2, gore as f32 / 12.0 * TAU, TAU / 12.0);
            let (r, g, b) = if gore % 2 == 0 { colour } else { (0.97, 0.96, 0.92) };
            set.satin.add_within(at, &slice, Transform::from_translation(at), Paint::shaded(r, g, b, 0.2));
        }
        let top = at + Vec3::Y * 5.0 * size;
        set.satin.add(&rod(top, top + Vec3::Y * 1.3, 0.05), Transform::IDENTITY, steel);
        let pennant = [top + Vec3::Y * 1.3, top + Vec3::new(0.0, 1.0, 0.0), top + Vec3::new(1.1, 1.15, 0.0)];
        set.bright.add(
            &Shape::faces(&[&pennant], true),
            Transform::IDENTITY,
            Paint::plain(colour.0, colour.1, colour.2),
        );
        tops.push(top);
    }
    for pair in tops.windows(2) {
        bunting(&mut set.bright, pair[0] - Vec3::Y * 0.6, pair[1] - Vec3::Y * 0.6, &PALETTE);
    }
}
