// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The karts: a racing kart and its driver to scale (`osage-kart-model`),
//! and the motion that makes it feel alive — wheels that spin and steer, a
//! body that leans a little in bends, lifts its nose when it accelerates and
//! hops into a drift, a driver who looks into the turn.
//!
//! Each kart is a body, a helmet and four wheels, each a few meshes, one per
//! finish; colours are in the vertices, so that all karts share one material
//! per finish.
//!
//! A kart faces +Z, stands on the ground at its origin, and has its left
//! at +X.

use std::{
    collections::HashMap,
    f32::consts::{PI, TAU},
};

use bevy::{
    camera::visibility::NoFrustumCulling,
    mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes},
    prelude::*,
};
use osage_kart_model::{AXLES, DRIVER_BONES, Finish, Livery, NECK, WHEELS, driver_pose, driver_rest, steering_column};
pub use osage_kart_model::{EXHAUSTS, REAR_CONTACTS};
use osage_kart_physics::{MAX_SPEED, wrap};

use crate::scene::{PALETTE, Pose};

/// A hop into a drift lasts this long, seconds.
const HOP: f32 = 0.28;

/// The meshes of a part, each with its finish.
type Meshes = Vec<(Finish, Handle<Mesh>)>;

/// The meshes of one colour of kart.
struct Model {
    body: Meshes,
    /// The driver below the neck, skinned to the bones of [`Bones`].
    driver: Meshes,
    steering: Meshes,
    helmet: Meshes,
    wheels: [Meshes; 2],
}

/// A kart's moving parts that steering moves: the steering wheel and the
/// driver's bones, which the arms follow.
pub struct Bones {
    steering: Entity,
    joints: [Entity; DRIVER_BONES],
}

/// What the karts share: a material per finish, and a model per colour,
/// made the first time a kart of that colour comes.
#[derive(Resource)]
pub struct Kit {
    finishes: HashMap<Finish, Handle<StandardMaterial>>,
    models: HashMap<usize, Model>,
    /// The driver's bones at rest, inverted: every driver's skin.
    bindposes: Handle<SkinnedMeshInverseBindposes>,
}

/// The moving parts of a kart and where its motion stands.
#[derive(Component)]
pub struct Rig {
    body: Entity,
    head: Entity,
    bones: Bones,
    /// Each wheel's pivot (the front ones turn to steer) and its tyre (which spins).
    wheels: [(Entity, Entity); 4],
    heading: f32,
    speed: f32,
    steer: f32,
    spin: f32,
    slip: f32,
    lean: f32,
    pitch: f32,
    /// Seconds left of the hop into a drift.
    hop: f32,
    drifting: bool,
}

pub fn setup(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut skins: ResMut<Assets<SkinnedMeshInverseBindposes>>,
) {
    let stickers = images.add(osage_kart_model::decals::atlas());
    let finishes = Finish::ALL
        .into_iter()
        .map(|finish| {
            let mut material = finish.material();
            if finish == Finish::Decal {
                material.base_color_texture = Some(stickers.clone());
            }
            (finish, materials.add(material))
        })
        .collect();
    let rest: Vec<Mat4> = driver_rest().iter().map(|bone| bone.to_matrix().inverse()).collect();
    commands.insert_resource(Kit { finishes, models: HashMap::new(), bindposes: skins.add(rest) });
}

/// A team's colours from the palette: the paint, white or black stripes
/// against it, a frame in silver or in the paint's darker shade.
fn livery(colour: usize) -> Livery {
    let (r, g, b) = PALETTE[colour];
    // Racing paint is deeper than the browser client's palette: more saturated, a little darker.
    let base = Vec3::new(r, g, b);
    let grey = base.dot(Vec3::new(0.2126, 0.7152, 0.0722));
    let main = (Vec3::splat(grey).lerp(base, 1.35) * 0.88).clamp(Vec3::ZERO, Vec3::ONE);
    let light = 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.6;
    let second = if light { Vec3::splat(0.12) } else { Vec3::splat(0.92) };
    let frame = if colour.is_multiple_of(2) { Vec3::new(0.8, 0.81, 0.83) } else { main * 0.7 };
    // Each colour has its number, as a team keeps it season after season.
    let number = [7, 12, 23, 31, 44, 58, 66, 89][colour % 8];
    // Suits: anthracite, white, or a deep shade of the team's colour.
    let suit = match colour % 3 {
        0 => Vec3::new(0.13, 0.13, 0.15),
        1 => Vec3::new(0.88, 0.88, 0.86),
        _ => main * 0.45,
    };
    Livery { main, second, frame, suit, number }
}

/// Children with the meshes of `part`, each in its finish's material.
fn spawn_part(commands: &mut Commands, kit: &Kit, part: &Meshes, parent: Entity) {
    for (finish, mesh) in part {
        commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(kit.finishes[finish].clone()),
            Transform::default(),
            ChildOf(parent),
        ));
    }
}

/// A kart standing still where the scenery puts it, out of the race: in the
/// pit lane, its driver waiting in it.
#[derive(Component)]
pub struct Parked {
    pub colour: usize,
}

/// The model of a colour of kart, made the first time it is needed.
fn model_of(kit: &mut Kit, meshes: &mut Assets<Mesh>, colour: usize) {
    kit.models.entry(colour).or_insert_with(|| {
        let built = osage_kart_model::build(livery(colour));
        let mut add = |part: Vec<(Finish, Mesh)>| -> Meshes {
            part.into_iter().map(|(finish, mesh)| (finish, meshes.add(mesh))).collect()
        };
        let [front, rear] = built.wheels;
        Model {
            body: add(built.body),
            driver: add(built.driver),
            steering: add(built.steering),
            helmet: add(built.helmet),
            wheels: [add(front), add(rear)],
        }
    });
}

/// A kart's parts under `kart`: its body, its head, and each wheel's pivot and tyre.
fn assemble(
    commands: &mut Commands,
    kit: &Kit,
    colour: usize,
    kart: Entity,
) -> (Entity, Entity, [(Entity, Entity); 4], Bones) {
    let model = &kit.models[&colour];
    let body = commands.spawn((Transform::default(), Visibility::default(), ChildOf(kart))).id();
    spawn_part(commands, kit, &model.body, body);
    // The steering wheel turns about its column; the driver's bones follow it.
    let steering = commands.spawn((steering_column(), Visibility::default(), ChildOf(body))).id();
    spawn_part(commands, kit, &model.steering, steering);
    let joints = driver_rest().map(|rest| commands.spawn((rest, ChildOf(body))).id());
    for (finish, mesh) in &model.driver {
        commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(kit.finishes[finish].clone()),
            SkinnedMesh { inverse_bindposes: kit.bindposes.clone(), joints: joints.to_vec() },
            // Its bounds move with its bones: never culled by stale ones.
            NoFrustumCulling,
            Transform::default(),
            ChildOf(body),
        ));
    }
    let head = commands.spawn((Transform::from_translation(NECK), Visibility::default(), ChildOf(body))).id();
    spawn_part(commands, kit, &model.helmet, head);
    let wheels = [0, 1, 2, 3].map(|i| {
        let pivot = commands.spawn((Transform::from_translation(AXLES[i]), Visibility::default(), ChildOf(body))).id();
        // Wheels on the right are the left ones turned round.
        let facing = if AXLES[i].x < 0.0 { Quat::from_rotation_y(PI) } else { Quat::IDENTITY };
        let tyre = commands.spawn((Transform::from_rotation(facing), Visibility::default(), ChildOf(pivot))).id();
        spawn_part(commands, kit, &model.wheels[i / 2], tyre);
        (pivot, tyre)
    });
    (body, head, wheels, Bones { steering, joints })
}

/// Gives every parked kart its model.
pub fn park(
    mut commands: Commands,
    mut kit: ResMut<Kit>,
    mut meshes: ResMut<Assets<Mesh>>,
    parked: Query<(Entity, &Parked), Added<Parked>>,
) {
    for (kart, parked) in &parked {
        let colour = parked.colour % PALETTE.len();
        model_of(&mut kit, &mut meshes, colour);
        assemble(&mut commands, &kit, colour, kart);
        commands.entity(kart).insert(Visibility::default());
    }
}

/// Gives every new kart its model.
pub fn dress(
    mut commands: Commands,
    mut kit: ResMut<Kit>,
    mut meshes: ResMut<Assets<Mesh>>,
    karts: Query<(Entity, &Pose), Added<Pose>>,
) {
    for (kart, pose) in &karts {
        let colour = pose.color as usize % PALETTE.len();
        model_of(&mut kit, &mut meshes, colour);
        let (body, head, wheels, bones) = assemble(&mut commands, &kit, colour, kart);

        commands.entity(kart).insert((
            Transform::default(),
            Visibility::default(),
            Rig {
                body,
                head,
                bones,
                wheels,
                heading: pose.kart.heading,
                speed: pose.kart.speed,
                steer: 0.0,
                spin: 0.0,
                slip: 0.0,
                lean: 0.0,
                pitch: 0.0,
                hop: 0.0,
                drifting: false,
            },
        ));
    }
}

/// Places every kart and moves its parts from how its state changes.
pub fn animate(time: Res<Time>, mut karts: Query<(Entity, &Pose, &mut Rig)>, mut transforms: Query<&mut Transform>) {
    let seconds = time.delta_secs().max(1e-4);
    let ease = |rate: f32| (seconds * rate).min(1.0);
    for (entity, pose, mut rig) in &mut karts {
        let kart = &pose.kart;
        let turning = (wrap(kart.heading - rig.heading) / seconds).clamp(-3.5, 3.5);
        let pushing = ((kart.speed - rig.speed) / seconds).clamp(-40.0, 40.0);
        (rig.heading, rig.speed) = (kart.heading, kart.speed);
        let pace = (kart.speed / MAX_SPEED).clamp(-1.0, 1.3);

        rig.steer += ((turning / 2.3).clamp(-1.0, 1.0) - rig.steer) * ease(12.0);
        rig.spin = (rig.spin + kart.speed * seconds / WHEELS[1]) % TAU;
        // A drifting kart is drawn turned into the slide; it leans out of a
        // bend and lifts its nose when it accelerates.
        rig.slip += (-kart.drift * 0.4 - rig.slip) * ease(9.0);
        rig.lean += ((turning * 0.02 * pace).clamp(-0.05, 0.05) - rig.lean) * ease(7.0);
        rig.pitch += ((-pushing * 0.0015).clamp(-0.02, 0.03) - rig.pitch) * ease(6.0);
        if kart.drift != 0.0 && !rig.drifting {
            rig.hop = HOP;
        }
        rig.drifting = kart.drift != 0.0;
        rig.hop = (rig.hop - seconds).max(0.0);
        let hop = 0.06 * (PI * rig.hop / HOP).sin();

        if let Ok(mut place) = transforms.get_mut(entity) {
            *place = Transform::from_xyz(kart.x, hop, kart.z).with_rotation(Quat::from_rotation_y(kart.heading));
        }
        if let Ok(mut body) = transforms.get_mut(rig.body) {
            body.rotation =
                Quat::from_rotation_y(rig.slip) * Quat::from_rotation_z(rig.lean) * Quat::from_rotation_x(rig.pitch);
        }
        // The driver looks into the turn and leans against the bend.
        if let Ok(mut head) = transforms.get_mut(rig.head) {
            head.rotation =
                Quat::from_rotation_y(rig.steer * 0.45 - rig.slip * 0.6) * Quat::from_rotation_z(-rig.lean * 1.5);
        }
        // The steering wheel turns twice as far as the front wheels, the
        // hands on it, the arms following.
        let turn = rig.steer * 1.1;
        if let Ok(mut wheel) = transforms.get_mut(rig.bones.steering) {
            *wheel = steering_column() * Transform::from_rotation(Quat::from_rotation_y(turn));
        }
        for (joint, bone) in rig.bones.joints.iter().zip(driver_pose(turn)) {
            if let Ok(mut place) = transforms.get_mut(*joint) {
                *place = bone;
            }
        }
        for (i, (pivot, tyre)) in rig.wheels.into_iter().enumerate() {
            if let (true, Ok(mut pivot)) = (i < 2, transforms.get_mut(pivot)) {
                pivot.rotation = Quat::from_rotation_y(rig.steer * 0.55);
            }
            if let Ok(mut tyre) = transforms.get_mut(tyre) {
                let turns = rig.spin * WHEELS[1] / WHEELS[i / 2];
                // A wheel on the right is turned round: it spins the other way about its own X.
                let facing = if AXLES[i].x < 0.0 { Quat::from_rotation_y(PI) } else { Quat::IDENTITY };
                let spin = if AXLES[i].x < 0.0 { -turns } else { turns };
                tyre.rotation = facing * Quat::from_rotation_x(spin);
            }
        }
    }
}
