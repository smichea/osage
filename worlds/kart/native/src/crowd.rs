// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The crowd follows the race (D-103). Every spectator in the grandstand
//! and along the fences is a figure of its own, built twice — calm, and
//! cheering with an arm up — and moved here, from the karts' poses alone:
//!
//! People are not a flock: every spectator has their own temper — how
//! quickly they react (a fifth of a second to a second and a half), how
//! much it takes to excite them, how long they cheer, which kart they
//! follow, how fast and how far they turn, how they bounce and sway.
//!
//! - **Interest.** What a spectator sees: each kart near (within 35 m,
//!   most within 6), the more the faster it goes, and far more when it
//!   spins round or flies over the crest. Excitement rises towards it at
//!   their pace and falls slowly: a kart passing sends a ragged wave along
//!   the stand, not a drill.
//! - **Eyes.** Most turn towards the nearest kart within 45 m, some
//!   towards the one behind it; each as far as their own neck allows, a
//!   little off, at their own speed, and back when it has gone.
//! - **Cheering.** Past their own threshold they cheer, each their own
//!   way (the scenery decides): standing up to wave, standing up only to
//!   see, waving from the seat — or not at all — and they keep it up for
//!   a while of their own before sitting back.
//! - **Idle.** Nobody stands still: a slow sway, each in their own time.

use std::f32::consts::TAU;

use bevy::{light::NotShadowCaster, prelude::*};
use osage_kart_physics::MAX_SPEED;

use crate::scene::{Part, Pose, Stage};

/// A figure gathered by the scenery, before it is spawned.
pub struct Spectator {
    pub place: Transform,
    pub seated: bool,
    pub calm: Mesh,
    /// How they cheer; none for those who never stir.
    pub cheer: Option<Mesh>,
    /// Seated, they cheer standing: they get up to cheer and sit back after.
    pub stands: bool,
}

/// A spectator in the world: where they stand at rest, their two figures,
/// their own temper, and how excited they are now.
#[derive(Component)]
pub struct Watcher {
    rest: Transform,
    seated: bool,
    calm: Handle<Mesh>,
    cheer: Option<Handle<Mesh>>,
    /// Their number in the crowd, which their temper is drawn from.
    k: usize,
    /// Their own time, 0..1: no two sway or jump together.
    phase: f32,
    /// How excited they must be to cheer.
    threshold: f32,
    /// How fast their excitement follows what they see, per second.
    pace: f32,
    excitement: f32,
    /// How far they have turned from rest, rad; how fast and how far they
    /// turn; how far off the kart they look.
    turn: f32,
    turning: f32,
    reach: f32,
    off: f32,
    /// Which kart they follow: 0 the nearest, 1 the one after.
    follows: usize,
    /// Their bounce and sway: rad/s and metres or radians.
    bounce: (f32, f32),
    sway: (f32, f32),
    /// How long they cheer at least once they start, s, and since when.
    keeps: f32,
    since: f32,
    cheering: bool,
    /// Getting up and sitting down: 0 seated, 1 up; and how fast each
    /// goes, per second.
    stands: bool,
    rising: f32,
    rates: (f32, f32),
}

/// How far through getting up the standing figure takes over.
const HALFWAY: f32 = 0.55;

pub struct CrowdPlugin;

impl Plugin for CrowdPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CrowdSeed>().add_systems(Update, ((temper, react).chain(), wave).in_set(Stage::Draw));
    }
}

/// A repeatable number in 0..1 from `k` and a salt.
fn hashed(k: usize, salt: u32) -> f32 {
    let mut x = (k as u32).wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65535.0
}

pub fn spawn(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    spectator: Spectator,
    cloth: &Handle<StandardMaterial>,
    k: usize,
) {
    let calm = meshes.add(spectator.calm);
    let cheer = spectator.cheer.map(|mesh| meshes.add(mesh));
    let mut watcher = Watcher {
        rest: spectator.place,
        seated: spectator.seated,
        calm: calm.clone(),
        cheer,
        k,
        phase: 0.0,
        threshold: 0.0,
        pace: 0.0,
        excitement: 0.0,
        turn: 0.0,
        turning: 0.0,
        reach: 0.0,
        off: 0.0,
        follows: 0,
        bounce: (0.0, 0.0),
        sway: (0.0, 0.0),
        keeps: 0.0,
        since: 0.0,
        cheering: false,
        stands: spectator.stands,
        rising: 0.0,
        rates: (0.0, 0.0),
    };
    watcher.temper(0);
    let mut entity = commands.spawn((Part, Mesh3d(calm), MeshMaterial3d(cloth.clone()), spectator.place, watcher));
    // The grandstand's crowd sits under a roof: no shadows of its own.
    if spectator.seated {
        entity.insert(NotShadowCaster);
    }
}

/// The seed of the crowd's tempers: drawn from the clock for a game, so
/// that no two races' crowds behave alike; 0 on the bench, whose frames
/// must be the same on every run.
#[derive(Resource)]
pub struct CrowdSeed(pub u32);

impl Default for CrowdSeed {
    fn default() -> Self {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        Self(now.subsec_nanos() | 1)
    }
}

impl Watcher {
    /// Draws this spectator's temper from their number and `seed`.
    fn temper(&mut self, seed: u32) {
        let k = self.k;
        let roll = |salt: u32| hashed(k, salt ^ seed.wrapping_mul(0x27D4_EB2F));
        self.phase = roll(1);
        self.threshold = 0.25 + 0.75 * roll(2);
        // A reaction time of 0.2 to 1.5 s.
        self.pace = 1.0 / (0.2 + 1.3 * roll(3));
        self.turning = 0.8 + 3.2 * roll(4);
        self.reach = 0.35 + 0.55 * roll(5);
        self.off = (roll(6) - 0.5) * 0.5;
        self.follows = usize::from(roll(7) < 0.25);
        self.bounce = (4.5 + 4.5 * roll(8), 0.02 + 0.05 * roll(9));
        self.sway = (0.4 + 0.7 * roll(10), 0.01 + 0.03 * roll(11));
        self.keeps = 0.8 + 3.0 * roll(12);
        // Up in 0.45 to 0.8 s; down, more slowly, in 0.9 to 1.6 s.
        self.rates = (1.0 / (0.45 + 0.35 * roll(13)), 1.0 / (0.9 + 0.7 * roll(14)));
    }
}

/// A crowd just built takes the tempers of this race; the next build
/// (the next race's track) the next seed's.
fn temper(mut seed: ResMut<CrowdSeed>, mut added: Query<&mut Watcher, Added<Watcher>>) {
    let mut any = false;
    for mut watcher in &mut added {
        watcher.temper(seed.0);
        any = true;
    }
    if any && seed.0 != 0 {
        seed.0 = seed.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223) | 1;
    }
}

/// How much a kart at `distance` m interests a spectator, 0 to about 2.
fn interest(distance: f32, speed: f32, spin: f32, flying: bool) -> f32 {
    let near = 1.0 - ((distance - 6.0) / 29.0).clamp(0.0, 1.0);
    let near = near * near;
    let pace = (speed.abs() / MAX_SPEED).clamp(0.0, 1.0);
    let drama = if spin.abs() > 2.0 { 1.2 } else { 0.0 } + if flying { 0.6 } else { 0.0 };
    near * (0.35 + 0.65 * pace + drama)
}

fn react(time: Res<Time>, karts: Query<&Pose>, mut watchers: Query<(&mut Watcher, &mut Transform, &mut Mesh3d)>) {
    let seconds = time.delta_secs().min(0.1);
    let t = time.elapsed_secs();
    let karts: Vec<(Vec2, f32, f32, bool)> = karts
        .iter()
        .filter(|pose| !pose.ghost)
        .map(|pose| {
            let k = &pose.kart;
            (Vec2::new(k.x, k.z), k.speed, k.spin, k.climb.abs() > 2.0)
        })
        .collect();
    for (mut watcher, mut transform, mut mesh) in &mut watchers {
        let at = Vec2::new(watcher.rest.translation.x, watcher.rest.translation.z);
        let mut seen = 0.0f32;
        let mut near: Vec<(f32, Vec2)> = Vec::new();
        for &(kart, speed, spin, flying) in &karts {
            let distance = kart.distance(at);
            seen += interest(distance, speed, spin, flying);
            if distance < 45.0 {
                near.push((distance, kart));
            }
        }
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        let nearest = near.get(watcher.follows).or(near.first()).copied();
        let seen = seen.min(1.5);
        // Up fast, down slowly: a cheer lingers after the kart has gone.
        let rate = if seen > watcher.excitement { watcher.pace } else { watcher.pace * 0.2 };
        watcher.excitement += (seen - watcher.excitement) * (seconds * rate).min(1.0);

        // Eyes on the nearest kart, within what a neck and shoulders allow.
        let facing = watcher.rest.rotation * Vec3::X;
        let wanted = match nearest {
            Some((_, kart)) => {
                let to = kart - at;
                let ahead = Vec2::new(facing.x, facing.z);
                let angle = ahead.angle_to(to);
                // A figure's turn about +Y goes the other way round from the ground's angle.
                (watcher.off - angle).clamp(-watcher.reach, watcher.reach)
            }
            None => 0.0,
        };
        watcher.turn += (wanted - watcher.turn) * (seconds * watcher.turning).min(1.0);

        // Once up, each stays up a while of their own; then sits back when
        // the excitement has gone down well below what got them up.
        watcher.since += seconds;
        let cheering = if watcher.cheering {
            watcher.since < watcher.keeps || watcher.excitement > watcher.threshold * 0.6
        } else {
            watcher.excitement > watcher.threshold
        };
        if cheering != watcher.cheering && watcher.cheer.is_some() {
            watcher.cheering = cheering;
            watcher.since = 0.0;
        }
        let cheering = watcher.cheering;
        let Some(cheer) = watcher.cheer.clone() else { continue };
        // Getting up or sitting down takes a moment: the seated figure
        // lifts and leans forward off the seat; past halfway the standing
        // one takes over, knees still bent, and straightens. Sitting down
        // is the same backwards, more slowly.
        let (mut lift, mut lean, wanted) = (0.0, 0.0, if cheering { cheer.clone() } else { watcher.calm.clone() });
        let wanted = if watcher.stands {
            let (up, down) = watcher.rates;
            let goal = if cheering { 1.0 } else { 0.0 };
            let rate = if goal > watcher.rising { up } else { down };
            watcher.rising = (watcher.rising + (goal - watcher.rising).signum() * rate * seconds).clamp(0.0, 1.0);
            let r = watcher.rising;
            if r >= HALFWAY {
                let bent = (1.0 - r) / (1.0 - HALFWAY);
                (lift, lean) = (-0.22 * bent, 0.25 * bent);
                cheer
            } else {
                let off = r / HALFWAY;
                (lift, lean) = (0.22 * off, 0.3 * off);
                watcher.calm.clone()
            }
        } else {
            wanted
        };
        if mesh.0 != wanted {
            mesh.0 = wanted;
        }
        let (rate, height) = watcher.bounce;
        let beat = (t * rate + watcher.phase * TAU).sin().max(0.0);
        lift += match (cheering, watcher.seated) {
            // Up (most cheering figures stand), bouncing on the toes, once up.
            (true, true) if !watcher.stands || watcher.rising >= 1.0 => height * 0.6 * beat,
            // A jump on the spot.
            (true, false) => height * 1.6 * beat * watcher.excitement.min(1.0),
            _ => 0.0,
        };
        let (rate, size) = watcher.sway;
        let sway = size * (t * rate + watcher.phase * TAU).sin();
        let rest = watcher.rest;
        *transform = Transform {
            translation: rest.translation + Vec3::Y * lift,
            // Leaning forward is about -Z: the figure faces +X.
            rotation: rest.rotation
                * Quat::from_rotation_y(watcher.turn)
                * Quat::from_rotation_x(sway)
                * Quat::from_rotation_z(-lean),
            scale: rest.scale,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kart_close_and_fast_interests_more_than_one_far_or_slow_and_a_spin_most() {
        let close_fast = interest(5.0, MAX_SPEED, 0.0, false);
        assert!(close_fast > interest(5.0, 2.0, 0.0, false));
        assert!(close_fast > interest(30.0, MAX_SPEED, 0.0, false));
        assert_eq!(interest(40.0, MAX_SPEED, 0.0, false), 0.0, "out of sight");
        assert!(interest(10.0, 5.0, 4.0, false) > close_fast * 0.8, "a spin draws every eye");
    }

    #[test]
    fn tempers_differ_from_one_spectator_to_the_next() {
        let thresholds: Vec<f32> = (0..20).map(|k| hashed(k, 2)).collect();
        let spread = thresholds.iter().cloned().fold(0.0, f32::max) - thresholds.iter().cloned().fold(1.0, f32::min);
        assert!(spread > 0.5, "{thresholds:?}");
    }
}

/// A marshal at a post, gathered by the scenery: the figure, the staff in
/// the hand's own space (from its foot to its top), and where the hand is
/// (D-109).
pub struct Flagman {
    pub place: Transform,
    pub figure: Mesh,
    pub staff: Mesh,
    pub foot: Vec3,
    pub top: Vec3,
    pub hand: Vec3,
}

/// The flag's cloth: its size, m, and how finely it is cut to move.
const CLOTH: (f32, f32) = (0.55, 0.42);
const CLOTH_CELLS: (usize, usize) = (10, 6);

/// A marshal in the world: their flag (the hand's pivot and the cloth's
/// mesh), how hard they wave it now, their own rhythm.
#[derive(Component)]
pub struct Marshal {
    pivot: Entity,
    cloth: Handle<Mesh>,
    /// The staff's top and its direction, and across it, in the hand's space.
    top: Vec3,
    along: Vec3,
    across: Vec3,
    at: Vec2,
    waving: f32,
    rate: f32,
    phase: f32,
}

impl Marshal {
    /// Where the cloth's point (`u` out from the staff, `v` down it, both
    /// 0..1) is at time `t`: a travelling wave out along the cloth, its
    /// height growing away from the staff, a breeze at rest and a flutter
    /// when waved; the cloth sags a little at its free end.
    fn cloth_point(&self, u: f32, v: f32, t: f32) -> Vec3 {
        let normal = self.across.cross(self.along).normalize_or(Vec3::Z);
        let flutter = 0.02 + 0.09 * self.waving.min(1.4);
        let speed = 6.0 + 6.0 * self.waving.min(1.0);
        let wave = flutter * u * (7.0 * u - speed * t + self.phase + 1.3 * v).sin();
        let sag = 0.05 * u * u * (1.0 - 0.6 * self.waving.min(1.0));
        self.top + self.across * (CLOTH.0 * u) - self.along * (CLOTH.1 * v + sag) + normal * wave
    }
}

/// The cloth as a grid of `CLOTH_CELLS`, its vertices placed by `point`.
fn cloth_mesh(point: impl Fn(f32, f32) -> Vec3) -> Mesh {
    let (nu, nv) = CLOTH_CELLS;
    let mut positions = Vec::new();
    for j in 0..=nv {
        for i in 0..=nu {
            positions.push(point(i as f32 / nu as f32, j as f32 / nv as f32).to_array());
        }
    }
    let mut indices = Vec::new();
    for j in 0..nv as u32 {
        for i in 0..nu as u32 {
            let a = j * (nu as u32 + 1) + i;
            let (b, c, d) = (a + 1, a + nu as u32 + 1, a + nu as u32 + 2);
            indices.extend([a, c, b, b, c, d]);
        }
    }
    let count = positions.len();
    let mut mesh = Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.93f32, 0.64, 0.004, 1.0]; count])
        .with_inserted_indices(bevy::mesh::Indices::U32(indices));
    mesh.compute_smooth_normals();
    mesh
}

pub fn spawn_marshal(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    marshal: Flagman,
    [cloth, flag_material]: [&Handle<StandardMaterial>; 2],
    k: usize,
) {
    let along = (marshal.top - marshal.foot).normalize_or(Vec3::Y);
    // Out from the staff towards the track the marshal faces (+X), square to the staff.
    let across = (Vec3::X - along * along.dot(Vec3::X)).normalize_or(Vec3::X);
    let at = Vec2::new(marshal.place.translation.x, marshal.place.translation.z);
    debug!(?at, height = marshal.place.translation.y, "a marshal");
    let mut state = Marshal {
        pivot: Entity::PLACEHOLDER,
        cloth: Handle::default(),
        top: marshal.top,
        along,
        across,
        at,
        waving: 0.0,
        rate: 5.0 + 2.5 * hashed(k, 31),
        phase: hashed(k, 32) * TAU,
    };
    let fabric = cloth_mesh(|u, v| state.cloth_point(u, v, 0.0));
    state.cloth = meshes.add(fabric);
    let staff =
        commands.spawn((Mesh3d(meshes.add(marshal.staff)), MeshMaterial3d(cloth.clone()), Transform::IDENTITY)).id();
    let fabric =
        commands.spawn((Mesh3d(state.cloth.clone()), MeshMaterial3d(flag_material.clone()), Transform::IDENTITY)).id();
    let pivot = commands.spawn(Transform::from_translation(marshal.hand)).add_children(&[staff, fabric]).id();
    state.pivot = pivot;
    commands
        .spawn((Part, Mesh3d(meshes.add(marshal.figure)), MeshMaterial3d(cloth.clone()), marshal.place, state))
        .add_child(pivot);
}

/// Marshals wave their flags at karts coming by: side to side, the staff
/// pivoting in the hand, harder at a kart spinning round, easing off as
/// the karts go; the cloth flutters as it is waved, and stirs in a breeze
/// at rest.
fn wave(
    time: Res<Time>,
    karts: Query<&Pose>,
    mut marshals: Query<&mut Marshal>,
    mut pivots: Query<&mut Transform, Without<Marshal>>,
    cameras: Query<&GlobalTransform, With<osage_render::OsageCamera>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let (t, seconds) = (time.elapsed_secs(), time.delta_secs().min(0.1));
    for mut marshal in &mut marshals {
        let wanted = karts
            .iter()
            .filter(|pose| !pose.ghost)
            .map(|pose| {
                let d = Vec2::new(pose.kart.x, pose.kart.z).distance(marshal.at);
                let near = 1.0 - ((d - 10.0) / 35.0).clamp(0.0, 1.0);
                near * if pose.kart.spin.abs() > 2.0 { 1.6 } else { 1.0 }
            })
            .fold(0.0f32, f32::max)
            .min(1.6);
        let ease = if wanted > marshal.waving { 4.0 } else { 0.8 };
        marshal.waving += (wanted - marshal.waving) * (seconds * ease).min(1.0);
        if let Ok(mut pivot) = pivots.get_mut(marshal.pivot) {
            // Facing +X, waving side to side is a turn about X.
            let swing = 0.65 * marshal.waving * (t * marshal.rate + marshal.phase).sin();
            pivot.rotation = Quat::from_rotation_x(swing);
        }
        // The cloth is cut again only where a camera is near enough to see
        // it move: a mesh changed is a mesh sent to the GPU again.
        let seen = cameras.iter().any(|camera| {
            let at = camera.translation();
            Vec2::new(at.x, at.z).distance(marshal.at) < 120.0
        });
        if seen && let Some(mut mesh) = meshes.get_mut(&marshal.cloth) {
            let fabric = cloth_mesh(|u, v| marshal.cloth_point(u, v, t));
            *mesh = fabric;
        }
    }
}
