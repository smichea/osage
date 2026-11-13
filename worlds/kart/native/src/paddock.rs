// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The paddock objects as the world sends them (`kart.paddock/1`): crates
//! standing across the road, tyres rolling, cones and oil left behind,
//! drones flying to the leader (D-052), which burst where they come down. The
//! world decides what they do ([osage-kart-items](osage_kart_items)); this
//! only draws them, a pool of models per kind.
//!
//! The crates are see-through boxes of amber glass with a question mark on
//! every face: what is inside is a surprise. The player's own kart breaks
//! one at once, where it is predicted to be, without waiting for the world
//! to say so, and it bursts into confetti; another kart's crate bursts when
//! the world says it is broken.

use std::f32::consts::FRAC_PI_2;

use bevy::{
    asset::RenderAssetUsages,
    light::{NotShadowCaster, NotShadowReceiver},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use osage_kart_items::{
    CONE_LIFE, CONE_RADIUS, CRATE_REACH, Item, OIL_LIFE, OIL_RADIUS, ROLLING, TYRE_LIFE, TYRE_RADIUS, touches,
};

/// A drone: a body, four arms and four rotors, as one mesh.
pub fn drone_mesh() -> Mesh {
    let mut drone: Mesh = Cuboid::new(0.34, 0.12, 0.42).into();
    for (x, z) in [(1.0f32, 1.0f32), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
        // An arm from the body out to the rotor, along the diagonal.
        let arm: Mesh = Cuboid::new(0.05, 0.04, 0.5).into();
        let arm = arm.rotated_by(Quat::from_rotation_y(x.atan2(z))).translated_by(Vec3::new(x * 0.18, 0.02, z * 0.18));
        drone.merge(&arm).expect("the same attributes");
        let rotor: Mesh = Cylinder::new(0.2, 0.015).into();
        drone.merge(&rotor.translated_by(Vec3::new(x * 0.34, 0.07, z * 0.34))).expect("the same attributes");
    }
    drone
}
use osage_kart_physics::Kart;
use osage_world_kart::proto;

use crate::scene::Stage;

pub struct PaddockPlugin;

impl Plugin for PaddockPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PaddockView>()
            .add_systems(Startup, setup)
            .add_systems(Update, (show, confetti).chain().in_set(Stage::Draw));
    }
}

/// What the world last sent of the paddock, and the player's kart as it
/// is drawn: predicted, `lead` seconds ahead of the world's last state.
#[derive(Resource, Default)]
pub struct PaddockView {
    pub paddock: proto::Paddock,
    pub lead: f32,
    pub me: Option<Kart>,
    /// What the player's kart let go of, drawn at once until the world's
    /// own arrives (D-071).
    pub mine: Vec<Provisional>,
}

/// A thing the player's kart let go of, as the world will release it: a
/// tyre thrown, a cone or oil left; seconds since.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Provisional {
    pub kind: osage_kart_items::Kind,
    pub x: f32,
    pub z: f32,
    pub vx: f32,
    pub vz: f32,
    pub age: f32,
}

/// Longest a thing let go of is drawn without the world's, s.
const UNCONFIRMED: f32 = 1.5;
/// Nearer than this to one of the world's of its kind, it is that one, m.
const SAME: f32 = 3.5;

/// The item of a protocol value.
pub fn item(item: proto::Item) -> Option<Item> {
    match item {
        proto::Item::None => None,
        proto::Item::Nitro => Some(Item::Nitro),
        proto::Item::Tyre => Some(Item::Tyre),
        proto::Item::Cone => Some(Item::Cone),
        proto::Item::Oil => Some(Item::Oil),
        proto::Item::Rocket => Some(Item::Rocket),
        proto::Item::Smoke => Some(Item::Smoke),
        proto::Item::Stack => Some(Item::Stack),
        proto::Item::Drone => Some(Item::Drone),
        proto::Item::Bumper => Some(Item::Bumper),
        proto::Item::Push => Some(Item::Push),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Crate,
    Tyre,
    Cone,
    Oil,
    Drone,
}

const SHAPES: [Shape; 5] = [Shape::Crate, Shape::Tyre, Shape::Cone, Shape::Oil, Shape::Drone];

/// Each kind's model, and the models made so far.
#[derive(Resource)]
struct Looks {
    looks: [(Handle<Mesh>, Handle<StandardMaterial>); 5],
    pools: [Vec<Entity>; 5],
    /// Where each drone was at the last frame: one gone near the road came down on a kart.
    drones: Vec<Vec3>,
    /// When each crate was last seen broken here (s, game time), and
    /// whether the world had it standing.
    broken: Vec<f32>,
    standing: Vec<bool>,
    /// Confetti: a mesh, its colours, the pieces flying.
    bit: Handle<Mesh>,
    colours: Vec<Handle<StandardMaterial>>,
    flying: Vec<(Entity, Bit)>,
    idle: Vec<Entity>,
    bursts: u32,
}

/// A piece of confetti in flight.
#[derive(Clone, Copy)]
struct Bit {
    velocity: Vec3,
    spin: Vec3,
    life: f32,
}

const CRATE: f32 = 0.8;
/// A cone flies this long from the kart to the road, s.
const TOSS: f32 = 0.45;
/// Oil spreads to its size in this long, s.
const SPREAD: f32 = 0.35;
/// A crate broken here stays hidden this long whatever the world says,
/// s: the world's word comes a round trip later.
const TRUST: f32 = 1.5;
/// Pieces of confetti in a burst, and in flight at most.
const BURST: usize = 48;
const CONFETTI: usize = 480;
/// Seconds a piece of confetti flies.
const FLIGHT: f32 = 1.4;

/// A face of a crate: amber glass, a darker frame, a question mark drawn
/// from an arc, a stem and a dot, with soft edges.
fn crate_face() -> Image {
    const SIDE: usize = 128;
    let segment = |p: Vec2, a: Vec2, b: Vec2| {
        let t = ((p - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
        p.distance(a + (b - a) * t)
    };
    let mut data = Vec::with_capacity(SIDE * SIDE * 4);
    for y in 0..SIDE {
        for x in 0..SIDE {
            let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            // How far outside the mark, pixels (negative inside).
            let centre = Vec2::new(64.0, 44.0);
            let ring = (p.distance(centre) - 22.0).abs() - 8.0;
            let angle = (p.y - centre.y).atan2(p.x - centre.x);
            // The arc: from the left, over the top, down the right to 45°.
            let arc = if (-std::f32::consts::PI * 0.95..=std::f32::consts::FRAC_PI_4).contains(&angle) {
                ring
            } else {
                f32::MAX
            };
            let stem = segment(p, Vec2::new(79.5, 59.5), Vec2::new(64.0, 72.0)).min(segment(
                p,
                Vec2::new(64.0, 72.0),
                Vec2::new(64.0, 82.0),
            )) - 8.0;
            let dot = p.distance(Vec2::new(64.0, 102.0)) - 9.0;
            let mark = (0.5 - arc.min(stem).min(dot)).clamp(0.0, 1.0);
            let edge = x.min(y).min(SIDE - 1 - x).min(SIDE - 1 - y);
            let glass = if edge < 8 { [150.0, 70.0, 10.0, 240.0] } else { [255.0, 160.0, 40.0, 90.0] };
            let white = [255.0, 250.0, 235.0, 255.0];
            for c in 0..4 {
                data.push((glass[c] + (white[c] - glass[c]) * mark).round() as u8);
            }
        }
    }
    Image::new(
        Extent3d { width: SIDE as u32, height: SIDE as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// A box's texture coordinates made again so that every side face shows
/// its picture upright (a cuboid's left and right faces have it turned a
/// quarter), read from outside.
fn upright(mut mesh: Mesh) -> Mesh {
    let (Some(positions), Some(normals)) = (
        mesh.attribute(Mesh::ATTRIBUTE_POSITION).and_then(|a| a.as_float3()),
        mesh.attribute(Mesh::ATTRIBUTE_NORMAL).and_then(|a| a.as_float3()),
    ) else {
        return mesh;
    };
    let uvs: Vec<[f32; 2]> = positions
        .iter()
        .zip(normals)
        .map(|(p, n)| {
            let (p, n) = (Vec3::from(*p) / CRATE, Vec3::from(*n));
            if n.y.abs() > 0.5 {
                [0.5 + p.x, 0.5 + p.z * n.y.signum()]
            } else {
                // Screen right for someone looking at the face.
                let right = Vec3::Y.cross(n);
                [0.5 + p.dot(right), 0.5 - p.y]
            }
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let face = images.add(crate_face());
    let bit = meshes.add(Rectangle::new(0.07, 0.11));
    let colours =
        [(1.0, 0.25, 0.3), (1.0, 0.8, 0.15), (0.2, 0.75, 1.0), (0.35, 0.95, 0.4), (0.85, 0.4, 1.0), (1.0, 1.0, 1.0)]
            .map(|(r, g, b)| {
                materials.add(StandardMaterial {
                    base_color: Color::srgb(r, g, b),
                    emissive: LinearRgba::rgb(r * 0.6, g * 0.6, b * 0.6),
                    double_sided: true,
                    cull_mode: None,
                    ..default()
                })
            })
            .to_vec();
    let mut look = |mesh: Mesh, material: StandardMaterial| (meshes.add(mesh), materials.add(material));
    let looks = [
        // Amber glass with a question mark on each face, lit from inside:
        // it reads from far away.
        look(
            upright(Cuboid::from_length(CRATE).into()),
            StandardMaterial {
                base_color: Color::WHITE,
                base_color_texture: Some(face.clone()),
                emissive: LinearRgba::rgb(1.4, 0.9, 0.45),
                emissive_texture: Some(face),
                perceptual_roughness: 0.15,
                reflectance: 0.5,
                // Front faces only: the marks on the far faces, seen
                // through the glass, mirrored, blurred the near ones.
                alpha_mode: AlphaMode::Blend,
                ..default()
            },
        ),
        look(
            Torus { minor_radius: 0.11, major_radius: TYRE_RADIUS - 0.11 }.into(),
            StandardMaterial { base_color: Color::srgb(0.035, 0.035, 0.04), perceptual_roughness: 0.85, ..default() },
        ),
        look(
            Cone { radius: CONE_RADIUS, height: 0.6 }.into(),
            StandardMaterial { base_color: Color::srgb(1.0, 0.36, 0.05), perceptual_roughness: 0.55, ..default() },
        ),
        // Oil: black, glossy, a little see-through at its edge.
        look(
            Circle::new(OIL_RADIUS).into(),
            StandardMaterial {
                base_color: Color::srgba(0.015, 0.015, 0.02, 0.88),
                perceptual_roughness: 0.06,
                reflectance: 0.6,
                alpha_mode: AlphaMode::Blend,
                ..default()
            },
        ),
        // A drone: dark carbon, a red light under it to be seen coming.
        look(
            drone_mesh(),
            StandardMaterial {
                base_color: Color::srgb(0.08, 0.08, 0.09),
                emissive: LinearRgba::rgb(0.6, 0.04, 0.02),
                perceptual_roughness: 0.4,
                metallic: 0.3,
                ..default()
            },
        ),
    ];
    commands.insert_resource(Looks {
        looks,
        pools: default(),
        drones: Vec::new(),
        broken: Vec::new(),
        standing: Vec::new(),
        bit,
        colours,
        flying: Vec::new(),
        idle: Vec::new(),
        bursts: 0,
    });
}

/// Places a model for each crate and thing, hides the models left over.
fn show(
    mut commands: Commands,
    time: Res<Time>,
    mut view: ResMut<PaddockView>,
    course: Option<Res<crate::scene::Course>>,
    mut looks: ResMut<Looks>,
    mut models: Query<(&mut Transform, &mut Visibility)>,
) {
    let t = time.elapsed_secs();
    // The road's height under a thing: over a crest, it is raised (D-051).
    let road = |x: f32, z: f32| course.as_ref().map_or(0.0, |c| c.0.track.lift(c.0.track.locate(x, z).along).0);
    let mut wanted: [Vec<Transform>; 5] = default();
    let crates = &view.paddock.crates;
    looks.broken.resize(crates.len(), f32::NEG_INFINITY);
    looks.standing.resize(crates.len(), false);
    let mut bursts = Vec::new();
    for (i, c) in crates.iter().enumerate() {
        let trusted = t - looks.broken[i] < TRUST;
        // Broken by the player's kart, as drawn: at once.
        let at = Vec3::new(c.x, road(c.x, c.z) + CRATE / 2.0 + 0.15, c.z);
        if c.standing && !trusted && view.me.is_some_and(|me| touches(&me, c.x, c.z, CRATE_REACH)) {
            looks.broken[i] = t;
            bursts.push(at);
        } else if looks.standing[i] && !c.standing && !trusted {
            // Broken by another kart, as the world says.
            looks.broken[i] = t;
            bursts.push(at);
        }
        looks.standing[i] = c.standing;
    }
    for at in bursts {
        burst(&mut commands, &mut looks, at);
    }
    for (i, c) in crates.iter().enumerate().filter(|&(i, c)| c.standing && t - looks.broken[i] >= TRUST) {
        let bob = 0.08 * (t * 2.0 + i as f32).sin();
        wanted[0].push(
            Transform::from_xyz(c.x, road(c.x, c.z) + CRATE / 2.0 + 0.15 + bob, c.z)
                .with_rotation(Quat::from_rotation_y(t * 0.9 + i as f32) * Quat::from_rotation_x(0.2)),
        );
    }
    // The player's kart is drawn where it will be once the world has its
    // last steps: things are drawn as far ahead, or a tyre just thrown
    // would show behind the kart that threw it, and run through it.
    let lead = view.lead;
    // The player's own, drawn at once: each until the world's of its kind
    // arrives where it is, or a while (D-071).
    let seconds = time.delta_secs();
    let world: Vec<(proto::Item, f32, f32, f32)> = view
        .paddock
        .things
        .iter()
        .map(|t| {
            let speed = t.vx.hypot(t.vz).max(1e-3);
            let ahead = (speed * lead - 0.5 * ROLLING * lead * lead).max(0.0) / speed;
            (t.kind(), t.x + t.vx * ahead, t.z + t.vz * ahead, t.life)
        })
        .collect();
    let mut mine = std::mem::take(&mut view.mine);
    mine.retain_mut(|p| {
        p.age += seconds;
        let (x, z) = provisional_at(p);
        let kind = match p.kind {
            osage_kart_items::Kind::Tyre => proto::Item::Tyre,
            osage_kart_items::Kind::Cone => proto::Item::Cone,
            osage_kart_items::Kind::Oil => proto::Item::Oil,
            osage_kart_items::Kind::Drone => proto::Item::Drone,
        };
        let confirmed = world.iter().any(|(k, wx, wz, _)| *k == kind && (wx - x).hypot(wz - z) < SAME);
        p.age < UNCONFIRMED && !confirmed
    });
    let provisional: Vec<proto::Thing> = mine
        .iter()
        .map(|p| {
            let (x, z) = provisional_at(p);
            let (kind, life) = match p.kind {
                osage_kart_items::Kind::Tyre => (proto::Item::Tyre, TYRE_LIFE),
                osage_kart_items::Kind::Cone => (proto::Item::Cone, CONE_LIFE),
                _ => (proto::Item::Oil, OIL_LIFE),
            };
            let speed = p.vx.hypot(p.vz).max(1e-3);
            let kept = (speed - ROLLING * p.age).max(0.0) / speed;
            proto::Thing { kind: kind.into(), x, z, vx: p.vx * kept, vz: p.vz * kept, life: life - p.age, height: 0.0 }
        })
        .collect();
    view.mine = mine;
    // The world's things drawn `lead` ahead; the player's own, now.
    let things = view.paddock.things.iter().map(|t| (t, lead)).chain(provisional.iter().map(|t| (t, 0.0)));
    for (thing, lead) in things {
        match thing.kind() {
            proto::Item::Tyre => {
                let speed = thing.vx.hypot(thing.vz);
                let heading = thing.vx.atan2(thing.vz);
                // Where it rolls in `lead`, slowing (barriers aside).
                let ahead = (speed * lead - 0.5 * ROLLING * lead * lead).max(0.0) / speed.max(1e-3);
                let (x, z) = (thing.x + thing.vx * ahead, thing.z + thing.vz * ahead);
                // Turned as far as it has rolled, about.
                let roll = (TYRE_LIFE - thing.life + lead) * speed / TYRE_RADIUS;
                let turn =
                    Quat::from_rotation_y(heading) * Quat::from_rotation_z(FRAC_PI_2) * Quat::from_rotation_y(roll);
                wanted[1].push(Transform::from_xyz(x, road(x, z) + TYRE_RADIUS, z).with_rotation(turn));
            }
            proto::Item::Cone => {
                // Tossed over the kart's tail: up and over, tumbling, then standing.
                let age = (CONE_LIFE - thing.life + lead) / TOSS;
                let place = if age < 1.0 {
                    let height = 1.1 + 1.6 * age * (1.0 - age) - 0.8 * age;
                    Transform::from_xyz(thing.x, road(thing.x, thing.z) + height, thing.z)
                        .with_rotation(Quat::from_rotation_x((1.0 - age) * 5.0) * Quat::from_rotation_z(age * 2.0))
                } else {
                    Transform::from_xyz(thing.x, road(thing.x, thing.z) + 0.3, thing.z)
                };
                wanted[2].push(place);
            }
            proto::Item::Oil => {
                let spread = ((OIL_LIFE - thing.life + lead) / SPREAD).clamp(0.15, 1.0);
                wanted[3].push(
                    Transform::from_xyz(thing.x, road(thing.x, thing.z) + 0.045, thing.z)
                        .with_rotation(Quat::from_rotation_x(-FRAC_PI_2))
                        .with_scale(Vec3::splat(thing.life.clamp(0.2, 1.0) * spread)),
                )
            }
            proto::Item::Drone => {
                // Where it flies in `lead`, nose to where it goes, rocking a little.
                let (x, z) = (thing.x + thing.vx * lead, thing.z + thing.vz * lead);
                let heading = thing.vx.atan2(thing.vz);
                let rock = 0.08 * (t * 7.0 + thing.x).sin();
                wanted[4].push(
                    Transform::from_xyz(x, road(x, z) + thing.height, z)
                        .with_rotation(Quat::from_rotation_y(heading) * Quat::from_rotation_x(0.18 + rock)),
                );
            }
            _ => {}
        }
    }
    // A drone gone near the road came down on a kart: a burst there.
    let drones: Vec<Vec3> = wanted[4].iter().map(|place| place.translation).collect();
    let landed: Vec<Vec3> = looks
        .drones
        .iter()
        .filter(|was| was.y - road(was.x, was.z) < 2.0 && !drones.iter().any(|now| now.distance(**was) < 3.0))
        .copied()
        .collect();
    for at in landed {
        burst(&mut commands, &mut looks, at);
    }
    looks.drones = drones;
    let Looks { looks, pools, .. } = &mut *looks;
    for (k, _) in SHAPES.iter().enumerate() {
        let pool = &mut pools[k];
        for (n, place) in wanted[k].iter().enumerate() {
            match pool.get(n).and_then(|&e| models.get_mut(e).ok()) {
                Some((mut transform, mut visibility)) => {
                    *transform = *place;
                    visibility.set_if_neq(Visibility::Inherited);
                }
                None if n >= pool.len() => {
                    let (mesh, material) = looks[k].clone();
                    let model = commands.spawn((Mesh3d(mesh), MeshMaterial3d(material), *place)).id();
                    // A drone buzzes where it flies (where the game has sound).
                    #[cfg(feature = "sound")]
                    if SHAPES[k] == Shape::Drone {
                        commands.entity(model).insert(crate::sound::Drone);
                    }
                    pool.push(model);
                }
                None => {}
            }
        }
        for &e in pool.iter().skip(wanted[k].len()) {
            if let Ok((_, mut visibility)) = models.get_mut(e) {
                visibility.set_if_neq(Visibility::Hidden);
            }
        }
    }
}

/// Where a thing let go of is now: a tyre rolling on, slowing.
fn provisional_at(p: &Provisional) -> (f32, f32) {
    let speed = p.vx.hypot(p.vz);
    if speed < 1e-3 {
        return (p.x, p.z);
    }
    let rolled = (speed * p.age - 0.5 * ROLLING * p.age * p.age).max(0.0);
    (p.x + p.vx / speed * rolled, p.z + p.vz / speed * rolled)
}

/// A crate bursts into confetti at `at`.
fn burst(commands: &mut Commands, looks: &mut Looks, at: Vec3) {
    looks.bursts = looks.bursts.wrapping_add(1);
    let seed = looks.bursts.wrapping_mul(0x9e37_79b9);
    for n in 0..BURST {
        let entity = match looks.idle.pop() {
            Some(entity) => entity,
            None if looks.flying.len() < CONFETTI => {
                let colour = looks.colours[n % looks.colours.len()].clone();
                commands
                    .spawn((
                        Mesh3d(looks.bit.clone()),
                        MeshMaterial3d(colour),
                        Transform::from_translation(at),
                        Visibility::Hidden,
                        NotShadowCaster,
                        NotShadowReceiver,
                    ))
                    .id()
            }
            None => return,
        };
        let u = |k: u32| unit(seed ^ (n as u32).wrapping_mul(0x85eb_ca6b) ^ k.wrapping_mul(0xc2b2_ae35));
        let around = u(1) * std::f32::consts::TAU;
        let out = 1.5 + 3.0 * u(2);
        let velocity = Vec3::new(around.cos() * out, 3.5 + 4.0 * u(3), around.sin() * out);
        let spin = Vec3::new(u(4) - 0.5, u(5) - 0.5, u(6) - 0.5) * 30.0;
        commands.entity(entity).insert((Transform::from_translation(at), Visibility::Inherited));
        looks.flying.push((entity, Bit { velocity, spin, life: FLIGHT * (0.7 + 0.3 * u(7)) }));
    }
}

/// Moves the confetti: thrown up, slowed by the air, falling, turning.
fn confetti(time: Res<Time>, mut looks: ResMut<Looks>, mut bits: Query<(&mut Transform, &mut Visibility)>) {
    let seconds = time.delta_secs();
    let Looks { flying, idle, .. } = &mut *looks;
    flying.retain_mut(|(entity, bit)| {
        bit.life -= seconds;
        let Ok((mut place, mut shown)) = bits.get_mut(*entity) else { return false };
        if bit.life <= 0.0 {
            *shown = Visibility::Hidden;
            idle.push(*entity);
            return false;
        }
        bit.velocity.y -= 9.0 * seconds;
        bit.velocity *= (-2.2 * seconds).exp();
        place.translation += bit.velocity * seconds;
        place.rotate(Quat::from_euler(EulerRot::XYZ, bit.spin.x * seconds, bit.spin.y * seconds, bit.spin.z * seconds));
        place.scale = Vec3::splat((bit.life / 0.3).min(1.0));
        true
    });
}

/// A number in [0, 1) from a hash: the same confetti on every run.
fn unit(mut x: u32) -> f32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 24) as f32
}
