// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The paddock objects as the world sends them (`kart.paddock/1`): crates
//! standing across the road, tyres rolling, cones and oil left behind. The
//! world decides what they do ([osage-kart-items](osage_kart_items)); this
//! only draws them, a pool of models per kind.

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use osage_kart_items::{CONE_LIFE, CONE_RADIUS, Item, OIL_LIFE, OIL_RADIUS, ROLLING, TYRE_LIFE, TYRE_RADIUS};
use osage_world_kart::proto;

use crate::scene::Stage;

pub struct PaddockPlugin;

impl Plugin for PaddockPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PaddockView>().add_systems(Startup, setup).add_systems(Update, show.in_set(Stage::Draw));
    }
}

/// What the world last sent of the paddock, and how far the player's kart
/// is drawn ahead of it in time (its prediction's lead, seconds).
#[derive(Resource, Default)]
pub struct PaddockView(pub proto::Paddock, pub f32);

/// The item of a protocol value.
pub fn item(item: proto::Item) -> Option<Item> {
    match item {
        proto::Item::None => None,
        proto::Item::Nitro => Some(Item::Nitro),
        proto::Item::Tyre => Some(Item::Tyre),
        proto::Item::Cone => Some(Item::Cone),
        proto::Item::Oil => Some(Item::Oil),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Crate,
    Tyre,
    Cone,
    Oil,
}

const SHAPES: [Shape; 4] = [Shape::Crate, Shape::Tyre, Shape::Cone, Shape::Oil];

/// Each kind's model, and the models made so far.
#[derive(Resource)]
struct Looks {
    looks: [(Handle<Mesh>, Handle<StandardMaterial>); 4],
    pools: [Vec<Entity>; 4],
}

const CRATE: f32 = 0.8;
/// A cone flies this long from the kart to the road, s.
const TOSS: f32 = 0.45;
/// Oil spreads to its size in this long, s.
const SPREAD: f32 = 0.35;

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let mut look = |mesh: Mesh, material: StandardMaterial| (meshes.add(mesh), materials.add(material));
    let looks = [
        // A slatted wooden crate, lit from inside: it reads from far away.
        look(
            Cuboid::from_length(CRATE).into(),
            StandardMaterial {
                base_color: Color::srgb(0.62, 0.43, 0.22),
                emissive: LinearRgba::rgb(0.9, 0.55, 0.12),
                perceptual_roughness: 0.75,
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
    ];
    commands.insert_resource(Looks { looks, pools: default() });
}

/// Places a model for each crate and thing, hides the models left over.
fn show(
    mut commands: Commands,
    time: Res<Time>,
    view: Res<PaddockView>,
    mut looks: ResMut<Looks>,
    mut models: Query<(&mut Transform, &mut Visibility)>,
) {
    let t = time.elapsed_secs();
    let mut wanted: [Vec<Transform>; 4] = default();
    for (i, c) in view.0.crates.iter().enumerate().filter(|(_, c)| c.standing) {
        let bob = 0.08 * (t * 2.0 + i as f32).sin();
        wanted[0].push(
            Transform::from_xyz(c.x, CRATE / 2.0 + 0.15 + bob, c.z)
                .with_rotation(Quat::from_rotation_y(t * 0.9 + i as f32) * Quat::from_rotation_x(0.2)),
        );
    }
    // The player's kart is drawn where it will be once the world has its
    // last steps: things are drawn as far ahead, or a tyre just thrown
    // would show behind the kart that threw it, and run through it.
    let lead = view.1;
    for thing in &view.0.things {
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
                wanted[1].push(Transform::from_xyz(x, TYRE_RADIUS, z).with_rotation(turn));
            }
            proto::Item::Cone => {
                // Tossed over the kart's tail: up and over, tumbling, then standing.
                let age = (CONE_LIFE - thing.life + lead) / TOSS;
                let place = if age < 1.0 {
                    let height = 1.1 + 1.6 * age * (1.0 - age) - 0.8 * age;
                    Transform::from_xyz(thing.x, height, thing.z)
                        .with_rotation(Quat::from_rotation_x((1.0 - age) * 5.0) * Quat::from_rotation_z(age * 2.0))
                } else {
                    Transform::from_xyz(thing.x, 0.3, thing.z)
                };
                wanted[2].push(place);
            }
            proto::Item::Oil => {
                let spread = ((OIL_LIFE - thing.life + lead) / SPREAD).clamp(0.15, 1.0);
                wanted[3].push(
                    Transform::from_xyz(thing.x, 0.045, thing.z)
                        .with_rotation(Quat::from_rotation_x(-FRAC_PI_2))
                        .with_scale(Vec3::splat(thing.life.clamp(0.2, 1.0) * spread)),
                )
            }
            _ => {}
        }
    }
    let Looks { looks, pools } = &mut *looks;
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
                    pool.push(commands.spawn((Mesh3d(mesh), MeshMaterial3d(material), *place)).id());
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
