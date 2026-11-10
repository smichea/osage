// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the kart world looks like, natively: the circuit built from its
//! centre line (road, kerbs, walls, start line, trees) and the karts. All of
//! it is procedural for now; the rendering programme (design/03, R6 to R9)
//! replaces it piece by piece with real content.
//!
//! Whoever computes the karts (the network replica when playing, a local
//! simulation in the bench) writes each kart's [`Pose`] in [`Stage::Simulate`];
//! this module draws them in [`Stage::Draw`].

use std::f32::consts::{FRAC_PI_2, PI};

use bevy::{
    asset::RenderAssetUsages,
    color::ColorToComponents,
    image::ImageSampler,
    mesh::Indices,
    prelude::*,
    render::render_resource::{Extent3d, PrimitiveTopology, TextureDimension, TextureFormat},
};
use osage_kart_physics::{Kart, LONG_TURBO_CHARGE, SHORT_TURBO_CHARGE, Track};
use osage_render::{OsageCamera, Sun};
use osage_world_kart::Circuit;

/// Kart colours, by `kart.racer/1` colour index: the browser client's palette.
pub const PALETTE: [(f32, f32, f32); 8] = [
    (0.898, 0.325, 0.294),
    (0.325, 0.608, 0.961),
    (0.341, 0.671, 0.353),
    (0.776, 0.565, 0.149),
    (0.690, 0.514, 0.941),
    (0.224, 0.773, 0.812),
    (0.878, 0.510, 0.239),
    (0.969, 0.471, 0.729),
];

pub struct ScenePlugin;

/// The two halves of a frame: something computes the karts, then they are drawn.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum Stage {
    Simulate,
    Draw,
}

/// The circuit to draw.
#[derive(Resource)]
pub struct Course(pub Circuit);

/// A kart to draw, as it is now.
#[derive(Component)]
pub struct Pose {
    pub kart: Kart,
    pub color: u32,
}

/// The kart the camera follows.
#[derive(Component)]
pub struct Followed;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(Update, (Stage::Simulate, Stage::Draw).chain()).add_systems(Startup, setup).add_systems(
            Update,
            (build_circuit.run_if(resource_exists_and_changed::<Course>), dress_karts, place_karts, follow)
                .chain()
                .in_set(Stage::Draw),
        );
    }
}

/// Meshes and materials every kart shares.
#[derive(Resource)]
struct Shapes {
    chassis: Handle<Mesh>,
    nose: Handle<Mesh>,
    wing: Handle<Mesh>,
    wheel: Handle<Mesh>,
    helmet: Handle<Mesh>,
    spark: Handle<Mesh>,
    flame: Handle<Mesh>,
    paint: Vec<Handle<StandardMaterial>>,
    rubber: Handle<StandardMaterial>,
    dark: Handle<StandardMaterial>,
    white: Handle<StandardMaterial>,
    /// Drift sparks: no charge yet, short turbo ready, long turbo ready.
    sparks: [Handle<StandardMaterial>; 3],
    fire: Handle<StandardMaterial>,
}

#[derive(Component)]
struct CircuitPart;

/// The moving parts of a kart's model.
#[derive(Component)]
struct Parts {
    body: Entity,
    wheels: [Entity; 4],
    sparks: [Entity; 2],
    flame: Entity,
    spin: f32,
    slip: f32,
}

/// Where the camera looks, eased like its position.
#[derive(Component)]
struct Chase(Vec3);

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.spawn((
        Camera3d::default(),
        OsageCamera,
        Projection::Perspective(PerspectiveProjection { fov: 65f32.to_radians(), far: 4000.0, ..default() }),
        Transform::from_xyz(0.0, 120.0, -160.0).looking_at(Vec3::new(-60.0, 0.0, 0.0), Vec3::Y),
        Chase(Vec3::new(-60.0, 0.0, 0.0)),
    ));
    commands.spawn((
        DirectionalLight::default(),
        Sun,
        Transform::from_xyz(55.0, 60.0, 35.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    let glow = |materials: &mut Assets<StandardMaterial>, (r, g, b): (f32, f32, f32)| {
        // Brighter than white, whatever the exposure: bloom makes it glow.
        materials.add(StandardMaterial {
            base_color: Color::BLACK,
            emissive: LinearRgba::rgb(r * 6.0, g * 6.0, b * 6.0),
            emissive_exposure_weight: 0.0,
            ..default()
        })
    };
    let paint = PALETTE
        .iter()
        .map(|&(r, g, b)| {
            materials.add(StandardMaterial {
                base_color: Color::srgb(r, g, b),
                perceptual_roughness: 0.4,
                metallic: 0.3,
                clearcoat: 1.0,
                clearcoat_perceptual_roughness: 0.15,
                ..default()
            })
        })
        .collect();
    let matte = |materials: &mut Assets<StandardMaterial>, color: Color, perceptual_roughness| {
        materials.add(StandardMaterial { base_color: color, perceptual_roughness, ..default() })
    };
    commands.insert_resource(Shapes {
        chassis: meshes.add(Cuboid::new(1.3, 0.35, 2.0)),
        nose: meshes.add(Cuboid::new(0.9, 0.22, 0.6)),
        wing: meshes.add(Cuboid::new(1.4, 0.08, 0.35)),
        wheel: meshes.add(Cylinder::new(0.32, 0.3)),
        helmet: meshes.add(Sphere::new(0.3)),
        spark: meshes.add(Sphere::new(0.14)),
        flame: meshes.add(Cone::new(0.22, 0.9)),
        paint,
        rubber: matte(&mut materials, Color::srgb(0.03, 0.03, 0.03), 0.8),
        dark: matte(&mut materials, Color::srgb(0.02, 0.025, 0.03), 0.5),
        white: matte(&mut materials, Color::srgb(0.92, 0.92, 0.92), 0.25),
        sparks: [(1.0, 0.95, 0.7), (0.25, 0.65, 1.0), (1.0, 0.5, 0.1)].map(|c| glow(&mut materials, c)),
        fire: glow(&mut materials, (1.0, 0.55, 0.2)),
    });
}

/// A point of the centre line and the unit vector to its right.
struct Frame {
    at: Vec2,
    right: Vec2,
    /// Distance from the first point, along the centre line.
    along: f32,
}

fn frames(track: &Track) -> Vec<Frame> {
    let points = track.points();
    let n = points.len();
    let mut along = 0.0;
    (0..n)
        .map(|i| {
            let [before, at, after] = [points[(i + n - 1) % n], points[i], points[(i + 1) % n]].map(Vec2::from);
            let forward = (after - before).normalize_or_zero();
            let frame = Frame { at, right: Vec2::new(-forward.y, forward.x), along };
            along += at.distance(after);
            frame
        })
        .collect()
}

fn linear(r: f32, g: f32, b: f32) -> [f32; 4] {
    Color::srgb(r, g, b).to_linear().to_f32_array()
}

/// A flat strip from `inner` to `outer` metres right of the centre line
/// (negative: left), `y` above the ground, coloured per segment; with
/// texture coordinates in metres for the materials to come.
fn strip(frames: &[Frame], inner: f32, outer: f32, y: f32, colour: impl Fn(usize) -> [f32; 4]) -> Mesh {
    let (mut positions, mut uvs, mut colours) = (Vec::new(), Vec::new(), Vec::new());
    for (i, a) in frames.iter().enumerate() {
        let b = &frames[(i + 1) % frames.len()];
        let ahead = if b.along > a.along { b.along } else { a.along + a.at.distance(b.at) };
        let corner = |f: &Frame, lateral: f32, along: f32| {
            let p = f.at + f.right * lateral;
            ([p.x, y, p.y], [lateral, along])
        };
        // Counter-clockwise seen from above, so that the strip faces up.
        let quad = [
            corner(a, outer, a.along),
            corner(b, outer, ahead),
            corner(a, inner, a.along),
            corner(a, inner, a.along),
            corner(b, outer, ahead),
            corner(b, inner, ahead),
        ];
        for (position, uv) in quad {
            positions.push(position);
            uvs.push(uv);
            colours.push(colour(i));
        }
    }
    let normals = vec![[0.0, 1.0, 0.0]; positions.len()];
    triangles(positions, normals, uvs, colours)
}

/// A wall `height` high, `lateral` metres right of the centre line, facing it.
fn wall(frames: &[Frame], lateral: f32, height: f32, colour: impl Fn(usize) -> [f32; 4]) -> Mesh {
    let (mut positions, mut normals, mut uvs, mut colours) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (i, a) in frames.iter().enumerate() {
        let b = &frames[(i + 1) % frames.len()];
        let foot = |f: &Frame| f.at + f.right * lateral;
        let (a0, b0) = (foot(a), foot(b));
        let length = a0.distance(b0);
        let corners = [
            ([a0.x, 0.0, a0.y], [0.0, 0.0]),
            ([b0.x, 0.0, b0.y], [length, 0.0]),
            ([b0.x, height, b0.y], [length, height]),
            ([a0.x, height, a0.y], [0.0, height]),
        ];
        // The face turned to the track is the front one.
        let order = if lateral > 0.0 { [0, 3, 1, 1, 3, 2] } else { [0, 1, 3, 3, 1, 2] };
        let inward = (a.right + b.right).normalize_or_zero() * -lateral.signum();
        for corner in order {
            positions.push(corners[corner].0);
            uvs.push(corners[corner].1);
            normals.push([inward.x, 0.0, inward.y]);
            colours.push(colour(i));
        }
    }
    triangles(positions, normals, uvs, colours)
}

fn triangles(positions: Vec<[f32; 3]>, normals: Vec<[f32; 3]>, uvs: Vec<[f32; 2]>, colours: Vec<[f32; 4]>) -> Mesh {
    let indices = (0..positions.len() as u32).collect();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colours)
        .with_inserted_indices(Indices::U32(indices))
}

fn build_circuit(
    mut commands: Commands,
    course: Res<Course>,
    old: Query<Entity, With<CircuitPart>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    for part in &old {
        commands.entity(part).despawn();
    }
    let track = &course.0.track;
    let frames = frames(track);
    let (half_width, verge) = (track.half_width, track.verge);
    let grass = materials.add(StandardMaterial {
        base_color: Color::srgb(0.2, 0.42, 0.14),
        perceptual_roughness: 1.0,
        ..default()
    });
    commands.spawn((
        CircuitPart,
        Mesh3d(meshes.add(Plane3d::default().mesh().size(3000.0, 3000.0))),
        MeshMaterial3d(grass),
    ));

    let asphalt = [linear(0.21, 0.23, 0.26), linear(0.24, 0.26, 0.29)];
    let road = strip(&frames, -half_width, half_width, 0.02, |i| asphalt[i % 2]);
    let kerbs = [linear(0.84, 0.23, 0.23), linear(0.94, 0.94, 0.94)];
    let kerb = |i: usize| kerbs[i / 2 % 2];
    let barriers = [linear(0.18, 0.44, 0.84), linear(0.91, 0.93, 0.97)];
    let barrier = |i: usize| barriers[i % 2];
    // White: the colours are the meshes' own, per segment.
    let flat = materials.add(StandardMaterial { perceptual_roughness: 0.9, ..default() });
    for mesh in [
        road,
        strip(&frames, half_width, half_width + 0.9, 0.03, kerb),
        strip(&frames, -half_width - 0.9, -half_width, 0.03, kerb),
    ] {
        commands.spawn((CircuitPart, Mesh3d(meshes.add(mesh)), MeshMaterial3d(flat.clone())));
    }
    let both_sides =
        materials.add(StandardMaterial { perceptual_roughness: 0.6, double_sided: true, cull_mode: None, ..default() });
    for side in [1.0, -1.0] {
        let mesh = wall(&frames, side * (half_width + verge), 0.9, barrier);
        commands.spawn((CircuitPart, Mesh3d(meshes.add(mesh)), MeshMaterial3d(both_sides.clone())));
    }

    // Start/finish line: a chequered band across the road.
    let squares: Vec<u8> = (0..2)
        .flat_map(|j| {
            (0..16).flat_map(move |i| if (i + j) % 2 == 0 { [250, 250, 250, 255] } else { [17, 17, 17, 255] })
        })
        .collect();
    let mut chequers = Image::new(
        Extent3d { width: 16, height: 2, depth_or_array_layers: 1 },
        TextureDimension::D2,
        squares,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    chequers.sampler = ImageSampler::nearest();
    let (line, heading) = track.at(course.0.start);
    commands.spawn((
        CircuitPart,
        Mesh3d(meshes.add(Plane3d::default().mesh().size(2.0 * half_width, 1.6))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: Some(images.add(chequers)),
            perceptual_roughness: 0.8,
            ..default()
        })),
        Transform::from_xyz(line[0], 0.04, line[1]).with_rotation(Quat::from_rotation_y(heading)),
    ));

    // Trees outside the walls, placed the same way by every client.
    let mut seed = 7u64;
    let mut random = move || {
        seed = seed * 16807 % 2_147_483_647;
        seed as f32 / 2_147_483_647.0
    };
    let trunk = meshes.add(Cylinder::new(0.3, 1.6));
    let crown = meshes.add(Cone::new(1.6, 3.6));
    let matte = |base_color| StandardMaterial { base_color, perceptual_roughness: 0.9, ..default() };
    let bark = materials.add(matte(Color::srgb(0.15, 0.08, 0.03)));
    let leaves = materials.add(matte(Color::srgb(0.05, 0.18, 0.06)));
    let clear = half_width + verge + 3.0;
    for frame in frames.iter().step_by(3) {
        let side = if random() < 0.5 { -1.0 } else { 1.0 };
        let away = half_width + verge + 5.0 + random() * 25.0;
        let scale = 0.8 + random() * 0.7;
        let place = frame.at + frame.right * away * side;
        if frames.iter().any(|f| f.at.distance(place) < clear) {
            continue;
        }
        commands
            .spawn((
                CircuitPart,
                Transform::from_xyz(place.x, 0.0, place.y).with_scale(Vec3::splat(scale)),
                Visibility::default(),
            ))
            .with_children(|tree| {
                tree.spawn((Mesh3d(trunk.clone()), MeshMaterial3d(bark.clone()), Transform::from_xyz(0.0, 0.8, 0.0)));
                tree.spawn((Mesh3d(crown.clone()), MeshMaterial3d(leaves.clone()), Transform::from_xyz(0.0, 3.2, 0.0)));
            });
    }
}

/// Gives every new kart its model: a kart facing +Z, its origin on the ground.
fn dress_karts(mut commands: Commands, shapes: Res<Shapes>, karts: Query<(Entity, &Pose), Added<Pose>>) {
    for (kart, pose) in &karts {
        let paint = shapes.paint[pose.color as usize % shapes.paint.len()].clone();
        let part = |mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>, x: f32, y: f32, z: f32| {
            (Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), Transform::from_xyz(x, y, z))
        };
        let body = commands.spawn((Transform::default(), Visibility::default(), ChildOf(kart))).id();
        commands.entity(body).with_children(|body| {
            body.spawn(part(&shapes.chassis, &paint, 0.0, 0.4, 0.0));
            body.spawn(part(&shapes.nose, &paint, 0.0, 0.35, 1.25));
            body.spawn(part(&shapes.wing, &shapes.dark, 0.0, 0.85, -1.0));
            body.spawn(part(&shapes.helmet, &shapes.white, 0.0, 1.0, -0.2));
        });
        let hidden = |commands: &mut Commands, bundle: (Mesh3d, MeshMaterial3d<StandardMaterial>, Transform)| {
            commands.spawn((bundle, Visibility::Hidden, ChildOf(body))).id()
        };
        let wheels = [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)].map(|(side, end)| {
            commands.spawn((part(&shapes.wheel, &shapes.rubber, 0.75 * side, 0.32, 0.72 * end), ChildOf(body))).id()
        });
        let sparks = [-1.0, 1.0]
            .map(|side| hidden(&mut commands, part(&shapes.spark, &shapes.sparks[0], 0.75 * side, 0.15, -0.95)));
        // The cone's tip points backwards.
        let mut flame = part(&shapes.flame, &shapes.fire, 0.0, 0.45, -1.45);
        flame.2.rotation = Quat::from_rotation_x(-FRAC_PI_2);
        let flame = hidden(&mut commands, flame);
        commands.entity(kart).insert((
            Transform::default(),
            Visibility::default(),
            Parts { body, wheels, sparks, flame, spin: 0.0, slip: 0.0 },
        ));
    }
}

/// Poses every kart and its effects from its state.
fn place_karts(
    time: Res<Time>,
    mut frame: Local<u32>,
    mut karts: Query<(Entity, &Pose, &mut Parts)>,
    mut transforms: Query<&mut Transform>,
    mut visibility: Query<&mut Visibility>,
    mut materials: Query<&mut MeshMaterial3d<StandardMaterial>>,
    shapes: Res<Shapes>,
) {
    let seconds = time.delta_secs();
    *frame = frame.wrapping_add(1);
    for (entity, pose, mut parts) in &mut karts {
        let kart = &pose.kart;
        if let Ok(mut place) = transforms.get_mut(entity) {
            *place = Transform::from_xyz(kart.x, 0.0, kart.z).with_rotation(Quat::from_rotation_y(kart.heading));
        }
        // A drifting kart is drawn turned into the slide.
        parts.slip += (-kart.drift * 0.35 - parts.slip) * (seconds * 10.0).min(1.0);
        if let Ok(mut body) = transforms.get_mut(parts.body) {
            body.rotation = Quat::from_rotation_y(parts.slip);
        }
        parts.spin = (parts.spin + kart.speed * seconds / 0.32) % (2.0 * PI);
        for wheel in parts.wheels {
            if let Ok(mut wheel) = transforms.get_mut(wheel) {
                wheel.rotation = Quat::from_rotation_x(parts.spin) * Quat::from_rotation_z(FRAC_PI_2);
            }
        }

        // Flicker without randomness: the same frames on every run.
        let flicker = |salt: u32| {
            (frame.wrapping_mul(2_654_435_761).wrapping_add(salt.wrapping_mul(40_503)) >> 16 & 0xff) as f32 / 255.0
        };
        let stage = match kart.charge {
            c if c >= LONG_TURBO_CHARGE => 2,
            c if c >= SHORT_TURBO_CHARGE => 1,
            _ => 0,
        };
        for (i, spark) in parts.sparks.into_iter().enumerate() {
            let salt = entity.index_u32().wrapping_add(i as u32);
            if let Ok(mut shown) = visibility.get_mut(spark) {
                *shown =
                    if kart.drift != 0.0 && flicker(salt) < 0.8 { Visibility::Inherited } else { Visibility::Hidden };
            }
            if let Ok(mut material) = materials.get_mut(spark) {
                material.0 = shapes.sparks[stage].clone();
            }
            if let Ok(mut spark) = transforms.get_mut(spark) {
                spark.scale = Vec3::splat(0.6 + 0.9 * flicker(salt.wrapping_add(7)));
            }
        }
        if let Ok(mut shown) = visibility.get_mut(parts.flame) {
            *shown = if kart.boost > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
        }
        if let Ok(mut flame) = transforms.get_mut(parts.flame) {
            flame.scale = Vec3::new(1.0, 0.7 + 0.6 * flicker(entity.index_u32().wrapping_add(13)), 1.0);
        }
    }
}

/// Chase camera behind the followed kart.
fn follow(time: Res<Time>, followed: Query<&Pose, With<Followed>>, mut camera: Query<(&mut Transform, &mut Chase)>) {
    let (Some(pose), Ok((mut camera, mut chase))) = (followed.iter().next(), camera.single_mut()) else { return };
    let (kart, seconds) = (&pose.kart, time.delta_secs());
    let forward = Vec3::new(kart.heading.sin(), 0.0, kart.heading.cos());
    let at = Vec3::new(kart.x, 0.0, kart.z);
    let behind = at - forward * 6.5 + Vec3::Y * 2.9;
    camera.translation = camera.translation.lerp(behind, 1.0 - (-seconds * 7.0).exp());
    chase.0 = chase.0.lerp(at + forward * 4.0 + Vec3::Y, 1.0 - (-seconds * 10.0).exp());
    camera.look_at(chase.0, Vec3::Y);
}
